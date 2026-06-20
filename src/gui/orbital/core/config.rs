//! Desktop Configuration for Ziqa-Orbital

#[derive(Clone, Copy, Debug)]
pub struct Color {
    pub data: u32,
}

impl Color {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self {
            data: 0xFF000000 | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32),
        }
    }
}

pub struct Config {
    pub background: Color,
    pub bar_color: Color,
    pub text_color: Color,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            background: Color::rgb(0, 0, 0),
            bar_color: Color::rgb(27, 27, 27),
            text_color: Color::rgb(231, 231, 231),
        }
    }
}