use core::fmt;
use spin::Mutex;
use lazy_static::lazy_static;
use volatile::Volatile;

const HEIGHT: usize = 25;
const WIDTH: usize = 80;

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Color {
    Black = 0, Blue, Green, Cyan, Red, Magenta, Brown, LightGray,
    DarkGray, LightBlue, LightGreen, LightCyan, LightRed, Pink, Yellow, White,
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
}

impl Writer {
    pub fn write_byte(&mut self, byte: u8) {
        match byte {
            b'\n' => self.newline(),
            b => {
                if self.col >= WIDTH { self.newline(); }
                let row = HEIGHT - 1;
                self.buffer.chars[row][self.col].write(ScreenChar { ascii: b, color: self.color });
                self.col += 1;
            }
        }
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

    fn clear_row(&mut self, row: usize) {
        let blank = ScreenChar { ascii: b' ', color: self.color };
        for col in 0..WIDTH {
            self.buffer.chars[row][col].write(blank);
        }
    }

    pub fn clear_screen(&mut self) {
        for row in 0..HEIGHT {
            self.clear_row(row);
        }
        self.col = 0;
    }

    pub fn set_color(&mut self, fg: Color, bg: Color) {
        self.color = ColorCode::new(fg, bg);
    }
}

impl fmt::Write for Writer {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for c in s.chars() {
            let byte = match c {
                '\n' => { self.newline(); continue; }
                '\r' => continue,
                // CP437 double-line box drawing
                '═' => 0xCD, '║' => 0xBA,
                '╔' => 0xC9, '╗' => 0xBB, '╚' => 0xC8, '╝' => 0xBC,
                // CP437 block elements
                '█' => 0xDB, '▀' => 0xDD, '▄' => 0xDC,
                // CP437 shade
                '░' => 0xB0, '▒' => 0xB1, '▓' => 0xB2,
                // CP437 line drawing (single)
                '┌' => 0xDA, '┐' => 0xBF, '└' => 0xC0, '┘' => 0xD9,
                '─' => 0xC4, '│' => 0xB3,
                _ if c.is_ascii() => c as u8,
                _ => { self.write_byte(0xFE); continue; }
            };
            self.write_byte(byte);
        }
        Ok(())
    }
}

lazy_static! {
    pub static ref WRITER: Mutex<Writer> = Mutex::new(Writer {
        col: 0,
        color: ColorCode::new(Color::Yellow, Color::Black),
        buffer: unsafe { &mut *(0xb8000 as *mut Buffer) },
    });
}

pub fn print(args: fmt::Arguments) {
    use core::fmt::Write;
    use x86_64::instructions::interrupts;
    interrupts::without_interrupts(|| {
        WRITER.lock().write_fmt(args).unwrap();
    });
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
