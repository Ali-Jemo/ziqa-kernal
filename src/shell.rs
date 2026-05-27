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
                let parts: Vec<&str> = trimmed.splitn(3, ' ').collect();
                match parts[0] {
                    "help"    => self.cmd_help(),
                    "uptime"  => self.cmd_uptime(),
                    "klog"    => self.cmd_klog(parts.get(1).copied().unwrap_or("info")),
                    "spawn"   => self.cmd_spawn(),
                    "ps"      => self.cmd_ps(),
                    "kill"    => self.cmd_kill(parts.get(1).copied(), parts.get(2).copied()),
                    "sleep"   => self.cmd_sleep(parts.get(1).copied()),
                    "meminfo" => self.cmd_meminfo(),
                    "netstat" => self.cmd_netstat(),
                    "reboot"  => self.cmd_reboot(),
                    "clear"   => { for _ in 0..25 { println!(""); } },
                    "echo"    => println!("{}", parts.get(1).copied().unwrap_or("")),
                    _         => println!("Unknown command: {}. Type 'help'.", parts[0]),
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
        println!("Commands:");
        println!("  help              - this message");
        println!("  uptime            - kernel uptime in ms");
        println!("  ps                - list processes");
        println!("  spawn             - spawn a new process");
        println!("  kill <pid> [sig]  - send signal to process (default: SIGTERM=15)");
        println!("  sleep <ms>        - sleep current shell process N milliseconds");
        println!("  meminfo           - heap memory statistics");
        println!("  netstat           - network device statistics");
        println!("  klog [level]      - dump kernel log (debug/info/error)");
        println!("  reboot            - reboot the system");
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

    fn cmd_spawn(&self) {
        let pid = crate::process::scheduler::spawn(
            AbiKind::LinuxElf,
            VirtAddr::new(0x400000),
            VirtAddr::new(0x7fff_ffff_000),
        );
        match pid {
            Some(p) => println!("Spawned PID={}", p.0),
            None    => println!("spawn: no free slots"),
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
        let signum: u8 = sig_str.and_then(|s| s.parse().ok()).unwrap_or(15); // SIGTERM
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
        // Use the shell's own PID (0 = kernel/shell context)
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
        // ACPI/PS2 keyboard controller reboot (port 0x64, command 0xFE)
        unsafe {
            use x86_64::instructions::port::Port;
            let mut port: Port<u8> = Port::new(0x64);
            port.write(0xFE);
        }
        // If that didn't work, triple-fault
        loop { x86_64::instructions::hlt(); }
    }
}

pub fn start() -> ! {
    let mut shell = Shell::new();
    shell.run();
}

pub fn run() -> ! {
    start()
}
