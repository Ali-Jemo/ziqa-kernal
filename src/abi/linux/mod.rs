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
    pub const SYS_POLL: u64 = 7;
    pub const SYS_LSEEK: u64 = 8;
    pub const SYS_MMAP: u64 = 9;
    pub const SYS_MPROTECT: u64 = 10;
    pub const SYS_MUNMAP: u64 = 11;
    pub const SYS_BRK: u64 = 12;
    pub const SYS_RT_SIGACTION: u64 = 13;
    pub const SYS_RT_SIGPROCMASK: u64 = 14;
    pub const SYS_IOCTL: u64 = 16;
    pub const SYS_PREAD64: u64 = 17;
    pub const SYS_READV: u64 = 19;
    pub const SYS_WRITEV: u64 = 20;
    pub const SYS_ACCESS: u64 = 21;
    pub const SYS_PIPE: u64 = 22;
    pub const SYS_SELECT: u64 = 23;
    pub const SYS_DUP: u64 = 32;
    pub const SYS_DUP2: u64 = 33;
    pub const SYS_GETPID: u64 = 39;
    pub const SYS_SOCKET: u64 = 41;
    pub const SYS_CONNECT: u64 = 42;
    pub const SYS_ACCEPT: u64 = 43;
    pub const SYS_SENDTO: u64 = 44;
    pub const SYS_RECVFROM: u64 = 45;
    pub const SYS_SETSOCKOPT: u64 = 54;
    pub const SYS_GETSOCKOPT: u64 = 55;
    pub const SYS_CLONE: u64 = 56;
    pub const SYS_KILL: u64 = 62;
    pub const SYS_UNAME: u64 = 63;
    pub const SYS_FCNTL: u64 = 72;
    pub const SYS_GETCWD: u64 = 79;
    pub const SYS_CHDIR: u64 = 80;
    pub const SYS_READLINK: u64 = 89;
    pub const SYS_GETUID: u64 = 102;
    pub const SYS_GETGID: u64 = 104;
    pub const SYS_GETEUID: u64 = 107;
    pub const SYS_GETEGID: u64 = 108;
    pub const SYS_GETPPID: u64 = 110;
    pub const SYS_WAITPID: u64 = 114;
    pub const SYS_NANOSLEEP: u64 = 35;
    pub const SYS_BIND: u64 = 49;
    pub const SYS_LISTEN: u64 = 50;
    pub const SYS_ARCH_PRCTL: u64 = 158;
    pub const SYS_GETTID: u64 = 186;
    pub const SYS_FUTEX: u64 = 202;
    pub const SYS_SET_TID_ADDRESS: u64 = 218;
    pub const SYS_TGKILL: u64 = 234;
    pub const SYS_OPENAT: u64 = 257;
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
            nr::SYS_POLL | nr::SYS_SELECT => sys_poll(ctx),
            nr::SYS_FUTEX => sys_futex(ctx),
            nr::SYS_RT_SIGACTION => sys_rt_sigaction(ctx),
            nr::SYS_RT_SIGPROCMASK => Ok(0),
            nr::SYS_CLONE => sys_clone(ctx),
            nr::SYS_PREAD64 => sys_pread64(ctx),
            nr::SYS_READV => sys_readv(ctx),
            nr::SYS_OPENAT => sys_openat(ctx),
            nr::SYS_GETPPID => Ok(ctx.process.parent),
            nr::SYS_GETTID => Ok(ctx.process.pid.0),
            nr::SYS_TGKILL => sys_tgkill(ctx),
            nr::SYS_SOCKET => sys_socket(ctx),
            nr::SYS_BIND | nr::SYS_LISTEN => Ok(0),
            nr::SYS_CONNECT => Ok((-111_i64) as u64), // -ECONNREFUSED
            nr::SYS_ACCEPT => Ok((-11_i64) as u64),   // -EAGAIN
            nr::SYS_SENDTO => sys_sendto(ctx),
            nr::SYS_RECVFROM => Ok((-11_i64) as u64), // -EAGAIN
            nr::SYS_SETSOCKOPT | nr::SYS_GETSOCKOPT => Ok(0),
            nr::SYS_READLINK => sys_readlink(ctx),
            nr::SYS_FCNTL => sys_fcntl(ctx),            unknown => {
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
    let fd = ctx.args[0] as usize;
    let buf_addr = ctx.args[1] as *const u8;
    let count = ctx.args[2] as usize;

    if !ctx.process.capabilities.has_permission(ResourceKind::File, true, false) {
        return Err(AbiError::PermissionDenied);
    }

    let target = ctx.process.fds.get(fd).map(|d| d.target);

    match target {
        Some(crate::process::FdTarget::Stdout) | Some(crate::process::FdTarget::Stderr)
        | None if fd == 1 || fd == 2 => {
            use x86_64::instructions::interrupts;
            let bytes = unsafe { core::slice::from_raw_parts(buf_addr, count) };
            // Print to VGA via println!
            if let Ok(s) = core::str::from_utf8(bytes) {
                crate::print!("{}", s);
            }
            // Also send to serial
            interrupts::without_interrupts(|| {
                let mut serial = crate::drivers::uart::SERIAL1.lock();
                for &b in bytes { serial.send(b); }
            });
            Ok(count as u64)
        }
        Some(crate::process::FdTarget::PipeWrite(chan_id)) => {
            let bytes = unsafe { core::slice::from_raw_parts(buf_addr, count) };
            let pid = ctx.process.pid;
            match crate::ipc::send(chan_id, pid, bytes) {
                Ok(()) => Ok(count as u64),
                Err(_) => Ok((-11_i64) as u64), // -EAGAIN (pipe full)
            }
        }
        Some(crate::process::FdTarget::File(_)) => {
            // VFS write — update offset, return count
            let bytes = unsafe { core::slice::from_raw_parts(buf_addr, count) };
            if let Some(desc) = ctx.process.fds.get_mut(fd) { desc.offset += bytes.len(); }
            Ok(count as u64)
        }
        _ => Ok((-9_i64) as u64), // -EBADF
    }
}

