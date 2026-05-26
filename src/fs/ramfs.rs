/// RamFS implementation for ZiqaKernel
///
/// Stores files in-memory using fixed-size buffers for now.

use crate::fs::{File, FileType};
use crate::abi::AbiError;

pub struct RamFile {
    pub data: [u8; 4096],
    pub size: usize,
}

impl RamFile {
    pub const fn new() -> Self {
        Self {
            data: [0; 4096],
            size: 0,
        }
    }
}

impl File for RamFile {
    fn read(&self, buf: &mut [u8], offset: usize) -> Result<usize, AbiError> {
        if offset >= self.size {
            return Ok(0);
        }
        let available = self.size - offset;
        let to_copy = available.min(buf.len());
        buf[..to_copy].copy_from_slice(&self.data[offset..offset + to_copy]);
        Ok(to_copy)
    }

    fn write(&mut self, buf: &[u8], offset: usize) -> Result<usize, AbiError> {
        if offset + buf.len() > self.data.len() {
            return Err(AbiError::OutOfMemory);
        }
        self.data[offset..offset + buf.len()].copy_from_slice(buf);
        self.size = self.size.max(offset + buf.len());
        Ok(buf.len())
    }

    fn file_type(&self) -> FileType {
        FileType::Regular
    }

    fn size(&self) -> usize {
        self.size
    }
}
