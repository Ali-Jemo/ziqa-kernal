// Zig FFI bindings for ZiqaKernel
//
// Safe Rust wrappers around the C-ABI functions exported by `src/zig/blitter.zig`.
// The Zig module is compiled as a static library and linked via `build.rs`.

// ── Raw extern declarations ─────────────────────────────────────────────────

extern "C" {
    fn zig_fill_rect(fb: *mut u8, pitch: u32, x: u32, y: u32, w: u32, h: u32, color: u32);

    fn zig_blit_bitmap(
        dst: *mut u8,
        pitch: u32,
        src: *const u8,
        sx: u32,
        sy: u32,
        sw: u32,
        sh: u32,
        dx: u32,
        dy: u32,
    );

    fn zig_scroll_up(fb: *mut u8, pitch: u32, w: u32, h: u32, lines: u32, fill_color: u32);

    fn zig_clear(fb: *mut u8, size: usize, color: u32);

    fn zig_memset32(dst: *mut u32, val: u32, count: usize);

    fn zig_memcpy(dst: *mut u8, src: *const u8, len: usize);

    fn zig_doom_fire_step(
        fb: *mut u8,
        pitch: u32,
        w: u32,
        h: u32,
        palette: *const u32,
        fire_buf: *mut u8,
    );

    fn zig_doom_fire_step_seeded(
        fb: *mut u8,
        pitch: u32,
        w: u32,
        h: u32,
        palette: *const u32,
        palette_len: u32,
        fire_buf: *mut u8,
        tick: u32,
        wind: i32,
    );

    fn zig_doom_fire_to_ascii(
        fire_buf: *const u8,
        w: u32,
        h: u32,
        out: *mut u8,
        out_size: usize,
    ) -> usize;

    fn zig_gradient_fill(
        fb: *mut u8,
        pitch: u32,
        x: u32,
        y: u32,
        w: u32,
        h: u32,
        color_top: u32,
        color_bottom: u32,
    );

    fn zig_blend_rect(
        fb: *mut u8,
        pitch: u32,
        x: u32,
        y: u32,
        w: u32,
        h: u32,
        color: u32,
        alpha: u32,
    );

    fn zig_shake_fb(
        fb: *mut u8,
        pitch: u32,
        x: u32,
        y: u32,
        w: u32,
        h: u32,
        magnitude: u32,
        phase: u32,
    );

    fn zig_draw_embers(
        fb: *mut u8,
        pitch: u32,
        fb_w: u32,
        fb_h: u32,
        particles: *const u8,
        count: u32,
        global_alpha: u32,
    );

    fn zig_trail_blur(fb: *mut u8, pitch: u32, w: u32, h: u32, ghost: *mut u8, decay: u32);

    fn zig_save_ghost(fb: *const u8, pitch: u32, w: u32, h: u32, ghost: *mut u8);

    fn zig_scanline_overlay(fb: *mut u8, pitch: u32, w: u32, h: u32, strength: u32, phase: u32);

    fn zig_vignette(fb: *mut u8, pitch: u32, w: u32, h: u32, radius: u32, strength: u32);

    fn zig_draw_line(fb: *mut u8, pitch: u32, x0: u32, y0: u32, x1: u32, y1: u32, color: u32);

    fn zig_fireworks_burst(
        fb: *mut u8,
        pitch: u32,
        fb_w: u32,
        fb_h: u32,
        cx: u32,
        cy: u32,
        color: u32,
        count: u32,
        speed: u32,
        tick: u32,
        particle_buf: *mut u8,
        buf_size: usize,
    ) -> u32;

    fn zig_fireworks_update(
        fb: *mut u8,
        pitch: u32,
        fb_w: u32,
        fb_h: u32,
        particle_buf: *mut u8,
        count: u32,
        color: u32,
        gravity: i16,
    ) -> u32;

    pub fn zig_demo_client_main();
}

