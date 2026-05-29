/// Interactive shell for ZiqaKernel

use alloc::vec::Vec;
use alloc::string::String;
use alloc::string::ToString;
use alloc::collections::BTreeMap;
use crate::{print, println};
use crate::process::{AbiKind, Pid};
use crate::fs::vfs::VFS;
use crate::fs::ziqafs::{ZIQAFS, ZiqaFs, ROOT_INODE};
use x86_64::VirtAddr;

/// Represents a background job
#[derive(Debug)]
pub struct Job {
    pub pid: Pid,
    pub command: String,
    pub state: JobState,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum JobState {
    Running,
    Stopped,
    Done,
}

const COMMANDS: &[&str] = &[
    "help", "uptime", "ps", "spawn", "spawnelf", "exec", "kill",
    "sleep", "meminfo", "diskinfo", "netstat", "klog", "doom", "tetris",
    "reboot", "echo", "clear", "edit", "ls", "cd", "pwd", "mkdir",
    "dir", "rm", "rmdir", "cat", "ping", "wget", "ifconfig",
    "mv", "cp", "touch", "stat", "du",
    "dashboard", "top", "history", "alias", "export",
    "jobs", "bg", "fg", "nwm-test",
];

const MAX_HISTORY: usize = 50;

// ANSI escape sequences for terminal colors
const C_RESET: &str = "\x1b[0m";
const C_BOLD: &str = "\x1b[1m";
const C_DIM: &str = "\x1b[2m";
const C_RED: &str = "\x1b[31m";
const C_GREEN: &str = "\x1b[32m";
const C_YELLOW: &str = "\x1b[33m";
const C_BLUE: &str = "\x1b[34m";
const C_CYAN: &str = "\x1b[36m";

pub struct Shell {
    input_buf: [u8; 256],
    cursor: usize,
    history: Vec<[u8; 256]>,
    history_pos: isize,
    cwd: [u8; 256],
    cwd_len: usize,
    prev_cwd: [u8; 256],
    prev_cwd_len: usize,
    last_exit_status: i32,
    aliases: BTreeMap<String, String>,
    env: BTreeMap<String, String>,
    /// Job control
    jobs: Vec<Job>,
    fg_job: Option<usize>, // index in jobs vector of foreground job
}

impl Shell {
    pub const fn new() -> Self {
        Self {
            input_buf: [0; 256],
            cursor: 0,
            history: Vec::new(),
            history_pos: -1,
            cwd: [0; 256],
            cwd_len: 0,
            prev_cwd: [0; 256],
            prev_cwd_len: 0,
            last_exit_status: 0,
            aliases: BTreeMap::new(),
            env: BTreeMap::new(),
            jobs: Vec::new(),
            fg_job: None,
        }
    }

    fn set_exit_status(&mut self, status: i32) {
        self.last_exit_status = status;
    }

    fn cwd_str(&self) -> &str {
        if self.cwd_len == 0 {
            "/"
        } else {
            core::str::from_utf8(&self.cwd[..self.cwd_len]).unwrap_or("/")
        }
    }

    fn resolve_path(&self, path: &str) -> alloc::string::String {
        if path.is_empty() {
            return self.cwd_str().to_string();
        }
        if path.starts_with('/') {
            Self::normalize(path)
        } else {
            let base = self.cwd_str();
            if base == "/" {
                Self::normalize(&alloc::format!("/{}", path))
            } else {
                Self::normalize(&alloc::format!("{}/{}", base, path))
            }
        }
    }

    fn normalize(path: &str) -> alloc::string::String {
        let mut parts: Vec<&str> = Vec::new();
        for seg in path.split('/') {
            match seg {
                "" | "." => continue,
                ".." => { parts.pop(); }
                s => parts.push(s),
            }
        }
        if parts.is_empty() {
            "/".to_string()
        } else {
            alloc::format!("/{}", parts.join("/"))
        }
    }

    pub fn run(&mut self) -> ! {
        // ── Welcome banner ──
        let ms = crate::timer::uptime_ms();
        println!("\x1b[36m\x1b[1m  ⚡ ZiqaKernel v0.1 ⚡\x1b[0m");
        println!("\x1b[2m  ─────────────────────────────────────────────\x1b[0m");
        println!("\x1b[32m  System ready\x1b[0m  uptime={}ms  cpu=x86_64  mem={}MiB",
            ms, crate::memory::heap::HEAP_SIZE / (1024 * 1024));
        println!("\x1b[2m  Type 'help' for available commands\x1b[0m");
        println!("");

        self.update_status_bar();

        loop {
            // Poll network stack for incoming/outgoing packets
            crate::net::stack::poll_network();

            let cwd = self.cwd_str();
            // Enhanced prompt with colors and exit status
            let exit_status = if self.last_exit_status != 0 { 
                alloc::format!("{}", self.last_exit_status) 
            } else {
                String::new()
            };
            let time = crate::timer::uptime_ms() / 1000; // seconds since boot
            let prompt = if cwd == "/" {
                alloc::format!("\x1b[36mziqa\x1b[0m\x1b[33m[\x1b[0m{}{}\x1b[33m]\x1b[0m \x1b[32m>\x1b[0m ", 
                    if exit_status.is_empty() { "" } else { &exit_status }, 
                    time)
            } else {
                alloc::format!("\x1b[36mziqa\x1b[0m\x1b[33m[\x1b[0m{}{}\x1b[0m \x1b[33m{}\x1b[33m]\x1b[0m \x1b[32m>\x1b[0m ", 
                    if exit_status.is_empty() { "" } else { &exit_status }, 
                    time, cwd)
            };
            print!("{}", prompt);
            
            self.read_line();

            let has_input = self.input_buf[..self.cursor].iter().any(|&b| b != b' ' && b != b'\t' && b != b'\r' && b != b'\n');
            if has_input {
                self.push_history();
                let input = core::str::from_utf8(&self.input_buf[..self.cursor]).unwrap_or("");
                let trimmed = input.trim();
                let parts: Vec<&str> = trimmed.splitn(3, ' ').collect();
                let mut cmd = parts[0].to_string();
                
                // Alias expansion
                if let Some(expanded) = self.aliases.get(&cmd) {
                    cmd = expanded.clone();
                }

                let arg1 = parts.get(1).copied().map(String::from);
                let arg2 = parts.get(2).copied().map(String::from);
                match cmd.as_str() {
                     "help"    => {
                         self.cmd_help();
                         self.set_exit_status(0);
                     },
                     "uptime"  => {
                         self.cmd_uptime();
                         self.set_exit_status(0);
                     },
                     "klog"    => {
                         self.cmd_klog(arg1.as_deref().unwrap_or("info"));
                         self.set_exit_status(0);
                     },
                     "spawn"   => {
                         self.cmd_spawn(arg1.as_deref());
                         self.set_exit_status(0);
                     },
                     "spawnelf" => {
                         self.cmd_spawn_elf(arg1.as_deref());
                         self.set_exit_status(0);
                     },
                     "exec"    => {
                         self.cmd_exec(arg1.as_deref());
                         self.set_exit_status(0);
                     },
                     "ps"      => {
                         self.cmd_ps();
                         self.set_exit_status(0);
                     },
                     "kill"    => self.cmd_kill(arg1.as_deref(), arg2.as_deref()),
                    "sleep"   => self.cmd_sleep(arg1.as_deref()),
                    "meminfo" => self.cmd_meminfo(),
                    "diskinfo" => self.cmd_diskinfo(),
                    "netstat" => self.cmd_netstat(),
                    "doom"    => self.cmd_doom(arg1.as_deref()),
                    "tetris"  => self.cmd_tetris(),
                    "nwm-test" => {
                        self.cmd_nwm_test();
                        self.set_exit_status(0);
                    },
                    "dashboard" | "top" => self.cmd_dashboard(),
                    "reboot"  => self.cmd_reboot(),
                    "edit"    => self.cmd_edit(arg1.as_deref()),
                    "ls"      => self.cmd_ls(arg1.as_deref()),
                    "cd"      => self.cmd_cd(arg1.as_deref()),
                    "pwd"     => self.cmd_pwd(),
                    "mkdir"   => self.cmd_mkdir(arg1.as_deref()),
                    "dir"     => self.cmd_dir(arg1.as_deref()),
                    "rm"      => self.cmd_rm(arg1.as_deref()),
                    "rmdir"   => self.cmd_rm(arg1.as_deref()),
                    "cat"     => self.cmd_cat(arg1.as_deref()),
                    "mv"      => self.cmd_mv(arg1.as_deref(), arg2.as_deref()),
                    "cp"      => self.cmd_cp(arg1.as_deref(), arg2.as_deref()),
                    "touch"   => self.cmd_touch(arg1.as_deref()),
                    "stat"    => self.cmd_stat(arg1.as_deref()),
                    "du"      => self.cmd_du(arg1.as_deref()),
                     "clear"   => {
                         self.cmd_clear();
                         self.set_exit_status(0);
                     },
                    "echo"    => println!("{}", trimmed.trim_start_matches("echo").trim_start()),
                    "ping"     => self.cmd_ping(trimmed.trim_start_matches("ping").trim_start()),
                    "wget"     => self.cmd_wget(trimmed.trim_start_matches("wget").trim_start()),
                     "ifconfig" => {
                         self.cmd_ifconfig();
                         self.set_exit_status(0);
                     },
                     "history"  => {
                         self.cmd_history();
                         self.set_exit_status(0);
                     },
                     "jobs"     => {
                         self.cmd_jobs();
                         self.set_exit_status(0);
                     },
                     "bg"       => {
                         self.cmd_bg(arg1.as_deref());
                         self.set_exit_status(0);
                     },
                     "fg"       => {
                         self.cmd_fg(arg1.as_deref());
                         self.set_exit_status(0);
                     },
                    "alias"    => self.cmd_alias(arg1.as_deref(), arg2.as_deref()),
                    "export"   => self.cmd_export(arg1.as_deref()),
                     _         => {
                         let suggestion = self.find_similar_command(&cmd);
                         if let Some(s) = suggestion {
                             println!("Unknown command: {}. Did you mean '{}'?", cmd, s);
                         } else {
                             println!("Unknown command: {}. Type 'help'.", cmd);
                         }
                         self.set_exit_status(1);
                     }
                }
                 self.update_status_bar();
                 // Add visual separator after command output
                 println!("\x1b[2m──────────────────────────────────────────────────────────────────────\x1b[0m");
             }
             self.history_pos = -1;
             self.cursor = 0;
         }
    }

