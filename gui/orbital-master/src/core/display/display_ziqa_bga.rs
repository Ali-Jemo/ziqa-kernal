//! ZiqaKernel BGA direct framebuffer implementation
//
// This module is used when the "ziqa-bga-direct" feature IS enabled.
// It bypasses Redox DRM/graphics-ipc and uses SYS_FMAP to map the BGA framebuffer directly.

#![cfg(feature = "ziqa-bga-direct")]

use orbclient::image::{Image, ImageRef, ImageRoiMut};
use orbclient::rect::{Rect, RectEdge};
use orbclient::{Color, Renderer};
use std::slice;
use std::io;

pub const SCALE_BASELINE: u32 = 160;

#[derive(Clone, Copy)]
pub struct Framebuffer {
    pub ptr: *mut u32,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
}

impl Framebuffer {
    fn image_mut(&mut self) -> ImageRef<'_> {
        let data = unsafe {
            slice::from_raw_parts_mut(
                self.ptr as *mut orbclient::Color,
                (self.stride * self.height) as usize,
            )
        };
        ImageRef::from_data(self.width, self.height, data)
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
        Display { x, y, fb, scale, factored_scale }
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

    pub fn sync_rect(&mut self, _display_handle: &(), _rect: &Rect) -> io::Result<()> {
        // BGA framebuffer is scanned directly by hardware — no sync needed
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

    pub fn move_cursor(&mut self, _display_handle: &(), _x: i32, _y: i32) -> io::Result<()> {
        // Hardware cursor not supported in this minimal backend
        Ok(())
    }

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
        let fb = Framebuffer {
            ptr,
            width,
            height,
            stride: width,
        };
        let pixels = unsafe { slice::from_raw_parts_mut(ptr, (width * height) as usize) };
        pixels.fill(0x0010_1824);
        let display = Display::new(0, 0, fb);
        Displays {
            displays: vec![display],
        }
    }
}