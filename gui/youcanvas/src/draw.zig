const Rect = @import("rect.zig").Rect;
const Simd = @import("simd.zig").Simd;

pub const Canvas = struct {
    ptr: [*]u32,
    width: u32,
    height: u32,

    pub fn init(ptr: [*]u32, width: u32, height: u32) Canvas {
        return .{ .ptr = ptr, .width = width, .height = height };
    }
    pub fn fill(self: Canvas, color: u32) void {
        Simd.fill(self.ptr, color, self.width * self.height);
    }

    pub fn rect(self: Canvas, r: Rect, color: u32) void {
        fill_rect(self.ptr, self.width, self.height, r.x, r.y, r.w, r.h, color);
    }

    pub fn border(self: Canvas, r: Rect, color: u32, thickness: u32) void {
        draw_border(self.ptr, self.width, self.height, r, color, thickness);
    }

    pub fn line(self: Canvas, x0: i32, y0: i32, x1: i32, y1: i32, color: u32) void {
        draw_line(self.ptr, self.width, self.height, x0, y0, x1, y1, color);
    }

    pub fn gradient_h(self: Canvas, r: Rect, c1: u32, c2: u32) void {
        draw_gradient_rect(self.ptr, self.width, self.height, r.x, r.y, r.w, r.h, c1, c2);
    }

    pub fn gradient_v(self: Canvas, r: Rect, c1: u32, c2: u32) void {
        draw_gradient_rect_v(self.ptr, self.width, self.height, r.x, r.y, r.w, r.h, c1, c2);
    }

    pub fn circle(self: Canvas, cx: i32, cy: i32, radius: u32, color: u32) void {
        draw_circle(self.ptr, self.width, self.height, cx, cy, radius, color);
    }

    pub fn rounded_rect_alpha(self: Canvas, r: Rect, color: u32, radius: u32, alpha: u8) void {
        draw_rounded_rect_alpha(self.ptr, self.width, self.height, r.x, r.y, r.w, r.h, color, radius, alpha);
    }
};

pub const Draw = struct {
    pub fn fill_rect(canvas: Canvas, r: Rect, color: u32) void {
        canvas.rect(r, color);
    }

    pub fn border(canvas: Canvas, r: Rect, color: u32, thickness: u32) void {
        canvas.border(r, color, thickness);
    }
};

const Clip = struct { x0: u32, y0: u32, x1: u32, y1: u32 };

fn clip_rect(width: u32, height: u32, x: i32, y: i32, w: u32, h: u32) ?Clip {
    if (width == 0 or height == 0 or w == 0 or h == 0) return null;
    const x0 = @max(@as(i64, 0), @as(i64, x));
    const y0 = @max(@as(i64, 0), @as(i64, y));
    const x1 = @min(@as(i64, width), @as(i64, x) + @as(i64, w));
    const y1 = @min(@as(i64, height), @as(i64, y) + @as(i64, h));
    if (x1 <= x0 or y1 <= y0) return null;
    return .{ .x0 = @intCast(x0), .y0 = @intCast(y0), .x1 = @intCast(x1), .y1 = @intCast(y1) };
}

pub fn fill_rect(ptr: [*]u32, width: u32, height: u32, x: i32, y: i32, w: u32, h: u32, color: u32) void {
    const clip = clip_rect(width, height, x, y, w, h) orelse return;
    var yy = clip.y0;
    while (yy < clip.y1) : (yy += 1) {
        var xx = clip.x0;
        const row = @as(usize, yy) * @as(usize, width);
        while (xx < clip.x1) : (xx += 1) ptr[row + xx] = color;
    }
}

pub fn fill_rect_alpha(ptr: [*]u32, width: u32, height: u32, x: i32, y: i32, w: u32, h: u32, color: u32, alpha: u8) void {
    const clip = clip_rect(width, height, x, y, w, h) orelse return;
    var yy = clip.y0;
    while (yy < clip.y1) : (yy += 1) {
        const row = @as(usize, yy) * @as(usize, width);
        const row_end = clip.x1;
        var xx = clip.x0;
        // 4x unrolled blend
        while (xx + 4 <= row_end) : (xx += 4) {
            const i = row + xx;
            ptr[i] = blend(ptr[i], color, alpha);
            ptr[i + 1] = blend(ptr[i + 1], color, alpha);
            ptr[i + 2] = blend(ptr[i + 2], color, alpha);
            ptr[i + 3] = blend(ptr[i + 3], color, alpha);
        }
        while (xx < row_end) : (xx += 1) ptr[row + xx] = blend(ptr[row + xx], color, alpha);
    }
}