    fn push_history(&mut self) {
        let last = self.history.last().map(|e| *e == self.input_buf).unwrap_or(false);
        if !last {
            if self.history.len() >= MAX_HISTORY {
                self.history.remove(0);
            }
            let mut entry = [0u8; 256];
            entry[..self.cursor].copy_from_slice(&self.input_buf[..self.cursor]);
            self.history.push(entry);
        }
    }

    fn prompt_len(&self) -> usize {
        let cwd = self.cwd_str();
        if cwd == "/" {
            "ziqa > ".chars().count()
        } else {
            alloc::format!("ziqa {} > ", cwd).chars().count()
        }
    }

    fn refresh_line(&self, idx: usize) {
        let cwd = self.cwd_str();
        let prompt = if cwd == "/" {
            alloc::format!("ziqa > ")
        } else {
            alloc::format!("ziqa {} > ", cwd)
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

    fn load_history(&mut self, idx: &mut usize) {
        let entry = &self.history[self.history_pos as usize];
        let len = entry.iter().position(|&b| b == 0).unwrap_or(256);
        self.input_buf[..len].copy_from_slice(&entry[..len]);
        *idx = len;
        self.refresh_line(*idx);
    }

    fn autocomplete(&mut self, idx: &mut usize) {
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
                    } else if VFS.lock().is_dir(c) {
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

    fn complete_arg(&self, cmd: &str, prefix: &str) -> Vec<String> {
        if matches!(cmd, "ls" | "cd" | "edit" | "rm" | "cat" | "mkdir" | "dir" | "spawnelf" | "spawn") {
            let vfs = VFS.lock();
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

        if matches!(cmd, "kill" | "exec") {
            let pids = crate::process::scheduler::list_pids();
            return pids.iter().map(|p| alloc::format!("{}", p.0)).filter(|s| s.starts_with(prefix)).collect();
        }

        Vec::new()
    }

    fn find_similar_command(&self, cmd: &str) -> Option<&'static str> {
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

    fn update_status_bar(&self) {
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

    fn read_line(&mut self) {
        use crate::drivers::keyboard::read_stdin;
        let mut idx = 0;
        self.input_buf = [0; 256];
        loop {
            let mut byte = [0u8; 1];
            if read_stdin(&mut byte) > 0 {
                let b = byte[0];
                match b {
                    0x80 => {
                        if !self.history.is_empty() {
                            if self.history_pos < 0 {
                                self.history_pos = self.history.len() as isize - 1;
                            } else if self.history_pos > 0 {
                                self.history_pos -= 1;
                            }
                            self.load_history(&mut idx);
                        }
                    }
                    0x81 => {
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
                            // Shift remaining characters left
                            let len = self.input_buf.iter().position(|&b| b == 0).unwrap_or(256);
                            for i in idx-1..len-1 {
                                self.input_buf[i] = self.input_buf[i+1];
                            }
                            self.input_buf[len-1] = 0;
                            idx -= 1;
                            self.refresh_line(idx); // Redraw
                        }
                    }
                    0x09 => {
                        self.autocomplete(&mut idx);
                    }
                    0x88 => { // Delete
                        if idx < 255 && self.input_buf[idx] != 0 {
                            // Shift remaining characters left starting from current index
                            let len = self.input_buf.iter().position(|&b| b == 0).unwrap_or(256);
                            for i in idx..len-1 {
                                self.input_buf[i] = self.input_buf[i+1];
                            }
                            self.input_buf[len-1] = 0;
                            self.refresh_line(idx); // Redraw
                        }
                    }
                    b'\n' | b'\r' => {
                        self.cursor = idx;
                        if crate::drivers::vga::is_scrolled() {
                            crate::drivers::vga::restore_terminal();
                        }
                        println!("");
                        break;
                    }
                    0x03 => {
                        self.cursor = idx;
                        println!("^C");
                        self.input_buf = [0; 256];
                        idx = 0;
                        self.refresh_line(idx);
                    }
                    0x0C => {
                        self.cmd_clear();
                        self.refresh_line(idx);
                    }
                    0x04 => {
                        println!("^D - Exiting shell");
                        // In a real shell this might exit the process or logout
                        // For now we just break the current input loop
                        break;
                    }
                    _ => {
                        if idx < 255 {
                            // Shift remaining characters right to make space
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
                x86_64::instructions::hlt();
            }
        }
    }

    fn cmd_alias(&mut self, arg1: Option<&str>, arg2: Option<&str>) {
        match (arg1, arg2) {
            (Some(name), Some(value)) => {
                self.aliases.insert(name.to_string(), value.to_string());
                println!("alias {}='{}'", name, value);
            }
            (Some(name), None) => {
                if let Some(value) = self.aliases.get(name) {
                    println!("alias {}='{}'", name, value);
                } else {
                    println!("alias: {}: not found", name);
                }
            }
            _ => {
                println!("{}{}  ALIASES {} {}", C_YELLOW, C_BOLD, C_RESET, C_DIM);
                for (name, value) in &self.aliases {
                    println!("  {}='{}'", name, value);
                }
            }
        }
    }

    fn cmd_export(&mut self, arg1: Option<&str>) {
        match arg1 {
            Some(arg) => {
                if let Some((name, value)) = arg.split_once('=') {
                    self.env.insert(name.to_string(), value.to_string());
                    println!("export {}={}", name, value);
                } else {
                    println!("export: invalid format, use NAME=VALUE");
                }
            }
            None => {
                println!("{}{}  ENVIRONMENT {} {}", C_YELLOW, C_BOLD, C_RESET, C_DIM);
                for (name, value) in &self.env {
                    println!("  {}={}", name, value);
                }
            }
        }
    }

    fn cmd_help(&self) {
        println!("{}{}  ⚡ ZiqaKernel Shell ⚡{}", C_CYAN, C_BOLD, C_RESET);
        println!("{}  ─────────────────────────────────────{}", C_DIM, C_RESET);
        println!("");

        let groups: &[(&str, &[(&str, &str)])] = &[
            ("Filesystem", &[
                ("ls [path]",        "list directory contents"),
                ("dir [path]",       "detailed listing with sizes"),
                ("cd [path]",        "change directory (.. / - for previous)"),
                ("pwd",              "print working directory"),
                ("mkdir <path>",     "create a directory"),
                ("rm <path>",        "remove a file"),
                ("cat [-n] <path>",  "display file (-n for line numbers)"),
                ("mv <src> <dst>",   "move/rename a file"),
                ("cp <src> <dst>",   "copy a file"),
                ("touch <path>",     "create file or update mtime"),
                ("stat <path>",      "show inode details"),
                ("du [path]",        "disk usage in blocks"),
                ("edit <path>",      "nano-like text editor"),
            ]),
            ("Process", &[
                ("ps",               "list all processes"),
                ("spawn [path]",     "spawn skeleton or ELF process"),
                ("spawnelf <path>",  "spawn ELF from VFS"),
                ("exec <pid>",       "execute process entry point"),
                ("kill <pid> [sig]", "send signal (name or number)"),
                ("sleep <ms|Ns>",    "sleep milliseconds or Ns seconds"),
            ]),
            ("System", &[
                ("help",             "show this message"),
                ("uptime",           "kernel uptime + system summary"),
                ("meminfo",          "heap memory statistics"),
                ("diskinfo",         "ZiqaFS disk usage + fsck"),
                ("klog [lvl] [-N]",  "kernel log (debug/info/error, -N last N)"),
                ("dashboard",        "real-time system dashboard"),
                ("reboot",           "reboot the system"),
                ("clear",            "clear screen"),
                ("echo <text>",      "print text"),
                ("history",          "show command history"),
                ("alias [n=v]",      "define or list aliases"),
                ("export N=V",       "set environment variable"),
            ]),
            ("Network", &[
                ("netstat",          "network device statistics"),
                ("ifconfig",         "interface addresses and stats"),
                ("ping [-c N] <ip>", "ICMP echo with RTT stats"),
                ("wget [-O f] <url>","HTTP GET, saves to /tmp/"),
            ]),
            ("Entertainment", &[
                ("doom [steps]",     "DOOM fire demo (SPACE=blow, T=tornado)"),
                ("tetris",           "graphical Tetris on VGA console"),
                ("nwm-test",         "launch native Wayland compositor + Zig client"),
            ]),
        ];

        for (group_name, cmds) in groups {
            println!("{}  {} ›{}", C_YELLOW, group_name, C_RESET);
            for (cmd, desc) in *cmds {
                println!("    {:<18} {}{}{}", cmd, C_DIM, desc, C_RESET);
            }
            println!("");
        }
    }

    fn cmd_history(&self) {
        println!("{}{}  HISTORY {} {}", C_YELLOW, C_BOLD, C_RESET, C_DIM);
        for (i, entry) in self.history.iter().enumerate() {
            let len = entry.iter().position(|&b| b == 0).unwrap_or(256);
            if let Ok(s) = core::str::from_utf8(&entry[..len]) {
                println!("  {:>3}: {}", i, s);
            }
        }
    }

    fn cmd_uptime(&self) {
        let ms = crate::timer::uptime_ms();
        let secs = ms / 1000;
        let mins = secs / 60;
        let hrs = mins / 60;
        let days = hrs / 24;
        let proc_count = crate::process::scheduler::SCHEDULER.lock().get_pid_list().len();
        let stats = crate::memory::heapstats::get_stats();
        let mem_pct = (stats.current_usage_bytes() * 100) / crate::memory::heap::HEAP_SIZE as u64;
        println!("{}{}  UPTIME{}", C_YELLOW, C_BOLD, C_RESET);
        if days > 0 {
            println!("  up {}d {:02}:{:02}:{:02},  {} processes,  mem {}% used",
                days, hrs % 24, mins % 60, secs % 60, proc_count, mem_pct);
        } else {
            println!("  up {:02}:{:02}:{:02},  {} processes,  mem {}% used",
                hrs, mins % 60, secs % 60, proc_count, mem_pct);
        }
        println!("  {}ticks: {}  raw: {}ms{}", C_DIM, crate::timer::uptime_ticks(), ms, C_RESET);
    }

    fn cmd_klog(&self, level_str: &str) {
        use crate::klog::Level;
        // Parse: klog [level] [-n count]
        let mut level = Level::Info;
        let mut limit: usize = usize::MAX;
        for part in level_str.split_whitespace() {
            match part {
                "debug" => level = Level::Debug,
                "warn"  => level = Level::Warn,
                "error" => level = Level::Error,
                "info"  => level = Level::Info,
                n if n.starts_with('-') => {
                    if let Ok(v) = n[1..].parse::<usize>() { limit = v; }
                }
                _ => {}
            }
        }
        let klog = crate::klog::KLOG.lock();
        let total = klog.count();
        let skip = if total > limit { total - limit } else { 0 };
        let mut shown = 0;
        for (i, entry) in klog.iter().enumerate() {
            if i < skip { continue; }
            if entry.level <= level {
                let ms = entry.tick * 10; // ticks → ms (10ms/tick)
                let level_color = match entry.level {
                    Level::Error => C_RED,
                    Level::Warn  => C_YELLOW,
                    Level::Info  => C_GREEN,
                    Level::Debug => C_DIM,
                };
                println!("{}[{}]{} [{:>8}ms] {}",
                    level_color, entry.level.as_str(), C_RESET,
                    ms, entry.message());
                shown += 1;
            }
        }
        if shown == 0 {
            println!("{}(no log entries at level {:?}){}", C_DIM, level, C_RESET);
        } else {
            println!("{}-- {} entries shown --{}", C_DIM, shown, C_RESET);
        }
    }

    fn cmd_spawn(&self, path: Option<&str>) {
        if let Some(p) = path {
            self.cmd_spawn_elf(Some(p))
        } else {
            let pid = crate::process::scheduler::spawn(
                AbiKind::LinuxElf,
                VirtAddr::new(0x400000),
                VirtAddr::new(0x7fff_ffff_000),
            );
            match pid {
                Some(p) => println!("Spawned PID={} (skeleton)", p.0),
                None    => println!("spawn: no free slots"),
            }
        }
    }

    fn cmd_spawn_elf(&self, path: Option<&str>) {
        let p = match path {
            Some(s) => s,
            None => { println!("Usage: spawnelf <path>"); return; }
        };
        let resolved = self.resolve_path(p);
        let mut buf = [0u8; 65536];
        match crate::fs::vfs::VFS.lock().read_raw(&resolved, &mut buf, 0) {
            Ok(n) if n > 0 => {
                let data = &buf[..n];
                match crate::process::scheduler::spawn_elf(data) {
                    Some(pid) => println!("Spawned PID={} from '{}'", pid.0, resolved),
                    None => println!("spawnelf: failed to spawn from '{}'", resolved),
                }
            }
            _ => println!("spawnelf: file '{}' not found in VFS", resolved),
        }
    }

    fn cmd_exec(&self, pid_str: Option<&str>) {
        let pid_val = match pid_str.and_then(|s| s.parse::<u64>().ok()) {
            Some(v) => v,
            None => { println!("Usage: exec <pid>"); return; }
        };
        let pid = crate::process::Pid(pid_val);

        let _entry_vaddr = {
            let mut sched = crate::process::scheduler::SCHEDULER.lock();
            if !sched.set_current(pid) {
                println!("exec: no process with PID {}", pid_val);
                return;
            }
            let proc = sched.current_task().unwrap();
            let entry = proc.entry_point.as_u64();
            println!("[EXEC] Switching to PID {} entry=0x{:x}", pid_val, entry);
            entry
        };

        let mut sched = crate::process::scheduler::SCHEDULER.lock();
        if let Some(proc) = sched.get_process_mut(pid) {
            proc.state = crate::process::ProcessState::Ready;
        }
        drop(sched);
        
        crate::process::scheduler::yield_now();
    }

    fn cmd_ps(&self) {
        use crate::process::scheduler::SCHEDULER;
        let rows: alloc::vec::Vec<(u64, crate::process::ProcessState, u8, u64, crate::process::AbiKind, u64)> = {
            let sched = SCHEDULER.lock();
            sched.get_pid_list().into_iter().filter_map(|pid| {
                sched.get_process(pid).map(|p| (
                    p.pid.0, p.state, p.priority, p.parent, p.abi, p.entry_point.as_u64(),
                ))
            }).collect()
        };
        println!("{}{}  PROCESSES  ({} total){}", C_YELLOW, C_BOLD, rows.len(), C_RESET);
        println!("  ┌──────┬──────────────┬──────┬────────┬──────────┬────────────────┐");
        println!("  │ {}PID  │ STATE        │ PRI  │ PARENT │ ABI      │ ENTRY          {}│", C_CYAN, C_RESET);
        println!("  ├──────┼──────────────┼──────┼────────┼──────────┼────────────────┤");
        for (pid, state, pri, parent, abi, entry) in &rows {
            let (sc, ss) = match state {
                crate::process::ProcessState::Running  => (C_GREEN,  "Running     "),
                crate::process::ProcessState::Ready    => (C_CYAN,   "Ready       "),
                crate::process::ProcessState::Blocked  => (C_YELLOW, "Blocked     "),
                crate::process::ProcessState::Created  => (C_DIM,    "Created     "),
                crate::process::ProcessState::Exited(_)=> (C_RED,    "Exited      "),
            };
            let abi_s = match abi {
                crate::process::AbiKind::LinuxElf   => "Linux/ELF ",
                crate::process::AbiKind::Wasm        => "WASM      ",
                crate::process::AbiKind::ZiqaNative  => "ZiqaNative",
            };
            println!("  │ {:>4} │ {}{}{} │ {:>4} │ {:>6} │ {} │ 0x{:012x} │",
                pid, sc, ss, C_RESET, pri, parent, abi_s, entry);
        }
        println!("  └──────┴──────────────┴──────┴────────┴──────────┴────────────────┘");
    }

    fn cmd_kill(&self, pid_str: Option<&str>, sig_str: Option<&str>) {
        let pid_val = match pid_str.and_then(|s| s.parse::<u64>().ok()) {
            Some(v) => v,
            None => { println!("Usage: kill <pid> [signal]\n  Signals: SIGTERM(15) SIGKILL(9) SIGINT(2) SIGHUP(1) SIGUSR1(10) SIGUSR2(12)"); return; }
        };
        let signum: u8 = match sig_str {
            None => 15,
            Some(s) => match s.to_uppercase().as_str() {
                "SIGTERM" | "TERM" => 15,
                "SIGKILL" | "KILL" => 9,
                "SIGINT"  | "INT"  => 2,
                "SIGHUP"  | "HUP"  => 1,
                "SIGUSR1" | "USR1" => 10,
                "SIGUSR2" | "USR2" => 12,
                "SIGSTOP" | "STOP" => 19,
                "SIGCONT" | "CONT" => 18,
                n => n.parse().unwrap_or(15),
            }
        };
        let sig_name = match signum {
            1 => "SIGHUP", 2 => "SIGINT", 9 => "SIGKILL", 10 => "SIGUSR1",
            12 => "SIGUSR2", 15 => "SIGTERM", 18 => "SIGCONT", 19 => "SIGSTOP", _ => "SIG",
        };
        let ok = crate::process::scheduler::SCHEDULER.lock()
            .send_signal(crate::process::Pid(pid_val), signum);
        if ok {
            println!("Sent {}({}) to PID {}", sig_name, signum, pid_val);
        } else {
            println!("kill: ({}) - No such process", pid_val);
        }
    }

    fn cmd_sleep(&self, ms_str: Option<&str>) {
        let arg = match ms_str {
            Some(v) => v,
            None => { println!("Usage: sleep <ms>  or  sleep <N>s"); return; }
        };
        let ms: u64 = if let Some(s) = arg.strip_suffix('s') {
            s.parse::<u64>().unwrap_or(0) * 1000
        } else {
            arg.parse().unwrap_or(0)
        };
        if ms == 0 { println!("sleep: invalid duration '{}'", arg); return; }
        crate::timer::sleep_ms(crate::process::Pid(0), ms);
        if ms >= 1000 { println!("Slept {}.{}s", ms / 1000, (ms % 1000) / 100); }
        else { println!("Slept {}ms", ms); }
    }

    fn cmd_meminfo(&self) {
        let stats = crate::memory::heapstats::get_stats();
        let heap_size = crate::memory::heap::HEAP_SIZE;
        let heap_kib = heap_size / 1024;
        let heap_mib = heap_kib / 1024;
        let used = stats.current_usage_bytes() as usize;
        let peak = stats.peak_usage_bytes as usize;
        let pct = used * 100 / heap_size;
        let peak_pct = peak * 100 / heap_size;

        // 40-char bar
        let bar_width = 40usize;
        let filled = used * bar_width / heap_size;
        let peak_pos = peak * bar_width / heap_size;

        println!("{}{}  MEMORY{}", C_YELLOW, C_BOLD, C_RESET);
        println!("  Heap:  0x{:x}  size: {} MiB ({} KiB)", crate::memory::heap::HEAP_START, heap_mib, heap_kib);
        println!("");

        // Bar: [████████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░]
        print!("  [");
        for i in 0..bar_width {
            if i < filled {
                let c = if pct > 80 { C_RED } else if pct > 50 { C_YELLOW } else { C_GREEN };
                print!("{}█{}", c, C_RESET);
            } else if i == peak_pos {
                print!("{}|{}", C_YELLOW, C_RESET);
            } else {
                print!("░");
            }
        }
        println!("] {}%", pct);
        println!("  Used:  {} KiB / {} KiB  (peak: {} KiB, {}%)",
            used / 1024, heap_kib, peak / 1024, peak_pct);
        println!("  Allocs: {}  Frees: {}  Live blocks: {}",
            stats.total_allocations, stats.total_frees, stats.current_blocks);
    }

    fn cmd_diskinfo(&self) {
        use crate::fs::ziqafs::ZIQAFS;
        let guard = ZIQAFS.lock();
        if let Some(fs_arc) = guard.as_ref() {
            let mut fs = fs_arc.lock();
            let st = crate::fs::ziqafs::ZiqaFs::statfs(&fs);
            let total_kib = st.total_blocks as u64 * st.block_size as u64 / 1024;
            let free_kib  = st.free_blocks  as u64 * st.block_size as u64 / 1024;
            let used_kib  = total_kib - free_kib;
            let pct = if total_kib > 0 { used_kib * 100 / total_kib } else { 0 } as usize;

            println!("{}{}  DISK (ZiqaFS){}", C_YELLOW, C_BOLD, C_RESET);
            println!("  Block size: {} B   Total: {} KiB   Used: {} KiB   Free: {} KiB",
                st.block_size, total_kib, used_kib, free_kib);
            println!("  Inodes: {}/{} used", st.total_inodes - st.free_inodes, st.total_inodes);
            println!("");

            // Disk bar
            let bar_width = 40usize;
            let filled = pct * bar_width / 100;
            print!("  [");
            for i in 0..bar_width {
                if i < filled {
                    let c = if pct > 80 { C_RED } else if pct > 50 { C_YELLOW } else { C_GREEN };
                    print!("{}█{}", c, C_RESET);
                } else {
                    print!("░");
                }
            }
            println!("] {}%", pct);

            let r = crate::fs::ziqafs::ZiqaFs::fsck(&mut fs);
            if r.ok {
                println!("  fsck: {}OK{}", C_GREEN, C_RESET);
            } else {
                println!("  fsck: {}ERRORS — errs={} leaked_blocks={} leaked_inodes={}{}",
                    C_RED, r.errors, r.leaked_blocks, r.leaked_inodes, C_RESET);
            }
        } else {
            println!("diskinfo: ZiqaFS not mounted");
        }
    }

    fn cmd_netstat(&self) {
        println!("{}{}  NETWORK INTERFACES {}", C_YELLOW, C_BOLD, C_RESET);
        println!("  ┌──────┬──────────────┬──────────────┬──────────────┬──────────────┐");
        println!("  │ {}Dev  │ {}TX pkts     │ {}RX pkts     │ {}TX bytes    │ {}RX bytes    {}│", C_CYAN, C_GREEN, C_CYAN, C_GREEN, C_CYAN, C_RESET);
        println!("  ├──────┼──────────────┼──────────────┼──────────────┼──────────────┤");
        {
            let guard = crate::net::NET.lock();
            for slot in guard.devices.iter() {
                if let Some(dev) = slot {
                    println!("  │ {:<4} │ {:>12} │ {:>12} │ {:>12} │ {:>12} │",
                        dev.name, dev.tx_packets, dev.rx_packets, dev.tx_bytes, dev.rx_bytes);
                }
            }
        }
        println!("  └──────┴──────────────┴──────────────┴──────────────┴──────────────┘");
        println!("");
        // IP config summary
        let stack = crate::net::stack::TCPIP.lock();
        if let Some(s) = stack.as_ref() {
            for cidr in s.iface.ip_addrs() {
                println!("  eth0  inet {}  gw {}", cidr.address(), s.gateway());
            }
        }
        println!("  lo    inet 127.0.0.1/8  loopback");
    }

    fn cmd_reboot(&self) {
        println!("Rebooting...");
        unsafe {
            use x86_64::instructions::port::Port;
            let mut port: Port<u8> = Port::new(0x64);
            port.write(0xFE);
        }
        loop { x86_64::instructions::hlt(); }
    }

    fn cmd_dashboard(&self) {
        crate::drivers::vga::clear_screen();
        let ms = crate::timer::uptime_ms();
        let secs = ms / 1000;
        let mins = secs / 60;
        let hrs = mins / 60;
        let days = hrs / 24;
        let stats = crate::memory::heapstats::get_stats();
        let heap_size = crate::memory::heap::HEAP_SIZE;
        let used = stats.current_usage_bytes() as usize;
        let pct = used * 100 / heap_size;

        println!("\x1b[36m\x1b[1m  ╔══════════════════════════════════════════╗\x1b[0m");
        println!("\x1b[36m\x1b[1m  ║     ZIQA KERNEL SYSTEM DASHBOARD        ║\x1b[0m");
        println!("\x1b[36m\x1b[1m  ╚══════════════════════════════════════════╝\x1b[0m");

        // ── System ──
        println!("\n\x1b[33m  ▸ SYSTEM\x1b[0m");
        if days > 0 {
            println!("    Uptime:  {}d {:02}:{:02}:{:02}", days, hrs % 24, mins % 60, secs % 60);
        } else {
            println!("    Uptime:  {:02}:{:02}:{:02}", hrs, mins % 60, secs % 60);
        }
        println!("    Arch:    x86_64  |  Heap: {} MiB", heap_size / 1024 / 1024);

        // Memory bar
        let bar = 30usize;
        let filled = pct * bar / 100;
        let mem_color = if pct > 80 { "\x1b[31m" } else if pct > 50 { "\x1b[33m" } else { "\x1b[32m" };
        print!("    Mem:     [");
        for i in 0..bar {
            if i < filled { print!("{}█\x1b[0m", mem_color); } else { print!("░"); }
        }
        println!("] {}%  ({} KiB / {} KiB)", pct, used / 1024, heap_size / 1024);
        println!("    Allocs:  {}  Frees: {}  Peak: {} KiB",
            stats.total_allocations, stats.total_frees, stats.peak_usage_bytes as usize / 1024);

        // ── Processes ──
        println!("\n\x1b[33m  ▸ PROCESSES\x1b[0m");
        use crate::process::scheduler::SCHEDULER;
        let proc_rows: alloc::vec::Vec<(u64, crate::process::ProcessState, u8)> = {
            let sched = SCHEDULER.lock();
            sched.get_pid_list().into_iter().filter_map(|pid| {
                sched.get_process(pid).map(|p| (p.pid.0, p.state, p.priority))
            }).collect()
        };
        println!("    Total: \x1b[32m{}\x1b[0m", proc_rows.len());
        for (pid, state, pri) in &proc_rows {
            let (sc, ss) = match state {
                crate::process::ProcessState::Running   => ("\x1b[32m", "Run"),
                crate::process::ProcessState::Ready     => ("\x1b[36m", "Rdy"),
                crate::process::ProcessState::Blocked   => ("\x1b[33m", "Blk"),
                crate::process::ProcessState::Exited(_) => ("\x1b[31m", "Ext"),
                _                                        => ("\x1b[2m",  "New"),
            };
            print!("    PID={} {}[{}]\x1b[0m pri={}   ", pid, sc, ss, pri);
        }
        if !proc_rows.is_empty() { println!(); }

        // ── Network ──
        println!("\n\x1b[33m  ▸ NETWORK\x1b[0m");
        {
            let guard = crate::net::NET.lock();
            for slot in guard.devices.iter() {
                if let Some(dev) = slot {
                    println!("    {:<6}  TX: {:>6} pkts {:>8} B   RX: {:>6} pkts {:>8} B",
                        dev.name, dev.tx_packets, dev.tx_bytes, dev.rx_packets, dev.rx_bytes);
                }
            }
        }

        // ── Storage ──
        println!("\n\x1b[33m  ▸ STORAGE\x1b[0m");
        use crate::fs::ziqafs::ZIQAFS;
        let guard = ZIQAFS.lock();
        if let Some(fs_arc) = guard.as_ref() {
            let fs = fs_arc.lock();
            let st = crate::fs::ziqafs::ZiqaFs::statfs(&fs);
            let total_kib = st.total_blocks as u64 * st.block_size as u64 / 1024;
            let free_kib  = st.free_blocks  as u64 * st.block_size as u64 / 1024;
            let used_kib  = total_kib - free_kib;
            let dpct = if total_kib > 0 { used_kib * 100 / total_kib } else { 0 } as usize;
            let dfilled = dpct * 30 / 100;
            let dc = if dpct > 80 { "\x1b[31m" } else if dpct > 50 { "\x1b[33m" } else { "\x1b[32m" };
            print!("    ZiqaFS   [");
            for i in 0..30usize { if i < dfilled { print!("{}█\x1b[0m", dc); } else { print!("░"); } }
            println!("] {}%  ({}/{} KiB)", dpct, used_kib, total_kib);
            println!("    Inodes:  {}/{} used", st.total_inodes - st.free_inodes, st.total_inodes);
        } else {
            println!("    ZiqaFS: \x1b[2mnot mounted\x1b[0m");
        }

        println!("\n\x1b[2m  Press any key to return...\x1b[0m");
        crate::drivers::keyboard::clear_stdin();
        loop {
            let mut k = [0u8; 1];
            if crate::drivers::keyboard::read_stdin(&mut k) > 0 { break; }
            x86_64::instructions::hlt();
        }
        crate::drivers::vga::clear_screen();
    }

    fn cmd_doom(&self, steps_str: Option<&str>) {
        let steps: usize = steps_str.and_then(|s| s.parse().ok()).unwrap_or(60);
        crate::doom::run(steps);
    }

    fn cmd_nwm_test(&self) {
        println!("{}{}  🚀 LAUNCHING NATIVE COMPOSITOR (NWCC) ...{}", C_CYAN, C_BOLD, C_RESET);
        println!("  - Spawning Compositor Task...");
        
        // Spawn Compositor as a separate task
        crate::process::scheduler::SCHEDULER.lock().spawn_kthread(|| {
            crate::userspace::compositor::start();
        });

        // Small delay to let compositor create its IPC channel
        crate::timer::sleep_ms(crate::process::Pid(0), 100);

        println!("  - Spawning Zig-accelerated Demo Client...");
        
        // Spawn Zig Client as a separate task
        crate::process::scheduler::SCHEDULER.lock().spawn_kthread(|| {
            unsafe { crate::zig_ffi::zig_demo_client_main(); }
        });

        println!("\n  {}GUI Active. Use mouse to drag windows.{}", C_GREEN, C_RESET);
    }

    fn cmd_clear(&self) {
        crate::drivers::vga::clear_screen();
        if crate::drivers::vga::is_scrolled() {
            crate::drivers::vga::restore_terminal();
        }
        self.update_status_bar();
        use core::fmt::Write;
        let mut serial = crate::drivers::uart::SERIAL1.lock();
        write!(serial, "\x1b[2J\x1b[H").ok();
    }

    fn cmd_tetris(&self) {
        crate::tetris::run();
    }

    fn cmd_edit(&self, path: Option<&str>) {
        let p = match path {
            Some(s) => s,
            None => { println!("Usage: edit <path>"); return; }
        };
        let resolved = self.resolve_path(p);
        crate::edit::edit_file(&resolved);
    }

    fn cmd_ls(&self, target: Option<&str>) {
        let dir = target.map(|p| self.resolve_path(p)).unwrap_or_else(|| self.cwd_str().to_string());
        let vfs = VFS.lock();
        if !vfs.is_dir(&dir) {
            println!("{}ls{}: {}: {}No such directory{}", C_RED, C_RESET, dir, C_DIM, C_RESET);
            return;
        }
        let entries = vfs.list_dir(&dir);

        println!("{}{}  [{}]{}", C_CYAN, C_BOLD, dir, C_RESET);

        println!("  {}D  {} .   {} (parent){}", C_BLUE, C_DIM, dir, C_RESET);
        println!("  {}D  {} ..  {} (parent){}", C_BLUE, C_DIM, dir, C_RESET);

        if entries.is_empty() {
            println!("  {}(empty){}", C_DIM, C_RESET);
        } else {
            for e in &entries {
                let name = e.rsplit('/').next().unwrap_or(e.as_str());
                if vfs.is_dir(e) {
                    println!("  {}D  {}{}/{}", C_BLUE, C_RESET, name, C_RESET);
                } else {
                    let size = vfs.file_size(e).unwrap_or(0);
                    let color = if name.ends_with(".elf") || name.contains("bin") {
                        C_GREEN
                    } else if name.ends_with(".txt") || name.ends_with(".md") {
                        C_YELLOW
                    } else if name.ends_with(".wasm") {
                        C_CYAN
                    } else if name.ends_with(".rs") || name.ends_with(".zig") {
                        "\x1b[35m" // magenta for source
                    } else {
                        C_RESET
                    };
                    println!("  {:>8}  {}{}{}", size, color, name, C_RESET);
                }
            }
        }
    }

    fn cmd_cd(&mut self, target: Option<&str>) {
        let raw = target.unwrap_or("/");
        let resolved = if raw == "-" {
            if self.prev_cwd_len == 0 {
                println!("{}cd{}: {}no previous directory{}", C_RED, C_RESET, C_DIM, C_RESET);
                return;
            }
            core::str::from_utf8(&self.prev_cwd[..self.prev_cwd_len]).unwrap_or("/").to_string()
        } else {
            self.resolve_path(raw)
        };
        let vfs = VFS.lock();
        if !vfs.is_dir(&resolved) {
            println!("{}cd{}: {}: {}No such directory{}", C_RED, C_RESET, resolved, C_DIM, C_RESET);
            return;
        }
        // Save current as previous before changing
        let cur = alloc::string::String::from(self.cwd_str());
        let prev_bytes = cur.as_bytes();
        let pn = prev_bytes.len().min(255);
        self.prev_cwd[..pn].copy_from_slice(&prev_bytes[..pn]);
        self.prev_cwd_len = pn;

        let bytes = resolved.as_bytes();
        let n = bytes.len().min(255);
        self.cwd[..n].copy_from_slice(&bytes[..n]);
        self.cwd_len = n;

        println!("{}▸ {}{}", C_GREEN, self.cwd_str(), C_RESET);
    }

    fn cmd_pwd(&self) {
        println!("{}", self.cwd_str());
    }

    fn cmd_mkdir(&self, target: Option<&str>) {
        let p = match target {
            Some(s) => s,
            None => { println!("Usage: mkdir <path>"); return; }
        };
        let resolved = self.resolve_path(p);
        let mut vfs = VFS.lock();
        if vfs.exists(&resolved) {
            println!("mkdir: {}: File exists", resolved);
            return;
        }
        if resolved.starts_with("/disk/") {
            let name = resolved.trim_start_matches("/disk/");
            let fs_guard = ZIQAFS.lock();
            if let Some(ref fs) = *fs_guard {
                let parent_id = if let Some(idx) = name.rfind('/') {
                    let dir_part = &name[..idx];
                    let mut fsl = fs.lock();
                    ZiqaFs::root_lookup(&mut fsl, &alloc::format!("/disk/{}", dir_part)).unwrap_or(ROOT_INODE)
                } else {
                    ROOT_INODE
                };
                let leaf_name = name.rsplit('/').next().unwrap_or(name);
                let mut fsl = fs.lock();
                match ZiqaFs::create_dir(&mut fsl, parent_id, leaf_name) {
                    Ok(_) => {
                        vfs.mkdir(&resolved);
                        println!("mkdir: created {} (ziqafs)", resolved);
                        return;
                    }
                    Err(e) => {
                        println!("mkdir: {}: {:?}", resolved, e);
                        return;
                    }
                }
            }
        }
        vfs.mkdir(&resolved);
        println!("mkdir: created {}", resolved);
    }

    fn cmd_dir(&self, target: Option<&str>) {
        let dir = target.map(|p| self.resolve_path(p)).unwrap_or_else(|| self.cwd_str().to_string());
        let vfs = VFS.lock();
        if !vfs.is_dir(&dir) {
            println!("dir: {}: No such directory", dir);
            return;
        }
        let entries = vfs.list_dir(&dir);
        println!(" Directory of {}", dir);
        println!("");
        let mut total: u64 = 0;
        for e in &entries {
            let name = e.rsplit('/').next().unwrap_or(e.as_str());
            if vfs.is_dir(e) {
                println!("  {:>10}  <DIR>   {}", "", name);
            } else {
                let size = vfs.file_size(e).unwrap_or(0);
                total += size as u64;
                println!("  {:>10}         {}", size, name);
            }
        }
        println!("");
        println!("  {} file(s)  {} bytes total", entries.len(), total);
    }

    fn cmd_rm(&self, target: Option<&str>) {
        let p = match target {
            Some(s) => s,
            None => { println!("Usage: rm <path>"); return; }
        };
        let resolved = self.resolve_path(p);
        if resolved.starts_with("/disk/") {
            let name = resolved.trim_start_matches("/disk/");
            let fs_guard = ZIQAFS.lock();
            if let Some(ref fs) = *fs_guard {
                let parent_id = if let Some(idx) = name.rfind('/') {
                    let dir_part = &name[..idx];
                    let mut fsl = fs.lock();
                    ZiqaFs::root_lookup(&mut fsl, &alloc::format!("/disk/{}", dir_part)).unwrap_or(ROOT_INODE)
                } else {
                    ROOT_INODE
                };
                let leaf_name = name.rsplit('/').next().unwrap_or(name);
                let mut fsl = fs.lock();
                let _ = ZiqaFs::unlink(&mut fsl, parent_id, leaf_name);
            }
        }
        match VFS.lock().remove(&resolved) {
            Ok(_) => println!("rm: removed {}", resolved),
            Err(_) => println!("rm: {}: No such file", resolved),
        }
    }

    fn cmd_cat(&self, target: Option<&str>) {
        let p = match target {
            Some(s) => s,
            None => { println!("Usage: cat [-n] <path>"); return; }
        };
        let (line_numbers, path) = if p == "-n" {
            println!("Usage: cat [-n] <path>"); return;
        } else if let Some(rest) = p.strip_prefix("-n ") {
            (true, rest)
        } else {
            (false, p)
        };
        let resolved = self.resolve_path(path);
        let mut buf = [0u8; 65536];
        match VFS.lock().read_raw(&resolved, &mut buf, 0) {
            Ok(0) | Err(_) => println!("cat: {}: No such file or directory", resolved),
            Ok(n) => {
                if let Ok(s) = core::str::from_utf8(&buf[..n]) {
                    if line_numbers {
                        for (i, line) in s.lines().enumerate() {
                            println!("{:>6}  {}", i + 1, line);
                        }
                    } else {
                        print!("{}", s);
                        if !s.ends_with('\n') { println!(); }
                    }
                } else {
                    // Binary: hex dump first 256 bytes
                    println!("(binary, {} bytes)", n);
                    for (i, chunk) in buf[..n.min(256)].chunks(16).enumerate() {
                        print!("  {:04x}  ", i * 16);
                        for b in chunk { print!("{:02x} ", b); }
                        println!();
                    }
                    if n > 256 { println!("  ... ({} more bytes)", n - 256); }
                }
            }
        }
    }

    fn cmd_ping(&self, args: &str) {
        // Parse: ping [-c count] <host>
        let mut count: usize = 4;
        let mut host = "";
        let parts: alloc::vec::Vec<&str> = args.split_whitespace().collect();
        let mut i = 0;
        while i < parts.len() {
            match parts[i] {
                "-c" => {
                    i += 1;
                    count = parts.get(i).and_then(|s| s.parse().ok()).unwrap_or(4);
                }
                h => host = h,
            }
            i += 1;
        }
        if host.is_empty() {
            println!("Usage: ping [-c count] <ip>");
            println!("  Note: only IPv4 literals supported (e.g. ping 10.0.2.2)");
            return;
        }

        let ip = match crate::net::dns::resolve(host) {
            Some(ip) => ip,
            None => {
                println!("ping: {}: Name or service not known", host);
                println!("  Hint: use an IPv4 address (e.g. 10.0.2.2 for gateway)");
                return;
            }
        };

        println!("PING {} ({}) 56(84) bytes of data.", host, ip);

        let mut stack_guard = crate::net::stack::TCPIP.lock();
        if let Some(stack) = stack_guard.as_mut() {
            use smoltcp::socket::icmp;
            use smoltcp::wire::{IpAddress, Icmpv4Repr, Icmpv4Packet};

            let icmp_rx_buffer = icmp::PacketBuffer::new(
                alloc::vec![icmp::PacketMetadata::EMPTY; 8],
                alloc::vec![0; 2048],
            );
            let icmp_tx_buffer = icmp::PacketBuffer::new(
                alloc::vec![icmp::PacketMetadata::EMPTY; 8],
                alloc::vec![0; 2048],
            );
            let handle = stack.sockets.add(icmp::Socket::new(icmp_rx_buffer, icmp_tx_buffer));
            let ident: u16 = (crate::timer::uptime_ms() & 0xFFFF) as u16;
            stack.sockets.get_mut::<icmp::Socket>(handle).bind(icmp::Endpoint::Ident(ident)).ok();

            let mut received: usize = 0;
            let mut rtt_min = u64::MAX;
            let mut rtt_max = 0u64;
            let mut rtt_sum = 0u64;

            for seq in 0..count as u16 {
                let start = crate::timer::uptime_ms();
                {
                    let socket = stack.sockets.get_mut::<icmp::Socket>(handle);
                    let repr = Icmpv4Repr::EchoRequest {
                        ident,
                        seq_no: seq,
                        data: b"ziqa-ping-payload-56bytes-padding-here-1234567890ab",
                    };
                    if let Some(payload) = socket.send(repr.buffer_len(), IpAddress::Ipv4(ip)).ok() {
                        let mut pkt = Icmpv4Packet::new_unchecked(payload);
                        repr.emit(&mut pkt, &smoltcp::phy::ChecksumCapabilities::default());
                    }
                }

                let mut got = false;
                let deadline = start + 2000;
                while crate::timer::uptime_ms() < deadline {
                    let serviced = stack.poll();
                    let socket = stack.sockets.get_mut::<icmp::Socket>(handle);
                    if socket.can_recv() {
                        if let Ok(_) = socket.recv() {
                            let rtt = crate::timer::uptime_ms() - start;
                            println!("64 bytes from {}: icmp_seq={} ttl=64 time={} ms", ip, seq, rtt);
                            rtt_min = rtt_min.min(rtt);
                            rtt_max = rtt_max.max(rtt);
                            rtt_sum += rtt;
                            received += 1;
                            got = true;
                            break;
                        }
                    }
                    if !serviced {
                        x86_64::instructions::hlt();
                    } else {
                        x86_64::instructions::nop();
                    }
                }
                if !got {
                    println!("Request timeout for icmp_seq {}", seq);
                }

                // 1s interval between pings
                if seq + 1 < count as u16 {
                    let wait = start + 1000;
                    while crate::timer::uptime_ms() < wait {
                        stack.poll();
                        x86_64::instructions::hlt();
                    }
                }
            }

            stack.sockets.remove(handle);

            let loss = (count - received) * 100 / count;
            println!("\n--- {} ping statistics ---", host);
            println!("{} packets transmitted, {} received, {}% packet loss",
                count, received, loss);
            if received > 0 {
                let avg = rtt_sum / received as u64;
                println!("rtt min/avg/max = {}/{}/{} ms", rtt_min, avg, rtt_max);
            }
        } else {
            println!("ping: network stack not initialized");
        }
    }

    fn cmd_wget(&self, args: &str) {
        // Parse: wget [-O output] <url>
        let mut output_name: Option<&str> = None;
        let mut url_arg = "";
        let parts: alloc::vec::Vec<&str> = args.split_whitespace().collect();
        let mut i = 0;
        while i < parts.len() {
            match parts[i] {
                "-O" => { i += 1; output_name = parts.get(i).copied(); }
                u => url_arg = u,
            }
            i += 1;
        }
        if url_arg.is_empty() {
            println!("Usage: wget [-O filename] <url>");
            println!("  Example: wget http://10.0.2.2/index.html");
            println!("  Note: only http:// and IPv4 hosts supported");
            return;
        }

        let url = url_arg.strip_prefix("http://").unwrap_or(url_arg);
        let (hostport, path) = match url.find('/') {
            Some(i) => (&url[..i], &url[i..]),
            None => (url, "/"),
        };
        let (host, port) = match hostport.find(':') {
            Some(i) => (&hostport[..i], hostport[i+1..].parse::<u16>().unwrap_or(80)),
            None => (hostport, 80u16),
        };

        let ip = match crate::net::dns::resolve(host) {
            Some(ip) => ip,
            None => {
                println!("wget: {}: Name or service not known", host);
                println!("  Hint: use an IPv4 address as hostname");
                return;
            }
        };

        println!("--{}--  http://{}:{}{}", crate::timer::uptime_ms(), host, port, path);
        println!("Connecting to {} ({}):{} ...", host, ip, port);

        // Follow up to 3 redirects
        let mut cur_host = alloc::string::String::from(host);
        let mut cur_path = alloc::string::String::from(path);
        let mut cur_ip = ip;
        let mut cur_port = port;
        let mut final_response = None;

        for redirect in 0..=3usize {
            match crate::net::http::get(&cur_host, &cur_path, cur_ip, cur_port) {
                Ok(resp) => {
                    if (resp.status == 301 || resp.status == 302) && redirect < 3 {
                        if let Some(loc) = &resp.location {
                            println!("Location: {} [following]", loc);
                            let loc_str = loc.strip_prefix("http://").unwrap_or(loc.as_str());
                            let (hp, np) = match loc_str.find('/') {
                                Some(i) => (&loc_str[..i], &loc_str[i..]),
                                None => (loc_str, "/"),
                            };
                            let (nh, np2) = match hp.find(':') {
                                Some(i) => (&hp[..i], hp[i+1..].parse::<u16>().unwrap_or(80)),
                                None => (hp, 80u16),
                            };
                            if let Some(nip) = crate::net::dns::resolve(nh) {
                                cur_host = alloc::string::String::from(nh);
                                cur_path = alloc::string::String::from(np);
                                cur_ip = nip;
                                cur_port = np2;
                                continue;
                            }
                        }
                    }
                    final_response = Some(resp);
                    break;
                }
                Err(e) => { println!("wget: {}", e); return; }
            }
        }

        let response = match final_response {
            Some(r) => r,
            None => { println!("wget: too many redirects"); return; }
        };

        // Print status and key headers
        println!("HTTP/1.1 {}", response.status);
        if let Some(ct) = &response.content_type {
            println!("Content-Type: {}", ct);
        }
        println!("Content-Length: {}", response.body.len());
        println!();

        if response.status >= 400 {
            println!("wget: server returned error {}", response.status);
            if let Ok(text) = core::str::from_utf8(&response.body) {
                let preview = if text.len() > 256 { &text[..256] } else { text };
                println!("{}", preview);
            }
            return;
        }

        // Print body if small
        if response.body.len() <= 4096 {
            if let Ok(text) = core::str::from_utf8(&response.body) {
                println!("{}", text);
            } else {
                println!("(binary data, {} bytes)", response.body.len());
            }
        }

        // Save to VFS
        let default_name = cur_path.rsplit('/').next().unwrap_or("index.html");
        let default_name = if default_name.is_empty() { "index.html" } else { default_name };
        let save_name = output_name.unwrap_or(default_name);
        let filepath = if save_name.starts_with('/') {
            alloc::string::String::from(save_name)
        } else {
            alloc::format!("/tmp/{}", save_name)
        };
        use crate::fs::ramfs::RamFile;
        use alloc::sync::Arc;
        let file = Arc::new(spin::Mutex::new(RamFile::from_bytes(&response.body)));
        VFS.lock().mount(&filepath, file);
        println!("Saved to '{}' [{} bytes]", filepath, response.body.len());
    }

    fn cmd_ifconfig(&self) {
        let stack_guard = crate::net::stack::TCPIP.lock();
        if let Some(stack) = stack_guard.as_ref() {
            let mac = stack.mac();
            println!("eth0: flags=UP,BROADCAST,RUNNING,MULTICAST  mtu 1500");
            for cidr in stack.iface.ip_addrs() {
                println!("        inet {}  netmask {}  broadcast 10.0.2.255", 
                    cidr.address(), cidr.prefix_len());
            }
            println!("        ether {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}  txqueuelen 1000  (Ethernet)",
                mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]);
            
            let stats = crate::net::NET.lock();
            if let Some(dev) = stats.devices.iter().filter_map(|d| d.as_ref()).find(|d| d.name == "eth0") {
                println!("        RX packets {}  bytes {} ", dev.rx_packets, dev.rx_bytes);
                println!("        TX packets {}  bytes {} ", dev.tx_packets, dev.tx_bytes);
            }
        } else {
            println!("ifconfig: TCP/IP stack not initialized");
        }
        println!();
        println!("lo: flags=UP,LOOPBACK,RUNNING  mtu 65536");
        println!("        inet 127.0.0.1  netmask 8");
        println!("        loop  txqueuelen 1000  (Local Loopback)");
    }

    fn cmd_mv(&self, src: Option<&str>, dst: Option<&str>) {
        let (src, dst) = match (src, dst) {
            (Some(s), Some(d)) => (s, d),
            _ => { println!("Usage: mv <src> <dst>"); return; }
        };
        let src_path = self.resolve_path(src);
        let dst_path = self.resolve_path(dst);
        match VFS.lock().rename(&src_path, &dst_path) {
            Ok(_) => println!("mv: {} -> {}", src_path, dst_path),
            Err(_) => println!("mv: {}: No such file", src_path),
        }
    }

    fn cmd_cp(&self, src: Option<&str>, dst: Option<&str>) {
        use crate::fs::ziqafs::{ZIQAFS, ZiqaFs, ROOT_INODE};
        let (src, dst) = match (src, dst) {
            (Some(s), Some(d)) => (s, d),
            _ => { println!("Usage: cp <src> <dst>"); return; }
        };
        let src_path = self.resolve_path(src);
        let dst_path = self.resolve_path(dst);
        let guard = ZIQAFS.lock();
        if let Some(fs_arc) = guard.as_ref() {
            let mut fs = fs_arc.lock();
            let src_id = match ZiqaFs::root_lookup(&mut fs, &src_path) {
                Ok(id) => id,
                Err(_) => { println!("cp: {}: No such file", src_path); return; }
            };
            let dst_name = dst_path.rsplit('/').next().unwrap_or(&dst_path);
            match ZiqaFs::copy_file(&mut fs, src_id, ROOT_INODE, dst_name) {
                Ok(_) => println!("cp: {} -> {}", src_path, dst_path),
                Err(e) => println!("cp: {:?}", e),
            }
        } else {
            println!("cp: ZiqaFS not mounted");
        }
    }

    fn cmd_touch(&self, path: Option<&str>) {
        let p = match path {
            Some(s) => s,
            None => { println!("Usage: touch <path>"); return; }
        };
        let resolved = self.resolve_path(p);
        let mut vfs = VFS.lock();
        if !vfs.exists(&resolved) {
            vfs.create(&resolved);
            println!("touch: created {}", resolved);
        } else {
            // File exists — just update mtime via a zero-byte write
            let _ = vfs.write_raw(&resolved, &[], 0);
            println!("touch: updated {}", resolved);
        }
    }

    fn cmd_stat(&self, path: Option<&str>) {
        use crate::fs::ziqafs::{ZIQAFS, ZiqaFs};
        let p = match path {
            Some(s) => s,
            None => { println!("Usage: stat <path>"); return; }
        };
        let resolved = self.resolve_path(p);
        let guard = ZIQAFS.lock();
        if let Some(fs_arc) = guard.as_ref() {
            let mut fs = fs_arc.lock();
            match ZiqaFs::root_lookup(&mut fs, &resolved) {
                Ok(inode_id) => {
                    if let Ok(inode) = ZiqaFs::get_inode(&mut fs, inode_id) {
                        let kind = if inode.mode == 0o040000 { "directory" } else { "regular file" };
                        println!("  File:   {}", resolved);
                        println!("  Inode:  {}", inode_id);
                        println!("  Type:   {}", kind);
                        println!("  Size:   {} bytes", unsafe { core::ptr::read_unaligned(core::ptr::addr_of!(inode.size)) });
                        println!("  Links:  {}", unsafe { core::ptr::read_unaligned(core::ptr::addr_of!(inode.nlink)) });
                        println!("  mtime:  {}s", unsafe { core::ptr::read_unaligned(core::ptr::addr_of!(inode.mtime)) });
                        println!("  ctime:  {}s", unsafe { core::ptr::read_unaligned(core::ptr::addr_of!(inode.ctime)) });
                        println!("  atime:  {}s", unsafe { core::ptr::read_unaligned(core::ptr::addr_of!(inode.atime)) });
                    }
                }
                Err(_) => println!("stat: {}: No such file", resolved),
            }
        } else {
            // Fallback to VFS size
            let vfs = VFS.lock();
            if let Some(size) = vfs.file_size(&resolved) {
                println!("  File:  {}", resolved);
                println!("  Size:  {} bytes", size);
            } else {
                println!("stat: {}: No such file", resolved);
            }
        }
    }

    fn cmd_du(&self, path: Option<&str>) {
        // Try ZiqaFS first
        use crate::fs::ziqafs::{ZIQAFS, ZiqaFs, BLOCK_SIZE};
        let p = path.map(|s| self.resolve_path(s)).unwrap_or_else(|| self.cwd_str().to_string());
        let guard = ZIQAFS.lock();
        if let Some(fs_arc) = guard.as_ref() {
            let mut fs = fs_arc.lock();
            match ZiqaFs::root_lookup(&mut fs, &p) {
                Ok(inode_id) => {
                    let blocks = ZiqaFs::du(&mut fs, inode_id);
                    println!("{}\t{} ({} KiB)", blocks, p, blocks as usize * BLOCK_SIZE / 1024);
                    return;
                }
                Err(_) => {}
            }
        }
        
        // Fallback to VFS for basic file size
        let vfs = VFS.lock();
        if vfs.is_dir(&p) {
            // For directories, we can't easily get size without ZiqaFS
            let entries = vfs.list_dir(&p);
            let mut total_size: u64 = 0;
            for entry in &entries {
                if let Some(size) = vfs.file_size(entry) {
                    total_size += size as u64;
                }
            }
            let kib = total_size / 1024;
            println!("{}\t{} ({} KiB)", kib, p, kib);
        } else if let Some(size) = vfs.file_size(&p) {
            let kib = size as u64 / 1024;
            println!("{}\t{} ({} KiB)", kib, p, kib);
        } else {
            println!("du: {}: No such file", p);
        }
    }

    fn cmd_jobs(&self) {
        if self.jobs.is_empty() {
            println!("No background jobs.");
            return;
        }
        println!("{}{}  JOBS {} {}", C_YELLOW, C_BOLD, C_RESET, C_DIM);
        for (i, job) in self.jobs.iter().enumerate() {
            let state_str = match job.state {
                JobState::Running => "\x1b[32mRunning\x1b[0m",
                JobState::Stopped => "\x1b[33mStopped\x1b[0m",
                JobState::Done => "\x1b[31mDone\x1b[0m",
            };
            println!("  [{}] {} {}", i + 1, state_str, job.command);
        }
    }

    fn cmd_bg(&mut self, arg: Option<&str>) {
        let job_num = match arg.and_then(|s| s.parse::<usize>().ok()) {
            Some(n) if n > 0 && n <= self.jobs.len() => n - 1,
            _ => {
                println!("Usage: bg <%job-number>");
                return;
            }
        };

        let is_stopped = self.jobs[job_num].state == JobState::Stopped;
        if is_stopped {
            let pid = self.jobs[job_num].pid;
            let cmd = self.jobs[job_num].command.clone();
            crate::process::scheduler::SCHEDULER.lock()
                .send_signal(pid, crate::process::signal::sig::SIGCONT);
            self.jobs[job_num].state = JobState::Running;
            println!("{} [{}] &", cmd, job_num + 1);
        } else {
            println!("bg: job {} is not stopped", job_num + 1);
        }
    }

    fn cmd_fg(&mut self, arg: Option<&str>) {
        let job_num = match arg.and_then(|s| s.parse::<usize>().ok()) {
            Some(n) if n > 0 && n <= self.jobs.len() => n - 1,
            _ => {
                println!("Usage: fg <%job-number>");
                return;
            }
        };

        let job_state = self.jobs[job_num].state;
        if job_state == JobState::Stopped || job_state == JobState::Running {
            self.fg_job = Some(job_num);
            if job_state == JobState::Stopped {
                let pid = self.jobs[job_num].pid;
                crate::process::scheduler::SCHEDULER.lock()
                    .send_signal(pid, crate::process::signal::sig::SIGCONT);
            }
            self.jobs[job_num].state = JobState::Running;
            self.fg_job = None;
        } else {
            println!("fg: job {} is not in a valid state", job_num + 1);
        }
    }

fn levenshtein_distance(a: &str, b: &str) -> usize {
    let a = a.as_bytes();
    let b = b.as_bytes();
    let alen = a.len();
    let blen = b.len();
    if alen == 0 { return blen; }
    if blen == 0 { return alen; }
    let mut prev: Vec<usize> = (0..=blen).collect();
    let mut curr: Vec<usize> = (0..=blen).map(|_| 0).collect();
    for i in 1..=alen {
        curr[0] = i;
        for j in 1..=blen {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            curr[j] = (prev[j] + 1)
                .min(curr[j - 1] + 1)
                .min(prev[j - 1] + cost);
        }
        core::mem::swap(&mut prev, &mut curr);
    }
    prev[blen]
}

fn longest_common_prefix(strings: &[String]) -> String {
    if strings.is_empty() {
        return String::new();
    }
    if strings.len() == 1 {
        return strings[0].clone();
    }
    let first = strings[0].as_bytes();
    let mut len = first.len();
    for s in &strings[1..] {
        let bytes = s.as_bytes();
        let mut i = 0;
        while i < len && i < bytes.len() && first[i] == bytes[i] {
            i += 1;
        }
        len = i;
        if len == 0 {
            break;
        }
    }
    strings[0][..len].to_string()
}

}

pub fn start() -> ! {
    let mut shell = Shell::new();
    shell.run();
}

pub fn run() -> ! {
    start()
}
