/// Virtual File System (VFS) for ZiqaKernel
///
/// This module provides the core traits and management logic for
/// the kernel's filesystem.

pub mod ramfs;
pub mod vfs;
pub mod ziqafs;
pub mod pagecache;

use crate::abi::AbiError;

/// File type in ZiqaKernel
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileType {
    Regular,
    Directory,
    CharacterDevice,
}

/// Generic file trait that all filesystem implementations must satisfy.
pub trait File: Send {
    /// Read from the file at the given offset.
    fn read(&self, buf: &mut [u8], offset: usize) -> Result<usize, AbiError>;
    
    /// Write to the file at the given offset.
    fn write(&mut self, buf: &[u8], offset: usize) -> Result<usize, AbiError>;
    
    /// Get the type of the file.
    fn file_type(&self) -> FileType;
    
    /// Get the current size of the file.
    fn size(&self) -> usize;
}

/// Per-process file descriptor table (fixed-size, no alloc)
const MAX_FDS: usize = 64;

#[allow(dead_code)]
pub struct FdTable {
    fds: [Option<usize>; MAX_FDS], // Maps FD to a global VFS handle or similar
}

impl FdTable {
    pub const fn new() -> Self {
        Self { fds: [None; MAX_FDS] }
    }
}
