// ZiqaKernel Zig Blitter — high-performance framebuffer operations
//
// All functions use the C ABI (`export`) so Rust can call them via FFI.
// Compiled for x86_64-freestanding-none with -O ReleaseFast.
//
// Pixel format: XRGB8888 (4 bytes per pixel, 0x00RRGGBB)
// Pitch is in BYTES (width * 4 for XRGB8888).

// ─── Helpers ────────────────────────────────────────────────────────────────

inline fn pixelPtr(fb: [*]u8, pitch: u32, x: u32, y: u32) [*]u8 {
    const offset: usize = @as(usize, y) * @as(usize, pitch) + @as(usize, x) * 4;
    return fb + offset;
}

// ─── fill_rect ──────────────────────────────────────────────────────────────

/// Fill a rectangle at (x,y) of size (w,h) with a 32-bit XRGB color.
export fn zig_fill_rect(
    fb: [*]u8,
    pitch: u32,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    color: u32,
) void {
    const color_bytes: [4]u8 = @bitCast(color);
    var row: u32 = 0;
    while (row < h) : (row += 1) {
        const row_start = pixelPtr(fb, pitch, x, y + row);
        var col: u32 = 0;
        while (col < w) : (col += 1) {
            const px: [*]u8 = row_start + @as(usize, col) * 4;
            px[0] = color_bytes[0];
            px[1] = color_bytes[1];
            px[2] = color_bytes[2];
            px[3] = color_bytes[3];
        }
    }
}

// ─── blit_bitmap ────────────────────────────────────────────────────────────

/// Copy a rectangular region from src framebuffer to dst framebuffer.
/// Both framebuffers use the same pitch. Copies (sw × sh) pixels
/// from (sx, sy) in src to (dx, dy) in dst.
export fn zig_blit_bitmap(
    dst: [*]u8,
    pitch: u32,
    src: [*]const u8,
    sx: u32,
    sy: u32,
    sw: u32,
    sh: u32,
    dx: u32,
    dy: u32,
) void {
    var row: u32 = 0;
    while (row < sh) : (row += 1) {
        const src_offset: usize = @as(usize, sy + row) * @as(usize, pitch) + @as(usize, sx) * 4;
        const dst_offset: usize = @as(usize, dy + row) * @as(usize, pitch) + @as(usize, dx) * 4;
        const byte_count: usize = @as(usize, sw) * 4;

        const src_row: [*]const u8 = src + src_offset;
        const dst_row: [*]u8 = dst + dst_offset;

        // Manual copy — no libc available
        var i: usize = 0;
        while (i < byte_count) : (i += 1) {
            dst_row[i] = src_row[i];
        }
    }
}

// ─── scroll_up ──────────────────────────────────────────────────────────────

/// Scroll the framebuffer up by `lines` pixel rows, filling the bottom
/// with `fill_color`.
export fn zig_scroll_up(
    fb: [*]u8,
    pitch: u32,
    w: u32,
    h: u32,
    lines: u32,
    fill_color: u32,
) void {
    if (lines >= h) {
        // Just clear the whole thing
        zig_clear(fb, @as(usize, pitch) * @as(usize, h), fill_color);
        return;
    }

    // Move rows up
    const clamped_lines = if (lines > h) h else lines;
    var dst_y: u32 = 0;
    var src_y: u32 = clamped_lines;
    while (src_y < h) : ({
        dst_y += 1;
        src_y += 1;
    }) {
        const src_row: [*]const u8 = fb + @as(usize, src_y) * @as(usize, pitch);
        const dst_row: [*]u8 = fb + @as(usize, dst_y) * @as(usize, pitch);
        const row_bytes: usize = @as(usize, w) * 4;
        var i: usize = 0;
        while (i < row_bytes) : (i += 1) {
            dst_row[i] = src_row[i];
        }
    }

    // Fill bottom rows
    const fill_bytes: [4]u8 = @bitCast(fill_color);
    var fy: u32 = h - clamped_lines;
    while (fy < h) : (fy += 1) {
        const row_ptr = fb + @as(usize, fy) * @as(usize, pitch);
        var fx: u32 = 0;
        while (fx < w) : (fx += 1) {
            const px: [*]u8 = row_ptr + @as(usize, fx) * 4;
            px[0] = fill_bytes[0];
            px[1] = fill_bytes[1];
            px[2] = fill_bytes[2];
            px[3] = fill_bytes[3];
        }
    }
}

// ─── clear ──────────────────────────────────────────────────────────────────

