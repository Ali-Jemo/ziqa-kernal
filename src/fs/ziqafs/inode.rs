//! Inode allocation, read/write, and block mapping for ZiqaFS.

use super::block::{alloc_data_block, free_data_block, read_block, write_block};
use super::types::*;
use crate::abi::AbiError;
use crate::drivers::block::BlockDevice;
use crate::zig_kernel_ops;

fn calculate_inode_checksum(inode: &Inode) -> u32 {
    let mut temp = *inode;
    temp.checksum = 0;
    let bytes = unsafe {
        core::slice::from_raw_parts(&temp as *const Inode as *const u8, INODE_SIZE)
    };
    zig_kernel_ops::crc32(bytes)
}
pub fn read_inode(device: &dyn BlockDevice, inode_id: u32) -> Result<Inode, AbiError> {
    if inode_id >= INODE_COUNT {
        return Err(AbiError::OutOfBounds);
    }
    let mut buf = [0u8; BLOCK_SIZE];
    // ARCH: [inode→block] CS-01 read_inode reads INODE_TABLE_BLOCK (block 3) via the
    //       block-layer gateway. Caller supplies device by reference; no global state.
    read_block(device, INODE_TABLE_BLOCK, &mut buf)?;
    let offset = inode_id as usize * INODE_SIZE;
    let inode: Inode = unsafe { core::ptr::read_unaligned(buf[offset..].as_ptr() as *const Inode) };
    if inode.checksum != calculate_inode_checksum(&inode) {
        return Err(AbiError::Other("Inode checksum mismatch"));
    }
    Ok(inode)
}
pub fn patch_inode_raw(device: &dyn BlockDevice, inode_id: u32, offset: usize, data: &[u8]) -> Result<(), AbiError> {
    let mut buf = [0u8; BLOCK_SIZE];
    read_block(device, INODE_TABLE_BLOCK, &mut buf)?;
    let o = inode_id as usize * INODE_SIZE + offset;
    buf[o..o+data.len()].copy_from_slice(data);
    write_block(device, INODE_TABLE_BLOCK, &buf)
}

pub fn write_inode(device: &dyn BlockDevice, inode_id: u32, inode: &Inode) -> Result<(), AbiError> {
    if inode_id >= INODE_COUNT {
        return Err(AbiError::OutOfBounds);
    }
    let mut buf = [0u8; BLOCK_SIZE];
    // ARCH: [inode→block] CS-02 write_inode performs a read-modify-write of INODE_TABLE_BLOCK.
    //       Read here loads the full table so a single inode slot can be patched in-place.
    read_block(device, INODE_TABLE_BLOCK, &mut buf)?;
    let offset = inode_id as usize * INODE_SIZE;
    let mut inode_copy = *inode;
    inode_copy.checksum = calculate_inode_checksum(&inode_copy);
    unsafe {
        core::ptr::copy_nonoverlapping(
            &inode_copy as *const Inode as *const u8,
            buf[offset..].as_mut_ptr(),
            INODE_SIZE,
        );
    }
    write_block(device, INODE_TABLE_BLOCK, &buf)
}

pub fn alloc_inode(
    device: &dyn BlockDevice,
    sb: &mut Superblock,
    mode: u16,
) -> Result<u32, AbiError> {
    let mut buf = [0u8; BLOCK_SIZE];
    // ARCH: [inode→block] CS-03 alloc_inode scans INODE_TABLE_BLOCK for a free slot (mode==0).
    //       Read-modify-write pattern; Zig helper bitmap_find_clear operates on the loaded buffer.
    read_block(device, INODE_TABLE_BLOCK, &mut buf)?;
    let i = zig_kernel_ops::inode_find_free(&buf, INODE_COUNT, INODE_SIZE as u32, 1)
        .ok_or(AbiError::Other("No free inodes"))?;
    let offset = i as usize * INODE_SIZE;
    let now = crate::timer::TIMER.lock().uptime_secs() as u32;
    let mut inode = Inode {
        checksum: 0,
        mode,
        nlink: 1,
        uid: 0,
        gid: 0,
        size: 0,
        mtime: now,
        ctime: now,
        atime: now,
        blocks: [0u32; 10],
        block_checksums: [0u32; 10],
        indirect: 0,
        double_indirect: 0,
        indirect_checksum: 0,
        double_indirect_checksum: 0,
        reserved: [0u8; 12],
    };
    inode.checksum = calculate_inode_checksum(&inode);
    unsafe {
        core::ptr::copy_nonoverlapping(
            &inode as *const Inode as *const u8,
            buf[offset..].as_mut_ptr(),
            INODE_SIZE,
        );
    }
    write_block(device, INODE_TABLE_BLOCK, &buf)?;
    sb.free_inodes = sb.free_inodes.saturating_sub(1);
    Ok(i)
}