pub fn draw_border(ptr: [*]u32, width: u32, height: u32, r: Rect, color: u32, thickness: u32) void {
    var t: u32 = 0;
    while (t < thickness and t * 2 < r.w and t * 2 < r.h) : (t += 1) {
        const ti: i32 = @intCast(t);
        fill_rect(ptr, width, height, r.x + ti, r.y + ti, r.w - t * 2, 1, color);
        fill_rect(ptr, width, height, r.x + ti, r.y + @as(i32, @intCast(r.h - 1 - t)), r.w - t * 2, 1, color);
        fill_rect(ptr, width, height, r.x + ti, r.y + ti, 1, r.h - t * 2, color);
        fill_rect(ptr, width, height, r.x + @as(i32, @intCast(r.w - 1 - t)), r.y + ti, 1, r.h - t * 2, color);
    }
}

pub fn draw_line(ptr: [*]u32, width: u32, height: u32, x0_arg: i32, y0_arg: i32, x1: i32, y1: i32, color: u32) void {
    var x0 = x0_arg;
    var y0 = y0_arg;
    const dx = @abs(x1 - x0);
    const sx: i32 = if (x0 < x1) 1 else -1;
    const dy = -@as(i32, @intCast(@abs(y1 - y0)));
    const sy: i32 = if (y0 < y1) 1 else -1;
    var err = @as(i32, @intCast(dx)) + dy;

    while (true) {
        if (x0 >= 0 and y0 >= 0 and x0 < @as(i32, @intCast(width)) and y0 < @as(i32, @intCast(height))) {
            ptr[@as(usize, @intCast(y0)) * @as(usize, width) + @as(usize, @intCast(x0))] = color;
        }
        if (x0 == x1 and y0 == y1) break;
        const e2 = err * 2;
        if (e2 >= dy) {
            err += dy;
            x0 += sx;
        }
        if (e2 <= @as(i32, @intCast(dx))) {
            err += @as(i32, @intCast(dx));
            y0 += sy;
        }
    }
}

pub fn draw_rounded_rect(ptr: [*]u32, width: u32, height: u32, x: i32, y: i32, w: u32, h: u32, color: u32, radius: u32) void {
    if (w == 0 or h == 0) return;
    const r = @min(radius, @min(w, h) / 2);
    if (r == 0) return fill_rect(ptr, width, height, x, y, w, h, color);

    var row: u32 = 0;
    while (row < h) : (row += 1) {
        var inset: u32 = 0;
        if (row < r) {
            inset = rounded_inset(r, r - row - 1);
        } else if (row >= h - r) {
            inset = rounded_inset(r, row - (h - r));
        }
        if (inset < w / 2) {
            fill_rect(ptr, width, height, x + @as(i32, @intCast(inset)), y + @as(i32, @intCast(row)), w - inset * 2, 1, color);
        }
    }
}

fn rounded_inset(radius: u32, dy: u32) u32 {
    var inset: u32 = 0;
    while (inset < radius) : (inset += 1) {
        const dx = radius - inset;
        if (dx * dx + dy * dy <= radius * radius) return inset;
    }
    return radius;
}
fn lerp_color(c1: u32, c2: u32, t: u32) u32 {
    const r1 = (c1 >> 16) & 0xff;
    const g1 = (c1 >> 8) & 0xff;
    const b1 = c1 & 0xff;
    const r2 = (c2 >> 16) & 0xff;
    const g2 = (c2 >> 8) & 0xff;
    const b2 = c2 & 0xff;
    const inv = 255 - t;
    const r = (r1 * inv + r2 * t) / 255;
    const g = (g1 * inv + g2 * t) / 255;
    const b = (b1 * inv + b2 * t) / 255;
    return 0xff000000 | (r << 16) | (g << 8) | b;
}

