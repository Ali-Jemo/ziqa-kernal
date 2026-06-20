
use alloc::vec::Vec;
use alloc::string::String;
use alloc::string::ToString;
use alloc::collections::BTreeMap;
use core::fmt::Write;
use crate::{print, println};
use crate::process::Pid;
use crate::fs::vfs::VFS;
#[cfg(feature = "ziqafs")]
use crate::fs::ziqafs::{ZIQAFS, ZiqaFs, ROOT_INODE, BLOCK_SIZE};


use super::*;

impl Shell {
    pub(crate) fn push_history(&mut self) {
        let input = core::str::from_utf8(&self.input_buf[..self.cursor]).unwrap_or("");
        if input.is_empty() { return; }
        let last = self.history.last().map(|e| e.as_str() == input).unwrap_or(false);
        if !last {
            if self.history.len() >= MAX_HISTORY {
                self.history.remove(0);
            }
            self.history.push(input.to_string());
        }
    }

    pub(crate) fn prompt_len(&self) -> usize {
        let cwd = self.cwd_str();
        if cwd == "/" {
            "ziqa > ".chars().count()
        } else {
            ("ziqa ".to_string() + cwd + " > ").chars().count()
        }
    }

    pub(crate) fn refresh_line(&self, idx: usize) {
        let cwd = self.cwd_str();
        let prompt = if cwd == "/" {
            "ziqa > ".to_string()
        } else {
            "ziqa ".to_string() + cwd + " > "
        };
        print!("\r");
        for _ in 0..79 {
            print!(" ");
        }
        print!("\r");
        print!("{}", prompt);
        let len = self.input_buf.iter().position(|&b| b == 0).unwrap_or(256);
        if let Ok(s) = core::str::from_utf8(&self.input_buf[..len]) {
            print!("{}", s);
        }
        let prompt_len = prompt.chars().count();
        crate::drivers::vga::set_cursor_pos(24, prompt_len + idx);
    }

    pub(crate) fn load_history(&mut self, idx: &mut usize) {
        let entry = &self.history[self.history_pos as usize];
        let bytes = entry.as_bytes();
        let len = bytes.len().min(255);
        self.input_buf[..len].copy_from_slice(&bytes[..len]);
        if len < 256 { self.input_buf[len] = 0; }
        *idx = len;
        self.refresh_line(*idx);
    }

    pub(crate) fn autocomplete(&mut self, idx: &mut usize) {
        let input = core::str::from_utf8(&self.input_buf[..*idx]).unwrap_or("");
        let last_space = input.rfind(' ').map(|i| i + 1).unwrap_or(0);
        let prefix = &input[last_space..];

        let is_first_word = last_space == 0;

        let candidates: Vec<String> = if is_first_word {
            COMMANDS.iter().filter(|c| c.starts_with(prefix)).map(|s| s.to_string()).collect()
        } else if !prefix.is_empty() {
            let cmd_end = input.find(' ').unwrap_or(input.len());
            let cmd = &input[..cmd_end];
            self.complete_arg(cmd, prefix)
        } else {
            Vec::new()
        };

        if candidates.is_empty() {
            return;
        }

        if candidates.len() == 1 {
            let completion = &candidates[0];
            let bytes = completion.as_bytes();
            let new_end = last_space + bytes.len();
            self.input_buf[last_space..new_end].copy_from_slice(bytes);
            for i in new_end..*idx {
                self.input_buf[i] = 0;
            }
            *idx = new_end;
            self.refresh_line(*idx);
        } else {
            let common = Self::longest_common_prefix(&candidates);
            if common.len() > prefix.len() {
                let bytes = common.as_bytes();
                let new_end = last_space + common.len();
                self.input_buf[last_space..new_end].copy_from_slice(bytes);
                for i in new_end..*idx {
                    self.input_buf[i] = 0;
                }
                *idx = new_end;
                self.refresh_line(*idx);
            } else {
                println!("");
                for c in &candidates {
                    if is_first_word {
                        print!("{}{}{}  ", C_GREEN, c, C_RESET);
                    } else if c.parse::<u64>().is_ok() {
                        print!("{}{}{}  ", C_YELLOW, c, C_RESET);
                    } else if VFS.read().is_dir(c) {
                        print!("{}{}{}  ", C_BLUE, c, C_RESET);
                    } else {
                        print!("{}  ", c);
                    }
                }
                println!("");
                self.refresh_line(*idx);
            }
        }
    }

    pub(crate) fn complete_arg(&self, cmd: &str, prefix: &str) -> Vec<String> {
        if matches!(cmd, "ls" | "cd" | "edit" | "rm" | "cat" | "mkdir" | "dir" | "spawnelf" | "spawn") {
            let vfs = VFS.read();
            let all = vfs.list();
            let cwd = self.cwd_str();
            let search_prefix = if prefix.starts_with('/') {
                prefix.to_string()
            } else if cwd == "/" {
                alloc::format!("/{}", prefix)
            } else {
                alloc::format!("{}/{}", cwd, prefix)
            };
            let matched: Vec<String> = all.into_iter().filter(|p| p.starts_with(&search_prefix)).collect();
            if matched.is_empty() {
                return Vec::new();
            }
            if prefix.starts_with('/') {
                return matched;
            }
            let cwd_prefix: String = if cwd == "/" { "/".to_string() } else { alloc::format!("{}/", cwd) };
            return matched.into_iter().map(|p| {
                p.strip_prefix(&cwd_prefix).unwrap_or(&p).to_string()
            }).collect();
        }

        if matches!(cmd, "syscall" | "syscalls") {
            let mut out = Vec::new();
            for entry in SYSCALLS {
                if entry.name.starts_with(prefix) {
                    out.push(entry.name.to_string());
                }
                if entry.category.starts_with(prefix) && !out.iter().any(|s| s == entry.category) {
                    out.push(entry.category.to_string());
                }
            }
            return out;
        }

        if matches!(cmd, "kill" | "exec") {
            let pids = crate::process::scheduler::list_pids();
            return pids.iter().map(|p| alloc::format!("{}", p.0)).filter(|s| s.starts_with(prefix)).collect();
        }

        Vec::new()
    }

