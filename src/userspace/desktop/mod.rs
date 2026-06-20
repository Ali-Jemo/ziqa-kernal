//! Desktop Environment — compositor client, window manager, and app launcher.
//!
//! Runs as a kernel thread or shell command. Connects to the kernel compositor
//! via IPC (channel 3 for commands, channel 4 for events), manages windows,
//! and renders the desktop background + window chrome.
//!
//! Each window is a separate compositor surface. The desktop manager tracks
//! window positions, Z-order, focus, and routes input from channel 4.

pub mod glyph;

use crate::ipc::gui::*;
use crate::process::Pid;
use core::mem::size_of;

const COMPOSITOR_CHAN: u32 = 3;
const EVENT_CHAN: u32 = 4;

/// Pack a sized struct as a byte slice.
unsafe fn any_as_bytes<T: Sized>(x: &T) -> &[u8] {
    unsafe {
        core::slice::from_raw_parts(x as *const T as *const u8, core::mem::size_of::<T>())
    }
}

/// Send a message to the compositor channel.
fn send_compositor(opcode: OpCode, payload: &[u8]) {
    let mut buf = [0u8; 256];
    buf[0] = opcode as u8;
    let n = payload.len().min(255);
    if n > 0 {
        buf[1..1 + n].copy_from_slice(&payload[..n]);
    }
    let _ = crate::ipc::send(COMPOSITOR_CHAN, Pid(0), &buf[..1 + n]);
}

/// A surface handle managed by the compositor.
#[derive(Clone, Copy, Debug)]
pub struct SurfaceHandle {
    pub id: u32,
    pub width: u32,
    pub height: u32,
    pub shm_id: u32,
}

/// Desktop window manager state.
pub struct Desktop {
    /// A full-screen surface for the desktop background.
    pub background: Option<SurfaceHandle>,
    /// Window surfaces that are being managed.
    pub windows: alloc::vec::Vec<Window>,
    /// Active Z-order: indices into `windows`, front to back.
    pub zorder: alloc::vec::Vec<usize>,
    /// Currently focused window index into `windows`.
    pub focus: Option<usize>,
    /// Cursor position in screen-space pixels.
    pub cursor_x: i32,
    pub cursor_y: i32,
    /// Last frame tick for animations.
    pub tick: u32,
}

/// A managed window with its compositor surface.
#[derive(Clone)]
pub struct Window {
    pub surface: SurfaceHandle,
    pub title: alloc::string::String,
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
    pub minimized: bool,
    pub maximized: bool,
    pub saved_x: i32,
    pub saved_y: i32,
    pub saved_w: u32,
    pub saved_h: u32,
}

impl Desktop {
    pub fn new() -> Self {
        Desktop {
            background: None,
            windows: alloc::vec::Vec::new(),
            zorder: alloc::vec::Vec::new(),
            focus: None,
            cursor_x: 0,
            cursor_y: 0,
            tick: 0,
        }
    }

    /// Connect to the compositor and set up the desktop background.
    pub fn init(&mut self) -> bool {
        // Get framebuffer info to size the desktop surface
        let fb_info = crate::drivers::virtio_gpu::get_fb_info();
        let (fb_w, fb_h) = match fb_info {
            Some((_, w, h, _)) => (w, h),
            None => {
                // Fallback: try BGA framebuffer
                let fb = crate::drivers::framebuffer::FB.lock();
                match fb.as_ref() {
                    Some(f) => (f.width as u32, f.height as u32),
                    None => return false,
                }
            }
        };

        crate::klog!(
            crate::klog::Level::Info,
            "[Desktop] Initializing on {}x{} framebuffer",
            fb_w,
            fb_h,
        );

        // Connect to compositor (OpCode 1)
        let conn = ConnectMsg { pid: 0 };
        send_compositor(OpCode::Connect, unsafe { any_as_bytes(&conn) });

        // Create background surface at framebuffer size
        let bg_surf = self.create_surface(fb_w, fb_h);
        self.background = Some(bg_surf);

        // Set background position to (0,0)
        send_compositor(
            OpCode::SetPosition,
            unsafe {
                any_as_bytes(&SetPositionMsg {
                    surface_id: bg_surf.id,
                    x: 0,
                    y: 0,
                })
            },
        );

        true
    }

    /// Create a new compositor surface and SHM buffer.
    pub fn create_surface(&mut self, width: u32, height: u32) -> SurfaceHandle {
        // 1. Create SHM
        let buf_size = (width * height * 4) as usize;
        let shm_id = crate::ipc::shm::SHM
            .lock()
            .create(Pid(0), buf_size)
            .unwrap_or(0);

        // 2. Tell compositor to create surface
        send_compositor(
            OpCode::CreateSurface,
            unsafe {
                any_as_bytes(&CreateSurfaceMsg { width, height })
            },
        );

        // The compositor assigns IDs sequentially starting from 1.
        // After creating, we need the next_id. For a kernel-thread client
        // the simplest approach: track locally and assume compositor assigns 1,2,3...
        let id = self.next_surface_id();
        crate::klog!(
            crate::klog::Level::Debug,
            "[Desktop] Created surface {} ({}x{}) shm={}",
            id,
            width,
            height,
            shm_id,
        );

        // 3. Attach SHM buffer
        let shm_addr = crate::ipc::shm::SHM
            .lock()
            .attach(shm_id, Pid(0))
            .unwrap_or(0);
        if shm_addr != 0 {
            // We have direct access to the buffer in kernel mode
            // Zero-fill it
            unsafe {
                core::ptr::write_bytes(shm_addr as *mut u8, 0, buf_size);
            }
        }

        send_compositor(
            OpCode::BufferAttach,
            unsafe {
                any_as_bytes(&BufferAttachMsg {
                    surface_id: id,
                    shm_id,
                    width,
                    height,
                })
            },
        );

        SurfaceHandle {
            id,
            width,
            height,
            shm_id,
        }
    }

