/// Interactive shell for ZiqaKernel

use alloc::vec::Vec;
use alloc::string::String;
use alloc::string::ToString;
use crate::{print, println};
use crate::klog::Level;
use crate::process::AbiKind;
use crate::fs::vfs::VFS;
use crate::fs::ziqafs::{ZIQAFS, ZiqaFs, ROOT_INODE};
use x86_64::VirtAddr;
use embedded_cli::cli::{Cli, CliBuilder, Command};
use embedded_cli::command;

const COMMANDS: &[&str] = &[
    "help", "uptime", "ps", "spawn", "spawnelf", "exec", "kill",
    "sleep", "meminfo", "diskinfo", "netstat", "klog", "doom", "tetris",
    "reboot", "echo", "clear", "edit", "ls", "cd", "pwd", "mkdir",
    "dir", "rm", "rmdir", "cat", "ping", "wget", "ifconfig",
    "mv", "cp", "touch", "stat", "du",
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
    cli: Cli<'static, 256, 16>,
    prompt: &'static str,
    cwd: [u8; 256],
    cwd_len: usize,
    prev_cwd: [u8; 256],
    prev_cwd_len: usize,
}

impl Shell {
    pub fn new() -> Self {
        let mut cli = CliBuilder::default()
            .prompt("> ")
            .build()
            .unwrap();

        // FS Commands
        cli = Self::register_fs_commands(cli);
        
        Self {
            cli,
            prompt: "> ",
            cwd: [0; 256],
            cwd_len: 0,
            prev_cwd: [0; 256],
            prev_cwd_len: 0,
        }
    }

