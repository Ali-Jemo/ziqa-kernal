use std::{
    collections::{BTreeMap, VecDeque},
    io::{self, Write},
    mem,
    os::unix::io::AsRawFd,
    slice,
    str::{self, FromStr},
};

use event::{EventQueue, user_data};
#[cfg(not(feature = "ziqa-bga-direct"))]
use graphics_ipc::V2GraphicsHandle;
use inputd::{ConsumerHandle, ConsumerHandleEvent};
use log::{error, info};
use orbclient::{Color, Event, WindowDragKind, rect::Rect};
use redox_scheme::{
    CallerCtx, OpenResult, RequestKind, Response, SignalBehavior, Socket,
    scheme::{IntoTag, Op, OpRead, SchemeState, SchemeSync, register_scheme_inner},
};
use syscall::{
    EACCES, EAGAIN, EBADF, ECANCELED, EINVAL, EOPNOTSUPP, EWOULDBLOCK, flag::EventFlags,
    schemev2::NewFdFlags,
};

use crate::window::WindowId;
use crate::{core::display::Displays, scheme::OrbitalScheme};

pub(crate) mod display;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("io error")]
    IoError(#[from] io::Error),
    #[error("syscall error: {0}")]
    SyscallError(syscall::Error),
    #[error("system error")]
    LibredoxError(#[from] libredox::error::Error),
}
impl From<syscall::Error> for Error {
    fn from(err: syscall::Error) -> Self {
        Error::SyscallError(err)
    }
}

pub struct Properties<'a> {
    // TODO: avoid allocation
    pub flags: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub title: &'a str,
}

pub struct Orbital {
    pub scheme: Option<Socket>,
    pub delayed: VecDeque<(CallerCtx, OpRead)>,
    /// Handle to "/scheme/input/consumer" to receive input events.
    pub input: Option<ConsumerHandle>,
}

impl Orbital {
    /// Open an orbital display — feature-gated path:
    /// - Without "ziqa-bga-direct": Redox DRM via graphics-ipc
    /// - With "ziqa-bga-direct": ZiqaKernel direct BGA framebuffer via SYS_FMAP
    pub fn open_display() -> io::Result<(Self, Displays)> {
        #[cfg(not(feature = "ziqa-bga-direct"))]
        {
            let input_handle = ConsumerHandle::new_vt()?;
            let display = input_handle.open_display_v2().map_err(|err| {
                error!("failed to open display: {}", err);
                err
            })?;

            let scheme = Socket::nonblock().map_err(|err| {
                error!("failed to create scheme: {}", err);
                err
            })?;

            let display_handle = V2GraphicsHandle::from_file(display)?;
            let displays = Displays::new(display_handle)?;

            Ok((
                Orbital {
                    scheme: Some(scheme),
                    delayed: VecDeque::new(),
                    input: Some(input_handle),
                },
                displays,
            ))
        }

        #[cfg(feature = "ziqa-bga-direct")]
        {
            info!("open_display: using ziqa-bga-direct path");
            // Map BGA framebuffer via SYS_FMAP (kernel always returns BGA for any FMAP call)
            let width = 1280u32;
            let height = 960u32;
            let fb_size = width as usize * height as usize * 4;
            let map = syscall::data::Map {
                offset: 0,
                size: fb_size,
                flags: syscall::flag::MapFlags::MAP_SHARED,
                address: 0,
            };
            let fb_ptr = unsafe {
                syscall::fmap(0, &map).map_err(|e| {
                    error!("failed to map framebuffer: {}", e);
                    io::Error::new(io::ErrorKind::Other, "framebuffer mmap failed")
                })? as *mut u32
            };
            info!("syscall::fmap returned address=0x{:x}", fb_ptr as usize);

            let displays = Displays::from_framebuffer(fb_ptr, width, height);
            info!(
                "Displays::from_framebuffer: addr=0x{:x}, size={:.1}MB",
                fb_ptr as usize,
                fb_size as f64 / 1024.0 / 1024.0
            );

            let scheme = None;

            Ok((
                Orbital {
                    scheme,
                    delayed: VecDeque::new(),
                    input: None,
                },
                displays,
            ))
        }
    }

    /// Write a Packet to scheme I/O (no-op if scheme is None)
    pub fn scheme_write(&self, response: Response) -> io::Result<()> {
        if let Some(scheme) = &self.scheme {
            scheme.write_response(response, SignalBehavior::Restart)?;
        }
        Ok(())
    }

