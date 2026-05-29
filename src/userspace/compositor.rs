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
use crate::drivers::drm::FramebufferId;

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

pub struct CompositorState {
    pub surfaces: BTreeMap<usize, Surface>,
    pub buffers: BTreeMap<usize, CompositorBuffer>,
    pub next_id: usize,
}

impl CompositorState {
    pub fn new() -> Self {
        Self {
            surfaces: BTreeMap::new(),
            buffers: BTreeMap::new(),
            next_id: 1,
        }
    }

    /// Register a new buffer from a client's SHM segment
    pub fn create_buffer(&mut self, owner: Pid, shm_id: usize, w: u32, h: u32) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        
        self.buffers.insert(id, CompositorBuffer {
            shm_id,
            width: w,
            height: h,
            stride: w * 4, // XRGB8888
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
        // Sort surfaces by Z-index before rendering
        let mut sorted_surfaces: Vec<_> = self.surfaces.values().collect();
        sorted_surfaces.sort_by_key(|s| s.z_index);

        for surface in sorted_surfaces {
            if let Some(buf_id) = surface.active_buffer {
                if let Some(buf) = self.buffers.get(&buf_id) {
                    // 1. Get SHM address for this compositor process
                    // (Assuming the compositor is a privileged process that can attach any SHM)
                    if let Ok(shm_addr) = SHM.lock().attach(buf.shm_id, Pid(0)) {
                        // 2. Use Zig blitter to blend the surface into the target
                        // ARCH: [compositor->zig] Blend client buffer to screen
                        crate::zig_ffi::blend_rect(
                            target_fb,
                            target_pitch,
                            surface.x as u32,
                            surface.y as u32,
                            shm_addr as *const u8,
                            buf.stride,
                            buf.width,
                            buf.height,
                        );
                    }
                }
            }
        }
    }
}