pub fn free_inode(
    device: &dyn BlockDevice,
    sb: &mut Superblock,
    inode_id: u32,
) -> Result<(), AbiError> {
    if inode_id >= INODE_COUNT {
        return Err(AbiError::OutOfBounds);
    }
    let mut buf = [0u8; BLOCK_SIZE];
    // ARCH: [inode→block] CS-04 free_inode zeroes the inode slot in INODE_TABLE_BLOCK.
    //       Read-modify-write; zeroing 72 bytes at the slot offset marks the inode as free.
    read_block(device, INODE_TABLE_BLOCK, &mut buf)?;
    let offset = inode_id as usize * INODE_SIZE;
    buf[offset..offset + INODE_SIZE].fill(0);
    write_block(device, INODE_TABLE_BLOCK, &buf)?;
    sb.free_inodes = sb.free_inodes.saturating_add(1);
    Ok(())
}

pub fn inode_get_block(
    device: &dyn BlockDevice,
    inode: &Inode,
    logical: u32,
) -> Result<(u32, u32), AbiError> {
    if logical < 10 {
        return Ok((inode.blocks[logical as usize], inode.block_checksums[logical as usize]));
    }
    let idx = (logical - 10) as usize;
    if idx < BLOCK_SIZE / 8 {
        if inode.indirect == 0 {
            return Ok((0, 0));
        }
        let mut buf = [0u8; BLOCK_SIZE];
        read_block(device, inode.indirect, &mut buf)?;
        if inode.indirect_checksum != 0 && zig_kernel_ops::crc32(&buf) != inode.indirect_checksum {
            return Err(AbiError::Other("Indirect block checksum mismatch"));
        }
        let ptrs: &[(u32, u32)] =
            unsafe { core::slice::from_raw_parts(buf.as_ptr() as *const (u32, u32), BLOCK_SIZE / 8) };
        return Ok(ptrs[idx]);
    }
    let idx2 = idx - BLOCK_SIZE / 8;
    if idx2 < (BLOCK_SIZE / 8) * (BLOCK_SIZE / 8) {
        if inode.double_indirect == 0 {
            return Ok((0, 0));
        }
        let mut buf = [0u8; BLOCK_SIZE];
        read_block(device, inode.double_indirect, &mut buf)?;
        if inode.double_indirect_checksum != 0 && zig_kernel_ops::crc32(&buf) != inode.double_indirect_checksum {
            return Err(AbiError::Other("Double indirect block checksum mismatch"));
        }
        let l1: &[(u32, u32)] =
            unsafe { core::slice::from_raw_parts(buf.as_ptr() as *const (u32, u32), BLOCK_SIZE / 8) };
        let l1_idx = idx2 / (BLOCK_SIZE / 8);
        let l2_idx = idx2 % (BLOCK_SIZE / 8);
        if l1[l1_idx].0 == 0 {
            return Ok((0, 0));
        }
        let (l1_phys, l1_checksum) = l1[l1_idx];
        let mut buf2 = [0u8; BLOCK_SIZE];
        read_block(device, l1_phys, &mut buf2)?;
        if l1_checksum != 0 && zig_kernel_ops::crc32(&buf2) != l1_checksum {
            return Err(AbiError::Other("L2 pointer block checksum mismatch"));
        }
        let l2: &[(u32, u32)] =
            unsafe { core::slice::from_raw_parts(buf2.as_ptr() as *const (u32, u32), BLOCK_SIZE / 8) };
        return Ok(l2[l2_idx]);
    }
    Err(AbiError::Other("File too large"))
}