    /// Start the main loop
    pub fn run(
        self,
        handler: OrbitalScheme,
        _login_cmd: &mut std::process::Command,
    ) -> Result<(), Error> {
        user_data! {
            enum Source {
                Scheme,
                Input,
            }
        }

        let mut state = SchemeState::new();
        let mut me = OrbitalHandler {
            orb: self,
            handler,
            handles: BTreeMap::new(),
            next_id: 0,
        };

        #[cfg(feature = "ziqa-bga-direct")]
        if me.orb.scheme.is_none() && me.orb.input.is_none() {
            let mut ziqa_input = crate::ziqa_input::ZiqaInput::open();
            let mut events = [orbclient::Event::new(); 16];
            let mut coalesced = [orbclient::Event::new(); 512];

            // ponytail: software frame pacing. BGA exposes no vsync/pageflip IRQ, so hold a
            // steady 60 Hz while input or damage is active and coalesce extra mouse packets.
            // Trying to paint mouse input at 120 Hz just doubles framebuffer work on QEMU.
            // Idle (no input, no damage) drops to ~10Hz to save CPU.
            let input_frame = std::time::Duration::from_nanos(16_666_667); // ~60 Hz while input is flowing
            let active_frame = std::time::Duration::from_nanos(16_666_667); // ~60 Hz dirty redraw without input
            let idle_frame = std::time::Duration::from_millis(100); // ~10 Hz idle
            // PROF-TEMP: record full-screen area for dirty/screen ratio reporting
            crate::prof::set_screen_area(me.handler.screen_area());
            // PROF-TEMP: calibrate TSC->us using a 100ms sleep (Instant is ~1s-granular here)
            crate::prof::calibrate();
            loop {
                let frame_start = std::time::Instant::now();
                let _ft0 = crate::prof::tsc(); // PROF-TEMP: frame work-interval TSC origin
                // Drain the whole kernel input ring once per frame. Reading only
                // 16 events per 16 ms lets high-rate mouse input build a permanent
                // backlog; then clicks/keys arrive late and the GUI feels frozen.
                //
                // Mouse moves are absolute positions, so a run of moves can be
                // collapsed to its final event. Non-mouse events flush the pending
                // mouse first, preserving button/key ordering.
                let mut n = 0;
                let mut pending_mouse = None;
                for _ in 0..32 {
                    let count = ziqa_input.read_events(&mut events);
                    if count == 0 {
                        break;
                    }

                    for event in events[..count].iter().copied() {
                        if event.code == orbclient::EVENT_MOUSE {
                            pending_mouse = Some(event);
                            continue;
                        }

                        if let Some(mouse) = pending_mouse.take() {
                            if n < coalesced.len() {
                                coalesced[n] = mouse;
                                n += 1;
                            }
                        }
                        if n < coalesced.len() {
                            coalesced[n] = event;
                            n += 1;
                        }
                    }
                }

                if let Some(mouse) = pending_mouse {
                    if n < coalesced.len() {
                        coalesced[n] = mouse;
                        n += 1;
                    }
                }
                let _prof_in = crate::prof::tsc(); // PROF-TEMP
                let had_input = n > 0;
                if had_input {
                    me.handler.handle_input(&coalesced[..n]);
                }
                let _prof_input_us = crate::prof::since(_prof_in); // PROF-TEMP
                let dirty = me.handler.is_dirty();
                me.handler.redraw();
                // PROF-TEMP: fold this frame into its phase bucket (idle=0/move=1/drag=2)
                let _prof_total_us = crate::prof::since(_ft0); // PROF-TEMP
                let _prof_bucket = if me.handler.is_dragging() {
                    2
                } else if had_input {
                    1
                } else {
                    0
                };
                crate::prof::commit(_prof_bucket, _prof_total_us, _prof_input_us);
                crate::prof::maybe_flush();
                let target = if had_input {
                    input_frame
                } else if dirty {
                    active_frame
                } else {
                    idle_frame
                };
                let elapsed = frame_start.elapsed();
                if elapsed < target {
                    std::thread::sleep(target - elapsed);
                }
            }
        }

        if me.orb.scheme.is_none() && me.orb.input.is_none() {
            me.handler.redraw();
            loop {
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
        }

        let event_queue = EventQueue::<Source>::new()?;

        // Only register scheme and subscribe to scheme events if socket is available
        if me.orb.scheme.is_some() {
            let cap_id = me.scheme_root()?;
            register_scheme_inner(me.orb.scheme.as_mut().unwrap(), "orbital", cap_id)?;

            let scheme_fd = me.orb.scheme.as_ref().unwrap().inner().raw();
            event_queue.subscribe(scheme_fd, Source::Scheme, event::EventFlags::READ)?;
        } else {
            error!("No scheme socket available — running in display-only mode");
        }

        if let Some(input) = me.orb.input.as_ref() {
            let input_fd = input.event_handle().as_raw_fd();
            event_queue.subscribe(input_fd as usize, Source::Input, event::EventFlags::READ)?;
        } else {
            error!("No input consumer available — input events disabled");
        }

        let mut event_iter = event_queue.map(|e| e.map(|e| e.user_data));
        let mut fake_input_event = None; // TODO: a hack
        let mut request_buf = Vec::with_capacity(16);

        'events: while let Some(event_res) = fake_input_event.take().or_else(|| event_iter.next()) {
            match event_res? {
                Source::Scheme => {
                    if me.orb.scheme.is_none() {
                        continue 'events;
                    }
                    loop {
                        let read_result = {
                            let scheme = me.orb.scheme.as_ref().unwrap();
                            scheme.read_requests(&mut request_buf, SignalBehavior::Restart)
                        };
                        match read_result {
                            Ok(()) => (),
                            Err(err) => {
                                if err.errno == EWOULDBLOCK || err.errno == EAGAIN {
                                    continue 'events;
                                } else {
                                    return Err(err.into());
                                }
                            }
                        }
                        if request_buf.is_empty() {
                            break 'events;
                        }
                        for request in request_buf.drain(..) {
                            let req = match request.kind() {
                                RequestKind::Call(req) => req,
                                RequestKind::OnClose { id } => {
                                    me.on_close(id);
                                    continue;
                                }
                                RequestKind::Cancellation(req) => {
                                    if let Some(idx) = me
                                        .orb
                                        .delayed
                                        .iter()
                                        .position(|(_, op)| op.req_id() == req.id)
                                    {
                                        let (_, op) = me
                                            .orb
                                            .delayed
                                            .remove(idx)
                                            .expect("already found at index");
                                        me.orb.scheme_write(Response::err(ECANCELED, op))?;
                                    }
                                    fake_input_event = Some(Ok(Source::Input));
                                    continue;
                                }
                                _ => continue, // TODO?
                            };
                            let caller_ctx = req.caller();
                            let op = match req.op() {
                                Ok(op) => op,
                                Err(req) => {
                                    me.orb.scheme_write(Response::err(EOPNOTSUPP, req))?;
                                    continue;
                                }
                            };
                            if let Op::Read(mut read_op) = op {
                                let should_delay = me.should_delay(read_op.fd);
                                let res = me.read(read_op.fd, read_op.buf(), 0, 0, &caller_ctx);
                                if should_delay && res == Ok(0) {
                                    me.orb.delayed.push_back((caller_ctx, read_op));
                                } else {
                                    me.orb.scheme_write(Response::new(res, read_op))?;
                                }
                            } else {
                                let resp = op.handle_sync(caller_ctx, &mut me, &mut state);
                                me.orb.scheme_write(resp)?;
                            }
                        }
                        me.handler.handle_after(&mut me.orb, &me.handles)?;
                    }
                }
                Source::Input => {
                    let mut events = [Event::new(); 16];
                    loop {
                        let input_result = match me.orb.input.as_ref() {
                            Some(input) => input.read_events(&mut events)?,
                            None => break,
                        };
                        match input_result {
                            ConsumerHandleEvent::Events(&[]) => break,
                            ConsumerHandleEvent::Events(events) => {
                                let mut delayed_left = me.orb.delayed.len();

                                while delayed_left > 0
                                    && let Some((ctx, mut read_op)) = me.orb.delayed.pop_front()
                                {
                                    delayed_left -= 1;

                                    let should_delay = me.should_delay(read_op.fd);

                                    // TODO: deduplicate with the same code above
                                    let res = me.read(
                                        read_op.fd,
                                        read_op.buf(),
                                        // dont-care
                                        0,
                                        // dont-care
                                        0,
                                        &ctx,
                                    );
                                    if should_delay && res == Ok(0) {
                                        me.orb.delayed.push_back((ctx, read_op));
                                    } else {
                                        me.orb.scheme_write(Response::new(res, read_op))?;
                                    }
                                }

                                me.handler.handle_input(events);
                            }
                            ConsumerHandleEvent::Handoff => {}
                        };
                    }
                    me.handler.handle_after(&mut me.orb, &me.handles)?;
                }
            }
        }
        Ok(())
    }
}

