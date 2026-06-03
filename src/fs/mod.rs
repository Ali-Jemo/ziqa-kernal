pub mod pagecache;
/// Virtual File System (VFS) for ZiqaKernel
///
/// This module provides the core traits and management logic for
/// the kernel's filesystem.
pub mod ramfs;
pub mod vfs;
#[cfg(feature = "ziqafs")]
pub mod ziqafs; // now a directory module: src/fs/ziqafs/mod.rs
#[cfg(feature = "fat32")]
pub mod fat32;

use crate::abi::AbiError;
extern crate alloc;

/// Normalize a path by resolving `.` and `..` segments.
/// Returns a clean absolute path starting with `/`.
pub fn normalize_path(path: &str) -> alloc::string::String {
    let mut parts: alloc::vec::Vec<&str> = alloc::vec::Vec::new();
    for seg in path.split('/') {
        match seg {
            "" | "." => continue,
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    if parts.is_empty() {
        alloc::string::String::from("/")
    } else {
        alloc::format!("/{}", parts.join("/"))
    }
}

/// Resolve a (possibly relative) path against a working directory and normalize it.
pub fn resolve_path(cwd: &[u8], cwd_len: usize, path: &str) -> alloc::string::String {
    let cwd_str = core::str::from_utf8(&cwd[..cwd_len]).unwrap_or("/");
    if path.starts_with('/') {
        normalize_path(path)
    } else {
        let combined = if cwd_str == "/" {
            alloc::format!("/{}", path)
        } else {
            alloc::format!("{}/{}", cwd_str.trim_end_matches('/'), path)
        };
        normalize_path(&combined)
    }
}

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

    /// Truncate the file to the given size.
    fn truncate(&mut self, _new_size: usize) -> Result<(), AbiError> {
        Err(AbiError::Other("Truncate not implemented"))
    }
}

/// Per-process file descriptor table (fixed-size, no alloc)
const MAX_FDS: usize = 64;

#[allow(dead_code)]
pub struct FdTable {
    fds: [Option<usize>; MAX_FDS], // Maps FD to a global VFS handle or similar
}

impl FdTable {
    pub const fn new() -> Self {
        Self {
            fds: [None; MAX_FDS],
        }
    }
}
