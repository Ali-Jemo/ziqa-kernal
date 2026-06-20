//! Bitmap font rendering for the GPU framebuffer.
//!
//! Provides an 8×16 font glyph atlas and a `GlyphCanvas` that draws
//! text and primitive shapes into an RGBA pixel buffer suitable for
//! SHM surfaces or direct framebuffer blits.

/// 8×16 font bitmap — 256 ASCII entries, each a 16-byte row mask.
/// A set bit (1) = foreground pixel; cleared bit (0) = background.
pub const FONT_8X16: [[u8; 16]; 256] = {
    let mut font = [[0xFF; 16]; 256];

    // Space
    font[32] = [0x00; 16];

    // Digits '0'-'9'
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

    // 'A'-'Z'
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

    // Basic Box Drawing (code page 437 positions)
    font[0xCD] = [0,0,0,0,0,0,0,255,255,0,0,0,0,0,0,0]; // ═
    font[0xBA] = [0,0,24,24,24,24,24,24,24,24,24,24,24,24,24,24]; // ║
    font[0xC9] = [0,0,0,0,0,0,0,127,127,24,24,24,24,24,24,24]; // ╔
    font[0xBB] = [0,0,0,0,0,0,0,254,254,24,24,24,24,24,24,24]; // ╗
    font[0xC8] = [24,24,24,24,24,24,24,127,127,0,0,0,0,0,0,0]; // ╚
    font[0xBC] = [24,24,24,24,24,24,24,254,254,0,0,0,0,0,0,0]; // ╝
    font[0xDB] = [255; 16]; // █ (full block)

    font
};

pub const FONT_W: usize = 8;
pub const FONT_H: usize = 16;

/// A pixel canvas backed by an RGBA buffer.
/// Used to render text and shapes onto SHM surfaces.
pub struct GlyphCanvas<'a> {
    pub buf: &'a mut [u8],
    pub width: u32,
    pub height: u32,
}

impl<'a> GlyphCanvas<'a> {
    /// Create a new canvas wrapping an RGBA pixel buffer.
    /// `buf` must be `width * height * 4` bytes long.
    pub fn new(buf: &'a mut [u8], width: u32, height: u32) -> Self {
        GlyphCanvas { buf, width, height }
    }

    /// Pack a 32-bit RGBA colour from (r,g,b) components.
    #[inline]
    pub fn rgb(r: u8, g: u8, b: u8) -> u32 {
        0xFF00_0000 | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
    }

    /// Pack an ARGB colour where A is in the top 8 bits.
    #[inline]
    pub fn argb(a: u8, r: u8, g: u8, b: u8) -> u32 {
        ((a as u32) << 24) | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
    }

    #[inline]
    fn offset(&self, x: u32, y: u32) -> usize {
        (y as usize * self.width as usize + x as usize) * 4
    }

    /// Set a single pixel at (x, y) to `color` (RGBA).
    #[inline]
    pub fn pixel(&mut self, x: u32, y: u32, color: u32) {
        if x < self.width && y < self.height {
            let off = self.offset(x, y);
            if off + 4 <= self.buf.len() {
                self.buf[off..off + 4].copy_from_slice(&color.to_le_bytes());
            }
        }
    }

    /// Fill a rectangle with `color`.
    pub fn fill_rect(&mut self, x: u32, y: u32, w: u32, h: u32, color: u32) {
        let x0 = x.min(self.width);
        let y0 = y.min(self.height);
        let x1 = (x + w).min(self.width);
        let y1 = (y + h).min(self.height);
        for row in y0..y1 {
            let off = self.offset(x0, row);
            for col in x0..x1 {
                let p = off + (col - x0) as usize * 4;
                if p + 4 <= self.buf.len() {
                    self.buf[p..p + 4].copy_from_slice(&color.to_le_bytes());
                }
            }
        }
    }

    /// Draw a rectangle outline (1 px wide).
    pub fn draw_rect(&mut self, x: u32, y: u32, w: u32, h: u32, color: u32) {
        if w == 0 || h == 0 { return; }
        self.hline(x, y, w, color);
        if h > 1 {
            self.hline(x, y + h - 1, w, color);
            self.vline(x, y, h, color);
            if w > 1 {
                self.vline(x + w - 1, y, h, color);
            }
        }
    }

    /// Horizontal line.
    pub fn hline(&mut self, x: u32, y: u32, w: u32, color: u32) {
        let y0 = y.min(self.height - 1);
        let x0 = x.min(self.width);
        let x1 = (x + w).min(self.width);
        let off = self.offset(x0, y0);
        for col in x0..x1 {
            let p = off + (col - x0) as usize * 4;
            if p + 4 <= self.buf.len() {
                self.buf[p..p + 4].copy_from_slice(&color.to_le_bytes());
            }
        }
    }

