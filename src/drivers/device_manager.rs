use alloc::vec::Vec;
use alloc::boxed::Box;
use spin::Mutex;
use lazy_static::lazy_static;
use crate::drivers::pci::{PciDevice, PCI_DEVICES};

pub trait Driver: Send + Sync {
    fn name(&self) -> &str;
    fn pci_match(&self, device: &PciDevice) -> bool;
    fn init(&self, device: &PciDevice) -> Result<(), ()>;
}

pub struct DeviceManager {
    drivers: Vec<Box<dyn Driver>>,
}

lazy_static! {
    pub static ref DEVICE_MANAGER: Mutex<DeviceManager> = Mutex::new(DeviceManager::new());
}

impl DeviceManager {
    pub fn new() -> Self {
        Self {
            drivers: Vec::new(),
        }
    }

    pub fn register_driver(&mut self, driver: Box<dyn Driver>) {
        self.drivers.push(driver);
    }

    pub fn scan_and_match(&self) {
        let devices = PCI_DEVICES.lock();
        for device in devices.iter() {
            for driver in self.drivers.iter() {
                if driver.pci_match(device) {
                    let _ = driver.init(device);
                    break;
                }
            }
        }
    }
}

pub fn init() {
    // Device manager initialized via DEVICE_MANAGER lazy_static.
}

/// Match registered drivers against discovered PCI devices.
pub fn scan() {
    let manager = DEVICE_MANAGER.lock();
    manager.scan_and_match();
}
