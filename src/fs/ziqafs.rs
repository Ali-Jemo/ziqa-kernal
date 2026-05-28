use crate::drivers::block::BlockDevice;
use crate::abi::AbiError;
use crate::fs::{File, FileType};
use alloc::sync::Arc;
use alloc::vec::Vec;
use alloc::string::String;
use lazy_static::lazy_static;
use spin::Mutex;

pub const BLOCK_SIZE: usize = 4096;
const SECTOR_SIZE: usize = 512;
const SECTORS_PER_BLOCK: usize = BLOCK_SIZE / SECTOR_SIZE;
const MAGIC: u32 = 0x21514146;

pub const SUPERBLOCK_BLOCK: u32 = 1;
pub const BITMAP_BLOCK: u32 = 2;
pub const INODE_TABLE_BLOCK: u32 = 3;
pub const JOURNAL_BLOCK: u32 = 4;   // circular WAL
pub const FIRST_DATA_BLOCK: u32 = 5;

const INODE_COUNT: u32 = 64;
pub const ROOT_INODE: u32 = 1;

const INODE_MODE_FILE: u16 = 0o100000;
const INODE_MODE_DIR:  u16 = 0o040000;

const BMAP_CLEAN: u32 = 0x0000_0000;
const BMAP_DIRTY: u32 = 0x0000_0001;

