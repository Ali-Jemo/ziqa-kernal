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

    fn pci_match(&self, _device: &PciDevice) -> bool {
        // Temporarily disabled to debug boot hang and purple artifact
        false
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
            crate::drivers::pci::enable_bus_mastering(device.address);
            crate::drivers::pci::enable_memory_space(device.address);
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
                                    
                                    // Register IPC channel for GPU service
                                    let chan_id = crate::ipc::create_channel().ok_or(())?;
                                    *GPU_IPC_CHANNEL.lock() = Some(chan_id);
                                    crate::println!("[VirtIO-GPU] GPU IPC channel registered: {}", chan_id);

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
    VIRTIO_GPU.lock().is_some() || crate::drivers::framebuffer::is_bga_available()
}

pub fn get_resolution() -> Option<(u32, u32)> {
    let gpu_lock = VIRTIO_GPU.lock();
    if let Some(gpu) = gpu_lock.as_ref() {
        Some((gpu.width, gpu.height))
    } else if let Some((_, w, h, _)) = crate::drivers::framebuffer::get_bga_fb_info() {
        Some((w, h))
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
    if let Some((addr, _, _, _)) = crate::drivers::framebuffer::get_bga_fb_info() {
        return Some(addr as *mut u8);
    }
    None
}

/// Returns (virtual_address, width, height, bpp) if the framebuffer is ready.
pub fn get_fb_info() -> Option<(u64, u32, u32, u32)> {
    let gpu_lock = VIRTIO_GPU.lock();
    if let Some(gpu) = gpu_lock.as_ref() {
        if !gpu.fb_ptr.is_null() {
            return Some((gpu.fb_ptr as u64, gpu.width, gpu.height, 32));
        }
    }
    crate::drivers::framebuffer::get_bga_fb_info()
}

pub fn init_display() -> bool {
    let mut ptr_opt = None;
    let mut w = 0;
    let mut h = 0;

    {
        let mut gpu_lock = VIRTIO_GPU.lock();
        if let Some(gpu) = gpu_lock.as_mut() {
            let mut inner = gpu.inner.lock();
            
            match inner.setup_framebuffer() {
                Ok(fb) => {
                    let ptr = fb.as_mut_ptr();
                    let gpu_mut = unsafe { &mut *(Arc::as_ptr(gpu) as *mut VirtioGpuDevice) };
                    gpu_mut.fb_ptr = ptr;
                    
                    ptr_opt = Some(ptr);
                    w = gpu.width;
                    h = gpu.height;
                }
                Err(_e) => {
                    ptr_opt = None;
                }
            }
        }
    }

    if let Some(ptr) = ptr_opt {
        crate::println!("[VirtIO-GPU] Framebuffer allocated at {:p} ({}x{})", ptr, w, h);
        crate::drivers::fb_console::init(ptr, w as usize, h as usize, (w * 4) as usize);
        true
    } else {
        false
    }
}

pub fn flush() {
    if let Some(gpu) = VIRTIO_GPU.lock().as_ref() {
        if gpu.fb_ptr.is_null() {
            return;
        }
        // Use direct VGA print to avoid recursive deadlock with println!
        // crate::drivers::vga::print(format_args!("[GPU] flush..."));
        let mut inner = gpu.inner.lock();
        let _ = inner.flush();
        // crate::drivers::vga::print(format_args!("done\n"));
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
                    4 => 0xFFFF00FF, // Magenta
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

pub static GPU_IPC_CHANNEL: Mutex<Option<u32>> = Mutex::new(None);
pub fn gpu_ipc_listener(_arg: *const ()) {
    let chan_id = match *GPU_IPC_CHANNEL.lock() {
        Some(id) => id,
        None => {
            crate::klog!(
                crate::klog::Level::Warn,
                "[VirtIO-GPU] IPC listener: channel not available"
            );
            return;
        }
    };
    loop {
        if let Ok(msg) = crate::ipc::recv(chan_id) {
            if msg.len > 0 {
                match msg.data[0] {
                    1 => flush(),          // Flush
                    2 => draw_test_pattern(), // Draw test pattern
                    _ => {}
                }
            }
        }
        crate::process::scheduler::yield_now();
    }
}