pub(crate) enum Handle {
    SchemeRoot,
    DisplaySize(usize),
    Window(WindowId),
    Clipboard(WindowId),
}

pub struct OrbitalHandler {
    orb: Orbital,
    handler: OrbitalScheme,
    handles: BTreeMap<usize, Handle>,
    next_id: usize,
}

impl SchemeSync for OrbitalHandler {
    fn scheme_root(&mut self) -> syscall::Result<usize> {
        let id = self.next_id;
        self.next_id += 1;
        self.handles.insert(id, Handle::SchemeRoot);
        Ok(id)
    }

    fn openat(
        &mut self,
        dirfd: usize,
        path: &str,
        _flags: usize,
        _fcntl_flags: u32,
        _ctx: &CallerCtx,
    ) -> syscall::Result<OpenResult> {
        {
            let Some(handle) = self.handles.get(&dirfd) else {
                return Err(syscall::Error::new(EBADF));
            };
            if !matches!(handle, Handle::SchemeRoot) {
                return Err(syscall::Error::new(EACCES));
            }
        }

        // FIXME remove once orbclient no longer depends on the DISPLAY env var
        if let Some(display) = path.strip_prefix("99.") {
            let display = display.parse().map_err(|_| syscall::Error::new(EINVAL))?;
            if display >= self.handler.display_count() {
                return Err(syscall::Error::new(EINVAL));
            }

            let new_id = self.next_id;
            self.next_id += 1;
            // Use display index for both paths
            self.handles.insert(new_id, Handle::DisplaySize(display));
            return Ok(OpenResult::ThisScheme {
                number: new_id,
                flags: NewFdFlags::empty(),
            });
        }

        let mut parts = path.split('/');

        let path_first_char = path.chars().nth(0).unwrap_or('\0');
        let flags = if path_first_char.is_ascii_digit() || path_first_char == '-' {
            // to handle case like `/scheme/orbital//` being assumed as one slash
            ""
        } else {
            parts.next().unwrap_or("")
        };

        let x = parts.next().unwrap_or("").parse::<i32>().unwrap_or(0);
        let y = parts.next().unwrap_or("").parse::<i32>().unwrap_or(0);
        let width = parts.next().unwrap_or("").parse::<u32>().unwrap_or(0);
        let height = parts.next().unwrap_or("").parse::<u32>().unwrap_or(0);

        let mut title = parts.next().unwrap_or("").to_string();
        for part in parts {
            title.push('/');
            title.push_str(part);
        }

        let id = self
            .handler
            .handle_window_new(x, y, width, height, flags, title)?;
        let new_id = self.next_id;
        self.handles.insert(new_id, Handle::Window(id));
        self.next_id += 1;
        Ok(OpenResult::ThisScheme {
            number: new_id,
            flags: NewFdFlags::empty(),
        })
    }

