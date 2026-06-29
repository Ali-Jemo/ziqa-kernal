const youcanvas = @import("youcanvas");
const Rect = youcanvas.Rect;
const Canvas = youcanvas.Canvas;
const Theme = youcanvas.Theme;

pub const WidgetId = packed struct(u64) { value: u64 };

pub const InputState = struct {
    mouse_x: i32 = 0,
    mouse_y: i32 = 0,
    mouse_left: bool = false,
    left_pressed: bool = false,
    left_released: bool = false,
    last_char: u8 = 0,
    tick: u64 = 0,
    dt: f32 = 0.016,
    key: u32 = 0,
    key_pressed: bool = false,
    focus_next: bool = false,
    focus_prev: bool = false,
    activate: bool = false,
};

pub const UIState = struct {
    hot: u64 = 0,
    active: u64 = 0,
    focused: u64 = 0,
};

pub const Layout = enum { Row, Column, Absolute };

const Point = struct { x: i32, y: i32 };

const LayoutEntry = struct {
    rect: Rect,
    layout: Layout,
    cursor: Point,
    gap: u32 = 0,
};

pub const UI = struct {
    canvas: Canvas,
    theme: Theme,
    input: InputState,
    state: *UIState,
    layout_stack: [16]LayoutEntry = undefined,
    layout_depth: u8 = 0,
    first_focus: u64 = 0,
    last_focus: u64 = 0,
    seen_focused: bool = false,
    focus_moved: bool = false,

    pub fn begin(canvas: Canvas, input: InputState, state: *UIState) UI {
        // ponytail: copy input by value; persistent state is via the UIState pointer.
        return .{ .canvas = canvas, .theme = Theme.default(), .input = input, .state = state };
    }

    pub fn end(self: *UI) void {
        _ = self;
    }

    pub fn interact(self: *UI, id: u64, rect: Rect) struct { hover: bool, pressed: bool, clicked: bool } {
        if (rect.w == 0 or rect.h == 0) {
            return .{ .hover = false, .pressed = false, .clicked = false };
        }
        const mx: i32 = self.input.mouse_x;
        const my: i32 = self.input.mouse_y;
        const hover: bool = rect.contains(mx, my);
        const pressed_edge: bool = self.input.left_pressed;
        const released_edge: bool = self.input.left_released;

        if (hover and pressed_edge) self.state.active = id;
        const pressed: bool = self.state.active == id and self.input.mouse_left;
        const clicked: bool = self.state.active == id and released_edge;
        if (clicked) self.state.active = 0;
        self.state.hot = if (hover) id else 0;
        return .{ .hover = hover, .pressed = pressed, .clicked = clicked };
    }

    pub fn focusable(self: *UI, id: u64) bool {
        if (id == 0) return false;
        const previous: u64 = self.state.focused;
        if (self.first_focus == 0) self.first_focus = id;
        if (self.state.focused == 0) self.state.focused = id;
        if (id == previous) self.seen_focused = true;
        if (id == previous and self.input.focus_prev and !self.focus_moved and self.last_focus != 0) {
            self.state.focused = self.last_focus;
            self.focus_moved = true;
        } else if (self.input.focus_next and self.seen_focused and !self.focus_moved and id != previous) {
            self.state.focused = id;
            self.focus_moved = true;
        }
        self.last_focus = id;
        return self.state.focused == id;
    }

    pub fn is_focused(self: *const UI, id: u64) bool {
        return self.state.focused == id;
    }

    pub fn finish_focus(self: *UI) void {
        if (self.seen_focused and !self.focus_moved) {
            if (self.input.focus_next and self.first_focus != 0) {
                self.state.focused = self.first_focus;
                self.focus_moved = true;
            } else if (self.input.focus_prev and self.last_focus != 0) {
                self.state.focused = self.last_focus;
                self.focus_moved = true;
            }
        }
    }

    pub fn push_panel(self: *UI, rect: Rect, layout: Layout) void {
        self.push_panel_gap(rect, layout, 0);
    }

    pub fn push_panel_gap(self: *UI, rect: Rect, layout: Layout, gap: u32) void {
        if (self.layout_depth >= self.layout_stack.len) return;
        self.layout_stack[self.layout_depth] = .{ .rect = rect, .layout = layout, .cursor = .{ .x = rect.x, .y = rect.y }, .gap = gap };
        self.layout_depth += 1;
    }

    pub fn next(self: *UI, w: u32, h: u32) Rect {
        if (self.layout_depth == 0) return Rect.xywh(0, 0, w, h);
        var entry = &self.layout_stack[self.layout_depth - 1];
        const rect = Rect.xywh(entry.cursor.x, entry.cursor.y, w, h);
        switch (entry.layout) {
            .Row => entry.cursor.x += @intCast(w + entry.gap),
            .Column => entry.cursor.y += @intCast(h + entry.gap),
            .Absolute => {},
        }
        return rect;
    }

    pub fn panel(self: *UI, rect: Rect, layout: Layout, color: u32) void {
        self.canvas.rect(rect, color);
        self.push_panel(rect, layout);
    }

    pub fn pop_panel(self: *UI) void {
        if (self.layout_depth > 0) self.layout_depth -= 1;
    }
};

