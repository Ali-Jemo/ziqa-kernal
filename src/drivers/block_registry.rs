//! Global block-device registry.
//!
//! Driver probing registers disks here (`hda`, `vda`, `nvme0n1`, ...). Filesystem
//! mounting code should consume this registry instead of hardcoding ATA/VirtIO.

extern crate alloc;

use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::Mutex;

use crate::drivers::block::BlockDevice;

#[derive(Clone)]
pub struct BlockDeviceEntry {
    pub name: String,
    pub driver: &'static str,
    pub device: Arc<dyn BlockDevice>,
}

pub static BLOCK_DEVICES: Mutex<Vec<BlockDeviceEntry>> = Mutex::new(Vec::new());

/// Register a block device if its name is not already present.
pub fn register(name: &str, driver: &'static str, device: Arc<dyn BlockDevice>) -> bool {
    let mut devices = BLOCK_DEVICES.lock();
    if devices.iter().any(|d| d.name == name) {
        return false;
    }

    crate::println!(
        "[block] registered /dev/{} via {} ({} sectors)",
        name,
        driver,
        device.total_sectors()
    );

    devices.push(BlockDeviceEntry {
        name: name.to_string(),
        driver,
        device,
    });
    true
}

pub fn get(name: &str) -> Option<Arc<dyn BlockDevice>> {
    BLOCK_DEVICES
        .lock()
        .iter()
        .find(|d| d.name == name)
        .map(|d| d.device.clone())
}

pub fn first() -> Option<BlockDeviceEntry> {
    BLOCK_DEVICES.lock().first().cloned()
}

pub fn count() -> usize {
    BLOCK_DEVICES.lock().len()
}

pub fn print_devices() {
    let devices = BLOCK_DEVICES.lock();
    if devices.is_empty() {
        crate::println!("[block] no block devices registered");
        return;
    }

    crate::println!("[block] {} block device(s):", devices.len());
    for d in devices.iter() {
        crate::println!(
            "[block]   /dev/{:<8} driver={:<12} sectors={}",
            d.name,
            d.driver,
            d.device.total_sectors()
        );
    }
}
