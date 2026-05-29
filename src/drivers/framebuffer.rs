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
    pub fn fill(&mut self, color: u8) {
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
    pub fn fill32(&mut self, color: u32) {
        let total_bytes = self.pitch * self.height;
        crate::zig_ffi::clear(self.ptr, total_bytes, color);
    }

    /// Fill a rectangle — dispatches to Zig blitter
    pub fn fill_rect(&mut self, x: u32, y: u32, w: u32, h: u32, color: u32) {
        crate::zig_ffi::fill_rect(self.ptr, self.pitch as u32, x, y, w, h, color);
    }

    /// Scroll up by N pixel lines — dispatches to Zig blitter
    pub fn scroll_up(&mut self, lines: u32, fill_color: u32) {
        crate::zig_ffi::scroll_up(
            self.ptr,
            self.pitch as u32,
            self.width as u32,
            self.height as u32,
            lines,
            fill_color,
        );
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