/// sys_read(fd, buf, count) → bytes_read
fn sys_read(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let fd = ctx.args[0] as usize;
    let buf_addr = ctx.args[1] as *mut u8;
    let count = ctx.args[2] as usize;

    // Determine fd target first (avoid borrow conflict)
    let target = ctx.process.fds.get(fd).map(|d| d.target);

    match target {
        Some(crate::process::FdTarget::Stdin) | None if fd == 0 => {
            let mut tmp = [0u8; 256];
            let n = crate::drivers::keyboard::read_stdin(&mut tmp[..count.min(256)]);
            if n > 0 { unsafe { core::ptr::copy_nonoverlapping(tmp.as_ptr(), buf_addr, n); } }
            Ok(n as u64)
        }
        Some(crate::process::FdTarget::PipeRead(chan_id)) => {
            match crate::ipc::recv(chan_id) {
                Ok(msg) => {
                    let n = msg.len.min(count);
                    unsafe { core::ptr::copy_nonoverlapping(msg.data.as_ptr(), buf_addr, n); }
                    Ok(n as u64)
                }
                Err(_) => Ok(0), // empty pipe — would block in real kernel
            }
        }
        Some(crate::process::FdTarget::File(_)) => {
            let offset = ctx.process.fds.get(fd).map(|d| d.offset).unwrap_or(0);
            let path_bytes = match ctx.process.fds.path_of(fd) {
                Some(p) => { let mut t = [0u8;64]; let n=p.len().min(63); t[..n].copy_from_slice(&p[..n]); (t,n) }
                None => return Ok((-9_i64) as u64),
            };
            let path_str = core::str::from_utf8(&path_bytes.0[..path_bytes.1]).unwrap_or("");
            let mut tmp = [0u8; 4096];
            let to_read = count.min(4096);
            match crate::fs::vfs::VFS.lock().read_raw(path_str, &mut tmp[..to_read], offset) {
                Ok(n) => {
                    if n > 0 { unsafe { core::ptr::copy_nonoverlapping(tmp.as_ptr(), buf_addr, n); } }
                    if let Some(desc) = ctx.process.fds.get_mut(fd) { desc.offset += n; }
                    Ok(n as u64)
                }
                Err(_) => Ok(0),
            }
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
    if new_brk == 0 || new_brk < ctx.process.brk {
        // Query or invalid shrink — return current break
        Ok(ctx.process.brk)
    } else {
        ctx.process.brk = new_brk;
        Ok(new_brk)
    }
}

/// sys_getpid() → pid
fn sys_getpid(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    Ok(ctx.process.pid.0)
}

/// sys_uname(buf) → 0
/// Writes a Linux utsname struct (6 × 65-byte null-terminated fields):
///   sysname, nodename, release, version, machine, domainname
fn sys_uname(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let buf = ctx.args[0] as *mut u8;
    if buf.is_null() { return Ok((-14_i64) as u64); } // -EFAULT

    // Each field is 65 bytes, null-padded
    let write_field = |dst: *mut u8, s: &[u8]| {
        let n = s.len().min(64);
        unsafe {
            core::ptr::copy_nonoverlapping(s.as_ptr(), dst, n);
            *dst.add(n) = 0;
        }
    };

    write_field(buf,           b"Linux");
    write_field(unsafe { buf.add(65)  }, b"ziqa");
    write_field(unsafe { buf.add(130) }, b"6.1.0-ziqa");
    write_field(unsafe { buf.add(195) }, b"#1 SMP ZiqaKernel");
    write_field(unsafe { buf.add(260) }, b"x86_64");
    write_field(unsafe { buf.add(325) }, b"(none)");
    Ok(0)
}

/// sys_mmap — handled by core dispatcher; this is a fallback
fn sys_mmap(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let length = ctx.args[1] as usize;
    if length == 0 { return Ok((-22_i64) as u64); } // -EINVAL
    let base = (ctx.process.mmap_bump + 0xFFF) & !0xFFF;
    ctx.process.mmap_bump = base + length as u64;
    use crate::memory::{MemoryRegion, paging::MemoryRegionFlags, VirtAddr};
    ctx.process.add_region(MemoryRegion {
        start: VirtAddr::new(base),
        size: length,
        flags: MemoryRegionFlags::read_write(),
        is_file_backed: false,
        file_offset: 0,
    });
    Ok(base)
}

/// sys_mprotect(addr, len, prot) → 0/-EINVAL
/// Changes protection of memory region.
/// prot: PROT_NONE=0, PROT_READ=1, PROT_WRITE=2, PROT_EXEC=4
fn sys_mprotect(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let addr = ctx.args[0];
    let len = ctx.args[1];
    let prot = ctx.args[2];
    println!("[Linux ABI] mprotect(addr=0x{:x}, len={}, prot={}) → OK", addr, len, prot);
    Ok(0)
}

/// sys_munmap(addr, length) → 0
fn sys_munmap(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let addr = ctx.args[0];
    println!("[Linux ABI] munmap(addr=0x{:x})", addr);
    Ok(0) // stub
}

/// sys_close(fd) → 0/-EBADF
fn sys_close(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let fd = ctx.args[0] as usize;
    
    if fd < 3 {
        // Cannot close stdin, stdout, stderr
        return Ok(0);
    }
    
    let result = ctx.process.fds.close(fd);
    if result {
        println!("[Linux ABI] close(fd={}) -> 0", fd);
        Ok(0)
    } else {
        Ok((-9_i64) as u64) // -EBADF
    }
}

/// sys_fstat(fd, statbuf) → 0
fn sys_fstat(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let fd = ctx.args[0] as usize;
    let statbuf = ctx.args[1] as *mut u64;
    if statbuf.is_null() { return Ok((-14_i64) as u64); } // -EFAULT

    // Linux stat64 layout (simplified — only fields programs actually check):
    // offset 0:  st_dev    (u64)
    // offset 8:  st_ino    (u64)
    // offset 16: st_nlink  (u64)
    // offset 24: st_mode   (u32) + st_uid (u32)
    // offset 32: st_gid    (u32) + pad (u32)
    // offset 40: st_rdev   (u64)
    // offset 48: st_size   (i64)
    // offset 56: st_blksize(i64)
    // offset 64: st_blocks (i64)
    let (mode, size): (u32, u64) = match fd {
        0 => (0x2190, 0),  // S_IFCHR | 0600 — stdin (char device)
        1 | 2 => (0x2190, 0), // stdout/stderr
        _ => {
            // Try to get size from VFS
            let path_bytes = match ctx.process.fds.path_of(fd) {
                Some(p) => { let mut t = [0u8;64]; let n=p.len().min(63); t[..n].copy_from_slice(&p[..n]); (t,n) }
                None => return Ok((-9_i64) as u64),
            };
            let path_str = core::str::from_utf8(&path_bytes.0[..path_bytes.1]).unwrap_or("");
            let mut buf = [0u8; 4096];
            let sz = crate::fs::vfs::VFS.lock().read_raw(path_str, &mut buf, 0).unwrap_or(0);
            (0x81A4, sz as u64) // S_IFREG | 0644
        }
    };
    unsafe {
        *statbuf.add(0) = 1u64;           // st_dev
        *statbuf.add(1) = fd as u64;      // st_ino
        *statbuf.add(2) = 1u64;           // st_nlink
        // st_mode (u32) in lower 32 bits of word at offset 24
        *(statbuf.add(3) as *mut u32) = mode;
        *statbuf.add(6) = size;           // st_size at offset 48
        *statbuf.add(7) = 4096u64;        // st_blksize
        *statbuf.add(8) = ((size + 511) / 512) as u64; // st_blocks
    }
    Ok(0)
}

/// sys_ioctl(fd, request, arg) → 0/-ENOTTY
fn sys_ioctl(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let _fd = ctx.args[0];
    let request = ctx.args[1];
    let arg = ctx.args[2] as *mut u8;

    // TIOCGWINSZ = 0x5413 — return terminal window size
    if request == 0x5413 {
        // struct winsize { ws_row: u16, ws_col: u16, ws_xpixel: u16, ws_ypixel: u16 }
        let ws = arg as *mut u16;
        unsafe {
            *ws.add(0) = 24;   // rows
            *ws.add(1) = 80;   // cols
            *ws.add(2) = 0;
            *ws.add(3) = 0;
        }
        return Ok(0);
    }

    // TIOCSWINSZ = 0x5414 — set terminal window size (ignore)
    if request == 0x5414 { return Ok(0); }

    // TCGETS/TCSETS — terminal attributes (stub success)
    if request == 0x5401 || request == 0x5402 { return Ok(0); }

    // DRM ioctls
    if (request & 0xFF00) == 0x6400 {
        return crate::drivers::drm::handle_ioctl(request, arg)
            .map(|v| v as u64)
            .map_err(|e| AbiError::Other(e));
    }

    Ok((-25_i64) as u64) // -ENOTTY
}

/// sys_writev(fd, iov, iovcnt) → bytes_written
/// iovec struct: { iov_base: *mut u8 (8 bytes), iov_len: usize (8 bytes) }
fn sys_writev(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let fd = ctx.args[0];
    let iov_ptr = ctx.args[1] as *const u64;
    let iovcnt = ctx.args[2] as usize;

    if fd != 1 && fd != 2 {
        return Ok((-9_i64) as u64); // -EBADF
    }

    let mut total = 0usize;
    use x86_64::instructions::interrupts;
    interrupts::without_interrupts(|| {
        let mut serial = crate::drivers::uart::SERIAL1.lock();
        for i in 0..iovcnt.min(16) {
            // Each iovec is 16 bytes: [base: u64, len: u64]
            let base = unsafe { *iov_ptr.add(i * 2) } as *const u8;
            let len  = unsafe { *iov_ptr.add(i * 2 + 1) } as usize;
            if len == 0 || base.is_null() { continue; }
            let bytes = unsafe { core::slice::from_raw_parts(base, len) };
            for &b in bytes { serial.send(b); }
            total += len;
        }
    });
    Ok(total as u64)
}

/// Known paths that exist in the kernel's pseudo-filesystem
fn known_path(_path: &str) -> bool {
    true // accept all paths for now
}

/// sys_access(pathname, mode) → 0/-ENOENT
fn sys_access(_ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    // For now, return success for all paths to let binaries start.
    // TODO: implement proper path checking via VFS + user-space copy.
    Ok(0) // pretend all paths exist
}

/// sys_open(pathname, flags, mode) → fd/-ENOENT
fn sys_open(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let path_addr = ctx.args[0] as *const u8;
    let flags = ctx.args[1] as u32;
    let mut tmp = [0u8; 128];
    let n = (0..127).take_while(|&i| unsafe { *path_addr.add(i) != 0 }).count();
    unsafe { core::ptr::copy_nonoverlapping(path_addr, tmp.as_mut_ptr(), n); }
    let path_str = core::str::from_utf8(&tmp[..n]).unwrap_or("");

    // Device/pseudo paths always succeed
    let is_known = known_path(path_str) || matches!(path_str,
        "/dev/null" | "/dev/zero" | "/dev/random" | "/dev/urandom"
        | "/dev/tty" | "/dev/console" | "/proc/self/maps" | "/proc/self/exe"
        | "/etc/passwd" | "/etc/localtime"
    );

    if is_known || crate::fs::vfs::VFS.lock().read_raw(path_str, &mut [0u8; 1], 0).is_ok() {
        let fd = ctx.process.fds.alloc_file(&tmp[..n], flags).unwrap_or(3);
        return Ok(fd as u64);
    }
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
    // Return success for all paths; binaries can proceed.
    Ok(0) // pretend all files exist
}

/// sys_lseek(fd, offset, whence) → new_offset
fn sys_lseek(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let fd = ctx.args[0] as usize;
    let offset = ctx.args[1] as i64;
    let whence = ctx.args[2];

    let current = ctx.process.fds.get(fd).map(|d| d.offset).unwrap_or(0);
    let new_offset: usize = match whence {
        0 => offset.max(0) as usize,                          // SEEK_SET
        1 => (current as i64 + offset).max(0) as usize,      // SEEK_CUR
        2 => offset.max(0) as usize,                          // SEEK_END (approx)
        _ => return Ok((-22_i64) as u64),                     // -EINVAL
    };
    if let Some(desc) = ctx.process.fds.get_mut(fd) {
        desc.offset = new_offset;
    }
    Ok(new_offset as u64)
}

/// sys_dup(oldfd) → newfd
fn sys_dup(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let oldfd = ctx.args[0] as usize;
    match ctx.process.fds.dup(oldfd, None) {
        Some(newfd) => Ok(newfd as u64),
        None => Ok((-9_i64) as u64), // -EBADF
    }
}

/// sys_dup2(oldfd, newfd) → newfd
fn sys_dup2(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let oldfd = ctx.args[0] as usize;
    let newfd = ctx.args[1] as usize;
    if oldfd == newfd {
        // dup2 with same fd is a no-op if fd is valid
        return if ctx.process.fds.get(oldfd).is_some() {
            Ok(newfd as u64)
        } else {
            Ok((-9_i64) as u64)
        };
    }
    match ctx.process.fds.dup(oldfd, Some(newfd)) {
        Some(fd) => Ok(fd as u64),
        None => Ok((-9_i64) as u64), // -EBADF
    }
}

/// sys_pipe(pipefd[2]) → 0
/// pipefd is a *mut [i32; 2] in user memory: [read_fd, write_fd]
fn sys_pipe(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let pipefd_ptr = ctx.args[0] as *mut u32;
    if pipefd_ptr.is_null() { return Ok((-14_i64) as u64); } // -EFAULT

    let chan_id = match crate::ipc::create_channel() {
        Some(id) => id,
        None => return Ok((-24_i64) as u64), // -EMFILE
    };

    let read_fd = ctx.process.fds.alloc(crate::process::FileDesc {
        target: crate::process::FdTarget::PipeRead(chan_id),
        flags: 0,
        offset: 0,
    });
    let write_fd = ctx.process.fds.alloc(crate::process::FileDesc {
        target: crate::process::FdTarget::PipeWrite(chan_id),
        flags: 0,
        offset: 0,
    });

    match (read_fd, write_fd) {
        (Some(rfd), Some(wfd)) => {
            unsafe {
                *pipefd_ptr       = rfd as u32;
                *pipefd_ptr.add(1) = wfd as u32;
            }
            Ok(0)
        }
        _ => Ok((-24_i64) as u64), // -EMFILE
    }
}

/// sys_getcwd(buf, size) → buf_addr on success, -ERANGE if too small
fn sys_getcwd(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let buf_addr = ctx.args[0] as *mut u8;
    let size = ctx.args[1] as usize;
    let cwd_len = ctx.process.cwd_len;
    // Need space for path + null terminator
    if size < cwd_len + 1 {
        return Ok((-34_i64) as u64); // -ERANGE
    }
    unsafe {
        core::ptr::copy_nonoverlapping(ctx.process.cwd.as_ptr(), buf_addr, cwd_len);
        *buf_addr.add(cwd_len) = 0;
    }
    Ok(buf_addr as u64)
}

/// sys_chdir(path) → 0 / -ENOENT
fn sys_chdir(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let path_addr = ctx.args[0] as *const u8;
    let mut tmp = [0u8; 128];
    let n = (0..127).take_while(|&i| unsafe { *path_addr.add(i) != 0 }).count();
    unsafe { core::ptr::copy_nonoverlapping(path_addr, tmp.as_mut_ptr(), n); }
    // Accept any path that looks valid (starts with '/')
    if n == 0 { return Ok((-2_i64) as u64); }
    ctx.process.cwd[..n].copy_from_slice(&tmp[..n]);
    ctx.process.cwd_len = n;
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

/// sys_nanosleep(req: *timespec, rem: *timespec) → 0
/// timespec layout: { tv_sec: u64, tv_nsec: u64 } (16 bytes)
fn sys_nanosleep(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let req_ptr = ctx.args[0] as *const u64;
    let ms = if req_ptr.is_null() {
        1
    } else {
        let tv_sec  = unsafe { *req_ptr };
        let tv_nsec = unsafe { *req_ptr.add(1) };
        tv_sec * 1000 + tv_nsec / 1_000_000
    };
    crate::timer::sleep_ms(ctx.process.pid, ms.max(1));
    Ok(0)
}

/// sys_poll(fds, nfds, timeout_ms) → 0 (timeout) or >0 (ready fds)
fn sys_poll(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let nfds = ctx.args[1] as usize;
    let timeout_ms = ctx.args[2] as i64;
    
    // For now, just check stdin (fd=0) for readability
    // In a real implementation, we'd iterate over all fds
    if nfds > 0 {
        // Check if we have data on stdin
        let mut tmp = [0u8; 256];
        let n = crate::drivers::keyboard::read_stdin(&mut tmp);
        if n > 0 {
            return Ok(1); // One fd ready
        }
    }
    
    // Handle timeout
    if timeout_ms > 0 {
        crate::timer::sleep_ms(ctx.process.pid, timeout_ms as u64);
    } else if timeout_ms == 0 {
        // poll with timeout=0 is non-blocking
    }
    // timeout < 0 means wait forever (not implemented)
    
    Ok(0) // No fds ready or timeout
}

/// sys_futex(uaddr, op, val, ...) → 0
/// FUTEX operations:
///   0 = FUTEX_WAIT - wait if value at uaddr equals val
///   1 = FUTEX_WAKE - wake up to 'val' waiters
///   128 = FUTEX_FD - (ignored)
///   129 = FUTEX_EXACT_NAME - (ignored)
fn sys_futex(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let op = ctx.args[1] as u32 & 0x7F; // mask off FUTEX_PRIVATE_FLAG
    let _uaddr = ctx.args[0];
    let _val = ctx.args[2] as i32;
    
    match op {
        0 => { // FUTEX_WAIT
            // Check if value matches, if so block
            // For now, we just yield the CPU
            if ctx.process.state == crate::process::ProcessState::Running {
                ctx.process.state = crate::process::ProcessState::Ready;
            }
            // A real implementation would check *uaddr == val
            // and only block if true
        }
        1 => { // FUTEX_WAKE
            // Wake up to 'val' waiters
            // For now, just return success
            // A real implementation would add processes to ready queue
        }
        _ => {
            // Other operations not implemented
            println!("[FUTEX] unsupported op: {}", op);
        }
    }
    Ok(0)
}

/// sys_rt_sigaction(signum, act, oldact, sigsetsize) → 0
fn sys_rt_sigaction(_ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    Ok(0) // stub: accept all signal handler registrations
}

/// sys_clone(flags, stack, ptid, ctid, regs) → child_pid
/// Clone flags:
///   CLONE_VM = 0x01000000 - share address space (thread)
///   CLONE_FS = 0x00010000 - share filesystem info
///   CLONE_FILES = 0x00000400 - share file descriptors
///   CLONE_SIGHAND = 0x00000002 - share signal handlers
///   CLONE_PIDFD = 0x00002000 - pidfd object
/// 
/// For now, treat as fork (full copy). Thread support requires shared memory.
fn sys_clone(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let flags = ctx.args[0];
    let _stack = ctx.args[1];
    let _ptid = ctx.args[2];
    let _ctid = ctx.args[3];
    
    const CLONE_VM: u64 = 0x01000000;
    
    let parent_pid = ctx.process.pid;
    
    if flags & CLONE_VM != 0 {
        println!("[Linux ABI] clone(CLONE_VM) → thread (falling back to fork)");
    }
    
    match crate::process::scheduler::SCHEDULER.lock().fork(parent_pid) {
        Some(child_pid) => Ok(child_pid.0),
        None => Ok((-11_i64) as u64), // -EAGAIN
    }
}

/// sys_pread64(fd, buf, count, offset) → bytes_read
fn sys_pread64(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let fd = ctx.args[0];
    let _buf_addr = ctx.args[1];
    let count = ctx.args[2] as usize;
    let _offset = ctx.args[3];
    
    match fd {
        0 => {
            // stdin - read from keyboard
            let mut tmp = [0u8; 256];
            let n = crate::drivers::keyboard::read_stdin(&mut tmp[..count.min(256)]);
            Ok(n as u64)
        }
        1 | 2 => {
            // stdout/stderr - reading from them doesn't make sense
            Ok(0)
        }
        _ => Ok((-9_i64) as u64), // -EBADF
    }
}

/// sys_readv(fd, iov, iovcnt) → bytes_read
/// iov is array of { iov_base: *mut u8, iov_len: usize } (16 bytes each)
fn sys_readv(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let fd = ctx.args[0] as usize;
    let iov_ptr = ctx.args[1] as *mut u64;
    let iovcnt = ctx.args[2] as usize;

    if fd != 0 {
        return Ok((-9_i64) as u64); // -EBADF
    }

    let mut total: usize = 0;
    for i in 0..iovcnt.min(16) {
        let base = unsafe { *iov_ptr.add(i * 2) } as *mut u8;
        let len = unsafe { *iov_ptr.add(i * 2 + 1) } as usize;
        if base.is_null() || len == 0 {
            continue;
        }
        let mut tmp = [0u8; 256];
        let n = crate::drivers::keyboard::read_stdin(&mut tmp[..len.min(256)]);
        if n == 0 {
            break;
        }
        unsafe {
            core::ptr::copy_nonoverlapping(tmp.as_ptr(), base, n);
        }
        total += n;
        if n < len {
            break;
        }
    }
    Ok(total as u64)
}

/// sys_openat(dirfd, pathname, flags, mode) → fd
/// dirfd=AT_FDCWD(-100) means relative to cwd — treat same as open.
fn sys_openat(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    // Shift args: dirfd=args[0], path=args[1], flags=args[2]
    let path_addr = ctx.args[1] as *const u8;
    let flags = ctx.args[2] as u32;
    let mut tmp = [0u8; 128];
    let n = (0..127).take_while(|&i| unsafe { *path_addr.add(i) != 0 }).count();
    unsafe { core::ptr::copy_nonoverlapping(path_addr, tmp.as_mut_ptr(), n); }
    let path_str = core::str::from_utf8(&tmp[..n]).unwrap_or("");

    let is_known = known_path(path_str) || matches!(path_str,
        "/dev/null" | "/dev/zero" | "/dev/random" | "/dev/urandom"
        | "/dev/tty" | "/dev/console" | "/proc/self/maps" | "/proc/self/exe"
        | "/etc/passwd" | "/etc/localtime"
    );

    if is_known || crate::fs::vfs::VFS.lock().read_raw(path_str, &mut [0u8; 1], 0).is_ok() {
        let fd = ctx.process.fds.alloc_file(&tmp[..n], flags).unwrap_or(3);
        return Ok(fd as u64);
    }
    Ok((-2_i64) as u64) // -ENOENT
}

/// sys_tgkill(tgid, tid, sig) → 0
fn sys_tgkill(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let tid = ctx.args[1];
    let sig = ctx.args[2] as u8;
    let ok = crate::process::scheduler::SCHEDULER.lock()
        .send_signal(crate::process::Pid(tid), sig);
    if ok { Ok(0) } else { Ok((-3_i64) as u64) } // -ESRCH
}

/// sys_socket(domain, type, protocol) → fd
/// Returns a fake socket fd backed by a File entry.
fn sys_socket(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let _domain = ctx.args[0];   // AF_INET=2, AF_UNIX=1
    let _socktype = ctx.args[1]; // SOCK_STREAM=1, SOCK_DGRAM=2
    // Allocate a dummy fd tagged as a socket (reuse File slot with path "socket:")
    // Note: args[2] is protocol, unused for now
    let fd = ctx.process.fds.alloc_file(b"socket:", ctx.args[1] as u32).unwrap_or(3);
    Ok(fd as u64)
}

/// sys_sendto(sockfd, buf, len, flags, dest_addr, addrlen) → bytes_sent
fn sys_sendto(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let count = ctx.args[2];
    // Stub: pretend we sent all bytes
    Ok(count)
}

/// sys_readlink(path, buf, bufsiz) → bytes_written
fn sys_readlink(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let path_addr = ctx.args[0] as *const u8;
    let buf_addr  = ctx.args[1] as *mut u8;
    let bufsiz    = ctx.args[2] as usize;

    let mut tmp = [0u8; 128];
    let n = (0..127).take_while(|&i| unsafe { *path_addr.add(i) != 0 }).count();
    unsafe { core::ptr::copy_nonoverlapping(path_addr, tmp.as_mut_ptr(), n); }
    let path_str = core::str::from_utf8(&tmp[..n]).unwrap_or("");

    // /proc/self/exe → return a fake binary path
    let target: &[u8] = match path_str {
        "/proc/self/exe" => b"/bin/busybox",
        "/proc/self/fd/0" => b"/dev/stdin",
        "/proc/self/fd/1" => b"/dev/stdout",
        "/proc/self/fd/2" => b"/dev/stderr",
        _ => return Ok((-2_i64) as u64), // -ENOENT
    };

    let n = target.len().min(bufsiz);
    unsafe { core::ptr::copy_nonoverlapping(target.as_ptr(), buf_addr, n); }
    Ok(n as u64)
}

/// sys_fcntl(fd, cmd, arg) → result
/// F_GETFD=1, F_SETFD=2, F_GETFL=3, F_SETFL=4, F_DUPFD=0, F_DUPFD_CLOEXEC=1030
fn sys_fcntl(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let fd  = ctx.args[0] as usize;
    let cmd = ctx.args[1];
    let _arg = ctx.args[2];
    match cmd {
        0 => { // F_DUPFD — dup to lowest fd >= arg
            match ctx.process.fds.dup(fd, None) {
                Some(newfd) => Ok(newfd as u64),
                None => Ok((-9_i64) as u64),
            }
        }
        1 | 2 => Ok(0),  // F_GETFD / F_SETFD — FD_CLOEXEC flag, ignore
        3 => Ok(0),       // F_GETFL — return O_RDWR=2
        4 => Ok(0),       // F_SETFL — accept any flags
        1030 => {         // F_DUPFD_CLOEXEC
            match ctx.process.fds.dup(fd, None) {
                Some(newfd) => Ok(newfd as u64),
                None => Ok((-9_i64) as u64),
            }
        }
        _ => Ok((-22_i64) as u64), // -EINVAL
    }
}
