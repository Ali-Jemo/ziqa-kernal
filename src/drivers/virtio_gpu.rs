use alloc::boxed::Box;
use alloc::sync::Arc;
use core::ptr::NonNull;
use spin::Mutex;
use virtio_drivers::device::gpu::VirtIOGpu;
use virtio_drivers::transport::mmio::{MmioTransport, VirtIOHeader};

use crate::drivers::device_manager::Driver;
use crate::drivers::pci::{bar_address, PciDevice};
use crate::drivers::virtio_hal::VirtioHal;

pub struct VirtioGpuDevice {
    inner: Mutex<VirtIOGpu<VirtioHal, MmioTransport>>,
    width: u32,
    height: u32,
    fb_ptr: *mut u8,
}

// SAFETY: Mutex makes the inner driver thread-safe.
unsafe impl Send for VirtioGpuDevice {}
unsafe impl Sync for VirtioGpuDevice {}

pub static VIRTIO_GPU: Mutex<Option<Arc<VirtioGpuDevice>>> = Mutex::new(None);

pub struct VirtioGpuDriver;

impl Driver for VirtioGpuDriver {
    fn name(&self) -> &str {
        "VirtIO GPU"
    }

    fn pci_match(&self, device: &PciDevice) -> bool {
        // Vendor 0x1AF4 (Red Hat / VirtIO), Device 0x1050 (VirtIO GPU)
        device.vendor_id == 0x1AF4 && device.device_id == 0x1050
    }

    fn init(&self, device: &PciDevice) -> Result<(), ()> {
        crate::println!(
            "[VirtIO-GPU] Initializing device at {:02X}:{:02X}.{}",
            device.bus, device.dev, device.func
        );

        let mut mmio_addr = None;
        for bar in device.bars.iter() {
            let (addr, is_io) = bar_address(*bar);
            if !is_io && addr != 0 {
                mmio_addr = Some(addr);
                break;
            }
        }

        if let Some(addr) = mmio_addr {
            crate::println!("[VirtIO-GPU] Found MMIO BAR at {:#X}", addr);
            let virt_addr = crate::memory::paging::phys_offset().as_u64() + addr;
            let header = unsafe { NonNull::new_unchecked(virt_addr as *mut VirtIOHeader) };

            match unsafe { MmioTransport::new(header) } {
                Ok(transport) => {
                    match VirtIOGpu::<VirtioHal, MmioTransport>::new(transport) {
                        Ok(mut gpu) => {
                            match gpu.resolution() {
                                Ok((w, h)) => {
                                    crate::println!(
                                        "[VirtIO-GPU] Device initialized successfully. Native resolution: {}x{}",
                                        w, h
                                    );
                                    
                                    // We don't allocate the framebuffer here. That's done in init_display.
                                    let wrapped = Arc::new(VirtioGpuDevice {
                                        inner: Mutex::new(gpu),
                                        width: w,
                                        height: h,
                                        fb_ptr: core::ptr::null_mut(),
                                    });
                                    *VIRTIO_GPU.lock() = Some(wrapped);
                                    Ok(())
                                }
                                Err(e) => {
                                    crate::println!("[VirtIO-GPU] Failed to get resolution: {:?}", e);
                                    Err(())
                                }
                            }
                        }
                        Err(e) => {
                            crate::println!("[VirtIO-GPU] Failed to initialize VirtIO GPU driver: {:?}", e);
                            Err(())
                        }
                    }
                }
                Err(e) => {
                    crate::println!("[VirtIO-GPU] Failed to create MmioTransport: {:?}", e);
                    Err(())
                }
            }
        } else {
            crate::println!("[VirtIO-GPU] No MMIO BAR found");
            Err(())
        }
    }
}

pub fn is_available() -> bool {
    VIRTIO_GPU.lock().is_some()
}

pub fn get_resolution() -> Option<(u32, u32)> {
    let gpu_lock = VIRTIO_GPU.lock();
    if let Some(gpu) = gpu_lock.as_ref() {
        Some((gpu.width, gpu.height))
    } else {
        None
    }
}

pub fn get_fb_ptr() -> Option<*mut u8> {
    let gpu_lock = VIRTIO_GPU.lock();
    if let Some(gpu) = gpu_lock.as_ref() {
        if !gpu.fb_ptr.is_null() {
            return Some(gpu.fb_ptr);
        }
    }
    None
}

pub fn init_display() {
    let mut gpu_lock = VIRTIO_GPU.lock();
    if let Some(gpu) = gpu_lock.as_mut() {
        let mut inner = gpu.inner.lock();
        
        crate::println!("[VirtIO-GPU] Setting up framebuffer...");
        match inner.setup_framebuffer() {
            Ok(fb) => {
                let ptr = fb.as_mut_ptr();
                // We need unsafe to bypass the mutability rules since we are exposing the fb_ptr
                // The underlying memory is managed by the GPU driver and won't move.
                let gpu_mut = unsafe { &mut *(Arc::as_ptr(gpu) as *mut VirtioGpuDevice) };
                gpu_mut.fb_ptr = ptr;
                
                let w = gpu.width;
                let h = gpu.height;
                crate::println!("[VirtIO-GPU] Framebuffer allocated at {:p} ({}x{})", ptr, w, h);
                
                // Also init the console
                crate::drivers::fb_console::init(ptr, w as usize, h as usize, (w * 4) as usize);
            }
            Err(e) => {
                crate::println!("[VirtIO-GPU] Failed to setup framebuffer: {:?}", e);
            }
        }
    }
}

pub fn flush() {
    if let Some(gpu) = VIRTIO_GPU.lock().as_ref() {
        let mut inner = gpu.inner.lock();
        let _ = inner.flush();
    }
}

pub fn draw_test_pattern() {
    if let Some(gpu) = VIRTIO_GPU.lock().as_ref() {
        if gpu.fb_ptr.is_null() {
            return;
        }
        
        let w = gpu.width as usize;
        let h = gpu.height as usize;
        
        // Simple color bars
        for y in 0..h {
            for x in 0..w {
                let color = match (x / (w / 8)) % 8 {
                    0 => 0xFFFFFF, // White
                    1 => 0xFFFF00, // Yellow
                    2 => 0x00FFFF, // Cyan
                    3 => 0x00FF00, // Green
                    4 => 0xFF00FF, // Magenta
                    5 => 0xFF0000, // Red
                    6 => 0x0000FF, // Blue
                    7 => 0x000000, // Black
                    _ => 0,
                };
                
                unsafe {
                    let ptr = gpu.fb_ptr.add(y * w * 4 + x * 4) as *mut u32;
                    core::ptr::write_volatile(ptr, color);
                }
            }
        }
        
        flush();
    }
}
