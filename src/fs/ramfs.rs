/// RamFS implementation for ZiqaKernel
///
/// Stores files in-memory using dynamic buffers.

extern crate alloc;
use alloc::vec::Vec;
use crate::fs::{File, FileType};
use crate::abi::AbiError;

pub struct RamFile {
    pub data: Vec<u8>,
}

impl RamFile {
    pub fn new() -> Self {
        Self {
            data: Vec::new(),
        }
    }
    
    pub fn from_bytes(bytes: &[u8]) -> Self {
        Self {
            data: bytes.to_vec(),
        }
    }
}

impl File for RamFile {
    fn read(&self, buf: &mut [u8], offset: usize) -> Result<usize, AbiError> {
        let size = self.data.len();
        if offset >= size {
            return Ok(0);
        }
        let available = size - offset;
        let to_copy = available.min(buf.len());
        buf[..to_copy].copy_from_slice(&self.data[offset..offset + to_copy]);
        Ok(to_copy)
    }

    fn write(&mut self, buf: &[u8], offset: usize) -> Result<usize, AbiError> {
        let new_min_size = offset + buf.len();
        if new_min_size > self.data.len() {
            self.data.resize(new_min_size, 0);
        }
        self.data[offset..new_min_size].copy_from_slice(buf);
        Ok(buf.len())
    }

    fn file_type(&self) -> FileType {
        FileType::Regular
    }

    fn size(&self) -> usize {
        self.data.len()
    }
}
