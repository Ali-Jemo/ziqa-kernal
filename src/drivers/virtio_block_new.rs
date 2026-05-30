use virtio_drivers::device::blk::VirtIOBlk;
use virtio_drivers::transport::mmio::{MmioTransport, VirtIOHeader};
use crate::drivers::virtio_hal::VirtioHal;
use crate::drivers::pci::{PciDevice, bar_address};
use crate::drivers::block::BlockDevice;
use crate::abi::AbiError;
use core::ptr::NonNull;
use alloc::sync::Arc;
use spin::Mutex;

pub struct VirtioBlockNew {
    inner: Mutex<VirtIOBlk<VirtioHal, MmioTransport>>,
}

// SAFETY: Mutex makes the inner driver thread-safe.
unsafe impl Send for VirtioBlockNew {}
unsafe impl Sync for VirtioBlockNew {}

impl BlockDevice for VirtioBlockNew {
    fn read_sectors(&self, sector: u64, _count: u32, buf: &mut [u8]) -> Result<(), AbiError> {
        let mut inner = self.inner.lock();
        inner.read_blocks(sector as usize, buf).map_err(|e| {
            crate::println!("[VirtIO-block-new] read error: {:?}", e);
            AbiError::Other("VirtIO block read failed")
        })
    }

    fn write_sectors(&self, sector: u64, _count: u32, buf: &[u8]) -> Result<(), AbiError> {
        let mut inner = self.inner.lock();
        inner.write_blocks(sector as usize, buf).map_err(|e| {
            crate::println!("[VirtIO-block-new] write error: {:?}", e);
            AbiError::Other("VirtIO block write failed")
        })
    }

    fn total_sectors(&self) -> u64 {
        let inner = self.inner.lock();
        inner.capacity()
    }
}

pub struct VirtioBlockDriverNew;

impl crate::drivers::device_manager::Driver for VirtioBlockDriverNew {
    fn name(&self) -> &str { "Experimental VirtIO Block (virtio-drivers)" }
    fn pci_match(&self, device: &PciDevice) -> bool {
        // 0x1AF4 = Red Hat / VirtIO; 0x1001 = VirtIO Block
        device.vendor_id == 0x1AF4 && device.device_id == 0x1001
    }
    fn init(&self, device: &PciDevice) -> Result<(), ()> {
        crate::println!("[VirtIO-block-new] Initializing device at {:02X}:{:02X}.{}", device.bus, device.dev, device.func);

        // Find the first Memory BAR. In PCI, VirtIO modern is memory-mapped.
        let mut mmio_addr = None;
        for bar in device.bars.iter() {
            let (addr, is_io) = bar_address(*bar);
            if !is_io && addr != 0 {
                mmio_addr = Some(addr);
                break;
            }
        }

        if let Some(addr) = mmio_addr {
            crate::println!("[VirtIO-block-new] Found MMIO BAR at {:#X}", addr);
            
            // Map the MMIO region. 
            // In a real kernel, this would need to use paging to map the device memory.
            // For now, we use the physical address directly if it's identity mapped.
            let virt_addr = crate::memory::paging::phys_offset().as_u64() + addr;
            
            let header = unsafe { NonNull::new_unchecked(virt_addr as *mut VirtIOHeader) };
            
            match unsafe { MmioTransport::new(header) } {
                Ok(transport) => {
                    match VirtIOBlk::<VirtioHal, MmioTransport>::new(transport) {
                        Ok(blk) => {
                            crate::println!("[VirtIO-block-new] VirtIO Block device initialized successfully. Capacity: {} sectors", blk.capacity());
                            let wrapped = Arc::new(VirtioBlockNew {
                                inner: Mutex::new(blk),
                            });
                            crate::drivers::block_registry::register("vda", "virtio-blk-new", wrapped);
                            Ok(())
                        }
                        Err(e) => {
                            crate::println!("[VirtIO-block-new] Failed to initialize VirtIO block driver: {:?}", e);
                            Err(())
                        }
                    }
                }
                Err(e) => {
                    crate::println!("[VirtIO-block-new] Failed to create MmioTransport: {:?}", e);
                    Err(())
                }
            }
        } else {
            crate::println!("[VirtIO-block-new] No MMIO BAR found");
            Err(())
        }
    }
}
