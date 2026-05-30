/// Capability-based Virtual File System (VFS) for ZiqaKernel
///
/// Every file operation is checked against the calling process's
/// capability space. Access is only granted if a valid token exists.
extern crate alloc;

use crate::abi::AbiError;
use crate::capability::ResourceKind;
use crate::fs::File;
use crate::process::Process;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::string::ToString;
use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::{Mutex, RwLock};

/// Global VFS state
pub struct Vfs {
    /// Map of file paths to actual file implementations
    files: BTreeMap<String, Arc<Mutex<dyn File + Send>>>,
}

impl Vfs {
    pub const fn new() -> Self {
        Self {
            files: BTreeMap::new(),
        }
    }

    /// Register a file in the VFS
    pub fn mount(&mut self, path: &str, file: Arc<Mutex<dyn File + Send>>) {
        self.files.insert(path.to_string(), file);
    }

    /// Read from a file, checking for a valid File capability
    pub fn read(
        &self,
        process: &Process,
        path: &str,
        buf: &mut [u8],
        offset: usize,
    ) -> Result<usize, AbiError> {
        // 1. Check if process has a File capability
        if !process
            .capabilities
            .has_permission(ResourceKind::File, false, false)
        {
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
    pub fn write(
        &self,
        process: &Process,
        path: &str,
        buf: &[u8],
        offset: usize,
    ) -> Result<usize, AbiError> {
        // 1. Check for write permission
        if !process
            .capabilities
            .has_permission(ResourceKind::File, true, false)
        {
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
    /// Write to a file without capability checks (kernel-internal use)
    pub fn write_raw(&self, path: &str, buf: &[u8], offset: usize) -> Result<usize, AbiError> {
        if let Some(file_mutex) = self.files.get(path) {
            let mut file = file_mutex.lock();
            file.write(buf, offset)
        } else {
            Err(AbiError::Other("File not found"))
        }
    }

    /// Check if a path exists in the VFS
    pub fn exists(&self, path: &str) -> bool {
        self.files.contains_key(path)
    }

    /// List all registered file paths
    pub fn list(&self) -> Vec<alloc::string::String> {
        self.files.keys().cloned().collect()
    }

    /// Create a new empty RamFile at the given path (kernel-internal)
    pub fn create(&mut self, path: &str) {
        use crate::fs::ramfs::RamFile;
        if !self.exists(path) {
            self.files
                .insert(path.to_string(), Arc::new(Mutex::new(RamFile::new())));
        }
    }

    /// Get the size of a file
    pub fn file_size(&self, path: &str) -> Option<usize> {
        self.files.get(path).map(|f| f.lock().size())
    }

    /// Check if a path is a directory (exists as prefix of any file)
    pub fn is_dir(&self, path: &str) -> bool {
        if path == "/" || path.is_empty() {
            return true;
        }
        let p = path.trim_end_matches('/');
        if self.files.contains_key(p) {
            return true;
        }
        let prefix = alloc::format!("{}/", p);
        self.files.keys().any(|k| k.starts_with(&prefix))
    }

    /// List entries in a directory (flattened VFS view)
    pub fn list_dir(&self, path: &str) -> Vec<alloc::string::String> {
        let prefix = if path == "/" || path.is_empty() {
            "/".to_string()
        } else {
            alloc::format!("{}/", path.trim_end_matches('/'))
        };

        let is_root = prefix == "/";
        let mut entries: Vec<String> = Vec::new();
        for key in self.files.keys() {
            if let Some(relative) = key.strip_prefix(&prefix) {
                if relative.is_empty() {
                    continue;
                }
                let entry_name = relative.split('/').next().unwrap_or(relative);
                let full_path = if is_root {
                    alloc::format!("/{}", entry_name)
                } else {
                    alloc::format!("{}/{}", prefix.trim_end_matches('/'), entry_name)
                };
                if !entries.contains(&full_path) {
                    entries.push(full_path);
                }
            }
        }
        entries.sort();
        entries
    }

    /// Remove a file from VFS
    pub fn remove(&mut self, path: &str) -> Result<(), AbiError> {
        if self.files.remove(path).is_some() {
            Ok(())
        } else {
            Err(AbiError::Other("File not found"))
        }
    }

    /// Rename a file (move from old path to new path)
    pub fn rename(&mut self, old: &str, new: &str) -> Result<(), AbiError> {
        if let Some(file) = self.files.remove(old) {
            let p_new = new.trim_end_matches('/');
            self.files.insert(p_new.to_string(), file);
            Ok(())
        } else {
            Err(AbiError::Other("File not found"))
        }
    }

    /// Create a directory marker in VFS
    pub fn mkdir(&mut self, path: &str) {
        use crate::fs::ramfs::RamFile;
        let p = path.trim_end_matches('/');
        if !p.is_empty() && !self.files.contains_key(p) {
            self.files
                .insert(p.to_string(), Arc::new(Mutex::new(RamFile::new())));
        }
    }
}

/// Global VFS instance with fine-grained locking
pub static VFS: RwLock<Vfs> = RwLock::new(Vfs::new());

#[derive(Clone)]
pub struct MountInfo {
    pub source: String,
    pub target: String,
    pub fstype: String,
}

pub static MOUNT_REGISTRY: Mutex<Vec<MountInfo>> = Mutex::new(Vec::new());

pub fn register_mount(source: &str, target: &str, fstype: &str) {
    let mut mounts = MOUNT_REGISTRY.lock();
    mounts.push(MountInfo {
        source: source.to_string(),
        target: target.to_string(),
        fstype: fstype.to_string(),
    });
}
