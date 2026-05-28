/// Capability-based Virtual File System (VFS) for ZiqaKernel
///
/// Every file operation is checked against the calling process's 
/// capability space. Access is only granted if a valid token exists.

extern crate alloc;

use crate::process::Process;
use crate::capability::ResourceKind;
use crate::abi::AbiError;
use crate::fs::File;
use spin::Mutex;
use alloc::collections::BTreeMap;
use alloc::sync::Arc;

/// Global VFS state
pub struct Vfs {
    /// Map of file paths to actual file implementations
    files: BTreeMap<&'static str, Arc<Mutex<dyn File + Send>>>,
}

impl Vfs {
    pub const fn new() -> Self {
        Self {
            files: BTreeMap::new(),
        }
    }

    /// Register a file in the VFS
    pub fn mount(&mut self, path: &'static str, file: Arc<Mutex<dyn File + Send>>) {
        self.files.insert(path, file);
    }

    /// Read from a file, checking for a valid File capability
    pub fn read(&self, process: &Process, path: &str, buf: &mut [u8], offset: usize) -> Result<usize, AbiError> {
        // 1. Check if process has a File capability
        if !process.capabilities.has_permission(ResourceKind::File, false, false) {
            return Err(AbiError::PermissionDenied);
        }

        // 2. Lookup file
        if let Some(file_mutex) = self.files.get(path) {
            let file = file_mutex.lock();
            file.read(buf, offset)
        } else {
            Err(AbiError::Other("File not found"))
        }
    }

    /// Read from a file without capability checks (kernel-internal use).
    pub fn read_raw(&self, path: &str, buf: &mut [u8], offset: usize) -> Result<usize, AbiError> {
        if let Some(file_mutex) = self.files.get(path) {
            let file = file_mutex.lock();
            file.read(buf, offset)
        } else {
            Err(AbiError::Other("File not found"))
        }
    }

    /// Write to a file, checking for a valid Write-enabled File capability
    pub fn write(&self, process: &Process, path: &str, buf: &[u8], offset: usize) -> Result<usize, AbiError> {
        // 1. Check for write permission
        if !process.capabilities.has_permission(ResourceKind::File, true, false) {
            return Err(AbiError::PermissionDenied);
        }

        // 2. Lookup and write
        if let Some(file_mutex) = self.files.get(path) {
            let mut file = file_mutex.lock();
            file.write(buf, offset)
        } else {
            Err(AbiError::Other("File not found"))
        }
    }
}

/// Global VFS instance
pub static VFS: Mutex<Vfs> = Mutex::new(Vfs::new());
