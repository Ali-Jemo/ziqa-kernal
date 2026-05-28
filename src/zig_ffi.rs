// Zig FFI bindings for ZiqaKernel
//
// Safe Rust wrappers around the C-ABI functions exported by `src/zig/blitter.zig`.
// The Zig module is compiled as a static library and linked via `build.rs`.

// ── Raw extern declarations ─────────────────────────────────────────────────

extern "C" {
    fn zig_fill_rect(
        fb: *mut u8,
        pitch: u32,
        x: u32,
        y: u32,
        w: u32,
        h: u32,
        color: u32,
    );

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

    fn zig_scroll_up(
        fb: *mut u8,
        pitch: u32,
        w: u32,
        h: u32,
        lines: u32,
        fill_color: u32,
    );

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

    fn zig_doom_fire_to_ascii(
        fire_buf: *const u8,
        w: u32,
        h: u32,
        out: *mut u8,
        out_size: usize,
    ) -> usize;
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
    sx: u32, sy: u32,
    sw: u32, sh: u32,
    dx: u32, dy: u32,
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

/// Run one step of the DOOM fire propagation + render.
pub fn doom_fire_step(
    fb: *mut u8,
    pitch: u32,
    w: u32,
    h: u32,
    palette: &[u32],
    fire_buf: &mut [u8],
) {
    unsafe {
        zig_doom_fire_step(fb, pitch, w, h, palette.as_ptr(), fire_buf.as_mut_ptr())
    }
}

/// Render the fire buffer as ASCII art into `out`, returning bytes written.
pub fn doom_fire_to_ascii(fire_buf: &[u8], w: u32, h: u32, out: &mut [u8]) -> usize {
    unsafe {
        zig_doom_fire_to_ascii(fire_buf.as_ptr(), w, h, out.as_mut_ptr(), out.len())
    }
}