pub fn inode_alloc_block(
    device: &dyn BlockDevice,
    sb: &mut Superblock,
    inode: &mut Inode,
    logical: u32,
) -> Result<u32, AbiError> {
    if logical < 10 {
        if inode.blocks[logical as usize] == 0 {
            inode.blocks[logical as usize] = alloc_data_block(device, sb)?;
        }
        return Ok(inode.blocks[logical as usize]);
    }
    let idx = (logical - 10) as usize;
    if idx < BLOCK_SIZE / 4 {
        if inode.indirect == 0 {
            inode.indirect = alloc_data_block(device, sb)?;
        }
        let mut buf = [0u8; BLOCK_SIZE];
        // ARCH: [inode→block] CS-08 inode_alloc_block reads the indirect pointer block to find
        //       or allocate a slot. Read-modify-write; writes back only if a new slot was added.
        read_block(device, inode.indirect, &mut buf)?;
        let ptrs: &mut [u32] = unsafe {
            core::slice::from_raw_parts_mut(buf.as_mut_ptr() as *mut u32, BLOCK_SIZE / 4)
        };
        if ptrs[idx] == 0 {
            ptrs[idx] = alloc_data_block(device, sb)?;
            write_block(device, inode.indirect, &buf)?;
        }
        return Ok(ptrs[idx]);
    }
    let idx2 = idx - BLOCK_SIZE / 4;
    if idx2 < (BLOCK_SIZE / 4) * (BLOCK_SIZE / 4) {
        if inode.double_indirect == 0 {
            inode.double_indirect = alloc_data_block(device, sb)?;
        }
        let mut buf = [0u8; BLOCK_SIZE];
        // ARCH: [inode→block] CS-09 inode_alloc_block reads the double-indirect L1 block to
        //       locate or allocate the L2 pointer block for the target logical index.
        read_block(device, inode.double_indirect, &mut buf)?;
        let l1: &mut [u32] = unsafe {
            core::slice::from_raw_parts_mut(buf.as_mut_ptr() as *mut u32, BLOCK_SIZE / 4)
        };
        let l1_idx = idx2 / (BLOCK_SIZE / 4);
        let l2_idx = idx2 % (BLOCK_SIZE / 4);
        if l1[l1_idx] == 0 {
            l1[l1_idx] = alloc_data_block(device, sb)?;
            write_block(device, inode.double_indirect, &buf)?;
        }
        let l1_phys = l1[l1_idx];
        let mut buf2 = [0u8; BLOCK_SIZE];
        // ARCH: [inode→block] CS-10 inode_alloc_block reads the double-indirect L2 block to
        //       allocate the final data block if the slot is empty.
        read_block(device, l1_phys, &mut buf2)?;
        let l2: &mut [u32] = unsafe {
            core::slice::from_raw_parts_mut(buf2.as_mut_ptr() as *mut u32, BLOCK_SIZE / 4)
        };
        if l2[l2_idx] == 0 {
            l2[l2_idx] = alloc_data_block(device, sb)?;
            write_block(device, l1_phys, &buf2)?;
        }
        return Ok(l2[l2_idx]);
    }
    Err(AbiError::Other("File too large"))
}

