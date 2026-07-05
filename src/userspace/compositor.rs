//! Kernel-mode compositor with surface management and damage tracking.
//!
//! Manages SHM-backed surfaces, tracks dirty regions, and composites
//! them to the GPU framebuffer. Runs as a kernel thread.

use alloc::vec::Vec;
#[cfg(not(feature = "zig-hotpaths"))]
use core::ptr;
#[cfg(feature = "zig-hotpaths")]
unsafe extern "C" {
    fn zig_blit_bitmap(
        dst: *mut u8,
        dst_pitch: u32,
        src: *const u8,
        src_pitch: u32,
        sx: u32,
        sy: u32,
        sw: u32,
        sh: u32,
        dx: u32,
        dy: u32,
    );
}
/// Typed identifier for a render surface.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Default)]
pub struct SurfaceId(pub u32);

/// Typed identifier for a shared-memory segment.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Default)]
pub struct ShmId(pub u32);

/// Typed identifier for an event IPC channel.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Default)]
pub struct EventChannelId(pub u32);
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum WindowKind {
    #[default]
    Floating,
    Tiled,
    Dialog,
    Popup,
    Fullscreen,
}
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum CursorShape {
    #[default]
    Default, // arrow
    Text,    // I-beam
    Hidden,
}

/// Shell actions triggered by keybindings.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    CloseFocused,
    FocusNext,
    FocusPrev,
    ToggleFullscreen,
    ToggleCRT,
    ToggleVignette,
}

/// Modifier flags matching keyboard.rs constants.
#[allow(unused_imports)]
use crate::drivers::keyboard::{MOD_SHIFT, MOD_CTRL, MOD_ALT, MOD_SUPER};
use pc_keyboard::KeyCode;

/// Static keybinding table: (modifiers, keycode, action).
/// Keycode values are `pc_keyboard::KeyCode as u8`.
const KEYBINDINGS: &[(u8, u8, Action)] = &[
    (MOD_SUPER, KeyCode::Q as u8, Action::CloseFocused),
    (MOD_SUPER, KeyCode::Tab as u8, Action::FocusNext),
    (MOD_SUPER | MOD_SHIFT, KeyCode::Tab as u8, Action::FocusPrev),
    (MOD_SUPER, KeyCode::F as u8, Action::ToggleFullscreen),
    (MOD_SUPER, KeyCode::Key1 as u8, Action::ToggleCRT),
    (MOD_SUPER, KeyCode::Key2 as u8, Action::ToggleVignette),
];

const fn resolve_keybinding(mods: u8, keycode: u8) -> Option<Action> {
    let mut i = 0;
    while i < KEYBINDINGS.len() {
        let bind = KEYBINDINGS[i];
        if bind.0 == mods && bind.1 == keycode {
            return Some(bind.2);
        }
        i += 1;
    }
    None
}

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

    pub fn contains(self, other: Rect) -> bool {
        if other.is_empty() {
            return true;
        }
        self.x <= other.x
            && self.y <= other.y
            && self.x + self.w >= other.x + other.w
            && self.y + self.h >= other.y + other.h
    }

    /// Subtract `other` from `self`, returning the portion of `self` not covered.
    /// Returns `Rect::default()` when `other` fully covers `self`.
    /// Exact 4-region decomposition is elided; this conservative check is sufficient
    /// for the skip-clear optimization where only full coverage matters.
    pub fn difference(self, other: Rect) -> Rect {
        let overlap = self.intersect(other);
        if overlap.is_empty() {
            return self;
        }
        if other.contains(self) {
            return Rect::default();
        }
        // ponytail: return self as largest uncovered segment.
        // Full 4-region decomposition only needed when multiple surfaces
        // collectively cover damage, which is rare.
        self
    }
}

/// Triple-buffer swapchain for pipelined rendering.
/// Three buffers cycle through draw/pending/displayed roles:
/// - draw_idx:  compositor render target (current frame)
/// - pending_idx: fully rendered, waiting for VRAM copy
/// - displayed_idx: last frame copied to VRAM
/// This eliminates the backbuffer stall where drawing waits for VRAM copy.
struct TripleSwapchain {
    buffers: [alloc::vec::Vec<u8>; 3],
    draw_idx: usize,
    pending_idx: usize,
    displayed_idx: usize,
}

