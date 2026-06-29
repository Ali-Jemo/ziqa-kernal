//! Redox Orbital Scheme Protocol Implementation for ZiqaKernel
//!
//! This module implements the display_v2 and input schemes that Orbital expects.
//! Bridges Orbital's display_v2 commands to the Ziqa kernel compositor (IPC channel 3).
//! Each window gets a SHM region (backing the pixel buffer) + a compositor surface.
//! Orbital writes pixels via shm_at; flip triggers a compositor Flush.

use crate::scheme::{Scheme, SchemeResult};

use core::sync::atomic::{AtomicI32, AtomicU8, AtomicUsize, Ordering};
use alloc::collections::VecDeque;
use alloc::string::String;
use alloc::string::ToString;
use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::Mutex;

/// Compositor IPC channel — matches the channel the compositor_main listens on.
const COMPOSITOR_CHAN: u32 = 3;

/// ZiqaKernel's simplified Event structure (matches Redox syscall::data::Event)
#[derive(Copy, Clone, Debug, Default)]
#[repr(C)]
pub struct Event {
    pub id: usize,
    pub flags: usize,
    pub data: usize,
}

/// Single window state for Orbital — bridges display_v2 to Ziqa compositor
pub struct WindowState {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub title: String,
    pub async_mode: bool,
    pub mouse_cursor: bool,
    pub mouse_grab: bool,
    pub mouse_relative: bool,
    pub events: VecDeque<Event>,
    /// SHM region id — backs the window pixel buffer
    pub shm_id: u32,
    /// Compositor surface id (assigned by compositor, tracked by bridge)
    pub surface_id: u32,
    /// Owning process PID (0 = kernel)
    pub pid: usize,
}

impl WindowState {
    pub fn new(x: i32, y: i32, width: u32, height: u32, title: String) -> Self {
        Self {
            x, y, width, height, title,
            async_mode: false,
            mouse_cursor: true,
            mouse_grab: false,
            mouse_relative: false,
            events: VecDeque::new(),
            shm_id: 0,
            surface_id: 0,
            pid: 0,
        }
    }

    pub fn push_event(&mut self, event: Event) {
        self.events.push_back(event);
    }

    pub fn pop_events(&mut self, buf: &mut [Event]) -> usize {
        let mut count = 0;
        for dest in buf.iter_mut() {
            match self.events.pop_front() {
                Some(ev) => {
                    *dest = ev;
                    count += 1;
                }
                None => break,
            }
        }
        count
    }
}

/// Global window registry
pub static WINDOWS: Mutex<alloc::collections::BTreeMap<usize, Arc<Mutex<WindowState>>>> =
    Mutex::new(alloc::collections::BTreeMap::new());

/// OrbitalBridge handles both display_v2 and input schemes
pub struct OrbitalBridge {
    next_handle: AtomicUsize,
    last_mouse_x: AtomicI32,
    last_mouse_y: AtomicI32,
    last_mouse_btn: AtomicU8,
    /// Compositor surface ID counter — mirrors the compositor's next_id.
    /// The bridge and compositor both start from 1, and surfaces are created
    /// in the same order, so these stay in sync.
    next_surface_id: AtomicUsize,
}

impl OrbitalBridge {
    pub const fn new() -> Self {
        Self {
            next_handle: AtomicUsize::new(1),
            last_mouse_x: AtomicI32::new(0),
            last_mouse_y: AtomicI32::new(0),
            last_mouse_btn: AtomicU8::new(0),
            next_surface_id: AtomicUsize::new(1),
        }
    }
}

/// Helper: send a raw opcode + payload to the compositor channel.
fn send_compositor_msg(opcode: u8, payload: &[u8]) {
    let mut buf = [0u8; 256];
    buf[0] = opcode;
    let n = payload.len().min(255);
    buf[1..1 + n].copy_from_slice(&payload[..n]);
    let _ = crate::ipc::send(COMPOSITOR_CHAN, crate::process::Pid(0), &buf[..1 + n]);
}

/// Helper: create a SHM region for a window and attach it (returns shm_id).
fn create_window_shm(width: u32, height: u32) -> Option<u32> {
    let size = (width as usize) * (height as usize) * 4;
    let pid = crate::process::scheduler::SCHEDULER
        .current_pid()
        .unwrap_or(crate::process::Pid(0));
    let shm_id = crate::ipc::shm::SHM.lock().create(pid, size).ok()?;
    // Attach so the kernel compositor can read pixels for compositing
    let _ = crate::ipc::shm::SHM.lock().attach(shm_id, crate::process::Pid(0));
    Some(shm_id)
}

