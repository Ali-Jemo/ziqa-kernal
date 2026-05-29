/// Native Wayland-Compatible Compositor (NWCC) - Buffer Management
/// 
/// This module implements the "Plumbing" for the compositor:
/// 1. SHM-backed buffer allocation for clients.
/// 2. Surface-to-Buffer attachments.
/// 3. Integration with the Zig-accelerated blitter.

use alloc::vec::Vec;
use alloc::collections::BTreeMap;
use crate::ipc::shm::SHM;
use crate::process::Pid;


/// Represents a client-side buffer backed by shared memory.
pub struct CompositorBuffer {
    pub shm_id: usize,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
}

/// A surface is a rectangular area on the screen where a client renders.
pub struct Surface {
    pub owner: Pid,
    pub active_buffer: Option<usize>, // Index into buffers
    pub x: i32,
    pub y: i32,
    pub z_index: i32,
}

/// Wayland-inspired IPC protocol messages for NWCC
#[derive(Debug, Clone, Copy)]
pub enum WlMessage {
    /// Create a new surface (window)
    CreateSurface { owner: Pid },
    /// Create a buffer from an SHM segment
    CreateBuffer { owner: Pid, shm_id: usize, width: u32, height: u32 },
    /// Attach a buffer to a surface
    Attach { surface_id: usize, buffer_id: usize },
    /// Set the screen position of a surface
    SetPosition { surface_id: usize, x: i32, y: i32 },
    /// Commit surface updates (mark for redraw)
    Commit { surface_id: usize },
}

pub struct CompositorState {
    pub surfaces: BTreeMap<usize, Surface>,
    pub buffers: BTreeMap<usize, CompositorBuffer>,
    pub next_id: usize,
    pub ipc_channel: Option<u32>,
}

impl CompositorState {
    pub fn new() -> Self {
        let chan = crate::ipc::create_channel();
        if let Some(id) = chan {
            crate::println!("[NWCC] Created IPC channel: {}", id);
        }
        Self {
            surfaces: BTreeMap::new(),
            buffers: BTreeMap::new(),
            next_id: 1,
            ipc_channel: chan,
        }
    }

