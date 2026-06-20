//! File read, write, truncate, copy, and disk-usage for ZiqaFS.

use super::block::{read_block, write_block};
use super::dir::{dir_add_entry, lookup_in_dir};
use super::inode::{alloc_inode, free_inode_blocks, inode_get_block, read_inode, write_inode};
use super::types::*;
use crate::abi::AbiError;
use crate::drivers::block::BlockDevice;
use alloc::string::String;
use alloc::vec::Vec;

fn flush_superblock(device: &dyn BlockDevice, sb: &Superblock) -> Result<(), AbiError> {
    let mut buf = alloc::boxed::Box::new([0u8; BLOCK_SIZE]);
    unsafe {
        let bytes = core::slice::from_raw_parts(
            sb as *const Superblock as *const u8,
            core::mem::size_of::<Superblock>(),
        );
        buf[..bytes.len()].copy_from_slice(bytes);
    }
    super::block::write_blocks(device, SUPERBLOCK_BLOCK, 1, &*buf)
}

pub fn read_file(
    device: &dyn BlockDevice,
    inode_id: u32,
    offset: usize,
    buf: &mut [u8],
) -> Result<usize, AbiError> {
    let inode = read_inode(device, inode_id)?;
    if offset >= inode.size as usize {
        return Ok(0);
    }
    let to_read = (inode.size as usize - offset).min(buf.len());
    let mut done = 0;
    while done < to_read {
        let logical = (offset + done) as u32 / BLOCK_SIZE as u32;
        let block_off = (offset + done) % BLOCK_SIZE;
        let (phys, _cs) = inode_get_block(device, &inode, logical)?;
        if phys == 0 {
            break;
        }
        let mut block_buf = [0u8; BLOCK_SIZE];
        // ARCH: [file→block] CS-11 read_file reads a data block resolved via inode_get_block.
        //       Copies the relevant slice into the caller's buffer; no block number is hardcoded.
        read_block(device, phys, &mut block_buf)?;
        let chunk = (to_read - done).min(BLOCK_SIZE - block_off);
        buf[done..done + chunk].copy_from_slice(&block_buf[block_off..block_off + chunk]);
        done += chunk;
    }
    Ok(done)
}

pub fn write_file(
    device: &dyn BlockDevice,
    sb: &mut Superblock,
    inode_id: u32,
    offset: usize,
    buf: &[u8],
) -> Result<usize, AbiError> {
    let mut inode = read_inode(device, inode_id)?;
    if inode.mode == INODE_MODE_DIR {
        return Err(AbiError::Other("Cannot write to directory"));
    }
    let mut done = 0;
    while done < buf.len() {
        let logical = (offset + done) as u32 / BLOCK_SIZE as u32;
        let block_off = (offset + done) % BLOCK_SIZE;
        let (old_phys, _old_cs) = super::inode::inode_get_block(device, &inode, logical)?;
        let mut block_buf = [0u8; BLOCK_SIZE];
        if old_phys != 0 {
            if block_off > 0 || done + BLOCK_SIZE > buf.len() {
            read_block(device, old_phys, &mut block_buf)?;
            }
        }
        let chunk = (BLOCK_SIZE - block_off).min(buf.len() - done);
        block_buf[block_off..block_off + chunk].copy_from_slice(&buf[done..done + chunk]);
        
        let new_phys = super::block::alloc_data_block(device, sb)?;
        write_block(device, new_phys, &block_buf)?;
        super::inode::inode_set_block(device, sb, &mut inode, logical, new_phys, 0)?;
        
        if old_phys != 0 && old_phys != new_phys {
            super::block::free_data_block(device, sb, old_phys)?;
        }
        done += chunk;
    }
    let new_size = (offset + buf.len()).max(inode.size as usize);
    inode.size = new_size as u32;
    inode.mtime = crate::timer::TIMER.lock().uptime_secs() as u32;
    write_inode(device, inode_id, &inode)?;
    flush_superblock(device, sb)?;
    Ok(buf.len())
}

