/// Interactive shell for ZiqaKernel

use alloc::vec::Vec;
use alloc::string::String;
use alloc::string::ToString;
use crate::{print, println};
use crate::klog::Level;
use crate::process::AbiKind;
use crate::fs::vfs::VFS;
use x86_64::VirtAddr;

const COMMANDS: &[&str] = &[
    "help", "uptime", "ps", "spawn", "spawnelf", "exec", "kill",
    "sleep", "meminfo", "netstat", "klog", "doom", "tetris",
    "reboot", "echo", "clear", "edit", "ls", "cd", "pwd", "mkdir",
    "dir", "rm", "cat",
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
    prompt: &'static str,
    input_buf: [u8; 256],
    cursor: usize,
    history: Vec<[u8; 256]>,
    history_pos: isize,
    cwd: [u8; 256],
    cwd_len: usize,
    prev_cwd: [u8; 256],
    prev_cwd_len: usize,
}

impl Shell {
    pub const fn new() -> Self {
        Self {
            prompt: "> ",
            input_buf: [0; 256],
            cursor: 0,
            history: Vec::new(),
            history_pos: -1,
            cwd: [0; 256],
            cwd_len: 0,
            prev_cwd: [0; 256],
            prev_cwd_len: 0,
        }
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
        println!("[ZIQA] Starting interactive shell...");
        loop {
            let cwd = self.cwd_str();
            if cwd == "/" {
                print!("{}", self.prompt);
            } else {
                print!("{} {}", cwd, self.prompt);
            }
            self.read_line();

            let has_input = self.input_buf[..self.cursor].iter().any(|&b| b != b' ' && b != b'\t' && b != b'\r' && b != b'\n');
            if has_input {
                self.push_history();
                let input = core::str::from_utf8(&self.input_buf[..self.cursor]).unwrap_or("");
                let trimmed = input.trim();
                let parts: Vec<&str> = trimmed.splitn(3, ' ').collect();
                let cmd = parts[0].to_string();
                let arg1 = parts.get(1).copied().map(String::from);
                let arg2 = parts.get(2).copied().map(String::from);
                match cmd.as_str() {
                    "help"    => self.cmd_help(),
                    "uptime"  => self.cmd_uptime(),
                    "klog"    => self.cmd_klog(arg1.as_deref().unwrap_or("info")),
                    "spawn"   => self.cmd_spawn(arg1.as_deref()),
                    "spawnelf" => self.cmd_spawn_elf(arg1.as_deref()),
                    "exec"    => self.cmd_exec(arg1.as_deref()),
                    "ps"      => self.cmd_ps(),
                    "kill"    => self.cmd_kill(arg1.as_deref(), arg2.as_deref()),
                    "sleep"   => self.cmd_sleep(arg1.as_deref()),
                    "meminfo" => self.cmd_meminfo(),
                    "netstat" => self.cmd_netstat(),
                    "doom"    => self.cmd_doom(arg1.as_deref()),
                    "tetris"  => self.cmd_tetris(),
                    "reboot"  => self.cmd_reboot(),
                    "edit"    => self.cmd_edit(arg1.as_deref()),
                    "ls"      => self.cmd_ls(arg1.as_deref()),
                    "cd"      => self.cmd_cd(arg1.as_deref()),
                    "pwd"     => self.cmd_pwd(),
                    "mkdir"   => self.cmd_mkdir(arg1.as_deref()),
                    "dir"     => self.cmd_dir(arg1.as_deref()),
                    "rm"      => self.cmd_rm(arg1.as_deref()),
                    "cat"     => self.cmd_cat(arg1.as_deref()),
                    "clear"   => self.cmd_clear(),
                    "echo"    => println!("{}", arg1.as_deref().unwrap_or("")),
                    _         => println!("Unknown command: {}. Type 'help'.", cmd),
                }
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

    fn refresh_line(&self, idx: usize) {
        print!("\r");
        for _ in 0..79 {
            print!(" ");
        }
        print!("\r");
        print!("{}", self.prompt);
        if let Ok(s) = core::str::from_utf8(&self.input_buf[..idx]) {
            print!("{}", s);
        }
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

        let matches: Vec<&&str> = COMMANDS.iter().filter(|c| c.starts_with(prefix)).collect();

        match matches.len() {
            0 => {}
            1 => {
                let cmd = matches[0].as_bytes();
                let new_end = last_space + cmd.len();
                self.input_buf[last_space..new_end].copy_from_slice(cmd);
                for i in new_end..*idx {
                    self.input_buf[i] = 0;
                }
                *idx = new_end;
                self.refresh_line(*idx);
            }
            _ => {
                println!("");
                for m in matches {
                    print!("{}  ", m);
                }
                println!("");
                self.refresh_line(*idx);
            }
        }
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
                    0x09 => {
                        self.autocomplete(&mut idx);
                    }
                    b'\n' | b'\r' => {
                        self.cursor = idx;
                        println!("");
                        break;
                    }
                    8 | 127 => {
                        if idx > 0 {
                            idx -= 1;
                        }
                    }
                    _ => {
                        if idx < self.input_buf.len() - 1 {
                            self.input_buf[idx] = b;
                            idx += 1;
                        }
                    }
                }
            } else {
                x86_64::instructions::hlt();
            }
        }
    }

