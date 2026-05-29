//! Zig Client for Axiq-IQ Native Compositor (NWCC)
//! 
//! Architecture:
//! 1. Allocates SHM segment via native Axiq-IQ syscall.
//! 2. Sends WlMessage::CreateBuffer via IPC.
//! 3. High-performance rendering loop (pure Zig).
//! 4. Bypasses POSIX/Linux overhead for ultra-low latency.

const std = @import("std");

// Native Axiq-IQ Syscall Numbers
const ZIQA_SHM_CREATE = 1010;
const ZIQA_IPC_SEND   = 1020;

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
    const compositor_chan: u64 = 1; // Well-known or passed via env

    // 1. Create SHM Segment (400x300x4 bytes)
    const shm_id = ziqa_syscall(ZIQA_SHM_CREATE, width * height * 4, 0, 0, 0, 0, 0);

    // 2. Register Buffer with Compositor
    var msg = WlMessage{
        .create_buffer = .{
            .owner_pid = 0, // Current process
            .shm_id = shm_id,
            .width = width,
            .height = height,
        },
    };
    _ = ziqa_syscall(ZIQA_IPC_SEND, compositor_chan, @intFromPtr(&msg), @sizeOf(WlMessage), 0, 0, 0);

    // 3. High-performance rendering loop
    var color: u32 = 0xFFFF0000; // Bright Red
    while (true) {
        // High-speed Zig logic here (e.g., raycasting, particles)
        // For demo: just cycle colors to show speed
        color = color +% 1;
        
        // Signal compositor to refresh (Commit)
        const commit_msg = WlMessage{ .attach = .{ .surface_id = 1, .buffer_id = 1 } };
        _ = ziqa_syscall(ZIQA_IPC_SEND, compositor_chan, @intFromPtr(&commit_msg), @sizeOf(WlMessage), 0, 0, 0);
    }
}