pub fn truncate(
    device: &dyn BlockDevice,
    sb: &mut Superblock,
    inode_id: u32,
    new_size: usize,
) -> Result<(), AbiError> {
    let mut inode = read_inode(device, inode_id)?;
    if inode.mode == INODE_MODE_DIR {
        return Err(AbiError::Other("Cannot truncate directory"));
    }
    let old_blocks = (inode.size as usize + BLOCK_SIZE - 1) / BLOCK_SIZE;
    let new_blocks = (new_size + BLOCK_SIZE - 1) / BLOCK_SIZE;
    for logical in (new_blocks as u32)..(old_blocks as u32) {
        let (phys, _cs) = inode_get_block(device, &inode, logical)?;
        if phys != 0 {
            let mut block_buf = [0u8; BLOCK_SIZE];
            read_block(device, phys, &mut block_buf)?;
            if logical < 10 {
                write_block(device, phys, &block_buf)?;
            } else {
                let idx = (logical - 10) as usize;
                if idx < BLOCK_SIZE / 4 && inode.indirect != 0 {
                    let mut buf = [0u8; BLOCK_SIZE];
                    // ARCH: [file→block] CS-13 truncate reads the indirect pointer block to
                    //       zero out the freed slot. Read-modify-write; preserves other pointers.
                    read_block(device, inode.indirect, &mut buf)?;
                    let ptrs: &mut [u32] = unsafe {
                        core::slice::from_raw_parts_mut(
                            buf.as_mut_ptr() as *mut u32,
                            BLOCK_SIZE / 4,
                        )
                    };
                    ptrs[idx] = 0;
                    write_block(device, inode.indirect, &buf)?;
                }
            }
        }
    }
    if new_size > 0 && new_size % BLOCK_SIZE != 0 {
        let last_logical = (new_size - 1) as u32 / BLOCK_SIZE as u32;
        let (phys, _cs) = inode_get_block(device, &inode, last_logical)?;
        if phys != 0 {
            let mut block_buf = [0u8; BLOCK_SIZE];
            // ARCH: [file→block] CS-14 truncate reads the last data block to zero the tail
            //       bytes beyond new_size. Ensures no stale data is visible after shrink.
            read_block(device, phys, &mut block_buf)?;
            let tail_start = new_size % BLOCK_SIZE;
            block_buf[tail_start..].fill(0);
            write_block(device, phys, &block_buf)?;
        }
    }
    inode.size = new_size as u32;
    inode.mtime = crate::timer::TIMER.lock().uptime_secs() as u32;
    write_inode(device, inode_id, &inode)?;
    flush_superblock(device, sb)?;
    Ok(())
}

pub fn create_file(
    device: &dyn BlockDevice,
    sb: &mut Superblock,
    parent_inode: u32,
    name: &str,
) -> Result<u32, AbiError> {
    if name.is_empty() || name.len() > 252 {
        return Err(AbiError::Other("Invalid name"));
    }
    let parent = read_inode(device, parent_inode)?;
    if parent.mode != INODE_MODE_DIR {
        return Err(AbiError::Other("Not a directory"));
    }
    if lookup_in_dir(device, &parent, name)?.is_some() {
        return Err(AbiError::Other("File exists"));
    }
    let new_id = alloc_inode(device, sb, INODE_MODE_FILE)?;
    dir_add_entry(device, sb, parent_inode, new_id, name)?;
    flush_superblock(device, sb)?;
    Ok(new_id)
}

pub fn create_dir(
    device: &dyn BlockDevice,
    sb: &mut Superblock,
    parent_inode: u32,
    name: &str,
) -> Result<u32, AbiError> {
    if name.is_empty() || name.len() > 252 {
        return Err(AbiError::Other("Invalid name"));
    }
    let parent = read_inode(device, parent_inode)?;
    if parent.mode != INODE_MODE_DIR {
        return Err(AbiError::Other("Not a directory"));
    }
    if lookup_in_dir(device, &parent, name)?.is_some() {
        return Err(AbiError::Other("File exists"));
    }
    let new_id = alloc_inode(device, sb, INODE_MODE_DIR)?;
    super::dir::init_dir_block(device, sb, new_id, parent_inode)?;
    dir_add_entry(device, sb, parent_inode, new_id, name)?;
    flush_superblock(device, sb)?;
    Ok(new_id)
}

