/// ZiqaKernel — Optimised operations written in Zig for performance.
/// Exposes C-ABI functions so Rust can call them via extern "C".
///
/// Currently provides: bitmap scan, memory copy/zero, CRC32, inode scan,
/// packet copy, IP checksum.

export fn zig_bitmap_find_clear(
    bitmap: [*]const u8,
    len_bytes: u32,
    start_bit: u32,
) u32 {
    var i: u32 = start_bit;
    const end = len_bytes * 8;
    while (i < end) : (i += 1) {
        const byte_idx = i / 8;
        const bit_idx = i % 8;
        if (bitmap[byte_idx] & (@as(u8, 1) << @intCast(bit_idx)) == 0) {
            return i;
        }
    }
    return 0xFFFF_FFFF;
}

export fn zig_block_copy(
    dst: [*]u8,
    src: [*]const u8,
    len: usize,
) void {
    @memcpy(dst[0..len], src[0..len]);
}

export fn zig_block_zero(
    dst: [*]u8,
    len: usize,
) void {
    @memset(dst[0..len], 0);
}

fn crc32_byte(crc: u32) u32 {
    var c = crc;
    var j: u32 = 0;
    while (j < 8) : (j += 1) {
        if (c & 1 != 0) {
            c = (c >> 1) ^ 0xEDB88320;
        } else {
            c >>= 1;
        }
    }
    return c;
}

const crc32_table = blk: {
    @setEvalBranchQuota(3000);
    var table: [256]u32 = undefined;
    for (&table, 0..) |*entry, i| {
        entry.* = crc32_byte(@intCast(i));
    }
    break :blk table;
};

export fn zig_crc32(
    data: [*]const u8,
    len: usize,
    init_crc: u32,
) u32 {
    var crc = ~init_crc;
    var i: usize = 0;
    while (i < len) : (i += 1) {
        const idx = @as(u8, @intCast(crc & 0xFF)) ^ data[i];
        crc = (crc >> 8) ^ crc32_table[idx];
    }
    return ~crc;
}

export fn zig_inode_find_free(
    buf: [*]const u8,
    count: u32,
    stride: u32,
    start_id: u32,
) u32 {
    var i: u32 = start_id;
    while (i < count) : (i += 1) {
        const byte_off = i * stride;
        if (buf[byte_off] == 0) {
            return i;
        }
    }
    return 0xFFFF_FFFF;
}

export fn zig_bitmap_count_leaked(
    bitmap: [*]const u8,
    reachable: [*]const u8,
    start_bit: u32,
    end_bit: u32,
) u32 {
    var count: u32 = 0;
    var i: u32 = start_bit;
    while (i < end_bit) : (i += 1) {
        const byte_idx = i / 8;
        const bit_idx = i % 8;
        const mask = @as(u8, 1) << @intCast(bit_idx);
        const in_bitmap = bitmap[byte_idx] & mask;
        const in_reachable = reachable[byte_idx] & mask;
        if (in_bitmap != 0 and in_reachable == 0) {
            count += 1;
        }
    }
    return count;
}

export fn zig_inet_checksum(
    data: [*]const u8,
    len: usize,
) u16 {
    var sum: u32 = 0;
    var i: usize = 0;
    while (i + 1 < len) : (i += 2) {
        sum += @as(u32, data[i]) << 8 | data[i + 1];
    }
    if (i < len) {
        sum += @as(u32, data[i]) << 8;
    }
    while (sum >> 16 != 0) {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    return @intCast(~sum & 0xFFFF);
}

export fn zig_packet_copy(
    dst: [*]u8,
    src: [*]const u8,
    src_len: usize,
    max_len: usize,
) usize {
    const copy_len = @min(src_len, max_len);
    @memcpy(dst[0..copy_len], src[0..copy_len]);
    return copy_len;
}