/// Native Axiq-IQ Syscall Entry Point (for Zig/C FFI)
///
/// This allows Zig/C code linked into the kernel to make "syscalls"
/// through the same dispatcher as user-mode processes.
#[no_mangle]
pub extern "C" fn ziqa_syscall(num: u64, a1: u64, a2: u64, a3: u64, a4: u64, a5: u64, a6: u64) -> u64 {
    let mut scheduler = crate::process::scheduler::SCHEDULER.lock();
    if let Some(proc) = scheduler.current_task_mut() {
        let mut ctx = crate::abi::syscall::SyscallContext::new(num, [a1, a2, a3, a4, a5, a6], proc);
        let registry = crate::init_abi_registry();
        let handler = crate::abi::handler::KernelSyscallHandler;
        match crate::abi::syscall::dispatch_syscall(&registry, &handler, &mut ctx) {
            Ok(v) => v,
            Err(_) => u64::MAX, // Simplified error for FFI
        }
    } else {
        u64::MAX
    }
}

// ── Safe wrappers ───────────────────────────────────────────────────────────

/// Fill a rectangle in the framebuffer with XRGB8888 color.
///
/// # Safety
/// `fb` must point to a valid framebuffer of at least `pitch * (y + h)` bytes.
pub fn fill_rect(fb: *mut u8, pitch: u32, x: u32, y: u32, w: u32, h: u32, color: u32) {
    unsafe { zig_fill_rect(fb, pitch, x, y, w, h, color) }
}

/// Copy a rectangular sprite from `src` to `dst` framebuffer.
pub fn blit_bitmap(
    dst: *mut u8,
    pitch: u32,
    src: *const u8,
    sx: u32,
    sy: u32,
    sw: u32,
    sh: u32,
    dx: u32,
    dy: u32,
) {
    unsafe { zig_blit_bitmap(dst, pitch, src, sx, sy, sw, sh, dx, dy) }
}

/// Scroll framebuffer up by `lines` rows, filling the bottom with `fill_color`.
pub fn scroll_up(fb: *mut u8, pitch: u32, w: u32, h: u32, lines: u32, fill_color: u32) {
    unsafe { zig_scroll_up(fb, pitch, w, h, lines, fill_color) }
}

/// Clear the entire framebuffer (`size` bytes) with XRGB8888 `color`.
pub fn clear(fb: *mut u8, size: usize, color: u32) {
    unsafe { zig_clear(fb, size, color) }
}

/// Set `count` 32-bit words at `dst` to `val`.
pub fn memset32(dst: *mut u32, val: u32, count: usize) {
    unsafe { zig_memset32(dst, val, count) }
}

/// Copy `len` bytes from `src` to `dst`.
pub fn memcpy(dst: *mut u8, src: *const u8, len: usize) {
    unsafe { zig_memcpy(dst, src, len) }
}

/// Fill a rectangle with a vertical gradient.
pub fn gradient_fill(
    fb: *mut u8,
    pitch: u32,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    color_top: u32,
    color_bottom: u32,
) {
    unsafe { zig_gradient_fill(fb, pitch, x, y, w, h, color_top, color_bottom) }
}

/// Fill a rectangle with an alpha-blended color over the existing framebuffer.
pub fn blend_rect(
    fb: *mut u8,
    pitch: u32,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    color: u32,
    alpha: u32,
) {
    unsafe { zig_blend_rect(fb, pitch, x, y, w, h, color, alpha) }
}

/// Apply a horizontal shake to a framebuffer region (for explosion feedback).
pub fn shake_fb(
    fb: *mut u8,
    pitch: u32,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    magnitude: u32,
    phase: u32,
) {
    unsafe { zig_shake_fb(fb, pitch, x, y, w, h, magnitude, phase) }
}

/// Draw a set of ember/firefly particles with glow halos onto the framebuffer.
///
/// `particles` is a flat slice of packed entries: each entry is
/// `[x: u16, y: u16, color: u32, size: u16, pad: u16]` = 12 bytes.
pub fn draw_embers(
    fb: *mut u8,
    pitch: u32,
    fb_w: u32,
    fb_h: u32,
    particles: &[u8],
    count: u32,
    global_alpha: u32,
) {
    unsafe { zig_draw_embers(fb, pitch, fb_w, fb_h, particles.as_ptr(), count, global_alpha) }
}

