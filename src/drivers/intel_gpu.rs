//! Intel GPU driver skeleton (native GPU support)

//! This is a placeholder driver that matches Intel display controllers (class 0x03) and
//! registers a minimal driver to satisfy the native GPU gap. It currently operates in
//! polling mode only and does not provide hardware acceleration. Future work will
//! implement MMIO register mapping, mode setting, and framebuffer management.

use crate::drivers::device_manager::Driver;
use crate::drivers::pci::PciDevice;
use spin::Mutex;

/// Minimal representation of an Intel GPU controller.
pub struct IntelGpuDevice {
    /// Base address of MMIO registers (BAR 0).
    mmio_base: usize,
}

impl IntelGpuDevice {
    /// Create a new instance from a PCI device description.
    fn new(pci: &PciDevice) -> Self {
        // SAFETY: `bar_address` returns the physical address for BAR0.
        let base = unsafe { crate::drivers::pci::bar_address(pci, 0) } as usize;
        IntelGpuDevice { mmio_base: base }
    }

    /// Placeholder initialization – logs that the driver was invoked.
    fn initialize(&self) {
        crate::println!("[IntelGPU] initialized at {:#x}", self.mmio_base);
    }
}

/// Global singleton for the driver instance.
pub static INTEL_GPU: Mutex<Option<IntelGpuDevice>> = Mutex::new(None);

pub struct IntelGpuDriver;

impl Driver for IntelGpuDriver {
    fn name(&self) -> &str { "intel_gpu" }

    // Match any display controller (class 0x03) from Intel (vendor 0x8086).
    fn pci_match(&self, device: &PciDevice) -> bool {
        device.vendor_id == 0x8086 && device.class == 0x03
    }

    fn init(&self, device: &PciDevice) -> Result<(), ()> {
        let dev = IntelGpuDevice::new(device);
        dev.initialize();
        *INTEL_GPU.lock() = Some(dev);
        Ok(())
    }
}

/// Register the driver with the global device manager.
pub fn register() {
    crate::drivers::device_manager::DEVICE_MANAGER
        .lock()
        .register_driver(Box::new(IntelGpuDriver));
    crate::println!("[IntelGPU] driver registered");
}
