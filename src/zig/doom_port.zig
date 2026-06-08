// ZiqaKernel Doom Port — platform abstraction layer
//
// Provides the DG_* functions that doomgeneric (or any C Doom port)
// expects, translating them into ZiqaKernel `int 0x80` syscalls.
//
// Can be compiled both as:
//   1. A standalone `doom.elf` executable (for testing the syscall path)
//   2. An object file linked with doomgeneric C code
//
// Syscall convention (int 0x80):
//   RAX = number, RDI = a0, RSI = a1, RDX = a2, R10 = a3, R8 = a4, R9 = a5
//   Return in RAX.

const sys = struct {
    const FB_BLIT: u64 = 204;
    const GET_TICKS: u64 = 205;
    const GET_KEY: u64 = 206;
    const NANOSLEEP: u64 = 230;

    inline fn syscall3(num: u64, a0: u64, a1: u64, a2: u64) u64 {
        return asm volatile (
            \\ int $0x80
            : [ret] "={rax}" (-> u64),
            : [num] "{rax}" (num),
              [a0] "{rdi}" (a0),
              [a1] "{rsi}" (a1),
              [a2] "{rdx}" (a2),
            : .{ .memory = true, .rcx = true, .r11 = true }
        );
    }

    inline fn syscall4(num: u64, a0: u64, a1: u64, a2: u64, a3: u64) u64 {
        return asm volatile (
            \\ int $0x80
            : [ret] "={rax}" (-> u64),
            : [num] "{rax}" (num),
              [a0] "{rdi}" (a0),
              [a1] "{rsi}" (a1),
              [a2] "{rdx}" (a2),
              [a3] "{r10}" (a3),
            : .{ .memory = true, .rcx = true, .r11 = true }
        );
    }

    inline fn syscall6(num: u64, a0: u64, a1: u64, a2: u64, a3: u64, a4: u64, a5: u64) u64 {
        return asm volatile (
            \\ int $0x80
            : [ret] "={rax}" (-> u64),
            : [num] "{rax}" (num),
              [a0] "{rdi}" (a0),
              [a1] "{rsi}" (a1),
              [a2] "{rdx}" (a2),
              [a3] "{r10}" (a3),
              [a4] "{r8}" (a4),
              [a5] "{r9}" (a5),
            : .{ .memory = true, .rcx = true, .r11 = true }
        );
    }

    fn fb_blit(pixels: [*]const u8, palette: [*]const u32, w: u32, h: u32, dst_x: u32, dst_y: u32) u64 {
        return syscall6(FB_BLIT, @intFromPtr(pixels), @intFromPtr(palette), w, h, dst_x, dst_y);
    }

    fn get_ticks() u64 {
        return syscall3(GET_TICKS, 0, 0, 0);
    }

    fn get_key() u64 {
        return syscall3(GET_KEY, 0, 0, 0);
    }

    fn sleep_ms(ms: u64) u64 {
        return syscall3(NANOSLEEP, ms, 0, 0);
    }
};

// ─── DOOM palette (XRGB8888) ───────────────────────────────────────────
// Classic 256-color Doom palette, generated from the canonical values.
// Placeholder — in production, doomgeneric provides its own palette via
// DG_DrawFrame which we also pass through the syscall.
const DOOM_PALETTE: [256]u32 = blk: {
    var p: [256]u32 = undefined;
    // Fill with a smooth fire-ish gradient as a default
    var i: usize = 0;
    while (i < 256) : (i += 1) {
        const r: u32 = @min(@as(u32, @intCast(i * 2)), 255);
        const g: u32 = @min(@as(u32, @intCast(i)), 200);
        const b: u32 = @min(@as(u32, @intCast(i / 2)), 100);
        p[i] = (r << 16) | (g << 8) | b;
    }
    break :blk p;
};

// ─── Internal framebuffer for Doom rendering ───────────────────────────
// doomgeneric renders to I_VideoBuffer[SCREENWIDTH * SCREENHEIGHT].
const SCREEN_W: usize = 320;
const SCREEN_H: usize = 200;
var video_buf: [SCREEN_W * SCREEN_H]u8 = undefined;

// ─── Exported DG_* API (C ABI) ─────────────────────────────────────────
// These are the functions that doomgeneric C code expects the platform
// to provide.  When compiled as a standalone ELF, main() also calls them.

/// Called once at startup.
export fn DG_Init() void {
    _ = sys.fb_blit(&video_buf, &DOOM_PALETTE, SCREEN_W, SCREEN_H, 0, 0);
}

/// Present the internal Doom framebuffer to the screen.
/// doomgeneric fills `I_VideoBuffer` (alias for `video_buf`) before calling
/// this. We blit it to the kernel framebuffer via FB_BLIT syscall.
export fn DG_DrawFrame() void {
    _ = sys.fb_blit(&video_buf, &DOOM_PALETTE, SCREEN_W, SCREEN_H, 0, 0);
}

