//! Directory entry operations for ZiqaFS.

use super::block::{alloc_data_block, read_block, write_block};
use super::inode::{inode_alloc_block, inode_get_block, read_inode, write_inode};
use super::types::*;
use crate::abi::AbiError;
use crate::drivers::block::BlockDevice;

pub fn dirent_foreach<F>(buf: &[u8; BLOCK_SIZE], size: u32, mut cb: F)
where
    F: FnMut(u32, &str),
{
    let mut off = 0usize;
    while off + 8 <= (size as usize).min(BLOCK_SIZE) {
        let inode = u32::from_le_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]]);
        let name_len = u16::from_le_bytes([buf[off + 4], buf[off + 5]]) as usize;
        let entry_size = u16::from_le_bytes([buf[off + 6], buf[off + 7]]) as usize;
        if entry_size < 8 || entry_size > 260 || off + entry_size > BLOCK_SIZE {
            break;
        }
        if inode != 0 && name_len > 0 && name_len <= 252 {
            let ns = off + 8;
            if ns + name_len <= BLOCK_SIZE {
                if let Ok(name) = core::str::from_utf8(&buf[ns..ns + name_len]) {
                    cb(inode, name);
                }
            }
        }
        off += entry_size;
    }
}

pub fn find_entry(buf: &[u8; BLOCK_SIZE], size: u32, name: &str) -> Option<u32> {
    let mut result = None;
    dirent_foreach(buf, size, |inode, n| {
        if n == name {
            result = Some(inode);
        }
    });
    result
}

pub fn write_entry_raw(
    buf: &mut [u8; BLOCK_SIZE],
    target_inode: u32,
    name: &str,
) -> Result<(), AbiError> {
    let name_bytes = name.as_bytes();
    let name_len = name_bytes.len();
    if name_len > 252 {
        return Err(AbiError::Other("Name too long"));
    }
    let entry_size = (8 + name_len + 3) & !3;
    let mut off = 0usize;
    loop {
        if off + entry_size > BLOCK_SIZE {
            return Err(AbiError::Other("Directory block full"));
        }
        let existing = u32::from_le_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]]);
        if existing == 0 {
            break;
        }
        let es = u16::from_le_bytes([buf[off + 6], buf[off + 7]]) as usize;
        if es < 8 {
            break;
        }
        off += es;
    }
    buf[off..off + 4].copy_from_slice(&target_inode.to_le_bytes());
    buf[off + 4..off + 6].copy_from_slice(&(name_len as u16).to_le_bytes());
    buf[off + 6..off + 8].copy_from_slice(&(entry_size as u16).to_le_bytes());
    if name_len > 0 {
        buf[off + 8..off + 8 + name_len].copy_from_slice(name_bytes);
    }
    Ok(())
}

pub fn remove_entry_raw(buf: &mut [u8; BLOCK_SIZE], size: u32, target_inode: u32) -> bool {
    let mut off = 0usize;
    while off + 8 <= (size as usize).min(BLOCK_SIZE) {
        let inode = u32::from_le_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]]);
        let entry_size = u16::from_le_bytes([buf[off + 6], buf[off + 7]]) as usize;
        if entry_size < 8 || off + entry_size > BLOCK_SIZE {
            break;
        }
        if inode == target_inode {
            buf[off..off + 4].fill(0);
            return true;
        }
        off += entry_size;
    }
    false
}

pub fn lookup_in_dir(
    device: &dyn BlockDevice,
    dir_inode: &Inode,
    name: &str,
) -> Result<Option<u32>, AbiError> {
    let total_blocks = (dir_inode.size as usize + BLOCK_SIZE - 1) / BLOCK_SIZE;
    for logical in 0..total_blocks as u32 {
        let phys = inode_get_block(device, dir_inode, logical)?;
        if phys == 0 {
            continue;
        }
        let mut buf = [0u8; BLOCK_SIZE];
        read_block(device, phys, &mut buf)?;
        if let Some(id) = find_entry(&buf, BLOCK_SIZE as u32, name) {
            return Ok(Some(id));
        }
    }
    Ok(None)
}

pub fn dir_add_entry(
    device: &dyn BlockDevice,
    sb: &mut Superblock,
    parent_inode: u32,
    new_id: u32,
    name: &str,
) -> Result<(), AbiError> {
    let mut parent = read_inode(device, parent_inode)?;
    let total_dir_blocks = (parent.size as usize + BLOCK_SIZE - 1) / BLOCK_SIZE;
    for logical in 0..total_dir_blocks as u32 {
        let phys = inode_get_block(device, &parent, logical)?;
        if phys == 0 {
            continue;
        }
        let mut buf = [0u8; BLOCK_SIZE];
        read_block(device, phys, &mut buf)?;
        if write_entry_raw(&mut buf, new_id, name).is_ok() {
            return write_block(device, phys, &buf);
        }
    }
    let new_logical = total_dir_blocks as u32;
    let new_phys = inode_alloc_block(device, sb, &mut parent, new_logical)?;
    parent.size = (new_logical + 1) * BLOCK_SIZE as u32;
    write_inode(device, parent_inode, &parent)?;
    let mut buf = [0u8; BLOCK_SIZE];
    write_entry_raw(&mut buf, new_id, name)?;
    write_block(device, new_phys, &buf)
}

pub fn init_dir_block(
    device: &dyn BlockDevice,
    sb: &mut Superblock,
    new_id: u32,
    parent_inode_id: u32,
) -> Result<u32, AbiError> {
    let new_data = alloc_data_block(device, sb)?;
    let mut new_inode = read_inode(device, new_id)?;
    new_inode.blocks[0] = new_data;
    new_inode.size = BLOCK_SIZE as u32;
    let now = crate::timer::TIMER.lock().uptime_secs() as u32;
    new_inode.mtime = now;
    new_inode.ctime = now;
    new_inode.atime = now;
    write_inode(device, new_id, &new_inode)?;
    let mut dir_buf = [0u8; BLOCK_SIZE];
    let _ = write_entry_raw(&mut dir_buf, new_id, ".");
    let _ = write_entry_raw(&mut dir_buf, parent_inode_id, "..");
    write_block(device, new_data, &dir_buf)?;
    Ok(new_data)
}
