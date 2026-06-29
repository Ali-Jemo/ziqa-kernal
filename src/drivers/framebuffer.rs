/// Linear Framebuffer Driver for ZiqaKernel
///
/// Supports both 8-bit (Mode 13h, 320×200) and 32-bit XRGB8888 modes.
/// When available, uses the Zig blitter for high-performance operations.
use core::ptr::write_volatile;
use spin::Mutex;

/// Default framebuffer dimensions (Mode 13h compatible)
pub const WIDTH: usize = 320;
pub const HEIGHT: usize = 200;
pub const FB_SIZE: usize = WIDTH * HEIGHT; // pixel count

/// Pixel format
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat {
    /// 8-bit indexed color (Mode 13h)
    Indexed8,
    /// 32-bit XRGB8888 (DRM/linear framebuffer)
    XRGB8888,
}

pub struct Framebuffer {
    pub ptr: *mut u8,
    pub width: usize,
    pub height: usize,
    pub format: PixelFormat,
    /// Pitch in bytes (bytes per row)
    pub pitch: usize,
}

unsafe impl Send for Framebuffer {}
unsafe impl Sync for Framebuffer {}

impl Framebuffer {
    /// Create a new 8-bit indexed framebuffer (legacy Mode 13h)
    pub fn new(addr: u64) -> Self {
        Self {
            ptr: addr as *mut u8,
            width: WIDTH,
            height: HEIGHT,
            format: PixelFormat::Indexed8,
            pitch: WIDTH, // 1 byte per pixel
        }
    }

    /// Create a new 32-bit XRGB8888 framebuffer
    pub fn new_xrgb(addr: u64, width: usize, height: usize) -> Self {
        Self {
            ptr: addr as *mut u8,
            width,
            height,
            format: PixelFormat::XRGB8888,
            pitch: width * 4,
        }
    }

    /// Write a pixel directly to the framebuffer (8-bit mode)
    pub fn draw_pixel(&mut self, x: usize, y: usize, color: u8) {
        if x < self.width && y < self.height {
            match self.format {
                PixelFormat::Indexed8 => unsafe {
                    write_volatile(self.ptr.add(y * self.pitch + x), color);
                },
                PixelFormat::XRGB8888 => {
                    // Convert 8-bit to 32-bit grayscale
                    let c32 = (color as u32) | ((color as u32) << 8) | ((color as u32) << 16);
                    self.draw_pixel32(x, y, c32);
                }
            }
        }
    }

    /// Write a 32-bit XRGB pixel
    pub fn draw_pixel32(&mut self, x: usize, y: usize, color: u32) {
        if x < self.width && y < self.height {
            unsafe {
                let offset = y * self.pitch + x * 4;
                let ptr = self.ptr.add(offset) as *mut u32;
                core::ptr::write_volatile(ptr, color);
            }
        }
    }

    /// Fill the entire screen — uses Zig blitter for XRGB8888 mode
    pub unsafe fn fill(&mut self, color: u8) {
        match self.format {
            PixelFormat::Indexed8 => {
                for i in 0..(self.width * self.height) {
                    unsafe {
                        write_volatile(self.ptr.add(i), color);
                    }
                }
            }
            PixelFormat::XRGB8888 => {
                let c32 = (color as u32) | ((color as u32) << 8) | ((color as u32) << 16);
                self.fill32(c32);
            }
        }
    }

    /// Fill with 32-bit XRGB color — dispatches to Zig blitter
    pub unsafe fn fill32(&mut self, color: u32) {
        let total_bytes = self.pitch * self.height;
        #[cfg(feature = "zig-hotpaths")]
        crate::zig_ffi::clear(self.ptr, total_bytes, color);
        #[cfg(not(feature = "zig-hotpaths"))]
        {
            // The slice is in bytes; cast to u32 elements so we can write the
            // color directly. The framebuffer's mapped region is page-aligned
            // and at least 4-byte aligned, so this cast is well-defined.
            let p = core::slice::from_raw_parts_mut(self.ptr as *mut u32, total_bytes / 4);
            for px in p.iter_mut() {
                *px = color;
            }
        }
    }

    /// Fill a rectangle — dispatches to Zig blitter
    pub unsafe fn fill_rect(&mut self, x: u32, y: u32, w: u32, h: u32, color: u32) {
        #[cfg(feature = "zig-hotpaths")]
        crate::zig_ffi::fill_rect(self.ptr, self.pitch as u32, x, y, w, h, color);
        #[cfg(not(feature = "zig-hotpaths"))]
        {
            let pitch = self.pitch as u32;
            for row in y..y + h {
                let row_start = (row * pitch + x) as usize;
                let row_end = (row * pitch + x + w) as usize;
                let slice = core::slice::from_raw_parts_mut(self.ptr.add(row_start), (row_end - row_start) * 4);
                for px in slice.chunks_exact_mut(4) {
                    let bytes = color.to_le_bytes();
                    px.copy_from_slice(&bytes);
                }
            }
        }
    }

