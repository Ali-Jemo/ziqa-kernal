//! Write-ahead journal (WAL) for ZiqaFS.

use super::block::{read_block, write_block};
use super::types::*;
use crate::abi::AbiError;
use crate::drivers::block::BlockDevice;

pub fn journal_read(
    device: &dyn BlockDevice,
) -> Result<([u8; BLOCK_SIZE], JournalHeader), AbiError> {
    let mut buf = [0u8; BLOCK_SIZE];
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
        if entry.op == JournalOp::WriteInode as u8 {
            let inode_id = entry.block;
            if inode_id < INODE_COUNT {
                if let Ok(mut ib) = {
                    let mut b = [0u8; BLOCK_SIZE];
                    read_block(device, INODE_TABLE_BLOCK, &mut b).map(|_| b)
                } {
                    let o = inode_id as usize * 72;
                    ib[o..o + 56].copy_from_slice(&entry.data);
                    let _ = write_block(device, INODE_TABLE_BLOCK, &ib);
                    replayed += 1;
                }
            }
        } else if entry.op == JournalOp::WriteBitmap as u8 {
            if let Ok(mut bm) = {
                let mut b = [0u8; BLOCK_SIZE];
                read_block(device, BITMAP_BLOCK, &mut b).map(|_| b)
            } {
                bm[entry.block as usize] = entry.data[0];
                let _ = write_block(device, BITMAP_BLOCK, &bm);
                replayed += 1;
            }
        }
    }
    replayed
}
