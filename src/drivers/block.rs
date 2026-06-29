/// Block Device Trait for ZiqaKernel
///
/// Provides a unified interface for disks (IDE, SATA, VirtIO, NVMe).
/// All operations are sector-based (typically 512 bytes).
use crate::abi::AbiError;

/// Standard sector size for block devices
pub const SECTOR_SIZE: usize = 512;

/// A generic block device (e.g., a disk)
pub trait BlockDevice: Send + Sync {
    /// Read sectors from the device into the buffer
    fn read_sectors(&self, sector: u64, count: u32, buf: &mut [u8]) -> Result<(), AbiError>;

    /// Write sectors from the buffer to the device
    fn write_sectors(&self, sector: u64, count: u32, buf: &[u8]) -> Result<(), AbiError>;

    /// Get the total number of sectors on the device
    fn total_sectors(&self) -> u64;
}