    /// Track next surface ID locally (assumes compositor assigns consecutively).
    fn next_surface_id(&self) -> u32 {
        let mut max_id = 0u32;
        if let Some(bg) = &self.background {
            max_id = max_id.max(bg.id);
        }
        for w in &self.windows {
            max_id = max_id.max(w.surface.id);
        }
        max_id + 1
    }

    /// Destroy a surface.
    pub fn destroy_surface(&mut self, handle: &SurfaceHandle) {
        send_compositor(
            OpCode::DestroySurface,
            unsafe {
                any_as_bytes(&DestroySurfaceMsg {
                    surface_id: handle.id,
                })
            },
        );
    }

    /// Move/resize a window and notify the compositor.
    pub fn set_window_position(&mut self, idx: usize, x: i32, y: i32) {
        if let Some(w) = self.windows.get_mut(idx) {
            w.x = x;
            w.y = y;
            send_compositor(
                OpCode::SetPosition,
                unsafe {
                    any_as_bytes(&SetPositionMsg {
                        surface_id: w.surface.id,
                        x,
                        y,
                    })
                },
            );
        }
    }

    /// Focus a window (raise to top).
    pub fn focus_window(&mut self, idx: usize) {
        self.focus = Some(idx);
        self.raise_window(idx);
        send_compositor(
            OpCode::FocusSurface,
            unsafe {
                any_as_bytes(&FocusSurfaceMsg {
                    surface_id: self.windows[idx].surface.id,
                })
            },
        );
    }

    /// Raise a specific window to the top of the Z-order.
    pub fn raise_window(&mut self, idx: usize) {
        self.zorder.retain(|&i| i != idx);
        self.zorder.push(idx);
    }

    /// Poll the event channel for input and process.
    pub fn poll_events(&mut self) {
        while let Ok(msg) = crate::ipc::recv(EVENT_CHAN) {
            if msg.len < 1 {
                continue;
            }
            let op = msg.data[0];
            if op != OpCode::Input as u8 || msg.len < size_of::<InputMsg>() + 1 {
                continue;
            }
            let input: InputMsg = unsafe {
                core::ptr::read_unaligned(msg.data.as_ptr().add(1) as *const InputMsg)
            };
            self.handle_input(input);
        }
    }

    /// Handle an input event from the compositor.
    fn handle_input(&mut self, input: InputMsg) {
        match input.kind {
            1 => {
                // Key event — forward to focused window
                if let Some(_idx) = self.focus {
                    // TODO: forward to the window's app
                }
            }
            2 => {
                // Mouse button event
                self.cursor_x = input.x;
                self.cursor_y = input.y;
                if input.code & 1 != 0 {
                    // Left click — hit test
                    self.handle_click(input.x, input.y);
                }
                if input.code & 2 != 0 {
                    // Right click
                }
                // Update compositor cursor position
                send_compositor(
                    OpCode::SetCursorPos,
                    unsafe {
                        any_as_bytes(&SetCursorPosMsg {
                            x: input.x,
                            y: input.y,
                            visible: 1,
                        })
                    },
                );
            }
            3 => {
                // Mouse move (no button)
                self.cursor_x = input.x;
                self.cursor_y = input.y;
                send_compositor(
                    OpCode::SetCursorPos,
                    unsafe {
                        any_as_bytes(&SetCursorPosMsg {
                            x: input.x,
                            y: input.y,
                            visible: 1,
                        })
                    },
                );
            }
            _ => {}
        }
    }

    /// Handle a mouse click — hit test windows and buttons.
    fn handle_click(&mut self, mx: i32, my: i32) {
        // Iterate Z-order back-to-front (top window first)
        for &idx in self.zorder.iter().rev() {
            if let Some(w) = self.windows.get(idx) {
                if w.minimized { continue; }
                if mx >= w.x && mx < w.x + w.w as i32
                    && my >= w.y && my < w.y + w.h as i32
                {
                    // Hit — focus this window
                    self.focus_window(idx);
                    return;
                }
            }
        }
        // Click on desktop background = unfocus
        self.focus = None;
    }

    /// Get a writable pointer to a surface's SHM buffer.
    pub fn get_surface_buffer(&self, handle: &SurfaceHandle) -> Option<*mut u8> {
        let shm = crate::ipc::shm::SHM.lock();
        shm.attach(handle.shm_id, Pid(0)).ok().map(|addr| addr as *mut u8)
    }

    /// Mark a surface's entire area as dirty (needs flush).
    pub fn flush_surface(&self, handle: &SurfaceHandle) {
        send_compositor(
            OpCode::Flush,
            unsafe {
                any_as_bytes(&FlushMsg {
                    surface_id: handle.id,
                    x: 0,
                    y: 0,
                    width: handle.width,
                    height: handle.height,
                })
            },
        );
    }

    /// Flush a rectangular region of a surface.
    pub fn flush_rect(&self, handle: &SurfaceHandle, x: u32, y: u32, w: u32, h: u32) {
        send_compositor(
            OpCode::Flush,
            unsafe {
                any_as_bytes(&FlushMsg {
                    surface_id: handle.id,
                    x,
                    y,
                    width: w,
                    height: h,
                })
            },
        );
    }
}
