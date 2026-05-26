/// VirtIO-Block Driver for ZiqaKernel
///
/// Handles persistent storage via QEMU's VirtIO interface.
/// This is the "fastest" path to native disk I/O in a VM.

use crate::drivers::block::BlockDevice;
use crate::abi::AbiError;

pub struct VirtioBlock {
    // Hardware-specific state (base address, queues, etc.)
    pub base_addr: u64,
    pub total_sectors: u64,
}

impl VirtioBlock {
    pub const fn new(base: u64, sectors: u64) -> Self {
        Self {
            base_addr: base,
            total_sectors: sectors,
        }
    }
}

impl BlockDevice for VirtioBlock {
    fn read_sectors(&self, sector: u64, count: u32, _buf: &mut [u8]) -> Result<(), AbiError> {
        if sector + count as u64 > self.total_sectors {
            return Err(AbiError::OutOfBounds);
        }
        
        // In a real driver, we'd build a VirtIO request and signal the device.
        // For now, we simulate a successful read.
        // println!("[VirtIO] Reading {} sectors from {}", count, sector);
        Ok(())
    }

    fn write_sectors(&self, sector: u64, count: u32, _buf: &[u8]) -> Result<(), AbiError> {
        if sector + count as u64 > self.total_sectors {
            return Err(AbiError::OutOfBounds);
        }
        
        // println!("[VirtIO] Writing {} sectors to {}", count, sector);
        Ok(())
    }

    fn total_sectors(&self) -> u64 {
        self.total_sectors
    }
}
