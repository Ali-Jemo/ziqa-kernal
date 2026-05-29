/// ZiqaKernel Text Editor
///
/// Features beyond nano/nvim in bare-metal context:
///   - Line numbers gutter
///   - Syntax highlighting (keywords, strings, comments, numbers)
///   - Tab → 4 spaces expansion
///   - Undo/redo stack (Ctrl+Z / Ctrl+Y)
///   - Find (Ctrl+F) + Replace (Ctrl+R) with incremental highlight
///   - Word jump (Ctrl+Left / Ctrl+Right)
///   - Page Up / Page Down
///   - Selection (Shift+arrows) + Copy/Cut/Paste (Ctrl+C / Ctrl+X / Ctrl+V)
///   - Status bar: mode, file type, line:col, % through file
use crate::drivers::keyboard;
use crate::drivers::vga::{self, Color, WRITER, set_cursor_pos};
use crate::fs::vfs::VFS;
use alloc::vec::Vec;
use core::cmp::min;

// ── Screen geometry ──────────────────────────────────────────────────────────
const COLS: usize = 80;
const ROWS: usize = 23; // one less — status bar at row 23, line 24 is VGA status
const GUTTER: usize = 5; // "NNN: " — line number + space

// ── Keyboard codes ───────────────────────────────────────────────────────────
const K_UP:    u8 = 0x80;
const K_DOWN:  u8 = 0x81;
const K_LEFT:  u8 = 0x82;
const K_RIGHT: u8 = 0x83;
const K_HOME:  u8 = 0x84;
const K_END:   u8 = 0x85;
const K_PGUP:  u8 = 0x86;
const K_PGDN:  u8 = 0x87;
const K_DEL:   u8 = 0x88;

// Ctrl codes
const CTRL_S: u8 = 0x13;
const CTRL_Q: u8 = 0x11;
const CTRL_Z: u8 = 0x1A;
const CTRL_Y: u8 = 0x19;
const CTRL_F: u8 = 0x06;
const CTRL_R: u8 = 0x12;
const CTRL_C: u8 = 0x03;
const CTRL_X: u8 = 0x18;
const CTRL_V: u8 = 0x16;
const CTRL_A: u8 = 0x01; // select all
// Shift+arrow: keyboard driver sends these as high bytes
const K_SHIFT_UP:    u8 = 0x90;
const K_SHIFT_DOWN:  u8 = 0x91;
const K_SHIFT_LEFT:  u8 = 0x92;
const K_SHIFT_RIGHT: u8 = 0x93;
// Ctrl+arrow: word jump
const K_CTRL_LEFT:  u8 = 0x94;
const K_CTRL_RIGHT: u8 = 0x95;

// ── Editor mode ──────────────────────────────────────────────────────────────
#[derive(Clone, Copy, PartialEq)]
enum Mode { Insert, Find, Replace }

// ── Undo record ──────────────────────────────────────────────────────────────
struct UndoEntry {
    buf: Vec<u8>,
    cursor: usize,
}

// ── File type for syntax highlighting ───────────────────────────────────────
#[derive(Clone, Copy, PartialEq)]
enum FileType { Rust, C, Zig, Text }

impl FileType {
    fn from_path(path: &str) -> Self {
        if path.ends_with(".rs")  { FileType::Rust }
        else if path.ends_with(".c") || path.ends_with(".h") { FileType::C }
        else if path.ends_with(".zig") { FileType::Zig }
        else { FileType::Text }
    }
}

// ── Main editor struct ───────────────────────────────────────────────────────
pub struct Editor {
    buf:       Vec<u8>,
    lines:     Vec<usize>,   // byte offset of each line start
    cursor:    usize,        // byte offset of cursor
    scroll:    usize,        // first visible line index
    path:      [u8; 256],
    path_len:  usize,
    modified:  bool,
    mode:      Mode,
    // undo/redo
    undo:      Vec<UndoEntry>,
    redo:      Vec<UndoEntry>,
    // selection: anchor byte offset (None = no selection)
    sel_anchor: Option<usize>,
    // clipboard
    clipboard: Vec<u8>,
    // find/replace
    find_query:   Vec<u8>,
    replace_with: Vec<u8>,
    find_match:   Option<usize>, // byte offset of current match
    input_buf:    Vec<u8>,       // reused for find/replace input
    filetype:     FileType,
}

