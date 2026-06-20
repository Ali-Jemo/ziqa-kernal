
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
    "help", "uptime", "ps", "spawn", "spawnelf",
    #[cfg(feature = "orbital")]
    "orbital",
    "exec", "kill",
    "doom", "tetris", "reboot", "echo", "clear", "edit", "ls", "cd", "pwd",
    "mkdir", "dir", "rm", "rmdir", "cat", "ping", "wget", "ifconfig",
    "mv", "cp", "touch", "stat", "du",
    "dashboard", "top", "history", "alias", "export",
    "jobs", "bg", "fg",
    #[cfg(feature = "games")]
    "nwm-test",
    "snap", "ls-snap", "rm-snap",
];

struct SyscallEntry {
    nr: u64,
    name: &'static str,
    category: &'static str,
    args: &'static str,
    desc: &'static str,
    probe: bool,
}

const SYSCALLS: &[SyscallEntry] = &[
    SyscallEntry { nr: crate::abi::syscall::nr::GETPID, name: "getpid", category: "process", args: "", desc: "current process id", probe: true },
    SyscallEntry { nr: crate::abi::syscall::nr::GETPPID, name: "getppid", category: "process", args: "", desc: "parent process id placeholder", probe: true },
    SyscallEntry { nr: crate::abi::syscall::nr::SCHED_YIELD, name: "sched_yield", category: "process", args: "", desc: "yield current userspace task", probe: false },
    SyscallEntry { nr: crate::abi::syscall::nr::FORK, name: "fork", category: "process", args: "", desc: "fork current process", probe: false },
    SyscallEntry { nr: crate::abi::syscall::nr::EXECVE, name: "execve", category: "process", args: "path argv envp", desc: "replace current process image", probe: false },
    SyscallEntry { nr: crate::abi::syscall::nr::WAITPID, name: "waitpid", category: "process", args: "pid status options", desc: "wait for child process", probe: false },
    SyscallEntry { nr: crate::abi::syscall::nr::KILL, name: "kill", category: "process", args: "pid sig", desc: "send signal", probe: false },
    SyscallEntry { nr: crate::abi::syscall::nr::EXIT, name: "exit", category: "process", args: "code", desc: "exit current process", probe: false },
    SyscallEntry { nr: crate::abi::syscall::nr::NANOSLEEP, name: "nanosleep", category: "time", args: "ms", desc: "sleep calling process", probe: false },
    SyscallEntry { nr: crate::abi::syscall::nr::CLOCK_NANOSLEEP, name: "clock_nanosleep", category: "time", args: "ms", desc: "sleep calling process", probe: false },
    SyscallEntry { nr: 205, name: "get_ticks", category: "time", args: "", desc: "kernel uptime in ms", probe: true },
    SyscallEntry { nr: 204, name: "fb_blit", category: "doom", args: "pixels palette w h x y", desc: "Doom framebuffer blit hook", probe: false },
    SyscallEntry { nr: 206, name: "get_key", category: "doom", args: "", desc: "Doom key poll hook", probe: true },
    SyscallEntry { nr: crate::abi::syscall::nr::ZIQA_CAP_REQUEST, name: "ziqa_cap_request", category: "capability", args: "kind path_ptr path_len flags", desc: "request a resource capability", probe: false },
    SyscallEntry { nr: crate::abi::syscall::nr::ZIQA_CAP_READ, name: "ziqa_cap_read", category: "capability", args: "id buf count offset", desc: "read via capability", probe: false },
    SyscallEntry { nr: crate::abi::syscall::nr::ZIQA_CAP_WRITE, name: "ziqa_cap_write", category: "capability", args: "id buf count offset", desc: "write via capability", probe: false },
    SyscallEntry { nr: crate::abi::syscall::nr::ZIQA_CAP_CLOSE, name: "ziqa_cap_close", category: "capability", args: "fd", desc: "close capability fd", probe: false },
    SyscallEntry { nr: crate::abi::syscall::nr::ZIQA_CAP_SEEK, name: "ziqa_cap_seek", category: "capability", args: "fd off whence", desc: "seek capability fd", probe: false },
    SyscallEntry { nr: crate::abi::syscall::nr::ZIQA_CAP_REVOKE, name: "ziqa_cap_revoke", category: "capability", args: "id", desc: "revoke capability", probe: false },
    SyscallEntry { nr: crate::abi::syscall::nr::ZIQA_SHM_CREATE, name: "ziqa_shm_create", category: "ipc", args: "size", desc: "create shared-memory segment", probe: false },
    SyscallEntry { nr: crate::abi::syscall::nr::ZIQA_SHM_ATTACH, name: "ziqa_shm_attach", category: "ipc", args: "id", desc: "attach shared-memory segment", probe: false },
    SyscallEntry { nr: crate::abi::syscall::nr::ZIQA_IPC_CREATE, name: "ziqa_ipc_create", category: "ipc", args: "", desc: "create IPC channel", probe: true },
    SyscallEntry { nr: crate::abi::syscall::nr::ZIQA_IPC_SEND, name: "ziqa_ipc_send", category: "ipc", args: "chan ptr len", desc: "send IPC message", probe: false },
    SyscallEntry { nr: crate::abi::syscall::nr::ZIQA_IPC_RECV, name: "ziqa_ipc_recv", category: "ipc", args: "chan ptr max", desc: "receive IPC message", probe: false },
    SyscallEntry { nr: crate::abi::syscall::nr::ZIQA_SIG_GETMASK, name: "ziqa_sig_getmask", category: "signal", args: "", desc: "read current signal mask", probe: true },
    SyscallEntry { nr: crate::abi::syscall::nr::ZIQA_SIG_SETMASK, name: "ziqa_sig_setmask", category: "signal", args: "mask", desc: "write current signal mask", probe: false },
    SyscallEntry { nr: crate::abi::syscall::nr::ZIQA_SIG_KILL, name: "ziqa_sig_kill", category: "signal", args: "pid sig", desc: "send Ziqa signal", probe: false },
    SyscallEntry { nr: crate::abi::syscall::nr::ZIQA_SIG_PAUSE, name: "ziqa_sig_pause", category: "signal", args: "", desc: "pause until signal", probe: false },
    SyscallEntry { nr: crate::abi::syscall::nr::ZIQA_DEV_GET_GPU_CHAN, name: "ziqa_dev_get_gpu_chan", category: "device", args: "", desc: "read GPU IPC channel id", probe: true },
    SyscallEntry { nr: crate::abi::syscall::nr::ZIQA_DEV_PCI_FIND, name: "ziqa_dev_pci_find", category: "device", args: "vendor device class subclass", desc: "find PCI device", probe: false },
    SyscallEntry { nr: crate::abi::syscall::nr::ZIQA_DEV_PCI_BAR, name: "ziqa_dev_pci_bar", category: "device", args: "bdf bar", desc: "read PCI BAR", probe: false },
    SyscallEntry { nr: crate::abi::syscall::nr::ZIQA_DEV_PCI_IRQ, name: "ziqa_dev_pci_irq", category: "device", args: "bdf", desc: "read PCI IRQ line", probe: false },
    SyscallEntry { nr: crate::abi::syscall::nr::ZIQA_DEV_PORT_IN, name: "ziqa_dev_port_in", category: "device", args: "port size", desc: "read I/O port", probe: false },
    SyscallEntry { nr: crate::abi::syscall::nr::ZIQA_DEV_PORT_OUT, name: "ziqa_dev_port_out", category: "device", args: "port size value", desc: "write I/O port", probe: false },
    SyscallEntry { nr: crate::abi::syscall::nr::NET_NOTIFY, name: "net_notify", category: "net", args: "queue", desc: "notify virtio-net queue", probe: false },
    SyscallEntry { nr: crate::abi::syscall::nr::NET_ACK, name: "net_ack", category: "net", args: "", desc: "ack virtio-net interrupt", probe: false },
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
const C_MAGENTA: &str = "\x1b[35m";
const C_CYAN: &str = "\x1b[36m";
#[allow(dead_code)]
const C_WHITE: &str = "\x1b[37m";

/// A parsed command with redirection and background info
#[derive(Clone)]
pub struct ParsedCmd {
    pub args: Vec<String>,
    pub stdin_file: Option<String>,
    pub stdout_file: Option<String>,
    pub stdout_append: bool,
    pub background: bool,
}

/// Signature for all builtin commands
pub type BuiltinFn = fn(&mut Shell, &[String]) -> i32;

pub struct Shell {
    input_buf: [u8; 256],
    cursor: usize,
    history: Vec<String>,
    history_pos: isize,
    cwd: String,
    prev_cwd: String,
    last_exit_status: i32,
    aliases: BTreeMap<String, String>,
    env: BTreeMap<String, String>,
    jobs: Vec<Job>,
    fg_job: Option<usize>,
    last_daemon_ms: u64,
}

impl Default for Shell {
    fn default() -> Self {
        Self::new()
    }
}

impl Shell {
    pub fn new() -> Self {
        Self {
            input_buf: [0; 256],
            cursor: 0,
            history: Vec::new(),
            history_pos: -1,
            cwd: String::new(),
            prev_cwd: String::new(),
            last_exit_status: 0,
            aliases: BTreeMap::new(),
            env: BTreeMap::new(),
            jobs: Vec::new(),
            fg_job: None,
            last_daemon_ms: 0,
        }
    }

    fn cwd_str(&self) -> &str {
        if self.cwd.is_empty() { "/" } else { &self.cwd }
    }

    fn resolve_path(&self, path: &str) -> String {
        if path.is_empty() {
            return self.cwd_str().to_string();
        }
        let cwd_bytes = self.cwd_str().as_bytes();
        crate::fs::resolve_path(cwd_bytes, cwd_bytes.len(), path)
    }

    fn normalize(path: &str) -> String {
        crate::fs::normalize_path(path)
    }

    fn skip_ws(input: &str, pos: &mut usize) {
        while *pos < input.len() && input.as_bytes()[*pos].is_ascii_whitespace() {
            *pos += 1;
        }
    }

    fn read_word<'a>(input: &'a str, pos: &mut usize) -> Result<String, &'a str> {
        let mut word = String::new();
        let bytes = input.as_bytes();
        while *pos < bytes.len() {
            let b = bytes[*pos];
            if b == b' ' || b == b'\t' {
                break;
            }
            if b == b'\\' && *pos + 1 < bytes.len() {
                *pos += 1;
                word.push(bytes[*pos] as char);
                *pos += 1;
                continue;
            }
            if b == b'"' || b == b'\'' {
                let quote = b;
                *pos += 1;
                loop {
                    if *pos >= bytes.len() {
                        return Err("unclosed quote");
                    }
                    if bytes[*pos] == quote {
                        break;
                    }
                    if quote == b'"' && bytes[*pos] == b'\\' && *pos + 1 < bytes.len() {
                        *pos += 1;
                        word.push(bytes[*pos] as char);
                        *pos += 1;
                        continue;
                    }
                    word.push(bytes[*pos] as char);
                    *pos += 1;
                }
                *pos += 1;
                continue;
            }
            if b == b'|' || b == b'>' || b == b'<' || b == b'&' {
                if !word.is_empty() {
                    break;
                }
                word.push(b as char);
                *pos += 1;
                if (b == b'>' || b == b'&') && *pos < bytes.len() && bytes[*pos] == b {
                    word.push(b as char);
                    *pos += 1;
                }
                break;
            }
            word.push(b as char);
            *pos += 1;
        }
        if word.is_empty() {
            return Err("empty word");
        }
        Ok(word)
    }

    fn expand_vars(s: &str, env: &BTreeMap<String, String>, last_exit: i32, shell_pid: u64) -> String {
        let mut out = String::new();
        let bytes = s.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'$' && i + 1 < bytes.len() {
                match bytes[i + 1] {
                    b'$' => { write!(out, "{}", shell_pid).ok(); i += 2; continue; }
                    b'?' => { write!(out, "{}", last_exit).ok(); i += 2; continue; }
                    b'{' => {
                        if let Some(end) = s[i+2..].find('}').map(|p| i + 2 + p) {
                            let var = &s[i+2..end];
                            let val = env.get(var).map(|s| s.as_str()).unwrap_or("");
                            out.push_str(val);
                            i = end + 1;
                            continue;
                        }
                    }
                    b'a'..=b'z' | b'A'..=b'Z' | b'_' => {
                        let start = i + 1;
                        let mut end = start;
                        while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
                            end += 1;
                        }
                        let var = &s[start..end];
                        let val = env.get(var).map(|s| s.as_str()).unwrap_or("");
                        out.push_str(val);
                        i = end;
                        continue;
                    }
                    _ => {}
                }
            }
            out.push(bytes[i] as char);
            i += 1;
        }
        out
    }

    fn parse_line(input: &str) -> Result<ParsedCmd, &str> {
        let input = input.trim();
        let mut pos = 0;
        Self::skip_ws(input, &mut pos);

        let mut args = Vec::new();
        let mut stdin_file = None;
        let mut stdout_file = None;
        let mut stdout_append = false;
        let mut background = false;

        while pos < input.len() {
            let word = Self::read_word(input, &mut pos)?;
            match word.as_str() {
                "<" => {
                    Self::skip_ws(input, &mut pos);
                    stdin_file = Some(Self::read_word(input, &mut pos)?);
                }
                ">" => {
                    Self::skip_ws(input, &mut pos);
                    stdout_file = Some(Self::read_word(input, &mut pos)?);
                    stdout_append = false;
                }
                ">>" => {
                    Self::skip_ws(input, &mut pos);
                    stdout_file = Some(Self::read_word(input, &mut pos)?);
                    stdout_append = true;
                }
                "&" => {
                    background = true;
                }
                _ => args.push(word),
            }
            Self::skip_ws(input, &mut pos);
        }

        if args.is_empty() {
            return Err("empty command");
        }

        Ok(ParsedCmd { args, stdin_file, stdout_file, stdout_append, background })
    }

    fn find_builtin(name: &str) -> Option<BuiltinFn> {
        match name {
            "help"      => Some(Self::cmd_help),
            "uptime"    => Some(Self::cmd_uptime),
            "ps"        => Some(Self::cmd_ps),
            "kill"      => Some(Self::cmd_kill),
            "sleep"     => Some(Self::cmd_sleep),
            "meminfo"   => Some(Self::cmd_meminfo),
            "lsblk"     => Some(Self::cmd_lsblk),
            "blkinfo"   => Some(Self::cmd_blkinfo),
            "mount"     => Some(Self::cmd_mount),
            #[cfg(feature = "ziqafs")]
            "diskinfo"  => Some(Self::cmd_diskinfo),
            #[cfg(feature = "net")]
            "netstat"   => Some(Self::cmd_netstat),
            "klog"      => Some(Self::cmd_klog),
            "syscalls"  => Some(Self::cmd_syscalls),
            "syscall"   => Some(Self::cmd_syscall),
            #[cfg(feature = "games")]
            "doom"      => Some(Self::cmd_doom),
            #[cfg(feature = "games")]
            "tetris"    => Some(Self::cmd_tetris),
            "reboot"    => Some(Self::cmd_reboot),
            "echo"      => Some(Self::cmd_echo),
            "clear"     => Some(Self::cmd_clear),
            "edit"      => Some(Self::cmd_edit),
            "ls"        => Some(Self::cmd_ls),
            "cd"        => Some(Self::cmd_cd),
            "pwd"       => Some(Self::cmd_pwd),
            "mkdir"     => Some(Self::cmd_mkdir),
            "dir"       => Some(Self::cmd_dir),
            "rm"        => Some(Self::cmd_rm),
            "rmdir"     => Some(Self::cmd_rmdir),
            "cat"       => Some(Self::cmd_cat),
            #[cfg(feature = "net")]
            "ping"      => Some(Self::cmd_ping),
            #[cfg(feature = "net")]
            "wget"      => Some(Self::cmd_wget),
            #[cfg(feature = "net")]
            "ifconfig"  => Some(Self::cmd_ifconfig),
            "mv"        => Some(Self::cmd_mv),
            #[cfg(feature = "ziqafs")]
            "cp"        => Some(Self::cmd_cp),
            "touch"     => Some(Self::cmd_touch),
            "writefile" => Some(Self::cmd_writefile),
            #[cfg(feature = "ziqafs")]
            "stat"      => Some(Self::cmd_stat),
            #[cfg(feature = "ziqafs")]
            "du"        => Some(Self::cmd_du),
            "history"   => Some(Self::cmd_history),
            "alias"     => Some(Self::cmd_alias),
            "export"    => Some(Self::cmd_export),
            "jobs"      => Some(Self::cmd_jobs),
            "bg"        => Some(Self::cmd_bg),
            "fg"        => Some(Self::cmd_fg),
            "dashboard" | "top" => Some(Self::cmd_dashboard),
            "spawn"     => Some(Self::cmd_spawn),
            "spawnelf"  => Some(Self::cmd_spawn_elf),
            #[cfg(feature = "orbital")]
            "orbital"   => Some(Self::cmd_orbital),
            "exec"      => Some(Self::cmd_exec),
            #[cfg(feature = "games")]
            "nwm-test"  => Some(Self::cmd_nwm_test),
            "bench"     => Some(Self::cmd_bench),
            "test"      => Some(Self::cmd_test),
            "compress"  => Some(Self::cmd_compress),
            "snap"      => Some(Self::cmd_snap),
            "ls-snap"   => Some(Self::cmd_ls_snap),
            "rm-snap"   => Some(Self::cmd_rm_snap),
            _ => None,
        }
    }

    fn poll_jobs(&mut self) {
        let pids = crate::process::scheduler::list_pids();
        let mut i = 0;
        while i < self.jobs.len() {
            if !pids.contains(&self.jobs[i].pid) {
                let done = self.jobs.remove(i);
                println!("[{}] Done  {}", i + 1, done.command);
                continue;
            }
            i += 1;
        }
    }

    fn execute_cmd(&mut self, parsed: &ParsedCmd) -> i32 {
        if parsed.args.is_empty() {
            return 0;
        }
        let name = &parsed.args[0];
        let args = &parsed.args[1..];

        if let Some(func) = Self::find_builtin(name) {
            func(self, args)
        } else {
            // Try spawning as ELF
            let resolved = self.resolve_path(name);
            let mut buf = [0u8; 65536];
            match VFS.read().read_raw(&resolved, &mut buf, 0) {
                Ok(n) if n > 0 => {
                    match crate::process::scheduler::spawn_elf(&buf[..n]) {
                        Some(pid) => {
                            println!("Spawned PID={} from '{}'", pid.0, resolved);
                            if parsed.background {
                                self.jobs.push(Job {
                                    pid,
                                    command: parsed.args.join(" "),
                                    state: JobState::Running,
                                });
                            }
                            0
                        }
                        None => { println!("Failed to spawn '{}'", resolved); 1 }
                    }
                }
                _ => {
                    let suggestion = self.find_similar_command(name);
                    if let Some(s) = suggestion {
                        println!("Unknown command: {}. Did you mean '{}'?", name, s);
                    } else {
                        println!("Unknown command: {}. Type 'help'.", name);
                    }
                    1
                }
            }
        }
    }

    pub fn run(&mut self) -> ! {
        // Keep timer-driven preemption disabled while the in-kernel shell owns
        // the console.  The current scheduler can switch the shell away to
        // boot-spawned demos/drivers for long stretches, which makes keyboard
        // input look frozen after the first timer slice.
        crate::process::scheduler::disable_preemption();

        let ms = crate::timer::uptime_ms();
        println!("\x1b[36m\x1b[1m  ⚡ ZiqaKernel v0.1 ⚡\x1b[0m");
        println!("\x1b[2m  ─────────────────────────────────────────────\x1b[0m");
        println!("\x1b[32m  System ready\x1b[0m  uptime={}ms  cpu=x86_64  mem={}MiB",
            ms, crate::memory::heap::HEAP_SIZE / (1024 * 1024));
        println!("\x1b[2m  Type 'help' for available commands\x1b[0m");
        println!("");

        self.update_status_bar();

        loop {
            crate::net::stack::poll_network();
            self.poll_jobs();

            // Background compression daemon (every 5 s, up to 64 pages)
            let now = crate::timer::uptime_ms();
            if now.wrapping_sub(self.last_daemon_ms) > 5000 {
                self.last_daemon_ms = now;
                crate::memory::compression::daemon::run_daemon_cycle(64);
            }

            // ── Zero-alloc prompt ──
            let cwd = self.cwd_str();
            let time = crate::timer::uptime_ms() / 1000;
            let mut buf = [0u8; 128];
            let mut pos = 0;

            let header = "\x1b[36mziqa\x1b[0m\x1b[33m[\x1b[0m";
            let hb = header.as_bytes();
            buf[pos..pos + hb.len()].copy_from_slice(hb);
            pos += hb.len();

            if self.last_exit_status != 0 {
                let mut tmp = [0u8; 12];
                let mut tp = 0;
                let mut n = self.last_exit_status;
                if n < 0 { tmp[tp] = b'-'; tp += 1; n = -n; }
                let start = tp;
                loop {
                    tmp[tp] = b'0' + (n % 10) as u8;
                    tp += 1;
                    n /= 10;
                    if n == 0 { break; }
                }
                tmp[start..tp].reverse();
                buf[pos..pos + (tp - start)].copy_from_slice(&tmp[start..tp]);
                pos += tp - start;
            }

            let time_str = alloc::format!("{}", time);
            let tb = time_str.as_bytes();
            let remaining = buf.len() - pos;
            let tc = tb.len().min(remaining);
            buf[pos..pos + tc].copy_from_slice(&tb[..tc]);
            pos += tc;

            if cwd != "/" {
                let space = b" ";
                buf[pos] = space[0]; pos += 1;
                let cb = cwd.as_bytes();
                let remaining = buf.len() - pos;
                let cc = cb.len().min(remaining);
                buf[pos..pos + cc].copy_from_slice(&cb[..cc]);
                pos += cc;
            }

            let tail = "\x1b[33m]\x1b[0m \x1b[32m>\x1b[0m ";
            let tl = tail.as_bytes();
            let remaining = buf.len() - pos;
            let tc2 = tl.len().min(remaining);
            buf[pos..pos + tc2].copy_from_slice(&tl[..tc2]);
            pos += tc2;

            let prompt = core::str::from_utf8(&buf[..pos]).unwrap_or("> ");
            print!("{}", prompt);

            self.read_line();

            let has_input = self.input_buf[..self.cursor].iter().any(|&b| b != b' ' && b != b'\t' && b != b'\r' && b != b'\n');
            if has_input {
                self.push_history();
                let input = core::str::from_utf8(&self.input_buf[..self.cursor]).unwrap_or("");
                let trimmed = input.trim();

                // Alias expansion
                let expanded_input = if let Some(idx) = trimmed.find(char::is_whitespace) {
                    let first = &trimmed[..idx];
                    let rest = &trimmed[idx..];
                    if let Some(alias) = self.aliases.get(first) {
                        let mut s = alias.clone();
                        s.push_str(rest);
                        s
                    } else {
                        trimmed.to_string()
                    }
                } else if let Some(alias) = self.aliases.get(trimmed) {
                    alias.clone()
                } else {
                    trimmed.to_string()
                };

                // Environment variable expansion
                let expanded = Self::expand_vars(&expanded_input, &self.env, self.last_exit_status, 0);

                match Self::parse_line(&expanded) {
                    Ok(cmd) => {
                        self.last_exit_status = self.execute_cmd(&cmd);
                    }
                    Err(e) => {
                        println!("{}", e);
                        self.last_exit_status = 1;
                    }
                }
            }
            self.update_status_bar();
            println!("\x1b[2m──────────────────────────────────────────────────────────────────────\x1b[0m");
            self.history_pos = -1;
            self.cursor = 0;
        }
    }

    fn push_history(&mut self) {
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

    fn prompt_len(&self) -> usize {
        let cwd = self.cwd_str();
        if cwd == "/" {
            "ziqa > ".chars().count()
        } else {
            ("ziqa ".to_string() + cwd + " > ").chars().count()
        }
    }

    fn refresh_line(&self, idx: usize) {
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

    fn load_history(&mut self, idx: &mut usize) {
        let entry = &self.history[self.history_pos as usize];
        let bytes = entry.as_bytes();
        let len = bytes.len().min(255);
        self.input_buf[..len].copy_from_slice(&bytes[..len]);
        if len < 256 { self.input_buf[len] = 0; }
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

    fn complete_arg(&self, cmd: &str, prefix: &str) -> Vec<String> {
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

    fn cmd_alias(&mut self, args: &[String]) -> i32 {
        let arg1 = args.first().map(|s| s.as_str());
        let arg2 = args.get(1).map(|s| s.as_str());
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
        0
    }

    fn cmd_export(&mut self, args: &[String]) -> i32 {
        match args.first().map(|s| s.as_str()) {
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
        0
    }

    fn cmd_help(&mut self, _args: &[String]) -> i32 {
        println!("{}{}  ⚡ ZiqaKernel Shell ⚡{}", C_CYAN, C_BOLD, C_RESET);
        println!("{}  ─────────────────────────────────────{}", C_DIM, C_RESET);
        println!("");

        let groups: &[(&str, &[(&str, &str)])] = &[
            ("Filesystem", &[
                ("ls [path]",        "list directory contents"),
                ("dir [path]",       "detailed listing with sizes"),
                ("cd [path]",        "change directory (.. / - for previous). Use -v to print new cwd"),
                ("pwd",              "print working directory"),
                ("mkdir <path>",     "create a directory"),
                ("rm <path>",        "remove a file"),
                ("cat [-n] <path>",  "display file (-n for line numbers)"),
                ("mv <src> <dst>",   "move/rename a file"),
                #[cfg(feature = "ziqafs")]
                ("cp <src> <dst>",   "copy a file"),
                ("touch <path>",     "create file or update mtime"),
                #[cfg(feature = "ziqafs")]
                ("stat <path>",      "show inode details"),
                #[cfg(feature = "ziqafs")]
                ("du [path]",        "disk usage in blocks"),
                ("edit <path>",      "nano-like text editor"),
            ]),
            ("Process", &[
                ("spawnelf <path>",  "spawn ELF from VFS"),
                #[cfg(feature = "orbital")]
                ("orbital",          "spawn embedded Redox Orbital compositor"),
                ("exec <pid>",       "execute process entry point"),
                ("kill <pid> [sig]", "send signal (name or number)"),
                ("sleep <ms|Ns>",    "sleep milliseconds or Ns seconds"),
            ]),
            ("System", &[
                ("help",             "show this message"),
                ("uptime",           "kernel uptime + system summary"),
                ("meminfo",          "heap memory statistics"),
                ("lsblk",            "list registered block devices"),
                ("blkinfo <dev>",    "show details of a block device"),
                ("mount",            "list mounted filesystems"),
                #[cfg(feature = "ziqafs")]
                ("diskinfo",         "ZiqaFS disk usage + fsck"),
                ("klog [lvl] [-N]",  "kernel log (debug/info/error, -N last N)"),
                ("syscalls [filter]", "list kernel syscall table"),
                ("syscall <name|nr>", "probe a safe syscall"),
                ("dashboard",        "real-time system dashboard"),
                ("reboot",           "reboot the system"),
                ("clear",            "clear screen"),
                ("echo <text>",      "print text. Use -N for null-terminated output"),
                ("history",          "show command history"),
                ("alias [n=v]",      "define or list aliases"),
                ("export N=V",       "set environment variable"),
            ]),
            #[cfg(feature = "net")]
            ("Network", &[
                ("netstat",          "network device statistics"),
                ("ping [-c N] <ip>", "ICMP echo (SLIRP: gateway only, no external ICMP)"),
                ("wget [-O f] <url>", "HTTP GET (SLIRP: use IP literals, limited DNS)"),
            ]),
            #[cfg(feature = "games")]
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
        0
    }

    fn parse_syscall_nr(text: &str) -> Option<u64> {
        if let Some(hex) = text.strip_prefix("0x") {
            u64::from_str_radix(hex, 16).ok()
        } else {
            text.parse::<u64>().ok()
        }
    }

    fn find_syscall_entry(text: &str) -> Option<&'static SyscallEntry> {
        if let Some(nr) = Self::parse_syscall_nr(text) {
            SYSCALLS.iter().find(|entry| entry.nr == nr)
        } else {
            SYSCALLS.iter().find(|entry| entry.name == text)
        }
    }

    fn cmd_syscalls(&mut self, args: &[String]) -> i32 {
        let filter = args.first().map(|s| s.as_str()).unwrap_or("");
        println!("{}{}  SYSCALLS{}", C_YELLOW, C_BOLD, C_RESET);
        println!("  {:>5}  {:<22} {:<10} {:<22} {}", "NR", "NAME", "GROUP", "ARGS", "SAFE");
        let mut shown = 0usize;
        for entry in SYSCALLS {
            let nr_text = alloc::format!("{}", entry.nr);
            let matches = filter.is_empty()
                || entry.name.starts_with(filter)
                || entry.category == filter
                || nr_text == filter;
            if !matches {
                continue;
            }
            println!("  {:>5}  {:<22} {:<10} {:<22} {}",
                entry.nr,
                entry.name,
                entry.category,
                entry.args,
                if entry.probe { "probe" } else { "listed" });
            shown += 1;
        }
        if shown == 0 {
            println!("  no syscalls match '{}'", filter);
            return 1;
        }
        println!("{}  {} syscall(s). Use: syscall <name|nr>{}", C_DIM, shown, C_RESET);
        0
    }

    fn cmd_syscall(&mut self, args: &[String]) -> i32 {
        let Some(name) = args.first() else {
            println!("Usage: syscall <name|nr>");
            println!("Try: syscalls process    or: syscall getpid");
            return 1;
        };
        let Some(entry) = Self::find_syscall_entry(name) else {
            println!("Unknown syscall '{}'. Use 'syscalls' to list supported numbers.", name);
            return 1;
        };
        println!("{}{}  {} ({}){}", C_CYAN, C_BOLD, entry.name, entry.nr, C_RESET);
        println!("  group: {}", entry.category);
        println!("  args : {}", if entry.args.is_empty() { "(none)" } else { entry.args });
        println!("  desc : {}", entry.desc);
        if !entry.probe {
            println!("  status: listed only; unsafe or requires userspace process context");
            return 0;
        }

        let value = match entry.nr {
            crate::abi::syscall::nr::GETPID => {
                crate::process::scheduler::with_current_task(|p| p.pid.0).unwrap_or(0)
            }
            crate::abi::syscall::nr::GETPPID => {
                crate::process::scheduler::with_current_task(|p| p.parent).unwrap_or(1)
            }
            205 => crate::timer::uptime_ms(),
            206 => 0,
            crate::abi::syscall::nr::ZIQA_IPC_CREATE => {
                crate::ipc::create_channel().map(|id| id as u64).unwrap_or(u64::MAX)
            }
            crate::abi::syscall::nr::ZIQA_SIG_GETMASK => {
                crate::process::scheduler::with_current_task(|p| p.signals.blocked as u64).unwrap_or(0)
            }
            crate::abi::syscall::nr::ZIQA_DEV_GET_GPU_CHAN => {
                crate::drivers::virtio_gpu::GPU_IPC_CHANNEL.lock().as_ref().copied().unwrap_or(0) as u64
            }
            _ => 0,
        };
        if value == u64::MAX {
            println!("  result: -1");
            return 1;
        }
        println!("  result: {} (0x{:x})", value, value);
        0
    }

    fn cmd_history(&mut self, _args: &[String]) -> i32 {
        println!("{}{}  HISTORY {} {}", C_YELLOW, C_BOLD, C_RESET, C_DIM);
        for (i, entry) in self.history.iter().enumerate() {
            println!("  {:>3}: {}", i, entry);
        }
        0
    }

    fn cmd_uptime(&mut self, _args: &[String]) -> i32 {
        let ms = crate::timer::uptime_ms();
        let secs = ms / 1000;
        let mins = secs / 60;
        let hrs = mins / 60;
        let days = hrs / 24;
        let proc_count = crate::process::scheduler::list_pids().len();
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
        0
    }

    fn cmd_klog(&mut self, args: &[String]) -> i32 {
        let level_str = args.join(" ");
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
        let skip = total.saturating_sub(limit);
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
        0
    }


    fn cmd_spawn(&mut self, args: &[String]) -> i32 {
        self.cmd_spawn_elf(args)
    }

    fn cmd_spawn_elf(&mut self, args: &[String]) -> i32 {
        let p = match args.first().map(|s| s.as_str()) {
            Some(s) => s,
            None => { println!("Usage: spawnelf <path>"); return 1; }
        };
        let resolved = self.resolve_path(p);
        let is_orbital = resolved.ends_with("orbital.elf");
        match crate::fs::vfs::VFS.read().read_raw_all(&resolved) {
            Ok(data) => {
                let pid = if is_orbital {
                    crate::process::scheduler::spawn_redox_elf_from_vec(data)
                } else {
                    crate::process::scheduler::spawn_elf_from_vec(data)
                };
                match pid {
                    Some(pid) => println!("Spawned PID={} from '{}'", pid.0, resolved),
                    None => println!("spawnelf: failed to spawn from '{}'", resolved),
                }
            }
            Err(e) => println!("spawnelf: failed to read '{}': {:?}", resolved, e),
        }
        0
    }
    #[cfg(feature = "orbital")]
    fn cmd_orbital(&mut self, _args: &[String]) -> i32 {
        let binary = include_bytes!("../assets/orbital.elf");
        match crate::process::scheduler::spawn_redox_elf(binary) {
            Some(pid) => {
                crate::process::scheduler::with_process_mut(pid, |proc| {
                    let full = crate::capability::Permissions::full();
                    proc.capabilities.grant(crate::capability::ResourceKind::File, full, 0, None);
                    proc.capabilities.grant(crate::capability::ResourceKind::Memory, full, 0, None);
                    proc.capabilities.grant(crate::capability::ResourceKind::DeviceIo, full, 0, None);
                    proc.capabilities.grant(crate::capability::ResourceKind::IpcChannel, full, 0, None);
                });
                println!("Spawned Orbital PID={} from embedded assets/orbital.elf", pid.0);
                crate::process::scheduler::yield_now();
            }
            None => println!("orbital: failed to spawn embedded assets/orbital.elf"),
        }
        0
    }


    fn cmd_exec(&mut self, args: &[String]) -> i32 {
        let pid_val = match args.first().and_then(|s| s.parse::<u64>().ok()) {
            Some(v) => v,
            None => { println!("Usage: exec <pid>"); return 1; }
        };
        let pid = crate::process::Pid(pid_val);
        use crate::process::scheduler::{self, with_process, with_process_mut};

        let Some(entry) = with_process(pid, |proc| {
            println!("[EXEC] Switching to PID {} entry=0x{:x}", pid_val, proc.entry_point.as_u64());
            Some(proc.entry_point.as_u64())
        }) else {
            println!("exec: no process with PID {}", pid_val);
            return 1;
        };
        let _entry_vaddr = entry;

        with_process_mut(pid, |proc| {
            proc.state = crate::process::ProcessState::Ready;
        });

        scheduler::yield_now();
        0
    }

    fn cmd_ps(&mut self, _args: &[String]) -> i32 {
        let pids = crate::process::scheduler::list_pids();
        let mut rows = alloc::vec::Vec::new();
        for pid in pids {
            if let Some(row) = crate::process::scheduler::with_process(pid, |p| {
                (p.pid.0, p.state, p.priority, p.parent, p.abi, p.entry_point.as_u64())
            }) {
                rows.push(row);
            }
        }
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
                crate::process::ProcessState::Exited(_)  => (C_RED,    "Exited      "),
                crate::process::ProcessState::Canceled  => (C_MAGENTA,"Canceled    "),
            };
            let abi_s = match abi {
                crate::process::AbiKind::LinuxElf   => "Linux/ELF ",
                crate::process::AbiKind::Wasm        => "WASM      ",
                crate::process::AbiKind::ZiqaNative  => "ZiqaNative",
                crate::process::AbiKind::RedoxElf    => "Redox/ELF ",
            };
            println!("  │ {:>4} │ {}{}{} │ {:>4} │ {:>6} │ {} │ 0x{:012x} │",
                pid, sc, ss, C_RESET, pri, parent, abi_s, entry);
        }
        println!("  └──────┴──────────────┴──────┴────────┴──────────┴────────────────┘");
        0
    }

    fn cmd_kill(&mut self, args: &[String]) -> i32 {
        let pid_str = args.first().map(|s| s.as_str());
        let sig_str = args.get(1).map(|s| s.as_str());
        let pid_val = match pid_str.and_then(|s| s.parse::<u64>().ok()) {
            Some(v) => v,
            None => { println!("Usage: kill <pid> [signal]\n  Signals: SIGTERM(15) SIGKILL(9) SIGINT(2) SIGHUP(1) SIGUSR1(10) SIGUSR2(12)"); return 1; }
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
        let ok = crate::process::scheduler::SCHEDULER
            .send_signal(crate::process::Pid(pid_val), signum);
        if ok {
            println!("Sent {}({}) to PID {}", sig_name, signum, pid_val);
        } else {
            println!("kill: ({}) - No such process", pid_val);
        }
        0
    }

    fn cmd_sleep(&mut self, args: &[String]) -> i32 {
        let arg = match args.first().map(|s| s.as_str()) {
            Some(v) => v,
            None => { println!("Usage: sleep <ms>  or  sleep <N>s"); return 1; }
        };
        let ms: u64 = if let Some(s) = arg.strip_suffix('s') {
            s.parse::<u64>().unwrap_or(0) * 1000
        } else {
            arg.parse().unwrap_or(0)
        };
        if ms == 0 { println!("sleep: invalid duration '{}'", arg); return 1; }
        crate::timer::sleep_ms(crate::process::Pid(0), ms);
        if ms >= 1000 { println!("Slept {}.{}s", ms / 1000, (ms % 1000) / 100); }
        else { println!("Slept {}ms", ms); }
        0
    }

    fn cmd_meminfo(&mut self, _args: &[String]) -> i32 {
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
        0
    }

    fn cmd_bench(&mut self, _args: &[String]) -> i32 {
        use crate::memory::heapstats::get_stats;
        use core::arch::x86_64::_rdtsc;

        println!("{}{}  BENCHMARK{}", C_YELLOW, C_BOLD, C_RESET);

        // ── Heap Stats ──
        let stats = get_stats();
        println!("  Heap:  {} allocs  {} frees  {} live blocks  {} B current  {} B peak",
            stats.total_allocations, stats.total_frees, stats.current_blocks,
            stats.current_usage_bytes(), stats.peak_usage_bytes);

        // ── parse_line throughput ──
        let inputs = [
            "ls -la /home",
            "cat /var/log/messages | grep error",
            "echo hello world > /tmp/out.txt",
            "ping -c 4 192.168.1.1",
            "cc -O2 -Wall -I/usr/include -L/usr/lib main.c -o main",
            "find /usr -name '*.rs' -type f 2>/dev/null",
        ];
        const PARSE_ITERS: u64 = 1000;
        let mut parse_total = 0u64;
        let mut parse_min = u64::MAX;
        let mut parse_max = 0u64;
        for _ in 0..PARSE_ITERS {
            for inp in &inputs {
                let start = unsafe { _rdtsc() };
                let _ = Self::parse_line(inp);
                let end = unsafe { _rdtsc() };
                let cycles = end.wrapping_sub(start);
                parse_total += cycles;
                parse_min = parse_min.min(cycles);
                parse_max = parse_max.max(cycles);
            }
        }
        let n_parses = PARSE_ITERS * inputs.len() as u64;
        println!("  parse_line:  {} iters  avg={} cyc  min={} cyc  max={} cyc",
            n_parses, parse_total / n_parses, parse_min, parse_max);

        // ── normalize path ──
        let paths = [
            "/usr/local/bin/../lib/./gcc",
            "/home/user/../../etc/passwd",
            "///usr//local//",
            "/a/b/c/d/e/f/g/h/i/j/k/l/m/n/o/p",
            "./relative/path/with/./dots/../here",
        ];
        const NORM_ITERS: u64 = 2000;
        let mut norm_total = 0u64;
        for _ in 0..NORM_ITERS {
            for p in &paths {
                let start = unsafe { _rdtsc() };
                let _ = Shell::normalize(p);
                let end = unsafe { _rdtsc() };
                norm_total += end.wrapping_sub(start);
            }
        }
        let n_norms = NORM_ITERS * paths.len() as u64;
        println!("  normalize:   {} iters  avg={} cyc",
            n_norms, norm_total / n_norms);

        // ── find_builtin dispatch ──
        let cmd_names = [
            "help", "uptime", "ps", "kill", "sleep",
            "meminfo", "diskinfo", "netstat", "klog", "doom",
            "tetris", "reboot", "echo", "clear", "edit",
            "ls", "cd", "pwd", "mkdir", "dir",
            "rm", "rmdir", "cat", "ping", "wget",
            "ifconfig", "mv", "cp", "touch", "stat",
            "du", "alias", "export", "jobs", "bg", "fg",
            "spawn", "spawnelf", "exec", "nonexistent",
        ];
        const DISPATCH_ITERS: u64 = 5000;
        let mut dispatch_total = 0u64;
        for _ in 0..DISPATCH_ITERS {
            for name in &cmd_names {
                let start = unsafe { _rdtsc() };
                let _ = Self::find_builtin(name);
                let end = unsafe { _rdtsc() };
                dispatch_total += end.wrapping_sub(start);
            }
        }
        let n_dispatches = DISPATCH_ITERS * cmd_names.len() as u64;
        println!("  find_builtin: {} iters  avg={} cyc",
            n_dispatches, dispatch_total / n_dispatches);

        // ── prompt_len ──
        self.cwd = "/".to_string();
        const PROMPT_ITERS: u64 = 5000;
        let mut prompt_total = 0u64;
        for _ in 0..PROMPT_ITERS {
            let start = unsafe { _rdtsc() };
            let _ = self.prompt_len();
            let end = unsafe { _rdtsc() };
            prompt_total += end.wrapping_sub(start);
        }
        println!("  prompt_len:  {} iters  avg={} cyc",
            PROMPT_ITERS, prompt_total / PROMPT_ITERS);
        self.cwd = "/home/user/projects/ziqakernel/src".to_string();
        prompt_total = 0;
        for _ in 0..PROMPT_ITERS {
            let start = unsafe { _rdtsc() };
            let _ = self.prompt_len();
            let end = unsafe { _rdtsc() };
            prompt_total += end.wrapping_sub(start);
        }
        println!("  prompt_len (long cwd): {} iters  avg={} cyc",
            PROMPT_ITERS, prompt_total / PROMPT_ITERS);
        self.cwd = String::new();

        // ── expand_vars ──
        let env = BTreeMap::from([
            ("HOME".to_string(), "/root".to_string()),
            ("USER".to_string(), "root".to_string()),
            ("PATH".to_string(), "/usr/bin:/bin:/sbin".to_string()),
            ("SHELL".to_string(), "/bin/zsh".to_string()),
        ]);
        let var_inputs = [
            "echo $HOME/$USER",
            "echo ${PATH}:$HOME/bin",
            "exit code: $?  pid: $$",
            "$HOME/${USER}${SHELL}",
            "no variables here",
        ];
        const EXPAND_ITERS: u64 = 1000;
        let mut expand_total = 0u64;
        for _ in 0..EXPAND_ITERS {
            for inp in &var_inputs {
                let start = unsafe { _rdtsc() };
                let _ = Self::expand_vars(inp, &env, 0, 42);
                let end = unsafe { _rdtsc() };
                expand_total += end.wrapping_sub(start);
            }
        }
        let n_expands = EXPAND_ITERS * var_inputs.len() as u64;
        println!("  expand_vars: {} iters  avg={} cyc",
            n_expands, expand_total / n_expands);

        // ── Memory Compression Benchmarks ──
        crate::memory::compression::bench::run_benchmarks();

        println!("{}{}  END BENCHMARK{}", C_GREEN, C_BOLD, C_RESET);
        0
    }

    fn cmd_test(&mut self, _args: &[String]) -> i32 {
        println!("{}{}  SHELL UNIT TESTS{}", C_YELLOW, C_BOLD, C_RESET);
        let mut passed = 0u32;
        let mut failed = 0u32;

        macro_rules! test {
            ($name:expr, $body:expr) => {{
                if $body {
                    passed += 1;
                } else {
                    failed += 1;
                    println!("  {}{}FAIL{}  {}", C_RED, C_BOLD, C_RESET, $name);
                }
            }};
        }

        // ── parse_line ──
        let r = Shell::parse_line("ls -la /home").unwrap();
        test!("parse: simple", r.args == ["ls", "-la", "/home"]);
        test!("parse: no redirect", r.stdout_file.is_none());
        test!("parse: no bg", !r.background);

        let r = Shell::parse_line("echo hello > /tmp/out").unwrap();
        test!("parse: redirect", r.stdout_file == Some("/tmp/out".to_string()));
        test!("parse: redirect args", r.args == ["echo", "hello"]);

        let r = Shell::parse_line("cat < input.txt").unwrap();
        test!("parse: stdin", r.stdin_file == Some("input.txt".to_string()));

        let r = Shell::parse_line("cc -o main main.c &").unwrap();
        test!("parse: bg", r.background);
        test!("parse: bg args", r.args == ["cc", "-o", "main", "main.c"]);

        let r = Shell::parse_line("echo 'hello world'").unwrap();
        test!("parse: single quotes", r.args.len() == 2 && r.args[1] == "hello world");

        let r = Shell::parse_line("echo \"hello world\"").unwrap();
        test!("parse: double quotes", r.args.len() == 2 && r.args[1] == "hello world");

        test!("parse: unclosed quote", Shell::parse_line("echo \"hello").is_err());
        test!("parse: empty", Shell::parse_line("").is_err());
        test!("parse: whitespace", Shell::parse_line("   ").is_err());

        // ── normalize ──
        test!("norm: simple", Shell::normalize("/usr/bin") == "/usr/bin");
        test!("norm: trailing slash", Shell::normalize("/usr/bin/") == "/usr/bin");
        test!("norm: double slash", Shell::normalize("/usr//bin") == "/usr/bin");
        test!("norm: .. parent", Shell::normalize("/usr/bin/..") == "/usr");
        test!("norm: . current", Shell::normalize("/usr/./bin") == "/usr/bin");
        test!("norm: root ..", Shell::normalize("/..") == "/");
        test!("norm: root .", Shell::normalize("/.") == "/");
        test!("norm: complex", Shell::normalize("/usr/local/../bin/./gcc") == "/usr/bin/gcc");
        test!("norm: empty", Shell::normalize("") == "/");
        test!("norm: triple /", Shell::normalize("///usr//local///") == "/usr/local");

        // ── expand_vars ──
        let env = BTreeMap::from([
            ("HOME".to_string(), "/root".to_string()),
            ("USER".to_string(), "joe".to_string()),
        ]);
        test!("expand: simple", Shell::expand_vars("echo $HOME", &env, 0, 0) == "echo /root");
        test!("expand: two vars", Shell::expand_vars("$HOME/$USER", &env, 0, 0) == "/root/joe");
        test!("expand: dollar dollar", Shell::expand_vars("pid=$$", &env, 0, 42) == "pid=42");
        test!("expand: dollar quest", Shell::expand_vars("code=$?", &env, 5, 0) == "code=5");
        test!("expand: braces", Shell::expand_vars("${HOME}/x", &env, 0, 0) == "/root/x");
        test!("expand: unknown", Shell::expand_vars("$NOTHING", &env, 0, 0) == "");
        test!("expand: no var", Shell::expand_vars("hello world", &env, 0, 0) == "hello world");

        // ── find_builtin ──
        test!("dispatch: known", Self::find_builtin("ls").is_some());
        test!("dispatch: unknown", Self::find_builtin("foobar42").is_none());
        test!("dispatch: alias", Self::find_builtin("alias").is_some());
        test!("dispatch: bg", Self::find_builtin("bg").is_some());

        // ── Memory Compression Tests ──
        crate::memory::compression::tests::run_tests();

        let total = passed + failed;
        println!("  {}{}  {}/{} passed{}",
            if failed == 0 { C_GREEN } else { C_RED },
            C_BOLD, passed, total, C_RESET);
        if failed > 0 { 1 } else { 0 }
    }

    #[cfg(feature = "ziqafs")]
    fn cmd_diskinfo(&mut self, _args: &[String]) -> i32 {
        let guard = ZIQAFS.lock();
        if let Some(fs_arc) = guard.as_ref() {
            let mut fs = fs_arc.lock();
            let st = crate::fs::ziqafs::ZiqaFs::statfs(&fs);
            let total_kib = st.total_blocks as u64 * st.block_size as u64 / 1024;
            let free_kib  = st.free_blocks  as u64 * st.block_size as u64 / 1024;
            let used_kib  = total_kib - free_kib;
            let pct = used_kib.saturating_mul(100).checked_div(total_kib).unwrap_or(0) as usize;

            println!("{}{}  DISK (ZiqaFS){}", C_YELLOW, C_BOLD, C_RESET);
            println!("  Block size: {} B   Total: {} KiB   Used: {} KiB   Free: {} KiB",
                st.block_size, total_kib, used_kib, free_kib);
            println!("  Inodes: {}/{} used", st.total_inodes - st.free_inodes, st.total_inodes);
            println!("");

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
        0
    }

    fn cmd_lsblk(&mut self, _args: &[String]) -> i32 {
        let devices = crate::drivers::block_registry::BLOCK_DEVICES.lock();
        if devices.is_empty() {
            println!("No block devices found.");
            return 0;
        }
        println!("{}{}  BLOCK DEVICES {}", C_YELLOW, C_BOLD, C_RESET);
        println!("  ┌──────────┬──────────────┬──────────────┬──────────────┐");
        println!("  │ {}Name     │ {}Driver       │ {}Sectors      │ {}Size         {}│", C_CYAN, C_GREEN, C_CYAN, C_GREEN, C_RESET);
        println!("  ├──────────┼──────────────┼──────────────┼──────────────┤");
        for d in devices.iter() {
            let total_sec = d.device.total_sectors();
            let size_mb = total_sec * 512 / 1024 / 1024;
            println!("  │ /dev/{:<3} │ {:<12} │ {:>12} │ {:>8} MB   │",
                d.name, d.driver, total_sec, size_mb);
        }
        println!("  └──────────┴──────────────┴──────────────┴──────────────┘");
        0
    }

    fn cmd_blkinfo(&mut self, args: &[String]) -> i32 {
        if args.is_empty() {
            println!("Usage: blkinfo <device_name> (e.g. vda)");
            return 1;
        }
        let dev_name = args[0].trim_start_matches("/dev/");
        let devices = crate::drivers::block_registry::BLOCK_DEVICES.lock();
        let entry = match devices.iter().find(|d| d.name == dev_name) {
            Some(e) => e,
            None => {
                println!("Error: block device '/dev/{}' not found", dev_name);
                return 1;
            }
        };

        let total_sec = entry.device.total_sectors();
        let size_bytes = total_sec * 512;
        let size_mb = size_bytes / 1024 / 1024;

        println!("{}{}  DEVICE INFO: /dev/{} {}", C_YELLOW, C_BOLD, dev_name, C_RESET);
        println!("  Driver:       {}", entry.driver);
        println!("  Sectors:      {}", total_sec);
        println!("  Capacity:     {} MB ({} bytes)", size_mb, size_bytes);
        println!("  Sector Size:  512 bytes");

        let mut first_sector = [0u8; 512];
        if entry.device.read_sectors(0, 1, &mut first_sector).is_ok() {
            let boot_sig = u16::from_le_bytes([first_sector[510], first_sector[511]]);
            if boot_sig == 0xAA55 {
                println!("  Partition Table: MBR detected");
                for i in 0..4 {
                    let off = 446 + i * 16;
                    let p_type = first_sector[off + 4];
                    if p_type != 0 {
                        let start = u32::from_le_bytes([
                            first_sector[off + 8],
                            first_sector[off + 9],
                            first_sector[off + 10],
                            first_sector[off + 11],
                        ]);
                        let size = u32::from_le_bytes([
                            first_sector[off + 12],
                            first_sector[off + 13],
                            first_sector[off + 14],
                            first_sector[off + 15],
                        ]);
                        let type_str = match p_type {
                            0x0B | 0x0C => "FAT32",
                            0x83 => "Linux",
                            0x7F => "ZiqaFS",
                            _t => "Unknown",
                        };
                        println!("    Partition {}: Type=0x{:02X} ({}) Start={} Size={} ({} MB)",
                            i + 1, p_type, type_str, start, size, size as u64 * 512 / 1024 / 1024);
                    }
                }
            } else {
                println!("  Partition Table: None / Raw Disk (No 0xAA55 boot signature)");
            }
        } else {
            println!("  Partition Table: Could not read sector 0");
        }
        0
    }

    fn cmd_mount(&mut self, _args: &[String]) -> i32 {
        let mounts = crate::fs::vfs::MOUNT_REGISTRY.lock();
        if mounts.is_empty() {
            println!("No filesystems mounted.");
            return 0;
        }
        println!("{}{}  MOUNTED FILESYSTEMS {}", C_YELLOW, C_BOLD, C_RESET);
        println!("  ┌──────────────┬──────────────┬──────────────┐");
        println!("  │ {}Device         │ {}Mount Point  │ {}Type         {}│", C_CYAN, C_GREEN, C_CYAN, C_RESET);
        println!("  ├──────────────┼──────────────┼──────────────┤");
        for m in mounts.iter() {
            println!("  │ {:<12} │ {:<12} │ {:<12} │", m.source, m.target, m.fstype);
        }
        println!("  └──────────────┴──────────────┴──────────────┘");
        0
    }

    #[cfg(feature = "net")]
    fn cmd_netstat(&mut self, _args: &[String]) -> i32 {
        println!("{}{}  NETWORK INTERFACES {}", C_YELLOW, C_BOLD, C_RESET);
        println!("  ┌──────┬──────────────┬──────────────┬──────────────┬──────────────┐");
        println!("  │ {}Dev  │ {}TX pkts     │ {}RX pkts     │ {}TX bytes    │ {}RX bytes    {}│", C_CYAN, C_GREEN, C_CYAN, C_GREEN, C_CYAN, C_RESET);
        println!("  ├──────┼──────────────┼──────────────┼──────────────┼──────────────┤");
        {
            let guard = crate::net::NET.lock();
            for dev in guard.devices.iter().flatten() {
                println!("  │ {:<4} │ {:>12} │ {:>12} │ {:>12} │ {:>12} │",
                    dev.name, dev.tx_packets, dev.rx_packets, dev.tx_bytes, dev.rx_bytes);
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
        0
    }

    fn cmd_reboot(&mut self, _args: &[String]) -> i32 {
        println!("Rebooting...");
        unsafe {
            use x86_64::instructions::port::Port;
            let mut port: Port<u8> = Port::new(0x64);
            port.write(0xFE);
        }
        loop { x86_64::instructions::hlt(); }
    }

    fn cmd_dashboard(&mut self, _args: &[String]) -> i32 {
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
            x86_64::instructions::interrupts::without_interrupts(|| {
                SCHEDULER.list_pids().into_iter().filter_map(|pid| {
                    SCHEDULER.get_process(pid).map(|p_arc| {
                        let p = p_arc.lock();
                        (p.pid.0, p.state, p.priority)
                    })
                }).collect()
            })
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
        #[cfg(feature = "net")]
        {
            println!("\n\x1b[33m  ▸ NETWORK\x1b[0m");
            let guard = crate::net::NET.lock();
            for dev in guard.devices.iter().flatten() {
                println!("    {:<6}  TX: {:>6} pkts {:>8} B   RX: {:>6} pkts {:>8} B",
                    dev.name, dev.tx_packets, dev.tx_bytes, dev.rx_packets, dev.rx_bytes);
            }
        }

        // ── Storage ──
        println!("\n\x1b[33m  ▸ STORAGE\x1b[0m");
        #[cfg(feature = "ziqafs")]
        {
            use crate::fs::ziqafs::ZIQAFS;
            let guard = ZIQAFS.lock();
            if let Some(fs_arc) = guard.as_ref() {
                let fs = fs_arc.lock();
                let st = crate::fs::ziqafs::ZiqaFs::statfs(&fs);
                let total_kib = st.total_blocks as u64 * st.block_size as u64 / 1024;
                let free_kib  = st.free_blocks  as u64 * st.block_size as u64 / 1024;
                let used_kib  = total_kib - free_kib;
                let dpct = used_kib.saturating_mul(100).checked_div(total_kib).unwrap_or(0) as usize;
                let dfilled = dpct * 30 / 100;
                let dc = if dpct > 80 { "\x1b[31m" } else if dpct > 50 { "\x1b[33m" } else { "\x1b[32m" };
                print!("    ZiqaFS   [");
                for i in 0..30usize { if i < dfilled { print!("{}█\x1b[0m", dc); } else { print!("░"); } }
                println!("] {}%  ({}/{} KiB)", dpct, used_kib, total_kib);
                println!("    Inodes:  {}/{} used", st.total_inodes - st.free_inodes, st.total_inodes);
            } else {
                println!("    ZiqaFS: \x1b[2mnot mounted\x1b[0m");
            }
        }
        #[cfg(not(feature = "ziqafs"))]
        {
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
        0
    }

    #[cfg(feature = "games")]
    fn cmd_doom(&mut self, args: &[String]) -> i32 {
        let steps: usize = args.first().and_then(|s| s.parse().ok()).unwrap_or(60);
        crate::doom::run(steps);
        0
    }

    #[cfg(feature = "games")]
    fn cmd_nwm_test(&mut self, _args: &[String]) -> i32 {
        println!("{}  Launching NWM desktop demo...{}", C_CYAN, C_RESET);
        crate::userspace::nwm_demo::run();
        println!("{}  NWM desktop exited.{}", C_GREEN, C_RESET);
        0
    }

    fn cmd_clear(&mut self, _args: &[String]) -> i32 {
        crate::drivers::vga::clear_screen();
        if crate::drivers::vga::is_scrolled() {
            crate::drivers::vga::restore_terminal();
        }
        self.update_status_bar();
        use core::fmt::Write;
        let mut serial = crate::drivers::uart::SERIAL1.lock();
        write!(serial, "\x1b[2J\x1b[H").ok();
        0
    }

    #[cfg(feature = "games")]
    fn cmd_tetris(&mut self, _args: &[String]) -> i32 {
        crate::tetris::run();
        0
    }

    fn cmd_edit(&mut self, args: &[String]) -> i32 {
        let p = match args.first().map(|s| s.as_str()) {
            Some(s) => s,
            None => { println!("Usage: edit <path>"); return 1; }
        };
        let resolved = self.resolve_path(p);
        crate::edit::edit_file(&resolved);
        0
    }

    fn cmd_ls(&mut self, args: &[String]) -> i32 {
        let target = args.first().map(|s| s.as_str());
        let dir = target.map(|p| self.resolve_path(p)).unwrap_or_else(|| self.cwd_str().to_string());
        let vfs = VFS.read();
        if !vfs.is_dir(&dir) {
            println!("{}ls{}: {}: {}No such directory{}", C_RED, C_RESET, dir, C_DIM, C_RESET);
            return 1;
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
        0
    }

    fn cmd_cd(&mut self, args: &[String]) -> i32 {
        // Flag handling: '-v' prints the new cwd after changing directory.
        let mut verbose = false;
        let mut path_opt: Option<String> = None;
        for arg in args {
            if arg == "-v" {
                verbose = true;
            } else if path_opt.is_none() {
                path_opt = Some(arg.clone());
            }
        }
        let raw = path_opt.as_deref().unwrap_or("/");
        let resolved = if raw == "-" {
            if self.prev_cwd.is_empty() {
                println!("{}cd{}: {}no previous directory{}", C_RED, C_RESET, C_DIM, C_RESET);
                return 1;
            }
            self.prev_cwd.clone()
        } else {
            self.resolve_path(raw)
        };
        let vfs = VFS.read();
        if !vfs.is_dir(&resolved) {
            println!("{}cd{}: {}: {}No such directory{}", C_RED, C_RESET, resolved, C_DIM, C_RESET);
            return 1;
        }
        self.prev_cwd = self.cwd.clone();
        self.cwd = resolved;
        // Default feedback (arrow). Keep for compatibility.
        println!("{}▸ {}{}", C_GREEN, self.cwd_str(), C_RESET);
        if verbose {
            // Additional explicit cwd output (same as `pwd`).
            println!("{}", self.cwd_str());
        }
        0
    }

    fn cmd_pwd(&mut self, _args: &[String]) -> i32 {
        println!("{}", self.cwd_str());
        0
    }

    fn cmd_mkdir(&mut self, args: &[String]) -> i32 {
        let p = match args.first().map(|s| s.as_str()) {
            Some(s) => s,
            None => { println!("Usage: mkdir <path>"); return 1; }
        };
        let resolved = self.resolve_path(p);
        let mut vfs = VFS.write();
        if vfs.exists(&resolved) {
            println!("mkdir: {}: File exists", resolved);
            return 1;
        }
        if resolved.starts_with("/disk/") {
            #[cfg(feature = "ziqafs")]
            {
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
                            return 0;
                        }
                        Err(e) => {
                            println!("mkdir: {}: {:?}", resolved, e);
                            return 1;
                        }
                    }
                }
            }
            #[cfg(not(feature = "ziqafs"))]
            {
                println!("mkdir: /disk/ requires ZiqaFS feature");
                return 1;
            }
        }
        vfs.mkdir(&resolved);
        println!("mkdir: created {}", resolved);
        0
    }

    fn cmd_dir(&mut self, args: &[String]) -> i32 {
        let target = args.first().map(|s| s.as_str());
        let dir = target.map(|p| self.resolve_path(p)).unwrap_or_else(|| self.cwd_str().to_string());
        let vfs = VFS.read();
        if !vfs.is_dir(&dir) {
            println!("dir: {}: No such directory", dir);
            return 1;
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
        0
    }

    fn cmd_rm(&mut self, args: &[String]) -> i32 {
        let p = match args.first().map(|s| s.as_str()) {
            Some(s) => s,
            None => { println!("Usage: rm <path>"); return 1; }
        };
        let resolved = self.resolve_path(p);
        if resolved.starts_with("/disk/") {
            #[cfg(feature = "ziqafs")]
            {
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
        }
        match VFS.write().remove(&resolved) {
            Ok(_) => println!("rm: removed {}", resolved),
            Err(_) => println!("rm: {}: No such file", resolved),
        }
        0
    }

    fn cmd_cat(&mut self, args: &[String]) -> i32 {
        let p = match args.first().map(|s| s.as_str()) {
            Some(s) => s,
            None => { println!("Usage: cat [-n] <path>"); return 1; }
        };
        let (line_numbers, path) = if p == "-n" {
            println!("Usage: cat [-n] <path>"); return 1;
        } else if let Some(rest) = p.strip_prefix("-n ") {
            (true, rest)
        } else {
            (false, p)
        };
        let resolved = self.resolve_path(path);
        let mut buf = [0u8; 65536];
        match VFS.read().read_raw(&resolved, &mut buf, 0) {
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
        0
    }

    #[cfg(feature = "net")]
    #[cfg(feature = "net")]
    fn cmd_ping(&mut self, args: &[String]) -> i32 {
        let joined = args.join(" ");
        let mut count: usize = 4;
        let mut host = "";
        let parts: alloc::vec::Vec<&str> = joined.split_whitespace().collect();
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
            println!("  Note: QEMU user networking (SLIRP) does not forward ICMP to external hosts.");
            println!("  Use 'ping 10.0.2.2' for gateway, or 'wget' for TCP connectivity test.");
            return 1;
        }

        let ip = match crate::net::dns::resolve(host) {
            Some(ip) => ip,
            None => {
                println!("ping: {}: Name or service not known", host);
                println!("  Hint: use an IPv4 address (e.g. 10.0.2.2 for gateway)");
                return 1;
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
                    if let Ok(payload) = socket.send(repr.buffer_len(), IpAddress::Ipv4(ip)) {
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
                        if socket.recv().is_ok() {
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
                    if crate::drivers::keyboard::check_and_clear_interrupt() {
                        println!("^C");
                        return 130;
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
                        if crate::drivers::keyboard::check_and_clear_interrupt() {
                            println!("^C");
                            return 130;
                        }
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
        0
    }

    #[cfg(feature = "net")]
    fn cmd_wget(&mut self, args: &[String]) -> i32 {
        let joined = args.join(" ");
        let mut output_name: Option<&str> = None;
        let mut url_arg = "";
        let parts: alloc::vec::Vec<&str> = joined.split_whitespace().collect();
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
            println!("  Note: QEMU user networking (SLIRP) has limited DNS/ICMP support.");
            println!("  Use IP literals (e.g., wget http://10.0.2.2/) for best results.");
            return 1;
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
                return 1;
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
                Err(e) => { println!("wget: {}", e); return 1; }
            }
        }

        let response = match final_response {
            Some(r) => r,
            None => { println!("wget: too many redirects"); return 1; }
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
            return 1;
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
        VFS.write().mount(&filepath, file);
        println!("Saved to '{}' [{} bytes]", filepath, response.body.len());
        0
    }

    #[cfg(feature = "net")]
    fn cmd_ifconfig(&mut self, _args: &[String]) -> i32 {
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
        0
    }

    fn cmd_mv(&mut self, args: &[String]) -> i32 {
        let (src, dst) = match (args.first(), args.get(1)) {
            (Some(s), Some(d)) => (s.as_str(), d.as_str()),
            _ => { println!("Usage: mv <src> <dst>"); return 1; }
        };
        let src_path = self.resolve_path(src);
        let dst_path = self.resolve_path(dst);
        match VFS.write().rename(&src_path, &dst_path) {
            Ok(_) => println!("mv: {} -> {}", src_path, dst_path),
            Err(_) => println!("mv: {}: No such file", src_path),
        }
        0
    }

    #[cfg(feature = "ziqafs")]
    fn cmd_cp(&mut self, args: &[String]) -> i32 {
        let (src, dst) = match (args.first(), args.get(1)) {
            (Some(s), Some(d)) => (s.as_str(), d.as_str()),
            _ => { println!("Usage: cp <src> <dst>"); return 1; }
        };
        let src_path = self.resolve_path(src);
        let dst_path = self.resolve_path(dst);
        let guard = ZIQAFS.lock();
        if let Some(fs_arc) = guard.as_ref() {
            let mut fs = fs_arc.lock();
            let src_id = match ZiqaFs::root_lookup(&mut fs, &src_path) {
                Ok(id) => id,
                Err(_) => { println!("cp: {}: No such file", src_path); return 1; }
            };
            let dst_name = dst_path.rsplit('/').next().unwrap_or(&dst_path);
            match ZiqaFs::copy_file(&mut fs, src_id, ROOT_INODE, dst_name) {
                Ok(_) => println!("cp: {} -> {}", src_path, dst_path),
                Err(e) => println!("cp: {:?}", e),
            }
        } else {
            println!("cp: ZiqaFS not mounted");
        }
        0
    }

    fn cmd_touch(&mut self, args: &[String]) -> i32 {
        let p = match args.first() {
            Some(s) => s.as_str(),
            None => { println!("Usage: touch <path>"); return 1; }
        };
        let resolved = self.resolve_path(p);
        let mut vfs = VFS.write();
        if !vfs.exists(&resolved) {
            vfs.create(&resolved);
            println!("touch: created {}", resolved);
        } else {
            let _ = vfs.write_raw(&resolved, &[], 0);
            println!("touch: updated {}", resolved);
        }
        0
    }

    fn cmd_writefile(&mut self, args: &[String]) -> i32 {
        let path = match args.first() {
            Some(p) => p.as_str(),
            None => { println!("Usage: writefile <path> <text>"); return 1; }
        };
        if args.len() < 2 {
            println!("Usage: writefile <path> <text>");
            return 1;
        }
        let text = args[1..].join(" ");
        let resolved = self.resolve_path(path);
        let mut vfs = VFS.write();
        if !vfs.exists(&resolved) {
            vfs.create(&resolved);
        }
        match vfs.write_raw(&resolved, text.as_bytes(), 0) {
            Ok(n) => println!("writefile: wrote {} bytes to {}", n, resolved),
            Err(e) => println!("writefile: failed: {:?}", e),
        }
        0
    }

    #[cfg(feature = "ziqafs")]
    fn cmd_stat(&mut self, args: &[String]) -> i32 {
        let p = match args.first() {
            Some(s) => s.as_str(),
            None => { println!("Usage: stat <path>"); return 1; }
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
            let vfs = VFS.read();
            if let Some(size) = vfs.file_size(&resolved) {
                println!("  File:  {}", resolved);
                println!("  Size:  {} bytes", size);
            } else {
                println!("stat: {}: No such file", resolved);
            }
        }
        0
    }

    #[cfg(feature = "ziqafs")]
    fn cmd_du(&mut self, args: &[String]) -> i32 {
        let p = args.first().map(|s| self.resolve_path(s)).unwrap_or_else(|| self.cwd_str().to_string());
        let guard = ZIQAFS.lock();
        if let Some(fs_arc) = guard.as_ref() {
            let mut fs = fs_arc.lock();
            if let Ok(inode_id) = ZiqaFs::root_lookup(&mut fs, &p) {
                let blocks = ZiqaFs::du(&mut fs, inode_id);
                println!("{}\t{} ({} KiB)", blocks, p, blocks as usize * BLOCK_SIZE / 1024);
                return 0;
            }
        }

        let vfs = VFS.read();
        if vfs.is_dir(&p) {
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
        0
    }

    fn cmd_echo(&mut self, args: &[String]) -> i32 {
        println!("{}", args.join(" "));
        0
    }

    fn cmd_rmdir(&mut self, args: &[String]) -> i32 {
        self.cmd_rm(args)
    }

    fn cmd_jobs(&mut self, _args: &[String]) -> i32 {
        if self.jobs.is_empty() {
            println!("No background jobs.");
            return 0;
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
        0
    }

    fn cmd_bg(&mut self, args: &[String]) -> i32 {
        let job_num = match args.first().and_then(|s| s.parse::<usize>().ok()) {
            Some(n) if n > 0 && n <= self.jobs.len() => n - 1,
            _ => {
                println!("Usage: bg <%job-number>");
                return 1;
            }
        };

        if self.jobs[job_num].state == JobState::Stopped {
            let pid = self.jobs[job_num].pid;
            let cmd = self.jobs[job_num].command.clone();
            crate::process::scheduler::SCHEDULER
                .send_signal(pid, crate::process::signal::sig::SIGCONT);
            self.jobs[job_num].state = JobState::Running;
            println!("{} [{}] &", cmd, job_num + 1);
        } else {
            println!("bg: job {} is not stopped", job_num + 1);
        }
        0
    }

    fn cmd_fg(&mut self, args: &[String]) -> i32 {
        let job_num = match args.first().and_then(|s| s.parse::<usize>().ok()) {
            Some(n) if n > 0 && n <= self.jobs.len() => n - 1,
            _ => {
                println!("Usage: fg <%job-number>");
                return 1;
            }
        };

        let job_state = self.jobs[job_num].state;
        if job_state == JobState::Stopped || job_state == JobState::Running {
            self.fg_job = Some(job_num);
            if job_state == JobState::Stopped {
                let pid = self.jobs[job_num].pid;
                crate::process::scheduler::SCHEDULER
                    .send_signal(pid, crate::process::signal::sig::SIGCONT);
            }
            self.jobs[job_num].state = JobState::Running;
            self.fg_job = None;
        } else {
            println!("fg: job {} is not in a valid state", job_num + 1);
        }
        0
    }

    fn cmd_compress(&mut self, args: &[String]) -> i32 {
        let max_pages: usize = args.first()
            .and_then(|s| s.parse().ok())
            .unwrap_or(256);
        let t0 = unsafe { core::arch::x86_64::_rdtsc() };
        let compressed = crate::memory::compression::daemon::run_daemon_cycle(max_pages);
        let t1 = unsafe { core::arch::x86_64::_rdtsc() };
        let elapsed = t1.wrapping_sub(t0);
        println!("Compressed {} pages in {} cycles ({:.0} cyc/page)",
            compressed, elapsed,
            if compressed > 0 { elapsed as f64 / compressed as f64 } else { 0.0 });
        println!("{}", crate::memory::compression::daemon::daemon_status());
        0
    }

    fn cmd_snap(&mut self, args: &[String]) -> i32 {
        if args.is_empty() {
            println!("Usage: snap <pid>        - Create a snapshot of process PID");
            println!("       snap restore <pid> - Restore process from snapshot PID");
            return 1;
        }

        if args[0] == "restore" {
            if args.len() < 2 { println!("Usage: snap restore <pid>"); return 1; }
            let pid_val = match args[1].parse::<u64>() {
                Ok(v) => v,
                Err(_) => { println!("Invalid PID: {}", args[1]); return 1; }
            };

            let dummy_binary = include_bytes!("../assets/test_elf.bin");
            if let Some(pid) = crate::process::scheduler::spawn_elf(dummy_binary) {
                println!("[Snapshot] Spawning target process PID={}...", pid.0);
                let success = crate::process::scheduler::with_process_mut(pid, |proc| {
                    crate::process::snapshot::SnapshotManager::load(pid_val, proc)
                }).unwrap_or(false);

                if success {
                    println!("[Snapshot] Successfully restored state from snapshot {} into process {}", pid_val, pid.0);
                    0
                } else {
                    println!("[Snapshot] Failed to restore from snapshot {}", pid_val);
                    crate::process::scheduler::SCHEDULER.send_signal(pid, 9);
                    1
                }
            } else {
                println!("Failed to spawn target process for restoration.");
                1
            }
        } else {
            let pid_val = match args[0].parse::<u64>() {
                Ok(v) => v,
                Err(_) => { println!("Invalid PID: {}", args[0]); return 1; }
            };
            let pid = crate::process::Pid(pid_val);

            let success = crate::process::scheduler::with_process(pid, |proc| {
                crate::process::snapshot::SnapshotManager::save(proc)
            }).unwrap_or(false);

            if success { 0 } else { 1 }
        }
    }

    fn cmd_ls_snap(&mut self, _args: &[String]) -> i32 {
        let pids = crate::process::snapshot::SnapshotManager::list();
        println!("{}{}  SAVED SNAPSHOTS  ({} total){}", C_YELLOW, C_BOLD, pids.len(), C_RESET);
        if pids.is_empty() {
            println!("  (none)");
        } else {
            for pid in pids {
                let path = alloc::format!("/fat/snapshots/{}.snap", pid);
                let size = VFS.read().file_size(&path).unwrap_or(0);
                println!("  PID {:>4}  ({} bytes)", pid, size);
            }
        }
        0
    }

    fn cmd_rm_snap(&mut self, args: &[String]) -> i32 {
        let pid_val = match args.first().and_then(|s| s.parse::<u64>().ok()) {
            Some(v) => v,
            None => { println!("Usage: rm-snap <pid>"); return 1; }
        };
        if crate::process::snapshot::SnapshotManager::delete(pid_val) {
            0
        } else {
            println!("Failed to delete snapshot for {}", pid_val);
            1
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
