//! Kernel-mode compositor with surface management and damage tracking.
//!
//! Manages SHM-backed surfaces, tracks dirty regions, and composites
//! them to the GPU framebuffer. Runs as a kernel thread.

use alloc::vec::Vec;
use core::ptr;

/// A rectangle with position and size.
#[derive(Clone, Copy, Default)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

impl Rect {
    pub fn is_empty(self) -> bool {
        self.w == 0 || self.h == 0
    }

    pub fn union(self, other: Rect) -> Rect {
        if self.is_empty() {
            return other;
        }
        if other.is_empty() {
            return self;
        }
        let x1 = self.x.min(other.x);
        let y1 = self.y.min(other.y);
        let x2 = (self.x + self.w).max(other.x + other.w);
        let y2 = (self.y + self.h).max(other.y + other.h);
        Rect {
            x: x1,
            y: y1,
            w: x2 - x1,
            h: y2 - y1,
        }
    }

    pub fn intersect(self, bounds: Rect) -> Rect {
        if self.is_empty() || bounds.is_empty() {
            return Rect::default();
        }
        let x1 = self.x.max(bounds.x);
        let y1 = self.y.max(bounds.y);
        let x2 = (self.x + self.w).min(bounds.x + bounds.w);
        let y2 = (self.y + self.h).min(bounds.y + bounds.h);
        if x2 <= x1 || y2 <= y1 {
            Rect::default()
        } else {
            Rect {
                x: x1,
                y: y1,
                w: x2 - x1,
                h: y2 - y1,
            }
        }
    }
}

/// A render surface backed by SHM.
pub struct Surface {
    pub id: u32,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub shm_id: u32,
    /// Kernel-virtual address of the SHM mapping.
    pub shm_addr: u64,
    pub dirty: bool,
    pub dirty_rect: Rect,
}

/// The compositor state machine — surface list, damage tracking, compositing.
pub struct Compositor {
    pub surfaces: Vec<Surface>,
    pub damage: Rect,
    pub next_id: u32,
    pub focused_surface_id: u32,
}

impl Compositor {
    pub fn new() -> Self {
        Compositor {
            surfaces: Vec::new(),
            damage: Rect::default(),
            next_id: 1,
            focused_surface_id: 0,
        }
    }