    fn register_fs_commands(mut cli: Cli<'static, 256, 16>) -> Cli<'static, 256, 16> {
        cli.add_command(Command::new("ls", "List files", |args| {
            let path = args.get(0).map(|s| s.to_string());
            crate::shell::SHELL.lock().cmd_ls(path.as_deref());
            Ok(())
        })).unwrap();
        // ... (more commands)
        cli
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
                    "diskinfo" => self.cmd_diskinfo(),
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
                    "rmdir"   => self.cmd_rm(arg1.as_deref()),
                    "cat"     => self.cmd_cat(arg1.as_deref()),
                    "mv"      => self.cmd_mv(arg1.as_deref(), arg2.as_deref()),
                    "cp"      => self.cmd_cp(arg1.as_deref(), arg2.as_deref()),
                    "touch"   => self.cmd_touch(arg1.as_deref()),
                    "stat"    => self.cmd_stat(arg1.as_deref()),
                    "du"      => self.cmd_du(arg1.as_deref()),
                    "clear"   => self.cmd_clear(),
                    "echo"    => println!("{}", arg1.as_deref().unwrap_or("")),
                    "ping"     => self.cmd_ping(arg1.as_deref()),
                    "wget"     => self.cmd_wget(arg1.as_deref()),
                    "ifconfig" => self.cmd_ifconfig(),
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
            let common = longest_common_prefix(&candidates);
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
                            print!("\x08 \x08");
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
                ("mv <src> <dst>", "move/rename a file"),
                ("cp <src> <dst>", "copy a file"),
                ("touch <path>", "create file or update mtime"),
                ("stat <path>", "show inode details"),
                ("du [path]", "disk usage (blocks)"),
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
                ("diskinfo", "ZiqaFS disk usage + fsck"),
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

    fn cmd_diskinfo(&self) {
        use crate::fs::ziqafs::ZIQAFS;
        let guard = ZIQAFS.lock();
        if let Some(fs_arc) = guard.as_ref() {
            let mut fs = fs_arc.lock();
            let st = crate::fs::ziqafs::ZiqaFs::statfs(&fs);
            println!("ZiqaFS Disk Info:");
            println!("  Block size:    {} bytes", st.block_size);
            println!("  Total blocks:  {}", st.total_blocks);
            println!("  Free blocks:   {} ({} KiB)", st.free_blocks, st.free_blocks as u64 * st.block_size as u64 / 1024);
            println!("  Total inodes:  {}", st.total_inodes);
            println!("  Free inodes:   {}", st.free_inodes);
            let r = crate::fs::ziqafs::ZiqaFs::fsck(&mut fs);
            if r.ok {
                println!("  fsck:          OK");
            } else {
                println!("  fsck:          ERRORS (errs={} leaked_blocks={} leaked_inodes={})",
                    r.errors, r.leaked_blocks, r.leaked_inodes);
            }
        } else {
            println!("diskinfo: ZiqaFS not mounted");
        }
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

    fn cmd_ping(&self, target: Option<&str>) {
        let target = match target {
            Some(t) => t,
            None => {
                println!("Usage: ping <host|ip>");
                return;
            }
        };
        
        let ip = match crate::net::dns::resolve(target) {
            Some(ip) => ip,
            None => {
                println!("ping: cannot resolve '{}'", target);
                return;
            }
        };
        
        println!("PING {} ({})", target, ip);
        
        let mut stack_guard = crate::net::stack::TCPIP.lock();
        if let Some(stack) = stack_guard.as_mut() {
            use smoltcp::socket::icmp;
            use smoltcp::wire::{IpAddress, Icmpv4Repr, Icmpv4Packet};
            
            let icmp_rx_buffer = icmp::PacketBuffer::new(
                alloc::vec![icmp::PacketMetadata::EMPTY; 4],
                alloc::vec![0; 1024],
            );
            let icmp_tx_buffer = icmp::PacketBuffer::new(
                alloc::vec![icmp::PacketMetadata::EMPTY; 4],
                alloc::vec![0; 1024],
            );
            let icmp_socket = icmp::Socket::new(icmp_rx_buffer, icmp_tx_buffer);
            let handle = stack.sockets.add(icmp_socket);
            
            let ident = 0x1234;
            {
                let socket = stack.sockets.get_mut::<icmp::Socket>(handle);
                socket.bind(icmp::Endpoint::Ident(ident)).ok();
            }
            
            for seq in 0..3u16 {
                let start = crate::timer::uptime_ms();
                
                // Send echo request
                {
                    let socket = stack.sockets.get_mut::<icmp::Socket>(handle);
                    let repr = Icmpv4Repr::EchoRequest {
                        ident,
                        seq_no: seq,
                        data: b"ziqa",
                    };
                    let payload = socket.send(repr.buffer_len(), IpAddress::Ipv4(ip)).ok();
                    if let Some(payload) = payload {
                        let mut packet = Icmpv4Packet::new_unchecked(payload);
                        repr.emit(&mut packet, &smoltcp::phy::ChecksumCapabilities::default());
                    }
                }
                
                // Poll for reply
                let mut got_reply = false;
                for _ in 0..500 {
                    stack.poll();
                    let socket = stack.sockets.get_mut::<icmp::Socket>(handle);
                    if socket.can_recv() {
                        if let Ok((data, _addr)) = socket.recv() {
                            let elapsed = crate::timer::uptime_ms() - start;
                            println!("  {} bytes from {}: seq={} time={}ms", data.len(), ip, seq, elapsed);
                            got_reply = true;
                            break;
                        }
                    }
                    // yield control slightly
                    x86_64::instructions::nop();
                }
                if !got_reply {
                    println!("  Request timeout for seq {}", seq);
                }
            }
            
            stack.sockets.remove(handle);
        } else {
            println!("ping: TCP/IP stack not initialized");
        }
    }

    fn cmd_wget(&self, url: Option<&str>) {
        let url = match url {
            Some(u) => u,
            None => {
                println!("Usage: wget <url>");
                println!("  Example: wget http://example.com/");
                return;
            }
        };
        
        let url = url.strip_prefix("http://").unwrap_or(url);
        let (host, path) = match url.find('/') {
            Some(i) => (&url[..i], &url[i..]),
            None => (url, "/"),
        };
        
        let (host, port) = match host.find(':') {
            Some(i) => (&host[..i], host[i+1..].parse::<u16>().unwrap_or(80)),
            None => (host, 80u16),
        };
        
        let ip = match crate::net::dns::resolve(host) {
            Some(ip) => ip,
            None => {
                println!("wget: cannot resolve '{}'", host);
                return;
            }
        };
        
        println!("Connecting to {} ({}):{} ...", host, ip, port);
        
        match crate::net::http::get(host, path, ip, port) {
            Ok(response) => {
                println!("HTTP {} - {} bytes received", response.status, response.body.len());
                if response.body.len() < 2048 {
                    if let Ok(text) = core::str::from_utf8(&response.body) {
                        println!("{}", text);
                    } else {
                        println!("(binary data, {} bytes)", response.body.len());
                    }
                } else {
                    let filename = path.rsplit('/').next().unwrap_or("download");
                    let filepath = alloc::format!("/tmp/{}", filename);
                    
                    use crate::fs::ramfs::RamFile;
                    use alloc::sync::Arc;
                    
                    let file = Arc::new(spin::Mutex::new(RamFile::from_bytes(&response.body)));
                    VFS.lock().mount(&filepath, file);
                    println!("Saved to {}", filepath);
                }
            }
            Err(e) => println!("wget: {}", e),
        }
    }

    fn cmd_ifconfig(&self) {
        let stack_guard = crate::net::stack::TCPIP.lock();
        if let Some(stack) = stack_guard.as_ref() {
            let mac = stack.mac();
            println!("eth0:");
            println!("  inet 10.0.2.15/24");
            println!("  gateway 10.0.2.2");
            println!("  ether {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
                mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]);
            println!("  mtu 1500");
        } else {
            println!("ifconfig: TCP/IP stack not initialized");
        }
        println!();
        println!("lo:");
        println!("  inet 127.0.0.1/8");
        println!("  loopback");
        
        println!();
        println!("Statistics:");
        crate::net::NET.lock().print_stats();
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
        use crate::fs::ziqafs::{ZIQAFS, ZiqaFs, BLOCK_SIZE};
        let p = path.map(|s| self.resolve_path(s)).unwrap_or_else(|| self.cwd_str().to_string());
        let guard = ZIQAFS.lock();
        if let Some(fs_arc) = guard.as_ref() {
            let mut fs = fs_arc.lock();
            match ZiqaFs::root_lookup(&mut fs, &p) {
                Ok(inode_id) => {
                    let blocks = ZiqaFs::du(&mut fs, inode_id);
                    println!("{}\t{} ({} KiB)", blocks, p, blocks as usize * BLOCK_SIZE / 1024);
                }
                Err(_) => println!("du: {}: No such file", p),
            }
        } else {
            println!("du: ZiqaFS not mounted");
        }
    }
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

pub fn start() -> ! {
    let mut shell = Shell::new();
    shell.run();
}

pub fn run() -> ! {
    start()
}