/// Run one step of the DOOM fire propagation + render.
pub fn doom_fire_step(
    fb: *mut u8,
    pitch: u32,
    w: u32,
    h: u32,
    palette: &[u32],
    fire_buf: &mut [u8],
) {
    unsafe { zig_doom_fire_step(fb, pitch, w, h, palette.as_ptr(), fire_buf.as_mut_ptr()) }
}

/// Run one animated step of the DOOM fire propagation + render.
pub fn doom_fire_step_seeded(
    fb: *mut u8,
    pitch: u32,
    w: u32,
    h: u32,
    palette: &[u32],
    fire_buf: &mut [u8],
    tick: u32,
    wind: i32,
) {
    unsafe {
        zig_doom_fire_step_seeded(
            fb,
            pitch,
            w,
            h,
            palette.as_ptr(),
            palette.len() as u32,
            fire_buf.as_mut_ptr(),
            tick,
            wind,
        )
    }
}

/// Render the fire buffer as ASCII art into `out`, returning bytes written.
pub fn doom_fire_to_ascii(fire_buf: &[u8], w: u32, h: u32, out: &mut [u8]) -> usize {
    unsafe { zig_doom_fire_to_ascii(fire_buf.as_ptr(), w, h, out.as_mut_ptr(), out.len()) }
}

/// Apply motion-blur trail by blending with a saved ghost frame.
pub fn trail_blur(fb: *mut u8, pitch: u32, w: u32, h: u32, ghost: &mut [u8], decay: u32) {
    unsafe { zig_trail_blur(fb, pitch, w, h, ghost.as_mut_ptr(), decay) }
}

/// Save current framebuffer as a ghost for trail_blur.
pub fn save_ghost(fb: &[u8], pitch: u32, w: u32, h: u32, ghost: &mut [u8]) {
    unsafe { zig_save_ghost(fb.as_ptr(), pitch, w, h, ghost.as_mut_ptr()) }
}

/// Apply CRT scanline overlay (darkens every other row).
pub fn scanline_overlay(fb: *mut u8, pitch: u32, w: u32, h: u32, strength: u32, phase: u32) {
    unsafe { zig_scanline_overlay(fb, pitch, w, h, strength, phase) }
}

/// Darken edges of the framebuffer (vignette effect).
pub fn vignette(fb: *mut u8, pitch: u32, w: u32, h: u32, radius: u32, strength: u32) {
    unsafe { zig_vignette(fb, pitch, w, h, radius, strength) }
}

/// Draw a line using Bresenham's algorithm.
pub fn draw_line(fb: *mut u8, pitch: u32, x0: u32, y0: u32, x1: u32, y1: u32, color: u32) {
    unsafe { zig_draw_line(fb, pitch, x0, y0, x1, y1, color) }
}

/// Spawn a fireworks burst of particles returning how many were spawned.
pub fn fireworks_burst(
    fb: *mut u8,
    pitch: u32,
    fb_w: u32,
    fb_h: u32,
    cx: u32,
    cy: u32,
    color: u32,
    count: u32,
    speed: u32,
    tick: u32,
    particle_buf: &mut [u8],
) -> u32 {
    unsafe { zig_fireworks_burst(fb, pitch, fb_w, fb_h, cx, cy, color, count, speed, tick, particle_buf.as_mut_ptr(), particle_buf.len()) }
}

/// Update and render fireworks particles, returns number still alive.
pub fn fireworks_update(
    fb: *mut u8,
    pitch: u32,
    fb_w: u32,
    fb_h: u32,
    particle_buf: &mut [u8],
    count: u32,
    color: u32,
    gravity: i16,
) -> u32 {
    unsafe { zig_fireworks_update(fb, pitch, fb_w, fb_h, particle_buf.as_mut_ptr(), count, color, gravity) }
}

/// Simple 32-bit hash function (mirrors the one in blitter.zig).
pub fn hash32(v: u32) -> u32 {
    let mut x = v;
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846ca68b);
    x ^= x >> 16;
    x
}
