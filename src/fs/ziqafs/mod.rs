//! ZiqaFS — a simple journaled filesystem for ZiqaKernel.
//!
//! Submodule layout (each has a single responsibility):
//!   types   — on-disk structs and constants
//!   block   — block I/O and bitmap allocation
//!   inode   — inode alloc/read/write and block mapping
//!   journal — write-ahead log (WAL)
//!   dir     — directory entry helpers
//!   file    — file read/write/truncate/copy/du + path ops
//!   fsck    — filesystem consistency check
//!   vfs     — VFS adapter (ZiqaFsFile) and mount_into_vfs

pub mod block;
pub mod dir;
pub mod file;
pub mod fsck;
pub mod inode;
pub mod journal;
pub mod types;
pub mod vfs;

// Re-export the public surface so callers can use `ziqafs::*` as before.
pub use types::{
    FsckResult, Inode, JournalOp, StatFs, Superblock, BITMAP_BLOCK, BLOCK_SIZE, BMAP_CLEAN,
    BMAP_DIRTY, FIRST_DATA_BLOCK, INODE_COUNT, INODE_MODE_DIR, INODE_MODE_FILE, INODE_TABLE_BLOCK,
    JOURNAL_BLOCK, MAGIC, ROOT_INODE, SECTOR_SIZE, SECTORS_PER_BLOCK, SUPERBLOCK_BLOCK,
};
pub use vfs::{mount_into_vfs, ZiqaFsFile, ZIQAFS};

use crate::abi::AbiError;
use crate::drivers::block::BlockDevice;
use alloc::sync::Arc;

// ── Main struct ───────────────────────────────────────────────────────────────

pub struct ZiqaFs {
    pub device: Arc<dyn BlockDevice>,
    pub sb: Superblock,
}

impl ZiqaFs {
    // ── Mount ─────────────────────────────────────────────────────────────────

    pub fn mount(device: Arc<dyn BlockDevice>) -> Result<Arc<spin::Mutex<Self>>, AbiError> {
        let mut sb_buf = alloc::boxed::Box::new([0u8; BLOCK_SIZE]);
        block::read_blocks(&*device, SUPERBLOCK_BLOCK, 1, &mut *sb_buf)?;
        let sb: Superblock =
            unsafe { core::ptr::read_unaligned(sb_buf.as_ptr() as *const Superblock) };
        if sb.magic != MAGIC {
            return Err(AbiError::Other("Not a ZiqaFS filesystem"));
        }
        if sb.block_size != BLOCK_SIZE as u32 {
            return Err(AbiError::Other("Block size mismatch"));
        }
        Ok(Arc::new(spin::Mutex::new(Self { device, sb })))
    }

    // ── Format ────────────────────────────────────────────────────────────────

