/// Simple text editor (nano-like) for ZiqaKernel

use alloc::vec::Vec;
use core::cmp::min;
use crate::drivers::vga::{self, Color, WRITER};
use crate::drivers::keyboard;
use crate::fs::vfs::VFS;

const COLS: usize = 80;
const ROWS: usize = 24;

const K_UP: u8 = 0x80;
const K_DOWN: u8 = 0x81;
const K_LEFT: u8 = 0x82;
const K_RIGHT: u8 = 0x83;
const K_HOME: u8 = 0x84;
const K_END: u8 = 0x85;
const K_DEL: u8 = 0x88;

pub struct Editor {
    buf: Vec<u8>,
    lines: Vec<usize>,
    cursor: usize,
    scroll: usize,
    path: [u8; 256],
    path_len: usize,
    modified: bool,
}

impl Editor {
    pub fn new(path: &str) -> Self {
        let mut ed = Editor {
            buf: Vec::new(),
            lines: Vec::new(),
            cursor: 0,
            scroll: 0,
            path: [0u8; 256],
            path_len: 0,
            modified: false,
        };
        let bytes = path.as_bytes();
        let n = min(bytes.len(), 255);
        ed.path[..n].copy_from_slice(&bytes[..n]);
        ed.path_len = n;

        let mut tmp = [0u8; 32768];
        if let Ok(n) = VFS.lock().read_raw(path, &mut tmp, 0) {
            ed.buf = tmp[..n].to_vec();
        }
        ed.build_lines();
        ed
    }

    fn path_str(&self) -> &str {
        core::str::from_utf8(&self.path[..self.path_len]).unwrap_or("?")
    }

    fn build_lines(&mut self) {
        self.lines.clear();
        self.lines.push(0);
        for i in 0..self.buf.len() {
            if self.buf[i] == b'\n' {
                self.lines.push(i + 1);
            }
        }
    }

    fn line_count(&self) -> usize {
        self.lines.len()
    }

    fn cursor_line(&self) -> usize {
        match self.lines.binary_search(&self.cursor) {
            Ok(i) => i,
            Err(i) => i.saturating_sub(1),
        }
    }

    fn cursor_col(&self) -> usize {
        self.cursor - self.lines[self.cursor_line()]
    }

    fn screen_pos(&self) -> (usize, usize) {
        let line = self.cursor_line();
        (line.saturating_sub(self.scroll), self.cursor - self.lines[line])
    }

    fn ensure_scroll(&mut self) {
        let line = self.cursor_line();
        if line < self.scroll {
            self.scroll = line;
        } else if line >= self.scroll + ROWS {
            self.scroll = line - ROWS + 1;
        }
    }

    fn move_up(&mut self) {
        let line = self.cursor_line();
        if line > 0 {
            let col = self.cursor_col();
            let plen = self.lines[line] - self.lines[line - 1];
            let plen = if plen > 0 { plen.saturating_sub(1) } else { 0 };
            self.cursor = self.lines[line - 1] + min(col, plen);
        }
        self.ensure_scroll();
    }

    fn move_down(&mut self) {
        let line = self.cursor_line();
        if line + 1 < self.lines.len() {
            let col = self.cursor_col();
            let nlen = if line + 2 < self.lines.len() {
                self.lines[line + 2] - self.lines[line + 1]
            } else {
                self.buf.len() - self.lines[line + 1]
            };
            let nlen = if nlen > 0 { nlen.saturating_sub(1) } else { 0 };
            self.cursor = self.lines[line + 1] + min(col, nlen);
        }
        self.ensure_scroll();
    }

    fn move_left(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
        self.ensure_scroll();
    }

    fn move_right(&mut self) {
        if self.cursor < self.buf.len() {
            self.cursor += 1;
        }
        self.ensure_scroll();
    }

    fn move_home(&mut self) {
        self.cursor = self.lines[self.cursor_line()];
        self.ensure_scroll();
    }

    fn move_end(&mut self) {
        let line = self.cursor_line();
        self.cursor = if line + 1 < self.lines.len() {
            self.lines[line + 1].saturating_sub(1)
        } else {
            self.buf.len()
        };
        self.ensure_scroll();
    }

    fn insert_char(&mut self, c: u8) {
        self.buf.insert(self.cursor, c);
        self.cursor += 1;
        self.modified = true;
        self.build_lines();
        self.ensure_scroll();
    }