    /// Scroll up by N pixel lines — dispatches to Zig blitter
    pub unsafe fn scroll_up(&mut self, lines: u32, fill_color: u32) {
        #[cfg(feature = "zig-hotpaths")]
        crate::zig_ffi::scroll_up(
            self.ptr,
            self.pitch as u32,
            self.width as u32,
            self.height as u32,
            lines,
            fill_color,
        );
        #[cfg(not(feature = "zig-hotpaths"))]
        {
            let pitch = self.pitch;
            let width = self.width as usize;
            let height = self.height as usize;
            let bytes_per_pixel = if self.format == PixelFormat::XRGB8888 { 4 } else { 1 };
            let line_bytes = width * bytes_per_pixel;
            let src = self.ptr.add(lines as usize * pitch);
            let dst = self.ptr;
            let count = (height - lines as usize) * pitch;
            core::ptr::copy(src, dst, count);
            let fill_start = (height - lines as usize) * pitch;
            let fill_size = lines as usize * pitch;
            core::ptr::write_bytes(self.ptr.add(fill_start), fill_color as u8, fill_size);
        }
    }
}

pub static FB: Mutex<Option<Framebuffer>> = Mutex::new(None);

pub fn init(addr: u64) {
    *FB.lock() = Some(Framebuffer::new(addr));
}

/// Initialize a 32-bit XRGB framebuffer
pub fn init_xrgb(addr: u64, width: usize, height: usize) {
    *FB.lock() = Some(Framebuffer::new_xrgb(addr, width, height));
}

pub static BGA_FB_INFO: Mutex<Option<(u64, u32, u32, u32)>> = Mutex::new(None);

pub fn is_bga_available() -> bool {
    BGA_FB_INFO.lock().is_some()
}

pub fn get_bga_fb_info() -> Option<(u64, u32, u32, u32)> {
    *BGA_FB_INFO.lock()
}

fn vbe_write(index: u16, value: u16) {
    use x86_64::instructions::port::Port;
    let mut index_port = Port::new(0x01CE);
    let mut data_port = Port::new(0x01CF);
    unsafe {
        index_port.write(index);
        data_port.write(value);
    }
}

pub fn init_bga() -> bool {
    if let Some(device) = crate::drivers::pci::find_device(0x1234, 0x1111) {
        crate::println!("[BGA] Found Bochs Graphics Adapter at PCI {:02X}:{:02X}", device.bus, device.dev);
        
        crate::drivers::pci::enable_memory_space(device.address);
        
        let (phys_addr, is_io) = crate::drivers::pci::bar_address(device.bars[0]);
        if is_io || phys_addr == 0 {
            crate::println!("[BGA] Error: Invalid BAR0 address (io={}, addr={:#X})", is_io, phys_addr);
            return false;
        }

        if phys_addr < 0x1000000 {
            crate::println!("[BGA] Warning: BAR0 address {:#X} is very low, possible clobber risk!", phys_addr);
        }
        
        // ponytail: 1280x960 is the sweet spot — large enough to be usable,
        // small enough to not choke QEMU emulation (3.75MB framebuffer).
        let width = 1280u32;
        let height = 960u32;
        let bpp = 32u32;
        
        vbe_write(0, 0xB0C0);
        vbe_write(1, width as u16);
        vbe_write(2, height as u16);
        vbe_write(3, bpp as u16);
        vbe_write(4, 0x01 | 0x40);
        
        let virt_addr = crate::memory::paging::phys_offset().as_u64() + phys_addr;
        
        crate::println!("[BGA] Framebuffer configured at phys {:#X}, virt {:#X}", phys_addr, virt_addr);
            
        let fb_size_bytes = width * height * 4;
        let page_size = 4096u64;
        let pages_count = (fb_size_bytes as u64 + page_size - 1) / page_size;
        
        let mut mapper_guard = crate::memory::paging::KERNEL_MAPPER.lock();
        if let Some(mapper) = mapper_guard.as_mut() {
            let mut fa_guard = crate::memory::FRAME_ALLOCATOR.lock();
            if let Some(fa) = fa_guard.as_mut() {
                use x86_64::structures::paging::{Page, PhysFrame, Mapper, PageTableFlags, Size4KiB};
                use x86_64::{VirtAddr, PhysAddr};
                
                for i in 0..pages_count {
                    let page_virt = virt_addr + i * page_size;
                    let page_phys = phys_addr + i * page_size;
                    
                    let page: Page<Size4KiB> = Page::containing_address(VirtAddr::new(page_virt));
                    let frame: PhysFrame<Size4KiB> = PhysFrame::containing_address(PhysAddr::new(page_phys));
                    let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;
                    
                    if let Ok(flusher) = unsafe { mapper.mapper.map_to(page, frame, flags, fa) } {
                        flusher.flush();
                    }
                }
                crate::println!("[BGA] Mapped {} pages ({} MB)", pages_count, fb_size_bytes / (1024*1024));
            }
        }
        
        *BGA_FB_INFO.lock() = Some((virt_addr, width, height, bpp));
        
        crate::drivers::fb_console::init(virt_addr as *mut u8, width as usize, height as usize, (width * 4) as usize);
        
        true
    } else {
        false
    }
}
