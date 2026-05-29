//! Filesystem consistency check (fsck) for ZiqaFS.

use super::block::read_block;
use super::inode::{inode_get_block, read_inode};
use super::types::*;
use crate::drivers::block::BlockDevice;
use crate::zig_kernel_ops;

pub fn fsck(device: &dyn BlockDevice, sb: &Superblock) -> FsckResult {
    let mut errors = 0u32;
    let mut leaked_inodes = 0u32;

    if sb.magic != MAGIC {
        return FsckResult { ok: false, errors: 1, leaked_blocks: 0, leaked_inodes: 0 };
    }

    let mut bitmap = [0u8; BLOCK_SIZE];
    // ARCH: [fsck→block] CS-24 fsck reads BITMAP_BLOCK to compare allocated bits against
    //       blocks reachable from live inodes. Detects leaked blocks; read-only, no writes.
    if read_block(device, BITMAP_BLOCK, &mut bitmap).is_err() {
        return FsckResult { ok: false, errors: 1, leaked_blocks: 0, leaked_inodes: 0 };
    }

    let mut reachable = [0u8; BLOCK_SIZE];
    for b in 0..sb.first_data_block {
        reachable[b as usize / 8] |= 1 << (b as usize % 8);
    }

    let mut live_inodes = 0u32;
    for i in 1..INODE_COUNT {
        if let Ok(inode) = read_inode(device, i) {
            if inode.mode == 0 {
                continue;
            }
            live_inodes += 1;
            let total_blocks = (inode.size as usize + BLOCK_SIZE - 1) / BLOCK_SIZE;
            for logical in 0..total_blocks as u32 {
                if let Ok(phys) = inode_get_block(device, &inode, logical) {
                    if phys != 0 {
                        reachable[phys as usize / 8] |= 1 << (phys as usize % 8);
                    }
                }
            }
            if inode.indirect != 0 {
                reachable[inode.indirect as usize / 8] |= 1 << (inode.indirect as usize % 8);
            }
            if inode.double_indirect != 0 {
                reachable[inode.double_indirect as usize / 8] |=
                    1 << (inode.double_indirect as usize % 8);
            }
        }
    }

    let leaked_blocks = zig_kernel_ops::bitmap_count_leaked(
        &bitmap, &reachable, sb.first_data_block, sb.total_blocks,
    );
    if leaked_blocks > 0 {
        errors += leaked_blocks;
    }

    let expected_free = INODE_COUNT - 1 - live_inodes;
    if sb.free_inodes != expected_free {
        leaked_inodes = (sb.free_inodes as i32 - expected_free as i32).unsigned_abs();
        errors += 1;
    }

    FsckResult { ok: errors == 0, errors, leaked_blocks, leaked_inodes }
}
