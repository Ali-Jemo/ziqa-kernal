//! Ziqa-OrbClient: GUI Library for Ziqa Applications

pub struct Window {
    pub id: usize,
    pub width: u32,
    pub height: u32,
}

impl Window {
    pub fn new(_x: i32, _y: i32, w: u32, h: u32, title: &str) -> Self {
        crate::println!("[OrbClient] Opening window '{}' ({}x{})", title, w, h);
        Self { id: 1, width: w, height: h }
    }
    
    pub fn draw_rect(&self, x: i32, y: i32, w: u32, h: u32, _color: u32) {
        crate::println!("[OrbClient] Sending draw command (fill_rect at {},{} {}x{})", x, y, w, h);
    }

    pub fn flush(&self) {
        crate::println!("[OrbClient] Flushing window buffers");
    }
}

pub fn create_window(x: i32, y: i32, w: u32, h: u32, title: &str) -> Window {
    Window::new(x, y, w, h, title)
}
