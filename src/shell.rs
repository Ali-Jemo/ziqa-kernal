/// Interactive shell for ZiqaKernel

use alloc::vec::Vec;
use crate::{print, println};
use crate::klog::Level;
use crate::process::AbiKind;
use crate::fs::vfs::VFS;
use x86_64::VirtAddr;

const COMMANDS: &[&str] = &[
    "help", "uptime", "ps", "spawn", "spawnelf", "exec", "kill",
    "sleep", "meminfo", "netstat", "klog", "doom", "tetris",
    "reboot", "echo", "clear",
];

const MAX_HISTORY: usize = 50;

pub struct Shell {
    prompt: &'static str,
    input_buf: [u8; 256],
    cursor: usize,
    history: Vec<[u8; 256]>,
    history_pos: isize,
}

impl Shell {
    pub const fn new() -> Self {
        Self {
            prompt: "> ",
            input_buf: [0; 256],
            cursor: 0,
            history: Vec::new(),
            history_pos: -1,
        }
    }

    pub fn run(&mut self) -> ! {
        println!("[ZIQA] Starting interactive shell...");
        loop {
            print!("{}", self.prompt);
            self.read_line();

            let has_input = self.input_buf[..self.cursor].iter().any(|&b| b != b' ' && b != b'\t' && b != b'\r' && b != b'\n');
            if has_input {
                self.push_history();
                let input = core::str::from_utf8(&self.input_buf[..self.cursor]).unwrap_or("");
                let trimmed = input.trim();
                let parts: Vec<&str> = trimmed.splitn(3, ' ').collect();
                match parts[0] {
                    "help"    => self.cmd_help(),
                    "uptime"  => self.cmd_uptime(),
                    "klog"    => self.cmd_klog(parts.get(1).copied().unwrap_or("info")),
                    "spawn"   => self.cmd_spawn(parts.get(1).copied()),
                    "spawnelf" => self.cmd_spawn_elf(parts.get(1).copied()),
                    "exec"    => self.cmd_exec(parts.get(1).copied()),
                    "ps"      => self.cmd_ps(),
                    "kill"    => self.cmd_kill(parts.get(1).copied(), parts.get(2).copied()),
                    "sleep"   => self.cmd_sleep(parts.get(1).copied()),
                    "meminfo" => self.cmd_meminfo(),
                    "netstat" => self.cmd_netstat(),
                    "doom"    => self.cmd_doom(parts.get(1).copied()),
                    "tetris"  => self.cmd_tetris(),
                    "reboot"  => self.cmd_reboot(),
                    "edit"    => self.cmd_edit(parts.get(1).copied()),
                    "ls"      => self.cmd_ls(),
                    "clear"   => { for _ in 0..25 { println!(""); } },
                    "echo"    => println!("{}", parts.get(1).copied().unwrap_or("")),
                    _         => println!("Unknown command: {}. Type 'help'.", parts[0]),
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
        println!("Commands:");
        println!("  help              - this message");
        println!("  uptime            - kernel uptime in ms");
        println!("  ps                - list processes");
        println!("  spawn [path]      - spawn process (skeleton, or from VFS path)");
        println!("  spawnelf <path>   - spawn process from VFS ELF binary");
        println!("  exec <pid>        - execute process entry point (runs in kernel)");
        println!("  kill <pid> [sig]  - send signal to process (default: SIGTERM=15)");
        println!("  sleep <ms>        - sleep current shell process N milliseconds");
        println!("  meminfo           - heap memory statistics");
        println!("  netstat           - network device statistics");
        println!("  klog [level]      - dump kernel log (debug/info/error)");
        println!("  reboot            - reboot the system");
        println!("  doom [steps]      - run DOOM fire demo (default: 60 steps)");
        println!("  tetris            - run graphical Tetris game on VGA console");
        println!("  echo <text>       - print text");
        println!("  clear             - clear screen");
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
        let mut buf = [0u8; 65536];
        match crate::fs::vfs::VFS.lock().read_raw(p, &mut buf, 0) {
            Ok(n) if n > 0 => {
                let data = &buf[..n];
                match crate::process::scheduler::spawn_elf(data) {
                    Some(pid) => println!("Spawned PID={} from '{}'", pid.0, p),
                    None => println!("spawnelf: failed to spawn from '{}'", p),
                }
            }
            _ => println!("spawnelf: file '{}' not found in VFS", p),
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

    fn cmd_tetris(&self) {
        crate::tetris::run();
    }
}

pub fn start() -> ! {
    let mut shell = Shell::new();
    shell.run();
}

pub fn run() -> ! {
    start()
}