    fn dup(&mut self, id: usize, buf: &[u8], _ctx: &CallerCtx) -> syscall::Result<OpenResult> {
        let Some(&Handle::Window(id) | &Handle::Clipboard(id)) = self.handles.get(&id) else {
            return Err(syscall::Error::new(EBADF));
        };
        if buf == b"clipboard" {
            //TODO: implement better clipboard mechanism
            let id = self.handler.handle_clipboard_new(id)?;
            let new_id = self.next_id;
            self.handles.insert(new_id, Handle::Clipboard(id));
            self.next_id += 1;
            Ok(OpenResult::ThisScheme {
                number: new_id,
                flags: NewFdFlags::POSITIONED,
            })
        } else {
            Err(syscall::Error::new(EINVAL))
        }
    }

    fn read(
        &mut self,
        id: usize,
        buf: &mut [u8],
        offset: u64,
        _flags: u32,
        _ctx: &CallerCtx,
    ) -> syscall::Result<usize> {
        let Some(handle) = self.handles.get(&id) else {
            return Err(syscall::Error::new(EBADF));
        };
        //TODO: implement better clipboard mechanism
        let id = match *handle {
            Handle::Clipboard(id) => return self.handler.handle_clipboard_read(id, offset, buf),
            Handle::Window(id) => id,
            Handle::SchemeRoot | Handle::DisplaySize(_) => return Err(syscall::Error::new(EBADF)),
        };

        let slice: &mut [Event] = unsafe {
            slice::from_raw_parts_mut(
                buf.as_mut_ptr() as *mut Event,
                buf.len() / mem::size_of::<Event>(),
            )
        };
        let n = self.handler.handle_window_read(id, slice)?;
        Ok(n * mem::size_of::<Event>())
    }