impl Editor {
    pub fn new(path: &str) -> Self {
        let mut ed = Editor {
            buf: Vec::new(), lines: Vec::new(),
            cursor: 0, scroll: 0,
            path: [0u8; 256], path_len: 0,
            modified: false, mode: Mode::Insert,
            undo: Vec::new(), redo: Vec::new(),
            sel_anchor: None, clipboard: Vec::new(),
            find_query: Vec::new(), replace_with: Vec::new(),
            find_match: None, input_buf: Vec::new(),
            filetype: FileType::from_path(path),
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
            if self.buf[i] == b'\n' { self.lines.push(i + 1); }
        }
    }

    fn line_count(&self) -> usize { self.lines.len() }

    fn cursor_line(&self) -> usize {
        match self.lines.binary_search(&self.cursor) {
            Ok(i) => i,
            Err(i) => i.saturating_sub(1),
        }
    }

    fn cursor_col(&self) -> usize {
        self.cursor - self.lines[self.cursor_line()]
    }

    fn line_len(&self, line: usize) -> usize {
        let start = self.lines[line];
        let end = if line + 1 < self.lines.len() {
            self.lines[line + 1].saturating_sub(1)
        } else {
            self.buf.len()
        };
        end.saturating_sub(start)
    }

    fn ensure_scroll(&mut self) {
        let line = self.cursor_line();
        if line < self.scroll { self.scroll = line; }
        else if line >= self.scroll + ROWS { self.scroll = line - ROWS + 1; }
    }

    // ── cursor movement ──────────────────────────────────────────────────────

    fn move_up(&mut self) {
        let line = self.cursor_line();
        if line > 0 {
            let col = self.cursor_col();
            self.cursor = self.lines[line - 1] + min(col, self.line_len(line - 1));
        }
        self.ensure_scroll();
    }

    fn move_down(&mut self) {
        let line = self.cursor_line();
        if line + 1 < self.lines.len() {
            let col = self.cursor_col();
            self.cursor = self.lines[line + 1] + min(col, self.line_len(line + 1));
        }
        self.ensure_scroll();
    }

    fn move_left(&mut self) {
        if self.cursor > 0 { self.cursor -= 1; }
        self.ensure_scroll();
    }

    fn move_right(&mut self) {
        if self.cursor < self.buf.len() { self.cursor += 1; }
        self.ensure_scroll();
    }

    fn move_home(&mut self) {
        self.cursor = self.lines[self.cursor_line()];
        self.ensure_scroll();
    }

    fn move_end(&mut self) {
        let line = self.cursor_line();
        self.cursor = self.lines[line] + self.line_len(line);
        self.ensure_scroll();
    }

    fn page_up(&mut self) {
        let line = self.cursor_line();
        let col  = self.cursor_col();
        let new_line = line.saturating_sub(ROWS);
        self.cursor = self.lines[new_line] + min(col, self.line_len(new_line));
        self.ensure_scroll();
    }

    fn page_down(&mut self) {
        let line = self.cursor_line();
        let col  = self.cursor_col();
        let new_line = min(line + ROWS, self.line_count().saturating_sub(1));
        self.cursor = self.lines[new_line] + min(col, self.line_len(new_line));
        self.ensure_scroll();
    }

    fn word_left(&mut self) {
        if self.cursor == 0 { return; }
        self.cursor -= 1;
        while self.cursor > 0 && !is_word_boundary(&self.buf, self.cursor) {
            self.cursor -= 1;
        }
        self.ensure_scroll();
    }

    fn word_right(&mut self) {
        let len = self.buf.len();
        if self.cursor >= len { return; }
        self.cursor += 1;
        while self.cursor < len && !is_word_boundary(&self.buf, self.cursor) {
            self.cursor += 1;
        }
        self.ensure_scroll();
    }
}

fn is_word_boundary(buf: &[u8], pos: usize) -> bool {
    if pos == 0 || pos >= buf.len() { return true; }
    let prev = buf[pos - 1];
    let curr = buf[pos];
    let prev_word = prev.is_ascii_alphanumeric() || prev == b'_';
    let curr_word = curr.is_ascii_alphanumeric() || curr == b'_';
    prev_word != curr_word
}

impl Editor {
    // ── selection ────────────────────────────────────────────────────────────

