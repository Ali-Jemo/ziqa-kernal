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

/// A node in the VFS tree
pub enum VfsNode {
    Directory {
        children: BTreeMap<String, Arc<RwLock<VfsNode>>>,
    },
    File {
        handle: Arc<Mutex<dyn File + Send>>,
    },
}

impl VfsNode {
    pub fn new_dir() -> Self {
        VfsNode::Directory {
            children: BTreeMap::new(),
        }
    }

    pub fn is_dir(&self) -> bool {
        matches!(self, VfsNode::Directory { .. })
    }
}

/// Global VFS state
pub struct Vfs {
    /// Root of the hierarchical tree
    root: Option<Arc<RwLock<VfsNode>>>,
}

impl Vfs {
    pub const fn new() -> Self {
        Self { root: None }
    }

    /// Ensure the VFS is initialized with a root directory.
    pub fn init(&mut self) {
        if self.root.is_none() {
            self.root = Some(Arc::new(RwLock::new(VfsNode::new_dir())));
        }
    }

    fn get_root(&self) -> Arc<RwLock<VfsNode>> {
        self.root.as_ref().expect("VFS not initialized").clone()
    }

    /// Resolve a path to a specific node in the tree.
    fn resolve_node(&self, path: &str) -> Result<Arc<RwLock<VfsNode>>, AbiError> {
        let path = path.trim_start_matches('/');
        let root = self.get_root();
        if path.is_empty() {
            return Ok(root);
        }

        let mut current = root;
        for part in path.split('/') {
            if part.is_empty() || part == "." {
                continue;
            }
            
            let next = {
                let guard = current.read();
                match &*guard {
                    VfsNode::Directory { children } => children.get(part).cloned(),
                    VfsNode::File { .. } => return Err(AbiError::Other("Not a directory")),
                }
            };

            match next {
                Some(n) => current = n,
                None => return Err(AbiError::Other("File not found")),
            }
        }
        Ok(current)
    }

    /// Helper to find parent node and leaf name.
    fn resolve_parent(&self, path: &str) -> Result<(Arc<RwLock<VfsNode>>, String), AbiError> {
        let (dir_part, leaf) = match path.rfind('/') {
            Some(idx) => (&path[..idx], &path[idx + 1..]),
            None => ("/", path),
        };
        let dir_path = if dir_part.is_empty() { "/" } else { dir_part };
        Ok((self.resolve_node(dir_path)?, leaf.to_string()))
    }

    /// Register a file in the VFS, creating parent directories as needed.
    pub fn mount(&mut self, path: &str, file: Arc<Mutex<dyn File + Send>>) {
        self.init();
        let path = path.trim_start_matches('/');
        let mut current = self.get_root();
        let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

        if parts.is_empty() { return; }

        for (i, part) in parts.iter().enumerate() {
            let is_last = i == parts.len() - 1;
            
            let next = {
                let mut guard = current.write();
                match &mut *guard {
                    VfsNode::Directory { children } => {
                        if is_last {
                            let node = Arc::new(RwLock::new(VfsNode::File { handle: file.clone() }));
                            children.insert(part.to_string(), node.clone());
                            Some(node)
                        } else {
                            if !children.contains_key(*part) {
                                children.insert(part.to_string(), Arc::new(RwLock::new(VfsNode::new_dir())));
                            }
                            children.get(*part).cloned()
                        }
                    }
                    VfsNode::File { .. } => return,
                }
            };

            if let Some(n) = next {
                current = n;
            }
        }
    }

    /// Read from a file, checking for a valid File capability
    pub fn read(
        &self,
        process: &Process,
        path: &str,
        buf: &mut [u8],
        offset: usize,
    ) -> Result<usize, AbiError> {
        if !process
            .capabilities
            .has_permission(ResourceKind::File, false, false)
        {
            return Err(AbiError::PermissionDenied);
        }

        self.read_raw(path, buf, offset)
    }

