//! Block I/O and bitmap allocation for ZiqaFS.

use super::types::*;
use crate::abi::AbiError;
use crate::drivers::block::BlockDevice;
use crate::zig_kernel_ops;

pub fn read_blocks(
    device: &dyn BlockDevice,
    block: u32,
    count: u32,
    buf: &mut [u8],
) -> Result<(), AbiError> {
    let sector = block as u64 * SECTORS_PER_BLOCK as u64;
    let n = count as u64 * SECTORS_PER_BLOCK as u64;
    if buf.len() < n as usize * SECTOR_SIZE {
        return Err(AbiError::OutOfBounds);
    }
    device.read_sectors(sector, n as u32, buf)
}

pub fn write_blocks(
    device: &dyn BlockDevice,
    block: u32,
    count: u32,
    buf: &[u8],
) -> Result<(), AbiError> {
    let sector = block as u64 * SECTORS_PER_BLOCK as u64;
    let n = count as u64 * SECTORS_PER_BLOCK as u64;
    if buf.len() < n as usize * SECTOR_SIZE {
        return Err(AbiError::OutOfBounds);
    }
    device.write_sectors(sector, n as u32, buf)
}

pub fn read_block(
    device: &dyn BlockDevice,
    block: u32,
    buf: &mut [u8; BLOCK_SIZE],
) -> Result<(), AbiError> {
    read_blocks(device, block, 1, buf)
}

pub fn write_block(
    device: &dyn BlockDevice,
    block: u32,
    buf: &[u8; BLOCK_SIZE],
) -> Result<(), AbiError> {
    write_blocks(device, block, 1, buf)
}

pub fn set_bitmap_bit(buf: &mut [u8; BLOCK_SIZE], block: usize) {
    buf[block / 8] |= 1u8 << (block % 8);
}

pub fn clear_bitmap_bit(buf: &mut [u8; BLOCK_SIZE], block: usize) {
    buf[block / 8] &= !(1u8 << (block % 8));
}

pub fn test_bitmap_bit(buf: &[u8; BLOCK_SIZE], block: usize) -> bool {
    (buf[block / 8] >> (block % 8)) & 1 != 0
}

pub fn alloc_data_block(device: &dyn BlockDevice, sb: &mut Superblock) -> Result<u32, AbiError> {
    let mut bitmap = [0u8; BLOCK_SIZE];
    read_block(device, BITMAP_BLOCK, &mut bitmap)?;
    if let Some(b) = zig_kernel_ops::bitmap_find_clear(&bitmap, sb.first_data_block) {
        if b < sb.total_blocks {
            set_bitmap_bit(&mut bitmap, b as usize);
            write_block(device, BITMAP_BLOCK, &bitmap)?;
            let zero = [0u8; BLOCK_SIZE];
            write_block(device, b, &zero)?;
            sb.free_blocks = sb.free_blocks.saturating_sub(1);
            return Ok(b);
        }
    }
    Err(AbiError::Other("No free blocks"))
}

pub fn free_data_block(
    device: &dyn BlockDevice,
    sb: &mut Superblock,
    block: u32,
) -> Result<(), AbiError> {
    if block < sb.first_data_block || block >= sb.total_blocks {
        return Err(AbiError::OutOfBounds);
    }
    let mut bitmap = [0u8; BLOCK_SIZE];
    read_block(device, BITMAP_BLOCK, &mut bitmap)?;
    clear_bitmap_bit(&mut bitmap, block as usize);
    write_block(device, BITMAP_BLOCK, &bitmap)?;
    sb.free_blocks = sb.free_blocks.saturating_add(1);
    Ok(())
}

pub fn patch_bitmap_byte(device: &dyn BlockDevice, offset: usize, value: u8) -> Result<(), AbiError> {
    let mut bitmap = [0u8; BLOCK_SIZE];
    read_block(device, BITMAP_BLOCK, &mut bitmap)?;
    bitmap[offset] = value;
    write_block(device, BITMAP_BLOCK, &bitmap)
}