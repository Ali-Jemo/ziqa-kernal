use core::fmt;
use lazy_static::lazy_static;
use spin::Mutex;
use volatile::Volatile;

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
}

impl Writer {
    pub fn write_byte(&mut self, byte: u8) {
        if self.skipping_ansi {
            if byte >= 0x40 && byte <= 0x7E {
                // End of ANSI sequence
                self.skipping_ansi = false;
            }
            return;
        }

        if byte == 0x1b {
            self.skipping_ansi = true;
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
}

impl fmt::Write for Writer {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for c in s.chars() {
            let byte = match c {
                '\n' => {
                    self.newline();
                    continue;
                }
                '\r' => continue,
                // CP437 double-line box drawing
                '═' => 0xCD,
                '║' => 0xBA,
                '╔' => 0xC9,
                '╗' => 0xBB,
                '╚' => 0xC8,
                '╝' => 0xBC,
                // CP437 block elements
                '█' => 0xDB,
                '▀' => 0xDD,
                '▄' => 0xDC,
                // CP437 shade
                '░' => 0xB0,
                '▒' => 0xB1,
                '▓' => 0xB2,
                // CP437 line drawing (single)
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
