//! Zig Client for ZiqaKernel Compositor
//!
//! Demonstrates the compositor protocol:
//! 1. Allocate SHM
//! 2. Connect to compositor (channel 3)
//! 3. Create surface
//! 4. Attach buffer (SHM)
//! 5. Loop: render gradient, send Flush

const std = @import("std");

// Native Axiq-IQ Syscall Numbers (Synced with src/abi/syscall.rs)
const ZIQA_SHM_CREATE = 1010;
const ZIQA_SHM_ATTACH = 1011;
const ZIQA_IPC_SEND   = 1021;

// Well-known compositor channel (registered in init.rs)
const COMPOSITOR_CHAN: u64 = 3;

// OpCodes matching src/ipc/gui.rs
const OpCode = enum(u8) {
    Connect = 1,
    CreateSurface = 2,
    Flush = 3,
    Input = 4,
    BufferAttach = 5,
    SetPosition = 6,
};

// Message payloads (packed after opcode byte)

const ConnectMsg = extern struct {
    pid: u64,
};

const CreateSurfaceMsg = extern struct {
    width: u32,
    height: u32,
};

const FlushMsg = extern struct {
    surface_id: u32,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
};

const BufferAttachMsg = extern struct {
    surface_id: u32,
    shm_id: u32,
    width: u32,
    height: u32,
};

const SetPositionMsg = extern struct {
    surface_id: u32,
    x: i32,
    y: i32,
};

extern fn ziqa_syscall(num: u64, a1: u64, a2: u64, a3: u64, a4: u64, a5: u64, a6: u64) u64;

/// Send a message to the compositor channel.
/// Message format: [opcode_byte][payload_bytes]
fn send_msg(opcode: OpCode, payload: []const u8) void {
    var buf: [256]u8 = undefined;
    buf[0] = @intFromEnum(opcode);
    if (payload.len > 0) {
        @memcpy(buf[1..][0..payload.len], payload);
    }
    _ = ziqa_syscall(ZIQA_IPC_SEND, COMPOSITOR_CHAN, @intFromPtr(&buf), payload.len + 1, 0, 0, 0);
}

pub fn main() void {
    const width: u32 = 400;
    const height: u32 = 300;

    // 1. Create SHM segment
    const shm_id = ziqa_syscall(ZIQA_SHM_CREATE, width * height * 4, 0, 0, 0, 0, 0);
    const shm_ptr: [*]u32 = @ptrFromInt(ziqa_syscall(ZIQA_SHM_ATTACH, shm_id, 0, 0, 0, 0, 0));

    // 2. Connect to compositor
    var conn = ConnectMsg{ .pid = 0 };
    send_msg(OpCode.Connect, @as([*]const u8, @ptrCast(&conn))[0..@sizeOf(ConnectMsg)]);

    // 3. Create surface
    var surf = CreateSurfaceMsg{ .width = width, .height = height };
    send_msg(OpCode.CreateSurface, @as([*]const u8, @ptrCast(&surf))[0..@sizeOf(CreateSurfaceMsg)]);

    // 4. Attach buffer (SHM) to surface
    var attach = BufferAttachMsg{
        .surface_id = 1, // first surface gets id 1
        .shm_id = @intCast(shm_id),
        .width = width,
        .height = height,
    };
    send_msg(OpCode.BufferAttach, @as([*]const u8, @ptrCast(&attach))[0..@sizeOf(BufferAttachMsg)]);

    // 5. Rendering loop

    // 4.5. Set surface position (centered on 1024x768 fb)
    var pos = SetPositionMsg{
        .surface_id = 1,
        .x = 312,
        .y = 234,
    };
    send_msg(OpCode.SetPosition, @as([*]const u8, @ptrCast(&pos))[0..@sizeOf(SetPositionMsg)]);
    var tick: u32 = 0;
    while (true) {
        tick +%= 1;

        // Render gradient directly in SHM
        var i: usize = 0;
        while (i < width * height) : (i += 1) {
            const x = @as(u32, @intCast(i % width));
            const y = @as(u32, @intCast(i / width));
            const r = (x + tick) & 0xFF;
            const g = (y + tick) & 0xFF;
            const b = (x + y + tick) & 0xFF;
            shm_ptr[i] = (r << 16) | (g << 8) | b;
        }

        // Flush: tell compositor to repaint
        // Flush: tell compositor to repaint this surface
        var flush = FlushMsg{
            .surface_id = 1,
            .x = 0,
            .y = 0,
            .width = width,
            .height = height,
        };
        send_msg(OpCode.Flush, @as([*]const u8, @ptrCast(&flush))[0..@sizeOf(FlushMsg)]);
    }
}