    pub fn format(device: Arc<dyn BlockDevice>) -> Result<Arc<spin::Mutex<Self>>, AbiError> {
        use block::{set_bitmap_bit, write_blocks};
        use types::{JournalHeader, JOURNAL_MAGIC};

        let mut buf = alloc::boxed::Box::new([0u8; BLOCK_SIZE]);
        let total_blocks =
            (device.total_sectors() * SECTOR_SIZE as u64 / BLOCK_SIZE as u64) as u32;
        let data_blocks = total_blocks.saturating_sub(FIRST_DATA_BLOCK);
        let sb = Superblock {
            magic: MAGIC,
            total_blocks,
            inode_count: INODE_COUNT,
            first_data_block: FIRST_DATA_BLOCK,
            block_size: BLOCK_SIZE as u32,
            flags: BMAP_DIRTY,
            free_blocks: data_blocks,
            free_inodes: INODE_COUNT - 1,
            reserved: [0u8; 20],
        };
        unsafe {
            let bytes = core::slice::from_raw_parts(
                &sb as *const Superblock as *const u8,
                core::mem::size_of::<Superblock>(),
            );
            buf[..bytes.len()].copy_from_slice(bytes);
        }
        write_blocks(&*device, SUPERBLOCK_BLOCK, 1, &*buf)?;

        buf.fill(0);
        for i in 0..FIRST_DATA_BLOCK {
            set_bitmap_bit(&mut *buf, i as usize);
        }
        write_blocks(&*device, BITMAP_BLOCK, 1, &*buf)?;

        buf.fill(0);
        let jh = JournalHeader {
            magic: JOURNAL_MAGIC,
            head: 0,
            committed: 0,
            _pad: [0u8; 52],
        };
        unsafe {
            core::ptr::copy_nonoverlapping(
                &jh as *const JournalHeader as *const u8,
                buf.as_mut_ptr(),
                core::mem::size_of::<JournalHeader>(),
            );
        }
        write_blocks(&*device, JOURNAL_BLOCK, 1, &*buf)?;

        buf.fill(0);
        write_blocks(&*device, INODE_TABLE_BLOCK, 1, &*buf)?;

        let mut sb = sb;
        Self::create_root_dir(&*device, &mut sb)?;

        let mut fs = Self { device, sb };
        fs.sb.flags = BMAP_CLEAN;
        Self::flush_superblock_inner(&*fs.device, &fs.sb)?;
        Ok(Arc::new(spin::Mutex::new(fs)))
    }

    // ── Superblock flush ──────────────────────────────────────────────────────

    fn flush_superblock_inner(device: &dyn BlockDevice, sb: &Superblock) -> Result<(), AbiError> {
        let mut buf = alloc::boxed::Box::new([0u8; BLOCK_SIZE]);
        unsafe {
            let bytes = core::slice::from_raw_parts(
                sb as *const Superblock as *const u8,
                core::mem::size_of::<Superblock>(),
            );
            buf[..bytes.len()].copy_from_slice(bytes);
        }
        block::write_blocks(device, SUPERBLOCK_BLOCK, 1, &*buf)
    }

    pub fn flush_superblock(&mut self) -> Result<(), AbiError> {
        Self::flush_superblock_inner(&*self.device, &self.sb)
    }

    // ── Root dir init (used only during format) ───────────────────────────────

    fn create_root_dir(device: &dyn BlockDevice, sb: &mut Superblock) -> Result<(), AbiError> {
        let root_id = inode::alloc_inode(device, sb, INODE_MODE_DIR)?;
        let root_block = block::alloc_data_block(device, sb)?;
        let mut root_inode = inode::read_inode(device, root_id)?;
        root_inode.blocks[0] = root_block;
        root_inode.size = BLOCK_SIZE as u32;
        inode::write_inode(device, root_id, &root_inode)?;
        let mut dir_buf = [0u8; BLOCK_SIZE];
        let _ = dir::write_entry_raw(&mut dir_buf, root_id, ".");
        block::write_block(device, root_block, &dir_buf)
    }

    // ── Delegating public API (thin wrappers so callers keep the same call site) ──

    pub fn read_file(
        fs: &mut spin::MutexGuard<ZiqaFs>,
        inode_id: u32,
        offset: usize,
        buf: &mut [u8],
    ) -> Result<usize, AbiError> {
        file::read_file(&*fs.device, inode_id, offset, buf)
    }

    pub fn write_file(
        fs: &mut spin::MutexGuard<ZiqaFs>,
        inode_id: u32,
        offset: usize,
        buf: &[u8],
    ) -> Result<usize, AbiError> {
        let dev = fs.device.clone();
        file::write_file(&*dev, &mut fs.sb, inode_id, offset, buf)
    }

    pub fn truncate(
        fs: &mut spin::MutexGuard<ZiqaFs>,
        inode_id: u32,
        new_size: usize,
    ) -> Result<(), AbiError> {
        let dev = fs.device.clone();
        file::truncate(&*dev, &mut fs.sb, inode_id, new_size)
    }

