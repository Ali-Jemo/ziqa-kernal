/// Interactive shell for ZiqaKernel
/// Simple command-line interface for kernel debugging and interaction

use crate::println;
use crate::klog::Level;
use crate::process::{AbiKind, Pid};
use crate::memory::VirtAddress;

pub struct Shell {
    prompt: &'static str,
    input_buf: [u8; 256],
    cursor: usize,
}

impl Shell {
    pub const fn new() -> Self {
        Self {
            prompt: "> ",
            input_buf: [0; 256],
            cursor: 0,
        }
    }

    pub fn run(&mut self) {
        println!("[ZIQA] Starting interactive shell...");
        println!("[ZIQA] Available commands: help, uptime, klog, spawn, echo");

        loop {
            self.print_prompt();
            self.read_line();

            if let Some(cmd) = self.parse_command() {
                match cmd {
                    ShellCmd::Help => self.cmd_help(),
                    ShellCmd::Uptime => self.cmd_uptime(),
                    ShellCmd::Klog(level) => self.cmd_klog(level),
                    ShellCmd::Spawn => self.cmd_spawn(),
                    ShellCmd::Echo(msg) => self.cmd_echo(msg),
                    ShellCmd::Unknown => self.cmd_unknown(),
                }
            }

            self.cursor = 0;
        }
    }

    fn print_prompt(&self) {
        print!("{}", self.prompt);
    }

    fn read_line(&mut self) {
        // Uses keyboard driver ring buffer
        use crate::drivers::keyboard::read_stdin;
        let n = read_stdin(&mut self.input_buf);
        self.cursor = n;
    }

    fn parse_command(&self) -> Option<ShellCmd<'static>> {
        let input = core::str::from_utf8(&self.input_buf[..self.cursor]).ok()?;
        let trimmed = input.trim();

        if trimmed.is_empty() {
            return Some(ShellCmd::Unknown);
        }

        let parts: Vec<&str> = trimmed.splitn(2, ' ').collect();
        match parts[0] {
            "help" => Some(ShellCmd::Help),
            "uptime" => Some(ShellCmd::Uptime),
            "klog" => {
                let level = if parts.len() > 1 && parts[1] == "debug" {
                    Level::Debug
                } else if parts.len() > 1 && parts[1] == "error" {
                    Level::Error
                } else {
                    Level::Info
                };
                Some(ShellCmd::Klog(level))
            }
            "spawn" => Some(ShellCmd::Spawn),
            "echo" => Some(ShellCmd::Echo(parts.get(1).copied().unwrap_or(""))),
            _ => Some(ShellCmd::Unknown),
        }
    }

    fn cmd_help(&self) {
        println!("ZiqaKernel v0.6 Shell Commands:");
        println!("  help          - Show this help");
        println!("  uptime        - Show system uptime");
        println!("  klog [level]  - Show kernel log (error/debug/info)");
        println!("  spawn         - Spawn a demo process");
        println!("  echo <msg>    - Echo a message");
    }

    fn cmd_uptime(&self) {
        let ticks = crate::timer::uptime_ticks();
        let ms = crate::timer::uptime_ms();
        println!("Uptime: {} ticks, {} ms", ticks, ms);
    }

    fn cmd_klog(&self, level: Level) {
        println!("Kernel log (level >= {:?}):", level);
        crate::klog::KLOG.lock().dump_level(level);
    }

    fn cmd_spawn(&self) {
        let pid = crate::process::scheduler::spawn(
            AbiKind::ZiqaNative,
            VirtAddress::new(0x400000),
            VirtAddress::new(0x7FFF0000),
        );
        if let Some(p) = pid {
            println!("Spawned demo process PID={:?}", p);
        } else {
            println!("Failed to spawn process (max processes reached)");
        }
    }

    fn cmd_echo(&self, msg: &'static str) {
        println!("{}", msg);
    }

    fn cmd_unknown(&self) {
        println!("Unknown command. Type 'help' for available commands.");
    }
}

enum ShellCmd<'a> {
    Help,
    Uptime,
    Klog(Level),
    Spawn,
    Echo(&'a str),
    Unknown,
}

pub static mut SHELL: Shell = Shell::new();

pub fn init() {
    unsafe {
        SHELL = Shell::new();
    }
}

pub fn run() {
    init();
    unsafe {
        SHELL.run();
    }
}