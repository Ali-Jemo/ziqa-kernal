const builtin = @import("builtin");

pub const Simd = struct {
    pub fn copy(dst: [*]u8, src: [*]const u8, len: usize) void {
        if (builtin.cpu.arch == .x86_64) {
            copy128(dst, src, len);
        } else {
            @memcpy(dst[0..len], src[0..len]);
        }
    }

    pub fn fill(dst: [*]u32, color: u32, len: u32) void {
        if (builtin.cpu.arch == .x86_64 and len >= 4) {
            fill128(dst, color, len);
        } else {
            var i: u32 = 0;
            while (i < len) : (i += 1) dst[i] = color;
        }
    }

    fn fill128(dst: [*]u32, color: u32, len: u32) void {
        const n = len / 4;
        const v: @Vector(4, u32) = @splat(color);
        var i: u32 = 0;
        while (i < n) : (i += 1) {
            const p: *align(1) [4]u32 = @ptrCast(&dst[i * 4]);
            p.* = @as([4]u32, v);
        }
        const tail = n * 4;
        var j = tail;
        while (j < len) : (j += 1) dst[j] = color;
    }

    fn copy128(dst: [*]u8, src: [*]const u8, len: usize) void {
        const n = len / 16;
        var i: usize = 0;
        while (i < n) : (i += 1) {
            const s: *align(1) const [16]u8 = @ptrCast(&src[i * 16]);
            const d: *align(1) [16]u8 = @ptrCast(&dst[i * 16]);
            d.* = s.*;
        }
        const tail = n * 16;
        @memcpy(dst[tail..len], src[tail..len]);
    }
};

test "fill sets 16 pixels correctly" {
    var pixels = [_]u32{0} ** 16;
    Simd.fill(&pixels, 0xDEADBEEF, 16);
    for (pixels) |p| {
        try @import("std").testing.expectEqual(@as(u32, 0xDEADBEEF), p);
    }
}

test "fill handles unaligned tail" {
    var pixels = [_]u32{0} ** 17;
    Simd.fill(&pixels, 0xCAFEBABE, 17);
    try @import("std").testing.expectEqual(@as(u32, 0xCAFEBABE), pixels[16]);
}

test "copy copies 16 bytes correctly" {
    var src = [_]u8{ 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16 };
    var dst = [_]u8{0} ** 16;
    Simd.copy(&dst, &src, 16);
    try @import("std").testing.expectEqualSlices(u8, &src, &dst);
}

test "copy handles unaligned tail" {
    var src = [_]u8{ 0xAA, 0xBB, 0xCC, 0xDD, 0xEE };
    var dst = [_]u8{0} ** 5;
    Simd.copy(&dst, &src, 5);
    try @import("std").testing.expectEqual(@as(u8, 0xEE), dst[4]);
}
