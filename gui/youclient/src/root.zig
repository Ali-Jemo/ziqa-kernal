const std = @import("std");
const youcanvas = @import("youcanvas");
const youui = @import("youui");
const protocol = @import("protocol.zig");
const syscall = @import("syscall.zig");

const Rect = youcanvas.Rect;
const Canvas = youcanvas.Canvas;
const InputState = youui.InputState;
const UI = youui.UI;

fn send_msg(op: protocol.OpCode, payload: []const u8) bool {
    var buf: [256]u8 = undefined;
    buf[0] = @intFromEnum(op);
    const len = @min(payload.len, 255);
    @memcpy(buf[1..1+len], payload[0..len]);
    return syscall.syscall4(protocol.ZIQA_IPC_SEND, protocol.COMPOSITOR_CHAN, @intFromPtr(&buf), len + 1, 0) == 0;
}

pub const Client = struct {
    canvas: Canvas,
    input: InputState = .{},
    ui_state: youui.UIState = .{},
    width: u32,
    height: u32,
    surface_id: u32,
    shm_id: u32,
    event_channel: u32,
    framebuffer: [*]u32,

    pub fn connect(width: u32, height: u32, x: i32, y: i32) ?Client {
        const shm_size = width * height * 4;
        const shm_id = syscall.syscall4(protocol.ZIQA_SHM_CREATE, shm_size, 0, 0, 0);
        if (shm_id == 0 or shm_id == std.math.maxInt(u64)) return null;

        const shm_addr = syscall.syscall4(protocol.ZIQA_SHM_ATTACH, shm_id, 0, 0, 0);
        if (shm_addr == 0 or shm_addr == std.math.maxInt(u64)) return null;

        if (!send_msg(.Connect, &[_]u8{})) return null;

        var surf = protocol.CreateSurfaceMsg{ .width = width, .height = height };
        if (!send_msg(.CreateSurface, std.mem.asBytes(&surf))) return null;

        var buf = protocol.BufferAttachMsg{ .surface_id = 1, .shm_id = @intCast(shm_id), .width = width, .height = height };
        if (!send_msg(.BufferAttach, std.mem.asBytes(&buf))) return null;

        const event_chan = syscall.syscall4(protocol.ZIQA_IPC_CREATE, 0, 0, 0, 0);
        if (event_chan == 0 or event_chan == std.math.maxInt(u64)) return null;
        var reg = protocol.RegisterEventChannelMsg{ .surface_id = 1, .event_channel_id = @intCast(event_chan) };
        if (!send_msg(.RegisterEventChannel, std.mem.asBytes(&reg))) return null;

        var pos = protocol.SetPositionMsg{ .surface_id = 1, .x = x, .y = y };
        if (!send_msg(.SetPosition, std.mem.asBytes(&pos))) return null;

        return Client{
            .canvas = Canvas.init(@as([*]u32, @ptrFromInt(shm_addr)), width, height),
            .width = width,
            .height = height,
            .surface_id = 1,
            .shm_id = @intCast(shm_id),
            .event_channel = @intCast(event_chan),
            .framebuffer = @as([*]u32, @ptrFromInt(shm_addr)),
        };
    }

    pub fn poll(self: *Client) void {
        var ev_buf: [256]u8 = undefined;
        self.input.left_pressed = false;
        self.input.left_released = false;
        self.input.key_pressed = false;
        self.input.activate = false;
        while (true) {
            const ret = syscall.syscall4(protocol.ZIQA_IPC_RECV, self.event_channel, @intFromPtr(&ev_buf), ev_buf.len, 0);
            if (ret == 0 or ret > ev_buf.len) break;
            if (ret < 2) continue;
            switch (ev_buf[1]) {
                1 => {
                    if (ret >= 1 + @sizeOf(protocol.InputMsg)) {
                        var input: protocol.InputMsg = undefined;
                        @memcpy(std.mem.asBytes(&input), ev_buf[1..1+@sizeOf(protocol.InputMsg)]);
                        self.input.key = input.code;
                        self.input.key_pressed = true;
                        self.input.activate = input.code == '\r' or input.code == ' ';
                    }
                },
                2 => {
                    if (ret >= 1 + @sizeOf(protocol.InputMsg)) {
                        var input: protocol.InputMsg = undefined;
                        @memcpy(std.mem.asBytes(&input), ev_buf[1..1+@sizeOf(protocol.InputMsg)]);
                        const prev_left = self.input.mouse_left;
                        self.input.mouse_x = input.x;
                        self.input.mouse_y = input.y;
                        self.input.mouse_left = (input.code & 1) != 0;
                        self.input.left_pressed = self.input.left_pressed or (self.input.mouse_left and !prev_left);
                        self.input.left_released = self.input.left_released or (!self.input.mouse_left and prev_left);
                    }
                },
                3 => {
                    if (ret >= 1 + @sizeOf(protocol.InputMsg)) {
                        var input: protocol.InputMsg = undefined;
                        @memcpy(std.mem.asBytes(&input), ev_buf[1..1+@sizeOf(protocol.InputMsg)]);
                        if (input.x > 0 and input.y > 0) {
                            self.width = @intCast(input.x);
                            self.height = @intCast(input.y);
                        }
                    }
                },
                11 => {
                    // FocusNotify — compositor tells us whether we have focus
                    if (ret >= 1 + @sizeOf(protocol.FocusNotifyMsg)) {
                        var notify: protocol.FocusNotifyMsg = undefined;
                        @memcpy(std.mem.asBytes(&notify), ev_buf[1..1+@sizeOf(protocol.FocusNotifyMsg)]);
                         // ponytail: focus state available for future use
                    }
                },
                else => {},
            }
        }
    }

    pub fn begin_ui(self: *Client) UI {
        return UI.begin(self.canvas, self.input, &self.ui_state);
    }

    pub fn flush(self: *Client, rect: Rect) void {
        if (rect.w == 0 or rect.h == 0) return;
        var msg = protocol.FlushMsg{ .surface_id = self.surface_id, .x = @intCast(rect.x), .y = @intCast(rect.y), .width = rect.w, .height = rect.h };
        _ = send_msg(.Flush, std.mem.asBytes(&msg));
    }
    pub fn resize(self: *Client, new_w: u32, new_h: u32) void {
        var msg = protocol.ResizeMsg{ .surface_id = self.surface_id, .width = new_w, .height = new_h };
        _ = send_msg(.Resize, std.mem.asBytes(&msg));
    }

    pub fn destroy(self: *Client) void {
        var msg = protocol.DestroySurfaceMsg{ .surface_id = self.surface_id };
        _ = send_msg(.DestroySurface, std.mem.asBytes(&msg));
    }

    pub fn lower(self: *Client) void {
        var msg = protocol.LowerSurfaceMsg{ .surface_id = self.surface_id };
        _ = send_msg(.LowerSurface, std.mem.asBytes(&msg));
    }

};

pub const Event = union(enum) {
    key: u32,
    mouse: struct { x: i32, y: i32, buttons: u32 },
    resize: struct { w: u32, h: u32 },
};