    /// Register a new buffer from a client's SHM segment
    pub fn create_buffer(&mut self, _owner: Pid, shm_id: usize, w: u32, h: u32) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        self.buffers.insert(id, CompositorBuffer {
            shm_id,
            width: w,
            height: h,
            stride: w * 4,
        });
        id
    }

    /// Attach a buffer to a surface
    pub fn attach(&mut self, surface_id: usize, buffer_id: usize) -> Result<(), &'static str> {
        let surface = self.surfaces.get_mut(&surface_id).ok_or("Surface not found")?;
        if !self.buffers.contains_key(&buffer_id) {
            return Err("Buffer not found");
        }
        surface.active_buffer = Some(buffer_id);
        Ok(())
    }

    /// Compose all active surfaces into the primary DRM backbuffer
    pub fn compose(&self, target_fb: *mut u8, target_pitch: u32) {
        let mut sorted_keys: Vec<usize> = self.surfaces.keys().cloned().collect();
        // Simple sort by ID as proxy for Z-order for now
        sorted_keys.sort();

        for id in sorted_keys {
            if let Some(surface) = self.surfaces.get(&id) {
                if let Some(buf_id) = surface.active_buffer {
                    if let Some(buf) = self.buffers.get(&buf_id) {
                        if let Ok(shm_addr) = SHM.lock().attach(buf.shm_id as u32, Pid(0)) {
                            crate::zig_ffi::blit_bitmap(
                                target_fb,
                                target_pitch,
                                shm_addr as *const u8,
                                0,
                                0,
                                buf.width,
                                buf.height,
                                surface.x as u32,
                                surface.y as u32,
                            );
                        }
                    }
                }
            }
        }
    }

    /// Main loop for the compositor process
    pub fn run(&mut self) -> ! {
        crate::println!("[NWCC] Starting Native Wayland-Compatible Compositor");

        // 1. Initialize DRM and get display resources
        let (width, height, pitch) = {
            let drm = crate::drivers::drm::DRM.lock();
            let res = drm.get_resources();
            (res.width, res.height, res.width * 4)
        };

        // 2. Pre-register surface 1 for demo
        self.surfaces.insert(1, Surface {
            owner: Pid(0),
            active_buffer: None,
            x: 100, y: 100, z_index: 10,
        });

        // 3. Allocate a real back-buffer via DRM
        let mut back_buffer = alloc::vec![0u8; (width * height * 4) as usize].into_boxed_slice();
        let bb_ptr = back_buffer.as_mut_ptr();

        loop {
            // 3. Process IPC Commands from Clients
            self.process_ipc();

            // 4. Clear back-buffer using Zig-accelerated clear
            crate::zig_ffi::clear(bb_ptr, (width * height * 4) as usize, 0xFF000000); // Black

            // 5. Composite all client surfaces
            self.compose(bb_ptr, pitch);

            // 6. Draw Mouse Cursor (Axiq-IQ Native Enhancement)
            self.draw_cursor(bb_ptr, pitch);

            // 7. Trigger DRM Page Flip
            let mut fb_id: u32 = 1; // Default FB
            let _ = crate::drivers::drm::handle_ioctl(
                crate::drivers::drm::ioctl::MODE_PAGE_FLIP, 
                &mut fb_id as *mut u32 as *mut u8
            );

            // 8. VSync delay
            crate::timer::sleep_ms(Pid(0), 16); 
        }
    }

    /// Render a native Axiq-IQ mouse cursor
    fn draw_cursor(&self, target_fb: *mut u8, pitch: u32) {
        let (mx, my) = crate::drivers::ps2_mouse::get_mouse_pos();
        // Use Zig blitter to draw a fast white cursor arrow (simple rect for now)
        crate::zig_ffi::fill_rect(
            target_fb,
            pitch,
            mx as u32,
            my as u32,
            12,
            12,
            0xFFFFFFFF, // White
        );
        // Draw a small black border for contrast
        crate::zig_ffi::draw_line(target_fb, pitch, mx as u32, my as u32, (mx+12) as u32, (my+12) as u32, 0xFF000000);
    }

    /// Poll for IPC messages from clients
    fn process_ipc(&mut self) {
        if let Some(chan_id) = self.ipc_channel {
            // Attempt to receive a message (non-blocking)
            while let Ok(msg) = crate::ipc::recv(chan_id) {
                // Protocol Decoding: Treat msg.data as WlMessage
                if msg.len >= core::mem::size_of::<WlMessage>() {
                    let cmd = unsafe { core::ptr::read(msg.data.as_ptr() as *const WlMessage) };
                    self.handle_message(cmd);
                }
            }
        }
    }

    fn handle_message(&mut self, msg: WlMessage) {
        match msg {
            WlMessage::CreateSurface { owner } => {
                let id = self.next_id;
                self.next_id += 1;
                self.surfaces.insert(id, Surface {
                    owner,
                    active_buffer: None,
                    x: 0, y: 0, z_index: 0,
                });
                crate::println!("[NWCC] Surface {} created for PID {}", id, owner.0);
            }
            WlMessage::CreateBuffer { owner, shm_id, width, height } => {
                let id = self.create_buffer(owner, shm_id, width, height);
                crate::println!("[NWCC] Buffer {} created (SHM {})", id, shm_id);
            }
            WlMessage::Attach { surface_id, buffer_id } => {
                let _ = self.attach(surface_id, buffer_id);
            }
            WlMessage::SetPosition { surface_id, x, y } => {
                if let Some(s) = self.surfaces.get_mut(&surface_id) {
                    s.x = x;
                    s.y = y;
                }
            }
            WlMessage::Commit { surface_id: _ } => {
                // Mark for redraw (already handled by main loop frequency)
            }
        }
    }
}

pub fn start() -> ! {
    let mut compositor = CompositorState::new();
    compositor.run();
}