    /// Create a new surface and return its ID. The entire surface is marked dirty.
    pub fn create_surface(
        &mut self,
        width: u32,
        height: u32,
        shm_id: u32,
        shm_addr: u64,
    ) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        // Surface starts clean — shm_addr is set later via BufferAttach
        self.surfaces.push(Surface {
            id,
            x: 0,
            y: 0,
            width,
            height,
            shm_id,
            shm_addr,
            dirty: shm_addr != 0,
            dirty_rect: if shm_addr != 0 { Rect { x: 0, y: 0, w: width, h: height } } else { Rect::default() },
        });
        if shm_addr != 0 {
            let sr = Rect { x: 0, y: 0, w: width, h: height };
            self.damage = self.damage.union(sr);
        }
        id
    }

    /// Remove a surface and mark its area as damaged.
    pub fn destroy_surface(&mut self, id: u32) {
        if let Some(pos) = self.surfaces.iter().position(|s| s.id == id) {
            let s = &self.surfaces[pos];
            if s.x >= 0 && s.y >= 0 {
                let sr = Rect {
                    x: s.x as u32,
                    y: s.y as u32,
                    w: s.width,
                    h: s.height,
                };
                self.damage = self.damage.union(sr);
            }
            self.surfaces.remove(pos);
        }
    }

    /// Move a surface and mark both old and new position as dirty.
    pub fn set_position(&mut self, id: u32, x: i32, y: i32) {
        if let Some(s) = self.surfaces.iter_mut().find(|s| s.id == id) {
            if s.x >= 0 && s.y >= 0 {
                let old = Rect {
                    x: s.x as u32,
                    y: s.y as u32,
                    w: s.width,
                    h: s.height,
                };
                self.damage = self.damage.union(old);
            }
            s.x = x;
            s.y = y;
            if x >= 0 && y >= 0 {
                let new = Rect {
                    x: x as u32,
                    y: y as u32,
                    w: s.width,
                    h: s.height,
                };
                self.damage = self.damage.union(new);
            }
        }
    }

    /// Mark a rectangular region of a surface as dirty.
    /// The rect is in surface-local coordinates and is clamped to surface bounds.
    pub fn mark_dirty(&mut self, id: u32, rect: Rect) {
        if let Some(s) = self.surfaces.iter_mut().find(|s| s.id == id) {
            let bounds = Rect {
                x: 0,
                y: 0,
                w: s.width,
                h: s.height,
            };
            let clipped = rect.intersect(bounds);
            if clipped.is_empty() {
                return;
            }
            s.dirty = true;
            s.dirty_rect = s.dirty_rect.union(clipped);
        }
    }

    /// Composite all dirty surfaces onto `fb`, then clear damage.
    /// Clears damaged regions to dark blue before drawing surfaces.
    /// `fb` is an RGBA 32-bpp framebuffer.
    pub fn composite(&mut self, fb: &mut [u8], fb_w: u32, fb_h: u32, fb_bpp: u32) {
        if self.damage.is_empty() && self.surfaces.iter().all(|s| !s.dirty) {
            return;
        }
        let bpp = (fb_bpp / 8) as usize;
        let fb_stride = fb_w as usize * bpp;

        // Expand damage to cover all per-surface dirty rects too
        for s in &self.surfaces {
            if s.dirty && !s.dirty_rect.is_empty() && s.shm_addr != 0 && s.x >= 0 && s.y >= 0 {
                let world = Rect {
                    x: (s.x as u32).wrapping_add(s.dirty_rect.x),
                    y: (s.y as u32).wrapping_add(s.dirty_rect.y),
                    w: s.dirty_rect.w,
                    h: s.dirty_rect.h,
                };
                // Clamp to fb
                let fb_clip = Rect { x: 0, y: 0, w: fb_w, h: fb_h };
                let clipped = world.intersect(fb_clip);
                if !clipped.is_empty() {
                    // Directly mutate damage — we own self here
                    self.damage = self.damage.union(clipped);
                }
            }
        }

        if self.damage.is_empty() {
            return;
        }
        // Pass 0 — clear the damage region to dark blue (background)
        let (bg_r, bg_g, bg_b) = (0x10u8, 0x10u8, 0x40u8);
        for row in self.damage.y..self.damage.y + self.damage.h {
            if row >= fb_h { break; }
            let dst_off = (row as usize) * fb_stride + (self.damage.x as usize) * bpp;
            let n = self.damage.w as usize * bpp;
            if dst_off + n <= fb.len() {
                // Fill row with solid color (3 or 4 bytes per pixel)
                let mut p = dst_off;
                while p < dst_off + n {
                    fb[p] = bg_r;
                    fb[p + 1] = bg_g;
                    fb[p + 2] = bg_b;
                    if bpp >= 4 { fb[p + 3] = 0xFF; }
                    p += bpp;
                }
            }
        }

        // Pass 1 — composite every dirty surface on top of the background
        for s in &self.surfaces {
            if !s.dirty || s.dirty_rect.is_empty() || s.shm_addr == 0 {
                continue;
            }

            let src_stride = s.width as usize * 4;
            let draw_x = s.x.max(0) as u32;
            let draw_y = s.y.max(0) as u32;

            // Clamp dirty area to the visible intersection
            let clip = Rect {
                x: 0, y: 0,
                w: s.width.min(fb_w.saturating_sub(draw_x)),
                h: s.height.min(fb_h.saturating_sub(draw_y)),
            };
            let da = s.dirty_rect.intersect(clip);
            if da.is_empty() { continue; }

            let src_base = s.shm_addr as usize;
            for row in da.y..da.y + da.h {
                let fb_y = draw_y + row;
                if fb_y >= fb_h { break; }
                let src_off = (row as usize) * src_stride + (da.x as usize) * 4;
                let dst_off = (fb_y as usize) * fb_stride + ((draw_x + da.x) as usize) * bpp;
                let n = da.w as usize * 4;
                if dst_off + n <= fb.len() {
                    unsafe {
                        ptr::copy_nonoverlapping(
                            src_base.wrapping_add(src_off) as *const u8,
                            fb.as_mut_ptr().add(dst_off),
                            n,
                        );
                    }
                }
            }
        }

        // Pass 2 — clear all per-surface dirty flags
        for s in &mut self.surfaces {
            s.dirty = false;
            s.dirty_rect = Rect::default();
        }
        self.damage = Rect::default();
    }
}

// ── Compositor kernel thread entry point ──────────────────────────────────

