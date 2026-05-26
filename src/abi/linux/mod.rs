/// Linux ABI Plugin for ZiqaKernel
///
/// This plugin allows ZiqaKernel to run standard Linux ELF binaries.
/// It implements the Linux x86_64 syscall ABI (syscall numbers, argument
/// passing convention) and routes each syscall to its handler.
///
/// Supported syscalls (initial set for busybox/bash):
///   - sys_write (1)    — write to fd
///   - sys_exit (60)    — terminate process
///   - sys_brk (12)     — adjust program break
///   - sys_mmap (9)     — map memory  
///   - sys_read (0)     — read from fd
///   - sys_close (3)    — close fd
///   - sys_uname (63)   — system identification

pub mod elf_loader;

use crate::abi::{AbiPlugin, AbiError};
use crate::abi::syscall::SyscallContext;
use crate::process::{Process, AbiKind};
use crate::println;
use crate::capability::ResourceKind;

/// Linux x86_64 syscall numbers
mod nr {
    pub const SYS_READ: u64 = 0;
    pub const SYS_WRITE: u64 = 1;
    pub const SYS_OPEN: u64 = 2;
    pub const SYS_CLOSE: u64 = 3;
    pub const SYS_STAT: u64 = 4;
    pub const SYS_FSTAT: u64 = 5;
    pub const SYS_LSTAT: u64 = 6;
    pub const SYS_LSEEK: u64 = 8;
    pub const SYS_MMAP: u64 = 9;
    pub const SYS_MPROTECT: u64 = 10;
    pub const SYS_MUNMAP: u64 = 11;
    pub const SYS_BRK: u64 = 12;
    pub const SYS_IOCTL: u64 = 16;
    pub const SYS_WRITEV: u64 = 20;
    pub const SYS_ACCESS: u64 = 21;
    pub const SYS_PIPE: u64 = 22;
    pub const SYS_DUP: u64 = 32;
    pub const SYS_DUP2: u64 = 33;
    pub const SYS_GETPID: u64 = 39;
    pub const SYS_KILL: u64 = 62;
    pub const SYS_UNAME: u64 = 63;
    pub const SYS_GETCWD: u64 = 79;
    pub const SYS_CHDIR: u64 = 80;
    pub const SYS_GETUID: u64 = 102;
    pub const SYS_GETGID: u64 = 104;
    pub const SYS_GETEUID: u64 = 107;
    pub const SYS_GETEGID: u64 = 108;
    pub const SYS_WAITPID: u64 = 114; // sys_wait4 on x86_64
    pub const SYS_NANOSLEEP: u64 = 35;
    pub const SYS_ARCH_PRCTL: u64 = 158;
    pub const SYS_SET_TID_ADDRESS: u64 = 218;
    pub const SYS_EXIT_GROUP: u64 = 231;
    pub const SYS_EXIT: u64 = 60;
}

/// The Linux ABI plugin instance
pub struct LinuxAbiPlugin;

/// Static instance so it can be registered in the ABI registry
pub static LINUX_PLUGIN: LinuxAbiPlugin = LinuxAbiPlugin;

impl AbiPlugin for LinuxAbiPlugin {
    fn name(&self) -> &'static str {
        "Linux x86_64 ELF"
    }

    fn kind(&self) -> AbiKind {
        AbiKind::LinuxElf
    }

    fn can_load(&self, binary: &[u8]) -> bool {
        // ELF magic: 0x7F 'E' 'L' 'F'
        binary.len() >= 4
            && binary[0] == 0x7F
            && binary[1] == b'E'
            && binary[2] == b'L'
            && binary[3] == b'F'
    }

    fn load(&self, binary: &[u8], process: &mut Process) -> Result<(), AbiError> {
        elf_loader::load_elf(binary, process)
    }

    fn handle_syscall(&self, ctx: &mut SyscallContext) -> Result<u64, AbiError> {
        match ctx.number {
            nr::SYS_WRITE => sys_write(ctx),
            nr::SYS_READ => sys_read(ctx),
            nr::SYS_EXIT | nr::SYS_EXIT_GROUP => sys_exit(ctx),
            nr::SYS_BRK => sys_brk(ctx),
            nr::SYS_GETPID => sys_getpid(ctx),
            nr::SYS_KILL => sys_kill(ctx),
            nr::SYS_WAITPID => sys_waitpid(ctx),
            nr::SYS_NANOSLEEP => sys_nanosleep(ctx),
            nr::SYS_UNAME => sys_uname(ctx),
            nr::SYS_MMAP => sys_mmap(ctx),
            nr::SYS_MPROTECT => sys_mprotect(ctx),
            nr::SYS_MUNMAP => sys_munmap(ctx),
            nr::SYS_CLOSE => sys_close(ctx),
            nr::SYS_FSTAT => sys_fstat(ctx),
            nr::SYS_IOCTL => sys_ioctl(ctx),
            nr::SYS_WRITEV => sys_writev(ctx),
            nr::SYS_ACCESS => sys_access(ctx),
            nr::SYS_OPEN => sys_open(ctx),
            nr::SYS_ARCH_PRCTL => sys_arch_prctl(ctx),
            nr::SYS_SET_TID_ADDRESS => sys_set_tid_address(ctx),
            nr::SYS_STAT | nr::SYS_LSTAT => sys_stat(ctx),
            nr::SYS_LSEEK => sys_lseek(ctx),
            nr::SYS_DUP => sys_dup(ctx),
            nr::SYS_DUP2 => sys_dup2(ctx),
            nr::SYS_PIPE => sys_pipe(ctx),
            nr::SYS_GETCWD => sys_getcwd(ctx),
            nr::SYS_CHDIR => sys_chdir(ctx),
            nr::SYS_GETUID | nr::SYS_GETGID => Ok(0),   // root
            nr::SYS_GETEUID | nr::SYS_GETEGID => Ok(0), // root
            unknown => {
                println!("[Linux ABI] Unimplemented syscall: {}", unknown);
                Err(AbiError::UnsupportedSyscall(unknown))
            }
        }
    }
}