    fn write(
        &mut self,
        id: usize,
        buf: &[u8],
        offset: u64,
        _flags: u32,
        _ctx: &CallerCtx,
    ) -> syscall::Result<usize> {
        let Some(handle) = self.handles.get(&id) else {
            return Err(syscall::Error::new(EBADF));
        };
        //TODO: implement better clipboard mechanism
        let id = match *handle {
            Handle::Clipboard(id) => return self.handler.handle_clipboard_write(id, offset, buf),
            Handle::Window(id) => id,
            Handle::SchemeRoot | Handle::DisplaySize(_) => return Err(syscall::Error::new(EBADF)),
        };

        if let Ok(msg) = str::from_utf8(buf) {
            let (kind, data) = {
                let mut parts = msg.splitn(2, ',');
                let kind = parts.next().unwrap_or("");
                let data = parts.next().unwrap_or("");
                (kind, data)
            };
            match kind {
                "A" => match data {
                    "0" => {
                        self.handler.handle_window_async(id, false)?;
                        Ok(buf.len())
                    }
                    "1" => {
                        self.handler.handle_window_async(id, true)?;
                        Ok(buf.len())
                    }
                    _ => Err(syscall::Error::new(EINVAL)),
                },
                "D" => {
                    let Ok(mode) = WindowDragKind::from_str(data) else {
                        return Err(syscall::Error::new(EINVAL));
                    };
                    self.handler.handle_window_drag(id, mode)?;
                    Ok(buf.len())
                }
                "F" => {
                    let mut parts = data.split(',');
                    let flags = parts.next().unwrap_or("");
                    let value = match parts.next().unwrap_or("") {
                        "0" => false,
                        "1" => true,
                        _ => return Err(syscall::Error::new(EINVAL)),
                    };
                    for flag in flags.chars() {
                        self.handler.handle_window_set_flag(id, flag, value)?;
                    }
                    Ok(buf.len())
                }
                "M" => match data {
                    "C,0" => {
                        self.handler.handle_window_mouse_cursor(id, false)?;
                        Ok(buf.len())
                    }
                    "C,1" => {
                        self.handler.handle_window_mouse_cursor(id, true)?;
                        Ok(buf.len())
                    }
                    "G,0" => {
                        self.handler.handle_window_mouse_grab(id, false)?;
                        Ok(buf.len())
                    }
                    "G,1" => {
                        self.handler.handle_window_mouse_grab(id, true)?;
                        Ok(buf.len())
                    }
                    "R,0" => {
                        self.handler.handle_window_mouse_relative(id, false)?;
                        Ok(buf.len())
                    }
                    "R,1" => {
                        self.handler.handle_window_mouse_relative(id, true)?;
                        Ok(buf.len())
                    }
                    _ => Err(syscall::Error::new(EINVAL)),
                },
                "P" => {
                    let mut parts = data.split(',');
                    let x = parts.next().unwrap_or("").parse::<i32>().ok();
                    let y = parts.next().unwrap_or("").parse::<i32>().ok();

                    self.handler.handle_window_position(id, x, y)?;

                    Ok(buf.len())
                }
                "S" => {
                    let mut parts = data.split(',');
                    let w = parts.next().unwrap_or("").parse::<u32>().ok();
                    let h = parts.next().unwrap_or("").parse::<u32>().ok();

                    self.handler.handle_window_resize(id, w, h)?;

                    Ok(buf.len())
                }
                "T" => {
                    self.handler.handle_window_title(id, data.to_string())?;

                    Ok(buf.len())
                }
                "Y" => {
                    let mut parts = data.split(',').peekable();
                    let mut damages = Vec::new();
                    while parts.peek().is_some() {
                        let x = parts.next().unwrap_or("").parse::<i32>().unwrap_or(0);
                        let y = parts.next().unwrap_or("").parse::<i32>().unwrap_or(0);
                        let w = parts.next().unwrap_or("").parse::<u32>().unwrap_or(0);
                        let h = parts.next().unwrap_or("").parse::<u32>().unwrap_or(0);
                        damages.push(Rect::new(x, y, w, h));
                    }
                    self.handler.handle_window_sync(id, Some(damages))?;

                    Ok(buf.len())
                }
                _ => Err(syscall::Error::new(EINVAL)),
            }
        } else {
            Err(syscall::Error::new(EINVAL))
        }
    }

