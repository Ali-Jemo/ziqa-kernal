use core::fmt;
use core::sync::atomic::{AtomicBool, Ordering};
use spin::Mutex;

pub static GPU_CONSOLE_ACTIVE: AtomicBool = AtomicBool::new(false);
pub static FB_CONSOLE: Mutex<Option<FbConsole>> = Mutex::new(None);

pub struct FbConsole {
    fb_ptr: *mut u8,
    width: usize,
    height: usize,
    pitch: usize,
    cols: usize,
    rows: usize,
    cursor_x: usize,
    cursor_y: usize,
    fg_color: u32,
    bg_color: u32,
}

// SAFETY: Mutex makes the inner driver thread-safe.
unsafe impl Send for FbConsole {}
unsafe impl Sync for FbConsole {}

// A minimal 8x16 font (printable ASCII + basic box drawing).
// For simplicity, we just use a block for non-printable/unimplemented characters.
const FONT_8X16: [[u8; 16]; 256] = {
    let mut font = [[0xFF; 16]; 256];
    
    // Space
    font[32] = [0x00; 16];
    
    // Some basic characters to make it readable...
    // '0'-'9'
    font[48] = [0,0,56,68,68,68,68,68,68,68,68,68,56,0,0,0]; // 0
    font[49] = [0,0,16,48,16,16,16,16,16,16,16,16,56,0,0,0]; // 1
    font[50] = [0,0,56,68,4,4,4,8,16,32,64,64,124,0,0,0]; // 2
    font[51] = [0,0,56,68,4,4,24,4,4,4,4,68,56,0,0,0]; // 3
    font[52] = [0,0,8,24,40,72,136,136,252,8,8,8,8,0,0,0]; // 4
    font[53] = [0,0,124,64,64,64,120,4,4,4,4,68,56,0,0,0]; // 5
    font[54] = [0,0,24,32,64,64,120,68,68,68,68,68,56,0,0,0]; // 6
    font[55] = [0,0,124,4,4,8,8,16,16,32,32,32,32,0,0,0]; // 7
    font[56] = [0,0,56,68,68,68,56,68,68,68,68,68,56,0,0,0]; // 8
    font[57] = [0,0,56,68,68,68,68,68,60,4,4,8,48,0,0,0]; // 9

    // 'A'-'Z' (Simplified)
    font[65] = [0,0,16,40,68,68,68,124,68,68,68,68,68,0,0,0]; // A
    font[66] = [0,0,120,68,68,68,120,68,68,68,68,68,120,0,0,0]; // B
    font[67] = [0,0,56,68,64,64,64,64,64,64,64,68,56,0,0,0]; // C
    font[68] = [0,0,120,68,68,68,68,68,68,68,68,68,120,0,0,0]; // D
    font[69] = [0,0,124,64,64,64,120,64,64,64,64,64,124,0,0,0]; // E
    font[70] = [0,0,124,64,64,64,120,64,64,64,64,64,64,0,0,0]; // F
    font[71] = [0,0,56,68,64,64,64,92,68,68,68,68,56,0,0,0]; // G
    font[72] = [0,0,68,68,68,68,124,68,68,68,68,68,68,0,0,0]; // H
    font[73] = [0,0,56,16,16,16,16,16,16,16,16,16,56,0,0,0]; // I
    font[74] = [0,0,28,8,8,8,8,8,8,8,8,72,48,0,0,0]; // J
    font[75] = [0,0,68,72,80,96,64,96,80,72,68,68,68,0,0,0]; // K
    font[76] = [0,0,64,64,64,64,64,64,64,64,64,64,124,0,0,0]; // L
    font[77] = [0,0,136,216,168,168,136,136,136,136,136,136,136,0,0,0]; // M
    font[78] = [0,0,132,196,164,164,148,148,148,136,136,136,136,0,0,0]; // N
    font[79] = [0,0,56,68,68,68,68,68,68,68,68,68,56,0,0,0]; // O
    font[80] = [0,0,120,68,68,68,120,64,64,64,64,64,64,0,0,0]; // P
    font[81] = [0,0,56,68,68,68,68,68,68,84,72,68,58,0,0,0]; // Q
    font[82] = [0,0,120,68,68,68,120,96,80,72,68,68,68,0,0,0]; // R
    font[83] = [0,0,56,68,64,64,56,4,4,4,4,68,56,0,0,0]; // S
    font[84] = [0,0,124,16,16,16,16,16,16,16,16,16,16,0,0,0]; // T
    font[85] = [0,0,68,68,68,68,68,68,68,68,68,68,56,0,0,0]; // U
    font[86] = [0,0,68,68,68,68,68,68,68,40,40,16,16,0,0,0]; // V
    font[87] = [0,0,136,136,136,136,136,168,168,168,80,80,80,0,0,0]; // W
    font[88] = [0,0,68,68,40,16,16,16,40,68,68,68,68,0,0,0]; // X
    font[89] = [0,0,68,68,68,40,16,16,16,16,16,16,16,0,0,0]; // Y
    font[90] = [0,0,124,4,8,16,32,64,64,124,0,0,0,0,0,0]; // Z
    
    // a-z
    font[97] = [0,0,0,0,0,56,4,60,68,68,68,60,0,0,0,0]; // a
    font[98] = [0,0,64,64,64,120,68,68,68,68,68,120,0,0,0,0]; // b
    font[99] = [0,0,0,0,0,56,68,64,64,64,68,56,0,0,0,0]; // c
    font[100] = [0,0,4,4,4,60,68,68,68,68,68,60,0,0,0,0]; // d
    font[101] = [0,0,0,0,0,56,68,124,64,64,68,56,0,0,0,0]; // e
    font[102] = [0,0,24,36,32,120,32,32,32,32,32,32,0,0,0,0]; // f
    font[103] = [0,0,0,0,0,60,68,68,68,68,60,4,68,56,0,0]; // g
    font[104] = [0,0,64,64,64,120,68,68,68,68,68,68,0,0,0,0]; // h
    font[105] = [0,0,16,0,0,48,16,16,16,16,16,56,0,0,0,0]; // i
    font[106] = [0,0,8,0,0,24,8,8,8,8,8,8,72,48,0,0]; // j
    font[107] = [0,0,64,64,64,72,80,96,80,72,68,68,0,0,0,0]; // k
    font[108] = [0,0,48,16,16,16,16,16,16,16,16,56,0,0,0,0]; // l
    font[109] = [0,0,0,0,0,120,148,148,148,148,148,148,0,0,0,0]; // m
    font[110] = [0,0,0,0,0,120,68,68,68,68,68,68,0,0,0,0]; // n
    font[111] = [0,0,0,0,0,56,68,68,68,68,68,56,0,0,0,0]; // o
    font[112] = [0,0,0,0,0,120,68,68,68,68,120,64,64,64,0,0]; // p
    font[113] = [0,0,0,0,0,60,68,68,68,68,60,4,4,4,0,0]; // q
    font[114] = [0,0,0,0,0,104,112,64,64,64,64,64,0,0,0,0]; // r
    font[115] = [0,0,0,0,0,60,64,56,4,68,56,0,0,0,0,0]; // s
    font[116] = [0,0,32,32,120,32,32,32,32,32,36,24,0,0,0,0]; // t
    font[117] = [0,0,0,0,0,68,68,68,68,68,68,60,0,0,0,0]; // u
    font[118] = [0,0,0,0,0,68,68,68,68,40,40,16,0,0,0,0]; // v
    font[119] = [0,0,0,0,0,136,136,136,168,168,80,80,0,0,0,0]; // w
    font[120] = [0,0,0,0,0,68,40,16,16,40,68,68,0,0,0,0]; // x
    font[121] = [0,0,0,0,0,68,68,68,68,60,4,68,56,0,0,0]; // y
    font[122] = [0,0,0,0,0,124,8,16,32,64,124,0,0,0,0,0]; // z

    // Punctuation
    font[33] = [0,0,16,16,16,16,16,16,16,0,16,16,0,0,0,0]; // !
    font[34] = [0,0,40,40,40,0,0,0,0,0,0,0,0,0,0,0]; // "
    font[35] = [0,0,40,40,254,40,40,254,40,40,0,0,0,0,0,0]; // #
    font[36] = [0,16,124,146,146,124,18,18,124,16,0,0,0,0,0,0]; // $
    font[37] = [0,0,194,196,8,16,32,32,64,138,134,0,0,0,0,0]; // %
    font[38] = [0,0,56,68,68,56,84,72,84,72,52,0,0,0,0,0]; // &
    font[39] = [0,0,16,16,16,0,0,0,0,0,0,0,0,0,0,0]; // '
    font[40] = [0,0,8,16,32,32,32,32,32,32,16,8,0,0,0,0]; // (
    font[41] = [0,0,32,16,8,8,8,8,8,8,16,32,0,0,0,0]; // )
    font[42] = [0,0,0,16,84,56,16,56,84,16,0,0,0,0,0,0]; // *
    font[43] = [0,0,0,16,16,16,254,16,16,16,0,0,0,0,0,0]; // +
    font[44] = [0,0,0,0,0,0,0,0,0,0,16,16,8,0,0,0]; // ,
    font[45] = [0,0,0,0,0,0,254,0,0,0,0,0,0,0,0,0]; // -
    font[46] = [0,0,0,0,0,0,0,0,0,0,16,16,0,0,0,0]; // .
    font[47] = [0,0,2,4,8,16,32,64,128,0,0,0,0,0,0,0]; // /
    font[58] = [0,0,0,0,16,16,0,0,16,16,0,0,0,0,0,0]; // :
    font[59] = [0,0,0,0,16,16,0,0,16,16,8,0,0,0,0,0]; // ;
    font[60] = [0,0,8,16,32,64,32,16,8,0,0,0,0,0,0,0]; // <
    font[61] = [0,0,0,0,254,0,0,254,0,0,0,0,0,0,0,0]; // =
    font[62] = [0,0,32,16,8,4,8,16,32,0,0,0,0,0,0,0]; // >
    font[63] = [0,0,124,2,2,28,32,32,0,32,32,0,0,0,0,0]; // ?
    font[64] = [0,0,124,130,154,170,170,158,128,124,0,0,0,0,0,0]; // @
    font[91] = [0,0,60,32,32,32,32,32,32,32,32,32,60,0,0,0]; // [
    font[92] = [0,0,128,64,32,16,8,4,2,0,0,0,0,0,0,0]; // \
    font[93] = [0,0,120,8,8,8,8,8,8,8,8,8,120,0,0,0]; // ]
    font[94] = [0,0,16,40,68,0,0,0,0,0,0,0,0,0,0,0]; // ^
    font[95] = [0,0,0,0,0,0,0,0,0,0,0,0,0,255,0,0]; // _
    font[96] = [0,0,32,16,8,0,0,0,0,0,0,0,0,0,0,0]; // `
    font[123] = [0,0,24,32,32,32,64,32,32,32,24,0,0,0,0,0]; // {
    font[124] = [0,0,16,16,16,16,16,16,16,16,16,16,16,0,0,0]; // |
    font[125] = [0,0,48,8,8,8,4,8,8,8,48,0,0,0,0,0]; // }
    font[126] = [0,0,0,0,0,172,210,0,0,0,0,0,0,0,0,0]; // ~

    // Basic Box Drawing
    font[0xCD] = [0,0,0,0,0,0,0,255,255,0,0,0,0,0,0,0]; // ═
    font[0xBA] = [0,0,24,24,24,24,24,24,24,24,24,24,24,24,24,24]; // ║
    font[0xC9] = [0,0,0,0,0,0,0,127,127,24,24,24,24,24,24,24]; // ╔
    font[0xBB] = [0,0,0,0,0,0,0,254,254,24,24,24,24,24,24,24]; // ╗
    font[0xC8] = [24,24,24,24,24,24,24,127,127,0,0,0,0,0,0,0]; // ╚
    font[0xBC] = [24,24,24,24,24,24,24,254,254,0,0,0,0,0,0,0]; // ╝
    font[0xDB] = [255; 16]; // █

    font
};

