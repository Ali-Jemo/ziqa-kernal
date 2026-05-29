/// Native Wayland-Compatible Compositor (NWCC) - Buffer Management
/// 
/// This module implements the "Plumbing" for the compositor:
/// 1. SHM-backed buffer allocation for clients.
/// 2. Surface-to-Buffer attachments.
/// 3. Integration with the Zig-accelerated blitter.
/// 4. Downsampled VGA output for legacy display support.

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
                            (surface.x + 4) as u32,
                            (surface.y + 4) as u32,
                            buf.width,
                            buf.height,
                            0x00000000, // Black
                            100,        // 40% opacity
                        );

                        // 2. Draw Window Border (Axiq-IQ Blue)
                        crate::zig_ffi::fill_rect(
                            target_fb,
                            target_pitch,
                            (surface.x - 1) as u32,
                            (surface.y - 1) as u32,
                            buf.width + 2,
                            buf.height + 2,
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
        // Mouse range is 0..1920, 0..1080. Map to 80x25 for hit testing
        let cur_x = mx * 80 / 1920;
        let cur_y = my * 25 / 1080;
        
        if self.grabbed_surface.is_none() {
            if let Some(id) = self.hit_test(cur_x, cur_y) {
                let s = self.surfaces.get(&id).unwrap();
                // Simple header grab
                if cur_y < s.y + 2 { 
                    self.grabbed_surface = Some(id);
                    self.grab_offset_x = cur_x - s.x;
                    self.grab_offset_y = cur_y - s.y;
                }
            }
        } else if let Some(id) = self.grabbed_surface {
            if let Some(s) = self.surfaces.get_mut(&id) {
                s.x = cur_x - self.grab_offset_x;
                s.y = cur_y - self.grab_offset_y;
                s.z_index = 100;
            }
            if cur_x < 0 || cur_y < 0 || cur_x > 79 { self.grabbed_surface = None; }
        }
    }

    /// Compose all active surfaces and blit to the physical VGA text screen
    pub fn run(&mut self) -> ! {
        crate::println!("[NWCC] Starting Native Wayland-Compatible Compositor (VGA-Downsampled)");

        // 1. Virtual Resolution: 80x25
        let width = 80;
        let height = 25;
        let pitch = width * 4;

        // 2. Pre-register surface 1 for demo
        self.surfaces.insert(1, Surface {
            owner: Pid(0),
            active_buffer: None,
            x: 10, y: 5, z_index: 10,
        });

        // 3. Allocate back-buffer (u32 pixels)
        let mut back_buffer = alloc::vec![0u32; (width * height) as usize].into_boxed_slice();
        let bb_ptr = back_buffer.as_mut_ptr() as *mut u8;

        loop {
            // 4. Interaction & IPC
            self.update_interaction();
            self.process_ipc();

            // 5. Render to Back-buffer
            crate::zig_ffi::clear(bb_ptr, (pitch * height) as usize, 0xFF111111); // Dark Gray
            self.compose(bb_ptr, pitch as u32);
            
            // Draw Cursor
            let (mx, my) = crate::drivers::ps2_mouse::get_mouse_pos();
            let cur_x = (mx * 80 / 1920) as u32;
            let cur_y = (my * 25 / 1080) as u32;
            crate::zig_ffi::fill_rect(bb_ptr, pitch as u32, cur_x, cur_y, 1, 1, 0xFFFFFFFF);

            // 6. Present to Physical VGA Screen
            present_to_vga(&back_buffer);

            // 7. Loop delay
            crate::timer::sleep_ms(Pid(0), 16); 
        }
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
            WlMessage::Commit { surface_id: _ } => {}
        }
    }
}

// ── Downsampling Helpers ───────────────────────────────────────────────────

fn present_to_vga(v_fb: &[u32]) {
    let offset = crate::BOOT_INFO.lock()
        .as_ref()
        .map(|bi| bi.physical_memory_offset)
        .unwrap_or(0);
    let vga_ptr = (offset + 0xb8000) as *mut u16;
    for y in 0..25 {
        for x in 0..80 {
            let val = v_fb[y * 80 + x];
            let char_val = if val == 0 {
                (b' ' as u16) | (0x00u16 << 8)
            } else if (val & 0xFF000000) == 0xFF000000 {
                let ascii = (val & 0xFF) as u8;
                let fg = ((val >> 8) & 0xFF) as u8;
                let bg = ((val >> 16) & 0xFF) as u8;
                (ascii as u16) | ((((bg << 4) | fg) as u16) << 8)
            } else {
                let vga_color = get_closest_vga_color(val);
                0xDBu16 | ((vga_color as u16) << 8)
            };
            unsafe {
                core::ptr::write_volatile(vga_ptr.add(y * 80 + x), char_val);
            }
        }
    }
}

fn get_closest_vga_color(color: u32) -> u8 {
    let vga_colors: [u32; 16] = [
        0x000000, 0x0000AA, 0x00AA00, 0x00AAAA,
        0xAA0000, 0xAA00AA, 0xAA5500, 0xAAAAAA,
        0x555555, 0x5555FF, 0x55FF55, 0x55FFFF,
        0xFF5555, 0xFF55FF, 0xFFFF55, 0xFFFFFF,
    ];
    let r = ((color >> 16) & 0xFF) as i32;
    let g = ((color >> 8) & 0xFF) as i32;
    let b = (color & 0xFF) as i32;
    let mut best_idx = 0;
    let mut min_dist = i32::MAX;
    for (i, &vc) in vga_colors.iter().enumerate() {
        let vr = ((vc >> 16) & 0xFF) as i32;
        let vg = ((vc >> 8) & 0xFF) as i32;
        let vb = (vc & 0xFF) as i32;
        let dist = (r - vr) * (r - vr) + (g - vg) * (g - vg) + (b - vb) * (b - vb);
        if dist < min_dist {
            min_dist = dist;
            best_idx = i;
        }
    }
    best_idx as u8
}

pub fn start() -> ! {
    let mut compositor = CompositorState::new();
    compositor.run();
}