/// Kernel-thread entry for the compositor main loop.
/// Runs in kernel mode — accesses GPU globals and IPC directly.
pub fn compositor_main(_arg: *const ()) {
    // 1. Get GPU IPC channel (optional, only needed for VirtIO GPU)
    let gpu_chan = *crate::drivers::virtio_gpu::GPU_IPC_CHANNEL.lock();

    // 2. Get framebuffer info
    let (fb_virt, fb_w, fb_h, _bpp) =
        match crate::drivers::virtio_gpu::get_fb_info() {
            Some(info) => info,
            None => {
                crate::klog!(
                    crate::klog::Level::Error,
                    "[Compositor] Framebuffer not available"
                );
                return;
            }
        };

    crate::klog!(
        crate::klog::Level::Info,
        "[Compositor] started on channel 3, fb={:p} {}x{}",
        fb_virt as *const u8,
        fb_w,
        fb_h,
    );

    let fb_slice = unsafe {
        core::slice::from_raw_parts_mut(
            fb_virt as *mut u8,
            (fb_w * fb_h * 4) as usize,
        )
    };

    let mut comp = Compositor::new();
    const COMPOSITOR_CHAN: u32 = 3;

    loop {
        // 3a. Poll client channel (channel 3) — non-blocking
        while let Ok(msg) = crate::ipc::recv(COMPOSITOR_CHAN) {
            if msg.len < 1 {
                continue;
            }
            let op = msg.data[0];
            match op {
                1 => {
                    // Connect — just log
                    crate::klog!(
                        crate::klog::Level::Info,
                        "[Compositor] Client connected"
                    );
                }
                2 => {
                    // CreateSurface
                    if msg.len >= 1 + core::mem::size_of::<crate::ipc::gui::CreateSurfaceMsg>()
                    {
                        let payload = unsafe {
                            core::ptr::read_unaligned(
                                msg.data.as_ptr().add(1) as *const crate::ipc::gui::CreateSurfaceMsg,
                            )
                        };
                        let sid = comp.create_surface(payload.width, payload.height, 0, 0);
                        crate::klog!(
                            crate::klog::Level::Info,
                            "[Compositor] Surface {} created ({}x{})",
                            sid,
                            payload.width,
                            payload.height,
                        );
                    }
                }
                3 => {
                    // Flush — mark a surface region as dirty
                    if msg.len >= 1 + core::mem::size_of::<crate::ipc::gui::FlushMsg>()
                    {
                        let payload = unsafe {
                            core::ptr::read_unaligned(
                                msg.data.as_ptr().add(1) as *const crate::ipc::gui::FlushMsg,
                            )
                        };
                        comp.mark_dirty(
                            payload.surface_id,
                            Rect {
                                x: payload.x,
                                y: payload.y,
                                w: payload.width,
                                h: payload.height,
                            },
                        );
                    }
                }
                5 => {
                    // BufferAttach — map SHM for compositor access
                    if msg.len >= 1 + core::mem::size_of::<crate::ipc::gui::BufferAttachMsg>()
                    {
                        let payload = unsafe {
                            core::ptr::read_unaligned(
                                msg.data.as_ptr().add(1) as *const crate::ipc::gui::BufferAttachMsg,
                            )
                        };
                        let shm = crate::ipc::shm::SHM.lock();
                        if let Ok(addr) = shm.attach(payload.shm_id, crate::process::Pid(0)) {
                            crate::klog!(
                                crate::klog::Level::Info,
                                "[Compositor] BufferAttach surface={} shm={} addr=0x{:x}",
                                payload.surface_id,
                                payload.shm_id,
                                addr,
                            );
                            if let Some(s) =
                                comp.surfaces.iter_mut().find(|s| s.id == payload.surface_id)
                            {
                                s.shm_id = payload.shm_id;
                                s.shm_addr = addr;
                                s.dirty = true;
                                s.dirty_rect = Rect {
                                    x: 0, y: 0,
                                    w: s.width, h: s.height,
                                };
                            }
                        } else {
                            crate::klog!(
                                crate::klog::Level::Warn,
                                "[Compositor] BufferAttach failed for shm {}",
                                payload.shm_id,
                            );
                        }
                    }
                }
                6 => {
                    // SetPosition — reposition a surface
                    if msg.len >= 1 + core::mem::size_of::<crate::ipc::gui::SetPositionMsg>()
                    {
                        let payload = unsafe {
                            core::ptr::read_unaligned(
                                msg.data.as_ptr().add(1) as *const crate::ipc::gui::SetPositionMsg,
                            )
                        };
                        comp.set_position(payload.surface_id, payload.x, payload.y);
                        crate::klog!(
                            crate::klog::Level::Debug,
                            "[Compositor] Surface {} moved to ({},{})",
                            payload.surface_id,
                            payload.x,
                            payload.y,
                        );
                    }
                }
                _ => {}
            }
        }

        // 3b. Poll keyboard state (ISR-safe atomic) and forward to event channel
        let key_val = crate::drivers::keyboard::poll_compositor_key();
        if key_val != 0 {
            let kind = if key_val & 0x100 != 0 { 1u8 } else { 2u8 };
            let code = (key_val & 0xFF) as u32;
            let input = crate::ipc::gui::InputMsg {
                kind,
                code,
                x: 0,
                y: 0,
            };
            let msg_bytes = unsafe {
                core::slice::from_raw_parts(
                    &input as *const crate::ipc::gui::InputMsg as *const u8,
                    core::mem::size_of::<crate::ipc::gui::InputMsg>(),
                )
            };
            let mut buf = [0u8; 256];
            buf[0] = crate::ipc::gui::OpCode::Input as u8;
            let n = msg_bytes.len().min(255);
            buf[1..1 + n].copy_from_slice(&msg_bytes[..n]);
            let _ = crate::ipc::send(4, crate::process::Pid(0), &buf[..1 + n]);
        }

        // 3c. Poll mouse state and forward to event channel
        let (mx, my) = crate::drivers::ps2_mouse::get_mouse_pos();
        let mb = crate::drivers::ps2_mouse::get_mouse_btn();
        if mb != 0 {
            let input = crate::ipc::gui::InputMsg {
                kind: 2, // Mouse
                code: mb as u32,
                x: mx,
                y: my,
            };
            let msg_bytes = unsafe {
                core::slice::from_raw_parts(
                    &input as *const crate::ipc::gui::InputMsg as *const u8,
                    core::mem::size_of::<crate::ipc::gui::InputMsg>(),
                )
            };
            let mut buf = [0u8; 256];
            buf[0] = crate::ipc::gui::OpCode::Input as u8;
            let n = msg_bytes.len().min(255);
            buf[1..1 + n].copy_from_slice(&msg_bytes[..n]);
            let _ = crate::ipc::send(4, crate::process::Pid(0), &buf[..1 + n]);
        }

        // Log keyboard events for now
        if key_val != 0 {
            if key_val & 0x100 != 0 {
                crate::klog!(
                    crate::klog::Level::Debug,
                    "[Compositor] Key: '{}' → channel 4",
                    (key_val & 0xFF) as u8 as char,
                );
            } else {
                crate::klog!(
                    crate::klog::Level::Debug,
                    "[Compositor] Key: 0x{:02x} → channel 4",
                    key_val as u8,
                );
            }
        }

        // 3d. Composite dirty regions → background clear + surface blit
        comp.composite(fb_slice, fb_w, fb_h, 32);

        // If no surfaces, draw a fallback animated gradient so the display
        // isn't just blank dark blue. The tick is incremented each frame.
        if comp.surfaces.is_empty() {
            // Composite cleared to dark blue; overdraw with animated pattern
            static TICK: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);
            let tick = TICK.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
            draw_fallback_pattern(fb_slice, fb_w, fb_h, tick);
        }

        // 3e. Always flush to GPU (shows background even with no surfaces)
        if let Some(chan) = gpu_chan {
            let flush_cmd = [1u8];
            let _ = crate::ipc::send(chan, crate::process::Pid(0), &flush_cmd);
        }
        // 3f. Yield to other tasks
        crate::process::scheduler::yield_now();
    }
}

/// Draw a small 100x100 animated square in the top-left corner
/// to avoid slowing down QEMU emulation with a full 1024x768 per-pixel loop.
fn draw_fallback_pattern(fb: &mut [u8], w: u32, h: u32, tick: u32) {
    let stride = (w * 4) as usize;
    let limit_y = 100.min(h);
    let limit_x = 100.min(w);
    for y in 0..limit_y {
        for x in 0..limit_x {
            let r = ((x + tick) & 0xFF) as u8;
            let g = ((y + tick) & 0xFF) as u8;
            let b = ((x + y + tick) & 0xFF) as u8;
            let off = (y as usize) * stride + (x as usize) * 4;
            if off + 3 < fb.len() {
                fb[off] = b;
                fb[off + 1] = g;
                fb[off + 2] = r;
                fb[off + 3] = 0xFF;
            }
        }
    }
}
