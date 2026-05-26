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
    Black=0, Blue, Green, Cyan, Red, Magenta, Brown, LightGray,
    DarkGray, LightBlue, LightGreen, LightCyan, LightRed, Pink, Yellow, White,
}

#[derive(Clone, Copy)]
#[repr(transparent)]
struct ColorCode(u8);

impl ColorCode {
    fn new(fg: Color, bg: Color) -> Self { ColorCode((bg as u8) << 4 | fg as u8) }
}

#[derive(Clone, Copy)]
#[repr(C)]
struct ScreenChar { ascii: u8, color: ColorCode }

#[repr(transparent)]
struct Buffer { chars: [[Volatile<ScreenChar>; WIDTH]; HEIGHT] }

pub struct Writer {
    col: usize,
    color: ColorCode,
    buf: &'static mut Buffer,
}

impl Writer {
    pub fn write_byte(&mut self, byte: u8) {
        match byte {
            b'\n' => self.newline(),
            b => {
                if self.col >= WIDTH { self.newline(); }
                let row = HEIGHT - 1;
                self.buf.chars[row][self.col].write(ScreenChar { ascii: b, color: self.color });
                self.col += 1;
            }
        }
    }

    fn newline(&mut self) {
        for row in 1..HEIGHT {
            for col in 0..WIDTH {
                let c = self.buf.chars[row][col].read();
                self.buf.chars[row-1][col].write(c);
            }
        }
        let blank = ScreenChar { ascii: b' ', color: self.color };
        for col in 0..WIDTH { self.buf.chars[HEIGHT-1][col].write(blank); }
        self.col = 0;
    }

    pub fn set_color(&mut self, fg: Color, bg: Color) {
        self.color = ColorCode::new(fg, bg);
    }
}

impl fmt::Write for Writer {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for byte in s.bytes() {
            match byte {
                0x20..=0x7e | b'\n' => self.write_byte(byte),
                _ => self.write_byte(0xfe),
            }
        }
        Ok(())
    }
}

lazy_static! {
    pub static ref WRITER: Mutex<Writer> = Mutex::new(Writer {
        col: 0,
        color: ColorCode::new(Color::LightGreen, Color::Black),
        buf: unsafe { &mut *(0xb8000 as *mut Buffer) },
    });
}

#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    use core::fmt::Write;
    use x86_64::instructions::interrupts;
    interrupts::without_interrupts(|| { WRITER.lock().write_fmt(args).unwrap(); });
}

#[macro_export]
macro_rules! vga_print {
    ($($arg:tt)*) => ($crate::drivers::vga::_print(format_args!($($arg)*)));
}

#[macro_export]
macro_rules! vga_println {
    () => ($crate::vga_print!("\n"));
    ($fmt:expr) => ($crate::vga_print!(concat!($fmt, "\n")));
    ($fmt:expr, $($arg:tt)*) => ($crate::vga_print!(concat!($fmt, "\n"), $($arg)*));
}
