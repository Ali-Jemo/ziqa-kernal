const std = @import("std");
const youcanvas = @import("youcanvas");
const Rect = youcanvas.Rect;
const UI = @import("root.zig").UI;
const UIState = @import("root.zig").UIState;
const Canvas = youcanvas.Canvas;

pub fn label(ui: *UI, rect: Rect, text: []const u8, color: u32) void {
    _ = rect.w;
    _ = rect.h;
    youcanvas.draw_text(ui.canvas, rect.x, rect.y, text, color);
}

pub fn label_next(ui: *UI, w: u32, h: u32, text: []const u8, color: u32) void {
    label(ui, ui.next(w, h), text, color);
}

pub fn button(ui: *UI, id: u64, rect: Rect, bg: u32, active_bg: u32) bool {
    if (rect.w == 0 or rect.h == 0) return false;
    const state = ui.interact(id, rect);
    const clicked = state.clicked or (ui.is_focused(id) and ui.input.activate);
    ui.canvas.rect(rect, if (state.pressed) active_bg else bg);
    ui.canvas.border(rect, if (ui.is_focused(id)) ui.theme.accent else ui.theme.border, 1);
    if (clicked and ui.state.active == id) ui.state.active = 0;
    return clicked;
}

pub fn button_next(ui: *UI, id: u64, w: u32, h: u32, bg: u32, active_bg: u32) bool {
    return button(ui, id, ui.next(w, h), bg, active_bg);
}

pub fn checkbox(ui: *UI, id: u64, rect: Rect, checked: *bool) bool {
    if (rect.w == 0 or rect.h == 0) return false;
    const state = ui.interact(id, rect);
    ui.canvas.rect(rect, if (state.pressed) ui.theme.accent else ui.theme.border);
    if (checked.*) {
        ui.canvas.rect(Rect.xywh(rect.x + 2, rect.y + 2, rect.w -| 4, rect.h -| 4), ui.theme.accent);
    }
    const toggled = state.clicked or (ui.is_focused(id) and ui.input.activate);
    if (toggled) {
        checked.* = !checked.*;
    }
    return toggled;
}

pub fn checkbox_next(ui: *UI, id: u64, w: u32, h: u32, checked: *bool) bool {
    return checkbox(ui, id, ui.next(w, h), checked);
}

pub fn slider_u32(ui: *UI, id: u64, rect: Rect, value: *u32, min: u32, max: u32) bool {
    if (rect.w == 0 or rect.h == 0 or max <= min) {
        ui.canvas.rect(rect, ui.theme.border);
        return false;
    }
    const state = ui.interact(id, rect);
    ui.canvas.rect(rect, ui.theme.border);
    var pos = value.*;
    if (state.pressed) {
        const numerator = if (ui.input.mouse_x > rect.x) @as(u32, @intCast(ui.input.mouse_x - rect.x)) else 0;
        pos = if (numerator >= rect.w) max else min + numerator * (max - min) / rect.w;
    }
    value.* = @min(pos, max);
    const handle_w = 6;
    const handle_x = rect.x + @as(i32, @intCast((value.* - min) * (rect.w - handle_w) / (max - min)));
    ui.canvas.rect(Rect.xywh(handle_x, rect.y - 1, handle_w, rect.h +| 2), ui.theme.accent);
    return false;
}

pub fn slider_u32_next(ui: *UI, id: u64, w: u32, h: u32, value: *u32, min: u32, max: u32) bool {
    return slider_u32(ui, id, ui.next(w, h), value, min, max);
}

pub fn panel_button(ui: *UI, id: u64, rect: Rect, text: []const u8, bg: u32, active_bg: u32) bool {
    if (rect.w == 0 or rect.h == 0) return false;
    const state = ui.interact(id, rect);
    const clicked = state.clicked or (ui.is_focused(id) and ui.input.activate);
    ui.canvas.rect(rect, if (state.pressed) active_bg else bg);
    ui.canvas.border(rect, if (ui.is_focused(id)) ui.theme.accent else ui.theme.border, 1);
    const tw = youcanvas.text_width(text);
    const text_x = rect.x + @as(i32, @intCast((@max(rect.w, tw) - tw) / 2));
    const text_y = rect.y + @as(i32, @intCast((@max(rect.h, @as(u32, 8)) - 8) / 2));
    youcanvas.draw_text(ui.canvas, text_x, text_y, text, ui.theme.text);
    if (clicked and ui.state.active == id) ui.state.active = 0;
    return clicked;
}

pub fn panel_button_next(ui: *UI, id: u64, w: u32, h: u32, text: []const u8, bg: u32, active_bg: u32) bool {
    return panel_button(ui, id, ui.next(w, h), text, bg, active_bg);
}

pub fn panel(ui: *UI, rect: Rect, color: u32) void {
    ui.canvas.rect(rect, color);
}

test "tab focus picks first button" {
    var pixels = [_]u32{0} ** 64;
    var state = UIState{};
    var ui = UI.begin(Canvas.init(&pixels, 8, 8), .{ .focus_next = true }, &state);
    _ = ui.focusable(1);
    _ = ui.focusable(2);
    ui.finish_focus();
    try std.testing.expect(ui.is_focused(1));
}

test "activate clicks focused button" {
    var pixels = [_]u32{0} ** 64;
    var state = UIState{};
    var ui = UI.begin(Canvas.init(&pixels, 8, 8), .{}, &state);
    _ = button(&ui, 2, Rect.xywh(0, 0, 8, 8), 1, 2);
    try std.testing.expect(!ui.is_focused(2));
    state.focused = 2;
    ui = UI.begin(Canvas.init(&pixels, 8, 8), .{ .activate = true }, &state);
    try std.testing.expect(button(&ui, 2, Rect.xywh(0, 0, 8, 8), 1, 2));
}

test "checkbox toggles once" {
    var pixels = [_]u32{0} ** 64;
    var state = UIState{};
    var ui = UI.begin(Canvas.init(&pixels, 8, 8), .{ .activate = true }, &state);
    _ = ui.focusable(3);
    var checked = false;
    try std.testing.expect(checkbox(&ui, 3, Rect.xywh(0, 0, 8, 8), &checked));
    try std.testing.expect(checked);

    ui = UI.begin(Canvas.init(&pixels, 8, 8), .{}, &state);
    try std.testing.expect(!checkbox(&ui, 3, Rect.xywh(0, 0, 8, 8), &checked));
}

test "slider clamps to max when mouse past right edge" {
    var pixels = [_]u32{0} ** 64;
    var state = UIState{};
    var ui = UI.begin(Canvas.init(&pixels, 8, 8), .{ .mouse_x = 100, .mouse_y = 4, .mouse_left = true }, &state);
    state.active = 4;
    var value: u32 = 0;
    _ = slider_u32(&ui, 4, Rect.xywh(0, 0, 8, 8), &value, 0, 100);
    try std.testing.expectEqual(@as(u32, 100), value);
}
test "panel_button returns true on activate" {
    var pixels = [_]u32{0} ** 64;
    var state = UIState{};
    var ui = UI.begin(Canvas.init(&pixels, 8, 8), .{ .activate = true }, &state);
    state.focused = 10;
    try std.testing.expect(panel_button(&ui, 10, Rect.xywh(0, 0, 8, 8), "OK", 1, 2));
}
