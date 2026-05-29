//! On-disk types for ZiqaFS.

pub const BLOCK_SIZE: usize = 4096;
pub const SECTOR_SIZE: usize = 512;
pub const SECTORS_PER_BLOCK: usize = BLOCK_SIZE / SECTOR_SIZE;
pub const MAGIC: u32 = 0x21514146;

pub const SUPERBLOCK_BLOCK: u32 = 1;
pub const BITMAP_BLOCK: u32 = 2;
pub const INODE_TABLE_BLOCK: u32 = 3;
pub const JOURNAL_BLOCK: u32 = 4;
pub const FIRST_DATA_BLOCK: u32 = 5;

pub const INODE_COUNT: u32 = 64;
pub const ROOT_INODE: u32 = 1;

pub const INODE_MODE_FILE: u16 = 0o100000;
pub const INODE_MODE_DIR: u16 = 0o040000;

pub const BMAP_CLEAN: u32 = 0x0000_0000;
pub const BMAP_DIRTY: u32 = 0x0000_0001;

pub const JOURNAL_ENTRIES: usize = 63;
pub const JOURNAL_ENTRY_SIZE: usize = 64;
pub const JOURNAL_MAGIC: u32 = 0x4A524E4C;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Superblock {
    pub magic: u32,
    pub total_blocks: u32,
    pub inode_count: u32,
    pub first_data_block: u32,
    pub block_size: u32,
    pub flags: u32,
    pub free_blocks: u32,
    pub free_inodes: u32,
    pub reserved: [u8; 20],
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct Inode {
    pub mode: u16,
    pub nlink: u16,
    pub uid: u16,
    pub gid: u16,
    pub size: u32,
    pub mtime: u32,
    pub ctime: u32,
    pub atime: u32,
    pub blocks: [u32; 10],
    pub indirect: u32,
    pub double_indirect: u32,
}
const _: () = assert!(core::mem::size_of::<Inode>() == 72);

pub struct StatFs {
    pub total_blocks: u32,
    pub free_blocks: u32,
    pub total_inodes: u32,
    pub free_inodes: u32,
    pub block_size: u32,
}

pub struct FsckResult {
    pub ok: bool,
    pub errors: u32,
    pub leaked_blocks: u32,
    pub leaked_inodes: u32,
}

#[repr(u8)]
#[derive(Clone, Copy, PartialEq)]
#[allow(dead_code)]
pub enum JournalOp {
    Free = 0,
    AllocBlock = 1,
    FreeBlock = 2,
    WriteInode = 3,
    WriteBitmap = 4,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct JournalHeader {
    pub magic: u32,
    pub head: u32,
    pub committed: u32,
    pub _pad: [u8; 52],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct JournalEntry {
    pub op: u8,
    pub _pad: [u8; 3],
    pub block: u32,
    pub data: [u8; 56],
}