    /// Read from a file without capability checks (kernel-internal use).
    pub fn read_raw(&self, path: &str, buf: &mut [u8], offset: usize) -> Result<usize, AbiError> {
        let node = self.resolve_node(path)?;
        let guard = node.read();
        match &*guard {
            VfsNode::File { handle } => handle.lock().read(buf, offset),
            VfsNode::Directory { .. } => Err(AbiError::Other("Is a directory")),
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
        if !process
            .capabilities
            .has_permission(ResourceKind::File, true, false)
        {
            return Err(AbiError::PermissionDenied);
        }

        self.write_raw(path, buf, offset)
    }

    /// Write to a file without capability checks (kernel-internal use)
    pub fn write_raw(&self, path: &str, buf: &[u8], offset: usize) -> Result<usize, AbiError> {
        let node = self.resolve_node(path)?;
        let guard = node.read();
        match &*guard {
            VfsNode::File { handle } => handle.lock().write(buf, offset),
            VfsNode::Directory { .. } => Err(AbiError::Other("Is a directory")),
        }
    }

    /// Check if a path exists in the VFS
    pub fn exists(&self, path: &str) -> bool {
        self.resolve_node(path).is_ok()
    }

    /// List all registered file paths (Recursive traversal)
    pub fn list(&self) -> Vec<alloc::string::String> {
        let mut all = Vec::new();
        self.list_recursive(&self.get_root(), "/", &mut all);
        all
    }

    fn list_recursive(&self, node: &Arc<RwLock<VfsNode>>, prefix: &str, out: &mut Vec<String>) {
        let guard = node.read();
        match &*guard {
            VfsNode::Directory { children } => {
                for (name, child) in children {
                    let path = if prefix == "/" {
                        alloc::format!("/{}", name)
                    } else {
                        alloc::format!("{}/{}", prefix.trim_end_matches('/'), name)
                    };
                    out.push(path.clone());
                    self.list_recursive(child, &path, out);
                }
            }
            VfsNode::File { .. } => {}
        }
    }

    /// Create a new empty RamFile at the given path (kernel-internal)
    pub fn create(&mut self, path: &str) {
        if self.exists(path) { return; }
        
        if path.starts_with("/fat/") {
            #[cfg(feature = "fat32")]
            {
                use crate::fs::fat32;
                if let Ok(fat_file) = fat32::create_file_on_disk(path) {
                    self.mount(path, Arc::new(Mutex::new(fat_file)));
                    return;
                }
            }
        }

        use crate::fs::ramfs::RamFile;
        self.mount(path, Arc::new(Mutex::new(RamFile::new())));
    }

    /// Get the type of a file
    pub fn file_type(&self, path: &str) -> Option<crate::fs::FileType> {
        let node = self.resolve_node(path).ok()?;
        let guard = node.read();
        match &*guard {
            VfsNode::Directory { .. } => Some(crate::fs::FileType::Directory),
            VfsNode::File { handle } => Some(handle.lock().file_type()),
        }
    }

    /// Get the size of a file
    pub fn file_size(&self, path: &str) -> Option<usize> {
        let node = self.resolve_node(path).ok()?;
        let guard = node.read();
        match &*guard {
            VfsNode::File { handle } => Some(handle.lock().size()),
            VfsNode::Directory { .. } => None,
        }
    }

    /// Check if a path is a directory
    pub fn is_dir(&self, path: &str) -> bool {
        match self.resolve_node(path) {
            Ok(node) => node.read().is_dir(),
            Err(_) => false,
        }
    }

    /// List entries in a directory
    pub fn list_dir(&self, path: &str) -> Vec<alloc::string::String> {
        let node = match self.resolve_node(path) {
            Ok(n) => n,
            Err(_) => return Vec::new(),
        };

        let guard = node.read();
        match &*guard {
            VfsNode::Directory { children } => {
                let prefix = if path == "/" || path.is_empty() { "" } else { path.trim_end_matches('/') };
                children.keys().map(|name| alloc::format!("{}/{}", prefix, name)).collect()
            }
            VfsNode::File { .. } => Vec::new(),
        }
    }

    /// Remove a file from VFS
    pub fn remove(&mut self, path: &str) -> Result<(), AbiError> {
        let (parent_node, leaf) = self.resolve_parent(path)?;
        let mut guard = parent_node.write();
        match &mut *guard {
            VfsNode::Directory { children } => {
                if children.remove(&leaf).is_some() {
                    Ok(())
                } else {
                    Err(AbiError::Other("File not found"))
                }
            }
            VfsNode::File { .. } => Err(AbiError::Other("Parent is not a directory")),
        }
    }

    /// Rename/move a file
    pub fn rename(&mut self, old: &str, new: &str) -> Result<(), AbiError> {
        let node = self.resolve_node(old)?;
        let (old_parent, old_leaf) = self.resolve_parent(old)?;
        let (new_parent, new_leaf) = self.resolve_parent(new)?;

        // Remove from old
        {
            let mut guard = old_parent.write();
            if let VfsNode::Directory { children } = &mut *guard {
                children.remove(&old_leaf);
            }
        }

        // Add to new
        {
            let mut guard = new_parent.write();
            if let VfsNode::Directory { children } = &mut *guard {
                children.insert(new_leaf, node);
            }
        }

        Ok(())
    }

    /// Truncate a file to `new_size` bytes.
    pub fn truncate(&self, path: &str, new_size: usize) -> Result<(), AbiError> {
        let node = self.resolve_node(path)?;
        let guard = node.read();
        match &*guard {
            VfsNode::File { handle } => handle.lock().truncate(new_size),
            VfsNode::Directory { .. } => Err(AbiError::Other("Is a directory")),
        }
    }

    /// Remove a directory
    pub fn rmdir(&mut self, path: &str) -> Result<(), AbiError> {
        let p = path.trim_end_matches('/');
        self.remove(p)
    }

    /// Create a directory marker in VFS
    pub fn mkdir(&mut self, path: &str) {
        let p = path.trim_end_matches('/');
        if p.is_empty() || self.exists(p) { return; }
        self.mount_dir(p);
    }

    fn mount_dir(&mut self, path: &str) {
        self.init();
        let path = path.trim_start_matches('/');
        let mut current = self.get_root();
        for part in path.split('/').filter(|s| !s.is_empty()) {
            let next = {
                let mut guard = current.write();
                match &mut *guard {
                    VfsNode::Directory { children } => {
                        if !children.contains_key(part) {
                            children.insert(part.to_string(), Arc::new(RwLock::new(VfsNode::new_dir())));
                        }
                        children.get(part).cloned()
                    }
                    VfsNode::File { .. } => return,
                }
            };
            if let Some(n) = next { current = n; }
        }
    }
}

/// Global VFS instance
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