    /// Vertical line.
    pub fn vline(&mut self, x: u32, y: u32, h: u32, color: u32) {
        let x0 = x.min(self.width - 1);
        let y0 = y.min(self.height);
        let y1 = (y + h).min(self.height);
        for row in y0..y1 {
            let p = self.offset(x0, row);
            if p + 4 <= self.buf.len() {
                self.buf[p..p + 4].copy_from_slice(&color.to_le_bytes());
            }
        }
    }

    /// Draw a single 8×16 glyph for byte `ch` at pixel (x, y).
    pub fn draw_char(&mut self, x: u32, y: u32, ch: u8, fg: u32, bg: u32) {
        let glyph = &FONT_8X16[ch as usize];
        for row in 0..16 {
            let row_bits = glyph[row];
            for col in 0..8 {
                let px = if (row_bits & (1 << (7 - col))) != 0 { fg } else { bg };
                self.pixel(x + col as u32, y + row as u32, px);
            }
        }
    }

    /// Draw a string at pixel (x, y).
    /// Returns the pixel x of the end of the drawn text.
    pub fn draw_string(&mut self, mut x: u32, y: u32, s: &str, fg: u32, bg: u32) -> u32 {
        for b in s.bytes() {
            if b == b'\n' {
                // newline not handled in this simple render path
                continue;
            }
            self.draw_char(x, y, b, fg, bg);
            x += FONT_W as u32;
            if x + FONT_W as u32 > self.width {
                break;
            }
        }
        x
    }

    /// Draw a character with VGA-style colour attribute.
    /// `attr` low nibble = foreground, high nibble = background.
    pub fn draw_char_vga(&mut self, x: u32, y: u32, ch: u8, attr: u8) {
        let fg = vga_pixel(attr & 0x0F);
        let bg = vga_pixel((attr >> 4) & 0x0F);
        self.draw_char(x, y, ch, fg, bg);
    }

    /// Clear the entire canvas to `color`.
    pub fn clear(&mut self, color: u32) {
        self.fill_rect(0, 0, self.width, self.height, color);
    }

    /// Blit from another canvas (or buffer) at the given offset.
    pub fn blit(&mut self, src: &[u8], src_w: u32, src_h: u32, dst_x: u32, dst_y: u32) {
        let x0 = dst_x.min(self.width);
        let y0 = dst_y.min(self.height);
        for row in 0..src_h {
            let sy = y0 + row;
            if sy >= self.height { break; }
            let src_off = row as usize * src_w as usize * 4;
            let dst_off = self.offset(x0, sy);
            let copy_w = ((src_w * 4) as usize).min(self.buf.len().saturating_sub(dst_off));
            if copy_w > 0 && src_off + copy_w <= src.len() {
                self.buf[dst_off..dst_off + copy_w].copy_from_slice(&src[src_off..src_off + copy_w]);
            }
        }
    }

    /// Draw a character with a specific foreground and background.
    /// (shorthand — identical to draw_char but kept for API compatibility)
    #[inline]
    pub fn draw_char_colored(&mut self, col: u32, row: u32, ch: u8, fg: u32, bg: u32) {
        self.draw_char(col * FONT_W as u32, row * FONT_H as u32, ch, fg, bg);
    }
}

/// Convert a VGA 4-bit colour index to a 32-bit RGBA pixel value.
#[inline]
pub fn vga_pixel(index: u8) -> u32 {
    match index & 0x0F {
        0x0 => 0xFF000000,       // Black
        0x1 => 0xFF0000AA,       // Blue
        0x2 => 0xFF00AA00,       // Green
        0x3 => 0xFF00AAAA,       // Cyan
        0x4 => 0xFFAA0000,       // Red
        0x5 => 0xFFAA00AA,       // Magenta
        0x6 => 0xFFAA5500,       // Brown
        0x7 => 0xFFAAAAAA,       // Light Gray
        0x8 => 0xFF555555,       // Dark Gray
        0x9 => 0xFF5555FF,       // Light Blue
        0xA => 0xFF55FF55,       // Light Green
        0xB => 0xFF55FFFF,       // Light Cyan
        0xC => 0xFFFF5555,       // Light Red
        0xD => 0xFFFF55FF,       // Light Magenta
        0xE => 0xFFFFFF55,       // Yellow
        0xF => 0xFFFFFFFF,       // White
        _   => 0xFF000000,
    }
}
