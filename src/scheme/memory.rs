use crate::scheme::{Scheme, SchemeResult};
use crate::abi::AbiError;
use crate::process::scheduler::with_current_task;
use crate::capability::ResourceKind;
use core::cmp::min;
use spin::Mutex;
use alloc::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryType {
    Allocated(u64), // physical address of allocated frame
    Physical,
}

pub struct HandleState {
    pub mem_type: MemoryType,
    pub offset: u64,
}

pub struct MemoryScheme {
    next_id: core::sync::atomic::AtomicUsize,
    handles: Mutex<BTreeMap<usize, HandleState>>,
}

impl MemoryScheme {
    pub const fn new() -> Self {
        Self {
            next_id: core::sync::atomic::AtomicUsize::new(1),
            handles: Mutex::new(BTreeMap::new()),
        }
    }
}

impl Scheme for MemoryScheme {
    fn open(&self, path: &str, _flags: usize) -> SchemeResult<usize> {
        let has_cap = with_current_task(|proc| {
            proc.capabilities.has_permission(ResourceKind::Memory, false, false)
        }).unwrap_or(false);

        if !has_cap {
            return Err(AbiError::PermissionDenied);
        }

        let mem_type = match path {
            "physical" => MemoryType::Physical,
            _ => return Err(AbiError::Other("Invalid memory type")),
        };

        let id = self.next_id.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        self.handles.lock().insert(id, HandleState { mem_type, offset: 0 });
        Ok(id)
    }

    fn read(&self, id: usize, buf: &mut [u8]) -> SchemeResult<usize> {
        let mut handles = self.handles.lock();
        let state = handles.get_mut(&id).ok_or(AbiError::Other("Invalid descriptor"))?;
        let offset_u64 = state.offset;

        let phys_addr = match state.mem_type {
            MemoryType::Allocated(_) => return Err(AbiError::Other("Not supported")),
            MemoryType::Physical => offset_u64,
        };

        let max_read = buf.len(); // Can read up to requested size
        let to_read = min(buf.len(), max_read);
        if to_read == 0 {
            return Ok(0);
        }

        unsafe {
            let virt_addr = crate::memory::VirtAddr::new(crate::memory::paging::phys_offset().as_u64() + phys_addr);
            let src = core::slice::from_raw_parts(virt_addr.as_ptr::<u8>(), to_read);
            buf[..to_read].copy_from_slice(src);
        }

        state.offset += to_read as u64;
        Ok(to_read)
    }

    fn write(&self, id: usize, buf: &[u8]) -> SchemeResult<usize> {
        let mut handles = self.handles.lock();
        let state = handles.get_mut(&id).ok_or(AbiError::Other("Invalid descriptor"))?;
        let offset_u64 = state.offset;

        let phys_addr = match state.mem_type {
            MemoryType::Allocated(_) => return Err(AbiError::Other("Not supported")),
            MemoryType::Physical => offset_u64,
        };

        let max_write = buf.len();
        let to_write = min(buf.len(), max_write);
        if to_write == 0 {
            return Ok(0);
        }

        unsafe {
            let virt_addr = crate::memory::VirtAddr::new(crate::memory::paging::phys_offset().as_u64() + phys_addr);
            let dst = core::slice::from_raw_parts_mut(virt_addr.as_mut_ptr::<u8>(), to_write);
            dst.copy_from_slice(&buf[..to_write]);
        }

        state.offset += to_write as u64;
        Ok(to_write)
    }

    fn close(&self, id: usize) -> SchemeResult<()> {
        self.handles.lock().remove(&id).ok_or(AbiError::Other("Invalid descriptor"))?;
        Ok(())
    }
}
