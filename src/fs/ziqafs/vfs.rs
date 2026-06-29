/// VFS adapter and mount integration for ZiqaFS.

use crate::capability::{CapabilityToken, CapabilityId, ResourceKind, Permissions};
use super::file::read_dir;
use super::inode::read_inode;
use super::types::*;
use super::ZiqaFs;
use crate::abi::AbiError;
use crate::fs::{File, FileType};
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use lazy_static::lazy_static;
use spin::Mutex;

pub struct ZiqaFsFile {
    pub fs: Arc<spin::Mutex<ZiqaFs>>,
    pub inode_id: u32,
    pub inode: Inode,
    pub cap: CapabilityToken,
}

impl File for ZiqaFsFile {
    fn read(&self, buf: &mut [u8], offset: usize) -> Result<usize, AbiError> {
        let fs = self.fs.lock();
        super::file::read_file(&*fs.device, self.inode_id, offset, buf)
    }

    fn write(&mut self, buf: &[u8], offset: usize) -> Result<usize, AbiError> {
        let mut fs = self.fs.lock();
        let dev = fs.device.clone();
        let result = super::file::write_file(&*dev, &mut fs.sb, self.inode_id, offset, buf);
        if result.is_ok() {
            if let Ok(inode) = read_inode(&*fs.device, self.inode_id) {
                self.inode = inode;
            }
        }
        result
    }

    fn file_type(&self) -> FileType {
        if self.inode.mode == INODE_MODE_DIR {
            FileType::Directory
        } else {
            FileType::Regular
        }
    }

    fn size(&self) -> usize {
        let fs = self.fs.lock();
        read_inode(&*fs.device, self.inode_id)
            .map(|i| i.size as usize)
            .unwrap_or(self.inode.size as usize)
    }
}

fn enumerate_dir(
    fs: &Arc<spin::Mutex<ZiqaFs>>,
    inode_id: u32,
    prefix: &str,
    entries: &mut Vec<(String, u32, u16)>,
) {
    let dir_entries = {
        let fs_lock = fs.lock();
        read_dir(&*fs_lock.device, inode_id).unwrap_or_default()
    };
    for (name, child_id) in dir_entries {
        let path = if prefix.is_empty() || prefix == "/" {
            alloc::format!("/{}", name)
        } else {
            alloc::format!("{}/{}", prefix, name)
        };
        let mode = {
            let fs_lock = fs.lock();
            read_inode(&*fs_lock.device, child_id)
                .map(|i| i.mode)
                .unwrap_or(0)
        };
        entries.push((path.clone(), child_id, mode));
        if mode == INODE_MODE_DIR {
            enumerate_dir(fs, child_id, &path, entries);
        }
    }
}

pub fn mount_into_vfs(fs: &Arc<spin::Mutex<ZiqaFs>>) {
    use crate::fs::vfs::VFS;

    let mut entries = Vec::new();
    enumerate_dir(fs, ROOT_INODE, "", &mut entries);

    let mut vfs = VFS.write();
    for (path, inode_id, _) in &entries {
        let inode = {
            let fs_lock = fs.lock();
            read_inode(&*fs_lock.device, *inode_id)
        };
        if let Ok(inode) = inode {
            let file = ZiqaFsFile {
                fs: fs.clone(),
                inode_id: *inode_id,
                inode,
                cap: CapabilityToken::new(CapabilityId(0), None, ResourceKind::ZiqaFsMount, Permissions::full(), 0),
            };
            vfs.mount(path, Arc::new(Mutex::new(file)));
        }
    }
    let root_inode = {
        let fs_lock = fs.lock();
        read_inode(&*fs_lock.device, ROOT_INODE)
    };
    if let Ok(inode) = root_inode {
        let file = ZiqaFsFile {
            fs: fs.clone(),
            inode_id: ROOT_INODE,
            inode,
            cap: CapabilityToken::new(CapabilityId(0), None, ResourceKind::ZiqaFsMount, Permissions::full(), 0),
        };
        vfs.mount("/disk", Arc::new(Mutex::new(file)));
        vfs.mkdir("/disk");
    }
    *ZIQAFS.lock() = Some(fs.clone());
}

lazy_static! {
    pub static ref ZIQAFS: Mutex<Option<Arc<spin::Mutex<ZiqaFs>>>> = Mutex::new(None);
}
