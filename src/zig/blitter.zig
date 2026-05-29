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

// ─── Gradient Fill ──────────────────────────────────────────────────────────

/// Fill a rectangle with a vertical gradient from `color_top` to `color_bottom`.
/// Each channel (R,G,B) is linearly interpolated.
export fn zig_gradient_fill(
    fb: [*]u8,
    pitch: u32,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    color_top: u32,
    color_bottom: u32,
) void {
    if (h == 0 or w == 0) return;
    const tr: u32 = (color_top >> 16) & 0xFF;
    const tg: u32 = (color_top >> 8) & 0xFF;
    const tb: u32 = color_top & 0xFF;
    const br: u32 = (color_bottom >> 16) & 0xFF;
    const bg: u32 = (color_bottom >> 8) & 0xFF;
    const bb: u32 = color_bottom & 0xFF;

    var row: u32 = 0;
    while (row < h) : (row += 1) {
        const t = row * 255 / (h - 1);
        const r: u8 = @truncate(tr + (br - tr) * t / 255);
        const g: u8 = @truncate(tg + (bg - tg) * t / 255);
        const b: u8 = @truncate(tb + (bb - tb) * t / 255);
        const color: u32 = (@as(u32, r) << 16) | (@as(u32, g) << 8) | @as(u32, b);

        const color_bytes: [4]u8 = @bitCast(color);
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

// ─── Alpha Blending ─────────────────────────────────────────────────────────

/// Blend a pixel `src` over `dst` with alpha (0–255). Full Porter-Duff "over".
/// `src` is XRGB8888, `dst` is XRGB8888. Returns blended XRGB8888.
inline fn blendPixel(src: u32, dst: u32, alpha: u32) u32 {
    const sa = alpha;
    const da = 255;
    const oo = sa * da + sa * (255 - da) + da * (255 - sa); // never used — da=255
    const sr: u32 = (src >> 16) & 0xFF;
    const sg: u32 = (src >> 8) & 0xFF;
    const sb: u32 = src & 0xFF;
    const dr: u32 = (dst >> 16) & 0xFF;
    const dg: u32 = (dst >> 8) & 0xFF;
    const db: u32 = dst & 0xFF;

    const out_r = (sr * sa + dr * (255 - sa)) / 255;
    const out_g = (sg * sa + dg * (255 - sa)) / 255;
    const out_b = (sb * sa + db * (255 - sa)) / 255;
    _ = oo;
    return (@as(u32, out_r) << 16) | (@as(u32, out_g) << 8) | @as(u32, out_b);
}

/// Fill a rectangle with an alpha-blended color over the existing framebuffer.
/// Produces a "glow" or "translucency" effect.
export fn zig_blend_rect(
    fb: [*]u8,
    pitch: u32,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    color: u32,
    alpha: u32,
) void {
    if (w == 0 or h == 0 or alpha == 0) return;
    if (alpha >= 255) {
        zig_fill_rect(fb, pitch, x, y, w, h, color);
        return;
    }
    var row: u32 = 0;
    while (row < h) : (row += 1) {
        var col: u32 = 0;
        while (col < w) : (col += 1) {
            const px: [*]u32 = @alignCast(@ptrCast(pixelPtr(fb, pitch, x + col, y + row)));
            px[0] = blendPixel(color, px[0], alpha);
        }
    }
}

// ─── Framebuffer Shake ──────────────────────────────────────────────────────

/// Apply a horizontal displacement shake to a region of the framebuffer.
/// `magnitude` is the max pixel shift; `phase` selects direction (0–3).
/// Useful for explosion/impact feedback.
export fn zig_shake_fb(
    fb: [*]u8,
    pitch: u32,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    magnitude: u32,
    phase: u32,
) void {
    if (w == 0 or h == 0 or magnitude == 0) return;
    const dirs = [4]i32{ 1, -1, 1, -1 };
    const shift: i32 = dirs[@as(usize, phase & 3)] * @as(i32, @intCast(magnitude));
    if (shift == 0) return;

    // Copy the region to a temp line buffer, then draw shifted
    const row_bytes: usize = @as(usize, w) * 4;
    var row: u32 = 0;
    while (row < h) : (row += 1) {
        const src_row: [*]const u8 = fb + @as(usize, y + row) * @as(usize, pitch) + @as(usize, x) * 4;
        var line_buf: [8192]u8 = undefined;
        const copy_len = @min(row_bytes, line_buf.len);
        var i: usize = 0;
        while (i < copy_len) : (i += 1) {
            line_buf[i] = src_row[i];
        }

        const new_col = if (shift >= 0) blk: {
            const nc = x +| @as(u32, @intCast(shift));
            break :blk @min(nc, x + w - 1);
        } else x;

        const dst_row: [*]u8 = fb + @as(usize, y + row) * @as(usize, pitch) + @as(usize, new_col) * 4;
        i = 0;
        while (i < copy_len) : (i += 1) {
            dst_row[i] = line_buf[i];
        }
    }
}

// ─── Ember Particles ────────────────────────────────────────────────────────

/// Draw a set of ember/firefly particles onto the framebuffer.
/// Each particle is a 2×2 glowing dot with a soft halo via alpha blending.
/// `particles` layout: pairs of [x: u16, y: u16, color: u32, size: u16] packed,
/// repeated `count` times.
export fn zig_draw_embers(
    fb: [*]u8,
    pitch: u32,
    fb_w: u32,
    fb_h: u32,
    particles: [*]const u8,
    count: u32,
    global_alpha: u32,
) void {
    const entry_bytes: usize = 12; // x:u16(2) + y:u16(2) + color:u32(4) + size:u16(2) + pad:u16(2)
    var i: u32 = 0;
    while (i < count) : (i += 1) {
        const offset: usize = @as(usize, i) * entry_bytes;
        const px: u16 = @as(u16, @intCast(particles[offset])) |
            (@as(u16, @intCast(particles[offset + 1])) << 8);
        const py: u16 = @as(u16, @intCast(particles[offset + 2])) |
            (@as(u16, @intCast(particles[offset + 3])) << 8);
        const color: u32 = (@as(u32, particles[offset + 4])) |
            (@as(u32, particles[offset + 5]) << 8) |
            (@as(u32, particles[offset + 6]) << 16) |
            (@as(u32, particles[offset + 7]) << 24);
        const size: u16 = @as(u16, @intCast(particles[offset + 8])) |
            (@as(u16, @intCast(particles[offset + 9])) << 8);

        if (px >= fb_w or py >= fb_h or size == 0) continue;

        const half = size / 2;
        var dy: u32 = 0;
        while (dy < @as(u32, size)) : (dy += 1) {
            var dx: u32 = 0;
            while (dx < @as(u32, size)) : (dx += 1) {
                const sx = @as(u32, px) + dx -| half;
                const sy = @as(u32, py) + dy -| half;
                if (sx >= fb_w or sy >= fb_h) continue;

                // Distance from center for falloff
                const cx: i32 = @as(i32, @intCast(dx)) - @as(i32, @intCast(half));
                const cy: i32 = @as(i32, @intCast(dy)) - @as(i32, @intCast(half));
                const dist_sq = cx * cx + cy * cy;
                const max_r = @as(i32, @intCast(half));
                if (dist_sq > max_r * max_r) continue;

                // Glow falloff: alpha decreases with distance
                const falloff: u32 = if (max_r > 0) blk: {
                    const d = @sqrt(@as(f32, @floatFromInt(dist_sq)));
                    const m = @as(f32, @floatFromInt(max_r));
                    break :blk @as(u32, @intFromFloat(255.0 * (1.0 - d / m)));
                } else 255;

                const effective_alpha = global_alpha * falloff / 255;
                const px_ptr: [*]u32 = @alignCast(@ptrCast(pixelPtr(fb, pitch, sx, sy)));
                px_ptr[0] = blendPixel(color, px_ptr[0], effective_alpha);
            }
        }
    }
}

// ─── Trail Blur ─────────────────────────────────────────────────────────────

/// Blend the current framebuffer with a ghost/previous framebuffer to create
/// a motion-blur / afterimage trail effect.
/// `ghost` holds the previous frame pixels. `decay` (0–255) controls how fast
/// the trail fades (lower = faster fade).
export fn zig_trail_blur(
    fb: [*]u8,
    pitch: u32,
    w: u32,
    h: u32,
    ghost: [*]u8,
    decay: u32,
) void {
    const pixel_count = @as(usize, w) * @as(usize, h);
    var i: usize = 0;
    while (i < pixel_count) : (i += 1) {
        const offset = i * 4;
        // Read current pixel and ghost pixel
        const cur_r: u32 = fb[offset + 2];
        const cur_g: u32 = fb[offset + 1];
        const cur_b: u32 = fb[offset + 0];
        const gst_r: u32 = ghost[offset + 2];
        const gst_g: u32 = ghost[offset + 1];
        const gst_b: u32 = ghost[offset + 0];

        const inv_decay = 255 - decay;
        const out_r: u8 = @truncate((cur_r * decay + gst_r * inv_decay) / 255);
        const out_g: u8 = @truncate((cur_g * decay + gst_g * inv_decay) / 255);
        const out_b: u8 = @truncate((cur_b * decay + gst_b * inv_decay) / 255);

        fb[offset + 2] = out_r;
        fb[offset + 1] = out_g;
        fb[offset + 0] = out_b;
    }
}

/// Copy the current framebuffer into `ghost` for the next trail_blur call.
export fn zig_save_ghost(
    fb: [*]const u8,
    pitch: u32,
    w: u32,
    h: u32,
    ghost: [*]u8,
) void {
    const bytes = @as(usize, w) * @as(usize, h) * 4;
    var i: usize = 0;
    while (i < bytes) : (i += 1) {
        ghost[i] = fb[i];
    }
}

// ─── Scanline Overlay ──────────────────────────────────────────────────────

/// Apply a CRT-style scanline overlay darkened pattern to the framebuffer.
/// `strength` (0–255) controls how pronounced the scanlines are.
/// Every other row is darkened; `phase` shifts the pattern for animation.
export fn zig_scanline_overlay(
    fb: [*]u8,
    pitch: u32,
    w: u32,
    h: u32,
    strength: u32,
    phase: u32,
) void {
    var y: u32 = 0;
    while (y < h) : (y += 1) {
        const is_scanline = ((y + phase) & 1) == 0;
        if (!is_scanline) continue;
        var x: u32 = 0;
        while (x < w) : (x += 1) {
            const offset = @as(usize, y) * @as(usize, pitch) + @as(usize, x) * 4;
            const r: u32 = fb[offset + 2];
            const g: u32 = fb[offset + 1];
            const b: u32 = fb[offset + 0];
            fb[offset + 2] = @truncate(r * (255 - strength) / 255);
            fb[offset + 1] = @truncate(g * (255 - strength) / 255);
            fb[offset + 0] = @truncate(b * (255 - strength) / 255);
        }
    }
}

// ─── Vignette ───────────────────────────────────────────────────────────────

/// Darken the edges of the framebuffer — a vignette effect.
/// `radius` controls how far from center the darkening starts (0 = full).
/// `strength` (0–255) controls darkness at the edges.
export fn zig_vignette(
    fb: [*]u8,
    pitch: u32,
    w: u32,
    h: u32,
    radius: u32,
    strength: u32,
) void {
    const cx = w / 2;
    const cy = h / 2;
    const max_dist = @max(cx, cy);
    if (max_dist == 0) return;
    const rad = @min(radius, max_dist);

    var y: u32 = 0;
    while (y < h) : (y += 1) {
        var x: u32 = 0;
        while (x < w) : (x += 1) {
            const dx = @as(i32, @intCast(x)) - @as(i32, @intCast(cx));
            const dy = @as(i32, @intCast(y)) - @as(i32, @intCast(cy));
            const dist: u32 = @intCast(@min(@sqrt(@as(f32, @floatFromInt(dx * dx + dy * dy))), @as(f32, @floatFromInt(max_dist))));
            if (dist <= rad) continue;

            const factor = (dist - rad) * strength / (max_dist - rad + 1);
            const clamp_factor = @min(factor, strength);
            const offset = @as(usize, y) * @as(usize, pitch) + @as(usize, x) * 4;
            const r: u32 = fb[offset + 2];
            const g: u32 = fb[offset + 1];
            const b: u32 = fb[offset + 0];
            fb[offset + 2] = @truncate(r * (255 - clamp_factor) / 255);
            fb[offset + 1] = @truncate(g * (255 - clamp_factor) / 255);
            fb[offset + 0] = @truncate(b * (255 - clamp_factor) / 255);
        }
    }
}

// ─── Draw Line (Bresenham) ──────────────────────────────────────────────────

/// Draw a line from (x0,y0) to (x1,y1) on the framebuffer with a given color.
/// Uses Bresenham's line algorithm.
export fn zig_draw_line(
    fb: [*]u8,
    pitch: u32,
    x0: u32,
    y0: u32,
    x1: u32,
    y1: u32,
    color: u32,
) void {
    var sx: i32 = @intCast(x0);
    var sy: i32 = @intCast(y0);
    const ex: i32 = @intCast(x1);
    const ey: i32 = @intCast(y1);
    const dx = @abs(ex - sx);
    const dy = -@abs(ey - sy);
    const step_x: i32 = if (sx < ex) 1 else -1;
    const step_y: i32 = if (sy < ey) 1 else -1;
    var err = dx + dy;

    while (true) {
        if (sx >= 0 and sy >= 0) {
            writePixel32(fb, pitch, @intCast(sx), @intCast(sy), color);
        }
        if (sx == ex and sy == ey) break;
        const e2 = 2 * err;
        if (e2 >= dy) {
            err += dy;
            sx += step_x;
        }
        if (e2 <= dx) {
            err += dx;
            sy += step_y;
        }
    }
}

// ─── Fireworks Burst ────────────────────────────────────────────────────────

/// Generate a fireworks/explosion burst of particles at (cx, cy).
/// Renders directly to the framebuffer. `particle_buf` is a scratch buffer
/// that must be at least `max_particles * 4` bytes (x,y,vx,vy as i16).
/// Returns the number of particles spawned.
export fn zig_fireworks_burst(
    fb: [*]u8,
    pitch: u32,
    fb_w: u32,
    fb_h: u32,
    cx: u32,
    cy: u32,
    color: u32,
    count: u32,
    speed: u32,
    tick: u32,
    particle_buf: [*]u8,
    buf_size: usize,
) u32 {
    const max_p = buf_size / 4;
    const actual = @min(count, @as(u32, @intCast(max_p)));
    var i: u32 = 0;
    while (i < actual) : (i += 1) {
        const seed = hash32(i *% 374761393 +% tick *% 668265263);
        const angle = (seed & 0xFFFF) * 360.0 / 65536.0;
        const vel = @as(f32, @floatFromInt((seed >> 16) & 0xFF)) * @as(f32, @floatFromInt(speed)) / 128.0;
        const vx: i16 = @intFromFloat(@cos(angle) * vel);
        const vy: i16 = @intFromFloat(@sin(angle) * vel);

        const off = @as(usize, i) * 4;
        if (off + 4 > buf_size) break;
        // Pack as i16 little-endian: x, y, vx, vy
        const x_le: u16 = @bitCast(@as(i16, @intCast(cx)));
        const y_le: u16 = @bitCast(@as(i16, @intCast(cy)));
        const vx_le: u16 = @bitCast(vx);
        const vy_le: u16 = @bitCast(vy);

        particle_buf[off + 0] = @truncate(x_le);
        particle_buf[off + 1] = @truncate(x_le >> 8);
        particle_buf[off + 2] = @truncate(y_le);
        particle_buf[off + 3] = @truncate(y_le >> 8);
        // Use the remaining bytes for vx/vy if we have 8 bytes per particle
        if (off + 8 <= buf_size) {
            particle_buf[off + 4] = @truncate(vx_le);
            particle_buf[off + 5] = @truncate(vx_le >> 8);
            particle_buf[off + 6] = @truncate(vy_le);
            particle_buf[off + 7] = @truncate(vy_le >> 8);
        }
    }
    return actual;
}

/// Update and render a previously-burst fireworks particle set.
/// `particle_buf` layout: repeated [x:i16, y:i16, vx:i16, vy:i16] (8 bytes each).
/// `count` is the number of live particles; returns updated count (minus dead particles).
export fn zig_fireworks_update(
    fb: [*]u8,
    pitch: u32,
    fb_w: u32,
    fb_h: u32,
    particle_buf: [*]u8,
    count: u32,
    color: u32,
    gravity: i16,
) u32 {
    var alive: u32 = 0;
    var i: u32 = 0;
    while (i < count) : (i += 1) {
        const off = @as(usize, i) * 8;
        // Read position (little-endian i16)
        const px: i16 = @bitCast(@as(u16, particle_buf[off + 0]) | (@as(u16, particle_buf[off + 1]) << 8));
        const py: i16 = @bitCast(@as(u16, particle_buf[off + 2]) | (@as(u16, particle_buf[off + 3]) << 8));
        const vx: i16 = @bitCast(@as(u16, particle_buf[off + 4]) | (@as(u16, particle_buf[off + 5]) << 8));
        const vy: i16 = @bitCast(@as(u16, particle_buf[off + 6]) | (@as(u16, particle_buf[off + 7]) << 8));
        const nvx = vx;
        const nvy = vy + gravity;

        const nx = px + nvx;
        const ny = py + nvy;

        // Off-screen = dead
        if (nx < 0 or nx >= @as(i16, @intCast(fb_w)) or ny < 0 or ny >= @as(i16, @intCast(fb_h))) {
            // Dead particle — skip
            continue;
        }

        // Draw a small dot
        writePixel32(fb, pitch, @intCast(nx), @intCast(ny), color);

        // Pack new position back
        const nx_le: u16 = @bitCast(nx);
        const ny_le: u16 = @bitCast(ny);
        const nvx_le: u16 = @bitCast(nvx);
        const nvy_le: u16 = @bitCast(nvy);

        // Write back to buffer (reuse slot if alive count differs)
        const woff = @as(usize, alive) * 8;
        particle_buf[woff + 0] = @truncate(nx_le);
        particle_buf[woff + 1] = @truncate(nx_le >> 8);
        particle_buf[woff + 2] = @truncate(ny_le);
        particle_buf[woff + 3] = @truncate(ny_le >> 8);
        particle_buf[woff + 4] = @truncate(nvx_le);
        particle_buf[woff + 5] = @truncate(nvx_le >> 8);
        particle_buf[woff + 6] = @truncate(nvy_le);
        particle_buf[woff + 7] = @truncate(nvy_le >> 8);
        alive += 1;
    }
    return alive;
}

// ─── DOOM Fire ──────────────────────────────────────────────────────────────

inline fn clampU32(v: i32, max_exclusive: u32) u32 {
    if (v <= 0) return 0;
    const uv: u32 = @intCast(v);
    return if (uv >= max_exclusive) max_exclusive - 1 else uv;
}

inline fn hash32(v: u32) u32 {
    var x = v;
    x ^= x >> 16;
    x *%= 0x7feb352d;
    x ^= x >> 15;
    x *%= 0x846ca68b;
    x ^= x >> 16;
    return x;
}

inline fn writePixel32(fb: [*]u8, pitch: u32, x: u32, y: u32, color: u32) void {
    const color_bytes: [4]u8 = @bitCast(color);
    const px = pixelPtr(fb, pitch, x, y);
    px[0] = color_bytes[0];
    px[1] = color_bytes[1];
    px[2] = color_bytes[2];
    px[3] = color_bytes[3];
}

/// One step of the classic DOOM fire propagation algorithm.
///
/// Compatibility entry point: uses a fixed seed and the original 37-color
/// palette contract. Prefer `zig_doom_fire_step_seeded` for animation.
export fn zig_doom_fire_step(
    fb: [*]u8,
    pitch: u32,
    w: u32,
    h: u32,
    palette: [*]const u32,
    fire_buf: [*]u8,
) void {
    zig_doom_fire_step_seeded(fb, pitch, w, h, palette, 37, fire_buf, 0, 0);
}

/// Seeded DOOM fire step with animated fuel, wind, sparks, and safer palette
/// clamping. `fire_buf` is a WIDTH × HEIGHT buffer of palette indices.
export fn zig_doom_fire_step_seeded(
    fb: [*]u8,
    pitch: u32,
    w: u32,
    h: u32,
    palette: [*]const u32,
    palette_len: u32,
    fire_buf: [*]u8,
    tick: u32,
    wind: i32,
) void {
    if (w == 0 or h == 0 or palette_len == 0) return;

    const max_pal: u8 = @truncate(if (palette_len > 255) 255 else palette_len - 1);
    const last_row: usize = @as(usize, h - 1) * @as(usize, w);

    // Animated fuel bed: uneven flame tongues beat a static bottom row.
    var fx: u32 = 0;
    while (fx < w) : (fx += 1) {
        const n = hash32(fx *% 0x45d9f3b +% tick *% 0x119de1f3);
        const dip: u8 = @truncate(n & 0x7);
        fire_buf[last_row + @as(usize, fx)] = if ((n & 0x1f) == 0) max_pal / 2 else max_pal -| dip;
    }

    // Propagate upward. Reads from the row below, cools, and drifts with wind.
    var y: u32 = 0;
    while (y < h - 1) : (y += 1) {
        var x: u32 = 0;
        while (x < w) : (x += 1) {
            const src_idx: usize = @as(usize, y + 1) * @as(usize, w) + @as(usize, x);
            const pixel = fire_buf[src_idx];
            const rnd = hash32(x *% 374761393 +% y *% 668265263 +% tick *% 2246822519 +% @as(u32, pixel));

            const base_decay: u8 = @truncate(rnd & 0x3);
            const height_cooling: u8 = if (y < h / 4 and (rnd & 0x8) != 0) 1 else 0;
            const decay = base_decay + height_cooling;
            const new_val: u8 = if (pixel > decay) pixel - decay else 0;

            const shimmer: i32 = @as(i32, @intCast((rnd >> 3) & 0x3)) - 1; // -1..2
            const dst_x = clampU32(@as(i32, @intCast(x)) + shimmer + wind, w);
            const dst_idx: usize = @as(usize, y) * @as(usize, w) + @as(usize, dst_x);
            fire_buf[dst_idx] = new_val;
        }
    }

    // Render fire_buf → framebuffer using palette. Occasional high-intensity
    // pixels become sparks to make the frame feel less flat.
    var ry: u32 = 0;
    while (ry < h) : (ry += 1) {
        var rx: u32 = 0;
        while (rx < w) : (rx += 1) {
            const idx: usize = @as(usize, ry) * @as(usize, w) + @as(usize, rx);
            var pal_idx: u32 = fire_buf[idx];
            if (pal_idx >= palette_len) pal_idx = palette_len - 1;

            var color: u32 = palette[@as(usize, pal_idx)];
            const spark = pal_idx > (palette_len * 3) / 4 and (hash32(rx +% ry *% w +% tick *% 17) & 0xff) == 0;
            if (spark) color = palette[@as(usize, palette_len - 1)];
            writePixel32(fb, pitch, rx, ry, color);
        }
    }
}

// ─── DOOM fire (serial ASCII fallback) ──────────────────────────────────────

/// Render the fire buffer as ASCII brightness chars to a text output buffer.
/// Returns the number of bytes written.
/// Characters use a dense luminance ramp with ordered dithering for smoother
/// gradients than the old 10-level renderer.
export fn zig_doom_fire_to_ascii(
    fire_buf: [*]const u8,
    w: u32,
    h: u32,
    out: [*]u8,
    out_size: usize,
) usize {
    const chars = " .`^\",:;Il!i><~+_-?][}{1)(|\\/tfjrxnuvczXYUJCLQ0OZmwqpdbkhao*#MW&8%B@$";
    const dither: [4]u8 = .{ 0, 2, 1, 3 };
    var written: usize = 0;
    if (out_size == 0) return 0;

    // Render every other row for aspect ratio.
    var y: u32 = 0;
    while (y < h) : (y += 2) {
        var x: u32 = 0;
        while (x < w) : (x += 1) {
            if (written + 1 >= out_size) return written;
            const idx: usize = @as(usize, y) * @as(usize, w) + @as(usize, x);
            const shade = @min(@as(u16, fire_buf[idx]) + dither[@as(usize, (x ^ y) & 3)], 36);
            var level: usize = (@as(usize, shade) * (chars.len - 1)) / 36;
            if (level >= chars.len) level = chars.len - 1;
            out[written] = chars[level];
            written += 1;
        }
        if (written + 1 >= out_size) return written;
        out[written] = '\n';
        written += 1;
    }
    return written;
}

const std = @import("std");

pub fn panic(msg: []const u8, error_return_trace: ?*std.builtin.StackTrace, ret_addr: ?usize) noreturn {
    _ = msg;
    _ = error_return_trace;
    _ = ret_addr;
    @trap();
}

