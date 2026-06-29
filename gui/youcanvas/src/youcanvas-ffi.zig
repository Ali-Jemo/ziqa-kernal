const draw = @import("draw.zig");
const Rect = @import("rect.zig").Rect;

export fn yc_fill_rect(ptr: [*]u32, width: u32, height: u32, x: i32, y: i32, w: u32, h: u32, color: u32) void {
    draw.fill_rect(ptr, width, height, x, y, w, h, color);
}

export fn yc_fill_rect_alpha(ptr: [*]u32, width: u32, height: u32, x: i32, y: i32, w: u32, h: u32, color: u32, alpha: u8) void {
    draw.fill_rect_alpha(ptr, width, height, x, y, w, h, color, alpha);
}

export fn yc_draw_border(ptr: [*]u32, width: u32, height: u32, x: i32, y: i32, w: u32, h: u32, color: u32, thickness: u32) void {
    draw.draw_border(ptr, width, height, Rect.xywh(x, y, w, h), color, thickness);
}

export fn yc_draw_rounded_rect(ptr: [*]u32, width: u32, height: u32, x: i32, y: i32, w: u32, h: u32, color: u32, radius: u32) void {
    draw.draw_rounded_rect(ptr, width, height, x, y, w, h, color, radius);
}

export fn yc_draw_line(ptr: [*]u32, width: u32, height: u32, x0: i32, y0: i32, x1: i32, y1: i32, color: u32) void {
    draw.draw_line(ptr, width, height, x0, y0, x1, y1, color);
}

export fn yc_clear(ptr: [*]u32, width: u32, height: u32, color: u32) void {
    var i: usize = 0;
    const n = @as(usize, width) * @as(usize, height);
    while (i < n) : (i += 1) ptr[i] = color;
}
const font = @import("font.zig");
const Canvas = @import("draw.zig").Canvas;

export fn yc_draw_text(ptr: [*]u32, width: u32, height: u32, x: i32, y: i32, text: [*]const u8, text_len: u32, color: u32) void {
    font.draw_text(Canvas.init(ptr, width, height), x, y, text[0..text_len], color);
}

export fn yc_text_width(text: [*]const u8, text_len: u32) i32 {
    return @as(i32, @intCast(font.text_width(text[0..text_len])));
}

export fn yc_draw_text_large(ptr: [*]u32, width: u32, height: u32, x: i32, y: i32, text: [*]const u8, text_len: u32, color: u32) void {
    font.draw_text_large(Canvas.init(ptr, width, height), x, y, text[0..text_len], color);
}

export fn yc_text_width_large(text: [*]const u8, text_len: u32) i32 {
    return @as(i32, @intCast(font.text_width_large(text[0..text_len])));
}
export fn yc_draw_gradient_rect(ptr: [*]u32, width: u32, height: u32, x: i32, y: i32, w: u32, h: u32, color_left: u32, color_right: u32) void {
    draw.draw_gradient_rect(ptr, width, height, x, y, w, h, color_left, color_right);
}

export fn yc_draw_gradient_rect_v(ptr: [*]u32, width: u32, height: u32, x: i32, y: i32, w: u32, h: u32, color_top: u32, color_bottom: u32) void {
    draw.draw_gradient_rect_v(ptr, width, height, x, y, w, h, color_top, color_bottom);
}

export fn yc_draw_circle(ptr: [*]u32, width: u32, height: u32, cx: i32, cy: i32, radius: u32, color: u32) void {
    draw.draw_circle(ptr, width, height, cx, cy, radius, color);
}

export fn yc_draw_rounded_rect_alpha(ptr: [*]u32, width: u32, height: u32, x: i32, y: i32, w: u32, h: u32, color: u32, radius: u32, alpha: u8) void {
    draw.draw_rounded_rect_alpha(ptr, width, height, x, y, w, h, color, radius, alpha);
}