pub fn draw_gradient_rect(ptr: [*]u32, width: u32, height: u32, x: i32, y: i32, w: u32, h: u32, color_left: u32, color_right: u32) void {
    const clip = clip_rect(width, height, x, y, w, h) orelse return;
    if (w <= 1) {
        var row = clip.y0;
        while (row < clip.y1) : (row += 1) {
            const base = @as(usize, row) * @as(usize, width);
            ptr[base + clip.x0] = color_left;
        }
        return;
    }
    const wm1 = w - 1;
    var col: u32 = clip.x0;
    while (col < clip.x1) : (col += 1) {
        const numer = @as(u32, @intCast(@as(i64, @intCast(col)) - @as(i64, @intCast(x))));
        const t = numer * 255 / wm1;
        const c = lerp_color(color_left, color_right, t);
        const base: usize = @intCast(col);
        var row: u32 = clip.y0;
        while (row < clip.y1) : (row += 1) {
            ptr[base + row * @as(usize, width)] = c;
        }
    }
}

pub fn draw_gradient_rect_v(ptr: [*]u32, width: u32, height: u32, x: i32, y: i32, w: u32, h: u32, color_top: u32, color_bottom: u32) void {
    const clip = clip_rect(width, height, x, y, w, h) orelse return;
    if (h <= 1) {
        var col = clip.x0;
        while (col < clip.x1) : (col += 1) {
            ptr[@as(usize, clip.y0) * @as(usize, width) + col] = color_top;
        }
        return;
    }
    const hm1 = h - 1;
    var row: u32 = clip.y0;
    while (row < clip.y1) : (row += 1) {
        const numer = @as(u32, @intCast(@as(i64, @intCast(row)) - @as(i64, @intCast(y))));
        const t = numer * 255 / hm1;
        const c = lerp_color(color_top, color_bottom, t);
        const base = @as(usize, row) * @as(usize, width);
        var col: u32 = clip.x0;
        while (col < clip.x1) : (col += 1) {
            ptr[base + col] = c;
        }
    }
}

pub fn draw_rounded_border(ptr: [*]u32, width: u32, height: u32, x: i32, y: i32, w: u32, h: u32, color: u32, radius_arg: u32, thickness: u32) void {
    if (w == 0 or h == 0 or thickness == 0) return;
    const r = @min(radius_arg, @min(w, h) / 2);
    const t = @min(thickness, @min(w, h) / 2);
    if (t == 0) return;
    const clip = clip_rect(width, height, x, y, w, h) orelse return;
    var row: u32 = clip.y0;
    while (row < clip.y1) : (row += 1) {
        const ry: u32 = @intCast(@as(i64, @intCast(row)) - @as(i64, @intCast(y)));
        const outer_inset = if (ry < r) rounded_inset(r, r - 1 - ry) else if (ry >= h - r) rounded_inset(r, ry - (h - r)) else 0;
        const inner_w = if (w > 2 * t) w - 2 * t else 0;
        const inner_h = if (h > 2 * t) h - 2 * t else 0;
        if (inner_w == 0 or inner_h == 0) {
            const left = @max(clip.x0, @as(u32, @intCast(x + @as(i32, @intCast(outer_inset)))));
            const right_excl = @min(clip.x1, @as(u32, @intCast(x + @as(i32, @intCast(w - outer_inset)))));
            if (left < right_excl) {
                const base = @as(usize, row) * @as(usize, width);
                var col: u32 = left;
                while (col < right_excl) : (col += 1) ptr[base + col] = color;
            }
            continue;
        }
        const ir = if (r > t) r - t else 0;
        const inner_inset = if (ir > 0) blk: {
            const iry: u32 = @intCast(ry - t);
            if (iry < ir) break :blk rounded_inset(ir, ir - 1 - iry);
            if (iry >= inner_h - ir) break :blk rounded_inset(ir, iry - (inner_h - ir));
            break :blk @as(u32, 0);
        } else @as(u32, 0);
        const outer_left_x = x + @as(i32, @intCast(outer_inset));
        const outer_right_x_excl = x + @as(i32, @intCast(w - outer_inset));
        const inner_left_x = x + @as(i32, @intCast(t + inner_inset));
        const inner_right_x = x + @as(i32, @intCast(w - t - inner_inset));
        const left_seg_start = @max(clip.x0, @as(u32, @intCast(outer_left_x)));
        const left_seg_end = @min(clip.x1, @as(u32, @intCast(inner_left_x)));
        const right_seg_start = @max(clip.x0, @as(u32, @intCast(inner_right_x)));
        const right_seg_end = @min(clip.x1, @as(u32, @intCast(outer_right_x_excl)));
        const base = @as(usize, row) * @as(usize, width);
        if (left_seg_start < left_seg_end) {
            var col: u32 = left_seg_start;
            while (col < left_seg_end) : (col += 1) ptr[base + col] = color;
        }
        if (right_seg_start < right_seg_end) {
            var col: u32 = right_seg_start;
            while (col < right_seg_end) : (col += 1) ptr[base + col] = color;
        }
    }
}