fn vga_color(index: u8) -> u32 {
    match index & 0x0F {
        0x0 => 0x00000000,
        0x1 => 0x000000AA,
        0x2 => 0x0000AA00,
        0x3 => 0x0000AAAA,
        0x4 => 0x00AA0000,
        0x5 => 0x00AA00AA,
        0x6 => 0x00AA5500,
        0x7 => 0x00AAAAAA,
        0x8 => 0x00555555,
        0x9 => 0x005555FF,
        0xA => 0x0055FF55,
        0xB => 0x0055FFFF,
        0xC => 0x00FF5555,
        0xD => 0x00FF55FF,
        0xE => 0x00FFFF55,
        _ => 0x00FFFFFF,
    }
}

pub fn draw_cell(col: usize, row: usize, byte: u8, attr: u8) {
    if let Some(console) = FB_CONSOLE.lock().as_ref() {
        console.draw_cell(col, row, byte, attr);
    }
}

pub fn flush() {
    crate::drivers::virtio_gpu::flush();
}

pub fn init(fb_ptr: *mut u8, width: usize, height: usize, pitch: usize) {
    let cols = width / 8;
    let rows = height / 16;
    
    let mut console = FbConsole {
        fb_ptr,
        width,
        height,
        pitch,
        cols,
        rows,
        cursor_x: 0,
        cursor_y: 0,
        fg_color: 0x00C0CAF5, // Tokyo Night Light Blue
        bg_color: 0x001A1B26, // Tokyo Night Dark Navy
    };
    
    console.clear_screen();
    
    *FB_CONSOLE.lock() = Some(console);
    GPU_CONSOLE_ACTIVE.store(true, Ordering::SeqCst);
}