pub fn inode_set_block(
    device: &dyn BlockDevice,
    sb: &mut Superblock,
    inode: &mut Inode,
    logical: u32,
    new_phys: u32,
    new_checksum: u32,
) -> Result<(), AbiError> {
    if logical < 10 {
        inode.blocks[logical as usize] = new_phys;
        inode.block_checksums[logical as usize] = new_checksum;
        return Ok(());
    }
    if logical < 10 {
        inode.blocks[logical as usize] = new_phys;
        return Ok(());
    }
    let idx = (logical - 10) as usize;
    if idx < BLOCK_SIZE / 8 {
        if inode.indirect == 0 {
            inode.indirect = alloc_data_block(device, sb)?;
        }
        let mut buf = [0u8; BLOCK_SIZE];
        read_block(device, inode.indirect, &mut buf)?;
        let ptrs: &mut [(u32, u32)] = unsafe {
            core::slice::from_raw_parts_mut(buf.as_mut_ptr() as *mut (u32, u32), BLOCK_SIZE / 8)
        };
        ptrs[idx] = (new_phys, new_checksum);
        write_block(device, inode.indirect, &buf)?;
        inode.indirect_checksum = zig_kernel_ops::crc32(&buf);
        return Ok(());
    }
    let idx2 = idx - BLOCK_SIZE / 8;
    if idx2 < (BLOCK_SIZE / 8) * (BLOCK_SIZE / 8) {
        if inode.double_indirect == 0 {
            inode.double_indirect = alloc_data_block(device, sb)?;
        }
        let mut buf = [0u8; BLOCK_SIZE];
        read_block(device, inode.double_indirect, &mut buf)?;
        let l1: &mut [(u32, u32)] = unsafe {
            core::slice::from_raw_parts_mut(buf.as_mut_ptr() as *mut (u32, u32), BLOCK_SIZE / 8)
        };
        let l1_idx = idx2 / (BLOCK_SIZE / 8);
        let l2_idx = idx2 % (BLOCK_SIZE / 8);
        if l1[l1_idx].0 == 0 {
            l1[l1_idx].0 = alloc_data_block(device, sb)?;
        }
        let l1_phys = l1[l1_idx].0;
        let mut buf2 = [0u8; BLOCK_SIZE];
        read_block(device, l1_phys, &mut buf2)?;
        let l2: &mut [(u32, u32)] = unsafe {
            core::slice::from_raw_parts_mut(buf2.as_mut_ptr() as *mut (u32, u32), BLOCK_SIZE / 8)
        };
        l2[l2_idx] = (new_phys, new_checksum);
        write_block(device, l1_phys, &buf2)?;
        l1[l1_idx].1 = zig_kernel_ops::crc32(&buf2);
        write_block(device, inode.double_indirect, &buf)?;
        inode.double_indirect_checksum = zig_kernel_ops::crc32(&buf);
        return Ok(());
    }
    Err(AbiError::Other("File too large"))
}
pub fn free_inode_blocks(
    device: &dyn BlockDevice,
    sb: &mut Superblock,
    inode: &Inode,
) -> Result<(), AbiError> {
    for i in 0..10 {
        if inode.blocks[i] != 0 {
            free_data_block(device, sb, inode.blocks[i])?;
        }
    }
    if inode.indirect != 0 {
        let mut buf = [0u8; BLOCK_SIZE];
        read_block(device, inode.indirect, &mut buf)?;
        let ptrs: &[u32] =
            unsafe { core::slice::from_raw_parts(buf.as_ptr() as *const u32, BLOCK_SIZE / 4) };
        for &p in ptrs {
            if p != 0 {
                free_data_block(device, sb, p)?;
            }
        }
        free_data_block(device, sb, inode.indirect)?;
    }
    if inode.double_indirect != 0 {
        let mut buf = [0u8; BLOCK_SIZE];
        read_block(device, inode.double_indirect, &mut buf)?;
        let l1: &[u32] =
            unsafe { core::slice::from_raw_parts(buf.as_ptr() as *const u32, BLOCK_SIZE / 4) };
        for &l1p in l1 {
            if l1p != 0 {
                let mut buf2 = [0u8; BLOCK_SIZE];
                read_block(device, l1p, &mut buf2)?;
                let l2: &[u32] = unsafe {
                    core::slice::from_raw_parts(buf2.as_ptr() as *const u32, BLOCK_SIZE / 4)
                };
                for &p in l2 {
                    if p != 0 {
                        free_data_block(device, sb, p)?;
                    }
                }
                free_data_block(device, sb, l1p)?;
            }
        }
        free_data_block(device, sb, inode.double_indirect)?;
    }
    Ok(())
}
