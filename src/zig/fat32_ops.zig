// ZiqaKernel — FAT32 hot-path helpers written in Zig
// C-ABI exports called from Rust via extern "C".
//
// These mirror the Rust helpers in src/fs/fat32.rs but run under Zig
// ReleaseFast for better throughput on block-device I/O prep.
const std = @import("std");

pub fn build(b: *std.Build) void {
    _ = b;
}

// ── helpers ──────────────────────────────────────────────────────────────

inline fn fatEntryOffset(cluster: u32, bytes_per_sector: u32) u64 {
    return @as(u64, cluster) * 4;
}

inline fn fatSector(
    fat_start_sector: u64,
    fat_offset: u64,
    bytes_per_sector: u32,
) u64 {
    return fat_start_sector + fat_offset / @as(u64, bytes_per_sector);
}

inline fn offsetInSector(fat_offset: u64, bytes_per_sector: u32) usize {
    return @intCast(fat_offset % @as(u64, bytes_per_sector));
}

// ── exports ──────────────────────────────────────────────────────────────

/// Read one FAT32 entry (4 bytes) for `cluster` from the FAT.
/// Returns 0 on error.
export fn fat32_read_fat_entry(
    disk_read_sectors: *const fn (sector: u64, count: u32, buf: [*]u8) callconv(.C) bool,
    fat_start_sector: u64,
    bytes_per_sector: u32,
    cluster: u32,
) u32 {
    const fat_offset = fatEntryOffset(cluster, bytes_per_sector);
    const sector = fatSector(fat_start_sector, fat_offset, bytes_per_sector);
    const off = offsetInSector(fat_offset, bytes_per_sector);

    var buf: [512]u8 = undefined;
    if (!disk_read_sectors(sector, 1, &buf)) return 0;
    if (off + 4 > buf.len) return 0;

    return std.mem.readIntLittle(u32, buf[off..][0..4]) & 0x0FFF_FFFF;
}

/// Write a FAT32 entry for `cluster` with `value` in ALL FAT copies.
/// Returns true on success.
export fn fat32_write_fat_entry(
    disk_read_sectors: *const fn (sector: u64, count: u32, buf: [*]u8) callconv(.C) bool,
    disk_write_sectors: *const fn (sector: u64, count: u32, buf: [*]const u8) callconv(.C) bool,
    fat_start_sector: u64,
    bytes_per_sector: u32,
    num_fats: u32,
    sectors_per_fat: u32,
    cluster: u32,
    value: u32,
) bool {
    const fat_offset = fatEntryOffset(cluster, bytes_per_sector);
    const base_sector = fatSector(fat_start_sector, fat_offset, bytes_per_sector);
    const off = offsetInSector(fat_offset, bytes_per_sector);

    var buf: [512]u8 = undefined;
    if (!disk_read_sectors(base_sector, 1, &buf)) return false;

    const masked = (std.mem.readIntLittle(u32, buf[off..][0..4]) & 0xF000_0000) | (value & 0x0FFF_FFFF);
    std.mem.writeIntLittle(u32, buf[off..][0..4], masked);

    var fat_idx: u32 = 0;
    while (fat_idx < num_fats) : (fat_idx += 1) {
        const sector = base_sector + @as(u64, fat_idx) * @as(u64, sectors_per_fat);
        if (!disk_write_sectors(sector, 1, &buf)) return false;
    }
    return true;
}

/// Zero a whole cluster on disk.
export fn fat32_zero_cluster(
    disk_write_sectors: *const fn (sector: u64, count: u32, buf: [*]const u8) callconv(.C) bool,
    cluster_sector: u64,
    sectors_per_cluster: u32,
    bytes_per_sector: u32,
) bool {
    const len = @as(usize, sectors_per_cluster) * @as(usize, bytes_per_sector);
    const zeros: [4096]u8 = std.mem.zeroes([4096]u8);
    if (len > zeros.len) return false;
    return disk_write_sectors(cluster_sector, sectors_per_cluster, &zeros);
}

/// Scan the FAT for a free cluster, starting at `start_cluster`.
/// Returns 0 if none found.
export fn fat32_find_free_cluster(
    disk_read_sectors: *const fn (sector: u64, count: u32, buf: [*]u8) callconv(.C) bool,
    fat_start_sector: u64,
    bytes_per_sector: u32,
    sectors_per_fat: u32,
    max_clusters: u32,
    start_cluster: u32,
) u32 {
    const bps = bytes_per_sector;
    var sec: u32 = 0;
    while (sec < sectors_per_fat) : (sec += 1) {
        var buf: [512]u8 = undefined;
        if (!disk_read_sectors(fat_start_sector + @as(u64, sec), 1, &buf)) continue;

        var off: usize = 0;
        while (off < @as(usize, bps)) : (off += 4) {
            const cluster = (@as(u32, @intCast(sec)) * bps + @as(u32, @intCast(off))) / 4;
            if (cluster < 2 or cluster >= max_clusters) continue;
            const entry = std.mem.readIntLittle(u32, buf[off..][0..4]) & 0x0FFF_FFFF;
            if (entry == 0) return cluster;
        }
    }
    return 0;
}