impl FbConsole {
    pub fn clear_screen(&mut self) {
        for y in 0..self.height {
            let row_offset = y * self.pitch;
            for x in 0..self.width {
                unsafe {
                    let ptr = self.fb_ptr.add(row_offset + x * 4) as *mut u32;
                    core::ptr::write_volatile(ptr, self.bg_color);
                }
            }
        }
        self.cursor_x = 0;
        self.cursor_y = 0;
        crate::drivers::virtio_gpu::flush();
    }
    
    pub fn set_color(&mut self, fg: u32, bg: u32) {
        self.fg_color = fg;
        self.bg_color = bg;
    }

    pub fn draw_cell(&self, col: usize, row: usize, byte: u8, attr: u8) {
        let fg = vga_color(attr & 0x0F);
        let bg = vga_color((attr >> 4) & 0x0F);
        self.draw_char_colored(col, row, byte, fg, bg);
    }

    fn draw_char_colored(&self, col: usize, row: usize, byte: u8, fg: u32, bg: u32) {
        let bitmap = &FONT_8X16[byte as usize];
        let px_x = col * 8;
        let px_y = row * 16;

        for (y, &row_bits) in bitmap.iter().enumerate() {
            let fb_y = px_y + y;
            if fb_y >= self.height { break; }

            let row_offset = fb_y * self.pitch;
            for x in 0..8 {
                let fb_x = px_x + x;
                if fb_x >= self.width { break; }

                let color = if (row_bits & (1 << (7 - x))) != 0 { fg } else { bg };
                unsafe {
                    let ptr = self.fb_ptr.add(row_offset + fb_x * 4) as *mut u32;
                    core::ptr::write_volatile(ptr, color);
                }
            }
        }
    }