/// Sleep for `ms` milliseconds.
export fn DG_SleepMs(ms: u32) void {
    _ = sys.sleep_ms(ms);
}

/// Return the number of milliseconds since kernel boot.
export fn DG_GetTicksMs() u32 {
    return @truncate(sys.get_ticks());
}

/// Read a key. If a key is available, set `pressed` to 1 and `doomKey`
/// to the Doom scancode; otherwise set `pressed` to 0.
/// Returns 0 on success.
export fn DG_GetKey(pressed: *i32, doomKey: *u8) i32 {
    const ascii = sys.get_key();
    if (ascii == 0) {
        pressed.* = 0;
        doomKey.* = 0;
        return 0;
    }
    pressed.* = 1;
    doomKey.* = asciiToDoomKey(@truncate(ascii));
    return 0;
}

/// Set the window title (no-op in kernel framebuffer mode).
export fn DG_SetWindowTitle(_: [*]const u8) void {}

// ─── Doom scancode conversion ──────────────────────────────────────────

fn asciiToDoomKey(c: u8) u8 {
    return switch (c) {
        'a'...'z' => c - 'a' + 0x1E, // KEY_A..KEY_Z
        'A'...'Z' => c - 'A' + 0x1E,
        '0'...'9' => c - '0' + 0x0B, // KEY_1..KEY_0 (scancodes 0x0B..0x0A for 1..9,0)
        ' ' => 0x39,  // KEY_SPACE
        '\n', '\r' => 0x1C, // KEY_ENTER
        0x08 => 0x0E, // KEY_BACKSPACE
        0x09 => 0x0F, // KEY_TAB
        0x1B => 0x01, // KEY_ESC
        else => 0,
    };
}

// ─── Utility: draw a colored bar pattern to video_buf ──────────────────

fn fillRect(buf: []u8, pitch: usize, x: usize, y: usize, w: usize, h: usize, color: u8) void {
    var row: usize = 0;
    while (row < h) : (row += 1) {
        var col: usize = 0;
        while (col < w) : (col += 1) {
            const sx = x + col;
            const sy = y + row;
            if (sx < SCREEN_W and sy < SCREEN_H) {
                buf[sy * pitch + sx] = color;
            }
        }
    }
}

fn drawTestPattern(buf: []u8) void {
    // Clear to black
    @memset(buf, 0);

    // Color bars
    const colors = [_]u8{ 4, 36, 68, 100, 132, 164, 196, 228 };
    const bar_w = SCREEN_W / colors.len;
    for (colors, 0..) |c, i| {
        fillRect(buf, SCREEN_W, i * bar_w, 0, bar_w, SCREEN_H / 2, c);
    }

    // Gradient bars
    var x: usize = 0;
    while (x < SCREEN_W) : (x += 1) {
        const c: u8 = @truncate(@as(u32, @intCast(x * 255 / SCREEN_W)));
        fillRect(buf, SCREEN_W, x, SCREEN_H / 2, 1, SCREEN_H / 4, c);
    }

    // Text area (simulated with a bright strip at the bottom)
    fillRect(buf, SCREEN_W, 0, SCREEN_H * 3 / 4, SCREEN_W, SCREEN_H / 4, 180);

    // Add some text-like patterns in the bright strip
    var tx: usize = 0;
    while (tx < SCREEN_W) : (tx += 1) {
        const c: u8 = if (tx % 8 < 4) 255 else 0;
        buf[(SCREEN_H * 3 / 4 + 4) * SCREEN_W + tx] = c;
        buf[(SCREEN_H * 3 / 4 + 12) * SCREEN_W + tx] = c;
    }
}

// ─── Standalone main() ─────────────────────────────────────────────────

pub fn main() void {
    DG_Init();

    // Draw a test pattern to show the syscall interface works
    drawTestPattern(&video_buf);
    DG_DrawFrame();

    // Animate a simple color-cycling bar across the screen
    var tick: u32 = 0;
    while (true) {
        const bar_pos = (tick * 2) % (SCREEN_W + 40);
        const x: usize = if (bar_pos > 20) (bar_pos - 20) else 0;

        // Re-draw base pattern
        drawTestPattern(&video_buf);

        // Moving bright bar
        var by: usize = 0;
        while (by < SCREEN_H) : (by += 1) {
            const col: u8 = @truncate((tick * 3 + @as(u32, @intCast(by))) & 0xFF);
            fillRect(&video_buf, SCREEN_W, x, by, 8, 1, col);
        }

        DG_DrawFrame();
        DG_SleepMs(33);

        // Check for ESC key to exit
        const k = sys.get_key();
        if (k == 0x1B) break;

        tick +%= 1;
    }

    // Clear screen and exit
    @memset(&video_buf, 0);
    DG_DrawFrame();
}