pub fn draw_circle(ptr: [*]u32, width: u32, height: u32, cx: i32, cy: i32, radius: u32, color: u32) void {
    if (radius == 0) {
        if (cx >= 0 and cy >= 0 and cx < @as(i32, @intCast(width)) and cy < @as(i32, @intCast(height))) {
            ptr[@as(usize, @intCast(cy)) * @as(usize, width) + @as(usize, @intCast(cx))] = color;
        }
        return;
    }
    const r_sq: i64 = @as(i64, @intCast(radius)) * @as(i64, @intCast(radius));
    const cx_i64: i64 = @intCast(cx);
    const cy_i64: i64 = @intCast(cy);
    const bb_x0 = cx - @as(i32, @intCast(radius));
    const bb_y0 = cy - @as(i32, @intCast(radius));
    const bb_w = radius * 2 + 1;
    const bb_h = radius * 2 + 1;
    const clip = clip_rect(width, height, bb_x0, bb_y0, bb_w, bb_h) orelse return;
    var row: u32 = clip.y0;
    while (row < clip.y1) : (row += 1) {
        const dy: i64 = @as(i64, @intCast(row)) - cy_i64;
        const dy_sq = dy * dy;
        if (dy_sq > r_sq) continue;
        const rem = r_sq - dy_sq;
        var dx: i32 = @intCast(@sqrt(@as(f64, rem)));
        while ((@as(i64, @intCast(dx)) + 1) * (@as(i64, @intCast(dx)) + 1) <= rem) : (dx += 1) {}
        while (@as(i64, @intCast(dx)) * @as(i64, @intCast(dx)) > rem) : (dx -= 1) {}

        const left = cx_i64 - @as(i64, @intCast(dx));
        const right = cx_i64 + @as(i64, @intCast(dx));
        const base = @as(usize, row) * @as(usize, width);
        const col_start = @max(clip.x0, @as(u32, @intCast(@max(@as(i64, 0), left))));
        const col_end_excl = @min(clip.x1, @as(u32, @intCast(@min(@as(i64, width), right + 1))));
        var col: u32 = col_start;
        while (col < col_end_excl) : (col += 1) {
            ptr[base + col] = color;
        }
    }
}

pub fn draw_rounded_rect_alpha(ptr: [*]u32, width: u32, height: u32, x: i32, y: i32, w: u32, h: u32, color: u32, radius: u32, alpha: u8) void {
    const clip = clip_rect(width, height, x, y, w, h) orelse return;
    var row: u32 = clip.y0;
    while (row < clip.y1) : (row += 1) {
        var inset: u32 = 0;
        const ry: u32 = @intCast(@as(i64, @intCast(row)) - @as(i64, @intCast(y)));
        if (ry < radius) {
            inset = rounded_inset(radius, radius - 1 - ry);
        } else if (ry >= h - radius) {
            inset = rounded_inset(radius, ry - (h - radius));
        }
        if (inset < w / 2) {
            const seg_x0 = @max(clip.x0, @as(u32, @intCast(x + @as(i32, @intCast(inset)))));
            const seg_x1_excl = @min(clip.x1, @as(u32, @intCast(x + @as(i32, @intCast(w - inset)))));
            const base = @as(usize, row) * @as(usize, width);
            var col: u32 = seg_x0;
            while (col < seg_x1_excl) : (col += 1) {
                ptr[base + col] = blend(ptr[base + col], color, alpha);
            }
        }
    }
}

fn blend(dst: u32, src: u32, alpha: u8) u32 {
    const a = @as(u32, alpha);
    const inv = 255 - a;
    const sr = (src >> 16) & 0xff;
    const sg = (src >> 8) & 0xff;
    const sb = src & 0xff;
    const dr = (dst >> 16) & 0xff;
    const dg = (dst >> 8) & 0xff;
    const db = dst & 0xff;
    return 0xff000000 | (((sr * a + dr * inv) / 255) << 16) | (((sg * a + dg * inv) / 255) << 8) | ((sb * a + db * inv) / 255);
}

test "rect clipping handles negative origin" {
    var pixels = [_]u32{0} ** 9;
    fill_rect(&pixels, 3, 3, -1, -1, 2, 2, 7);
    try @import("std").testing.expectEqual(@as(u32, 7), pixels[0]);
    try @import("std").testing.expectEqual(@as(u32, 0), pixels[1]);
}