// ──────────────────────────────────────────────────────────
// Syscall implementations
// ──────────────────────────────────────────────────────────



/// sys_write(fd, buf, count) → bytes_written
fn sys_write(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let fd = ctx.args[0];
    let buf_addr = ctx.args[1];
    let count = ctx.args[2];

    // Capability check: does the process have write access to this File descriptor?
    // In our capability system, `target` is the FD number.
    if !ctx.process.capabilities.has_permission(ResourceKind::File, true, false) {
        println!("[Linux ABI] Security violation: process {} attempted write without File capability", ctx.process.pid.0);
        return Err(AbiError::PermissionDenied);
    }

    // For now, only support stdout (fd=1) and stderr (fd=2) via serial
    match fd {
        1 | 2 => {
            println!("[Linux write fd={}] {} bytes from 0x{:x}", fd, count, buf_addr);
            Ok(count)
        }
        _ => {
            println!("[Linux ABI] write to unknown fd {}", fd);
            Ok((-9_i64) as u64) // -EBADF
        }
    }
}

/// sys_read(fd, buf, count) → bytes_read  
fn sys_read(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let fd = ctx.args[0];
    let buf_addr = ctx.args[1];
    let count = ctx.args[2] as usize;
    match fd {
        0 => {
            // stdin — drain keyboard ring buffer
            // We can't safely write to user-space memory yet, so we report
            // how many bytes are available (non-zero = data ready).
            let _ = buf_addr; // would copy to user buf in a real impl
            let mut tmp = [0u8; 256];
            let n = crate::drivers::keyboard::read_stdin(&mut tmp[..count.min(256)]);
            Ok(n as u64)
        }
        _ => Ok((-9_i64) as u64), // -EBADF
    }
}

/// sys_exit(status) → never returns
fn sys_exit(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let status = ctx.args[0] as i64;
    println!("[Linux ABI] Process {} exiting with code {}", ctx.process.pid.0, status);
    ctx.process.exit(status);
    Ok(0)
}

/// sys_brk(new_brk) → current_brk
fn sys_brk(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let new_brk = ctx.args[0];
    // Simple stub: just acknowledge the request
    // A real implementation would adjust the process's heap
    if new_brk == 0 {
        // Query current break — return a reasonable default
        Ok(0x0040_0000) // 4MB mark
    } else {
        // Set new break — just accept it for now
        Ok(new_brk)
    }
}

/// sys_getpid() → pid
fn sys_getpid(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    Ok(ctx.process.pid.0)
}

/// sys_uname(buf) → 0
fn sys_uname(_ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    // Would write a utsname struct to the buffer address
    // For now, just return success
    println!("[Linux ABI] uname() → ZiqaKernel");
    Ok(0)
}

/// sys_mmap(addr, length, prot, flags, fd, offset) → mapped_addr
fn sys_mmap(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let addr = ctx.args[0];
    let length = ctx.args[1];
    let _prot = ctx.args[2];
    let _flags = ctx.args[3];
    let _fd = ctx.args[4];
    let _offset = ctx.args[5];

    println!("[Linux ABI] mmap(0x{:x}, {}) → stub", addr, length);
    // Return a fake mapped address (would need real page table manipulation)
    if addr != 0 {
        Ok(addr)
    } else {
        Ok(0x7000_0000) // arbitrary user-space address
    }
}

/// sys_mprotect(addr, len, prot) → 0
fn sys_mprotect(_ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    Ok(0) // stub: pretend success
}

/// sys_munmap(addr, length) → 0
fn sys_munmap(_ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    Ok(0) // stub
}