test "column layout advances downward" {
    var pixels = [_]u32{0} ** 16;
    var state = UIState{};
    var ui = UI.begin(Canvas.init(&pixels, 4, 4), .{}, &state);
    ui.push_panel(Rect.xywh(1, 1, 2, 2), .Column);
    try @import("std").testing.expectEqual(Rect.xywh(1, 1, 2, 1), ui.next(2, 1));
    try @import("std").testing.expectEqual(Rect.xywh(1, 2, 2, 1), ui.next(2, 1));
}

test "tab focus picks first and advances" {
    var pixels = [_]u32{0} ** 16;
    var state = UIState{};
    var ui = UI.begin(Canvas.init(&pixels, 4, 4), .{}, &state);
    try @import("std").testing.expect(ui.focusable(1));
    _ = ui.focusable(2);
    try @import("std").testing.expectEqual(@as(u64, 1), state.focused);

    ui = UI.begin(Canvas.init(&pixels, 4, 4), .{ .focus_next = true }, &state);
    _ = ui.focusable(1);
    try @import("std").testing.expect(ui.focusable(2));
    ui.finish_focus();
    try @import("std").testing.expectEqual(@as(u64, 2), state.focused);
}
test "column layout with gap advances by h+gap" {
    var pixels = [_]u32{0} ** 16;
    var state = UIState{};
    var ui = UI.begin(Canvas.init(&pixels, 4, 4), .{}, &state);
    ui.push_panel_gap(Rect.xywh(0, 0, 4, 4), .Column, 2);
    try @import("std").testing.expectEqual(Rect.xywh(0, 0, 4, 1), ui.next(4, 1));
    try @import("std").testing.expectEqual(Rect.xywh(0, 3, 4, 1), ui.next(4, 1));
}

test "shift tab focus moves backward and wraps" {
    var pixels = [_]u32{0} ** 16;
    var state = UIState{ .focused = 2 };
    var ui = UI.begin(Canvas.init(&pixels, 4, 4), .{ .focus_prev = true }, &state);
    _ = ui.focusable(1);
    _ = ui.focusable(2);
    _ = ui.focusable(3);
    ui.finish_focus();
    try @import("std").testing.expectEqual(@as(u64, 1), state.focused);

    ui = UI.begin(Canvas.init(&pixels, 4, 4), .{ .focus_prev = true }, &state);
    _ = ui.focusable(1);
    _ = ui.focusable(2);
    _ = ui.focusable(3);
    ui.finish_focus();
    try @import("std").testing.expectEqual(@as(u64, 3), state.focused);
}
