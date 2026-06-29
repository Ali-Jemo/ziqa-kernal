pub const Rect = struct {
    x: i32,
    y: i32,
    w: u32,
    h: u32,

    pub fn xywh(x: i32, y: i32, w: u32, h: u32) Rect {
        return .{ .x = x, .y = y, .w = w, .h = h };
    }

    pub fn contains(self: Rect, px: i32, py: i32) bool {
        return px >= self.x and py >= self.y and px < self.x + @as(i32, @intCast(self.w)) and py < self.y + @as(i32, @intCast(self.h));
    }

    pub fn top_left(self: Rect) struct { x: i32, y: i32 } {
        return .{ .x = self.x, .y = self.y };
    }
};

pub const Damage = union(enum) {
    none,
    rect: Rect,
    full,
};
