
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

pub(crate) const COMMANDS: &[&str] = &[
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

pub(crate) struct SyscallEntry {
    nr: u64,
    name: &'static str,
    category: &'static str,
    args: &'static str,
    desc: &'static str,
    probe: bool,
}

pub(crate) const SYSCALLS: &[SyscallEntry] = &[
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
pub(crate) const C_RESET: &str = "\x1b[0m";
pub(crate) const C_BOLD: &str = "\x1b[1m";
pub(crate) const C_DIM: &str = "\x1b[2m";
pub(crate) const C_RED: &str = "\x1b[31m";
pub(crate) const C_GREEN: &str = "\x1b[32m";
pub(crate) const C_YELLOW: &str = "\x1b[33m";
pub(crate) const C_BLUE: &str = "\x1b[34m";
pub(crate) const C_MAGENTA: &str = "\x1b[35m";
pub(crate) const C_CYAN: &str = "\x1b[36m";
#[allow(dead_code)]
pub(crate) const C_WHITE: &str = "\x1b[37m";

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

    pub(crate) fn cwd_str(&self) -> &str {
        if self.cwd.is_empty() { "/" } else { &self.cwd }
    }

    pub(crate) fn resolve_path(&self, path: &str) -> String {
        if path.is_empty() {
            return self.cwd_str().to_string();
        }
        let cwd_bytes = self.cwd_str().as_bytes();
        crate::fs::resolve_path(cwd_bytes, cwd_bytes.len(), path)
    }

    pub(crate) fn normalize(path: &str) -> String {
        crate::fs::normalize_path(path)
    }

    pub(crate) fn skip_ws(input: &str, pos: &mut usize) {
        while *pos < input.len() && input.as_bytes()[*pos].is_ascii_whitespace() {
            *pos += 1;
        }
    }

    pub(crate) fn read_word<'a>(input: &'a str, pos: &mut usize) -> Result<String, &'a str> {
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

    pub(crate) fn expand_vars(s: &str, env: &BTreeMap<String, String>, last_exit: i32, shell_pid: u64) -> String {
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

    pub(crate) fn parse_line(input: &str) -> Result<ParsedCmd, &str> {
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

    pub(crate) fn find_builtin(name: &str) -> Option<BuiltinFn> {
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

    pub(crate) fn poll_jobs(&mut self) {
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

    pub(crate) fn execute_cmd(&mut self, parsed: &ParsedCmd) -> i32 {
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


}

mod commands;
mod line_editor;


impl Shell {

pub(crate) fn levenshtein_distance(a: &str, b: &str) -> usize {
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

pub(crate) fn longest_common_prefix(strings: &[String]) -> String {
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
