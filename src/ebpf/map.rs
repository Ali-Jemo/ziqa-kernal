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
    RingBuf = 3,
    ProgArray = 4,
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
        let total_size = match map_type {
            BpfMapType::Array | BpfMapType::ProgArray => {
                // ponytail: checked_mul prevents u32 overflow on crafted value_size * max_entries
                value_size.checked_mul(max_entries)
                    .map(|n| n as usize)
                    .unwrap_or(0)
            }
            BpfMapType::Hash => {
                // entry: 1 byte used flag + key + value
                let entry_size = 1u32.saturating_add(key_size).saturating_add(value_size);
                entry_size.checked_mul(max_entries)
                    .map(|n| n as usize)
                    .unwrap_or(0)
            }
            BpfMapType::RingBuf => max_entries as usize,
        };
        Self {
            map_type,
            key_size,
            value_size,
            max_entries,
            data: Mutex::new(alloc::vec![0u8; total_size]),
        }
    }

    pub fn lookup(&self, key_ptr: u64) -> Result<u64, BpfError> {
        if self.map_type == BpfMapType::Array || self.map_type == BpfMapType::ProgArray {
            let index = unsafe { *(key_ptr as *const u32) };
            if index >= self.max_entries {
                return Ok(0); // NULL
            }
            let offset = (index * self.value_size) as usize;
            let mut data = self.data.lock();
            let ptr = data.as_mut_ptr().wrapping_add(offset);
            return Ok(ptr as u64);
        } else if self.map_type == BpfMapType::Hash {
            let entry_size = (1 + self.key_size + self.value_size) as usize;
            let mut data = self.data.lock();
            let key_slice = unsafe { core::slice::from_raw_parts(key_ptr as *const u8, self.key_size as usize) };
            
            for i in 0..self.max_entries {
                let offset = (i as usize) * entry_size;
                if data[offset] == 1 {
                    let k = &data[offset + 1 .. offset + 1 + self.key_size as usize];
                    if k == key_slice {
                        let ptr = data.as_mut_ptr().wrapping_add(offset + 1 + self.key_size as usize);
                        return Ok(ptr as u64);
                    }
                }
            }
            return Ok(0); // NULL
        }
        
        Err(BpfError::ExecutionError) // Unsupported map type
    }

    pub fn update(&self, key_ptr: u64, value_ptr: u64) -> Result<u64, BpfError> {
        if self.map_type == BpfMapType::Array || self.map_type == BpfMapType::ProgArray {
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
        } else if self.map_type == BpfMapType::Hash {
            let entry_size = (1 + self.key_size + self.value_size) as usize;
            let mut data = self.data.lock();
            let key_slice = unsafe { core::slice::from_raw_parts(key_ptr as *const u8, self.key_size as usize) };
            let value_slice = unsafe { core::slice::from_raw_parts(value_ptr as *const u8, self.value_size as usize) };
            
            // First try to update existing
            for i in 0..self.max_entries {
                let offset = (i as usize) * entry_size;
                if data[offset] == 1 {
                    let k = &data[offset + 1 .. offset + 1 + self.key_size as usize];
                    if k == key_slice {
                        data[offset + 1 + self.key_size as usize .. offset + entry_size].copy_from_slice(value_slice);
                        return Ok(0);
                    }
                }
            }
            // If not found, find empty slot
            for i in 0..self.max_entries {
                let offset = (i as usize) * entry_size;
                if data[offset] == 0 {
                    data[offset] = 1;
                    data[offset + 1 .. offset + 1 + self.key_size as usize].copy_from_slice(key_slice);
                    data[offset + 1 + self.key_size as usize .. offset + entry_size].copy_from_slice(value_slice);
                    return Ok(0);
                }
            }
            return Ok(1); // Map full
        }
        Err(BpfError::ExecutionError)
    }

    pub fn delete(&self, key_ptr: u64) -> Result<u64, BpfError> {
        if self.map_type == BpfMapType::Array || self.map_type == BpfMapType::ProgArray {
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
        } else if self.map_type == BpfMapType::Hash {
            let entry_size = (1 + self.key_size + self.value_size) as usize;
            let mut data = self.data.lock();
            let key_slice = unsafe { core::slice::from_raw_parts(key_ptr as *const u8, self.key_size as usize) };
            
            for i in 0..self.max_entries {
                let offset = (i as usize) * entry_size;
                if data[offset] == 1 {
                    let k = &data[offset + 1 .. offset + 1 + self.key_size as usize];
                    if k == key_slice {
                        data[offset] = 0; // Mark as empty
                        return Ok(0);
                    }
                }
            }
            return Ok(1); // Not found
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