    fn insert_newline(&mut self) {
        self.buf.insert(self.cursor, b'\n');
        self.cursor += 1;
        self.modified = true;
        self.build_lines();
        self.ensure_scroll();
    }

    fn backspace(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            self.buf.remove(self.cursor);
            self.modified = true;
            self.build_lines();
            self.ensure_scroll();
        }
    }

    fn delete_at_cursor(&mut self) {
        if self.cursor < self.buf.len() {
            self.buf.remove(self.cursor);
            self.modified = true;
            self.build_lines();
            self.ensure_scroll();
        }
    }

    fn save(&mut self) {
        let path = self.path_str();
        let mut vfs = VFS.lock();
        if !vfs.exists(path) {
            vfs.create(path);
        }
        let _ = vfs.write_raw(path, &self.buf, 0);
        self.modified = false;
    }

    pub fn run(&mut self) {
        keyboard::set_echo(false);
        keyboard::clear_editor_buf();

        self.render();

        loop {
            let mut byte = [0u8; 1];
            if keyboard::read_editor_byte(&mut byte) == 0 {
                x86_64::instructions::hlt();
                continue;
            }
            let b = byte[0];
            match b {
                K_UP => self.move_up(),
                K_DOWN => self.move_down(),
                K_LEFT => self.move_left(),
                K_RIGHT => self.move_right(),
                K_HOME => self.move_home(),
                K_END => self.move_end(),
                K_DEL => self.delete_at_cursor(),
                0x08 | 0x7F => self.backspace(),
                b'\n' | b'\r' => self.insert_newline(),
                0x13 => self.save(),
                0x11 => break,
                c if c >= 0x20 => self.insert_char(c),
                _ => {}
            }
            self.render();
        }

        keyboard::set_echo(true);
        keyboard::clear_editor_buf();
        vga::clear_screen();
    }

    fn render(&self) {
        let mut writer = WRITER.lock();

        for row in 0..ROWS {
            writer.clear_row(row);
        }

        let end = min(self.scroll + ROWS, self.line_count());
        for i in self.scroll..end {
            let sr = i - self.scroll;
            let start = self.lines[i];
            let line_end = if i + 1 < self.lines.len() {
                self.lines[i + 1].saturating_sub(1)
            } else {
                self.buf.len()
            };
            let line_data = &self.buf[start..line_end];
            let mut col = 0;
            for &b in line_data {
                if col >= COLS {
                    break;
                }
                let ch = match b {
                    0x09 => b' ',
                    0x20..=0x7E => b,
                    _ => b'.',
                };
                writer.write_char_at(sr, col, ch, Color::White, Color::Black);
                col += 1;
            }
        }

        let (sr, sc) = self.screen_pos();
        if sr < ROWS && sc < COLS {
            let ch = writer.read_char_at(sr, sc);
            writer.write_char_at(sr, sc, ch, Color::Black, Color::White);
        }
        self.render_status(&mut writer);
        vga::set_cursor_pos(sr.min(ROWS - 1), sc.min(COLS - 1));
    }

    fn render_status(&self, writer: &mut vga::Writer) {
        for col in 0..COLS {
            writer.write_char_at(ROWS, col, b' ', Color::White, Color::Blue);
        }

        let path = self.path_str();
        let mut col = 1;
        for &b in path.as_bytes() {
            if col >= COLS - 1 {
                break;
            }
            writer.write_char_at(ROWS, col, b, Color::Yellow, Color::Blue);
            col += 1;
        }

        let line = self.cursor_line() + 1;
        let total = self.line_count();
        let info = alloc::format!(" Line {}/{} ", line, total);
        for &b in info.as_bytes() {
            if col >= COLS - 1 {
                break;
            }
            writer.write_char_at(ROWS, col, b, Color::White, Color::Blue);
            col += 1;
        }

        if self.modified {
            for &b in b" [Modified] " {
                if col >= COLS - 1 {
                    break;
                }
                writer.write_char_at(ROWS, col, b, Color::LightRed, Color::Blue);
                col += 1;
            }
        }

        let help = b" ^S=Save  ^Q=Quit ";
        let help_start = COLS.saturating_sub(help.len());
        let mut hcol = help_start;
        for &b in help {
            if hcol >= COLS {
                break;
            }
            writer.write_char_at(ROWS, hcol, b, Color::White, Color::Blue);
            hcol += 1;
        }
    }
}

pub fn edit_file(path: &str) {
    let mut editor = Editor::new(path);
    editor.run();
}
