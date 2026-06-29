
use crate::scheme::{Scheme, SchemeResult};
use crate::abi::AbiError;
use core::cmp::min;
use spin::Mutex;
use alloc::collections::BTreeMap;
use alloc::vec::Vec;

pub static DTB_DATA: Mutex<Vec<u8>> = Mutex::new(Vec::new());

pub struct DtbScheme {
    next_id: core::sync::atomic::AtomicUsize,
    offsets: Mutex<BTreeMap<usize, usize>>,
}

impl DtbScheme {
    /// Initialize the DTB data from the bootloader.
    /// This should be called by the architecture-specific boot code (e.g. AArch64/RISC-V)
    /// once the flattened device tree (FDT) is found in memory.
    pub fn init(data: &[u8]) {
        let mut dtb = DTB_DATA.lock();
        dtb.clear();
        dtb.extend_from_slice(data);
    }

    pub fn new() -> Self {
        Self {
            next_id: core::sync::atomic::AtomicUsize::new(1),
            offsets: Mutex::new(BTreeMap::new()),
        }
    }
}

impl Scheme for DtbScheme {
    fn open(&self, _path: &str, _flags: usize) -> SchemeResult<usize> {
        let id = self.next_id.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        self.offsets.lock().insert(id, 0);
        Ok(id)
    }
    
    fn read(&self, id: usize, buf: &mut [u8]) -> SchemeResult<usize> {
        let mut offsets = self.offsets.lock();
        let offset = offsets.get_mut(&id).ok_or(AbiError::Other("Invalid descriptor"))?;
        
        let data = DTB_DATA.lock();
        if *offset >= data.len() {
            return Ok(0); // EOF
        }
        
        let to_read = min(buf.len(), data.len() - *offset);
        buf[..to_read].copy_from_slice(&data[*offset..*offset + to_read]);
        *offset += to_read;
        
        Ok(to_read)
    }
    
    fn write(&self, _id: usize, _buf: &[u8]) -> SchemeResult<usize> {
        Err(AbiError::PermissionDenied)
    }
    
    fn close(&self, id: usize) -> SchemeResult<()> {
        self.offsets.lock().remove(&id);
        Ok(())
    }
}