// ── On-disk structures ────────────────────────────────────────────────────────

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Superblock {
    pub magic:        u32,
    pub total_blocks: u32,
    pub inode_count:  u32,
    pub first_data_block: u32,
    pub block_size:   u32,
    pub flags:        u32,
    /// Number of free data blocks (kept in sync by alloc/free helpers)
    pub free_blocks:  u32,
    /// Number of free inodes (kept in sync by alloc/free helpers)
    pub free_inodes:  u32,
    pub reserved: [u8; 20],
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct Inode {
    pub mode:   u16,
    pub nlink:  u16,  // hard link count
    pub uid:    u16,
    pub gid:    u16,
    pub size:   u32,
    pub mtime:  u32,  // last modification (uptime_secs)
    pub ctime:  u32,  // creation time
    pub atime:  u32,  // last access time
    pub blocks: [u32; 10],
    pub indirect: u32,
    pub double_indirect: u32,
}
const _: () = assert!(core::mem::size_of::<Inode>() == 64);

// ── Statfs result ─────────────────────────────────────────────────────────────

pub struct StatFs {
    pub total_blocks: u32,
    pub free_blocks:  u32,
    pub total_inodes: u32,
    pub free_inodes:  u32,
    pub block_size:   u32,
}

// ── Fsck result ───────────────────────────────────────────────────────────────

pub struct FsckResult {
    pub ok:              bool,
    pub errors:          u32,
    pub leaked_blocks:   u32,
    pub leaked_inodes:   u32,
}

// ── Journal (WAL) ─────────────────────────────────────────────────────────────
// One journal block holds up to 63 entries (each 64 bytes) + a 64-byte header.
// Layout: [JournalHeader(64)] [JournalEntry(64); 63]

const JOURNAL_ENTRIES: usize = 63;
const JOURNAL_ENTRY_SIZE: usize = 64;
const JOURNAL_MAGIC: u32 = 0x4A524E4C; // "JRNL"

#[repr(u8)]
#[derive(Clone, Copy, PartialEq)]
enum JournalOp {
    Free    = 0,
    AllocBlock  = 1,
    FreeBlock   = 2,
    WriteInode  = 3,
    WriteBitmap = 4,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct JournalHeader {
    magic:  u32,
    head:   u32,  // next write slot (0..JOURNAL_ENTRIES)
    committed: u32,
    _pad:   [u8; 52],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct JournalEntry {
    op:      u8,
    _pad:    [u8; 3],
    block:   u32,  // target block number
    data:    [u8; 56], // payload (inode bytes or bitmap diff)
}

// ── Main struct ───────────────────────────────────────────────────────────────

pub struct ZiqaFs {
    pub device: Arc<dyn BlockDevice>,
    pub sb: Superblock,
}

impl ZiqaFs {
    // ── Mount / Format ────────────────────────────────────────────────────────

    pub fn mount(device: Arc<dyn BlockDevice>) -> Result<Arc<spin::Mutex<Self>>, AbiError> {
        let mut sb_buf = [0u8; BLOCK_SIZE];
        Self::read_blocks(&*device, SUPERBLOCK_BLOCK, 1, &mut sb_buf)?;
        let sb: Superblock = unsafe { core::ptr::read_unaligned(sb_buf.as_ptr() as *const Superblock) };
        if sb.magic != MAGIC {
            return Err(AbiError::Other("Not a ZiqaFS filesystem"));
        }
        if sb.block_size != BLOCK_SIZE as u32 {
            return Err(AbiError::Other("Block size mismatch"));
        }
        Ok(Arc::new(spin::Mutex::new(Self { device, sb })))
    }

    pub fn format(device: Arc<dyn BlockDevice>) -> Result<Arc<spin::Mutex<Self>>, AbiError> {
        let mut buf = [0u8; BLOCK_SIZE];
        let total_blocks = (device.total_sectors() * SECTOR_SIZE as u64 / BLOCK_SIZE as u64) as u32;
        let data_blocks = total_blocks.saturating_sub(FIRST_DATA_BLOCK);
        let sb = Superblock {
            magic: MAGIC,
            total_blocks,
            inode_count: INODE_COUNT,
            first_data_block: FIRST_DATA_BLOCK,
            block_size: BLOCK_SIZE as u32,
            flags: BMAP_DIRTY,
            free_blocks: data_blocks,
            free_inodes: INODE_COUNT - 1, // inode 0 reserved
            reserved: [0u8; 20],
        };
        unsafe {
            let bytes = core::slice::from_raw_parts(&sb as *const Superblock as *const u8, core::mem::size_of::<Superblock>());
            buf[..bytes.len()].copy_from_slice(bytes);
        }
        Self::write_blocks(&*device, SUPERBLOCK_BLOCK, 1, &buf)?;

        buf.fill(0);
        for i in 0..FIRST_DATA_BLOCK {
            Self::set_bitmap_bit(&mut buf, i as usize);
        }
        Self::write_blocks(&*device, BITMAP_BLOCK, 1, &buf)?;

        // Initialise journal header
        buf.fill(0);
        let jh = JournalHeader { magic: JOURNAL_MAGIC, head: 0, committed: 0, _pad: [0u8; 52] };
        unsafe {
            core::ptr::copy_nonoverlapping(&jh as *const JournalHeader as *const u8, buf.as_mut_ptr(), core::mem::size_of::<JournalHeader>());
        }
        Self::write_blocks(&*device, JOURNAL_BLOCK, 1, &buf)?;

        buf.fill(0);
        Self::write_blocks(&*device, INODE_TABLE_BLOCK, 1, &buf)?;

        Self::create_root_dir(&*device, &sb)?;

        let mut fs = Self { device, sb };
        fs.sb.flags = BMAP_CLEAN;
        Self::flush_superblock_raw(&*fs.device, &fs.sb)?;
        Ok(Arc::new(spin::Mutex::new(fs)))
    }

    // ── Superblock flush ──────────────────────────────────────────────────────

    fn flush_superblock_raw(device: &dyn BlockDevice, sb: &Superblock) -> Result<(), AbiError> {
        let mut buf = [0u8; BLOCK_SIZE];
        unsafe {
            let bytes = core::slice::from_raw_parts(sb as *const Superblock as *const u8, core::mem::size_of::<Superblock>());
            buf[..bytes.len()].copy_from_slice(bytes);
        }
        Self::write_blocks(device, SUPERBLOCK_BLOCK, 1, &buf)
    }

    pub fn flush_superblock(&mut self) -> Result<(), AbiError> {
        Self::flush_superblock_raw(&*self.device, &self.sb)
    }

    // ── Block I/O ─────────────────────────────────────────────────────────────

    fn read_blocks(device: &dyn BlockDevice, block: u32, count: u32, buf: &mut [u8]) -> Result<(), AbiError> {
        let sector = block as u64 * SECTORS_PER_BLOCK as u64;
        let n = count as u64 * SECTORS_PER_BLOCK as u64;
        if buf.len() < n as usize * SECTOR_SIZE { return Err(AbiError::OutOfBounds); }
        device.read_sectors(sector, n as u32, buf)
    }

    fn write_blocks(device: &dyn BlockDevice, block: u32, count: u32, buf: &[u8]) -> Result<(), AbiError> {
        let sector = block as u64 * SECTORS_PER_BLOCK as u64;
        let n = count as u64 * SECTORS_PER_BLOCK as u64;
        if buf.len() < n as usize * SECTOR_SIZE { return Err(AbiError::OutOfBounds); }
        device.write_sectors(sector, n as u32, buf)
    }

    fn read_block(device: &dyn BlockDevice, block: u32, buf: &mut [u8; BLOCK_SIZE]) -> Result<(), AbiError> {
        Self::read_blocks(device, block, 1, buf)
    }

    fn write_block(device: &dyn BlockDevice, block: u32, buf: &[u8; BLOCK_SIZE]) -> Result<(), AbiError> {
        Self::write_blocks(device, block, 1, buf)
    }

    // ── Bitmap helpers ────────────────────────────────────────────────────────

    fn set_bitmap_bit(buf: &mut [u8; BLOCK_SIZE], block: usize) {
        buf[block / 8] |= 1u8 << (block % 8);
    }

    fn clear_bitmap_bit(buf: &mut [u8; BLOCK_SIZE], block: usize) {
        buf[block / 8] &= !(1u8 << (block % 8));
    }

    fn test_bitmap_bit(buf: &[u8; BLOCK_SIZE], block: usize) -> bool {
        (buf[block / 8] >> (block % 8)) & 1 != 0
    }

    // ── Block alloc / free (updates free_blocks counter) ─────────────────────

    fn alloc_data_block(device: &dyn BlockDevice, sb: &mut Superblock) -> Result<u32, AbiError> {
        let mut bitmap = [0u8; BLOCK_SIZE];
        Self::read_block(device, BITMAP_BLOCK, &mut bitmap)?;
        for b in sb.first_data_block..sb.total_blocks {
            if !Self::test_bitmap_bit(&bitmap, b as usize) {
                Self::set_bitmap_bit(&mut bitmap, b as usize);
                Self::write_block(device, BITMAP_BLOCK, &bitmap)?;
                let zero = [0u8; BLOCK_SIZE];
                Self::write_block(device, b, &zero)?;
                sb.free_blocks = sb.free_blocks.saturating_sub(1);
                return Ok(b);
            }
        }
        Err(AbiError::Other("No free blocks"))
    }

    fn free_data_block(device: &dyn BlockDevice, sb: &mut Superblock, block: u32) -> Result<(), AbiError> {
        if block < sb.first_data_block || block >= sb.total_blocks {
            return Err(AbiError::OutOfBounds);
        }
        let mut bitmap = [0u8; BLOCK_SIZE];
        Self::read_block(device, BITMAP_BLOCK, &mut bitmap)?;
        Self::clear_bitmap_bit(&mut bitmap, block as usize);
        Self::write_block(device, BITMAP_BLOCK, &bitmap)?;
        sb.free_blocks = sb.free_blocks.saturating_add(1);
        Ok(())
    }

    // ── Inode alloc / free (updates free_inodes counter) ─────────────────────

    fn read_inode(device: &dyn BlockDevice, inode_id: u32) -> Result<Inode, AbiError> {
        if inode_id >= INODE_COUNT { return Err(AbiError::OutOfBounds); }
        let mut buf = [0u8; BLOCK_SIZE];
        Self::read_block(device, INODE_TABLE_BLOCK, &mut buf)?;
        let offset = inode_id as usize * 64;
        Ok(unsafe { core::ptr::read_unaligned(buf[offset..].as_ptr() as *const Inode) })
    }

    fn write_inode(device: &dyn BlockDevice, inode_id: u32, inode: &Inode) -> Result<(), AbiError> {
        if inode_id >= INODE_COUNT { return Err(AbiError::OutOfBounds); }
        let mut buf = [0u8; BLOCK_SIZE];
        Self::read_block(device, INODE_TABLE_BLOCK, &mut buf)?;
        let offset = inode_id as usize * 64;
        unsafe {
            core::ptr::copy_nonoverlapping(inode as *const Inode as *const u8, buf[offset..].as_mut_ptr(), 64);
        }
        Self::write_block(device, INODE_TABLE_BLOCK, &buf)
    }

    fn alloc_inode(device: &dyn BlockDevice, sb: &mut Superblock, mode: u16) -> Result<u32, AbiError> {
        let mut buf = [0u8; BLOCK_SIZE];
        Self::read_block(device, INODE_TABLE_BLOCK, &mut buf)?;
        for i in 1..INODE_COUNT {
            let offset = i as usize * 64;
            let mode_val: u16 = unsafe { core::ptr::read_unaligned(buf[offset..].as_ptr() as *const u16) };
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
                    core::ptr::copy_nonoverlapping(&inode as *const Inode as *const u8, buf[offset..].as_mut_ptr(), 64);
                }
                Self::write_block(device, INODE_TABLE_BLOCK, &buf)?;
                sb.free_inodes = sb.free_inodes.saturating_sub(1);
                return Ok(i);
            }
        }
        Err(AbiError::Other("No free inodes"))
    }

    fn free_inode(device: &dyn BlockDevice, sb: &mut Superblock, inode_id: u32) -> Result<(), AbiError> {
        if inode_id >= INODE_COUNT { return Err(AbiError::OutOfBounds); }
        let mut buf = [0u8; BLOCK_SIZE];
        Self::read_block(device, INODE_TABLE_BLOCK, &mut buf)?;
        let offset = inode_id as usize * 64;
        buf[offset..offset + 64].fill(0);
        Self::write_block(device, INODE_TABLE_BLOCK, &buf)?;
        sb.free_inodes = sb.free_inodes.saturating_add(1);
        Ok(())
    }

    // ── Block mapping ─────────────────────────────────────────────────────────

    fn inode_get_block(device: &dyn BlockDevice, inode: &Inode, logical: u32) -> Result<u32, AbiError> {
        if logical < 10 {
            return Ok(inode.blocks[logical as usize]);
        }
        let idx = (logical - 10) as usize;
        if idx < BLOCK_SIZE / 4 {
            if inode.indirect == 0 { return Ok(0); }
            let mut buf = [0u8; BLOCK_SIZE];
            Self::read_block(device, inode.indirect, &mut buf)?;
            let ptrs: &[u32] = unsafe { core::slice::from_raw_parts(buf.as_ptr() as *const u32, BLOCK_SIZE / 4) };
            return Ok(ptrs[idx]);
        }
        let idx2 = idx - BLOCK_SIZE / 4;
        if idx2 < (BLOCK_SIZE / 4) * (BLOCK_SIZE / 4) {
            if inode.double_indirect == 0 { return Ok(0); }
            let mut buf = [0u8; BLOCK_SIZE];
            Self::read_block(device, inode.double_indirect, &mut buf)?;
            let l1: &[u32] = unsafe { core::slice::from_raw_parts(buf.as_ptr() as *const u32, BLOCK_SIZE / 4) };
            let l1_idx = idx2 / (BLOCK_SIZE / 4);
            let l2_idx = idx2 % (BLOCK_SIZE / 4);
            if l1[l1_idx] == 0 { return Ok(0); }
            let mut buf2 = [0u8; BLOCK_SIZE];
            Self::read_block(device, l1[l1_idx], &mut buf2)?;
            let l2: &[u32] = unsafe { core::slice::from_raw_parts(buf2.as_ptr() as *const u32, BLOCK_SIZE / 4) };
            return Ok(l2[l2_idx]);
        }
        Err(AbiError::Other("File too large"))
    }

    fn inode_alloc_block(device: &dyn BlockDevice, sb: &mut Superblock, inode: &mut Inode, logical: u32) -> Result<u32, AbiError> {
        if logical < 10 {
            if inode.blocks[logical as usize] == 0 {
                inode.blocks[logical as usize] = Self::alloc_data_block(device, sb)?;
            }
            return Ok(inode.blocks[logical as usize]);
        }
        let idx = (logical - 10) as usize;
        if idx < BLOCK_SIZE / 4 {
            if inode.indirect == 0 {
                inode.indirect = Self::alloc_data_block(device, sb)?;
            }
            let mut buf = [0u8; BLOCK_SIZE];
            Self::read_block(device, inode.indirect, &mut buf)?;
            let ptrs: &mut [u32] = unsafe { core::slice::from_raw_parts_mut(buf.as_mut_ptr() as *mut u32, BLOCK_SIZE / 4) };
            if ptrs[idx] == 0 {
                ptrs[idx] = Self::alloc_data_block(device, sb)?;
                Self::write_block(device, inode.indirect, &buf)?;
            }
            return Ok(ptrs[idx]);
        }
        let idx2 = idx - BLOCK_SIZE / 4;
        if idx2 < (BLOCK_SIZE / 4) * (BLOCK_SIZE / 4) {
            if inode.double_indirect == 0 {
                inode.double_indirect = Self::alloc_data_block(device, sb)?;
            }
            let mut buf = [0u8; BLOCK_SIZE];
            Self::read_block(device, inode.double_indirect, &mut buf)?;
            let l1: &mut [u32] = unsafe { core::slice::from_raw_parts_mut(buf.as_mut_ptr() as *mut u32, BLOCK_SIZE / 4) };
            let l1_idx = idx2 / (BLOCK_SIZE / 4);
            let l2_idx = idx2 % (BLOCK_SIZE / 4);
            if l1[l1_idx] == 0 {
                l1[l1_idx] = Self::alloc_data_block(device, sb)?;
                Self::write_block(device, inode.double_indirect, &buf)?;
            }
            let l1_phys = l1[l1_idx];
            let mut buf2 = [0u8; BLOCK_SIZE];
            Self::read_block(device, l1_phys, &mut buf2)?;
            let l2: &mut [u32] = unsafe { core::slice::from_raw_parts_mut(buf2.as_mut_ptr() as *mut u32, BLOCK_SIZE / 4) };
            if l2[l2_idx] == 0 {
                l2[l2_idx] = Self::alloc_data_block(device, sb)?;
                Self::write_block(device, l1_phys, &buf2)?;
            }
            return Ok(l2[l2_idx]);
        }
        Err(AbiError::Other("File too large"))
    }

    fn free_inode_blocks(device: &dyn BlockDevice, sb: &mut Superblock, inode: &Inode) -> Result<(), AbiError> {
        for i in 0..10 {
            if inode.blocks[i] != 0 {
                Self::free_data_block(device, sb, inode.blocks[i])?;
            }
        }
        if inode.indirect != 0 {
            let mut buf = [0u8; BLOCK_SIZE];
            Self::read_block(device, inode.indirect, &mut buf)?;
            let ptrs: &[u32] = unsafe { core::slice::from_raw_parts(buf.as_ptr() as *const u32, BLOCK_SIZE / 4) };
            for &p in ptrs { if p != 0 { Self::free_data_block(device, sb, p)?; } }
            Self::free_data_block(device, sb, inode.indirect)?;
        }
        if inode.double_indirect != 0 {
            let mut buf = [0u8; BLOCK_SIZE];
            Self::read_block(device, inode.double_indirect, &mut buf)?;
            let l1: &[u32] = unsafe { core::slice::from_raw_parts(buf.as_ptr() as *const u32, BLOCK_SIZE / 4) };
            for &l1p in l1 {
                if l1p != 0 {
                    let mut buf2 = [0u8; BLOCK_SIZE];
                    Self::read_block(device, l1p, &mut buf2)?;
                    let l2: &[u32] = unsafe { core::slice::from_raw_parts(buf2.as_ptr() as *const u32, BLOCK_SIZE / 4) };
                    for &p in l2 { if p != 0 { Self::free_data_block(device, sb, p)?; } }
                    Self::free_data_block(device, sb, l1p)?;
                }
            }
            Self::free_data_block(device, sb, inode.double_indirect)?;
        }
        Ok(())
    }

    // ── Directory entry helpers ───────────────────────────────────────────────

    fn dirent_foreach<F>(buf: &[u8; BLOCK_SIZE], size: u32, mut cb: F)
        where F: FnMut(u32, &str)
    {
        let mut off = 0usize;
        while off + 8 <= (size as usize).min(BLOCK_SIZE) {
            let inode = u32::from_le_bytes([buf[off], buf[off+1], buf[off+2], buf[off+3]]);
            let name_len = u16::from_le_bytes([buf[off+4], buf[off+5]]) as usize;
            let entry_size = u16::from_le_bytes([buf[off+6], buf[off+7]]) as usize;
            if entry_size < 8 || entry_size > 260 || off + entry_size > BLOCK_SIZE { break; }
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

    fn find_entry(buf: &[u8; BLOCK_SIZE], size: u32, name: &str) -> Option<u32> {
        let mut result = None;
        Self::dirent_foreach(buf, size, |inode, n| { if n == name { result = Some(inode); } });
        result
    }

    /// Write a dirent into buf. Returns Err if the block is full.
    fn write_entry_raw(buf: &mut [u8; BLOCK_SIZE], target_inode: u32, name: &str) -> Result<(), AbiError> {
        let name_bytes = name.as_bytes();
        let name_len = name_bytes.len();
        if name_len > 252 { return Err(AbiError::Other("Name too long")); }
        let entry_size = (8 + name_len + 3) & !3;
        let mut off = 0usize;
        loop {
            if off + entry_size > BLOCK_SIZE { return Err(AbiError::Other("Directory block full")); }
            let existing = u32::from_le_bytes([buf[off], buf[off+1], buf[off+2], buf[off+3]]);
            if existing == 0 { break; }
            let es = u16::from_le_bytes([buf[off+6], buf[off+7]]) as usize;
            if es < 8 { break; }
            off += es;
        }
        buf[off..off+4].copy_from_slice(&target_inode.to_le_bytes());
        buf[off+4..off+6].copy_from_slice(&(name_len as u16).to_le_bytes());
        buf[off+6..off+8].copy_from_slice(&(entry_size as u16).to_le_bytes());
        if name_len > 0 { buf[off+8..off+8+name_len].copy_from_slice(name_bytes); }
        Ok(())
    }

    /// Remove a dirent by inode id from a dir block (zero out the slot).
    fn remove_entry_raw(buf: &mut [u8; BLOCK_SIZE], size: u32, target_inode: u32) -> bool {
        let mut off = 0usize;
        while off + 8 <= (size as usize).min(BLOCK_SIZE) {
            let inode = u32::from_le_bytes([buf[off], buf[off+1], buf[off+2], buf[off+3]]);
            let entry_size = u16::from_le_bytes([buf[off+6], buf[off+7]]) as usize;
            if entry_size < 8 || off + entry_size > BLOCK_SIZE { break; }
            if inode == target_inode {
                buf[off..off+4].fill(0); // zero inode → slot free
                return true;
            }
            off += entry_size;
        }
        false
    }

    // ── Read / Write file ─────────────────────────────────────────────────────

    pub fn read_file(fs: &mut spin::MutexGuard<ZiqaFs>, inode_id: u32, offset: usize, buf: &mut [u8]) -> Result<usize, AbiError> {
        let device = &*fs.device;
        let inode = Self::read_inode(device, inode_id)?;
        if offset >= inode.size as usize { return Ok(0); }
        let to_read = (inode.size as usize - offset).min(buf.len());
        let mut done = 0;
        while done < to_read {
            let logical = (offset + done) as u32 / BLOCK_SIZE as u32;
            let block_off = (offset + done) % BLOCK_SIZE;
            let phys = Self::inode_get_block(device, &inode, logical)?;
            if phys == 0 { break; }
            let mut block_buf = [0u8; BLOCK_SIZE];
            Self::read_block(device, phys, &mut block_buf)?;
            let chunk = (BLOCK_SIZE - block_off).min(to_read - done);
            buf[done..done + chunk].copy_from_slice(&block_buf[block_off..block_off + chunk]);
            done += chunk;
        }
        Ok(done)
    }

    pub fn write_file(fs: &mut spin::MutexGuard<ZiqaFs>, inode_id: u32, offset: usize, buf: &[u8]) -> Result<usize, AbiError> {
        let dev = fs.device.clone();
        let device = &*dev;
        let sb = &mut fs.sb;
        let mut inode = Self::read_inode(device, inode_id)?;
        if inode.mode == INODE_MODE_DIR { return Err(AbiError::Other("Cannot write to directory")); }
        let mut done = 0;
        while done < buf.len() {
            let logical = (offset + done) as u32 / BLOCK_SIZE as u32;
            let block_off = (offset + done) % BLOCK_SIZE;
            let phys = Self::inode_alloc_block(device, sb, &mut inode, logical)?;
            let mut block_buf = [0u8; BLOCK_SIZE];
            if block_off > 0 || done + BLOCK_SIZE > buf.len() {
                Self::read_block(device, phys, &mut block_buf)?;
            }
            let chunk = (BLOCK_SIZE - block_off).min(buf.len() - done);
            block_buf[block_off..block_off + chunk].copy_from_slice(&buf[done..done + chunk]);
            Self::write_block(device, phys, &block_buf)?;
            done += chunk;
        }
        let new_size = (offset + buf.len()).max(inode.size as usize);
        inode.size = new_size as u32;
        inode.mtime = crate::timer::TIMER.lock().uptime_secs() as u32;
        Self::write_inode(device, inode_id, &inode)?;
        Self::flush_superblock_raw(device, sb)?;
        Ok(buf.len())
    }

    // ── Truncate ──────────────────────────────────────────────────────────────

    /// Shrink (or zero-extend) a file to `new_size` bytes.
    pub fn truncate(fs: &mut spin::MutexGuard<ZiqaFs>, inode_id: u32, new_size: usize) -> Result<(), AbiError> {
        let dev = fs.device.clone();
        let device = &*dev;
        let sb = &mut fs.sb;
        let mut inode = Self::read_inode(device, inode_id)?;
        if inode.mode == INODE_MODE_DIR { return Err(AbiError::Other("Cannot truncate directory")); }
        let old_blocks = (inode.size as usize + BLOCK_SIZE - 1) / BLOCK_SIZE;
        let new_blocks = (new_size + BLOCK_SIZE - 1) / BLOCK_SIZE;
        // Free excess blocks
        for logical in (new_blocks as u32)..(old_blocks as u32) {
            let phys = Self::inode_get_block(device, &inode, logical)?;
            if phys != 0 {
                Self::free_data_block(device, sb, phys)?;
                // Zero out the pointer
                if logical < 10 {
                    inode.blocks[logical as usize] = 0;
                } else {
                    let idx = (logical - 10) as usize;
                    if idx < BLOCK_SIZE / 4 && inode.indirect != 0 {
                        let mut buf = [0u8; BLOCK_SIZE];
                        Self::read_block(device, inode.indirect, &mut buf)?;
                        let ptrs: &mut [u32] = unsafe { core::slice::from_raw_parts_mut(buf.as_mut_ptr() as *mut u32, BLOCK_SIZE / 4) };
                        ptrs[idx] = 0;
                        Self::write_block(device, inode.indirect, &buf)?;
                    }
                }
            }
        }
        // If new_size is in the middle of a block, zero the tail
        if new_size > 0 && new_size % BLOCK_SIZE != 0 {
            let last_logical = (new_size - 1) as u32 / BLOCK_SIZE as u32;
            let phys = Self::inode_get_block(device, &inode, last_logical)?;
            if phys != 0 {
                let mut block_buf = [0u8; BLOCK_SIZE];
                Self::read_block(device, phys, &mut block_buf)?;
                let tail_start = new_size % BLOCK_SIZE;
                block_buf[tail_start..].fill(0);
                Self::write_block(device, phys, &block_buf)?;
            }
        }
        inode.size = new_size as u32;
        inode.mtime = crate::timer::TIMER.lock().uptime_secs() as u32;
        Self::write_inode(device, inode_id, &inode)?;
        Self::flush_superblock_raw(device, sb)?;
        Ok(())
    }

    // ── Create file / dir ─────────────────────────────────────────────────────

    /// Append a dirent to a directory inode, allocating a new dir block if needed.
    fn dir_add_entry(device: &dyn BlockDevice, sb: &mut Superblock, parent_inode: u32, new_id: u32, name: &str) -> Result<(), AbiError> {
        let mut parent = Self::read_inode(device, parent_inode)?;
        // Try each existing dir block
        let total_dir_blocks = (parent.size as usize + BLOCK_SIZE - 1) / BLOCK_SIZE;
        for logical in 0..total_dir_blocks as u32 {
            let phys = Self::inode_get_block(device, &parent, logical)?;
            if phys == 0 { continue; }
            let mut buf = [0u8; BLOCK_SIZE];
            Self::read_block(device, phys, &mut buf)?;
            if Self::write_entry_raw(&mut buf, new_id, name).is_ok() {
                return Self::write_block(device, phys, &buf);
            }
        }
        // All existing blocks full — allocate a new one
        let new_logical = total_dir_blocks as u32;
        let new_phys = Self::inode_alloc_block(device, sb, &mut parent, new_logical)?;
        parent.size = (new_logical + 1) * BLOCK_SIZE as u32;
        Self::write_inode(device, parent_inode, &parent)?;
        let mut buf = [0u8; BLOCK_SIZE];
        Self::write_entry_raw(&mut buf, new_id, name)?;
        Self::write_block(device, new_phys, &buf)
    }

    pub fn create_file(fs: &mut spin::MutexGuard<ZiqaFs>, parent_inode: u32, name: &str) -> Result<u32, AbiError> {
        if name.is_empty() || name.len() > 252 { return Err(AbiError::Other("Invalid name")); }
        let dev = fs.device.clone();
        let device = &*dev;
        let sb = &mut fs.sb;
        let parent = Self::read_inode(device, parent_inode)?;
        if parent.mode != INODE_MODE_DIR { return Err(AbiError::Other("Not a directory")); }
        // Check for duplicate across all dir blocks
        if Self::lookup_in_dir(device, &parent, name)?.is_some() {
            return Err(AbiError::Other("File exists"));
        }
        let new_id = Self::alloc_inode(device, sb, INODE_MODE_FILE)?;
        Self::dir_add_entry(device, sb, parent_inode, new_id, name)?;
        Self::flush_superblock_raw(device, sb)?;
        Ok(new_id)
    }

    pub fn create_dir(fs: &mut spin::MutexGuard<ZiqaFs>, parent_inode: u32, name: &str) -> Result<u32, AbiError> {
        if name.is_empty() || name.len() > 252 { return Err(AbiError::Other("Invalid name")); }
        let dev = fs.device.clone();
        let device = &*dev;
        let sb = &mut fs.sb;
        let parent = Self::read_inode(device, parent_inode)?;
        if parent.mode != INODE_MODE_DIR { return Err(AbiError::Other("Not a directory")); }
        if Self::lookup_in_dir(device, &parent, name)?.is_some() {
            return Err(AbiError::Other("File exists"));
        }
        let new_id = Self::alloc_inode(device, sb, INODE_MODE_DIR)?;
        let new_data = Self::alloc_data_block(device, sb)?;
        let mut new_inode = Self::read_inode(device, new_id)?;
        new_inode.blocks[0] = new_data;
        new_inode.size = BLOCK_SIZE as u32;
        new_inode.mtime = crate::timer::TIMER.lock().uptime_secs() as u32;
        new_inode.ctime = new_inode.mtime;
        new_inode.atime = new_inode.mtime;
        Self::write_inode(device, new_id, &new_inode)?;
        let mut dir_buf = [0u8; BLOCK_SIZE];
        let _ = Self::write_entry_raw(&mut dir_buf, new_id, ".");
        let _ = Self::write_entry_raw(&mut dir_buf, parent_inode, "..");
        Self::write_block(device, new_data, &dir_buf)?;
        Self::dir_add_entry(device, sb, parent_inode, new_id, name)?;
        Self::flush_superblock_raw(device, sb)?;
        Ok(new_id)
    }

    // ── Lookup helper (multi-block aware) ─────────────────────────────────────

    fn lookup_in_dir(device: &dyn BlockDevice, dir_inode: &Inode, name: &str) -> Result<Option<u32>, AbiError> {
        let total_blocks = (dir_inode.size as usize + BLOCK_SIZE - 1) / BLOCK_SIZE;
        for logical in 0..total_blocks as u32 {
            let phys = Self::inode_get_block(device, dir_inode, logical)?;
            if phys == 0 { continue; }
            let mut buf = [0u8; BLOCK_SIZE];
            Self::read_block(device, phys, &mut buf)?;
            if let Some(id) = Self::find_entry(&buf, BLOCK_SIZE as u32, name) {
                return Ok(Some(id));
            }
        }
        Ok(None)
    }

    // ── Unlink ────────────────────────────────────────────────────────────────

    // ── Rename ────────────────────────────────────────────────────────────────

    /// Move `name` from `src_parent` to `dst_parent` with `new_name`.
    pub fn rename(fs: &mut spin::MutexGuard<ZiqaFs>, src_parent: u32, name: &str, dst_parent: u32, new_name: &str) -> Result<(), AbiError> {
        if new_name.is_empty() || new_name.len() > 252 { return Err(AbiError::Other("Invalid name")); }
        let dev = fs.device.clone();
        let device = &*dev;
        let sb = &mut fs.sb;
        // Find the inode in src
        let src_inode_obj = Self::read_inode(device, src_parent)?;
        let target_id = Self::lookup_in_dir(device, &src_inode_obj, name)?
            .ok_or(AbiError::Other("File not found"))?;
        // Remove from src dir block
        let total_blocks = (src_inode_obj.size as usize + BLOCK_SIZE - 1) / BLOCK_SIZE;
        for logical in 0..total_blocks as u32 {
            let phys = Self::inode_get_block(device, &src_inode_obj, logical)?;
            if phys == 0 { continue; }
            let mut buf = [0u8; BLOCK_SIZE];
            Self::read_block(device, phys, &mut buf)?;
            if Self::remove_entry_raw(&mut buf, BLOCK_SIZE as u32, target_id) {
                Self::write_block(device, phys, &buf)?;
                break;
            }
        }
        // Add to dst dir
        Self::dir_add_entry(device, sb, dst_parent, target_id, new_name)?;
        Ok(())
    }

    // ── Read dir (multi-block) ────────────────────────────────────────────────

    pub fn read_dir(fs: &mut spin::MutexGuard<ZiqaFs>, inode_id: u32) -> Result<Vec<(String, u32)>, AbiError> {
        let device = &*fs.device;
        let inode = Self::read_inode(device, inode_id)?;
        if inode.mode != INODE_MODE_DIR { return Err(AbiError::Other("Not a directory")); }
        let total_blocks = (inode.size as usize + BLOCK_SIZE - 1) / BLOCK_SIZE;
        let mut entries = Vec::new();
        for logical in 0..total_blocks as u32 {
            let phys = Self::inode_get_block(device, &inode, logical)?;
            if phys == 0 { continue; }
            let mut buf = [0u8; BLOCK_SIZE];
            Self::read_block(device, phys, &mut buf)?;
            Self::dirent_foreach(&buf, BLOCK_SIZE as u32, |eid, ename| {
                if ename != "." && ename != ".." {
                    entries.push((String::from(ename), eid));
                }
            });
        }
        Ok(entries)
    }

    // ── Path lookup ───────────────────────────────────────────────────────────

    pub fn root_lookup(fs: &mut spin::MutexGuard<ZiqaFs>, path: &str) -> Result<u32, AbiError> {
        let device = &*fs.device;
        let name = path.trim_start_matches('/').trim_end_matches('/');
        if name.is_empty() { return Err(AbiError::Other("Empty path")); }
        let mut current_inode_id = ROOT_INODE;
        for part in name.split('/') {
            let inode = Self::read_inode(device, current_inode_id)?;
            if inode.mode != INODE_MODE_DIR { return Err(AbiError::Other("Not a directory")); }
            current_inode_id = Self::lookup_in_dir(device, &inode, part)?
                .ok_or(AbiError::Other("File not found"))?;
        }
        Ok(current_inode_id)
    }

    // ── Stat / StatFs ─────────────────────────────────────────────────────────

    pub fn get_inode(fs: &mut spin::MutexGuard<ZiqaFs>, inode_id: u32) -> Result<Inode, AbiError> {
        Self::read_inode(&*fs.device, inode_id)
    }

    pub fn stat(fs: &mut spin::MutexGuard<ZiqaFs>, inode_id: u32) -> Result<(u16, u32), AbiError> {
        let inode = Self::read_inode(&*fs.device, inode_id)?;
        Ok((inode.mode, inode.size))
    }

    pub fn statfs(fs: &spin::MutexGuard<ZiqaFs>) -> StatFs {
        StatFs {
            total_blocks: fs.sb.total_blocks,
            free_blocks:  fs.sb.free_blocks,
            total_inodes: fs.sb.inode_count,
            free_inodes:  fs.sb.free_inodes,
            block_size:   fs.sb.block_size,
        }
    }

    // ── Fsck ──────────────────────────────────────────────────────────────────

    /// Validate magic, then cross-reference bitmap vs inode block pointers.
    pub fn fsck(fs: &mut spin::MutexGuard<ZiqaFs>) -> FsckResult {
        let device = &*fs.device;
        let mut errors = 0u32;
        let mut leaked_blocks = 0u32;
        let mut leaked_inodes = 0u32;

        // 1. Magic check
        if fs.sb.magic != MAGIC {
            return FsckResult { ok: false, errors: 1, leaked_blocks: 0, leaked_inodes: 0 };
        }

        // 2. Read bitmap
        let mut bitmap = [0u8; BLOCK_SIZE];
        if Self::read_block(device, BITMAP_BLOCK, &mut bitmap).is_err() {
            return FsckResult { ok: false, errors: 1, leaked_blocks: 0, leaked_inodes: 0 };
        }

        // 3. Build a "reachable blocks" set by walking all live inodes
        let mut reachable = [0u8; BLOCK_SIZE]; // same layout as bitmap
        // Mark metadata blocks as reachable
        for b in 0..fs.sb.first_data_block {
            reachable[b as usize / 8] |= 1 << (b as usize % 8);
        }

        let mut live_inodes = 0u32;
        for i in 1..INODE_COUNT {
            if let Ok(inode) = Self::read_inode(device, i) {
                if inode.mode == 0 { continue; }
                live_inodes += 1;
                // Walk all block pointers
                let total_blocks = (inode.size as usize + BLOCK_SIZE - 1) / BLOCK_SIZE;
                for logical in 0..total_blocks as u32 {
                    match Self::inode_get_block(device, &inode, logical) {
                        Ok(phys) if phys != 0 => {
                            reachable[phys as usize / 8] |= 1 << (phys as usize % 8);
                        }
                        _ => {}
                    }
                }
                if inode.indirect != 0 {
                    reachable[inode.indirect as usize / 8] |= 1 << (inode.indirect as usize % 8);
                }
                if inode.double_indirect != 0 {
                    reachable[inode.double_indirect as usize / 8] |= 1 << (inode.double_indirect as usize % 8);
                }
            }
        }

        // 4. Compare bitmap vs reachable
        for b in fs.sb.first_data_block..fs.sb.total_blocks {
            let byte = b as usize / 8;
            let bit = 1u8 << (b as usize % 8);
            let in_bitmap = bitmap[byte] & bit != 0;
            let in_reachable = reachable[byte] & bit != 0;
            if in_bitmap && !in_reachable {
                // Marked used but not reachable from any inode → leaked
                leaked_blocks += 1;
                errors += 1;
            }
        }

        // 5. Check free_inodes counter consistency
        let expected_free = INODE_COUNT - 1 - live_inodes;
        if fs.sb.free_inodes != expected_free {
            leaked_inodes = (fs.sb.free_inodes as i32 - expected_free as i32).unsigned_abs();
            errors += 1;
        }

        FsckResult { ok: errors == 0, errors, leaked_blocks, leaked_inodes }
    }

    // ── Root dir init ─────────────────────────────────────────────────────────

    fn create_root_dir(device: &dyn BlockDevice, sb: &Superblock) -> Result<(), AbiError> {
        let mut sb_mut = *sb;
        let root_id = Self::alloc_inode(device, &mut sb_mut, INODE_MODE_DIR)?;
        let root_block = Self::alloc_data_block(device, &mut sb_mut)?;
        let mut root_inode = Self::read_inode(device, root_id)?;
        root_inode.blocks[0] = root_block;
        root_inode.size = BLOCK_SIZE as u32;
        Self::write_inode(device, root_id, &root_inode)?;
        let mut dir_buf = [0u8; BLOCK_SIZE];
        let _ = Self::write_entry_raw(&mut dir_buf, root_id, ".");
        Self::write_block(device, root_block, &dir_buf)
    }

    // ── Journal (WAL) ─────────────────────────────────────────────────────────

    fn journal_read(device: &dyn BlockDevice) -> Result<([u8; BLOCK_SIZE], JournalHeader), AbiError> {
        let mut buf = [0u8; BLOCK_SIZE];
        Self::read_block(device, JOURNAL_BLOCK, &mut buf)?;
        let hdr: JournalHeader = unsafe { core::ptr::read_unaligned(buf.as_ptr() as *const JournalHeader) };
        Ok((buf, hdr))
    }

    fn journal_commit(device: &dyn BlockDevice, op: JournalOp, block: u32, data: &[u8]) -> Result<(), AbiError> {
        let (mut buf, mut hdr) = Self::journal_read(device)?;
        if hdr.magic != JOURNAL_MAGIC { return Ok(()); }
        let slot = hdr.head as usize % JOURNAL_ENTRIES;
        let entry_off = core::mem::size_of::<JournalHeader>() + slot * JOURNAL_ENTRY_SIZE;
        let mut entry = JournalEntry { op: op as u8, _pad: [0; 3], block, data: [0u8; 56] };
        let n = data.len().min(56);
        entry.data[..n].copy_from_slice(&data[..n]);
        unsafe {
            core::ptr::copy_nonoverlapping(&entry as *const JournalEntry as *const u8, buf[entry_off..].as_mut_ptr(), JOURNAL_ENTRY_SIZE);
        }
        hdr.head = (hdr.head + 1) % JOURNAL_ENTRIES as u32;
        hdr.committed += 1;
        unsafe {
            core::ptr::copy_nonoverlapping(&hdr as *const JournalHeader as *const u8, buf.as_mut_ptr(), core::mem::size_of::<JournalHeader>());
        }
        Self::write_block(device, JOURNAL_BLOCK, &buf)
    }

    /// Replay uncommitted journal entries on mount for crash recovery.
    pub fn journal_replay(fs: &mut spin::MutexGuard<ZiqaFs>) -> u32 {
        let device = &*fs.device;
        let (buf, hdr) = match Self::journal_read(device) { Ok(v) => v, Err(_) => return 0 };
        if hdr.magic != JOURNAL_MAGIC || hdr.committed == 0 { return 0; }
        let mut replayed = 0u32;
        for i in 0..JOURNAL_ENTRIES {
            let off = core::mem::size_of::<JournalHeader>() + i * JOURNAL_ENTRY_SIZE;
            let entry: JournalEntry = unsafe { core::ptr::read_unaligned(buf[off..].as_ptr() as *const JournalEntry) };
            if entry.op == JournalOp::WriteInode as u8 {
                let inode_id = entry.block;
                if inode_id < INODE_COUNT {
                    if let Ok(mut ib) = { let mut b = [0u8; BLOCK_SIZE]; Self::read_block(device, INODE_TABLE_BLOCK, &mut b).map(|_| b) } {
                        let o = inode_id as usize * 64;
                        ib[o..o + 56].copy_from_slice(&entry.data);
                        let _ = Self::write_block(device, INODE_TABLE_BLOCK, &ib);
                        replayed += 1;
                    }
                }
            } else if entry.op == JournalOp::WriteBitmap as u8 {
                if let Ok(mut bm) = { let mut b = [0u8; BLOCK_SIZE]; Self::read_block(device, BITMAP_BLOCK, &mut b).map(|_| b) } {
                    bm[entry.block as usize] = entry.data[0];
                    let _ = Self::write_block(device, BITMAP_BLOCK, &bm);
                    replayed += 1;
                }
            }
        }
        replayed
    }

    // ── Hard links ────────────────────────────────────────────────────────────

    pub fn link(fs: &mut spin::MutexGuard<ZiqaFs>, src_inode_id: u32, dst_parent: u32, name: &str) -> Result<(), AbiError> {
        if name.is_empty() || name.len() > 252 { return Err(AbiError::Other("Invalid name")); }
        let dev = fs.device.clone();
        let device = &*dev;
        let sb = &mut fs.sb;
        let mut inode = Self::read_inode(device, src_inode_id)?;
        if inode.mode == INODE_MODE_DIR { return Err(AbiError::Other("Cannot hard-link a directory")); }
        let dst_inode_obj = Self::read_inode(device, dst_parent)?;
        if Self::lookup_in_dir(device, &dst_inode_obj, name)?.is_some() {
            return Err(AbiError::Other("File exists"));
        }
        inode.nlink += 1;
        inode.ctime = crate::timer::TIMER.lock().uptime_secs() as u32;
        let inode_bytes = unsafe { core::slice::from_raw_parts(&inode as *const Inode as *const u8, 56) };
        let _ = Self::journal_commit(device, JournalOp::WriteInode, src_inode_id, inode_bytes);
        Self::write_inode(device, src_inode_id, &inode)?;
        Self::dir_add_entry(device, sb, dst_parent, src_inode_id, name)?;
        Ok(())
    }

    // ── Unlink (nlink-aware) ──────────────────────────────────────────────────

    pub fn unlink(fs: &mut spin::MutexGuard<ZiqaFs>, parent_inode: u32, name: &str) -> Result<(), AbiError> {
        let dev = fs.device.clone();
        let device = &*dev;
        let sb = &mut fs.sb;
        let parent = Self::read_inode(device, parent_inode)?;
        let total_dir_blocks = (parent.size as usize + BLOCK_SIZE - 1) / BLOCK_SIZE;
        let mut target_id = None;
        let mut found_phys = 0u32;
        for logical in 0..total_dir_blocks as u32 {
            let phys = Self::inode_get_block(device, &parent, logical)?;
            if phys == 0 { continue; }
            let mut buf = [0u8; BLOCK_SIZE];
            Self::read_block(device, phys, &mut buf)?;
            if let Some(id) = Self::find_entry(&buf, BLOCK_SIZE as u32, name) {
                target_id = Some(id);
                found_phys = phys;
                break;
            }
        }
        let target_id = target_id.ok_or(AbiError::Other("File not found"))?;
        let mut target_inode = Self::read_inode(device, target_id)?;
        target_inode.nlink = target_inode.nlink.saturating_sub(1);
        target_inode.ctime = crate::timer::TIMER.lock().uptime_secs() as u32;
        let mut buf = [0u8; BLOCK_SIZE];
        Self::read_block(device, found_phys, &mut buf)?;
        Self::remove_entry_raw(&mut buf, BLOCK_SIZE as u32, target_id);
        Self::write_block(device, found_phys, &buf)?;
        if target_inode.nlink == 0 {
            Self::free_inode_blocks(device, sb, &target_inode)?;
            Self::free_inode(device, sb, target_id)?;
        } else {
            Self::write_inode(device, target_id, &target_inode)?;
        }
        Self::flush_superblock_raw(device, sb)?;
        Ok(())
    }

    // ── Copy file ─────────────────────────────────────────────────────────────

    pub fn copy_file(fs: &mut spin::MutexGuard<ZiqaFs>, src_inode_id: u32, dst_parent: u32, name: &str) -> Result<u32, AbiError> {
        let src_inode = { let dev = fs.device.clone(); Self::read_inode(&*dev, src_inode_id)? };
        if src_inode.mode == INODE_MODE_DIR { return Err(AbiError::Other("Cannot copy directory")); }
        let new_id = Self::create_file(fs, dst_parent, name)?;
        let size = src_inode.size as usize;
        let total_logical = (size + BLOCK_SIZE - 1) / BLOCK_SIZE;
        for logical in 0..total_logical as u32 {
            let src_phys = { let dev = fs.device.clone(); Self::inode_get_block(&*dev, &src_inode, logical)? };
            if src_phys == 0 { continue; }
            let mut block_buf = [0u8; BLOCK_SIZE];
            { let dev = fs.device.clone(); Self::read_block(&*dev, src_phys, &mut block_buf)?; }
            let write_len = if logical as usize == total_logical.saturating_sub(1) && size % BLOCK_SIZE != 0 {
                size % BLOCK_SIZE
            } else { BLOCK_SIZE };
            Self::write_file(fs, new_id, logical as usize * BLOCK_SIZE, &block_buf[..write_len])?;
        }
        Ok(new_id)
    }

    // ── Disk usage ────────────────────────────────────────────────────────────

    /// Recursively sum 4 KiB blocks used by inode_id and all descendants.
    pub fn du(fs: &mut spin::MutexGuard<ZiqaFs>, inode_id: u32) -> u32 {
        let (mode, size) = {
            let dev = fs.device.clone();
            match Self::read_inode(&*dev, inode_id) {
                Ok(i) => (i.mode, i.size),
                Err(_) => return 0,
            }
        };
        let own = ((size as usize + BLOCK_SIZE - 1) / BLOCK_SIZE) as u32;
        if mode != INODE_MODE_DIR { return own; }
        let children = Self::read_dir(fs, inode_id).unwrap_or_default();
        own + children.iter().map(|(_, cid)| Self::du(fs, *cid)).sum::<u32>()
    }
}

// ── ZiqaFsFile (VFS adapter) ──────────────────────────────────────────────────

pub struct ZiqaFsFile {
    pub fs: Arc<spin::Mutex<ZiqaFs>>,
    pub inode_id: u32,
    pub inode: Inode,
}

impl File for ZiqaFsFile {
    fn read(&self, buf: &mut [u8], offset: usize) -> Result<usize, AbiError> {
        ZiqaFs::read_file(&mut self.fs.lock(), self.inode_id, offset, buf)
    }

    fn write(&mut self, buf: &[u8], offset: usize) -> Result<usize, AbiError> {
        let result = ZiqaFs::write_file(&mut self.fs.lock(), self.inode_id, offset, buf);
        if result.is_ok() {
            if let Ok(inode) = ZiqaFs::get_inode(&mut self.fs.lock(), self.inode_id) {
                self.inode = inode;
            }
        }
        result
    }

    fn file_type(&self) -> FileType {
        if self.inode.mode == 0o040000 { FileType::Directory } else { FileType::Regular }
    }

    fn size(&self) -> usize {
        ZiqaFs::get_inode(&mut self.fs.lock(), self.inode_id)
            .map(|i| i.size as usize)
            .unwrap_or(self.inode.size as usize)
    }
}

// ── VFS integration ───────────────────────────────────────────────────────────

fn enumerate_dir(fs: &Arc<spin::Mutex<ZiqaFs>>, inode_id: u32, prefix: &str, entries: &mut Vec<(String, u32, u16)>) {
    let dir_entries = ZiqaFs::read_dir(&mut fs.lock(), inode_id).unwrap_or_default();
    for (name, child_id) in dir_entries {
        let path = if prefix.is_empty() || prefix == "/" {
            alloc::format!("/{}", name)
        } else {
            alloc::format!("{}/{}", prefix, name)
        };
        let mode = ZiqaFs::get_inode(&mut fs.lock(), child_id).map(|i| i.mode).unwrap_or(0);
        entries.push((path.clone(), child_id, mode));
        if mode == 0o040000 {
            enumerate_dir(fs, child_id, &path, entries);
        }
    }
}

pub fn mount_into_vfs(fs: &Arc<spin::Mutex<ZiqaFs>>) {
    use crate::fs::vfs::VFS;

    let mut entries = Vec::new();
    enumerate_dir(fs, ROOT_INODE, "", &mut entries);

    let mut vfs = VFS.lock();
    for (path, inode_id, _) in &entries {
        if let Ok(inode) = ZiqaFs::get_inode(&mut fs.lock(), *inode_id) {
            vfs.mount(path, Arc::new(Mutex::new(ZiqaFsFile { fs: fs.clone(), inode_id: *inode_id, inode })));
        }
    }
    // Mount root as /disk
    if let Ok(inode) = ZiqaFs::get_inode(&mut fs.lock(), ROOT_INODE) {
        vfs.mount("/disk", Arc::new(Mutex::new(ZiqaFsFile { fs: fs.clone(), inode_id: ROOT_INODE, inode })));
        vfs.mkdir("/disk");
    }
    *ZIQAFS.lock() = Some(fs.clone());
}

lazy_static! {
    pub static ref ZIQAFS: Mutex<Option<Arc<spin::Mutex<ZiqaFs>>>> = Mutex::new(None);
}