    pub fn create_file(
        fs: &mut spin::MutexGuard<ZiqaFs>,
        parent_inode: u32,
        name: &str,
    ) -> Result<u32, AbiError> {
        let dev = fs.device.clone();
        file::create_file(&*dev, &mut fs.sb, parent_inode, name)
    }

    pub fn create_dir(
        fs: &mut spin::MutexGuard<ZiqaFs>,
        parent_inode: u32,
        name: &str,
    ) -> Result<u32, AbiError> {
        let dev = fs.device.clone();
        file::create_dir(&*dev, &mut fs.sb, parent_inode, name)
    }

    pub fn unlink(
        fs: &mut spin::MutexGuard<ZiqaFs>,
        parent_inode: u32,
        name: &str,
    ) -> Result<(), AbiError> {
        let dev = fs.device.clone();
        file::unlink(&*dev, &mut fs.sb, parent_inode, name)
    }

    pub fn rename(
        fs: &mut spin::MutexGuard<ZiqaFs>,
        src_parent: u32,
        name: &str,
        dst_parent: u32,
        new_name: &str,
    ) -> Result<(), AbiError> {
        let dev = fs.device.clone();
        file::rename(&*dev, &mut fs.sb, src_parent, name, dst_parent, new_name)
    }

    pub fn link(
        fs: &mut spin::MutexGuard<ZiqaFs>,
        src_inode_id: u32,
        dst_parent: u32,
        name: &str,
    ) -> Result<(), AbiError> {
        let dev = fs.device.clone();
        file::link(&*dev, &mut fs.sb, src_inode_id, dst_parent, name)
    }

    pub fn read_dir(
        fs: &mut spin::MutexGuard<ZiqaFs>,
        inode_id: u32,
    ) -> Result<alloc::vec::Vec<(alloc::string::String, u32)>, AbiError> {
        file::read_dir(&*fs.device, inode_id)
    }

    pub fn root_lookup(
        fs: &mut spin::MutexGuard<ZiqaFs>,
        path: &str,
    ) -> Result<u32, AbiError> {
        file::root_lookup(&*fs.device, path)
    }

    pub fn copy_file(
        fs: &mut spin::MutexGuard<ZiqaFs>,
        src_inode_id: u32,
        dst_parent: u32,
        name: &str,
    ) -> Result<u32, AbiError> {
        let dev = fs.device.clone();
        file::copy_file(&*dev, &mut fs.sb, src_inode_id, dst_parent, name)
    }

    pub fn du(fs: &mut spin::MutexGuard<ZiqaFs>, inode_id: u32) -> u32 {
        file::du(&*fs.device, inode_id)
    }

    pub fn get_inode(
        fs: &mut spin::MutexGuard<ZiqaFs>,
        inode_id: u32,
    ) -> Result<Inode, AbiError> {
        inode::read_inode(&*fs.device, inode_id)
    }

    pub fn stat(
        fs: &mut spin::MutexGuard<ZiqaFs>,
        inode_id: u32,
    ) -> Result<(u16, u32), AbiError> {
        let i = inode::read_inode(&*fs.device, inode_id)?;
        Ok((i.mode, i.size))
    }

    pub fn statfs(fs: &spin::MutexGuard<ZiqaFs>) -> StatFs {
        StatFs {
            total_blocks: fs.sb.total_blocks,
            free_blocks: fs.sb.free_blocks,
            total_inodes: fs.sb.inode_count,
            free_inodes: fs.sb.free_inodes,
            block_size: fs.sb.block_size,
        }
    }

    pub fn fsck(fs: &mut spin::MutexGuard<ZiqaFs>) -> FsckResult {
        fsck::fsck(&*fs.device, &fs.sb)
    }

    pub fn journal_replay(fs: &mut spin::MutexGuard<ZiqaFs>) -> u32 {
        journal::journal_replay(&*fs.device)
    }
}
