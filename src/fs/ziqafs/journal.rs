//! Write-ahead journal (WAL) for ZiqaFS.

use super::block::{patch_bitmap_byte, read_block, write_block};
use super::inode::patch_inode_raw;
use super::types::*;
use crate::abi::AbiError;
use crate::drivers::block::BlockDevice;
use crate::zig_kernel_ops;

pub fn journal_read(
    device: &dyn BlockDevice,
) -> Result<([u8; BLOCK_SIZE], JournalHeader), AbiError> {
    let mut buf = [0u8; BLOCK_SIZE];
    // ARCH: [journal→block] CS-21 journal_read reads the fixed JOURNAL_BLOCK (block 4).
    //       All journal operations start here; the header and entry ring are in this block.
    read_block(device, JOURNAL_BLOCK, &mut buf)?;
    let hdr: JournalHeader =
        unsafe { core::ptr::read_unaligned(buf.as_ptr() as *const JournalHeader) };
    Ok((buf, hdr))
}

pub fn journal_commit(
    device: &dyn BlockDevice,
    op: JournalOp,
    block: u32,
    data: &[u8],
) -> Result<(), AbiError> {
    let (mut buf, mut hdr) = journal_read(device)?;
    if hdr.magic != JOURNAL_MAGIC {
        return Ok(());
    }
    let slot = hdr.head as usize % JOURNAL_ENTRIES;
    let entry_off = core::mem::size_of::<JournalHeader>() + slot * JOURNAL_ENTRY_SIZE;
    let mut entry = JournalEntry {
        op: op as u8,
        _pad: [0; 3],
        block,
        data: [0u8; 56],
    };
    let n = data.len().min(56);
    entry.data[..n].copy_from_slice(&data[..n]);
    // Stamp CRC-32 of the entry payload into the last 4 bytes of the data field.
    let checksum = zig_kernel_ops::crc32(&entry.data[..52]);
    entry.data[52..56].copy_from_slice(&checksum.to_le_bytes());
    unsafe {
        core::ptr::copy_nonoverlapping(
            &entry as *const JournalEntry as *const u8,
            buf[entry_off..].as_mut_ptr(),
            JOURNAL_ENTRY_SIZE,
        );
    }
    hdr.head = (hdr.head + 1) % JOURNAL_ENTRIES as u32;
    hdr.committed += 1;
    unsafe {
        core::ptr::copy_nonoverlapping(
            &hdr as *const JournalHeader as *const u8,
            buf.as_mut_ptr(),
            core::mem::size_of::<JournalHeader>(),
        );
    }
    write_block(device, JOURNAL_BLOCK, &buf)
}

/// Replay uncommitted journal entries on mount for crash recovery.
pub fn journal_replay(device: &dyn BlockDevice) -> u32 {
    let (buf, hdr) = match journal_read(device) {
        Ok(v) => v,
        Err(_) => return 0,
    };
    if hdr.magic != JOURNAL_MAGIC || hdr.committed == 0 {
        return 0;
    }
    let mut replayed = 0u32;
    for i in 0..JOURNAL_ENTRIES {
        let off = core::mem::size_of::<JournalHeader>() + i * JOURNAL_ENTRY_SIZE;
        let entry: JournalEntry =
            unsafe { core::ptr::read_unaligned(buf[off..].as_ptr() as *const JournalEntry) };
        
        let mut csum_bytes = [0u8; 4];
        csum_bytes.copy_from_slice(&entry.data[52..56]);
        let stored_crc = u32::from_le_bytes(csum_bytes);
        let computed_crc = zig_kernel_ops::crc32(&entry.data[..52]);
        
        if stored_crc != computed_crc {
            continue;
        }
        // ARCH: [journal→inode] CS-22 journal_replay patches inode directly via
        //       inode::patch_inode_raw helper. Crash-recovery path.
        if entry.op == JournalOp::WriteInode as u8 {
            let inode_id = entry.block;
            if inode_id < INODE_COUNT {
                // ARCH: [journal→inode] CS-22 journal_replay patches inode directly via
                //       inode::patch_inode_raw helper. Crash-recovery path.
                if patch_inode_raw(device, inode_id, 0, &entry.data).is_ok() {
                    replayed += 1;
                }
            }
        } else if entry.op == JournalOp::WriteBitmap as u8 {
        // ARCH: [journal→block] CS-23 journal_replay patches bitmap directly via
        //       block::patch_bitmap_byte helper. Crash-recovery path.
        // The journal now relies on these specialized helpers instead of constructing
        // raw read/write_block calls for structures it does not own, improving
        // encapsulation and maintainability.
            if patch_bitmap_byte(device, entry.block as usize, entry.data[0]).is_ok() {
                replayed += 1;
            }
        }
    }
    replayed
}