/// Clear `size` bytes of framebuffer with a repeating 32-bit color value.
/// `size` should be the total framebuffer size in bytes (pitch * height).
export fn zig_clear(fb: [*]u8, size: usize, color: u32) void {
    const color_bytes: [4]u8 = @bitCast(color);
    const pixel_count = size / 4;
    var i: usize = 0;
    while (i < pixel_count) : (i += 1) {
        const px: [*]u8 = fb + i * 4;
        px[0] = color_bytes[0];
        px[1] = color_bytes[1];
        px[2] = color_bytes[2];
        px[3] = color_bytes[3];
    }
}

// ─── memset32 ───────────────────────────────────────────────────────────────

/// Set `count` 32-bit words at `dst` to `val`.
export fn zig_memset32(dst: [*]u32, val: u32, count: usize) void {
    var i: usize = 0;
    while (i < count) : (i += 1) {
        dst[i] = val;
    }
}

// ─── memcpy ─────────────────────────────────────────────────────────────────

/// Copy `len` bytes from `src` to `dst`. No overlap handling.
export fn zig_memcpy(dst: [*]u8, src: [*]const u8, len: usize) void {
    var i: usize = 0;
    while (i < len) : (i += 1) {
        dst[i] = src[i];
    }
}

// ─── DOOM Fire ──────────────────────────────────────────────────────────────

/// One step of the classic DOOM fire propagation algorithm.
///
/// `fire_buf` is a WIDTH × HEIGHT buffer of palette indices (u8, 0..36).
/// `palette` is a 37-entry array of XRGB8888 colors.
/// `fb` is the destination framebuffer.
///
/// Algorithm: for each pixel (except the bottom row), sample the pixel below,
/// decay it by 0 or 1, spread it left or right by 0–2 pixels, write it back.
/// Then render fire_buf → fb using the palette.
export fn zig_doom_fire_step(
    fb: [*]u8,
    pitch: u32,
    w: u32,
    h: u32,
    palette: [*]const u32,
    fire_buf: [*]u8,
) void {
    // Propagate fire upward
    // Use a simple pseudo-random from pixel position
    var y: u32 = 0;
    while (y < h -| 1) : (y += 1) {
        var x: u32 = 0;
        while (x < w) : (x += 1) {
            const src_idx: usize = @as(usize, y + 1) * @as(usize, w) + @as(usize, x);
            const pixel = fire_buf[src_idx];

            // Pseudo-random decay and spread
            // Use a simple hash of position for randomness
            const rand_val: u32 = (x *% 7 +% y *% 13 +% pixel) & 3;
            const decay: u8 = @truncate(rand_val & 1);

            const new_val: u8 = if (pixel > decay) pixel - decay else 0;

            // Spread: offset x by rand_val & 1, wrapping
            const spread: u32 = rand_val & 1;
            var dst_x: u32 = undefined;
            if (x >= spread) {
                dst_x = x - spread;
            } else {
                dst_x = 0;
            }
            if (dst_x >= w) dst_x = w - 1;

            const dst_idx: usize = @as(usize, y) * @as(usize, w) + @as(usize, dst_x);
            fire_buf[dst_idx] = new_val;
        }
    }

    // Render fire_buf → framebuffer using palette
    var ry: u32 = 0;
    while (ry < h) : (ry += 1) {
        var rx: u32 = 0;
        while (rx < w) : (rx += 1) {
            const idx: usize = @as(usize, ry) * @as(usize, w) + @as(usize, rx);
            const pal_idx: usize = @as(usize, fire_buf[idx]);
            const color: u32 = palette[pal_idx];
            const color_bytes: [4]u8 = @bitCast(color);

            const px = pixelPtr(fb, pitch, rx, ry);
            px[0] = color_bytes[0];
            px[1] = color_bytes[1];
            px[2] = color_bytes[2];
            px[3] = color_bytes[3];
        }
    }
}

// ─── DOOM fire (serial ASCII fallback) ──────────────────────────────────────

/// Render the fire buffer as ASCII brightness chars to a text output buffer.
/// Returns the number of bytes written.
/// Characters: " .:-=+*#%@" (10 levels, palette index / 4 clamped).
export fn zig_doom_fire_to_ascii(
    fire_buf: [*]const u8,
    w: u32,
    h: u32,
    out: [*]u8,
    out_size: usize,
) usize {
    const chars = " .:-=+*#%@";
    var written: usize = 0;

    // Render every other row for aspect ratio
    var y: u32 = 0;
    while (y < h) : (y += 2) {
        var x: u32 = 0;
        while (x < w) : (x += 1) {
            if (written >= out_size - 2) return written;
            const idx: usize = @as(usize, y) * @as(usize, w) + @as(usize, x);
            var level: usize = @as(usize, fire_buf[idx]) / 4;
            if (level > 9) level = 9;
            out[written] = chars[level];
            written += 1;
        }
        if (written >= out_size - 2) return written;
        out[written] = '\n';
        written += 1;
    }
    return written;
}
