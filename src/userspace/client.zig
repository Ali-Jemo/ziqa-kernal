//! Zig Client for Axiq-IQ Native Compositor (NWCC)
//! 
//! Architecture:
//! 1. Allocates SHM segment via native Axiq-IQ syscall.
//! 2. Sends WlMessage::CreateBuffer via IPC.
//! 3. High-performance rendering loop (pure Zig).
//! 4. Bypasses POSIX/Linux overhead for ultra-low latency.

const std = @import("std");

// Native Axiq-IQ Syscall Numbers (Synced with src/abi/syscall.rs)
const ZIQA_SHM_CREATE = 1010;
const ZIQA_SHM_ATTACH = 1011;
const ZIQA_IPC_CREATE = 1020;
const ZIQA_IPC_SEND   = 1021;
const ZIQA_IPC_RECV   = 1022;

// Wayland-inspired IPC protocol messages for NWCC (must match compositor.rs)
const WlMessageTag = enum(u32) {
    CreateSurface = 0,
    CreateBuffer  = 1,
    Attach        = 2,
    SetPosition   = 3,
    Commit        = 4,
};

const WlMessage = extern union {
    create_surface: extern struct { tag: WlMessageTag = .CreateSurface, owner_pid: u64 },
    create_buffer: extern struct { tag: WlMessageTag = .CreateBuffer, owner_pid: u64, shm_id: u64, width: u32, height: u32 },
    attach: extern struct { tag: WlMessageTag = .Attach, surface_id: u64, buffer_id: u64 },
    set_position: extern struct { tag: WlMessageTag = .SetPosition, surface_id: u64, x: i32, y: i32 },
};

extern fn ziqa_syscall(num: u64, a1: u64, a2: u64, a3: u64, a4: u64, a5: u64, a6: u64) u64;

pub fn main() void {
    const width: u32 = 400;
    const height: u32 = 300;
    const compositor_chan: u64 = 1;

    // 1. Create SHM Segment
    const shm_id = ziqa_syscall(ZIQA_SHM_CREATE, width * height * 4, 0, 0, 0, 0, 0);
    const shm_ptr: [*]u32 = @ptrFromInt(ziqa_syscall(ZIQA_SHM_ATTACH, shm_id, 0, 0, 0, 0, 0));

    // 2. Register Buffer with Compositor
    var msg = WlMessage{
        .create_buffer = .{
            .owner_pid = 0,
            .shm_id = shm_id,
            .width = width,
            .height = height,
        },
    };
    _ = ziqa_syscall(ZIQA_IPC_SEND, compositor_chan, @intFromPtr(&msg), @sizeOf(WlMessage), 0, 0, 0);

    // 3. High-performance rendering loop
    var tick: u32 = 0;
    while (true) {
        tick +%= 1;
        
        // Render cycle directly in SHM
        var i: usize = 0;
        while (i < width * height) : (i += 1) {
            const x = @as(u32, @intCast(i % width));
            const y = @as(u32, @intCast(i / width));
            const r = (x + tick) & 0xFF;
            const g = (y + tick) & 0xFF;
            const b = (x + y + tick) & 0xFF;
            shm_ptr[i] = (r << 16) | (g << 8) | b;
        }
        
        // Signal compositor (Attach/Commit)
        const commit_msg = WlMessage{ .attach = .{ .surface_id = 1, .buffer_id = 1 } };
        _ = ziqa_syscall(ZIQA_IPC_SEND, compositor_chan, @intFromPtr(&commit_msg), @sizeOf(WlMessage), 0, 0, 0);
    }
}