    pub(crate) fn find_similar_command(&self, cmd: &str) -> Option<&'static str> {
        let mut best: Option<&'static str> = None;
        let mut best_dist = 4;
        for &c in COMMANDS {
            let d = Self::levenshtein_distance(cmd, c);
            if d < best_dist {
                best_dist = d;
                best = Some(c);
            }
        }
        best
    }

    pub(crate) fn update_status_bar(&self) {
        let ms = crate::timer::uptime_ms();
        let secs = ms / 1000;
        let hours = secs / 3600;
        let minutes = (secs % 3600) / 60;
        let seconds = secs % 60;
        let mem_usage = crate::memory::heapstats::get_stats().current_usage_bytes() / 1024;
        let pcount = crate::process::scheduler::list_pids().len();
        crate::drivers::vga::set_status_text(
            &alloc::format!(" \x1b[30;47m \x1b[36mZIQA\x1b[0m v0.1 | \x1b[33m{:02}:{:02}:{:02}\x1b[0m | \x1b[32mMem: {}K\x1b[0m | \x1b[35mProcs: {}\x1b[0m | \x1b[33mPress PgUp/PgDn for scrollback\x1b[0m ",
                hours, minutes, seconds, mem_usage, pcount)
        );
    }

    pub(crate) fn read_line(&mut self) {
        use crate::drivers::keyboard::read_stdin;
        let mut idx = 0;
        self.input_buf = [0; 256];
        self.cursor = 0;
        loop {
            // Stay on the shell context while waiting for input.  Hardware
            // interrupts still run; only timer-driven task switching is gated.

            let mut byte = [0u8; 1];
            if read_stdin(&mut byte) > 0 {
                let b = byte[0];
                match b {
                    0x80 => { // Up arrow — history back
                        if !self.history.is_empty() {
                            if self.history_pos < 0 {
                                self.history_pos = self.history.len() as isize - 1;
                            } else if self.history_pos > 0 {
                                self.history_pos -= 1;
                            }
                            self.load_history(&mut idx);
                        }
                    }
                    0x81 => { // Down arrow — history forward
                        if self.history_pos >= 0 {
                            self.history_pos += 1;
                            if self.history_pos as usize >= self.history.len() {
                                self.history_pos = -1;
                                self.input_buf = [0; 256];
                                idx = 0;
                                self.refresh_line(idx);
                            } else {
                                self.load_history(&mut idx);
                            }
                        }
                    }
                    0x82 => { // Left Arrow
                        if idx > 0 {
                            idx -= 1;
                            crate::drivers::vga::set_cursor_pos(24, idx + self.prompt_len());
                        }
                    }
                    0x83 => { // Right Arrow
                        if idx < 255 && self.input_buf[idx] != 0 {
                            idx += 1;
                            crate::drivers::vga::set_cursor_pos(24, idx + self.prompt_len());
                        }
                    }
                    8 | 127 => { // Backspace
                        if idx > 0 {
                            let len = self.input_buf.iter().position(|&b| b == 0).unwrap_or(256);
                            for i in idx-1..len-1 {
                                self.input_buf[i] = self.input_buf[i+1];
                            }
                            self.input_buf[len-1] = 0;
                            idx -= 1;
                            self.refresh_line(idx);
                        }
                    }
                    0x09 => { // Tab
                        self.autocomplete(&mut idx);
                    }
                    0x88 => { // Delete
                        if idx < 255 && self.input_buf[idx] != 0 {
                            let len = self.input_buf.iter().position(|&b| b == 0).unwrap_or(256);
                            for i in idx..len-1 {
                                self.input_buf[i] = self.input_buf[i+1];
                            }
                            self.input_buf[len-1] = 0;
                            self.refresh_line(idx);
                        }
                    }
                    b'\n' | b'\r' => {
                        self.cursor = idx;
                        if crate::drivers::vga::is_scrolled() {
                            crate::drivers::vga::restore_terminal();
                        }
                        println!("");
                        return;
                    }
                    0x03 => {
                        self.cursor = idx;
                        println!("^C");
                        self.input_buf = [0; 256];
                        idx = 0;
                        self.refresh_line(idx);
                    }
                    0x0C => {
                        self.cmd_clear(&[]);
                        self.refresh_line(idx);
                    }
                    0x04 => {
                        println!("^D - Exiting shell");
                        return;
                    }
                    _ => {
                        if idx < 255 {
                            let len = self.input_buf.iter().position(|&b| b == 0).unwrap_or(256);
                            for i in (idx..len.min(254)).rev() {
                                self.input_buf[i + 1] = self.input_buf[i];
                            }
                            self.input_buf[idx] = b;
                            idx += 1;
                            self.refresh_line(idx);
                        }
                    }
                }
            } else {
                // Keep polling COM1 and the PS/2 controller.  Interrupts still
                // fire, but the shell is not preempted away from its input loop.
                core::hint::spin_loop();
            }
        }
    }


}