impl Scheme for OrbitalBridge {
    fn open(&self, path: &str, _flags: usize) -> SchemeResult<usize> {
        let handle = self.next_handle.fetch_add(1, Ordering::Relaxed);

        if path.starts_with("display_v2:") || path.starts_with("display:") {
            let inner = if path.starts_with("display_v2:") {
                path.trim_start_matches("display_v2:")
            } else {
                path.trim_start_matches("display:")
            };
            if let Some((title, rest)) = inner.split_once(':') {
                let parts: Vec<&str> = rest.split(',').collect();
                if parts.len() >= 4 {
                    let x = parts[0].parse::<i32>().unwrap_or(100);
                    let y = parts[1].parse::<i32>().unwrap_or(100);
                    let w = parts[2].parse::<u32>().unwrap_or(800);
                    let h = parts[3].parse::<u32>().unwrap_or(600);

                    let _ = WINDOWS.lock().insert(handle, Arc::new(Mutex::new(WindowState::new(
                        x, y, w, h, title.to_string()
                    ))));
                    crate::println!(
                        "[OrbitalBridge] open '{}' -> handle={} ({}x{} at {},{})",
                        path, handle, w, h, x, y
                    );
                }
            }
        } else if path == "input" || path.starts_with("input:") {
            crate::println!("[OrbitalBridge] open input '{}' -> handle={}", path, handle);
        }
        Ok(handle)
    }

    fn read(&self, id: usize, buf: &mut [u8]) -> SchemeResult<usize> {
        let event_size = core::mem::size_of::<Event>();
        let events_to_read = buf.len() / event_size;
        if events_to_read == 0 {
            return Ok(0);
        }

        let events_slice = unsafe {
            core::slice::from_raw_parts_mut(
                buf.as_mut_ptr() as *mut Event,
                events_to_read,
            )
        };

        // Check window-specific events first
        {
            let windows = WINDOWS.lock();
            if let Some(window) = windows.get(&id) {
                let mut guard = window.lock();
                let count = guard.pop_events(events_slice);
                return Ok(count * event_size);
            }
        }

        // Use the separate Orbital input buffer so we don't consume
        // events before the kernel shell (read_stdin / INPUT_BUF) sees them.
        let mut key_buf = [0u8; 4];
        let n = crate::drivers::keyboard::read_editor_byte(&mut key_buf);
        if n > 0 {
            events_slice[0] = Event {
                id: 0, // Keyboard
                flags: 1, // EVENT_READ
                data: key_buf[0] as usize,
            };
            return Ok(core::mem::size_of::<Event>());
        }

        // Try mouse
        let (x, y) = crate::drivers::ps2_mouse::get_mouse_pos();
        let btn = crate::drivers::ps2_mouse::get_mouse_btn();
        if x != self.last_mouse_x.load(Ordering::Relaxed)
            || y != self.last_mouse_y.load(Ordering::Relaxed)
            || btn != self.last_mouse_btn.load(Ordering::Relaxed)
        {
            self.last_mouse_x.store(x, Ordering::Relaxed);
            self.last_mouse_y.store(y, Ordering::Relaxed);
            self.last_mouse_btn.store(btn, Ordering::Relaxed);
            events_slice[0] = Event {
                id: 1, // Mouse
                flags: 1, // EVENT_READ
                data: ((x as usize) << 16) | (y as usize),
            };
            return Ok(event_size);
        }

        Ok(0)
    }

    fn fevent(&self, id: usize, flags: usize) -> SchemeResult<usize> {
        let mut ready = 0usize;

        let windows = WINDOWS.lock();
        if let Some(window) = windows.get(&id) {
            if !window.lock().events.is_empty() {
                ready |= flags;
                return Ok(ready);
            }
        }

        if crate::drivers::keyboard::has_input() {
            ready |= flags & 1;
        }
        let (mx, my) = crate::drivers::ps2_mouse::get_mouse_pos();
        if mx != self.last_mouse_x.load(Ordering::Relaxed)
            || my != self.last_mouse_y.load(Ordering::Relaxed)
        {
            ready |= flags & 1;
        }

        Ok(ready & flags)
    }