impl TripleSwapchain {
    fn new(fb_size: usize) -> Self {
        Self {
            buffers: [
                alloc::vec![0u8; fb_size],
                alloc::vec![0u8; fb_size],
                alloc::vec![0u8; fb_size],
            ],
            draw_idx: 0,
            pending_idx: 1,
            displayed_idx: 2,
        }
    }
    fn draw_mut(&mut self) -> &mut [u8] {
        &mut self.buffers[self.draw_idx]
    }
    fn advance(&mut self) {
        self.displayed_idx = self.pending_idx;
        self.pending_idx = self.draw_idx;
        self.draw_idx = self.displayed_idx;
    }
}

/// A render surface backed by SHM.
pub struct Surface {
    pub id: SurfaceId,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub shm_id: ShmId,
    pub event_channel_id: EventChannelId,
    /// Kernel-virtual address of the SHM mapping.
    pub shm_addr: u64,
    pub dirty: bool,
    pub dirty_rect: Rect,
    pub kind: WindowKind,
    pub restore_rect: Option<Rect>,
    pub cursor_shape: CursorShape,
}

/// The compositor state machine — surface list, damage tracking, compositing.
pub struct Compositor {
    pub surfaces: Vec<Surface>,
    pub damage: Rect,
    pub next_id: u32,
    pub focused_surface_id: SurfaceId,
    pub crt_effect: bool,
    pub vignette_effect: bool,
}

impl Compositor {
    pub fn new() -> Self {
        Compositor {
            surfaces: Vec::new(),
            damage: Rect::default(),
            next_id: 1,
            focused_surface_id: SurfaceId::default(),
            crt_effect: false,
            vignette_effect: false,
        }
    }

