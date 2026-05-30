//! Zig Client for Axiq-IQ Native Compositor (NWCC)
//!
//! Protocol flow:
//!   1. CreateSurface  → get surface_id
//!   2. CreateBuffer   → get buffer_id
//!   3. SetTitle       → label the window
//!   4. Attach         → bind buffer to surface
//!   5. SetPosition    → initial placement
//!   6. Render loop    → write pixels to SHM, send Commit

const std = @import("std");

// Native Axiq-IQ Syscall Numbers
const ZIQA_SHM_CREATE = 1010;
const ZIQA_SHM_ATTACH = 1011;
const ZIQA_IPC_SEND   = 1021;

const COMPOSITOR_CHAN: u64 = 1;

// Must match compositor.rs WlMessage layout exactly.
const Tag = enum(u32) {
    CreateSurface = 0,
    CreateBuffer  = 1,
    Attach        = 2,
    SetPosition   = 3,
    Commit        = 4,
    SetTitle      = 5,
};

// Fixed-size union so every variant is the same size on the wire.
const MSG_SIZE = 48;

const Msg = extern struct {
    tag:  Tag,
    a:    u64 = 0,
    b:    u64 = 0,
    c:    u64 = 0,
    d:    u64 = 0,
    e:    u64 = 0,
};

extern fn ziqa_syscall(num: u64, a1: u64, a2: u64, a3: u64, a4: u64, a5: u64, a6: u64) u64;

fn send(msg: Msg) void {
    _ = ziqa_syscall(ZIQA_IPC_SEND, COMPOSITOR_CHAN, @intFromPtr(&msg), @sizeOf(Msg), 0, 0, 0);
}

export fn zig_demo_client_main() void {
    const W: u32 = 24;
    const H: u32 = 10;

    // 1. Allocate SHM
    const shm_id = ziqa_syscall(ZIQA_SHM_CREATE, W * H * 4, 0, 0, 0, 0, 0);
    const shm_ptr: [*]u32 = @ptrFromInt(ziqa_syscall(ZIQA_SHM_ATTACH, shm_id, 0, 0, 0, 0, 0));

    // 2. Create surface → compositor assigns surface_id = next_id (starts at 1)
    send(.{ .tag = .CreateSurface, .a = 0 }); // owner_pid = 0
    const surface_id: u64 = 1; // first surface gets id=1

    // 3. Create buffer → buffer_id = next_id after surface (= 2)
    send(.{ .tag = .CreateBuffer, .a = 0, .b = shm_id, .c = W, .d = H });
    const buffer_id: u64 = 2;

    // 4. Set window title ("Demo")
    var title_msg = Msg{ .tag = .SetTitle, .a = surface_id };
    const title = "Demo";
    @memcpy(@as([*]u8, @ptrCast(&title_msg.b))[0..title.len], title);
    send(title_msg);

    // 5. Attach buffer to surface
    send(.{ .tag = .Attach, .a = surface_id, .b = buffer_id });

    // 6. Initial position
    send(.{ .tag = .SetPosition, .a = surface_id, .b = 8, .c = 4 });

    // 7. Render loop
    var tick: u32 = 0;
    while (true) {
        tick +%= 1;

        // Animated gradient pattern
        for (0..H) |row| {
            for (0..W) |col| {
                const x: u32 = @intCast(col);
                const y: u32 = @intCast(row);
                const r: u32 = (x * 10 + tick * 3) & 0xFF;
                const g: u32 = (y * 25 + tick * 2) & 0xFF;
                const b: u32 = (tick * 5 + x + y) & 0xFF;
                shm_ptr[row * W + col] = (r << 16) | (g << 8) | b;
            }
        }

        // Commit
        send(.{ .tag = .Commit, .a = surface_id });

        // ~30fps pacing
        var j: usize = 0;
        while (j < 200_000) : (j += 1) std.mem.doNotOptimizeAway(j);
    }
}
