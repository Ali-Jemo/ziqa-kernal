use core::fmt;
use lazy_static::lazy_static;
use alloc::vec::Vec;
use spin::Mutex;
use volatile::Volatile;

const SCROLLBACK_CAP: usize = 1024;
const HEIGHT: usize = 25;
const WIDTH: usize = 80;

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Color {
    Black = 0,
    Blue,
    Green,
    Cyan,
    Red,
    Magenta,
    Brown,
    LightGray,
    DarkGray,
    LightBlue,
    LightGreen,
    LightCyan,
    LightRed,
    Pink,
    Yellow,
    White,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
struct ColorCode(u8);

impl ColorCode {
    fn new(fg: Color, bg: Color) -> ColorCode {
        ColorCode((bg as u8) << 4 | (fg as u8))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
struct ScreenChar {
    ascii: u8,
    color: ColorCode,
}

#[repr(transparent)]
struct Buffer {
    chars: [[Volatile<ScreenChar>; WIDTH]; HEIGHT],
}

pub struct Writer {
    col: usize,
    color: ColorCode,
    buffer: &'static mut Buffer,
    skipping_ansi: bool,
    skipping_ansi_bracket: bool,
    scrollback: Vec<[ScreenChar; WIDTH]>,
    scroll_offset: usize,
    status_text: [u8; WIDTH],
}

impl Writer {
    pub fn write_byte(&mut self, byte: u8) {
        if self.skipping_ansi {
            // ANSI escape sequences usually start with ESC [
            // If we just got ESC (0x1b), wait for '[' (0x5b)
            if self.skipping_ansi_bracket && (byte >= 0x40 && byte <= 0x7E) {
                // End of ANSI sequence
                self.skipping_ansi = false;
                self.skipping_ansi_bracket = false;
            } else if self.skipping_ansi && !self.skipping_ansi_bracket && byte == 0x5b {
                self.skipping_ansi_bracket = true;
            } else if !self.skipping_ansi_bracket && (byte >= 0x40 && byte <= 0x7E) {
                // Simple ESC sequence
                self.skipping_ansi = false;
            }
            return;
        }

        if byte == 0x1b {
            self.skipping_ansi = true;
            self.skipping_ansi_bracket = false;
            return;
        }

        match byte {
            b'\n' => self.newline(),
            8 | 127 => {
                if self.col > 0 {
                    self.col -= 1;
                }
            }
            b => {
                if self.col >= WIDTH {
                    self.newline();
                }
                let row = HEIGHT - 1;
                self.buffer.chars[row][self.col].write(ScreenChar {
                    ascii: b,
                    color: self.color,
                });
                self.col += 1;
            }
        }
        set_cursor_pos(HEIGHT - 1, self.col);
    }

    fn newline(&mut self) {
        if self.scroll_offset == 0 {
            let mut line = [ScreenChar { ascii: b' ', color: ColorCode(0x07) }; WIDTH];
            for c in 0..WIDTH {
                line[c] = self.buffer.chars[0][c].read();
            }
            if self.scrollback.len() >= SCROLLBACK_CAP {
                self.scrollback.remove(0);
            }
            self.scrollback.push(line);
        }
        for row in 1..HEIGHT {
            for col in 0..WIDTH {
                let character = self.buffer.chars[row][col].read();
                self.buffer.chars[row - 1][col].write(character);
            }
        }
        self.clear_row(HEIGHT - 1);
        self.col = 0;
    }

    pub fn clear_row(&mut self, row: usize) {
        let blank = ScreenChar {
            ascii: b' ',
            color: self.color,
        };
        for col in 0..WIDTH {
            self.buffer.chars[row][col].write(blank);
        }
    }

    pub fn write_char_at(&mut self, row: usize, col: usize, byte: u8, fg: Color, bg: Color) {
        if row < HEIGHT && col < WIDTH {
            let color = ColorCode::new(fg, bg);
            self.buffer.chars[row][col].write(ScreenChar { ascii: byte, color });
        }
    }

    pub fn read_char_at(&self, row: usize, col: usize) -> u8 {
        if row < HEIGHT && col < WIDTH {
            self.buffer.chars[row][col].read().ascii
        } else {
            b' '
        }
    }

    pub fn read_color_at(&self, row: usize, col: usize) -> (Color, Color) {
        if row < HEIGHT && col < WIDTH {
            let c = self.buffer.chars[row][col].read().color.0;
            let fg = match c & 0x0F {
                0 => Color::Black,
                1 => Color::Blue,
                2 => Color::Green,
                3 => Color::Cyan,
                4 => Color::Red,
                5 => Color::Magenta,
                6 => Color::Brown,
                7 => Color::LightGray,
                8 => Color::DarkGray,
                9 => Color::LightBlue,
                10 => Color::LightGreen,
                11 => Color::LightCyan,
                12 => Color::LightRed,
                13 => Color::Pink,
                14 => Color::Yellow,
                15 => Color::White,
                _ => Color::Black,
            };
            let bg = match (c >> 4) & 0x0F {
                0 => Color::Black,
                1 => Color::Blue,
                2 => Color::Green,
                3 => Color::Cyan,
                4 => Color::Red,
                5 => Color::Magenta,
                6 => Color::Brown,
                7 => Color::LightGray,
                8 => Color::DarkGray,
                9 => Color::LightBlue,
                10 => Color::LightGreen,
                11 => Color::LightCyan,
                12 => Color::LightRed,
                13 => Color::Pink,
                14 => Color::Yellow,
                15 => Color::White,
                _ => Color::Black,
            };
            (fg, bg)
        } else {
            (Color::Black, Color::Black)
        }
    }

    pub fn clear_screen(&mut self) {
        for row in 0..HEIGHT {
            self.clear_row(row);
        }
        self.col = 0;
        set_cursor_pos(HEIGHT - 1, 0);
    }

    pub fn set_color(&mut self, fg: Color, bg: Color) {
        self.color = ColorCode::new(fg, bg);
    }

    pub fn scroll_up(&mut self) {
        if self.scroll_offset < self.scrollback.len() {
            self.scroll_offset += 1;
            self.redraw_from_scrollback();
        }
    }

    pub fn scroll_down(&mut self) {
        if self.scroll_offset > 0 {
            self.scroll_offset -= 1;
            if self.scroll_offset == 0 {
                self.restore_terminal();
            } else {
                self.redraw_from_scrollback();
            }
        }
    }

    pub fn is_scrolled(&self) -> bool {
        self.scroll_offset > 0
    }

    fn redraw_from_scrollback(&mut self) {
        let total = self.scrollback.len();
        let offset = self.scroll_offset.min(total);
        for r in 0..HEIGHT {
            let sb_idx = if offset >= HEIGHT - r {
                offset - (HEIGHT - r)
            } else {
                0
            };
            if sb_idx > 0 && sb_idx <= total {
                let line = self.scrollback[total - sb_idx];
                for c in 0..WIDTH {
                    self.buffer.chars[r][c].write(line[c]);
                }
            } else if sb_idx == 0 && r < offset {
                for c in 0..WIDTH {
                    let ch = if c == 0 { b'~' } else { b' ' };
                    self.buffer.chars[r][c].write(ScreenChar { ascii: ch, color: ColorCode(0x08) });
                }
            } else {
                self.clear_row(r);
            }
        }
        let indicator = alloc::format!(" [SCROLLBACK {:>4}/{}] ", offset, total);
        let bytes = indicator.as_bytes();
        for c in 0..WIDTH {
            let ch = if c < bytes.len() { bytes[c] } else { b' ' };
            self.buffer.chars[HEIGHT - 1][c].write(ScreenChar { ascii: ch, color: ColorCode(0x1F) });
        }
        set_cursor_pos(HEIGHT - 1, 0);
    }

    pub fn restore_terminal(&mut self) {
        self.scroll_offset = 0;
        self.draw_status_bar();
        set_cursor_pos(HEIGHT - 1, self.col);
    }

    pub fn set_status_text(&mut self, text: &str) {
        let bytes = text.as_bytes();
        let n = bytes.len().min(WIDTH);
        self.status_text[..n].copy_from_slice(&bytes[..n]);
        if n < WIDTH {
            self.status_text[n] = 0;
        }
        if self.scroll_offset == 0 {
            self.draw_status_bar();
        }
    }

    fn draw_status_bar(&mut self) {
        let blank = ScreenChar { ascii: b' ', color: ColorCode(0x70) };
        for c in 0..WIDTH {
            self.buffer.chars[HEIGHT - 1][c].write(blank);
        }
        let mut len = 0;
        while len < WIDTH && len < self.status_text.len() && self.status_text[len] != 0 {
            let ch = self.status_text[len];
            self.buffer.chars[HEIGHT - 1][len].write(ScreenChar { ascii: ch, color: ColorCode(0x70) });
            len += 1;
        }
        set_cursor_pos(HEIGHT - 1, self.col);
    }
}

impl fmt::Write for Writer {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for c in s.chars() {
            let byte = match c {
                '\n' => {
                    self.newline();
                    continue;
                }
                '\r' => {
                    self.col = 0;
                    set_cursor_pos(HEIGHT - 1, self.col);
                    continue;
                }
                '═' => 0xCD,
                '║' => 0xBA,
                '╔' => 0xC9,
                '╗' => 0xBB,
                '╚' => 0xC8,
                '╝' => 0xBC,
                '█' => 0xDB,
                '▀' => 0xDD,
                '▄' => 0xDC,
                '░' => 0xB0,
                '▒' => 0xB1,
                '▓' => 0xB2,
                '┌' => 0xDA,
                '┐' => 0xBF,
                '└' => 0xC0,
                '┘' => 0xD9,
                '─' => 0xC4,
                '│' => 0xB3,
                _ if c.is_ascii() => c as u8,
                _ => {
                    self.write_byte(0xFE);
                    continue;
                }
            };
            self.write_byte(byte);
        }
        Ok(())
    }
}

lazy_static! {
    pub static ref WRITER: Mutex<Writer> = {
        let offset = crate::BOOT_INFO.lock()
            .as_ref()
            .map(|bi| bi.physical_memory_offset)
            .unwrap_or(0);
        Mutex::new(Writer {
            col: 0,
            color: ColorCode::new(Color::Yellow, Color::Black),
            buffer: unsafe { &mut *((offset + 0xb8000) as *mut Buffer) },
            skipping_ansi: false,
            skipping_ansi_bracket: false,
            scrollback: Vec::new(),
            scroll_offset: 0,
            status_text: [0; WIDTH],
        })
    };
}

pub fn print(args: fmt::Arguments) {
    use core::fmt::Write;
    use x86_64::instructions::interrupts;
    interrupts::without_interrupts(|| {
        WRITER.lock().write_fmt(args).unwrap();
    });
}

pub fn set_cursor_pos(row: usize, col: usize) {
    use x86_64::instructions::port::Port;
    let pos = row * WIDTH + col;
    unsafe {
        let mut addr: Port<u8> = Port::new(0x3D4);
        let mut data: Port<u8> = Port::new(0x3D5);
        addr.write(0x0F);
        data.write((pos & 0xFF) as u8);
        addr.write(0x0E);
        data.write(((pos >> 8) & 0xFF) as u8);
    }
}

pub fn hide_cursor() {
    use x86_64::instructions::port::Port;
    unsafe {
        let mut addr: Port<u8> = Port::new(0x3D4);
        let mut data: Port<u8> = Port::new(0x3D5);
        addr.write(0x0A);
        data.write(0x20);
    }
}

pub fn print_raw(bytes: &[u8]) {
    use x86_64::instructions::interrupts;
    interrupts::without_interrupts(|| {
        let mut writer = WRITER.lock();
        for &byte in bytes {
            writer.write_byte(byte);
        }
    });
}

pub fn clear_screen() {
    use x86_64::instructions::interrupts;
    interrupts::without_interrupts(|| {
        WRITER.lock().clear_screen();
    });
}

pub fn scroll_up() {
    use x86_64::instructions::interrupts;
    interrupts::without_interrupts(|| {
        WRITER.lock().scroll_up();
    });
}

pub fn scroll_down() {
    use x86_64::instructions::interrupts;
    interrupts::without_interrupts(|| {
        WRITER.lock().scroll_down();
    });
}

pub fn restore_terminal() {
    use x86_64::instructions::interrupts;
    interrupts::without_interrupts(|| {
        WRITER.lock().restore_terminal();
    });
}

pub fn is_scrolled() -> bool {
    use x86_64::instructions::interrupts;
    interrupts::without_interrupts(|| {
        WRITER.lock().is_scrolled()
    })
}

pub fn set_status_text(text: &str) {
    use x86_64::instructions::interrupts;
    interrupts::without_interrupts(|| {
        WRITER.lock().set_status_text(text);
    });
}

pub fn set_writer_color(fg: Color, bg: Color) {
    use x86_64::instructions::interrupts;
    interrupts::without_interrupts(|| {
        WRITER.lock().set_color(fg, bg);
    });
}