    /// Create a new surface with a specified ID. The entire surface is marked dirty.
    pub fn create_surface(&mut self, id: SurfaceId, width: u32, height: u32, shm_id: ShmId, shm_addr: u64) -> SurfaceId {
        // Surface starts clean — shm_addr is set later via BufferAttach
        self.surfaces.push(Surface {
            id,
            x: 0,
            y: 0,
            width,
            height,
            shm_id,
            shm_addr,
            event_channel_id: EventChannelId::default(),
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
            kind: WindowKind::Floating,
            restore_rect: None,
            cursor_shape: CursorShape::Default,
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
    pub fn destroy_surface(&mut self, id: SurfaceId) {
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
    pub fn raise_surface(&mut self, id: SurfaceId) {
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
    pub fn lower_surface(&mut self, id: SurfaceId) {
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
    pub fn resize_surface(&mut self, id: SurfaceId, new_w: u32, new_h: u32) {
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
    pub fn set_position(&mut self, id: SurfaceId, x: i32, y: i32) {
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
    pub fn mark_dirty(&mut self, id: SurfaceId, rect: Rect) {
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
    pub fn composite(&mut self, fb: &mut [u8], fb_w: u32, fb_h: u32, fb_bpp: u32) -> Rect {
        if self.damage.is_empty() && self.surfaces.iter().all(|s| !s.dirty) {
            return Rect::default();
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
            return Rect::default();
        }

        // ponytail: skip clear when union of visible surfaces covers damage.
        let mut uncovered = self.damage;
        for s in &self.surfaces {
            if s.shm_addr != 0 && s.x >= 0 && s.y >= 0 {
                let sr = Rect {
                    x: s.x as u32,
                    y: s.y as u32,
                    w: s.width,
                    h: s.height,
                };
                uncovered = uncovered.difference(sr);
                if uncovered.is_empty() {
                    break;
                }
            }
        }
        let need_clear = !uncovered.is_empty();

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

            // Draw shadow only for non-popup surfaces
            if s.kind != WindowKind::Popup {
                draw_shadow(fb, fb_w, fb_h, s.x, s.y, s.width, s.height, &self.damage);
            }
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

            #[cfg(feature = "zig-hotpaths")]
            unsafe {
                zig_blit_bitmap(
                    fb.as_mut_ptr(),
                    fb_stride as u32,
                    s.shm_addr as *const u8,
                    src_stride as u32,
                    repaint.x - sx,
                    repaint.y - sy,
                    repaint.w,
                    repaint.h,
                    repaint.x,
                    repaint.y,
                );
            }
            #[cfg(not(feature = "zig-hotpaths"))]
            {
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
        }

        // Pass 2 — clear all per-surface dirty flags
        for s in &mut self.surfaces {
            s.dirty = false;
            s.dirty_rect = Rect::default();
        }
        let final_damage = self.damage;
        self.damage = Rect::default();
        final_damage
    }

    /// Cycle focus to next/previous surface in z-order.
    pub fn cycle_focus(&mut self, forward: bool) {
        if self.surfaces.is_empty() {
            return;
        }
        // Find current focus index
        let current_idx = self.surfaces.iter().position(|s| s.id == self.focused_surface_id);
        let next_idx = match current_idx {
            Some(i) => {
                if forward {
                    (i + 1) % self.surfaces.len()
                } else {
                    if i == 0 { self.surfaces.len() - 1 } else { i - 1 }
                }
            }
            None => 0, // No current focus — focus the bottom surface
        };
        self.focused_surface_id = self.surfaces[next_idx].id;
        // Raise to front
        self.raise_surface(self.focused_surface_id);
    }

    /// Toggle fullscreen for a surface. Saves/restores the old geometry.
    pub fn toggle_fullscreen(&mut self, id: SurfaceId, fb_w: u32, fb_h: u32) {
        let pos = match self.surfaces.iter().position(|s| s.id == id) {
            Some(p) => p,
            None => return,
        };
        let s = &mut self.surfaces[pos];
        match s.kind {
            WindowKind::Fullscreen => {
                if let Some(r) = s.restore_rect.take() {
                    // Damage old (fullscreen) bounds
                    self.damage = self.damage.union(Rect {
                        x: s.x as u32, y: s.y as u32, w: s.width, h: s.height,
                    });
                    s.x = r.x as i32;
                    s.y = r.y as i32;
                    s.width = r.w;
                    s.height = r.h;
                    s.kind = WindowKind::Floating;
                    s.dirty = true;
                    s.dirty_rect = Rect { x: 0, y: 0, w: s.width, h: s.height };
                    self.damage = self.damage.union(r);
                }
            }
            _ => {
                // Damage current bounds
                if s.x >= 0 && s.y >= 0 {
                    self.damage = self.damage.union(Rect {
                        x: s.x as u32, y: s.y as u32, w: s.width, h: s.height,
                    });
                }
                s.restore_rect = Some(Rect {
                    x: if s.x >= 0 { s.x as u32 } else { 0 },
                    y: if s.y >= 0 { s.y as u32 } else { 0 },
                    w: s.width, h: s.height,
                });
                s.x = 0;
                s.y = 0;
                s.width = fb_w;
                s.height = fb_h;
                s.kind = WindowKind::Fullscreen;
                s.dirty = true;
                s.dirty_rect = Rect { x: 0, y: 0, w: fb_w, h: fb_h };
                self.damage = self.damage.union(Rect {
                    x: 0, y: 0, w: fb_w, h: fb_h,
                });
            }
        }
    }
}

fn handle_action(comp: &mut Compositor, action: Action, fb_w: u32, fb_h: u32) -> bool {
    match action {
        Action::CloseFocused => {
            if comp.focused_surface_id != SurfaceId(0) {
                comp.destroy_surface(comp.focused_surface_id);
                comp.focused_surface_id = SurfaceId(0);
                return true;
            }
            false
        }
        Action::FocusNext => {
            comp.cycle_focus(true);
            true
        }
        Action::FocusPrev => {
            comp.cycle_focus(false);
            true
        }
        Action::ToggleFullscreen => {
            if comp.focused_surface_id != SurfaceId(0) {
                comp.toggle_fullscreen(comp.focused_surface_id, fb_w, fb_h);
                return true;
            }
            false
        }
        Action::ToggleCRT => {
            comp.crt_effect = !comp.crt_effect;
            comp.damage = Rect { x: 0, y: 0, w: fb_w, h: fb_h };
            true
        }
        Action::ToggleVignette => {
            comp.vignette_effect = !comp.vignette_effect;
            comp.damage = Rect { x: 0, y: 0, w: fb_w, h: fb_h };
            true
        }
    }
}

#[cfg(feature = "zig-hotpaths")]
fn apply_vignette_to_clip(fb: &mut [u8], stride: usize, clip: &Rect, fb_w: u32, fb_h: u32) {
    let cx = (fb_w / 2) as i32;
    let cy = (fb_h / 2) as i32;
    let max_dist = cx.max(cy) as f32;
    let radius = max_dist * 0.4;
    let strength = 75.0;

    let y0 = clip.y as i32;
    let y1 = (clip.y + clip.h) as i32;
    let x0 = clip.x as i32;
    let x1 = (clip.x + clip.w) as i32;

    for y in y0..y1 {
        let row_off = (y as usize) * stride;
        let dy = y - cy;
        let dy2 = (dy * dy) as f32;
        for x in x0..x1 {
            let dx = x - cx;
            let dist = (dx * dx) as f32 + dy2;
            let dist = libm::sqrtf(dist);
            if dist > radius {
                let factor = (dist - radius) * strength / (max_dist - radius + 1.0);
                let factor = if factor > strength { strength } else { factor };
                let clamp_factor = factor as u32;

                let off = row_off + (x as usize) * 4;
                if off + 3 <= fb.len() {
                    let b = fb[off] as u32;
                    let g = fb[off + 1] as u32;
                    let r = fb[off + 2] as u32;
                    fb[off] = (b * (255 - clamp_factor) / 255) as u8;
                    fb[off + 1] = (g * (255 - clamp_factor) / 255) as u8;
                    fb[off + 2] = (r * (255 - clamp_factor) / 255) as u8;
                }
            }
        }
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

    // ponytail: Double-buffer — compose into cached RAM, then bulk-copy damaged region to GPU VRAM.
    // MMIO writes to GPU VRAM are uncached (~100x slower than RAM). Writing every pixel directly
    // to VRAM is the #1 cause of frame lag. Instead:
    //   1. Compose everything into a cached RAM back-buffer
    //   2. Bulk-copy only the damaged rectangle to GPU VRAM
    let fb_size = (fb_w * fb_h * 4) as usize;
    let mut swapchain = TripleSwapchain::new(fb_size);

    let mut comp = Compositor::new();
    let mut event_chan: Option<EventChannelId> = None;
    let mut current_cursor_shape = CursorShape::Default;
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
    let mut drag_surface_id = SurfaceId::default();
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
                        let sid = comp.create_surface(SurfaceId(payload.surface_id), payload.width, payload.height, ShmId(0), 0);
                        crate::klog!(
                            crate::klog::Level::Info,
                            "[Compositor] Surface {} created ({}x{})",
                            sid.0,
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
                            SurfaceId(payload.surface_id),
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
                                .find(|s| s.id == SurfaceId(payload.surface_id))
                            {
                                s.shm_id = ShmId(payload.shm_id);
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
                        comp.set_position(SurfaceId(payload.surface_id), payload.x, payload.y);
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
                            event_chan = Some(EventChannelId(payload.event_channel_id));
                        } else if let Some(s) = comp
                            .surfaces
                            .iter_mut()
                            .find(|s| s.id == SurfaceId(payload.surface_id))
                        {
                            s.event_channel_id = EventChannelId(payload.event_channel_id);
                        } else {
                            event_chan = Some(EventChannelId(payload.event_channel_id));
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
                        comp.resize_surface(SurfaceId(payload.surface_id), payload.width, payload.height);
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
                        comp.destroy_surface(SurfaceId(payload.surface_id));
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
                        comp.lower_surface(SurfaceId(payload.surface_id));
                    }
                }
                12 => {
                    // SetWindowKind
                    if msg.len >= 1 + core::mem::size_of::<crate::ipc::gui::SetWindowKindMsg>() {
                        let payload = unsafe {
                            core::ptr::read_unaligned(
                                msg.data.as_ptr().add(1) as *const crate::ipc::gui::SetWindowKindMsg
                            )
                        };
                        let kind = match payload.kind {
                            0 => WindowKind::Floating,
                            1 => WindowKind::Tiled,
                            2 => WindowKind::Dialog,
                            3 => WindowKind::Popup,
                            _ => WindowKind::Floating,
                        };
                        if let Some(s) = comp.surfaces.iter_mut().find(|s| s.id == SurfaceId(payload.surface_id)) {
                            s.kind = kind;
                        }
                    }
                }
                13 => {
                    // SetCursorShape
                    if msg.len >= 1 + core::mem::size_of::<crate::ipc::gui::SetCursorShapeMsg>() {
                        let payload = unsafe {
                            core::ptr::read_unaligned(
                                msg.data.as_ptr().add(1) as *const crate::ipc::gui::SetCursorShapeMsg
                            )
                        };
                        let shape = match payload.shape {
                            1 => CursorShape::Text,
                            2 => CursorShape::Hidden,
                            _ => CursorShape::Default,
                        };
                        if let Some(s) = comp.surfaces.iter_mut().find(|s| s.id == SurfaceId(payload.surface_id)) {
                            s.cursor_shape = shape;
                        }
                    }
                }
                _ => {}
            }
        }

        // 3b. Poll keyboard state (ISR-safe atomic) and forward to event channel
        let packed = crate::drivers::keyboard::poll_compositor_key();
        if packed != 0 {
            let keycode = (packed & 0xFF) as u8;
            let mods = ((packed >> 8) & 0xFF) as u8;
            let payload = ((packed >> 16) & 0xFFFF) as u16;

            if let Some(action) = resolve_keybinding(mods, keycode) {
                let handled = handle_action(&mut comp, action, fb_w, fb_h);
                if handled {
                    needs_redraw = true;
                }
            } else if payload != 0 {
                needs_redraw = true; // keyboard input may cause UI changes
                let kind = if payload & 0x100 != 0 { 1u8 } else { 2u8 };
                let code = (payload & 0xFF) as u32;
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
                    let _ = crate::ipc::send(chan.0, crate::process::Pid(0), &buf[..1 + n]);
                }
            }
        }

        // 3c. Poll mouse — single read per frame, used for both IPC dispatch and cursor draw
        let (mx, my) = crate::drivers::ps2_mouse::get_mouse_pos();
        let mb = crate::drivers::ps2_mouse::get_mouse_btn();
        let mouse_moved = mx != last_mouse_x || my != last_mouse_y || mb != last_mouse_btn;
        if mouse_moved {
            comp.damage =
                comp.damage
                    .union(cursor_damage_rect(last_mouse_x, last_mouse_y, fb_w, fb_h, current_cursor_shape));
            comp.damage = comp.damage.union(cursor_damage_rect(mx, my, fb_w, fb_h, current_cursor_shape));
            needs_redraw = true;
            let mut hit = if comp.surfaces.is_empty() {
                (true, mx, my, SurfaceId(0))
            } else {
                (false, 0, 0, SurfaceId(0))
            };
            for s in comp.surfaces.iter().rev() {
                let local = local_mouse_hit(mx, my, s.x, s.y, s.width, s.height);
                if local.0 {
                    hit = (true, local.1, local.2, s.id);
                    break;
                }
            }
            let new_shape = if hit.0 && hit.3 != SurfaceId(0) {
                comp.surfaces.iter()
                    .find(|s| s.id == hit.3)
                    .map(|s| s.cursor_shape)
                    .unwrap_or(CursorShape::Default)
            } else {
                CursorShape::Default
            };
            if new_shape != current_cursor_shape {
                comp.damage = comp.damage.union(cursor_damage_rect(mx, my, fb_w, fb_h, current_cursor_shape));
                comp.damage = comp.damage.union(cursor_damage_rect(mx, my, fb_w, fb_h, new_shape));
                current_cursor_shape = new_shape;
            }
            if hit.0 && hit.3 != SurfaceId(0) && mb != 0 {
                let prev_focus = comp.focused_surface_id;
                comp.focused_surface_id = hit.3;
                comp.raise_surface(hit.3);
                // Send focus notification if focus actually changed
                if prev_focus != hit.3 {
                    let notify = crate::ipc::gui::FocusNotifyMsg { focused_id: hit.3.0 };
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
                        let _ = crate::ipc::send(chan.0, crate::process::Pid(0), &buf[..1 + n]);
                    }
                    // Also notify the previously focused surface (lost focus)
                    if prev_focus != SurfaceId(0) {
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
                            let _ = crate::ipc::send(chan.0, crate::process::Pid(0), &lost_buf[..1 + n2]);
                        }
                    }
                }
                if drag_surface_id == SurfaceId(0) {
                    drag_surface_id = hit.3;
                    if let Some(s) = comp.surfaces.iter().find(|s| s.id == hit.3) {
                        drag_offset_x = mx - s.x;
                        drag_offset_y = my - s.y;
                    }
                }
            }
            // Stop drag on button-up
            if mb == 0 && drag_surface_id != SurfaceId(0) {
                drag_surface_id = SurfaceId(0);
            }
            // Perform drag: move surface by mouse delta
            if drag_surface_id != SurfaceId(0) && mb != 0 {
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
                    let _ = crate::ipc::send(chan.0, crate::process::Pid(0), &buf[..1 + n]);
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
            let mut frame_damage = comp.damage;

            if comp.surfaces.is_empty()
                && crate::scheme::orbital_bridge::WINDOW_COUNT.load(core::sync::atomic::Ordering::Relaxed) == 0
            {
                let repaint = no_surface_repaint(wallpaper_drawn, comp.damage, fb_w, fb_h);
                draw_wallpaper(swapchain.draw_mut(), fb_w, fb_h, &repaint);
                wallpaper_drawn = true;
                comp.damage = Rect::default();
                frame_damage = frame_damage.union(repaint);
            } else if !comp.surfaces.is_empty() {
                if !wallpaper_drawn {
                    let full_screen = Rect {
                        x: 0,
                        y: 0,
                        w: fb_w,
                        h: fb_h,
                    };
                    draw_wallpaper(swapchain.draw_mut(), fb_w, fb_h, &full_screen);
                    wallpaper_drawn = true;
                    comp.damage = full_screen;
                }

                frame_damage = comp.composite(swapchain.draw_mut(), fb_w, fb_h, 32);
            } else {
                // OrbitalBridge windows exist — skip wallpaper to let direct-BGA
                // clients render their own content. Just clear damage tracking.
                if !wallpaper_drawn {
                    let full_screen = Rect {
                        x: 0, y: 0, w: fb_w, h: fb_h,
                    };
                    draw_wallpaper(swapchain.draw_mut(), fb_w, fb_h, &full_screen);
                    wallpaper_drawn = true;
                    frame_damage = full_screen;
                }
                comp.damage = Rect::default();
            }

            draw_cursor(swapchain.draw_mut(), fb_w, fb_h, mx, my, current_cursor_shape);
            frame_damage = frame_damage
                .union(cursor_damage_rect(last_mouse_x, last_mouse_y, fb_w, fb_h, current_cursor_shape))
                .union(cursor_damage_rect(mx, my, fb_w, fb_h, current_cursor_shape));
            // ponytail: Bulk VRAM copy — full-width → single memcpy, else row-by-row.
            let clip = frame_damage.intersect(Rect { x: 0, y: 0, w: fb_w, h: fb_h });
            if !clip.is_empty() {
                let stride = (fb_w * 4) as usize;
                #[cfg(feature = "zig-hotpaths")]
                {
                    let start = clip.y as usize * stride + (clip.x * 4) as usize;
                    let sub_fb = unsafe { swapchain.buffers[swapchain.pending_idx].as_mut_ptr().add(start) };

                    if comp.crt_effect {
                        unsafe {
                            crate::zig_ffi::scanline_overlay(
                                sub_fb,
                                stride as u32,
                                clip.w,
                                clip.h,
                                40,
                                clip.y,
                            );
                        }
                    }
                    if comp.vignette_effect {
                        apply_vignette_to_clip(
                            &mut swapchain.buffers[swapchain.pending_idx],
                            stride,
                            &clip,
                            fb_w,
                            fb_h,
                        );
                    }
                }
                if clip.x == 0 && clip.w == fb_w {
                    // Full-width: contiguous block in both backbuffer and VRAM
                    let start = clip.y as usize * stride;
                    let total = clip.h as usize * stride;
                    if start + total <= fb_size {
                        unsafe {
                            core::ptr::copy_nonoverlapping(
                                swapchain.buffers[swapchain.pending_idx].as_ptr().add(start),
                                fb_slice.as_mut_ptr().add(start),
                                total,
                            );
                        }
                    }
                } else {
                    // Partial-width: row-by-row (stride gap between rows in VRAM)
                    let row_bytes = (clip.w * 4) as usize;
                    for row in 0..clip.h {
                        let y = (clip.y + row) as usize;
                        let off = y * stride + (clip.x * 4) as usize;
                        if off + row_bytes <= fb_size {
                            unsafe {
                                core::ptr::copy_nonoverlapping(
                                    swapchain.buffers[swapchain.pending_idx].as_ptr().add(off),
                                    fb_slice.as_mut_ptr().add(off),
                                    row_bytes,
                                );
                            }
                        }
                    }
                }
            }

            swapchain.advance();

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

        // 3f. Frame pacing — always sleep remainder of 16ms frame.
        // ponytail: sleep even after redraw. Without this, continuous damage
        // (e.g. animated client) causes compositor to hot-spin 100% CPU,
        // starving QEMU emulation and other kthreads.
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

    let fb_ptr = fb.as_mut_ptr();
    for y in y0..y1 {
        let t = y as u32;
        let inv = h - 1 - y;
        let b = ((0x1A * inv + 0x2A * t) / (h - 1).max(1)) as u32;
        let g = ((0x0E * inv + 0x1A * t) / (h - 1).max(1)) as u32;
        let r = ((0x0A * inv + 0x0F * t) / (h - 1).max(1)) as u32;
        let color: u32 = b | (g << 8) | (r << 16) | 0xFF_00_00_00;
        let row_off = (y as usize) * stride;
        let row_ptr = unsafe { fb_ptr.add(row_off) as *mut u32 };
        let xlen = x1 - x0;
        if row_off + xlen * 4 <= fb.len() {
            unsafe {
                let slice = core::slice::from_raw_parts_mut(row_ptr.add(x0), xlen);
                slice.fill(color);
            }
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
// I-beam cursor: 7 wide, 19 tall, hotspot (3, 9)
const IBEAM_W: usize = 7;
const IBEAM_H: usize = 19;
#[rustfmt::skip]
static IBEAM_BITMAP: [(u8, u8); IBEAM_H] = [
    // (outline, fill) — LSB = leftmost pixel
    (0b0001000, 0b0000000), //  0:  .X.
    (0b0001000, 0b0000000), //  1:  .X.
    (0b0001000, 0b0000000), //  2:  .X.
    (0b0001000, 0b0000000), //  3:  .X.
    (0b0111110, 0b0000000), //  4: XXXXX
    (0b0001000, 0b0000000), //  5:  .X.
    (0b0001000, 0b0000000), //  6:  .X.
    (0b0001000, 0b0000000), //  7:  .X.
    (0b0001000, 0b0000000), //  8:  .X.
    (0b0001000, 0b0000000), //  9:  .X.  (hotspot)
    (0b0001000, 0b0000000), // 10:  .X.
    (0b0001000, 0b0000000), // 11:  .X.
    (0b0001000, 0b0000000), // 12:  .X.
    (0b0001000, 0b0000000), // 13:  .X.
    (0b0111110, 0b0000000), // 14: XXXXX
    (0b0001000, 0b0000000), // 15:  .X.
    (0b0001000, 0b0000000), // 16:  .X.
    (0b0001000, 0b0000000), // 17:  .X.
    (0b0001000, 0b0000000), // 18:  .X.
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

fn surface_event_channel(surfaces: &[Surface], id: SurfaceId) -> Option<EventChannelId> {
    if id == SurfaceId(0) {
        return None;
    }
    for s in surfaces {
        if s.id == id && s.event_channel_id != EventChannelId(0) {
            return Some(s.event_channel_id);
        }
    }
    None
}

const fn target_event_channel(surface: Option<EventChannelId>, fallback: Option<EventChannelId>) -> Option<EventChannelId> {
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

const fn cursor_damage_rect(mx: i32, my: i32, fb_w: u32, fb_h: u32, shape: CursorShape) -> Rect {
    let fb_w = sat_i32(fb_w);
    let fb_h = sat_i32(fb_h);
    let (cw, ch) = match shape {
        CursorShape::Hidden => return Rect { x: 0, y: 0, w: 0, h: 0 },
        CursorShape::Default => (CURSOR_W as i32, CURSOR_H as i32),
        CursorShape::Text => (IBEAM_W as i32, IBEAM_H as i32),
    };
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
    let full = cursor_damage_rect(4, 5, 320, 200, CursorShape::Default);
    assert!(full.x == 4 && full.y == 5 && full.w == CURSOR_W as u32 && full.h == CURSOR_H as u32);

    let clipped = cursor_damage_rect(-4, -5, 320, 200, CursorShape::Default);
    assert!(clipped.x == 0 && clipped.y == 0 && clipped.w == 8 && clipped.h == 14);

    let hit = local_mouse_hit(25, 35, 20, 30, 100, 80);
    assert!(hit.0 && hit.1 == 5 && hit.2 == 5);

    let miss = local_mouse_hit(10, 35, 20, 30, 100, 80);
    assert!(!miss.0);

    assert!(should_raise_surface(0, 2));
    assert!(!should_raise_surface(1, 2));
    assert!(!should_raise_surface(0, 1));

    assert!(matches!(target_event_channel(Some(EventChannelId(7)), Some(EventChannelId(3))), Some(EventChannelId(7))));
    assert!(matches!(target_event_channel(None, Some(EventChannelId(3))), Some(EventChannelId(3))));
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
    // Keybinding resolver self-checks
    assert!(matches!(resolve_keybinding(MOD_SUPER, KeyCode::Q as u8), Some(Action::CloseFocused)));
    assert!(matches!(resolve_keybinding(MOD_SUPER | MOD_SHIFT, KeyCode::Tab as u8), Some(Action::FocusPrev)));
    assert!(matches!(resolve_keybinding(MOD_SUPER, KeyCode::Tab as u8), Some(Action::FocusNext)));
    assert!(matches!(resolve_keybinding(MOD_SHIFT, KeyCode::Q as u8), None)); // Shift alone doesn't match Super+Q
    assert!(matches!(resolve_keybinding(MOD_SUPER | MOD_CTRL, KeyCode::Q as u8), None)); // Extra mod doesn't match
};

/// Draw a proper arrow cursor at the mouse position.
/// Black outline + white fill, fully clipped to framebuffer bounds.
fn draw_cursor(fb: &mut [u8], fb_w: u32, fb_h: u32, mx: i32, my: i32, shape: CursorShape) {
    match shape {
        CursorShape::Hidden => {},
        CursorShape::Default => {
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
        CursorShape::Text => {
            let stride = (fb_w as usize) * 4;
            for row in 0..IBEAM_H {
                let py = my + row as i32;
                if py < 0 || py >= fb_h as i32 {
                    continue;
                }
                let (outline, fill) = IBEAM_BITMAP[row];
                for col in 0..IBEAM_W {
                    let px = mx + col as i32;
                    if px < 0 || px >= fb_w as i32 {
                        continue;
                    }
                    let bit = 1u8 << col;
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
    }
}