    fn sel_start(&self) -> usize {
        match self.sel_anchor {
            Some(a) => min(a, self.cursor),
            None => self.cursor,
        }
    }
    fn sel_end(&self) -> usize {
        match self.sel_anchor {
            Some(a) => min(a, self.cursor).max(a.max(self.cursor)),
            None => self.cursor,
        }
    }
    fn has_sel(&self) -> bool { self.sel_anchor.map(|a| a != self.cursor).unwrap_or(false) }

    fn sel_move(&mut self, f: fn(&mut Editor)) {
        if self.sel_anchor.is_none() { self.sel_anchor = Some(self.cursor); }
        f(self);
    }

    fn sel_clear(&mut self) { self.sel_anchor = None; }

    fn select_all(&mut self) {
        self.sel_anchor = Some(0);
        self.cursor = self.buf.len();
        self.ensure_scroll();
    }

    // ── undo/redo ────────────────────────────────────────────────────────────

    fn snapshot(&mut self) {
        let entry = UndoEntry { buf: self.buf.clone(), cursor: self.cursor };
        self.undo.push(entry);
        if self.undo.len() > 200 { self.undo.remove(0); }
        self.redo.clear();
    }

    fn undo(&mut self) {
        if let Some(entry) = self.undo.pop() {
            let redo_entry = UndoEntry { buf: self.buf.clone(), cursor: self.cursor };
            self.redo.push(redo_entry);
            self.buf = entry.buf;
            self.cursor = entry.cursor;
            self.modified = true;
            self.build_lines();
            self.ensure_scroll();
        }
    }

    fn redo(&mut self) {
        if let Some(entry) = self.redo.pop() {
            let undo_entry = UndoEntry { buf: self.buf.clone(), cursor: self.cursor };
            self.undo.push(undo_entry);
            self.buf = entry.buf;
            self.cursor = entry.cursor;
            self.modified = true;
            self.build_lines();
            self.ensure_scroll();
        }
    }

    // ── edit operations ──────────────────────────────────────────────────────

    fn delete_selection(&mut self) -> bool {
        if !self.has_sel() { return false; }
        let s = self.sel_start();
        let e = self.sel_end();
        self.snapshot();
        self.buf.drain(s..e);
        self.cursor = s;
        self.sel_anchor = None;
        self.modified = true;
        self.build_lines();
        self.ensure_scroll();
        true
    }

    fn insert_char(&mut self, c: u8) {
        self.delete_selection();
        self.snapshot();
        self.buf.insert(self.cursor, c);
        self.cursor += 1;
        self.modified = true;
        self.build_lines();
        self.ensure_scroll();
    }

    fn insert_tab(&mut self) {
        // expand tab to 4 spaces
        for _ in 0..4 { self.insert_char(b' '); }
    }

    fn insert_newline(&mut self) {
        self.delete_selection();
        self.snapshot();
        // auto-indent: copy leading whitespace of current line
        let line = self.cursor_line();
        let line_start = self.lines[line];
        let mut indent = 0;
        while line_start + indent < self.buf.len()
            && (self.buf[line_start + indent] == b' ' || self.buf[line_start + indent] == b'\t')
        { indent += 1; }
        self.buf.insert(self.cursor, b'\n');
        self.cursor += 1;
        for i in 0..indent {
            self.buf.insert(self.cursor, self.buf[line_start + i]);
            self.cursor += 1;
        }
        self.modified = true;
        self.build_lines();
        self.ensure_scroll();
    }

    fn backspace(&mut self) {
        if self.delete_selection() { return; }
        if self.cursor > 0 {
            self.snapshot();
            self.cursor -= 1;
            self.buf.remove(self.cursor);
            self.modified = true;
            self.build_lines();
            self.ensure_scroll();
        }
    }

    fn delete_at_cursor(&mut self) {
        if self.delete_selection() { return; }
        if self.cursor < self.buf.len() {
            self.snapshot();
            self.buf.remove(self.cursor);
            self.modified = true;
            self.build_lines();
            self.ensure_scroll();
        }
    }

    // ── clipboard ────────────────────────────────────────────────────────────

    fn copy(&mut self) {
        if self.has_sel() {
            let s = self.sel_start();
            let e = self.sel_end();
            self.clipboard = self.buf[s..e].to_vec();
        }
        self.sel_clear();
    }

    fn cut(&mut self) {
        if self.has_sel() {
            let s = self.sel_start();
            let e = self.sel_end();
            self.clipboard = self.buf[s..e].to_vec();
            self.delete_selection();
        }
    }