test "alpha clipping handles negative origin" {
    var pixels = [_]u32{0xff000000} ** 9;
    fill_rect_alpha(&pixels, 3, 3, -1, -1, 2, 2, 0xffffffff, 255);
    try @import("std").testing.expectEqual(@as(u32, 0xffffffff), pixels[0]);
    try @import("std").testing.expectEqual(@as(u32, 0xff000000), pixels[1]);
}

test "line diagonal sets three pixels" {
    var pixels = [_]u32{0} ** 9;
    Canvas.init(&pixels, 3, 3).line(0, 0, 2, 2, 7);
    try @import("std").testing.expectEqual(@as(u32, 7), pixels[0]);
    try @import("std").testing.expectEqual(@as(u32, 7), pixels[4]);
    try @import("std").testing.expectEqual(@as(u32, 7), pixels[8]);
}

test "rounded radius zero equals filled rect" {
    var a = [_]u32{0} ** 9;
    var b = [_]u32{0} ** 9;
    fill_rect(&a, 3, 3, 0, 0, 3, 3, 7);
    draw_rounded_rect(&b, 3, 3, 0, 0, 3, 3, 7, 0);
    try @import("std").testing.expectEqualSlices(u32, &a, &b);
}
test "alpha blend with scalar tail" {
    var pixels = [_]u32{0xff000000} ** 9;
    fill_rect_alpha(&pixels, 9, 1, 0, 0, 7, 1, 0xffffffff, 128);
    try @import("std").testing.expectEqual(@as(u32, 0xff808080), pixels[0]);
    try @import("std").testing.expectEqual(@as(u32, 0xff000000), pixels[7]);
}
test "horizontal gradient interpolates" {
    var pixels = [_]u32{0} ** 4;
    draw_gradient_rect(&pixels, 2, 2, 0, 0, 2, 2, 0xff00ff00, 0xffffffff);
    try @import("std").testing.expectEqual(@as(u32, 0xff00ff00), pixels[0]);
    try @import("std").testing.expectEqual(@as(u32, 0xffffffff), pixels[1]);
    try @import("std").testing.expectEqual(@as(u32, 0xff00ff00), pixels[2]);
    try @import("std").testing.expectEqual(@as(u32, 0xffffffff), pixels[3]);
}

test "vertical gradient interpolates" {
    var pixels = [_]u32{0} ** 4;
    draw_gradient_rect_v(&pixels, 2, 2, 0, 0, 2, 2, 0xff00ff00, 0xffffffff);
    try @import("std").testing.expectEqual(@as(u32, 0xff00ff00), pixels[0]);
    try @import("std").testing.expectEqual(@as(u32, 0xffffffff), pixels[2]);
    try @import("std").testing.expectEqual(@as(u32, 0xffffffff), pixels[1]);
    try @import("std").testing.expectEqual(@as(u32, 0xffffffff), pixels[3]);
}

test "rounded border leaves interior clear" {
    var pixels = [_]u32{0} ** 25;
    draw_rounded_border(&pixels, 5, 5, 0, 0, 5, 5, 0xffffffff, 2, 1);
    try @import("std").testing.expectEqual(@as(u32, 0), pixels[6]);
    try @import("std").testing.expectEqual(@as(u32, 0xffffffff), pixels[0]);
    try @import("std").testing.expectEqual(@as(u32, 0xffffffff), pixels[4]);
    try @import("std").testing.expectEqual(@as(u32, 0xffffffff), pixels[20]);
    try @import("std").testing.expectEqual(@as(u32, 0xffffffff), pixels[24]);
}

test "circle fills inside radius" {
    var pixels = [_]u32{0} ** 25;
    draw_circle(&pixels, 5, 5, 2, 2, 2, 0xffffffff);
    try @import("std").testing.expectEqual(@as(u32, 0xffffffff), pixels[2 * 5 + 2]);
    try @import("std").testing.expectEqual(@as(u32, 0), pixels[0]);
    try @import("std").testing.expectEqual(@as(u32, 0xffffffff), pixels[1 * 5 + 1]);
}

test "rounded rect alpha blends" {
    var pixels = [_]u32{0xff000000} ** 9;
    draw_rounded_rect_alpha(&pixels, 3, 3, 0, 0, 3, 3, 0xffffffff, 1, 128);
    try @import("std").testing.expectEqual(@as(u32, 0xff808080), pixels[4]);
}