/// sys_close(fd) → 0
fn sys_close(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let fd = ctx.args[0];
    println!("[Linux ABI] close(fd={})", fd);
    Ok(0)
}

/// sys_fstat(fd, statbuf) → 0
fn sys_fstat(_ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    Ok(0) // stub
}

/// sys_ioctl(fd, request, arg) → 0/-ENOTTY
fn sys_ioctl(_ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    // Return -ENOTTY for now (not a typewriter)
    Ok((-25_i64) as u64)
}

/// sys_writev(fd, iov, iovcnt) → bytes_written
fn sys_writev(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let fd = ctx.args[0];
    let _iov = ctx.args[1];
    let iovcnt = ctx.args[2];
    println!("[Linux ABI] writev(fd={}, iovcnt={})", fd, iovcnt);
    Ok(0) // stub
}

/// sys_access(pathname, mode) → 0/-ENOENT
fn sys_access(_ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    // Return -ENOENT (file not found) since we have no filesystem yet
    Ok((-2_i64) as u64)
}

/// sys_open(pathname, flags, mode) → fd/-ENOENT
fn sys_open(_ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    Ok((-2_i64) as u64) // -ENOENT
}

/// sys_arch_prctl(code, addr) → 0  
fn sys_arch_prctl(_ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    // Used to set FS/GS base for TLS — stub for now
    Ok(0)
}

/// sys_set_tid_address(tidptr) → tid
fn sys_set_tid_address(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    Ok(ctx.process.pid.0)
}

/// sys_stat / sys_lstat(path, statbuf) → 0/-ENOENT
fn sys_stat(_ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    Ok((-2_i64) as u64) // -ENOENT (no VFS backing yet)
}

/// sys_lseek(fd, offset, whence) → new_offset
fn sys_lseek(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let _fd = ctx.args[0];
    let offset = ctx.args[1] as i64;
    let whence = ctx.args[2];
    // Stub: SEEK_SET=0 → return offset, others → 0
    match whence {
        0 => Ok(offset as u64),
        _ => Ok(0),
    }
}

/// sys_dup(oldfd) → newfd
fn sys_dup(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let oldfd = ctx.args[0];
    // Stub: return oldfd + 10 as a fake new fd
    Ok(oldfd + 10)
}

/// sys_dup2(oldfd, newfd) → newfd
fn sys_dup2(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let newfd = ctx.args[1];
    Ok(newfd)
}

/// sys_pipe(pipefd[2]) → 0
fn sys_pipe(_ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    // Would create an IPC channel and write two fds to user memory.
    // Stub: create a channel and return success.
    let _chan = crate::ipc::create_channel();
    Ok(0)
}

/// sys_getcwd(buf, size) → buf_addr (stub returns "/")
fn sys_getcwd(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let buf_addr = ctx.args[0];
    // Would write "/" to user buf; return buf pointer on success
    Ok(buf_addr)
}

/// sys_chdir(path) → 0 (stub, no real CWD tracking yet)
fn sys_chdir(_ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    Ok(0)
}

/// sys_kill(pid, sig) → 0/-ESRCH
fn sys_kill(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let target_pid = ctx.args[0];
    let signum = ctx.args[1] as u8;
    let ok = crate::process::scheduler::SCHEDULER
        .lock()
        .send_signal(crate::process::Pid(target_pid), signum);
    if ok {
        println!("[Linux ABI] kill(pid={}, sig={}) → 0", target_pid, signum);
        Ok(0)
    } else {
        Ok((-3_i64) as u64) // -ESRCH: no such process
    }
}

/// sys_wait4(pid, wstatus, options, rusage) → child_pid / -ECHILD
/// We map sys_waitpid (114) here; args[0]=pid, args[1]=wstatus_ptr.
fn sys_waitpid(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let child_pid = ctx.args[0] as i64;
    let parent = ctx.process.pid;
    match crate::process::scheduler::SCHEDULER
        .lock()
        .waitpid(parent, child_pid)
    {
        Some((pid, code)) => {
            println!("[Linux ABI] waitpid → child {} exited with {}", pid.0, code);
            Ok(pid.0)
        }
        None => Ok((-10_i64) as u64), // -ECHILD
    }
}

/// sys_nanosleep(req, rem) → 0 / -EINTR
/// req is a *timespec { tv_sec: u64, tv_nsec: u64 } in user memory.
/// We can't safely dereference user pointers yet, so we use arg[0] as raw ns.
fn sys_nanosleep(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    // Treat arg[0] as nanoseconds directly (simplified; real impl reads timespec)
    let ns = ctx.args[0].max(1_000_000); // at least 1 ms
    let pid = ctx.process.pid;
    crate::timer::sleep_ms(pid, ns / 1_000_000);
    println!("[Linux ABI] nanosleep(pid={}, ns={}) → blocked", pid.0, ns);
    Ok(0)
}
