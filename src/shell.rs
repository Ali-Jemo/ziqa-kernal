/// Interactive shell for ZiqaKernel

use alloc::vec::Vec;
use crate::{print, println};
use crate::klog::Level;
use crate::process::AbiKind;
use x86_64::VirtAddr;

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

    pub fn run(&mut self) -> ! {
        println!("[ZIQA] Starting interactive shell...");
        loop {
            print!("{}", self.prompt);
            self.read_line();

            let input = core::str::from_utf8(&self.input_buf[..self.cursor]).unwrap_or("");
            let trimmed = input.trim();
            if !trimmed.is_empty() {
                let parts: Vec<&str> = trimmed.splitn(2, ' ').collect();
                match parts[0] {
                    "help" => self.cmd_help(),
                    "uptime" => self.cmd_uptime(),
                    "klog" => self.cmd_klog(parts.get(1).copied().unwrap_or("info")),
                    "spawn" => self.cmd_spawn(),
                    "ps" => self.cmd_ps(),
                    "clear" => { for _ in 0..25 { println!(""); } },
                    "echo" => println!("{}", parts.get(1).copied().unwrap_or("")),
                    _ => println!("Unknown command: {}", parts[0]),
                }
            }
            self.cursor = 0;
        }
    }

    fn read_line(&mut self) {
        use crate::drivers::keyboard::read_stdin;
        let n = read_stdin(&mut self.input_buf);
        self.cursor = n;
    }

    fn cmd_help(&self) {
        println!("Available commands: help, uptime, klog, spawn, ps, clear, echo");
    }

    fn cmd_uptime(&self) {
        println!("Uptime: {} ms", crate::timer::uptime_ms());
    }

    fn cmd_klog(&self, level_str: &str) {
        let level = match level_str {
            "debug" => Level::Debug,
            "error" => Level::Error,
            _ => Level::Info,
        };
        crate::klog::KLOG.lock().dump_level(level);
    }

    fn cmd_spawn(&self) {
        let pid = crate::process::scheduler::spawn(
            AbiKind::LinuxElf,
            VirtAddr::new(0x400000),
            VirtAddr::new(0x7fff_ffff_000),
        );
        println!("Spawned PID={:?}", pid);
    }

    fn cmd_ps(&self) {
        crate::process::scheduler::SCHEDULER.lock().print_process_list();
    }
}

pub fn start() -> ! {
    let mut shell = Shell::new();
    shell.run();
}

pub fn run() -> ! {
    start()
}
