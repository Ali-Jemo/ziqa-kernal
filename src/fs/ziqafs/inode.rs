//! Inode allocation, read/write, and block mapping for ZiqaFS.

use super::block::{alloc_data_block, free_data_block, read_block, write_block};
use super::types::*;
use crate::abi::AbiError;
use crate::drivers::block::BlockDevice;

pub fn read_inode(device: &dyn BlockDevice, inode_id: u32) -> Result<Inode, AbiError> {
    if inode_id >= INODE_COUNT {
        return Err(AbiError::OutOfBounds);
    }
    let mut buf = [0u8; BLOCK_SIZE];
    read_block(device, INODE_TABLE_BLOCK, &mut buf)?;
    let offset = inode_id as usize * 72;
    Ok(unsafe { core::ptr::read_unaligned(buf[offset..].as_ptr() as *const Inode) })
}

pub fn write_inode(device: &dyn BlockDevice, inode_id: u32, inode: &Inode) -> Result<(), AbiError> {
    if inode_id >= INODE_COUNT {
        return Err(AbiError::OutOfBounds);
    }
    let mut buf = [0u8; BLOCK_SIZE];
    read_block(device, INODE_TABLE_BLOCK, &mut buf)?;
    let offset = inode_id as usize * 72;
    unsafe {
        core::ptr::copy_nonoverlapping(
            inode as *const Inode as *const u8,
            buf[offset..].as_mut_ptr(),
            72,
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
    read_block(device, INODE_TABLE_BLOCK, &mut buf)?;
    for i in 1..INODE_COUNT {
        let offset = i as usize * 72;
        let mode_val: u16 =
            unsafe { core::ptr::read_unaligned(buf[offset..].as_ptr() as *const u16) };
        if mode_val == 0 {
            let now = crate::timer::TIMER.lock().uptime_secs() as u32;
            let inode = Inode {
                mode,
                nlink: 1,
                uid: 0,
                gid: 0,
                size: 0,
                mtime: now,
                ctime: now,
                atime: now,
                blocks: [0u32; 10],
                indirect: 0,
                double_indirect: 0,
            };
            unsafe {
                core::ptr::copy_nonoverlapping(
                    &inode as *const Inode as *const u8,
                    buf[offset..].as_mut_ptr(),
                    72,
                );
            }
            write_block(device, INODE_TABLE_BLOCK, &buf)?;
            sb.free_inodes = sb.free_inodes.saturating_sub(1);
            return Ok(i);
        }
    }
    Err(AbiError::Other("No free inodes"))
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
    read_block(device, INODE_TABLE_BLOCK, &mut buf)?;
    let offset = inode_id as usize * 72;
    buf[offset..offset + 72].fill(0);
    write_block(device, INODE_TABLE_BLOCK, &buf)?;
    sb.free_inodes = sb.free_inodes.saturating_add(1);
    Ok(())
}

pub fn inode_get_block(
    device: &dyn BlockDevice,
    inode: &Inode,
    logical: u32,
) -> Result<u32, AbiError> {
    if logical < 10 {
        return Ok(inode.blocks[logical as usize]);
    }
    let idx = (logical - 10) as usize;
    if idx < BLOCK_SIZE / 4 {
        if inode.indirect == 0 {
            return Ok(0);
        }
        let mut buf = [0u8; BLOCK_SIZE];
        read_block(device, inode.indirect, &mut buf)?;
        let ptrs: &[u32] =
            unsafe { core::slice::from_raw_parts(buf.as_ptr() as *const u32, BLOCK_SIZE / 4) };
        return Ok(ptrs[idx]);
    }
    let idx2 = idx - BLOCK_SIZE / 4;
    if idx2 < (BLOCK_SIZE / 4) * (BLOCK_SIZE / 4) {
        if inode.double_indirect == 0 {
            return Ok(0);
        }
        let mut buf = [0u8; BLOCK_SIZE];
        read_block(device, inode.double_indirect, &mut buf)?;
        let l1: &[u32] =
            unsafe { core::slice::from_raw_parts(buf.as_ptr() as *const u32, BLOCK_SIZE / 4) };
        let l1_idx = idx2 / (BLOCK_SIZE / 4);
        let l2_idx = idx2 % (BLOCK_SIZE / 4);
        if l1[l1_idx] == 0 {
            return Ok(0);
        }
        let mut buf2 = [0u8; BLOCK_SIZE];
        read_block(device, l1[l1_idx], &mut buf2)?;
        let l2: &[u32] =
            unsafe { core::slice::from_raw_parts(buf2.as_ptr() as *const u32, BLOCK_SIZE / 4) };
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