    fn cmd_help(&self) {
        println!("{}{}  ⚡ ZiqaKernel Shell ⚡{}", C_CYAN, C_BOLD, C_RESET);
        println!("{}  ─────────────────────────────────────{}", C_DIM, C_RESET);
        println!("");

        let groups: &[(&str, &[(&str, &str)])] = &[
            ("Filesystem", &[
                ("ls [path]", "list files in current directory"),
                ("cd [path]", "change directory (.. / - for previous)"),
                ("pwd", "print working directory"),
                ("mkdir <path>", "create a directory"),
                ("dir [path]", "detailed directory listing"),
                ("rm <path>", "remove a file"),
                ("cat <path>", "display file contents"),
                ("edit <path>", "nano-like text editor"),
            ]),
            ("Process", &[
                ("ps", "list processes"),
                ("spawn [path]", "spawn skeleton or ELF process"),
                ("spawnelf <path>", "spawn ELF from VFS"),
                ("exec <pid>", "execute process entry point"),
                ("kill <pid> [sig]", "send signal to process"),
                ("sleep <ms>", "sleep N milliseconds"),
            ]),
            ("System", &[
                ("help", "show this message"),
                ("uptime", "kernel uptime"),
                ("meminfo", "heap memory statistics"),
                ("netstat", "network device statistics"),
                ("klog [level]", "dump kernel log (debug/info/error)"),
                ("reboot", "reboot the system"),
                ("clear", "clear screen"),
                ("echo <text>", "print text"),
            ]),
            ("Entertainment", &[
                ("doom [steps]", "DOOM fire demo"),
                ("tetris", "graphical Tetris on VGA console"),
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

    fn cmd_uptime(&self) {
        let ms = crate::timer::uptime_ms();
        let secs = ms / 1000;
        println!("Uptime: {}ms ({}s {} ticks)", ms, secs, crate::timer::uptime_ticks());
    }

    fn cmd_klog(&self, level_str: &str) {
        let level = match level_str {
            "debug" => Level::Debug,
            "error" => Level::Error,
            _ => Level::Info,
        };
        crate::klog::KLOG.lock().dump_level(level);
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

        let entry_vaddr = {
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

        unsafe {
            let func: extern "C" fn() = core::mem::transmute(entry_vaddr);
            func();
        }
    }

    fn cmd_ps(&self) {
        crate::process::scheduler::SCHEDULER.lock().print_process_list();
    }

    fn cmd_kill(&self, pid_str: Option<&str>, sig_str: Option<&str>) {
        let pid_val = match pid_str.and_then(|s| s.parse::<u64>().ok()) {
            Some(v) => v,
            None => { println!("Usage: kill <pid> [signal]"); return; }
        };
        let signum: u8 = sig_str.and_then(|s| s.parse().ok()).unwrap_or(15);
        let ok = crate::process::scheduler::SCHEDULER.lock()
            .send_signal(crate::process::Pid(pid_val), signum);
        if ok {
            println!("Sent signal {} to PID {}", signum, pid_val);
        } else {
            println!("kill: no process with PID {}", pid_val);
        }
    }

    fn cmd_sleep(&self, ms_str: Option<&str>) {
        let ms = match ms_str.and_then(|s| s.parse::<u64>().ok()) {
            Some(v) => v,
            None => { println!("Usage: sleep <milliseconds>"); return; }
        };
        let shell_pid = crate::process::Pid(0);
        crate::timer::sleep_ms(shell_pid, ms);
        println!("Slept {}ms", ms);
    }

    fn cmd_meminfo(&self) {
        let stats = crate::memory::heapstats::get_stats();
        println!("Heap Memory Info:");
        println!("  Heap start:       0x{:x}", crate::memory::heap::HEAP_START);
        println!("  Heap size:        {} KiB", crate::memory::heap::HEAP_SIZE / 1024);
        println!("  Allocations:      {}", stats.total_allocations);
        println!("  Frees:            {}", stats.total_frees);
        println!("  Current blocks:   {}", stats.current_blocks);
        println!("  Current usage:    {} bytes", stats.current_usage_bytes());
        println!("  Peak usage:       {} bytes", stats.peak_usage_bytes);
    }

    fn cmd_netstat(&self) {
        println!("Network Devices:");
        crate::net::NET.lock().print_stats();
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

    fn cmd_doom(&self, steps_str: Option<&str>) {
        let steps: usize = steps_str.and_then(|s| s.parse().ok()).unwrap_or(60);
        crate::doom::run(steps);
    }

    fn cmd_clear(&self) {
        crate::drivers::vga::clear_screen();
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

        println!("  {}D  {} .{}", C_BLUE, C_DIM, C_RESET);
        println!("  {}D  {} ..{}", C_BLUE, C_DIM, C_RESET);

        if entries.is_empty() {
            println!("  {}(empty){}", C_DIM, C_RESET);
        } else {
            for e in &entries {
                let name = e.rsplit('/').next().unwrap_or(e);
                if vfs.is_dir(e) {
                    println!("  {}D  {}{} {}", C_BLUE, C_DIM, name, C_RESET);
                } else if let Some(size) = vfs.file_size(e) {
                    println!("  {:>8}  {}{}", size, name, C_RESET);
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
        for e in &entries {
            if vfs.is_dir(e) {
                println!("  <DIR>          {}", e);
            } else if let Some(size) = vfs.file_size(e) {
                println!("  {:>8}  {}", size, e);
            } else {
                println!("                 {}", e);
            }
        }
    }

    fn cmd_rm(&self, target: Option<&str>) {
        let p = match target {
            Some(s) => s,
            None => { println!("Usage: rm <path>"); return; }
        };
        let resolved = self.resolve_path(p);
        match VFS.lock().remove(&resolved) {
            Ok(_) => println!("rm: removed {}", resolved),
            Err(_) => println!("rm: {}: No such file", resolved),
        }
    }

    fn cmd_cat(&self, target: Option<&str>) {
        let p = match target {
            Some(s) => s,
            None => { println!("Usage: cat <path>"); return; }
        };
        let resolved = self.resolve_path(p);
        let mut buf = [0u8; 4096];
        match VFS.lock().read_raw(&resolved, &mut buf, 0) {
            Ok(0) => {}
            Ok(n) => {
                if let Ok(s) = core::str::from_utf8(&buf[..n]) {
                    print!("{}", s);
                } else {
                    println!("(binary data, {} bytes)", n);
                }
            }
            Err(_) => println!("cat: {}: No such file", resolved),
        }
    }
}

pub fn start() -> ! {
    let mut shell = Shell::new();
    shell.run();
}

pub fn run() -> ! {
    start()
}
