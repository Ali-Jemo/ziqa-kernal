const std = @import("std");
const Rect = @import("rect.zig").Rect;
const Canvas = @import("draw.zig").Canvas;

pub const RectPaint = struct { rect: Rect, color: u32 };
pub const BorderPaint = struct { rect: Rect, color: u32, thickness: u32 };
pub const TextPaint = struct { x: i32, y: i32, text: []const u8, color: u32 };

pub const AlphaRectPaint = struct { rect: Rect, color: u32, alpha: u8 };

pub const DrawCommand = union(enum) {
    clear: u32,
    rect: RectPaint,
    border: BorderPaint,
    text: TextPaint,
    alpha_rect: AlphaRectPaint,
};

pub const DrawList = struct {
    commands: []DrawCommand,
    len: usize = 0,

    pub fn init(commands: []DrawCommand) DrawList {
        return .{ .commands = commands };
    }

    pub fn reset(self: *DrawList) void {
        self.len = 0;
    }

    pub fn push(self: *DrawList, cmd: DrawCommand) bool {
        if (self.len >= self.commands.len) return false;
        self.commands[self.len] = cmd;
        self.len += 1;
        return true;
    }

    pub fn render(self: DrawList, canvas: Canvas) void {
        for (self.commands[0..self.len]) |cmd| {
            switch (cmd) {
                .clear => |color| canvas.fill(color),
                .rect => |paint| if (paint.rect.w != 0 and paint.rect.h != 0) canvas.rect(paint.rect, paint.color),
                .border => |paint| if (paint.rect.w != 0 and paint.rect.h != 0 and paint.thickness != 0) canvas.border(paint.rect, paint.color, paint.thickness),
                .text => |paint| if (paint.text.len != 0) @import("font.zig").draw_text(canvas, paint.x, paint.y, paint.text, paint.color),
                .alpha_rect => |paint| if (paint.rect.w != 0 and paint.rect.h != 0)
                    @import("draw.zig").fill_rect_alpha(canvas.ptr, canvas.width, canvas.height,
                        paint.rect.x, paint.rect.y, paint.rect.w, paint.rect.h, paint.color, paint.alpha),
            }
        }
    }
};

test "draw list renders in order" {
    var commands: [4]DrawCommand = undefined;
    var list = DrawList.init(&commands);
    try std.testing.expect(list.push(.{ .clear = 0 }));
    try std.testing.expect(list.push(.{ .rect = .{ .rect = Rect.xywh(1, 1, 1, 1), .color = 7 } }));

    var pixels = [_]u32{9} ** 9;
    list.render(Canvas.init(&pixels, 3, 3));

    try std.testing.expectEqual(@as(u32, 0), pixels[0]);
    try std.testing.expectEqual(@as(u32, 7), pixels[4]);
}
test "draw list renders alpha_rect" {
    var commands: [2]DrawCommand = undefined;
    var list = DrawList.init(&commands);
    _ = list.push(.{ .alpha_rect = .{ .rect = Rect.xywh(0, 0, 2, 2), .color = 0xffffffff, .alpha = 128 } });

    var pixels = [_]u32{0xff000000} ** 4;
    list.render(Canvas.init(&pixels, 2, 2));
    try std.testing.expectEqual(@as(u32, 0xff808080), pixels[0]);
}