pub fn unlink(
    device: &dyn BlockDevice,
    sb: &mut Superblock,
    parent_inode: u32,
    name: &str,
) -> Result<(), AbiError> {
    let parent = read_inode(device, parent_inode)?;
    let total_dir_blocks = (parent.size as usize + BLOCK_SIZE - 1) / BLOCK_SIZE;
    let mut target_id = None;
    let mut found_phys = 0u32;
    let mut last_phys = 0u32;
    let mut last_buf = [0u8; BLOCK_SIZE];
    for logical in 0..total_dir_blocks as u32 {
        let (phys, _cs) = inode_get_block(device, &parent, logical)?;
        if phys == 0 {
            continue;
        }
        
        if phys != last_phys {
            read_block(device, phys, &mut last_buf)?;
            last_phys = phys;
        }
        
        if let Some(id) = super::dir::find_entry(&last_buf, BLOCK_SIZE as u32, name) {
            target_id = Some(id);
            found_phys = phys;
        }
    }
    let target_id = target_id.ok_or(AbiError::Other("File not found"))?;
    let mut target_inode = read_inode(device, target_id)?;
    target_inode.nlink = target_inode.nlink.saturating_sub(1);
    target_inode.ctime = crate::timer::TIMER.lock().uptime_secs() as u32;

    // Use last_buf if found_phys == last_phys
    if found_phys != last_phys {
        read_block(device, found_phys, &mut last_buf)?;
    }
    
    super::dir::remove_entry_raw(&mut last_buf, BLOCK_SIZE as u32, target_id);
    write_block(device, found_phys, &last_buf)?;
    if target_inode.nlink == 0 {
        free_inode_blocks(device, sb, &target_inode)?;
        super::inode::free_inode(device, sb, target_id)?;
    } else {
        write_inode(device, target_id, &target_inode)?;
    }
    flush_superblock(device, sb)?;
    Ok(())
}

pub fn rename(
    device: &dyn BlockDevice,
    sb: &mut Superblock,
    src_parent: u32,
    name: &str,
    dst_parent: u32,
    new_name: &str,
) -> Result<(), AbiError> {
    if new_name.is_empty() || new_name.len() > 252 {
        return Err(AbiError::Other("Invalid name"));
    }
    let src_inode_obj = read_inode(device, src_parent)?;
    let target_id = lookup_in_dir(device, &src_inode_obj, name)?
        .ok_or(AbiError::Other("File not found"))?;
    let total_blocks = (src_inode_obj.size as usize + BLOCK_SIZE - 1) / BLOCK_SIZE;
    for logical in 0..total_blocks as u32 {
        let phys = inode_get_block(device, &src_inode_obj, logical)?;
        if phys.0 == 0 {
            continue;
        }
        let mut buf = [0u8; BLOCK_SIZE];
        // ARCH: [file→block] CS-17 rename scans source-parent directory blocks to find and
        //       remove the old entry before inserting it under the new name/parent.
        read_block(device, phys.0, &mut buf)?;
        if super::dir::remove_entry_raw(&mut buf, BLOCK_SIZE as u32, target_id) {
        write_block(device, phys.0, &buf)?;
            break;
        }
    }
    dir_add_entry(device, sb, dst_parent, target_id, new_name)?;
    Ok(())
}

pub fn link(
    device: &dyn BlockDevice,
    sb: &mut Superblock,
    src_inode_id: u32,
    dst_parent: u32,
    name: &str,
) -> Result<(), AbiError> {
    if name.is_empty() || name.len() > 252 {
        return Err(AbiError::Other("Invalid name"));
    }
    let mut inode = read_inode(device, src_inode_id)?;
    if inode.mode == INODE_MODE_DIR {
        return Err(AbiError::Other("Cannot hard-link a directory"));
    }
    let dst_inode_obj = read_inode(device, dst_parent)?;
    if lookup_in_dir(device, &dst_inode_obj, name)?.is_some() {
        return Err(AbiError::Other("File exists"));
    }
    inode.nlink += 1;
    inode.ctime = crate::timer::TIMER.lock().uptime_secs() as u32;
    let inode_bytes =
        unsafe { core::slice::from_raw_parts(&inode as *const Inode as *const u8, 56) };
    let _ = super::journal::journal_commit(device, JournalOp::WriteInode, src_inode_id, inode_bytes);
    write_inode(device, src_inode_id, &inode)?;
    dir_add_entry(device, sb, dst_parent, src_inode_id, name)?;
    Ok(())
}