    pub fn write_byte(&mut self, byte: u8) {
        match byte {
            b'\n' => self.newline(),
            b'\r' => {
                self.cursor_x = 0;
            }
            8 | 127 => {
                if self.cursor_x > 0 {
                    self.cursor_x -= 1;
                    self.draw_char(self.cursor_x, self.cursor_y, b' ');
                }
            }
            b => {
                if self.cursor_x >= self.cols {
                    self.newline();
                }
                self.draw_char(self.cursor_x, self.cursor_y, b);
                self.cursor_x += 1;
            }
        }
    }
    
    fn draw_char(&self, col: usize, row: usize, byte: u8) {
        self.draw_char_colored(col, row, byte, self.fg_color, self.bg_color);
    }
    
    fn newline(&mut self) {
        self.cursor_x = 0;
        self.cursor_y += 1;
        
        if self.cursor_y >= self.rows {
            // Scroll up
            self.cursor_y = self.rows - 1;
            
            let scroll_lines = 16;
            let dest = self.fb_ptr;
            let src = unsafe { self.fb_ptr.add(scroll_lines * self.pitch) };
            let count = (self.height - scroll_lines) * self.pitch;
            
            unsafe {
                core::ptr::copy(src, dest, count);
            }
            
            // Clear last line
            let clear_start = (self.height - scroll_lines) * self.pitch;
            for y in 0..scroll_lines {
                let row_offset = clear_start + y * self.pitch;
                for x in 0..self.width {
                    unsafe {
                        let ptr = self.fb_ptr.add(row_offset + x * 4) as *mut u32;
                        core::ptr::write_volatile(ptr, self.bg_color);
                    }
                }
            }
        }
        
        // We only flush periodically or on explicit command for performance,
        // but for now let's flush on newline.
        crate::drivers::virtio_gpu::flush();
    }
}

impl fmt::Write for FbConsole {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for c in s.chars() {
            let byte = match c {
                '\n' => b'\n',
                '\r' => b'\r',
                '═' => 0xCD,
                '║' => 0xBA,
                '╔' => 0xC9,
                '╗' => 0xBB,
                '╚' => 0xC8,
                '╝' => 0xBC,
                '█' => 0xDB,
                _ if c.is_ascii() => c as u8,
                _ => 0xDB, // Box block for missing chars
            };
            self.write_byte(byte);
        }
        // Force flush after writing a string to ensure it's visible
        crate::drivers::virtio_gpu::flush();
        Ok(())
    }
}