/// Allocate one free cluster and link it after `prev_cluster` (if any).
/// Writes EOC to the new cluster and zeroes it.
/// Returns 0 on failure.
export fn fat32_allocate_cluster(
    disk_read_sectors: *const fn (sector: u64, count: u32, buf: [*]u8) callconv(.C) bool,
    disk_write_sectors: *const fn (sector: u64, count: u32, buf: [*]const u8) callconv(.C) bool,
    fat_start_sector: u64,
    bytes_per_sector: u32,
    num_fats: u32,
    sectors_per_fat: u32,
    max_clusters: u32,
    cluster_to_sector_fn: *const fn (cluster: u32, data_start_sector: u64, sectors_per_cluster: u32) u64,
    data_start_sector: u64,
    sectors_per_cluster: u32,
    prev_cluster: u32,
) u32 {
    const new_cluster = fat32_find_free_cluster(
        disk_read_sectors,
        fat_start_sector,
        bytes_per_sector,
        sectors_per_fat,
        max_clusters,
        2,
    );
    if (new_cluster == 0) return 0;

    if (!fat32_write_fat_entry(
        disk_read_sectors,
        disk_write_sectors,
        fat_start_sector,
        bytes_per_sector,
        num_fats,
        sectors_per_fat,
        new_cluster,
        0x0FFF_FFFF,
    )) return 0;

    if (prev_cluster != 0) {
        if (!fat32_write_fat_entry(
            disk_read_sectors,
            disk_write_sectors,
            fat_start_sector,
            bytes_per_sector,
            num_fats,
            sectors_per_fat,
            prev_cluster,
            new_cluster,
        )) {
            const _ = fat32_write_fat_entry(
                disk_read_sectors,
                disk_write_sectors,
                fat_start_sector,
                bytes_per_sector,
                num_fats,
                sectors_per_fat,
                new_cluster,
                0,
            );
            return 0;
        }
    }

    const sector = cluster_to_sector_fn(new_cluster, data_start_sector, sectors_per_cluster);
    if (!fat32_zero_cluster(
        disk_write_sectors,
        sector,
        sectors_per_cluster,
        bytes_per_sector,
    )) {
        const _ = fat32_write_fat_entry(
            disk_read_sectors,
            disk_write_sectors,
            fat_start_sector,
            bytes_per_sector,
            num_fats,
            sectors_per_fat,
            new_cluster,
            0,
        );
        if (prev_cluster != 0) {
            const _ = fat32_write_fat_entry(
                disk_read_sectors,
                disk_write_sectors,
                fat_start_sector,
                bytes_per_sector,
                num_fats,
                sectors_per_fat,
                prev_cluster,
                0x0FFF_FFFF,
            );
        }
        return 0;
    }

    return new_cluster;
}

/// Free a whole cluster chain, clearing every FAT entry to 0.
/// Returns true on success.
export fn fat32_free_cluster_chain(
    disk_read_sectors: *const fn (sector: u64, count: u32, buf: [*]u8) callconv(.C) bool,
    disk_write_sectors: *const fn (sector: u64, count: u32, buf: [*]const u8) callconv(.C) bool,
    fat_start_sector: u64,
    bytes_per_sector: u32,
    num_fats: u32,
    sectors_per_fat: u32,
    follow_fn: *const fn (
        disk_read_sectors: *const fn (sector: u64, count: u32, buf: [*]u8) callconv(.C) bool,
        bpb: *const Fat32Bpb,
        current_cluster: u32,
    ) callconv(.C) ?u32,
    bpb: *const Fat32Bpb,
    start_cluster: u32,
) bool {
    if (start_cluster < 2) return true;

    var cluster = start_cluster;
    while (true) {
        const next = follow_fn(disk_read_sectors, bpb, cluster) orelse break;
        if (!fat32_write_fat_entry(
            disk_read_sectors,
            disk_write_sectors,
            fat_start_sector,
            bytes_per_sector,
            num_fats,
            sectors_per_fat,
            cluster,
            0,
        )) return false;

        if (next >= 2) {
            cluster = next;
        } else break;
    }
    return true;
}

pub const Fat32Bpb = extern struct {
    bytes_per_sector: u16,
    sectors_per_cluster: u8,
    reserved_sectors: u16,
    num_fats: u8,
    sectors_per_fat_32: u32,
    root_cluster: u32,
    partition_start: u64,
};