    fn paste(&mut self) {
        if self.clipboard.is_empty() { return; }
        self.delete_selection();
        self.snapshot();
        let clip = self.clipboard.clone();
        for &b in &clip {
            self.buf.insert(self.cursor, b);
            self.cursor += 1;
        }
        self.modified = true;
        self.build_lines();
        self.ensure_scroll();
    }

    // ── find / replace ───────────────────────────────────────────────────────

    fn find_next(&mut self) {
        if self.find_query.is_empty() { return; }
        let start = self.find_match.map(|m| m + 1).unwrap_or(self.cursor);
        let q = &self.find_query;
        // search forward from start, wrap around
        let found = search_forward(&self.buf, q, start)
            .or_else(|| search_forward(&self.buf, q, 0));
        self.find_match = found;
        if let Some(pos) = found {
            self.cursor = pos;
            self.ensure_scroll();
        }
    }

    fn do_replace_current(&mut self) {
        if let Some(pos) = self.find_match {
            let qlen = self.find_query.len();
            if pos + qlen <= self.buf.len()
                && self.buf[pos..pos + qlen] == *self.find_query
            {
                self.snapshot();
                self.buf.drain(pos..pos + qlen);
                let rep = self.replace_with.clone();
                for (i, &b) in rep.iter().enumerate() {
                    self.buf.insert(pos + i, b);
                }
                self.cursor = pos + rep.len();
                self.modified = true;
                self.build_lines();
                self.ensure_scroll();
                self.find_next();
            }
        }
    }
}

fn search_forward(buf: &[u8], query: &[u8], from: usize) -> Option<usize> {
    if query.is_empty() || buf.len() < query.len() { return None; }
    let limit = buf.len() - query.len();
    for i in from..=limit {
        if buf[i..i + query.len()] == *query { return Some(i); }
    }
    None
}

impl Editor {
    // ── rendering ──────────────────────────────────────────────────────────

    fn render(&mut self) {
        use x86_64::instructions::interrupts;
        interrupts::without_interrupts(|| {
            let mut writer = WRITER.lock();
            for row in 0..ROWS {
                writer.clear_row(row);
                let line_num = self.scroll + row;
                if line_num < self.line_count() {
                    let num_str = alloc::format!("{:>3}: ", line_num + 1);
                    for (i, &b) in num_str.as_bytes().iter().enumerate() {
                        if i < GUTTER {
                            writer.write_char_at(row, i, b, Color::DarkGray, Color::Black);
                        }
                    }
                    let start = self.lines[line_num];
                    let end = if line_num + 1 < self.lines.len() {
                        self.lines[line_num + 1].saturating_sub(1)
                    } else {
                        self.buf.len()
                    };
                    let line = &self.buf[start..end];
                    let mut col = GUTTER;
                    let mut in_comment = false;
                    let mut in_string = false;
                    for i in 0..line.len() {
                        if col >= COLS { break; }
                        let b = line[i];
                        let fg = if in_comment {
                            Color::Green
                        } else if in_string {
                            Color::Brown
                        } else {
                            self.char_color(&line, i, &mut in_comment, &mut in_string)
                        };
                        writer.write_char_at(row, col, b, fg, Color::Black);
                        col += 1;
                    }
                }
            }
            let status_row = ROWS;
            writer.clear_row(status_row);
            let pct = if self.line_count() > 0 {
                (self.cursor_line() * 100) / self.line_count()
            } else { 0 };
            let mode_str = match self.mode {
                Mode::Insert => "INSERT",
                Mode::Find => "FIND",
                Mode::Replace => "REPLACE",
            };
            let status = alloc::format!(
                " {} {} | {} | Ln {}, Col {} | {}%% ",
                if self.modified { "[+]" } else { "" },
                mode_str,
                self.path_str(),
                self.cursor_line() + 1,
                self.cursor_col() + 1,
                pct,
            );
            let bytes = status.as_bytes();
            for c in 0..COLS {
                let ch = if c < bytes.len() { bytes[c] } else { b' ' };
                writer.write_char_at(status_row, c, ch, Color::White, Color::Blue);
            }
            let vis_line = self.cursor_line().saturating_sub(self.scroll);
            let cursor_col = GUTTER + self.cursor_col();
            if vis_line < ROWS {
                set_cursor_pos(vis_line, cursor_col.min(COLS - 1));
            } else {
                set_cursor_pos(status_row, 0);
            }
        });
    }