    fn fevent(
        &mut self,
        id: usize,
        _flags: EventFlags,
        _ctx: &CallerCtx,
    ) -> syscall::Result<EventFlags> {
        let Some(&Handle::Window(id) | &Handle::Clipboard(id)) = self.handles.get(&id) else {
            return Err(syscall::Error::new(EBADF));
        };
        self.handler
            .handle_window_clear_notified(id)
            .and(Ok(EventFlags::empty()))
    }

    fn fpath(&mut self, id: usize, mut buf: &mut [u8], _ctx: &CallerCtx) -> syscall::Result<usize> {
        match self.handles.get(&id) {
            Some(&Handle::DisplaySize(display)) => {
                let (width, height, scale) = self.handler.display_size(display);
                let original_len = buf.len();
                let _ = write!(buf, "orbital:99.{display}/{}/{}/{}", width, height, scale);
                Ok(original_len - buf.len())
            }
            Some(&Handle::Window(id) | &Handle::Clipboard(id)) => {
                let props = self.handler.handle_window_properties(id)?;
                let original_len = buf.len();
                #[allow(clippy::write_literal)] // TODO: Z order
                let _ = write!(
                    buf,
                    "{}/{}/{}/{}/{}/{}",
                    props.flags, props.x, props.y, props.width, props.height, props.title
                );
                Ok(original_len - buf.len())
            }
            _ => Err(syscall::Error::new(EBADF)),
        }
    }

    fn fsync(&mut self, id: usize, _ctx: &CallerCtx) -> syscall::Result<()> {
        let Some(&Handle::Window(id) | &Handle::Clipboard(id)) = self.handles.get(&id) else {
            return Err(syscall::Error::new(EBADF));
        };
        self.handler.handle_window_sync(id, None)
    }

    fn mmap_prep(
        &mut self,
        id: usize,
        _offset: u64,
        size: usize,
        _flags: syscall::MapFlags,
        _ctx: &CallerCtx,
    ) -> syscall::Result<usize> {
        let Some(&Handle::Window(id) | &Handle::Clipboard(id)) = self.handles.get(&id) else {
            return Err(syscall::Error::new(EBADF));
        };
        // TODO: handle offset, flags?
        let data = self.handler.handle_window_map(id, true)?;

        if size > data.len() * core::mem::size_of::<Color>() {
            return Err(syscall::Error::new(EINVAL));
        }

        Ok(data.as_mut_ptr() as usize)
    }

    fn munmap(
        &mut self,
        id: usize,
        _offset: u64,
        _size: usize,
        _flags: syscall::MunmapFlags,
        _ctx: &CallerCtx,
    ) -> syscall::Result<()> {
        let Some(&Handle::Window(id) | &Handle::Clipboard(id)) = self.handles.get(&id) else {
            return Err(syscall::Error::new(EBADF));
        };
        // TODO: handle offset, size, flags?
        self.handler.handle_window_unmap(id)?;

        Ok(())
    }
}

impl OrbitalHandler {
    fn on_close(&mut self, id: usize) {
        let Some(handle) = self.handles.remove(&id) else {
            return;
        };
        // TODO: implement better clipboard mechanism
        match handle {
            Handle::Clipboard(id) => self.handler.handle_clipboard_close(id),
            Handle::Window(id) => self.handler.handle_window_close(id),
            Handle::SchemeRoot | Handle::DisplaySize(_) => {}
        };
    }

    fn should_delay(&self, id: usize) -> bool {
        if let Some(handle) = self.handles.get(&id) {
            match *handle {
                Handle::Clipboard(id) | Handle::Window(id) => self.handler.should_delay(id),
                Handle::SchemeRoot | Handle::DisplaySize(_) => false,
            }
        } else {
            false
        }
    }
}