    fn write(&self, id: usize, buf: &[u8]) -> SchemeResult<usize> {
        if buf.is_empty() {
            return Ok(0);
        }

        let s = match core::str::from_utf8(buf) {
            Ok(s) => s,
            Err(_) => return Err(crate::abi::AbiError::Other("Invalid UTF-8")),
        };

        let mut parts = s.split(',');
        let cmd = parts.next().unwrap_or("");

        // Helper: get the surface_id for a window, or 0 if not found
        let surface_id = || -> u32 {
            WINDOWS.lock().get(&id)
                .map(|w| w.lock().surface_id)
                .unwrap_or(0)
        };

        match cmd.chars().next() {
            Some('O') => {
                // Create window: O,x,y,w,h,title — create SHM + compositor surface
                let x: i32 = parts.next().unwrap_or("100").parse().unwrap_or(100);
                let y: i32 = parts.next().unwrap_or("100").parse().unwrap_or(100);
                let w: u32 = parts.next().unwrap_or("800").parse().unwrap_or(800);
                let h: u32 = parts.next().unwrap_or("600").parse().unwrap_or(600);
                let title: String = parts.next().unwrap_or("").to_string();

                let pid = crate::process::scheduler::SCHEDULER
                    .current_pid()
                    .map(|p| p.0 as usize)
                    .unwrap_or(0);

                // Allocate a new surface_id from our counter
                let sid = self.next_surface_id.fetch_add(1, Ordering::Relaxed) as u32;

                // Create SHM region for the window's pixel buffer
                let shm_id = match create_window_shm(w, h) {
                    Some(id) => id,
                    None => {
                        crate::println!("[OrbitalBridge] SHM create failed for {}x{}", w, h);
                        return Ok(buf.len());
                    }
                };

                // Create surface in compositor
                let surf_msg = crate::ipc::gui::CreateSurfaceMsg { surface_id: sid, width: w, height: h };
                let surf_bytes = unsafe {
                    core::slice::from_raw_parts(
                        &surf_msg as *const _ as *const u8,
                        core::mem::size_of::<crate::ipc::gui::CreateSurfaceMsg>(),
                    )
                };
                send_compositor_msg(crate::ipc::gui::OpCode::CreateSurface as u8, surf_bytes);

                // Attach SHM buffer to the surface
                let attach_msg = crate::ipc::gui::BufferAttachMsg {
                    surface_id: sid,
                    shm_id,
                    width: w,
                    height: h,
                };
                let attach_bytes = unsafe {
                    core::slice::from_raw_parts(
                        &attach_msg as *const _ as *const u8,
                        core::mem::size_of::<crate::ipc::gui::BufferAttachMsg>(),
                    )
                };
                send_compositor_msg(crate::ipc::gui::OpCode::BufferAttach as u8, attach_bytes);

                // Set position
                let pos_msg = crate::ipc::gui::SetPositionMsg { surface_id: sid, x, y };
                let pos_bytes = unsafe {
                    core::slice::from_raw_parts(
                        &pos_msg as *const _ as *const u8,
                        core::mem::size_of::<crate::ipc::gui::SetPositionMsg>(),
                    )
                };
                send_compositor_msg(crate::ipc::gui::OpCode::SetPosition as u8, pos_bytes);

                // Update window state
                {
                    let windows = WINDOWS.lock();
                    if let Some(window) = windows.get(&id) {
                        let mut guard = window.lock();
                        guard.x = x;
                        guard.y = y;
                        guard.width = w;
                        guard.height = h;
                        guard.title = title;
                        guard.shm_id = shm_id;
                        guard.surface_id = sid;
                        guard.pid = pid;
                    }
                }

                crate::println!(
                    "[OrbitalBridge] Created window {}: surface={} shm={} ({}x{} at {},{})",
                    id, sid, shm_id, w, h, x, y
                );
                Ok(buf.len())
            }
            Some('F') | Some('f') => {
                // Flip — mark the surface as dirty so compositor repaints it
                let sid = surface_id();
                if sid == 0 { return Ok(buf.len()); }
                let flush_msg = crate::ipc::gui::FlushMsg {
                    surface_id: sid,
                    x: 0,
                    y: 0,
                    width: u32::MAX,
                    height: u32::MAX,
                };
                let flush_bytes = unsafe {
                    core::slice::from_raw_parts(
                        &flush_msg as *const _ as *const u8,
                        core::mem::size_of::<crate::ipc::gui::FlushMsg>(),
                    )
                };
                send_compositor_msg(crate::ipc::gui::OpCode::Flush as u8, flush_bytes);
                Ok(buf.len())
            }
            Some('A') => {
                let val = parts.next().unwrap_or("0");
                if let Some(window) = WINDOWS.lock().get(&id) {
                    window.lock().async_mode = val == "1";
                }
                Ok(buf.len())
            }
            Some('M') => {
                let kind = parts.next().unwrap_or("");
                let val = parts.next().unwrap_or("0");
                if let Some(window) = WINDOWS.lock().get(&id) {
                    let mut guard = window.lock();
                    match kind {
                        "C" => guard.mouse_cursor = val == "1",
                        "G" => guard.mouse_grab = val == "1",
                        "R" => guard.mouse_relative = val == "1",
                        _ => {}
                    }
                }
                Ok(buf.len())
            }
            Some('P') => {
                let x: i32 = parts.next().unwrap_or("0").parse().unwrap_or(0);
                let y: i32 = parts.next().unwrap_or("0").parse().unwrap_or(0);
                let sid = surface_id();
                {
                    let windows = WINDOWS.lock();
                    if let Some(window) = windows.get(&id) {
                        let mut guard = window.lock();
                        guard.x = x;
                        guard.y = y;
                    }
                }
                if sid != 0 {
                    let pos_msg = crate::ipc::gui::SetPositionMsg { surface_id: sid, x, y };
                    let pos_bytes = unsafe {
                        core::slice::from_raw_parts(
                            &pos_msg as *const _ as *const u8,
                            core::mem::size_of::<crate::ipc::gui::SetPositionMsg>(),
                        )
                    };
                    send_compositor_msg(crate::ipc::gui::OpCode::SetPosition as u8, pos_bytes);
                }
                Ok(buf.len())
            }
            Some('S') => {
                let w: u32 = parts.next().unwrap_or("0").parse().unwrap_or(0);
                let h: u32 = parts.next().unwrap_or("0").parse().unwrap_or(0);
                let sid = surface_id();
                {
                    let windows = WINDOWS.lock();
                    if let Some(window) = windows.get(&id) {
                        let mut guard = window.lock();
                        guard.width = w;
                        guard.height = h;
                    }
                }
                if sid != 0 {
                    let resize_msg = crate::ipc::gui::ResizeMsg { surface_id: sid, width: w, height: h };
                    let resize_bytes = unsafe {
                        core::slice::from_raw_parts(
                            &resize_msg as *const _ as *const u8,
                            core::mem::size_of::<crate::ipc::gui::ResizeMsg>(),
                        )
                    };
                    send_compositor_msg(crate::ipc::gui::OpCode::Resize as u8, resize_bytes);
                }
                Ok(buf.len())
            }
            Some('T') => {
                let title = parts.next().unwrap_or("").to_string();
                if let Some(window) = WINDOWS.lock().get(&id) {
                    window.lock().title = title;
                }
                Ok(buf.len())
            }
            Some('Y') => {
                // Damage sync — flush the damaged region
                let x: u32 = parts.next().unwrap_or("0").parse().unwrap_or(0);
                let y: u32 = parts.next().unwrap_or("0").parse().unwrap_or(0);
                let w: u32 = parts.next().unwrap_or("0").parse().unwrap_or(0);
                let h: u32 = parts.next().unwrap_or("0").parse().unwrap_or(0);
                let sid = surface_id();
                if sid != 0 {
                    let flush_msg = crate::ipc::gui::FlushMsg { surface_id: sid, x, y, width: w, height: h };
                    let flush_bytes = unsafe {
                        core::slice::from_raw_parts(
                            &flush_msg as *const _ as *const u8,
                            core::mem::size_of::<crate::ipc::gui::FlushMsg>(),
                        )
                    };
                    send_compositor_msg(crate::ipc::gui::OpCode::Flush as u8, flush_bytes);
                }
                Ok(buf.len())
            }
            Some('D') => Ok(buf.len()),
            _ => Ok(buf.len()),
        }
    }

    fn close(&self, id: usize) -> SchemeResult<()> {
        let mut windows = WINDOWS.lock();
        windows.remove(&id);
        crate::klog!(crate::klog::Level::Debug, "[OrbitalBridge] close: {}", id);
        Ok(())
    }
}
