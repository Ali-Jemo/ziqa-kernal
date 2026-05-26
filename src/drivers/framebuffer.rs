/// Linear Framebuffer Driver for ZiqaKernel
///
/// Provides a raw memory-mapped interface to the graphics display.
/// This is the foundation for pixel-based drawing (e.g., DOOM).

use core::ptr::{write_volatile};
use spin::Mutex;

pub const WIDTH: usize = 320;
pub const HEIGHT: usize = 200;
pub const FB_SIZE: usize = WIDTH * HEIGHT; // 8-bit color

pub struct Framebuffer {
    pub ptr: *mut u8,
}

unsafe impl Send for Framebuffer {}
unsafe impl Sync for Framebuffer {}

impl Framebuffer {
    pub fn new(addr: u64) -> Self {
        Self { ptr: addr as *mut u8 }
    }

    /// Write a pixel directly to the framebuffer
    pub fn draw_pixel(&mut self, x: usize, y: usize, color: u8) {
        if x < WIDTH && y < HEIGHT {
            unsafe {
                write_volatile(self.ptr.add(y * WIDTH + x), color);
            }
        }
    }

    /// Fill the screen with a solid color
    pub fn fill(&mut self, color: u8) {
        for i in 0..FB_SIZE {
            unsafe {
                write_volatile(self.ptr.add(i), color);
            }
        }
    }
}

pub static FB: Mutex<Option<Framebuffer>> = Mutex::new(None);

pub fn init(addr: u64) {
    *FB.lock() = Some(Framebuffer::new(addr));
}
