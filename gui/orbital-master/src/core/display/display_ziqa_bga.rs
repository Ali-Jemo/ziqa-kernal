//! ZiqaKernel BGA direct framebuffer implementation
//
// This module is used when the "ziqa-bga-direct" feature IS enabled.
// It bypasses Redox DRM/graphics-ipc and uses SYS_FMAP to map the BGA framebuffer directly.

#![cfg(feature = "ziqa-bga-direct")]

use orbclient::image::{Image, ImageRef, ImageRoiMut};
use orbclient::rect::{Rect, RectEdge};
use orbclient::{Color, Renderer};
use std::io;

pub const SCALE_BASELINE: u32 = 160;

// NOTE: deliberately not `Copy`/`Clone` — holds a heap-allocated back buffer.
pub struct Framebuffer {
    /// Scanout target: the mmap'd BGA framebuffer. Written *only* by `flush`,
    /// so the hardware scanner never observes a half-drawn frame.
    ptr: *mut u32,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    /// Render target. All drawing (rect / roi / label blends) lands here; the
    /// compositor's `sync_rect` call copies the dirty region to `ptr` in a
    /// single pass per frame. This is the double-buffer that kills tearing on
    /// a directly-scanned framebuffer.
    backbuffer: Vec<Color>,
}

impl Framebuffer {
    fn image_mut(&mut self) -> ImageRef<'_> {
        ImageRef::from_data(self.width, self.height, &mut self.backbuffer[..])
    }

    /// Copy a local-coordinate region of the back buffer to the scanout
    /// framebuffer. The rect is clipped to framebuffer bounds; an empty rect
    /// is a no-op.
    fn flush(&mut self, rect: &Rect) {
        let x0 = rect.left().max(0) as u32;
        let y0 = rect.top().max(0) as u32;
        let x1 = (rect.left() + rect.width() as i32)
            .min(self.width as i32)
            .max(0) as u32;
        let y1 = (rect.top() + rect.height() as i32)
            .min(self.height as i32)
            .max(0) as u32;
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        let count = (x1 - x0) as usize;
        for y in y0..y1 {
            let start = (y * self.stride + x0) as usize;
            let src = &self.backbuffer[start..start + count];
            // SAFETY: `ptr` is the mmap'd BGA framebuffer, mapped for the full
            // stride*height span; `start + count` stays within it.
            unsafe {
                std::ptr::copy_nonoverlapping(
                    src.as_ptr(),
                    self.ptr.add(start) as *mut Color,
                    count,
                );
            }
        }
    }
}

pub struct Display {
    x: i32,
    y: i32,
    fb: Framebuffer,
    scale: u32,
    factored_scale: u32,
}

impl Display {
    pub fn new(x: i32, y: i32, fb: Framebuffer) -> Self {
        let scale = Self::calculate_scale(fb.height);
        let factored_scale = Self::calculate_factored(fb.height);
        Display {
            x,
            y,
            fb,
            scale,
            factored_scale,
        }
    }

    // Original signature: rect(&mut self, rect: &Rect, color: Color)
    pub fn rect(&mut self, rect: &Rect, color: Color) {
        self.fb.image_mut().rect(
            rect.left() - self.x,
            rect.top() - self.y,
            rect.width(),
            rect.height(),
            color,
        )
    }

    pub fn border_rect(&mut self, rect: &Rect, color: Color, thickness: u32) {
        self.rect(&rect.edge(thickness, 0, RectEdge::Top), color);
        self.rect(&rect.edge(thickness, 0, RectEdge::Bottom), color);
        self.rect(&rect.edge(thickness, 0, RectEdge::Left), color);
        self.rect(&rect.edge(thickness, 0, RectEdge::Right), color);
    }

    pub fn sync_rect(&mut self, _display_handle: &(), rect: &Rect) -> io::Result<()> {
        // `rect` is screen-absolute; translate into this display's local space
        // before flushing the back buffer to the scanout framebuffer.
        let local = Rect::new(
            rect.left() - self.x,
            rect.top() - self.y,
            rect.width(),
            rect.height(),
        );
        self.fb.flush(&local);
        Ok(())
    }

    pub fn resize_if_necessary(&mut self, _display_handle: &()) -> bool {
        // Fixed resolution on BGA
        false
    }

    pub fn screen_rect(&self) -> Rect {
        Rect::new(self.x, self.y, self.fb.width, self.fb.height)
    }

    pub fn scale(&self) -> u32 {
        self.scale
    }

    pub fn factored_scale(&self) -> u32 {
        self.factored_scale
    }

    fn calculate_scale(height: u32) -> u32 {
        if height >= 3200 {
            3
        } else if height >= 2400 {
            2
        } else if height >= 1600 {
            2
        } else {
            1
        }
    }

    fn calculate_factored(height: u32) -> u32 {
        const BASELINE_OFFSET: u32 = SCALE_BASELINE * 5;
        if height < BASELINE_OFFSET {
            return SCALE_BASELINE;
        }
        SCALE_BASELINE + ((height - BASELINE_OFFSET) / 10)
    }

    pub fn roi_mut(&mut self, rect: &Rect) -> ImageRoiMut<'_> {
        let x = self.x;
        let y = self.y;
        self.fb.image_mut().roi_mut(&Rect::new(
            rect.left() - x,
            rect.top() - y,
            rect.width(),
            rect.height(),
        ))
    }

    #[allow(dead_code)]
    pub fn move_cursor(&mut self, _display_handle: &(), _x: i32, _y: i32) -> io::Result<()> {
        // Hardware cursor not supported in this minimal backend
        Ok(())
    }

    #[allow(dead_code)]
    pub fn set_cursor(
        &mut self,
        _display_handle: &(),
        _hot_x: i32,
        _hot_y: i32,
        _cursor: &Image,
    ) -> io::Result<()> {
        Ok(())
    }
}

pub struct Displays {
    pub displays: Vec<Display>,
}

impl Displays {
    pub fn from_framebuffer(ptr: *mut u32, width: u32, height: u32) -> Self {
        let bg = Color::rgb(0x10, 0x18, 0x24);
        let mut fb = Framebuffer {
            ptr,
            width,
            height,
            stride: width,
            backbuffer: vec![bg; (width * height) as usize],
        };
        // Paint the initial background to the scanout so the screen is not
        // garbage before the first redraw + flush.
        fb.flush(&Rect::new(0, 0, width, height));
        let display = Display::new(0, 0, fb);
        Displays {
            displays: vec![display],
        }
    }
}