    fn char_color(&self, line: &[u8], i: usize, in_comment: &mut bool, in_string: &mut bool) -> Color {
        let b = line[i];
        if b == b'"' {
            *in_string = !*in_string;
            return Color::Brown;
        }
        if b == b'\'' && !*in_string {
            return Color::Brown;
        }
        if !*in_string && i + 1 < line.len() && line[i] == b'/' && line[i + 1] == b'/' {
            *in_comment = true;
            return Color::Green;
        }
        if b.is_ascii_digit() && !*in_string { return Color::Yellow; }
        let keywords: &[u8] = match self.filetype {
            FileType::Rust => {
                b"fn|let|mut|pub|if|else|for|while|return|match|unsafe|struct|enum|impl|use|mod|const|static|ref|self|true|false|as|in|where|trait|type|extern|macro|async|await|move|break|continue|super|crate|default|union|dyn|abstract|become|box|do|export|final|macro_rules|override|priv|unsized|virtual|yield" as &[_]
            }
            FileType::C => {
                b"if|else|for|while|do|return|switch|case|break|continue|struct|union|enum|typedef|static|const|volatile|extern|inline|int|char|void|long|short|unsigned|signed|float|double|sizeof|goto|register|auto|restrict|_Bool|_Complex|_Imaginary" as &[_]
            }
            FileType::Zig => {
                b"fn|var|const|if|else|for|while|return|switch|break|continue|struct|enum|union|pub|inline|comptime|export|extern|volatile|align|allowzero|callconv|defer|errdefer|error|async|await|suspend|resume|catch|noalias|nosuspend|or|orelse|try|anytype|anyerror|anyframe|anyopaque|bool|f16|f32|f64|f128|i8|i16|i32|i64|u8|u16|u32|u64|isize|usize|c_char|c_short|c_int|c_long|c_longlong|c_uint|c_ulong|c_ulonglong|noreturn|type|void|undefined|null|true|false" as &[_]
            }
            FileType::Text => b"" as &[_],
        };
        if !*in_string && b.is_ascii_alphabetic() {
            let start = i;
            let mut end = i + 1;
            while end < line.len() && (line[end].is_ascii_alphanumeric() || line[end] == b'_') {
                end += 1;
            }
            let word = &line[start..end];
            if keywords.windows(word.len()).any(|w| *w == *word) {
                return Color::Pink;
            }
        }
        Color::LightGray
    }

    fn render_prompt(&mut self, label: &str, input: &[u8]) {
        use x86_64::instructions::interrupts;
        interrupts::without_interrupts(|| {
            let mut writer = WRITER.lock();
            let row = ROWS;
            writer.clear_row(row);
            let text = alloc::format!(" {} {}", label, core::str::from_utf8(input).unwrap_or("?"));
            let bytes = text.as_bytes();
            for c in 0..COLS {
                let ch = if c < bytes.len() { bytes[c] } else { b' ' };
                writer.write_char_at(row, c, ch, Color::White, Color::Blue);
            }
            set_cursor_pos(row, bytes.len().min(COLS - 1));
        });
    }

    fn save(&mut self) {
        let path = self.path_str();
        let mut vfs = VFS.lock();
        if !vfs.exists(path) {
            if path.starts_with("/disk/") {
                let name = path.trim_start_matches("/disk/");
                let fs_guard = crate::fs::ziqafs::ZIQAFS.lock();
                if let Some(ref fs) = *fs_guard {
                    let mut fs_lock = fs.lock();
                    let (parent_id, file_name) = if let Some(idx) = name.rfind('/') {
                        let dir = &name[..idx];
                        let fname = &name[idx + 1..];
                        let pid = crate::fs::ziqafs::ZiqaFs::root_lookup(
                            &mut fs_lock, &alloc::format!("/disk/{}", dir)).unwrap_or(0);
                        (pid, fname)
                    } else {
                        (crate::fs::ziqafs::ROOT_INODE, name)
                    };
                    if let Ok(id) = crate::fs::ziqafs::ZiqaFs::create_file(&mut fs_lock, parent_id, file_name) {
                        if let Ok(ino) = crate::fs::ziqafs::ZiqaFs::get_inode(&mut fs_lock, id) {
                            let file = crate::fs::ziqafs::ZiqaFsFile { fs: fs.clone(), inode_id: id, inode: ino };
                            vfs.mount(path, alloc::sync::Arc::new(spin::Mutex::new(file)));
                        }
                    }
                }
            } else {
                vfs.create(path);
            }
        }
        let _ = vfs.write_raw(path, &self.buf, 0);
        self.modified = false;
    }

