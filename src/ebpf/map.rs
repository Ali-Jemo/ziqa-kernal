//! eBPF Maps for ZiqaKernel
//!
//! Maps are shared storage areas used by eBPF programs for state
//! and by userspace for data collection.

use alloc::vec::Vec;
use alloc::sync::Arc;
use spin::Mutex;
use crate::ebpf::BpfError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BpfMapType {
    Array = 1,
    Hash = 2,
}

pub struct BpfMap {
    pub map_type: BpfMapType,
    pub key_size: u32,
    pub value_size: u32,
    pub max_entries: u32,
    pub data: Mutex<Vec<u8>>,
}

impl BpfMap {
    pub fn new(map_type: BpfMapType, key_size: u32, value_size: u32, max_entries: u32) -> Self {
        let total_size = (value_size * max_entries) as usize;
        Self {
            map_type,
            key_size,
            value_size,
            max_entries,
            data: Mutex::new(alloc::vec![0u8; total_size]),
        }
    }

    pub fn lookup(&self, key_ptr: u64) -> Result<u64, BpfError> {
        // For Array maps, key is a 4-byte index
        if self.map_type == BpfMapType::Array {
            let index = unsafe { *(key_ptr as *const u32) };
            if index >= self.max_entries {
                return Ok(0); // NULL
            }
            let offset = (index * self.value_size) as usize;
            let mut data = self.data.lock();
            let ptr = data.as_mut_ptr().wrapping_add(offset);
            return Ok(ptr as u64);
        }
        
        Err(BpfError::ExecutionError) // Unsupported map type
    }

    pub fn update(&self, key_ptr: u64, value_ptr: u64) -> Result<u64, BpfError> {
        if self.map_type == BpfMapType::Array {
            let index = unsafe { *(key_ptr as *const u32) };
            if index >= self.max_entries {
                return Ok(1); // Error
            }
            let offset = (index * self.value_size) as usize;
            let mut data = self.data.lock();
            unsafe {
                core::ptr::copy_nonoverlapping(
                    value_ptr as *const u8,
                    data.as_mut_ptr().add(offset),
                    self.value_size as usize
                );
            }
            return Ok(0);
        }
        Err(BpfError::ExecutionError)
    }

    pub fn delete(&self, key_ptr: u64) -> Result<u64, BpfError> {
        if self.map_type == BpfMapType::Array {
            let index = unsafe { *(key_ptr as *const u32) };
            if index >= self.max_entries {
                return Ok(1); // Error
            }
            let offset = (index * self.value_size) as usize;
            let mut data = self.data.lock();
            unsafe {
                core::ptr::write_bytes(
                    data.as_mut_ptr().add(offset),
                    0,
                    self.value_size as usize
                );
            }
            return Ok(0);
        }
        Err(BpfError::ExecutionError)
    }
}

pub struct BpfMapRegistry {
    maps: Mutex<Vec<Arc<BpfMap>>>,
}

impl BpfMapRegistry {
    pub const fn new() -> Self {
        Self {
            maps: Mutex::new(Vec::new()),
        }
    }

    pub fn register(&self, map: BpfMap) -> usize {
        let mut maps = self.maps.lock();
        let id = maps.len();
        maps.push(Arc::new(map));
        id
    }

    pub fn get(&self, id: usize) -> Option<Arc<BpfMap>> {
        let maps = self.maps.lock();
        maps.get(id).cloned()
    }
}

pub static BPF_MAPS: BpfMapRegistry = BpfMapRegistry::new();
