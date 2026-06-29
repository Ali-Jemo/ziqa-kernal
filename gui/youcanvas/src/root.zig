pub const Color = @import("color.zig").Color;
pub const Rect = @import("rect.zig").Rect;
pub const Damage = @import("rect.zig").Damage;
pub const Canvas = @import("draw.zig").Canvas;
pub const Draw = @import("draw.zig").Draw;
pub const DrawCommand = @import("draw_list.zig").DrawCommand;
pub const DrawList = @import("draw_list.zig").DrawList;
pub const RectPaint = @import("draw_list.zig").RectPaint;
pub const BorderPaint = @import("draw_list.zig").BorderPaint;
pub const TextPaint = @import("draw_list.zig").TextPaint;
pub const Font = @import("font.zig").Font;
pub const glyph_bits = @import("font.zig").glyph_bits;
pub const text_width = @import("font.zig").text_width;
pub const draw_text = @import("font.zig").draw_text;

pub const glyph_bits_large = @import("font.zig").glyph_bits_large;
pub const draw_text_large = @import("font.zig").draw_text_large;
pub const text_width_large = @import("font.zig").text_width_large;
pub const Simd = @import("simd.zig").Simd;
pub const Theme = @import("color.zig").Theme;
pub const Sumerian = @import("color.zig").Sumerian;

test "basic types" {
    try @import("std").testing.expectEqual(Color.black.to_u32(), 0xFF000000);
    try @import("std").testing.expectEqual(Color.white.to_xrgb(), 0xFFFFFFFF);
}