    // ── find/replace input prompt ────────────────────────────────────────────

    fn prompt_input(&mut self, label: &str) -> Option<Vec<u8>> {
        self.input_buf.clear();
        loop {
            self.render_prompt(label, &self.input_buf.clone());
            let mut byte = [0u8; 1];
            if keyboard::read_editor_byte(&mut byte) == 0 {
                x86_64::instructions::hlt();
                continue;
            }
            match byte[0] {
                0x1B => return None,           // Esc = cancel
                b'\n' | b'\r' => return Some(self.input_buf.clone()),
                0x08 | 0x7F => { self.input_buf.pop(); }
                c if c >= 0x20 => self.input_buf.push(c),
                _ => {}
            }
        }
    }

    fn enter_find_mode(&mut self) {
        self.mode = Mode::Find;
        if let Some(q) = self.prompt_input("Find: ") {
            self.find_query = q;
            self.find_match = None;
            self.find_next();
        }
        self.mode = Mode::Insert;
    }

    fn enter_replace_mode(&mut self) {
        self.mode = Mode::Replace;
        if let Some(q) = self.prompt_input("Find: ") {
            self.find_query = q;
            if let Some(r) = self.prompt_input("Replace: ") {
                self.replace_with = r;
                self.find_match = None;
                self.find_next();
                // n=next match, r=replace current, Esc=done
                loop {
                    self.render();
                    let mut byte = [0u8; 1];
                    if keyboard::read_editor_byte(&mut byte) == 0 {
                        x86_64::instructions::hlt();
                        continue;
                    }
                    match byte[0] {
                        b'n' | b'N' => self.find_next(),
                        b'r' | b'R' | b'\r' | b'\n' => self.do_replace_current(),
                        0x1B | CTRL_Q => break,
                        _ => {}
                    }
                }
            }
        }
        self.mode = Mode::Insert;
    }

    // ── main run loop ────────────────────────────────────────────────────────

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

            // Selection movement (Shift+arrows)
            match b {
                K_SHIFT_UP    => { self.sel_move(Editor::move_up);    self.render(); continue; }
                K_SHIFT_DOWN  => { self.sel_move(Editor::move_down);  self.render(); continue; }
                K_SHIFT_LEFT  => { self.sel_move(Editor::move_left);  self.render(); continue; }
                K_SHIFT_RIGHT => { self.sel_move(Editor::move_right); self.render(); continue; }
                _ => {}
            }

            // Non-selection movement clears selection
            match b {
                K_UP | K_DOWN | K_LEFT | K_RIGHT |
                K_HOME | K_END | K_PGUP | K_PGDN |
                K_CTRL_LEFT | K_CTRL_RIGHT => self.sel_clear(),
                _ => {}
            }

            match b {
                K_UP          => self.move_up(),
                K_DOWN        => self.move_down(),
                K_LEFT        => self.move_left(),
                K_RIGHT       => self.move_right(),
                K_HOME        => self.move_home(),
                K_END         => self.move_end(),
                K_PGUP        => self.page_up(),
                K_PGDN        => self.page_down(),
                K_CTRL_LEFT   => self.word_left(),
                K_CTRL_RIGHT  => self.word_right(),
                K_DEL         => self.delete_at_cursor(),
                0x08 | 0x7F   => self.backspace(),
                b'\t'         => self.insert_tab(),
                b'\n' | b'\r' => self.insert_newline(),
                CTRL_S        => self.save(),
                CTRL_Q        => break,
                CTRL_Z        => self.undo(),
                CTRL_Y        => self.redo(),
                CTRL_F        => { self.enter_find_mode(); }
                CTRL_R        => { self.enter_replace_mode(); }
                CTRL_C        => self.copy(),
                CTRL_X        => self.cut(),
                CTRL_V        => self.paste(),
                CTRL_A        => self.select_all(),
                c if c >= 0x20 => { self.sel_clear(); self.insert_char(c); }
                _ => {}
            }
            self.render();
        }

        keyboard::set_echo(true);
        keyboard::clear_editor_buf();
        vga::clear_screen();
    }
}

pub fn edit_file(path: &str) {
    let mut editor = Editor::new(path);
    editor.run();
}
