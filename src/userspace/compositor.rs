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
    pub event_channel_id: u32,
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

    /// Create a new surface with a specified ID. The entire surface is marked dirty.
    pub fn create_surface(&mut self, id: u32, width: u32, height: u32, shm_id: u32, shm_addr: u64) -> u32 {
        // Surface starts clean — shm_addr is set later via BufferAttach
        self.surfaces.push(Surface {
            id,
            x: 0,
            y: 0,
            width,
            height,
            shm_id,
            shm_addr,
            event_channel_id: 0,
            dirty: shm_addr != 0,
            dirty_rect: if shm_addr != 0 {
                Rect {
                    x: 0,
                    y: 0,
                    w: width,
                    h: height,
                }
            } else {
                Rect::default()
            },
        });
        if shm_addr != 0 {
            let sr = Rect {
                x: 0,
                y: 0,
                w: width,
                h: height,
            };
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

    /// Raise a surface to the front and repaint its visible area.
    pub fn raise_surface(&mut self, id: u32) {
        if let Some(pos) = self.surfaces.iter().position(|s| s.id == id) {
            if should_raise_surface(pos, self.surfaces.len()) {
                if self.surfaces[pos].x >= 0 && self.surfaces[pos].y >= 0 {
                    self.damage = self.damage.union(Rect {
                        x: self.surfaces[pos].x as u32,
                        y: self.surfaces[pos].y as u32,
                        w: self.surfaces[pos].width,
                        h: self.surfaces[pos].height,
                    });
                }
                let surface = self.surfaces.remove(pos);
                self.surfaces.push(surface);
            }
        }
    }
    /// Move a surface one step down in z-order (behind its neighbor).
    pub fn lower_surface(&mut self, id: u32) {
        if let Some(pos) = self.surfaces.iter().position(|s| s.id == id) {
            if pos > 0 {
                // Mark both current and new position as damaged
                if self.surfaces[pos].x >= 0 && self.surfaces[pos].y >= 0 {
                    self.damage = self.damage.union(Rect {
                        x: self.surfaces[pos].x as u32,
                        y: self.surfaces[pos].y as u32,
                        w: self.surfaces[pos].width,
                        h: self.surfaces[pos].height,
                    });
                }
                self.surfaces.swap(pos, pos - 1);
                if self.surfaces[pos].x >= 0 && self.surfaces[pos].y >= 0 {
                    self.damage = self.damage.union(Rect {
                        x: self.surfaces[pos].x as u32,
                        y: self.surfaces[pos].y as u32,
                        w: self.surfaces[pos].width,
                        h: self.surfaces[pos].height,
                    });
                }
            }
        }
    }

    /// Resize a surface. Marks old and new bounds as damaged.
    /// The caller must re-allocate SHM and re-attach the buffer.
    pub fn resize_surface(&mut self, id: u32, new_w: u32, new_h: u32) {
        if let Some(s) = self.surfaces.iter_mut().find(|s| s.id == id) {
            // Mark old bounds as damaged
            if s.x >= 0 && s.y >= 0 {
                self.damage = self.damage.union(Rect {
                    x: s.x as u32,
                    y: s.y as u32,
                    w: s.width,
                    h: s.height,
                });
            }
            s.width = new_w;
            s.height = new_h;
            s.dirty = true;
            s.dirty_rect = Rect { x: 0, y: 0, w: new_w, h: new_h };
            // Mark new bounds as damaged
            if s.x >= 0 && s.y >= 0 {
                self.damage = self.damage.union(Rect {
                    x: s.x as u32,
                    y: s.y as u32,
                    w: new_w,
                    h: new_h,
                });
            }
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
    /// Skips clearing when opaque surfaces fully cover the damage region.
    /// `fb` is an RGBA 32-bpp framebuffer.
    pub fn composite(&mut self, fb: &mut [u8], fb_w: u32, fb_h: u32, fb_bpp: u32) -> bool {
        if self.damage.is_empty() && self.surfaces.iter().all(|s| !s.dirty) {
            return false;
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
                let fb_clip = Rect {
                    x: 0,
                    y: 0,
                    w: fb_w,
                    h: fb_h,
                };
                let clipped = world.intersect(fb_clip);
                if !clipped.is_empty() {
                    self.damage = self.damage.union(clipped);
                }
            }
        }

        if self.damage.is_empty() {
            return false;
        }

        // ponytail: skip clear when an opaque surface covers the damage region.
        // Clean surfaces count too; cursor damage still needs underlying pixels redrawn.
        let mut need_clear = true;
        for s in &self.surfaces {
            if s.shm_addr != 0 && s.x >= 0 && s.y >= 0 {
                let sx = s.x as u32;
                let sy = s.y as u32;
                // Surface world rect covers damage?
                if sx <= self.damage.x
                    && sy <= self.damage.y
                    && sx + s.width >= self.damage.x + self.damage.w
                    && sy + s.height >= self.damage.y + self.damage.h
                {
                    need_clear = false;
                    break;
                }
            }
        }

        if need_clear {
            // Repaint exposed wallpaper in the damage region
            draw_wallpaper(fb, fb_w, fb_h, &self.damage);
        }

        // Repaint every visible surface intersecting damage, not just dirty ones.
        // Cursor movement creates damage without making the surface dirty.
        let fb_clip = Rect {
            x: 0,
            y: 0,
            w: fb_w,
            h: fb_h,
        };
        for s in &self.surfaces {
            if s.shm_addr == 0 || s.x < 0 || s.y < 0 {
                continue;
            }
            let src_stride = s.width as usize * 4;
            let sx = s.x as u32;
            let sy = s.y as u32;

            // Draw shadow for this surface, clipped to damage rect
            draw_shadow(fb, fb_w, fb_h, s.x, s.y, s.width, s.height, &self.damage);

            let surface_world = Rect {
                x: sx,
                y: sy,
                w: s.width,
                h: s.height,
            }
            .intersect(fb_clip);
            let repaint = surface_world.intersect(self.damage);
            if repaint.is_empty() {
                continue;
            }

            let src_base = s.shm_addr as usize;
            for row in 0..repaint.h {
                let fb_y = repaint.y + row;
                let src_y = repaint.y - sy + row;
                let src_x = repaint.x - sx;
                let src_off = (src_y as usize) * src_stride + (src_x as usize) * 4;
                let dst_off = (fb_y as usize) * fb_stride + (repaint.x as usize) * bpp;
                let n = repaint.w as usize * 4;
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
        true
    }
}

// ── Compositor kernel thread entry point ──────────────────────────────────

/// Kernel-thread entry for the compositor main loop.
/// Runs in kernel mode — accesses GPU globals and IPC directly.
pub fn compositor_main(_arg: *const ()) {
    // 1. Get framebuffer info (try VirtIO GPU, fall back to BGA)
    // 1b. Get GPU IPC channel (optional, only needed for VirtIO GPU)
    let gpu_chan = *crate::drivers::virtio_gpu::GPU_IPC_CHANNEL.lock();

    // 2. Get framebuffer info
    let (fb_virt, fb_w, fb_h, _bpp) = match crate::drivers::virtio_gpu::get_fb_info() {
        Some(info) => info,
        None => {
            // Fall back to BGA framebuffer
            match crate::drivers::framebuffer::get_bga_fb_info() {
                Some(info) => info,
                None => {
                    crate::klog!(
                        crate::klog::Level::Error,
                        "[Compositor] Framebuffer not available (neither VirtIO GPU nor BGA)"
                    );
                    return;
                }
            }
        }
    };

    crate::klog!(
        crate::klog::Level::Info,
        "[Compositor] started on channel 3, fb={:p} {}x{}",
        fb_virt as *const u8,
        fb_w,
        fb_h,
    );

    let fb_slice =
        unsafe { core::slice::from_raw_parts_mut(fb_virt as *mut u8, (fb_w * fb_h * 4) as usize) };

    let mut comp = Compositor::new();
    let mut event_chan: Option<u32> = None;
    let mut last_mouse_x = i32::MIN;
    let mut last_mouse_y = i32::MIN;
    let mut last_mouse_btn = u8::MAX;
    comp.damage = Rect {
        x: 0,
        y: 0,
        w: fb_w,
        h: fb_h,
    };
    const COMPOSITOR_CHAN: u32 = 3;
    // ponytail: 60fps cap (~16ms). Prevents hot-spin that starves QEMU.
    const FRAME_MS: u64 = 16;

    // ponytail: simple diagnostics once per second
    static LAST_DIAG_LOG: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
    static DIAG_FLUSH_COUNT: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

    // Get our PID for sleep_ms (kernel thread has a scheduler PID)
    let my_pid =
        crate::process::scheduler::with_current_task(|p| p.pid).unwrap_or(crate::process::Pid(0));

    // Wallpaper is drawn once then only the damaged parts are repainted
    let mut wallpaper_drawn = false;
    // Window drag state: when mouse is held on a surface, track drag offset
    let mut drag_surface_id: u32 = 0;
    let mut drag_offset_x: i32 = 0;
    let mut drag_offset_y: i32 = 0;


    loop {
        let frame_start = crate::timer::uptime_ms();
        // Track whether anything needs a redraw this frame
        let mut needs_redraw = false;

        // 3a. Poll client channel (channel 3) — non-blocking, drain all
        while let Ok(msg) = crate::ipc::recv(COMPOSITOR_CHAN) {
            if msg.len < 1 {
                continue;
            }
            needs_redraw = true;
            let op = msg.data[0];
            match op {
                1 => {
                    crate::klog!(crate::klog::Level::Info, "[Compositor] Client connected");
                }
                2 => {
                    // CreateSurface
                    if msg.len >= 1 + core::mem::size_of::<crate::ipc::gui::CreateSurfaceMsg>() {
                        let payload = unsafe {
                            core::ptr::read_unaligned(msg.data.as_ptr().add(1)
                                as *const crate::ipc::gui::CreateSurfaceMsg)
                        };
                        let sid = comp.create_surface(payload.surface_id, payload.width, payload.height, 0, 0);
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
                    if msg.len >= 1 + core::mem::size_of::<crate::ipc::gui::FlushMsg>() {
                        let payload = unsafe {
                            core::ptr::read_unaligned(
                                msg.data.as_ptr().add(1) as *const crate::ipc::gui::FlushMsg
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
                    // BufferAttach
                    if msg.len >= 1 + core::mem::size_of::<crate::ipc::gui::BufferAttachMsg>() {
                        let payload = unsafe {
                            core::ptr::read_unaligned(
                                msg.data.as_ptr().add(1) as *const crate::ipc::gui::BufferAttachMsg
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
                            if let Some(s) = comp
                                .surfaces
                                .iter_mut()
                                .find(|s| s.id == payload.surface_id)
                            {
                                s.shm_id = payload.shm_id;
                                s.shm_addr = addr;
                                s.dirty = true;
                                s.dirty_rect = Rect {
                                    x: 0,
                                    y: 0,
                                    w: s.width,
                                    h: s.height,
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
                    // SetPosition
                    if msg.len >= 1 + core::mem::size_of::<crate::ipc::gui::SetPositionMsg>() {
                        let payload = unsafe {
                            core::ptr::read_unaligned(
                                msg.data.as_ptr().add(1) as *const crate::ipc::gui::SetPositionMsg
                            )
                        };
                        comp.set_position(payload.surface_id, payload.x, payload.y);
                    }
                }
                7 => {
                    // RegisterEventChannel
                    if msg.len
                        >= 1 + core::mem::size_of::<crate::ipc::gui::RegisterEventChannelMsg>()
                    {
                        let payload = unsafe {
                            core::ptr::read_unaligned(msg.data.as_ptr().add(1)
                                as *const crate::ipc::gui::RegisterEventChannelMsg)
                        };
                        if payload.surface_id == 0 {
                            event_chan = Some(payload.event_channel_id);
                        } else if let Some(s) = comp
                            .surfaces
                            .iter_mut()
                            .find(|s| s.id == payload.surface_id)
                        {
                            s.event_channel_id = payload.event_channel_id;
                        } else {
                            event_chan = Some(payload.event_channel_id);
                        }
                    }
                }
                8 => {
                    // Resize
                    if msg.len >= 1 + core::mem::size_of::<crate::ipc::gui::ResizeMsg>() {
                        let payload = unsafe {
                            core::ptr::read_unaligned(
                                msg.data.as_ptr().add(1) as *const crate::ipc::gui::ResizeMsg
                            )
                        };
                        crate::klog!(
                            crate::klog::Level::Info,
                            "[Compositor] Resize surface={} -> {}x{}",
                            payload.surface_id, payload.width, payload.height,
                        );
                        comp.resize_surface(payload.surface_id, payload.width, payload.height);
                    }
                }
                9 => {
                    // DestroySurface
                    if msg.len >= 1 + core::mem::size_of::<crate::ipc::gui::DestroySurfaceMsg>() {
                        let payload = unsafe {
                            core::ptr::read_unaligned(
                                msg.data.as_ptr().add(1) as *const crate::ipc::gui::DestroySurfaceMsg
                            )
                        };
                        crate::klog!(
                            crate::klog::Level::Info,
                            "[Compositor] DestroySurface id={}",
                            payload.surface_id,
                        );
                        comp.destroy_surface(payload.surface_id);
                    }
                }
                10 => {
                    // LowerSurface
                    if msg.len >= 1 + core::mem::size_of::<crate::ipc::gui::LowerSurfaceMsg>() {
                        let payload = unsafe {
                            core::ptr::read_unaligned(
                                msg.data.as_ptr().add(1) as *const crate::ipc::gui::LowerSurfaceMsg
                            )
                        };
                        crate::klog!(
                            crate::klog::Level::Info,
                            "[Compositor] LowerSurface id={}",
                            payload.surface_id,
                        );
                        comp.lower_surface(payload.surface_id);
                    }
                }


                _ => {}
            }
        }

        // 3b. Poll keyboard state (ISR-safe atomic) and forward to event channel
        let key_val = crate::drivers::keyboard::poll_compositor_key();
        if key_val != 0 {
            needs_redraw = true; // keyboard input may cause UI changes
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
            if let Some(chan) = target_event_channel(
                surface_event_channel(&comp.surfaces, comp.focused_surface_id),
                event_chan,
            ) {
                let _ = crate::ipc::send(chan, crate::process::Pid(0), &buf[..1 + n]);
            }
        }

        // 3c. Poll mouse — single read per frame, used for both IPC dispatch and cursor draw
        let (mx, my) = crate::drivers::ps2_mouse::get_mouse_pos();
        let mb = crate::drivers::ps2_mouse::get_mouse_btn();
        let mouse_moved = mx != last_mouse_x || my != last_mouse_y || mb != last_mouse_btn;
        if mouse_moved {
            comp.damage =
                comp.damage
                    .union(cursor_damage_rect(last_mouse_x, last_mouse_y, fb_w, fb_h));
            comp.damage = comp.damage.union(cursor_damage_rect(mx, my, fb_w, fb_h));
            needs_redraw = true;
            let mut hit = if comp.surfaces.is_empty() {
                (true, mx, my, 0)
            } else {
                (false, 0, 0, 0)
            };
            for s in comp.surfaces.iter().rev() {
                let local = local_mouse_hit(mx, my, s.x, s.y, s.width, s.height);
                if local.0 {
                    hit = (true, local.1, local.2, s.id);
                    break;
                }
            }
            if hit.0 && hit.3 != 0 && mb != 0 {
                let prev_focus = comp.focused_surface_id;
                comp.focused_surface_id = hit.3;
                comp.raise_surface(hit.3);
                // Send focus notification if focus actually changed
                if prev_focus != hit.3 {
                    let notify = crate::ipc::gui::FocusNotifyMsg { focused_id: hit.3 };
                    let notify_bytes = unsafe {
                        core::slice::from_raw_parts(
                            &notify as *const crate::ipc::gui::FocusNotifyMsg as *const u8,
                            core::mem::size_of::<crate::ipc::gui::FocusNotifyMsg>(),
                        )
                    };
                    let mut buf = [0u8; 256];
                    buf[0] = crate::ipc::gui::OpCode::FocusNotify as u8;
                    let n = notify_bytes.len().min(255);
                    buf[1..1 + n].copy_from_slice(&notify_bytes[..n]);
                    // Notify the newly focused surface
                    if let Some(chan) = surface_event_channel(&comp.surfaces, hit.3) {
                        let _ = crate::ipc::send(chan, crate::process::Pid(0), &buf[..1 + n]);
                    }
                    // Also notify the previously focused surface (lost focus)
                    if prev_focus != 0 {
                        let lost_notify = crate::ipc::gui::FocusNotifyMsg { focused_id: 0 };
                        let lost_bytes = unsafe {
                            core::slice::from_raw_parts(
                                &lost_notify as *const crate::ipc::gui::FocusNotifyMsg as *const u8,
                                core::mem::size_of::<crate::ipc::gui::FocusNotifyMsg>(),
                            )
                        };
                        let mut lost_buf = [0u8; 256];
                        lost_buf[0] = crate::ipc::gui::OpCode::FocusNotify as u8;
                        let n2 = lost_bytes.len().min(255);
                        lost_buf[1..1 + n2].copy_from_slice(&lost_bytes[..n2]);
                        if let Some(chan) = surface_event_channel(&comp.surfaces, prev_focus) {
                            let _ = crate::ipc::send(chan, crate::process::Pid(0), &lost_buf[..1 + n2]);
                        }
                    }
                }
                // Start drag on button-down if not already dragging
                if drag_surface_id == 0 {
                    drag_surface_id = hit.3;
                    if let Some(s) = comp.surfaces.iter().find(|s| s.id == hit.3) {
                        drag_offset_x = mx - s.x;
                        drag_offset_y = my - s.y;
                    }
                }
            }
            // Stop drag on button-up
            if mb == 0 && drag_surface_id != 0 {
                drag_surface_id = 0;
            }
            // Perform drag: move surface by mouse delta
            if drag_surface_id != 0 && mb != 0 {
                let new_x = mx - drag_offset_x;
                let new_y = my - drag_offset_y;
                comp.set_position(drag_surface_id, new_x, new_y);
                needs_redraw = true;
            }

            if hit.0 {
                let input = crate::ipc::gui::InputMsg {
                    kind: 2,
                    code: mb as u32,
                    x: hit.1,
                    y: hit.2,
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
                if let Some(chan) =
                    target_event_channel(surface_event_channel(&comp.surfaces, hit.3), event_chan)
                {
                    let _ = crate::ipc::send(chan, crate::process::Pid(0), &buf[..1 + n]);
                }
            }
            last_mouse_x = mx;
            last_mouse_y = my;
            last_mouse_btn = mb;
        }

        // Also check if any surface has pending dirty regions
        if !needs_redraw {
            needs_redraw = !comp.damage.is_empty()
                || comp.surfaces.iter().any(|s| s.dirty)
                || !wallpaper_drawn;
        }

        if needs_redraw {
            if comp.surfaces.is_empty() {
                let repaint = no_surface_repaint(wallpaper_drawn, comp.damage, fb_w, fb_h);
                draw_wallpaper(fb_slice, fb_w, fb_h, &repaint);
                wallpaper_drawn = true;
                comp.damage = Rect::default();
            } else {
                if !wallpaper_drawn {
                    let full_screen = Rect {
                        x: 0,
                        y: 0,
                        w: fb_w,
                        h: fb_h,
                    };
                    draw_wallpaper(fb_slice, fb_w, fb_h, &full_screen);
                    wallpaper_drawn = true;
                    comp.damage = full_screen;
                }

                let _composited = comp.composite(fb_slice, fb_w, fb_h, 32);
            }

            draw_cursor(fb_slice, fb_w, fb_h, mx, my);

            if let Some(chan) = gpu_chan {
                let flush_cmd = [1u8];
                let _ = crate::ipc::send(chan, crate::process::Pid(0), &flush_cmd);
            }
            DIAG_FLUSH_COUNT.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        }

        // ponytail: diagnostics once per second
        let uptime = crate::timer::uptime_ms();
        if uptime - LAST_DIAG_LOG.load(core::sync::atomic::Ordering::Relaxed) >= 1000 {
            LAST_DIAG_LOG.store(uptime, core::sync::atomic::Ordering::Relaxed);
            crate::klog!(
                crate::klog::Level::Info,
                "[Compositor] Diag mouse=({},{}) btn={} event_chan={} flush_cnt={}",
                mx,
                my,
                mb,
                event_chan.is_some(),
                DIAG_FLUSH_COUNT.load(core::sync::atomic::Ordering::Relaxed)
            );
        }

        // 3f. Frame pacing — sleep remainder of 16ms frame, yield CPU properly
        // ponytail: sleep_ms blocks the kthread and lets scheduler run other tasks.
        // Spin-yielding (the old code) starved QEMU emulation causing freezes.
        let elapsed = crate::timer::uptime_ms().saturating_sub(frame_start);
        if elapsed < FRAME_MS {
            crate::timer::sleep_ms(my_pid, FRAME_MS - elapsed);
        }
        crate::process::scheduler::yield_now();
    }
}

/// Draw a vertical gradient wallpaper across the full framebuffer.
/// Top: dark navy (0x0A0E1A), bottom: dark teal (0x0F1A2A).
/// ponytail: per-row linear interpolation, no per-pixel branching.
fn draw_wallpaper(fb: &mut [u8], w: u32, h: u32, clip: &Rect) {
    if h == 0 || w == 0 || clip.w == 0 || clip.h == 0 {
        return;
    }
    let stride = (w * 4) as usize;
    let y0 = clip.y.min(h);
    let y1 = (clip.y + clip.h).min(h);
    let x0 = clip.x.min(w) as usize;
    let x1 = (clip.x + clip.w).min(w) as usize;

    // Top color (BGRX): B=0x1A G=0x0E R=0x0A
    // Bottom color:      B=0x2A G=0x1A R=0x0F
    for y in y0..y1 {
        let t = y as u32;
        let inv = h - 1 - y;
        let b = ((0x1A * inv + 0x2A * t) / (h - 1).max(1)) as u8;
        let g = ((0x0E * inv + 0x1A * t) / (h - 1).max(1)) as u8;
        let r = ((0x0A * inv + 0x0F * t) / (h - 1).max(1)) as u8;
        let row_off = (y as usize) * stride;
        let mut x = x0;
        while x < x1 {
            let off = row_off + x * 4;
            if off + 3 < fb.len() {
                fb[off] = b;
                fb[off + 1] = g;
                fb[off + 2] = r;
                fb[off + 3] = 0xFF;
            }
            x += 1;
        }
    }
}
/// Draw a soft shadow behind a surface. 4px offset down-right, 8px blur radius.
/// ponytail: simple alpha rect, no gaussian — perceptually close enough.
fn draw_shadow(
    fb: &mut [u8],
    fb_w: u32,
    fb_h: u32,
    sx: i32,
    sy: i32,
    sw: u32,
    sh: u32,
    clip: &Rect,
) {
    const OFFSET: i32 = 4;
    const BLUR: u32 = 8;
    let shadow_x = sx + OFFSET;
    let shadow_y = sy + OFFSET;
    let shadow_w = sw + BLUR;
    let shadow_h = sh + BLUR;
    let stride = (fb_w * 4) as usize;
    // Clipped bounds to framebuffer and damage rect
    let x0 = (shadow_x.max(0) as u32).max(clip.x);
    let y0 = (shadow_y.max(0) as u32).max(clip.y);
    let x1 = (((shadow_x + shadow_w as i32) as u32).min(fb_w)).min(clip.x + clip.w);
    let y1 = (((shadow_y + shadow_h as i32) as u32).min(fb_h)).min(clip.y + clip.h);
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    for y in y0..y1 {
        let row_off = (y as usize) * stride;
        for x in x0..x1 {
            let off = row_off + (x as usize) * 4;
            if off + 3 >= fb.len() {
                continue;
            }
            // Alpha falloff: stronger near surface, fading at edges
            let dx = if (x as i32) < sx + OFFSET {
                (sx + OFFSET - x as i32) as u32
            } else if x >= sx as u32 + sw {
                x - sx as u32 - sw + 1
            } else {
                0
            };
            let dy = if (y as i32) < sy + OFFSET {
                (sy + OFFSET - y as i32) as u32
            } else if y >= sy as u32 + sh {
                y - sy as u32 - sh + 1
            } else {
                0
            };
            let dist = dx.max(dy);
            let alpha = if dist >= BLUR {
                0u8
            } else {
                (80 * (BLUR - dist) / BLUR) as u8
            };
            if alpha == 0 {
                continue;
            }
            // Blend black with alpha over existing pixel
            let inv = 255 - alpha as u32;
            fb[off] = ((fb[off] as u32 * inv) / 255) as u8;
            fb[off + 1] = ((fb[off + 1] as u32 * inv) / 255) as u8;
            fb[off + 2] = ((fb[off + 2] as u32 * inv) / 255) as u8;
        }
    }
}

/// 12×19 arrow cursor bitmap — classic pointer shape.
/// Hotspot at top-left (0,0). Each row is a pair of bitmasks:
/// `outline` = black border pixels, `fill` = white interior pixels.
/// ponytail: inline bitmap, no file I/O or alloc needed.
const CURSOR_W: usize = 12;
const CURSOR_H: usize = 19;
#[rustfmt::skip]
static CURSOR_BITMAP: [(u16, u16); CURSOR_H] = [
    // (outline,        fill)          — LSB = leftmost pixel (col 0)
    // Visual: B=black outline, W=white fill, .=transparent
    (0b000000000001, 0b000000000000), //  0: B
    (0b000000000011, 0b000000000000), //  1: BB
    (0b000000000101, 0b000000000010), //  2: BWB
    (0b000000001001, 0b000000000110), //  3: BWWB
    (0b000000010001, 0b000000001110), //  4: BWWWB
    (0b000000100001, 0b000000011110), //  5: BWWWWB
    (0b000001000001, 0b000000111110), //  6: BWWWWWB
    (0b000010000001, 0b000001111110), //  7: BWWWWWWB
    (0b000100000001, 0b000011111110), //  8: BWWWWWWWB
    (0b001000000001, 0b000111111110), //  9: BWWWWWWWWB
    (0b010000000001, 0b001111111110), // 10: BWWWWWWWWWB
    (0b011111000001, 0b000000111110), // 11: BWWWWWBBBBB
    (0b000001001001, 0b000000110110), // 12: BWWBWWB
    (0b000010010101, 0b000001100010), // 13: BWB.BWWB
    (0b000010010011, 0b000001100000), // 14: BB..BWWB
    (0b000100100001, 0b000011000000), // 15: B....BWWB
    (0b000100100000, 0b000011000000), // 16: .....BWWB
    (0b000101000000, 0b000010000000), // 17: ......BWB
    (0b000011000000, 0b000000000000), // 18: ......BB
];

const fn sat_i32(v: u32) -> i32 {
    if v > i32::MAX as u32 {
        i32::MAX
    } else {
        v as i32
    }
}

const fn should_raise_surface(pos: usize, len: usize) -> bool {
    pos + 1 < len
}

fn surface_event_channel(surfaces: &[Surface], id: u32) -> Option<u32> {
    if id == 0 {
        return None;
    }
    for s in surfaces {
        if s.id == id && s.event_channel_id != 0 {
            return Some(s.event_channel_id);
        }
    }
    None
}

const fn target_event_channel(surface: Option<u32>, fallback: Option<u32>) -> Option<u32> {
    match surface {
        Some(chan) => Some(chan),
        None => fallback,
    }
}

const fn local_mouse_hit(mx: i32, my: i32, sx: i32, sy: i32, sw: u32, sh: u32) -> (bool, i32, i32) {
    if sw > i32::MAX as u32 || sh > i32::MAX as u32 {
        return (false, 0, 0);
    }
    let sw = sw as i32;
    let sh = sh as i32;
    if mx >= sx && my >= sy && mx < sx + sw && my < sy + sh {
        (true, mx - sx, my - sy)
    } else {
        (false, 0, 0)
    }
}

const fn no_surface_repaint(wallpaper_drawn: bool, damage: Rect, fb_w: u32, fb_h: u32) -> Rect {
    if wallpaper_drawn {
        damage
    } else {
        Rect {
            x: 0,
            y: 0,
            w: fb_w,
            h: fb_h,
        }
    }
}

const fn cursor_damage_rect(mx: i32, my: i32, fb_w: u32, fb_h: u32) -> Rect {
    let fb_w = sat_i32(fb_w);
    let fb_h = sat_i32(fb_h);
    let cw = CURSOR_W as i32;
    let ch = CURSOR_H as i32;
    if fb_w <= 0 || fb_h <= 0 || mx >= fb_w || my >= fb_h || mx <= -cw || my <= -ch {
        return Rect {
            x: 0,
            y: 0,
            w: 0,
            h: 0,
        };
    }

    let x0 = if mx < 0 { 0 } else { mx };
    let y0 = if my < 0 { 0 } else { my };
    let x1 = if mx > fb_w - cw { fb_w } else { mx + cw };
    let y1 = if my > fb_h - ch { fb_h } else { my + ch };
    Rect {
        x: x0 as u32,
        y: y0 as u32,
        w: (x1 - x0) as u32,
        h: (y1 - y0) as u32,
    }
}

const _: () = {
    let full = cursor_damage_rect(4, 5, 320, 200);
    assert!(full.x == 4 && full.y == 5 && full.w == CURSOR_W as u32 && full.h == CURSOR_H as u32);

    let clipped = cursor_damage_rect(-4, -5, 320, 200);
    assert!(clipped.x == 0 && clipped.y == 0 && clipped.w == 8 && clipped.h == 14);

    let hit = local_mouse_hit(25, 35, 20, 30, 100, 80);
    assert!(hit.0 && hit.1 == 5 && hit.2 == 5);

    let miss = local_mouse_hit(10, 35, 20, 30, 100, 80);
    assert!(!miss.0);

    assert!(should_raise_surface(0, 2));
    assert!(!should_raise_surface(1, 2));
    assert!(!should_raise_surface(0, 1));

    assert!(matches!(target_event_channel(Some(7), Some(3)), Some(7)));
    assert!(matches!(target_event_channel(None, Some(3)), Some(3)));
    assert!(matches!(target_event_channel(None, None), None));

    let initial = no_surface_repaint(
        false,
        Rect {
            x: 8,
            y: 9,
            w: 10,
            h: 11,
        },
        320,
        200,
    );
    assert!(initial.x == 0 && initial.y == 0 && initial.w == 320 && initial.h == 200);

    let damaged = no_surface_repaint(
        true,
        Rect {
            x: 8,
            y: 9,
            w: 10,
            h: 11,
        },
        320,
        200,
    );
    assert!(damaged.x == 8 && damaged.y == 9 && damaged.w == 10 && damaged.h == 11);
};

/// Draw a proper arrow cursor at the mouse position.
/// Black outline + white fill, fully clipped to framebuffer bounds.
fn draw_cursor(fb: &mut [u8], fb_w: u32, fb_h: u32, mx: i32, my: i32) {
    let stride = (fb_w as usize) * 4;
    for row in 0..CURSOR_H {
        let py = my + row as i32;
        if py < 0 || py >= fb_h as i32 {
            continue;
        }
        let (outline, fill) = CURSOR_BITMAP[row];
        for col in 0..CURSOR_W {
            let px = mx + col as i32;
            if px < 0 || px >= fb_w as i32 {
                continue;
            }
            let bit = 1u16 << col;
            let (b, g, r) = if fill & bit != 0 {
                (255u8, 255u8, 255u8) // white fill
            } else if outline & bit != 0 {
                (0u8, 0u8, 0u8) // black outline
            } else {
                continue; // transparent
            };
            let off = (py as usize) * stride + (px as usize) * 4;
            if off + 3 < fb.len() {
                fb[off] = b;
                fb[off + 1] = g;
                fb[off + 2] = r;
                fb[off + 3] = 255;
            }
        }
    }
}
