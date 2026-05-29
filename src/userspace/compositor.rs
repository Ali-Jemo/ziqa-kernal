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

    // Interaction state
    pub grabbed_surface: Option<usize>,
    pub grab_offset_x: i32,
    pub grab_offset_y: i32,
}

impl CompositorState {
    pub fn new() -> Self {
        // Use a well-known channel ID for the compositor (ID 1)
        let chan = 1;
        crate::println!("[NWCC] Initialized on well-known channel: {}", chan);
        
        Self {
            surfaces: BTreeMap::new(),
            buffers: BTreeMap::new(),
            next_id: 1,
            ipc_channel: Some(chan),
            grabbed_surface: None,
            grab_offset_x: 0,
            grab_offset_y: 0,
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

    /// Compose all active surfaces with shadows and borders
    pub fn compose(&self, target_fb: *mut u8, target_pitch: u32) {
        let mut sorted_keys: Vec<usize> = self.surfaces.keys().cloned().collect();
        // Sort by Z-index
        sorted_keys.sort_by_key(|id| self.surfaces.get(id).unwrap().z_index);

        for id in sorted_keys {
            if let Some(surface) = self.surfaces.get(&id) {
                if let Some(buf_id) = surface.active_buffer {
                    if let Some(buf) = self.buffers.get(&buf_id) {
                        // 1. Draw Shadow (Alpha-blended black rect)
                        crate::zig_ffi::blend_rect(
                            target_fb,
                            target_pitch,
                            (surface.x + 8) as u32,
                            (surface.y + 8) as u32,
                            buf.width,
                            buf.height,
                            0x00000000, // Black
                            100,        // 40% opacity
                        );

                        // 2. Draw Window Border (Axiq-IQ Blue)
                        crate::zig_ffi::fill_rect(
                            target_fb,
                            target_pitch,
                            (surface.x - 2) as u32,
                            (surface.y - 2) as u32,
                            buf.width + 4,
                            buf.height + 4,
                            0x003366FF, // Axiq Blue
                        );

                        // 3. Draw Surface Content
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

    /// Find which surface is under the mouse (Z-index aware)
    fn hit_test(&self, x: i32, y: i32) -> Option<usize> {
        let mut sorted_keys: Vec<usize> = self.surfaces.keys().cloned().collect();
        sorted_keys.sort_by_key(|id| -self.surfaces.get(id).unwrap().z_index);

        for id in sorted_keys {
            if let Some(surface) = self.surfaces.get(&id) {
                if let Some(buf_id) = surface.active_buffer {
                    if let Some(buf) = self.buffers.get(&buf_id) {
                        if x >= surface.x && x < surface.x + buf.width as i32 &&
                           y >= surface.y && y < surface.y + buf.height as i32 {
                            return Some(id);
                        }
                    }
                }
            }
        }
        None
    }

    /// Update mouse interaction logic
    fn update_interaction(&mut self) {
        let (mx, my) = crate::drivers::ps2_mouse::get_mouse_pos();
        
        if self.grabbed_surface.is_none() {
            // Check for new grab (top 20px header)
            if let Some(id) = self.hit_test(mx, my) {
                let s = self.surfaces.get(&id).unwrap();
                if my < s.y + 20 { 
                    self.grabbed_surface = Some(id);
                    self.grab_offset_x = mx - s.x;
                    self.grab_offset_y = my - s.y;
                }
            }
        } else if let Some(id) = self.grabbed_surface {
            // Update position of grabbed window
            if let Some(s) = self.surfaces.get_mut(&id) {
                s.x = mx - self.grab_offset_x;
                s.y = my - self.grab_offset_y;
                s.z_index = 100; // Bring to front
            }
            // Release if mouse leaves screen area (demo hack)
            if mx < 0 || my < 0 || mx > 1900 { self.grabbed_surface = None; }
        }
    }

    /// Compose all active surfaces and blit to the HARDWARE framebuffer
    pub fn run(&mut self) -> ! {
        crate::println!("[NWCC] Starting Native Wayland-Compatible Compositor");

        // 1. Get hardware framebuffer info
        let (hw_ptr, width, _height, pitch) = {
            let fb_lock = crate::drivers::framebuffer::FB.lock();
            let fb = fb_lock.as_ref().expect("Framebuffer not initialized");
            (fb.ptr, fb.width as u32, fb.height as u32, fb.pitch as u32)
        };
        // Height is fixed at 1080 for this 32MiB heap allocation
        let height = 1080;

        // 2. Pre-register surface 1 for demo
        self.surfaces.insert(1, Surface {
            owner: Pid(0),
            active_buffer: None,
            x: 50, y: 50, z_index: 10,
        });

        // 3. Allocate back-buffer
        let mut back_buffer = alloc::vec![0u8; (pitch * height) as usize].into_boxed_slice();
        let bb_ptr = back_buffer.as_mut_ptr();

        loop {
            // 4. Interaction & IPC
            self.update_interaction();
            self.process_ipc();

            // 5. Render to Back-buffer
            crate::zig_ffi::clear(bb_ptr, (pitch * height) as usize, 0xFF111111); // Gray
            self.compose(bb_ptr, pitch);
            self.draw_cursor(bb_ptr, pitch);

            // 6. HARDWARE SYNC: Copy back-buffer to actual screen
            crate::zig_ffi::memcpy(hw_ptr, bb_ptr, (pitch * height) as usize);

            // 7. VSync delay
            crate::timer::sleep_ms(Pid(0), 16); 
        }
    }

    /// Render a native Axiq-IQ mouse cursor
    fn draw_cursor(&self, target_fb: *mut u8, pitch: u32) {
        let (mx, my) = crate::drivers::ps2_mouse::get_mouse_pos();
        crate::zig_ffi::fill_rect(
            target_fb,
            pitch,
            mx as u32,
            my as u32,
            12,
            12,
            0xFFFFFFFF, // White
        );
        crate::zig_ffi::draw_line(target_fb, pitch, mx as u32, my as u32, (mx+12) as u32, (my+12) as u32, 0xFF000000);
    }

    /// Poll for IPC messages from clients
    fn process_ipc(&mut self) {
        if let Some(chan_id) = self.ipc_channel {
            while let Ok(msg) = crate::ipc::recv(chan_id) {
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
                // Redraw is continuous
            }
        }
    }
}

pub fn start() -> ! {
    let mut compositor = CompositorState::new();
    compositor.run();
}