pub fn read_dir(
    device: &dyn BlockDevice,
    inode_id: u32,
) -> Result<Vec<(String, u32)>, AbiError> {
    let inode = read_inode(device, inode_id)?;
    if inode.mode != INODE_MODE_DIR {
        return Err(AbiError::Other("Not a directory"));
    }
    let total_blocks = (inode.size as usize + BLOCK_SIZE - 1) / BLOCK_SIZE;
    let mut entries = Vec::new();
    for logical in 0..total_blocks as u32 {
        let phys = inode_get_block(device, &inode, logical)?;
        if phys.0 == 0 {
            continue;
        }
        let mut buf = [0u8; BLOCK_SIZE];
        read_block(device, phys.0, &mut buf)?;
        super::dir::dirent_foreach(&buf, BLOCK_SIZE as u32, |eid, ename| {
            if ename != "." && ename != ".." {
                entries.push((String::from(ename), eid));
            }
        });
    }
    Ok(entries)
}

pub fn root_lookup(
    device: &dyn BlockDevice,
    path: &str,
) -> Result<u32, AbiError> {
    let name = path.trim_start_matches('/').trim_end_matches('/');
    if name.is_empty() {
        return Err(AbiError::Other("Empty path"));
    }
    let mut current_inode_id = ROOT_INODE;
    for part in name.split('/') {
        let inode = read_inode(device, current_inode_id)?;
        if inode.mode != INODE_MODE_DIR {
            return Err(AbiError::Other("Not a directory"));
        }
        current_inode_id = lookup_in_dir(device, &inode, part)?
            .ok_or(AbiError::Other("File not found"))?;
    }
    Ok(current_inode_id)
}

pub fn copy_file(
    device: &dyn BlockDevice,
    sb: &mut Superblock,
    src_inode_id: u32,
    dst_parent: u32,
    name: &str,
) -> Result<u32, AbiError> {
    let src_inode = read_inode(device, src_inode_id)?;
    if src_inode.mode == INODE_MODE_DIR {
        return Err(AbiError::Other("Cannot copy directory"));
    }
    let new_id = create_file(device, sb, dst_parent, name)?;
    let size = src_inode.size as usize;
    let total_logical = (size + BLOCK_SIZE - 1) / BLOCK_SIZE;
    for logical in 0..total_logical as u32 {
        let src_phys = inode_get_block(device, &src_inode, logical)?;
        if src_phys.0 == 0 {
            continue;
        }
        let mut block_buf = [0u8; BLOCK_SIZE];
        // ARCH: [file→block] CS-18 copy_file reads each source data block and writes it to
        //       the new inode via write_file. Block-by-block copy; no shared buffer aliasing.
        read_block(device, src_phys.0, &mut block_buf)?;
        let write_len =
            if logical as usize == total_logical.saturating_sub(1) && size % BLOCK_SIZE != 0 {
                size % BLOCK_SIZE
            } else {
                BLOCK_SIZE
            };
        write_file(device, sb, new_id, logical as usize * BLOCK_SIZE, &block_buf[..write_len])?;
    }
    Ok(new_id)
}

pub fn du(device: &dyn BlockDevice, inode_id: u32) -> u32 {
    let (mode, size) = match read_inode(device, inode_id) {
        Ok(i) => (i.mode, i.size),
        Err(_) => return 0,
    };
    let own = ((size as usize + BLOCK_SIZE - 1) / BLOCK_SIZE) as u32;
    if mode != INODE_MODE_DIR {
        return own;
    }
    let children = read_dir(device, inode_id).unwrap_or_default();
    own + children.iter().map(|(_, cid)| du(device, *cid)).sum::<u32>()
}
