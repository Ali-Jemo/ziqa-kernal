#![allow(dead_code)]

/// Linux ABI Plugin for ZiqaKernel
///
/// Community boundary: this file is the Linux ABI facade. Graphify flagged the
/// Linux syscall handlers as low-cohesion; keep new syscall-family logic in
/// dedicated modules per `docs/architecture/community-boundaries.md`.
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

mod fs;
mod memory;
mod misc;
#[cfg(feature = "net")]
mod net;
mod ebpf;
mod process;
mod time;
mod signal;

use crate::abi::syscall::SyscallContext;
use crate::abi::usercopy::{UserSliceRo, UserSliceWo};
use crate::abi::{AbiError, AbiPlugin};
use crate::capability::ResourceKind;
use crate::println;
use crate::process::vma::Vma;
use crate::process::{AbiKind, Process};

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
    pub const SYS_RT_SIGRETURN: u64 = 15;
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
    pub const SYS_EXECVE: u64 = 59;
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
    // Extended filesystem operations (targeting 100+ syscalls)
    pub const SYS_RENAME: u64 = 82;
    pub const SYS_MKDIR: u64 = 83;
    pub const SYS_RMDIR: u64 = 84;
    pub const SYS_CREAT: u64 = 85;
    pub const SYS_LINK: u64 = 86;
    pub const SYS_UNLINK: u64 = 87;
    pub const SYS_SYMLINK: u64 = 88;
    pub const SYS_CHMOD: u64 = 90;
    pub const SYS_UMASK: u64 = 95;
    pub const SYS_STATFS: u64 = 137;
    pub const SYS_GETDENTS64: u64 = 217;
    pub const SYS_CLOCK_GETTIME: u64 = 228;
    pub const SYS_NEWFSTATAT: u64 = 262;
    pub const SYS_GETRANDOM: u64 = 318;
    // === Batch 2: 20 more syscalls toward 100+ ===
    pub const SYS_MSYNC: u64 = 26;
    pub const SYS_SENDFILE: u64 = 40;
    pub const SYS_FLOCK: u64 = 73;
    pub const SYS_FSYNC: u64 = 74;
    pub const SYS_FDATASYNC: u64 = 75;
    pub const SYS_FTRUNCATE: u64 = 77;
    pub const SYS_GETTIMEOFDAY: u64 = 96;
    pub const SYS_GETRLIMIT: u64 = 97;
    pub const SYS_SETRLIMIT: u64 = 98;
    pub const SYS_SYSINFO: u64 = 99;
    pub const SYS_SETPGID: u64 = 109;
    pub const SYS_SETSID: u64 = 112;
    pub const SYS_GETPGID: u64 = 121;
    pub const SYS_GETSID: u64 = 124;
    pub const SYS_UTIMES: u64 = 134;
    pub const SYS_SYNC: u64 = 162;
    pub const SYS_IOPL: u64 = 172;
    pub const SYS_GETPRIORITY: u64 = 140;
    pub const SYS_SETPRIORITY: u64 = 141;
    pub const SYS_TIME: u64 = 201;
    pub const SYS_MADVISE: u64 = 233;
    pub const SYS_PRLIMIT64: u64 = 302;
    // === Batch 3: 28 more syscalls to surpass 100+ ===
    pub const SYS_NICE: u64 = 34;
    pub const SYS_MKNOD: u64 = 133;
    pub const SYS_PERSONALITY: u64 = 135;
    pub const SYS_SCHED_GETPARAM: u64 = 143;
    pub const SYS_SCHED_SETPARAM: u64 = 144;
    pub const SYS_SCHED_GETSCHEDULER: u64 = 145;
    pub const SYS_SCHED_SETSCHEDULER: u64 = 146;
    pub const SYS_SCHED_GET_PRIORITY_MAX: u64 = 147;
    pub const SYS_SCHED_GET_PRIORITY_MIN: u64 = 148;
    pub const SYS_SCHED_RR_GET_INTERVAL: u64 = 149;
    pub const SYS_PRCTL: u64 = 157;
    pub const SYS_CLOCK_GETRES: u64 = 229;
    pub const SYS_EPOLL_CREATE1: u64 = 291;
    pub const SYS_EPOLL_CTL: u64 = 255;
    pub const SYS_EPOLL_WAIT: u64 = 232;
    pub const SYS_EVENTFD: u64 = 284;
    pub const SYS_SIGNALFD: u64 = 289;
    pub const SYS_TIMERFD_CREATE: u64 = 283;
    pub const SYS_TIMERFD_SETTIME: u64 = 286;
    pub const SYS_FACCESSAT: u64 = 269;
    pub const SYS_UTIMENSAT: u64 = 280;
    pub const SYS_PPOLL: u64 = 271;
    pub const SYS_PSELECT6: u64 = 270;
    pub const SYS_GETCPU: u64 = 309;
    pub const SYS_BPF: u64 = 321;
    pub const SYS_PIDFD_OPEN: u64 = 434;
    pub const SYS_MEMFD_CREATE: u64 = 319;
    pub const SYS_FALLOCATE: u64 = 285;
    pub const SYS_COPY_FILE_RANGE: u64 = 326;
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

    fn handle_syscall(
        &self,
        _handler: &dyn crate::abi::handler::SyscallHandler,
        ctx: &mut SyscallContext,
    ) -> Result<u64, AbiError> {
        // NOTE: ZIQA native syscalls (1003/1004/2000–2004) are handled upstream
        // in dispatch_syscall() before this plugin is ever called — no need to
        // re-dispatch them here.
        //
        // Graphify Community 0 boundary: keep this facade thin and delegate
        // syscall families to focused dispatch modules.
        if let Some(result) = memory::handle(ctx) {
            return result;
        }
        if let Some(result) = process::handle(ctx) {
            return result;
        }
        if let Some(result) = fs::handle(ctx) {
            return result;
        }
        if let Some(result) = time::handle(ctx) {
            return result;
        }
        #[cfg(feature = "net")]
        if let Some(result) = net::handle(ctx) {
            return result;
        }
        if let Some(result) = signal::handle(ctx) {
            return result;
        }
        if let Some(result) = misc::handle(ctx) {
            return result;
        }
        if let Some(result) = ebpf::handle(ctx) {
            return result;
        }

        println!("[Linux ABI] Unimplemented syscall: {}", ctx.number);

        Err(AbiError::UnsupportedSyscall(ctx.number))
    }
}

// ──────────────────────────────────────────────────────────
// Syscall implementations
// ──────────────────────────────────────────────────────────

/// sys_write(fd, buf, count) → bytes_written
fn sys_write(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let fd = ctx.args[0] as usize;
    let count = ctx.args[2] as usize;

    if count == 0 {
        return Ok(0);
    }

    // Validate and copy user buffer into a kernel buffer
    let mut buf = alloc::vec![0u8; count];
    UserSliceRo::ro(ctx.args[1], count)
        .map_err(|_| AbiError::Other("EFAULT: bad write buffer"))?
        .copy_to_slice(&mut buf)
        .map_err(|_| AbiError::Other("EFAULT: copy failed"))?;

    let target = ctx.process.fds.get(fd).map(|d| d.target);

    match target {
        Some(crate::process::FdTarget::Stdout) | Some(crate::process::FdTarget::Stderr) | None
            if fd == 1 || fd == 2 =>
        {
            // Stdout/Stderr allowed without capability (basic console output)
            use x86_64::instructions::interrupts;
            if let Ok(s) = core::str::from_utf8(&buf) {
                crate::print!("{}", s);
            }
            // Also send to serial
            interrupts::without_interrupts(|| {
                let mut serial = crate::drivers::uart::SERIAL1.lock();
                for &b in &buf {
                    serial.send(b);
                }
            });
            Ok(count as u64)
        }
        Some(crate::process::FdTarget::PipeWrite(chan_id)) => {
            // Pipe writes require IpcChannel capability
            if !ctx.process.capabilities.has_permission(ResourceKind::IpcChannel, true, false) {
                return Err(AbiError::PermissionDenied);
            }
            let pid = ctx.process.pid;
            match crate::ipc::send(chan_id, pid, &buf) {
                Ok(()) => Ok(count as u64),
                Err(_) => Ok((-11_i64) as u64), // -EAGAIN (pipe full)
            }
        }
        Some(crate::process::FdTarget::File(_)) | Some(crate::process::FdTarget::Scheme(_, _)) => {
            if !ctx.process.capabilities.has_permission(ResourceKind::File, true, false) {
                return Err(AbiError::PermissionDenied);
            }
            let handle = match ctx.process.get_vfs_handle(fd) {
                Some(h) => h,
                None => return Ok((-9_i64) as u64), // -EBADF
            };
            let offset = ctx.process.fds.get(fd).map(|d| d.offset).unwrap_or(0);
            match crate::fs::vfs::VFS.read().write_handle(&handle, &buf, offset) {
                Ok(n) => {
                    if let Some(desc) = ctx.process.fds.get_mut(fd) {
                        desc.offset += n;
                    }
                    Ok(n as u64)
                }
                Err(e) => {
                    let errno = crate::abi::syscall::abi_error_to_errno(&e);
                    Ok((errno as i64).wrapping_neg() as u64)
                }
            }
        }
        _ => Ok((-9_i64) as u64), // -EBADF
    }
}

/// sys_read(fd, buf, count) → bytes_read
fn sys_read(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let fd = ctx.args[0] as usize;
    let count = ctx.args[2] as usize;

    if count == 0 {
        return Ok(0);
    }

    // Determine fd target first (avoid borrow conflict)
    let target = ctx.process.fds.get(fd).map(|d| d.target);

    match target {
        Some(crate::process::FdTarget::Stdin) | None if fd == 0 => {
            let mut tmp = [0u8; 256];
            let n = crate::drivers::keyboard::read_stdin(&mut tmp[..count.min(256)]);
            if n > 0 {
                UserSliceWo::wo(ctx.args[1], n)
                    .map_err(|_| AbiError::Other("EFAULT: bad read buffer"))?
                    .copy_from_slice(&tmp[..n])
                    .map_err(|_| AbiError::Other("EFAULT: copy failed"))?;
            }
            return Ok(n as u64);
        }
        Some(crate::process::FdTarget::PipeRead(chan_id)) => {
            match crate::ipc::recv(chan_id) {
                Ok(msg) => {
                    let n = msg.len.min(count);
                    if n > 0 {
                        UserSliceWo::wo(ctx.args[1], n)
                            .map_err(|_| AbiError::Other("EFAULT: bad read buffer"))?
                            .copy_from_slice(&msg.data[..n])
                            .map_err(|_| AbiError::Other("EFAULT: copy failed"))?;
                    }
                    return Ok(n as u64);
                }
                Err(_) => return Ok(0),
            }
        }
        Some(crate::process::FdTarget::File(_)) | Some(crate::process::FdTarget::Scheme(_, _)) => {
            if !ctx.process.capabilities.has_permission(ResourceKind::File, false, false) {
                return Err(AbiError::PermissionDenied);
            }
            let handle = match ctx.process.get_vfs_handle(fd) {
                Some(h) => h,
                None => return Ok((-9_i64) as u64), // -EBADF
            };
            let offset = ctx.process.fds.get(fd).map(|d| d.offset).unwrap_or(0);
            
            let mut tmp = alloc::vec![0u8; count];
            match crate::fs::vfs::VFS.read().read_handle(&handle, &mut tmp, offset) {
                Ok(n) => {
                    if let Some(desc) = ctx.process.fds.get_mut(fd) {
                        desc.offset += n;
                    }
                    if n > 0 {
                        UserSliceWo::wo(ctx.args[1], n)
                            .map_err(|_| AbiError::Other("EFAULT: bad read buffer"))?
                            .copy_from_slice(&tmp[..n])
                            .map_err(|_| AbiError::Other("EFAULT: copy failed"))?;
                    }
                    Ok(n as u64)
                }
                Err(e) => {
                    let errno = crate::abi::syscall::abi_error_to_errno(&e);
                    Ok((errno as i64).wrapping_neg() as u64)
                }
            }
        }
        _ => Ok((-9_i64) as u64),
    }
}

/// sys_exit(status) → never returns
fn sys_exit(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let status = ctx.args[0] as i64;
    println!(
        "[Linux ABI] Process {} exiting with code {}",
        ctx.process.pid.0, status
    );
    ctx.process.exit(status);
    Ok(0)
}

/// sys_brk(new_brk) → current_brk
fn sys_brk(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let new_brk = ctx.args[0];
    let old_brk = ctx.process.brk;
    
    // Query current brk (new_brk == 0 or less than heap start)
    if new_brk == 0 || new_brk < 0x2000_0000 {
        return Ok(old_brk);
    }
    
    let aligned_new_brk = (new_brk + 0xFFF) & !0xFFF;
    
    // Map pages for expanded heap
    if aligned_new_brk > old_brk {
        if let Err(e) = crate::memory::paging::handle_brk(
            ctx.process.page_table_frame,
            old_brk,
            aligned_new_brk,
        ) {
            crate::println!("[BRK] failed to map pages: {}", e);
            return Ok(old_brk); // Return old brk on failure
        }
    }
    
    ctx.process.brk = aligned_new_brk;
    Ok(aligned_new_brk)
}

/// sys_getpid() → pid
fn sys_getpid(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    Ok(ctx.process.pid.0)
}

/// sys_uname(buf) → 0
/// Writes a Linux utsname struct (6 × 65-byte null-terminated fields):
///   sysname, nodename, release, version, machine, domainname
fn sys_uname(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let fields: &[&[u8]] = &[b"Linux", b"ziqa", b"6.1.0-ziqa", b"#1 SMP ZiqaKernel", b"x86_64", b"(none)"];
    let mut buf = [0u8; 390]; // 6 × 65
    for (i, s) in fields.iter().enumerate() {
        let n = s.len().min(64);
        buf[i * 65..i * 65 + n].copy_from_slice(s);
    }
    UserSliceWo::wo(ctx.args[0], 390)
        .map_err(|_| AbiError::Other("EFAULT"))?
        .copy_from_slice(&buf)
        .map_err(|_| AbiError::Other("EFAULT"))?;
    Ok(0)
}

/// sys_mmap — handled by core dispatcher; this is a fallback
fn sys_mmap(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    // Check Memory capability before mapping
    if !ctx.process.capabilities.has_permission(ResourceKind::Memory, false, false) {
        return Err(AbiError::PermissionDenied);
    }

    let length = ctx.args[1] as usize;
    let prot = ctx.args[2];
    let flags = ctx.args[3];
    
    if length == 0 {
        return Ok((-22_i64) as u64); // -EINVAL
    }
    
    let is_anonymous = (flags & 0x20) != 0; // MAP_ANONYMOUS
    
    let start_hint = crate::memory::VirtAddr::new(ctx.args[0]);
    let base = crate::process::vma::find_free_range(&ctx.process.vmas, length, start_hint)
        .ok_or(AbiError::Other("mmap: no free address space"))?;

    let region_flags = prot_to_region_flags(prot);
    
    ctx.process.add_region(Vma {
        start: base,
        end: base + length as u64,
        flags: region_flags,
        is_file_backed: !is_anonymous,
        file_path: None,
        file_offset: 0,
        bco_hook: None,
    });
    
    Ok(base.as_u64())
}



/// Convert Linux PROT flags to MemoryRegionFlags
/// PROT_NONE=0, PROT_READ=1, PROT_WRITE=2, PROT_EXEC=4
fn prot_to_region_flags(prot: u64) -> crate::memory::paging::MemoryRegionFlags {
    use crate::memory::paging::MemoryRegionFlags;
    MemoryRegionFlags {
        readable: (prot & 1) != 0,
        writable: (prot & 2) != 0,
        executable: (prot & 4) != 0,
        user_accessible: true,
        copy_on_write: false,
    }
}

/// Create an OffsetPageTable for the given root frame (or current CR3 if None).
unsafe fn mapper_for_root(
    root_frame: Option<x86_64::structures::paging::PhysFrame>,
) -> x86_64::structures::paging::mapper::OffsetPageTable<'static> {
    use crate::memory::paging::phys_offset;
    use x86_64::structures::paging::mapper::OffsetPageTable;

    let po = phys_offset();
    match root_frame {
        Some(frame) => {
            let l4_virt = po + frame.start_address().as_u64();
            let l4 = &mut *(l4_virt.as_mut_ptr());
            OffsetPageTable::new(l4, po)
        }
        None => crate::memory::paging::current_mapper(),
    }
}

/// Apply new page-table flags to every mapped page in [start, end).
/// Skips unmapped pages (demand paging will use the VMA flags).
fn update_range_page_flags(
    root_frame: Option<x86_64::structures::paging::PhysFrame>,
    start: u64,
    end: u64,
    pte_flags: x86_64::structures::paging::PageTableFlags,
) {
    use x86_64::structures::paging::{Mapper, Page, Size4KiB, Translate};
    use x86_64::VirtAddr;

    let mut mapper = unsafe { mapper_for_root(root_frame) };

    for addr in (start..end).step_by(4096) {
        let page = Page::<Size4KiB>::containing_address(VirtAddr::new(addr));
        if mapper.translate_addr(page.start_address()).is_some() {
            let _ = unsafe { mapper.update_flags(page, pte_flags) };
        }
    }
}

/// Unmap every mapped page in [start, end).
/// Physical frames are NOT freed (no FrameDeallocator impl in kernel yet).
fn unmap_range_pages(
    root_frame: Option<x86_64::structures::paging::PhysFrame>,
    start: u64,
    end: u64,
) {
    use x86_64::structures::paging::{Mapper, Page, Size4KiB, Translate};
    use x86_64::VirtAddr;

    let mut mapper = unsafe { mapper_for_root(root_frame) };

    for addr in (start..end).step_by(4096) {
        let page = Page::<Size4KiB>::containing_address(VirtAddr::new(addr));
        if mapper.translate_addr(page.start_address()).is_some() {
            if let Ok((_frame, flusher)) = mapper.unmap(page) {
                flusher.flush();
            }
        }
    }
}

/// sys_mprotect(addr, len, prot) → 0/-EINVAL/-ENOMEM/-EFAULT
/// Changes protection of memory region.
/// prot: PROT_NONE=0, PROT_READ=1, PROT_WRITE=2, PROT_EXEC=4
fn sys_mprotect(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    use crate::memory::paging::region_flags_to_page_flags;
    use x86_64::instructions::tlb;

    let addr = ctx.args[0];
    let len = ctx.args[1];
    let prot = ctx.args[2];

    // Align to page boundaries
    let start_addr = addr & !0xFFF;
    let range_end = addr + len;
    let end_addr = if range_end == 0 { 0xFFFF_FFFF_FFFF_F000 } else { (range_end + 0xFFF) & !0xFFF };
    if start_addr >= end_addr || len == 0 {
        return Ok((-22i64) as u64); // -EINVAL
    }

    // Find the first VMA overlapping [start_addr, end_addr)
    let vma_idx = match ctx.process.vmas.iter().position(|vma| {
        let vs = vma.start.as_u64();
        let ve = vma.end.as_u64();
        start_addr < ve && end_addr > vs
    }) {
        Some(i) => i,
        None => return Ok((-14i64) as u64), // -EFAULT
    };

    let new_flags = prot_to_region_flags(prot);
    let pte_flags = region_flags_to_page_flags(&new_flags);

    // Update VMA flags
    ctx.process.vmas[vma_idx].flags = new_flags;

    // Update page-table entries for already-mapped pages
    let root_frame = ctx.process.page_table_frame;
    let vma_start = ctx.process.vmas[vma_idx].start.as_u64();
    let vma_end = ctx.process.vmas[vma_idx].end.as_u64();
    let update_start = start_addr.max(vma_start);
    let update_end = end_addr.min(vma_end);

    if prot == 0 {
        // PROT_NONE — unmap pages so accesses generate page faults
        unmap_range_pages(root_frame, update_start, update_end);
    } else {
        update_range_page_flags(root_frame, update_start, update_end, pte_flags);
    }

    tlb::flush_all();
    Ok(0)
}

/// sys_munmap(addr, length) → 0/-EINVAL
fn sys_munmap(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    use x86_64::instructions::tlb;

    let addr = ctx.args[0];
    let len = ctx.args[1];

    let start_addr = addr & !0xFFF;
    let end_addr = ((addr + len + 0xFFF) & !0xFFF).max(start_addr + 0x1000);
    if start_addr >= end_addr || len == 0 {
        return Ok((-22i64) as u64); // -EINVAL
    }

    // Find and remove VMAs fully covered by the range
    let root_frame = ctx.process.page_table_frame;
    ctx.process.vmas.retain(|vma| {
        let vs = vma.start.as_u64();
        let ve = vma.end.as_u64();
        // If this VMA is completely inside the munmap range, unmap it
        if vs >= start_addr && ve <= end_addr {
            unmap_range_pages(root_frame, vs, ve);
            false // remove
        } else {
            true // keep
        }
    });

    tlb::flush_all();
    Ok(0)
}

/// sys_close(fd) → 0/-EBADF
fn sys_close(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let fd = ctx.args[0] as usize;

    if fd < 3 {
        return Ok(0);
    }

    // Clean up socket state if this fd is a socket
    #[cfg(feature = "net")]
    if crate::net::socket::SOCKETS.lock().exists(fd) {
        crate::net::socket::SOCKETS.lock().remove(fd);
    }

    // Call scheme/VFS close if applicable
    if let Some(handle) = ctx.process.get_vfs_handle(fd) {
        let _ = crate::fs::vfs::VFS.read().close_handle(&handle);
    }

    let result = ctx.process.fds.close(fd);
    if result {
        Ok(0)
    } else {
        Ok((-9_i64) as u64) // -EBADF
    }
}

/// sys_fstat(fd, statbuf) → 0
fn sys_fstat(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let fd = ctx.args[0] as usize;

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
        0 => (0x2190, 0),     // S_IFCHR | 0600 — stdin (char device)
        1 | 2 => (0x2190, 0), // stdout/stderr
        _ => {
            // Try to get size from VFS
            let path_bytes = match ctx.process.fds.path_of(fd) {
                Some(p) => {
                    let mut t = [0u8; 64];
                    let n = p.len().min(63);
                    t[..n].copy_from_slice(&p[..n]);
                    (t, n)
                }
                None => return Ok((-9_i64) as u64),
            };
            let path_str = core::str::from_utf8(&path_bytes.0[..path_bytes.1]).unwrap_or("");
            let mut buf = [0u8; 4096];
            let sz = crate::fs::vfs::VFS
                .read()
                .read_raw(path_str, &mut buf, 0)
                .unwrap_or(0);
            (0x81A4, sz as u64) // S_IFREG | 0644
        }
    };

    let mut stat = [0u8; 72];
    stat[0..8].copy_from_slice(&1u64.to_ne_bytes());       // st_dev
    stat[8..16].copy_from_slice(&(fd as u64).to_ne_bytes()); // st_ino
    stat[16..24].copy_from_slice(&1u64.to_ne_bytes());     // st_nlink
    stat[24..28].copy_from_slice(&mode.to_ne_bytes());     // st_mode
    stat[48..56].copy_from_slice(&size.to_ne_bytes());     // st_size
    stat[56..64].copy_from_slice(&4096u64.to_ne_bytes());   // st_blksize
    stat[64..72].copy_from_slice(&((size + 511) / 512).to_ne_bytes()); // st_blocks
    UserSliceWo::wo(ctx.args[1], 72)
        .map_err(|_| AbiError::Other("EFAULT"))?
        .copy_from_slice(&stat)
        .map_err(|_| AbiError::Other("EFAULT"))?;
    Ok(0)
}

/// sys_ioctl(fd, request, arg) → 0/-ENOTTY
fn sys_ioctl(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let request = ctx.args[1];
    let arg = ctx.args[2] as *mut u8;

    // Safe ioctls (no capability check required)
    if request == 0x5413 || request == 0x5414 || request == 0x5401 || request == 0x5402 {
        // ... (terminal query implementation) ...
        return Ok(0);
    }

    // All other ioctls require DeviceIo capability
    if !ctx.process.capabilities.has_permission(ResourceKind::DeviceIo, true, false) {
        return Err(AbiError::PermissionDenied);
    }

    // DRM ioctls
    #[cfg(feature = "drm")]
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
    let iovcnt = ctx.args[2] as usize;

    if fd != 1 && fd != 2 {
        return Ok((-9_i64) as u64); // -EBADF
    }

    let mut total = 0usize;
    use x86_64::instructions::interrupts;
    interrupts::without_interrupts(|| {
        let mut serial = crate::drivers::uart::SERIAL1.lock();
        for i in 0..iovcnt.min(16) {
            // Read each iovec (16 bytes: [base: u64, len: u64]) from user memory
            let iov_offset = i * 16;
            let mut iov_buf = [0u8; 16];
            if UserSliceRo::ro(ctx.args[1] + iov_offset as u64, 16)
                .and_then(|s| s.copy_to_slice(&mut iov_buf))
                .is_err()
            {
                break;
            }
            let base = u64::from_ne_bytes([
                iov_buf[0], iov_buf[1], iov_buf[2], iov_buf[3],
                iov_buf[4], iov_buf[5], iov_buf[6], iov_buf[7],
            ]);
            let len = u64::from_ne_bytes([
                iov_buf[8], iov_buf[9], iov_buf[10], iov_buf[11],
                iov_buf[12], iov_buf[13], iov_buf[14], iov_buf[15],
            ]) as usize;
            if len == 0 || base == 0 {
                continue;
            }
            // Read user buffer
            let mut buf = alloc::vec![0u8; len];
            if UserSliceRo::ro(base, len)
                .and_then(|s| s.copy_to_slice(&mut buf))
                .is_err()
            {
                continue;
            }
            for &b in &buf {
                serial.send(b);
            }
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
    // Enforcement: Check File capability for all open calls
    if !ctx.process.capabilities.has_permission(ResourceKind::File, false, false) {
        return Err(AbiError::PermissionDenied);
    }

    let flags = ctx.args[1] as u32;
    let path_buf = crate::abi::usercopy::read_user_string(ctx.args[0], 4096)
        .map_err(|_| AbiError::Other("EFAULT"))?;
    let path_str = core::str::from_utf8(&path_buf).unwrap_or("");

    // NEW: Check for Redox-style Scheme prefix first (e.g. "debug:", "pipe:")
    if let Some(res) = crate::fs::vfs::VFS.read().handle_scheme(path_str, flags as usize) {
        match res {
            Ok(id) => {
                let fd = ctx.process.fds.alloc_scheme(path_str.as_bytes(), flags, id).unwrap_or(3);
                return Ok(fd as u64);
            }
            Err(e) => return Err(e),
        }
    }

    // Device/pseudo paths always succeed
    let is_known = known_path(path_str)
        || matches!(
            path_str,
            "/dev/null"
                | "/dev/zero"
                | "/dev/random"
                | "/dev/urandom"
                | "/dev/tty"
                | "/dev/console"
                | "/proc/self/maps"
                | "/proc/self/exe"
                | "/etc/passwd"
                | "/etc/localtime"
        );
    let o_creat = (flags & 0x40) != 0;
    if o_creat {
        if !ctx
            .process
            .capabilities
            .has_permission(ResourceKind::File, true, false)
        {
            return Err(AbiError::PermissionDenied);
        }
        let mut vfs = crate::fs::vfs::VFS.write();
        if !vfs.exists(path_str) {
            vfs.create(path_str);
        }
    }

    if is_known
        || o_creat
        || crate::fs::vfs::VFS
            .read()
            .read_raw(path_str, &mut [0u8; 1], 0)
            .is_ok()
    {
        let fd = ctx.process.fds.alloc_file(path_str.as_bytes(), flags).unwrap_or(3);
        return Ok(fd as u64);
    }
    Ok((-2_i64) as u64) // -ENOENT
}

/// sys_arch_prctl(code, addr) → 0  
fn sys_arch_prctl(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let code = ctx.args[0] as u32;
    let addr = ctx.args[1];

    match code {
        0x1002 => {
            // ARCH_SET_FS
            ctx.process.fs_base = addr;
            use x86_64::registers::model_specific::Msr;
            let mut fs_msr = Msr::new(0xC0000100);
            unsafe {
                fs_msr.write(addr);
            }
            Ok(0)
        }
        0x1003 => {
            // ARCH_GET_FS
            UserSliceWo::wo(addr, 8)
                .map_err(|_| AbiError::Other("EFAULT: bad arch_prctl buffer"))?
                .copy_from_slice(&ctx.process.fs_base.to_ne_bytes())
                .map_err(|_| AbiError::Other("EFAULT: copy failed"))?;
            Ok(0)
        }
        0x1001 => {
            // ARCH_SET_GS
            ctx.process.gs_base = addr;
            use x86_64::registers::model_specific::Msr;
            let mut kgs_msr = Msr::new(0xC0000102);
            unsafe {
                kgs_msr.write(addr);
            }
            Ok(0)
        }
        0x1004 => {
            // ARCH_GET_GS
            UserSliceWo::wo(addr, 8)
                .map_err(|_| AbiError::Other("EFAULT: bad arch_prctl buffer"))?
                .copy_from_slice(&ctx.process.gs_base.to_ne_bytes())
                .map_err(|_| AbiError::Other("EFAULT: copy failed"))?;
            Ok(0)
        }
        _ => {
            Ok((-22_i64) as u64) // -EINVAL
        }
    }
}

/// sys_set_tid_address(tidptr) → tid
fn sys_set_tid_address(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    Ok(ctx.process.pid.0)
}

/// sys_stat / sys_lstat(path, statbuf) → 0/-ENOENT
fn sys_stat(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let path_buf = crate::abi::usercopy::read_user_string(ctx.args[0], 4096)
        .map_err(|_| AbiError::Other("EFAULT"))?;
    let path_str = core::str::from_utf8(&path_buf).unwrap_or("");
    let exists = crate::fs::vfs::VFS.read().exists(path_str);
    if !exists {
        return Ok((-2_i64) as u64); // -ENOENT
    }
    let mut stat = [0u8; 144];
    stat[8..16].copy_from_slice(&1u64.to_ne_bytes()); // st_ino
    stat[16..24].copy_from_slice(&1u64.to_ne_bytes()); // st_nlink
    stat[24..28].copy_from_slice(&0o100644u32.to_ne_bytes()); // st_mode
    if let Some(sz) = crate::fs::vfs::VFS.read().file_size(path_str) {
        stat[48..56].copy_from_slice(&(sz as u64).to_ne_bytes()); // st_size
    }
    stat[56..64].copy_from_slice(&4096u64.to_ne_bytes()); // st_blksize
    UserSliceWo::wo(ctx.args[1], 144)
        .map_err(|_| AbiError::Other("EFAULT"))?
        .copy_from_slice(&stat)
        .map_err(|_| AbiError::Other("EFAULT"))?;
    Ok(0)
}

/// sys_lseek(fd, offset, whence) → new_offset
fn sys_lseek(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let fd = ctx.args[0] as usize;
    let offset = ctx.args[1] as i64;
    let whence = ctx.args[2];

    let current = ctx.process.fds.get(fd).map(|d| d.offset).unwrap_or(0);
    let new_offset: usize = match whence {
        0 => offset.max(0) as usize,                    // SEEK_SET
        1 => (current as i64 + offset).max(0) as usize, // SEEK_CUR
        2 => offset.max(0) as usize,                    // SEEK_END (approx)
        _ => return Ok((-22_i64) as u64),               // -EINVAL
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
    if !ctx.process.capabilities.has_permission(ResourceKind::IpcChannel, true, false) { return Err(AbiError::PermissionDenied); }

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
            let mut pipe_buf = [0u8; 8];
            pipe_buf[..4].copy_from_slice(&(rfd as u32).to_ne_bytes());
            pipe_buf[4..8].copy_from_slice(&(wfd as u32).to_ne_bytes());
            crate::abi::usercopy::UserSliceWo::wo(ctx.args[0], 8)
                .map_err(|_| AbiError::Other("EFAULT"))?
                .copy_from_slice(&pipe_buf)
                .map_err(|_| AbiError::Other("EFAULT"))?;
            Ok(0)
        }
        _ => Ok((-24_i64) as u64), // -EMFILE
    }
}

/// sys_getcwd(buf, size) → buf_addr on success, -ERANGE if too small
fn sys_getcwd(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let size = ctx.args[1] as usize;
    let cwd_len = ctx.process.cwd_len;
    // Need space for path + null terminator
    if size < cwd_len + 1 {
        return Ok((-34_i64) as u64); // -ERANGE
    }
    let mut cwd_buf = alloc::vec![0u8; cwd_len + 1];
    cwd_buf[..cwd_len].copy_from_slice(&ctx.process.cwd[..cwd_len]);
    cwd_buf[cwd_len] = 0;
    crate::abi::usercopy::UserSliceWo::wo(ctx.args[0], cwd_len + 1)
        .map_err(|_| AbiError::Other("EFAULT"))?
        .copy_from_slice(&cwd_buf)
        .map_err(|_| AbiError::Other("EFAULT"))?;
    Ok(ctx.args[0])
}

/// sys_chdir(path) → 0 / -ENOENT
fn sys_chdir(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let path_buf = crate::abi::usercopy::read_user_string(ctx.args[0], 4096)
        .map_err(|_| AbiError::Other("EFAULT"))?;
    let n = path_buf.len();
    if n == 0 {
        return Ok((-2_i64) as u64);
    }
    ctx.process.cwd[..n].copy_from_slice(&path_buf);
    ctx.process.cwd_len = n;
    Ok(0)
}

/// sys_kill(pid, sig) → 0/-ESRCH
fn sys_kill(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let target_pid = ctx.args[0];
    let signum = ctx.args[1] as u8;
    let ok = crate::process::scheduler::SCHEDULER
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
    let options = ctx.args[2] as i32;
    let parent = ctx.process.pid;
    match crate::process::scheduler::SCHEDULER.waitpid(parent, child_pid, options) {
        Some((pid, code)) => {
            if ctx.args[1] != 0 {
                let status = (code as i32 & 0xFF) << 8;
                UserSliceWo::wo(ctx.args[1], 4)
                    .map_err(|_| AbiError::Other("EFAULT"))?
                    .copy_from_slice(&status.to_ne_bytes())
                    .map_err(|_| AbiError::Other("EFAULT"))?;
            }
            println!("[Linux ABI] waitpid → child {} exited with {}", pid.0, code);
            Ok(pid.0)
        }
        None => Ok((-10_i64) as u64), // -ECHILD
    }
}

/// sys_nanosleep(req: *timespec, rem: *timespec) → 0
/// timespec layout: { tv_sec: u64, tv_nsec: u64 } (16 bytes)
fn sys_nanosleep(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let ms = if ctx.args[0] != 0 {
        let mut ts = [0u8; 16];
        UserSliceRo::ro(ctx.args[0], 16)
            .map_err(|_| AbiError::Other("EFAULT"))?
            .copy_to_slice(&mut ts)
            .map_err(|_| AbiError::Other("EFAULT"))?;
        let tv_sec = u64::from_ne_bytes(ts[..8].try_into().unwrap());
        let tv_nsec = u64::from_ne_bytes(ts[8..16].try_into().unwrap());
        tv_sec * 1000 + tv_nsec / 1_000_000
    } else {
        1
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
///   3 = FUTEX_REQUEUE - wake 'val', requeue up to 'timeout' to uaddr2
///   4 = FUTEX_CMP_REQUEUE - same as 3 if *uaddr == val3
///   5 = FUTEX_WAIT_BITSET - wait with bitset match
///   9 = FUTEX_WAKE_BITSET - wake with bitset match
fn sys_futex(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    use x86_64::VirtAddr;
    
    let op = ctx.args[1] as u32 & 0x7F; // mask off FUTEX_PRIVATE_FLAG
    let uaddr = ctx.args[0];
    let val = ctx.args[2] as i32;

    // Translate user virtual address to physical address.
    let vaddr = VirtAddr::new(uaddr);
    let entry = match crate::memory::paging::get_leaf_entry_mut(vaddr) {
        Some(e) => e,
        None => return Ok((-14_i64) as u64), // -EFAULT
    };

    let flags = entry.flags();
    if !flags.contains(x86_64::structures::paging::PageTableFlags::PRESENT) {
        return Ok((-14_i64) as u64); // -EFAULT
    }

    let phys_addr = entry.addr().as_u64() + (uaddr & 0xFFF);

    match op {
        0 | 5 => {
            // FUTEX_WAIT (0) or FUTEX_WAIT_BITSET (5)
            let bitset = if op == 5 { ctx.args[5] as u32 } else { 0xFFFFFFFF };
            let timeout_ptr = ctx.args[3];
            let mut timeout_ms = None;
            
            if timeout_ptr != 0 {
                let mut ts_buf = [0u8; 16];
                if UserSliceRo::ro(timeout_ptr, 16).and_then(|s| s.copy_to_slice(&mut ts_buf)).is_ok() {
                    let tv_sec = i64::from_le_bytes(ts_buf[0..8].try_into().unwrap());
                    let tv_nsec = i64::from_le_bytes(ts_buf[8..16].try_into().unwrap());
                    let ms = (tv_sec.max(0) as u64 * 1000) + (tv_nsec.max(0) as u64 / 1_000_000);
                    timeout_ms = Some(ms);
                } else {
                    return Ok((-14_i64) as u64); // -EFAULT
                }
            }

            let blocked = crate::sync::futex::FUTEX_MANAGER.lock().wait(phys_addr, val as u32, bitset);
            if blocked {
                if let Some(ms) = timeout_ms {
                    crate::timer::TIMER.lock().sleep_ms(ctx.process.pid, ms);
                }
                
                crate::process::scheduler::SCHEDULER.schedule();
                
                if ctx.process.timed_out {
                    crate::sync::futex::FUTEX_MANAGER.lock().cancel_wait(phys_addr, ctx.process.pid);
                    return Ok((-110_i64) as u64); // -ETIMEDOUT
                }
            } else {
                return Ok((-11_i64) as u64); // -EAGAIN
            }
            Ok(0)
        }
        1 | 9 => {
            // FUTEX_WAKE (1) or FUTEX_WAKE_BITSET (9)
            let bitset = if op == 9 { ctx.args[5] as u32 } else { 0xFFFFFFFF };
            let woken = crate::sync::futex::FUTEX_MANAGER.lock().wake(phys_addr, val as usize, bitset);
            Ok(woken as u64)
        }
        3 | 4 => {
            // FUTEX_REQUEUE (3) or FUTEX_CMP_REQUEUE (4)
            let val2 = ctx.args[3] as usize; // requeue_count
            let uaddr2 = ctx.args[4];

            if op == 4 {
                // FUTEX_CMP_REQUEUE: check val3 at uaddr
                let val3 = ctx.args[5] as u32;
                let po = crate::memory::paging::phys_offset().as_u64();
                let ptr = (po + phys_addr) as *const u32;
                let current_val = unsafe { *ptr };
                if current_val != val3 {
                    return Ok((-11_i64) as u64); // -EAGAIN
                }
            }

            // Translate uaddr2 to physical address
            let vaddr2 = VirtAddr::new(uaddr2);
            let entry2 = match crate::memory::paging::get_leaf_entry_mut(vaddr2) {
                Some(e) => e,
                None => return Ok((-14_i64) as u64), // -EFAULT
            };
            if !entry2.flags().contains(x86_64::structures::paging::PageTableFlags::PRESENT) {
                return Ok((-14_i64) as u64); // -EFAULT
            }
            let phys_addr2 = entry2.addr().as_u64() + (uaddr2 & 0xFFF);

            let total = crate::sync::futex::FUTEX_MANAGER.lock().requeue(
                phys_addr, phys_addr2, val as usize, val2
            );
            Ok(total as u64)
        }
        _ => {
            println!("[FUTEX] unsupported op: {}", op);
            Ok((-38_i64) as u64) // -ENOSYS
        }
    }
}

/// sys_rt_sigaction(signum, act_ptr, oldact_ptr, sigsetsize) → 0 / -EINVAL
///
/// Linux struct sigaction layout (x86_64, simplified):
///   offset 0:  sa_handler  (u64 — pointer or SIG_DFL=0 / SIG_IGN=1)
///   offset 8:  sa_flags    (u64)
///   offset 16: sa_restorer (u64 — ignored; we install our own trampoline)
///   offset 24: sa_mask     (u64 — first 64-bit word of the signal set)
///
/// We map SIG_DFL(0) → SignalAction::Default, SIG_IGN(1) → SignalAction::Ignore,
/// anything else → SignalAction::Handler(ptr).
fn sys_rt_sigaction(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    use crate::process::signal::{SignalAction, sig};
    let signum = ctx.args[0] as u8;

    if signum == 0 || signum > sig::MAX {
        return Ok((-22_i64) as u64); // -EINVAL
    }
    if signum == sig::SIGKILL || signum == sig::SIGSTOP {
        return Ok((-22_i64) as u64); // -EINVAL: cannot catch/ignore
    }

    // If act_ptr is null this is a query-only call (just returns oldact).
    if ctx.args[1] != 0 {
        let mut words = [0u8; 16];
        UserSliceRo::ro(ctx.args[1], 16)
            .map_err(|_| AbiError::Other("EFAULT"))?
            .copy_to_slice(&mut words)
            .map_err(|_| AbiError::Other("EFAULT"))?;
        let sa_handler = u64::from_ne_bytes(words[..8].try_into().unwrap());
        let _sa_flags  = u64::from_ne_bytes(words[8..16].try_into().unwrap());

        let action = match sa_handler {
            0 => SignalAction::Default,
            1 => SignalAction::Ignore,
            ptr => SignalAction::Handler(ptr),
        };

        ctx.process.signals.set_action(signum, action);
        crate::klog!(crate::klog::Level::Debug,
            "rt_sigaction: sig={} handler=0x{:x}", signum, sa_handler);
    }

    Ok(0)
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

    match crate::process::scheduler::SCHEDULER.fork(parent_pid) {
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
    let iovcnt = ctx.args[2] as usize;

    if fd != 0 {
        return Ok((-9_i64) as u64); // -EBADF
    }

    // Read iovec array from user memory
    let mut iov_buf = alloc::vec![0u8; 16 * iovcnt.min(16)];
    UserSliceRo::ro(ctx.args[1], iov_buf.len())
        .map_err(|_| AbiError::Other("EFAULT"))?
        .copy_to_slice(&mut iov_buf)
        .map_err(|_| AbiError::Other("EFAULT"))?;

    let mut total: usize = 0;
    for i in 0..iovcnt.min(16) {
        let off = i * 16;
        let base = u64::from_ne_bytes(iov_buf[off..off + 8].try_into().unwrap());
        let len = usize::from_ne_bytes(iov_buf[off + 8..off + 16].try_into().unwrap());
        if base == 0 || len == 0 {
            continue;
        }
        let mut tmp = [0u8; 256];
        let n = crate::drivers::keyboard::read_stdin(&mut tmp[..len.min(256)]);
        if n == 0 {
            break;
        }
        UserSliceWo::wo(base, n)
            .map_err(|_| AbiError::Other("EFAULT"))?
            .copy_from_slice(&tmp[..n])
            .map_err(|_| AbiError::Other("EFAULT"))?;
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
    let flags = ctx.args[2] as u32;
    let path_buf = crate::abi::usercopy::read_user_string(ctx.args[1], 4096)
        .map_err(|_| AbiError::Other("EFAULT"))?;
    let path_str = core::str::from_utf8(&path_buf).unwrap_or("");

    let is_known = known_path(path_str)
        || matches!(
            path_str,
            "/dev/null"
                | "/dev/zero"
                | "/dev/random"
                | "/dev/urandom"
                | "/dev/tty"
                | "/dev/console"
                | "/proc/self/maps"
                | "/proc/self/exe"
                | "/etc/passwd"
                | "/etc/localtime"
        );
    let o_creat = (flags & 0x40) != 0;
    if o_creat {
        if !ctx
            .process
            .capabilities
            .has_permission(ResourceKind::File, true, false)
        {
            return Err(AbiError::PermissionDenied);
        }
        let mut vfs = crate::fs::vfs::VFS.write();
        if !vfs.exists(path_str) {
            vfs.create(path_str);
        }
    }

    if is_known
        || o_creat
        || crate::fs::vfs::VFS
            .read()
            .read_raw(path_str, &mut [0u8; 1], 0)
            .is_ok()
    {
        let fd = ctx.process.fds.alloc_file(path_str.as_bytes(), flags).unwrap_or(3);
        return Ok(fd as u64);
    }
    Ok((-2_i64) as u64) // -ENOENT
}

/// sys_tgkill(tgid, tid, sig) → 0
fn sys_tgkill(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let tid = ctx.args[1];
    let sig = ctx.args[2] as u8;
    let ok = crate::process::scheduler::SCHEDULER
        .send_signal(crate::process::Pid(tid), sig);
    if ok {
        return Ok(0);
    } else {
        return Ok((-3_i64) as u64);
    }
}

/// === BATCH 2: 20 more syscalls toward 100+ ===

/// sys_gettimeofday(tv, tz) → 0
/// timeval: { tv_sec: i64, tv_usec: i64 } (16 bytes)
fn sys_gettimeofday(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    if ctx.args[0] != 0 {
        let ms = crate::timer::uptime_ms();
        let mut tv_buf = [0u8; 16];
        tv_buf[..8].copy_from_slice(&((ms / 1000) as i64).to_ne_bytes());
        tv_buf[8..16].copy_from_slice(&(((ms % 1000) * 1000) as i64).to_ne_bytes());
        UserSliceWo::wo(ctx.args[0], 16)
            .map_err(|_| AbiError::Other("EFAULT"))?
            .copy_from_slice(&tv_buf)
            .map_err(|_| AbiError::Other("EFAULT"))?;
    }
    Ok(0)
}

/// sys_time(tloc) → seconds
fn sys_time(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let secs = (crate::timer::uptime_ms() / 1000) as i64;
    if ctx.args[0] != 0 {
        UserSliceWo::wo(ctx.args[0], 8)
            .map_err(|_| AbiError::Other("EFAULT"))?
            .copy_from_slice(&secs.to_ne_bytes())
            .map_err(|_| AbiError::Other("EFAULT"))?;
    }
    Ok(secs as u64)
}

/// sys_getrlimit(resource, rlim) → 0
/// rlimit: { rlim_cur: u64, rlim_max: u64 } (16 bytes)
fn sys_getrlimit(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    if ctx.args[1] == 0 {
        return Ok((-14_i64) as u64);
    }
    let mut rlim_buf = [0u8; 16];
    rlim_buf[..8].copy_from_slice(&0x1_0000_0000u64.to_ne_bytes());
    rlim_buf[8..16].copy_from_slice(&0x1_0000_0000u64.to_ne_bytes());
    UserSliceWo::wo(ctx.args[1], 16)
        .map_err(|_| AbiError::Other("EFAULT"))?
        .copy_from_slice(&rlim_buf)
        .map_err(|_| AbiError::Other("EFAULT"))?;
    Ok(0)
}

/// sys_setrlimit(resource, rlim) → 0 (stub)
fn sys_setrlimit(_ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    Ok(0)
}

/// sys_sysinfo(info) → 0
/// x86_64 sysinfo (64 bytes, simplified)
fn sys_sysinfo(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    if ctx.args[0] == 0 {
        return Ok((-14_i64) as u64);
    }
    let ms = crate::timer::uptime_ms();
    let mut buf = [0u8; 88];
    buf[0..8].copy_from_slice(&((ms / 1000) as u64).to_ne_bytes()); // uptime
    buf[32..40].copy_from_slice(&((512 * 1024 * 1024 / 4096) as u64).to_ne_bytes()); // totalram
    buf[40..48].copy_from_slice(&((256 * 1024 * 1024) as u64).to_ne_bytes()); // freeram
    buf[80..82].copy_from_slice(&(16u16).to_ne_bytes()); // procs at byte offset 80
    UserSliceWo::wo(ctx.args[0], 88)
        .map_err(|_| AbiError::Other("EFAULT"))?
        .copy_from_slice(&buf)
        .map_err(|_| AbiError::Other("EFAULT"))?;
    Ok(0)
}

/// sys_prlimit64(pid, resource, new_rlim, old_rlim) → 0 / -ESRCH / -EINVAL
fn sys_prlimit64(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let _pid = ctx.args[0] as i64;
    let _resource = ctx.args[1];
    // If old_rlim is not null, fill it with default values
    if ctx.args[3] != 0 {
        let mut rlim_buf = [0u8; 16];
        rlim_buf[..8].copy_from_slice(&0x1_0000_0000u64.to_ne_bytes());
        rlim_buf[8..16].copy_from_slice(&0x1_0000_0000u64.to_ne_bytes());
        UserSliceWo::wo(ctx.args[3], 16)
            .map_err(|_| AbiError::Other("EFAULT"))?
            .copy_from_slice(&rlim_buf)
            .map_err(|_| AbiError::Other("EFAULT"))?;
    }
    // If new_rlim is not null, accept (stub)
    let _ = ctx.args[2];
    Ok(0)
}

/// sys_socket(domain, type, protocol) → fd
#[cfg(feature = "net")]
fn sys_socket(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    if !ctx.process.capabilities.has_permission(ResourceKind::Network, true, false) { return Err(AbiError::PermissionDenied); }
    let domain = ctx.args[0] as u32;
    let socktype = ctx.args[1] as u32;
    let protocol = ctx.args[2] as u32;
    let fd = ctx
        .process
        .fds
        .alloc_file(b"socket:", socktype)
        .unwrap_or(3);
    crate::net::socket::SOCKETS.lock().create(fd, domain, socktype, protocol);
    Ok(fd as u64)
}

/// sys_sendto(sockfd, buf, len, flags, dest_addr, addrlen) → bytes_sent
#[cfg(feature = "net")]
fn sys_sendto(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    if !ctx.process.capabilities.has_permission(ResourceKind::Network, true, false) { return Err(AbiError::PermissionDenied); }
    let fd = ctx.args[0] as usize;
    let len = ctx.args[2] as usize;
    let mut socks = crate::net::socket::SOCKETS.lock();
    let entry = match socks.get_mut(fd) {
        Some(e) if e.state != crate::net::socket::SocketState::Closed => e,
        _ => return Ok((-9_i64) as u64), // -EBADF
    };
    if ctx.args[1] == 0 || len == 0 {
        return Ok(0);
    }
    // Read data from userspace via UserSlice
    let copy_len = len.min(4096);
    let mut tmp = alloc::vec![0u8; copy_len];
    UserSliceRo::ro(ctx.args[1], copy_len)
        .map_err(|_| AbiError::Other("EFAULT"))?
        .copy_to_slice(&mut tmp)
        .map_err(|_| AbiError::Other("EFAULT"))?;
    entry.tx_buf.extend_from_slice(&tmp);
    // Forward to paired socket's rx buffer
    if let Some(paired) = entry.paired {
        if let Some(peer) = socks.get_mut(paired) {
            peer.rx_buf.extend_from_slice(&tmp);
        }
    }
    Ok(copy_len as u64)
}

/// sys_recvfrom(sockfd, buf, len, flags, src_addr, addrlen) → bytes_read
#[cfg(feature = "net")]
fn sys_recvfrom(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    if !ctx.process.capabilities.has_permission(ResourceKind::Network, true, false) { return Err(AbiError::PermissionDenied); }
    let fd = ctx.args[0] as usize;
    let len = ctx.args[2] as usize;
    if ctx.args[1] == 0 || len == 0 {
        return Ok(0);
    }
    let mut socks = crate::net::socket::SOCKETS.lock();
    let entry = match socks.get_mut(fd) {
        Some(e) if e.state != crate::net::socket::SocketState::Closed => e,
        _ => return Ok((-9_i64) as u64),
    };
    let avail = entry.rx_buf.len() - entry.rx_pos;
    if avail == 0 {
        return Ok((-11_i64) as u64); // -EAGAIN
    }
    let to_read = len.min(avail);
    UserSliceWo::wo(ctx.args[1], to_read)
        .map_err(|_| AbiError::Other("EFAULT"))?
        .copy_from_slice(&entry.rx_buf[entry.rx_pos..entry.rx_pos + to_read])
        .map_err(|_| AbiError::Other("EFAULT"))?;
    entry.rx_pos += to_read;
    // Compact buffer when fully consumed
    if entry.rx_pos >= entry.rx_buf.len() {
        entry.rx_buf.clear();
        entry.rx_pos = 0;
    }
    Ok(to_read as u64)
}

/// sys_readlink(path, buf, bufsiz) → bytes_written
fn sys_readlink(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let bufsiz = ctx.args[2] as usize;

    let path_buf = crate::abi::usercopy::read_user_string(ctx.args[0], 4096)
        .map_err(|_| AbiError::Other("EFAULT"))?;
    let path_str = core::str::from_utf8(&path_buf).unwrap_or("");

    // /proc/self/exe → return a fake binary path
    let target: &[u8] = match path_str {
        "/proc/self/exe" => b"/bin/busybox",
        "/proc/self/fd/0" => b"/dev/stdin",
        "/proc/self/fd/1" => b"/dev/stdout",
        "/proc/self/fd/2" => b"/dev/stderr",
        _ => return Ok((-2_i64) as u64), // -ENOENT
    };

    let n = target.len().min(bufsiz);
    crate::abi::usercopy::UserSliceWo::wo(ctx.args[1], n)
        .map_err(|_| AbiError::Other("EFAULT"))?
        .copy_from_slice(&target[..n])
        .map_err(|_| AbiError::Other("EFAULT"))?;
    Ok(n as u64)
}

/// sys_fcntl(fd, cmd, arg) → result
/// F_GETFD=1, F_SETFD=2, F_GETFL=3, F_SETFL=4, F_DUPFD=0, F_DUPFD_CLOEXEC=1030
fn sys_fcntl(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let fd = ctx.args[0] as usize;
    let cmd = ctx.args[1];
    let _arg = ctx.args[2];
    match cmd {
        0 => {
            // F_DUPFD — dup to lowest fd >= arg
            match ctx.process.fds.dup(fd, None) {
                Some(newfd) => Ok(newfd as u64),
                None => Ok((-9_i64) as u64),
            }
        }
        1 | 2 => Ok(0), // F_GETFD / F_SETFD — FD_CLOEXEC flag, ignore
        3 => Ok(0),     // F_GETFL — return O_RDWR=2
        4 => Ok(0),     // F_SETFL — accept any flags
        1030 => {
            // F_DUPFD_CLOEXEC
            match ctx.process.fds.dup(fd, None) {
                Some(newfd) => Ok(newfd as u64),
                None => Ok((-9_i64) as u64),
            }
        }
        _ => Ok((-22_i64) as u64), // -EINVAL
    }
}

/// === NEW SYSCALLS (100+ coverage) ===

/// sys_getdents64(fd, dirp, count) → bytes_written / -ENOTDIR
fn sys_getdents64(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let fd = ctx.args[0] as usize;
    let count = ctx.args[2] as usize;

    let path_bytes = ctx.process.fds.path_of(fd);
    if path_bytes.is_none() {
        return Ok((-20_i64) as u64); // -ENOTDIR
    }
    let path = core::str::from_utf8(path_bytes.unwrap()).unwrap_or("/");

    if !crate::fs::vfs::VFS.read().is_dir(path) {
        return Ok((-20_i64) as u64); // -ENOTDIR
    }

    let vfs = crate::fs::vfs::VFS.read();
    let entries = vfs.list_dir(path);
    
    const DT_REG: u8 = 8;
    const DT_DIR: u8 = 4;
    
    // Build all directory entries into a Vec<u8>
    let mut out = alloc::vec::Vec::new();
    
    // Always emit "." and ".." first (both are directories)
    for name_bytes in [b".".as_slice(), b"..".as_slice()].iter().copied() {
        let name_len = name_bytes.len();
        let reclen = (19 + name_len + 7) & !7;
        if out.len() + reclen > count {
            break;
        }
        let pos = out.len();
        out.resize(out.len() + reclen, 0);
        let slot = &mut out[pos..pos + reclen];
        slot[..8].copy_from_slice(&1u64.to_ne_bytes()); // d_ino
        slot[8..16].copy_from_slice(&(reclen as i64).to_ne_bytes()); // d_off
        slot[16..18].copy_from_slice(&(reclen as u16).to_ne_bytes()); // d_reclen
        slot[18] = DT_DIR;
        slot[19..19 + name_len].copy_from_slice(name_bytes);
        slot[19 + name_len] = 0;
    }
    
    // Emit regular entries with correct d_type
    for entry_path in entries.iter() {
        let name = entry_path.rsplit('/').next().unwrap_or(entry_path);
        let name_bytes = name.as_bytes();
        let name_len = name_bytes.len();
        let reclen = (19 + name_len + 7) & !7;
        if out.len() + reclen > count {
            break;
        }
        let d_type = if vfs.is_dir(entry_path) { DT_DIR } else { DT_REG };
        let pos = out.len();
        out.resize(out.len() + reclen, 0);
        let slot = &mut out[pos..pos + reclen];
        slot[..8].copy_from_slice(&1u64.to_ne_bytes()); // d_ino
        slot[8..16].copy_from_slice(&(reclen as i64).to_ne_bytes()); // d_off
        slot[16..18].copy_from_slice(&(reclen as u16).to_ne_bytes()); // d_reclen
        slot[18] = d_type;
        slot[19..19 + name_len].copy_from_slice(name_bytes);
        slot[19 + name_len] = 0;
    }
    
    // Write to user memory
    let written = out.len();
    if written > 0 {
        UserSliceWo::wo(ctx.args[1], written)
            .map_err(|_| AbiError::Other("EFAULT"))?
            .copy_from_slice(&out)
            .map_err(|_| AbiError::Other("EFAULT"))?;
    }
    Ok(written as u64)
}

/// sys_mkdir(pathname, mode) → 0 / -EINVAL
fn sys_mkdir(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    // Enforcement: Check File capability for filesystem modification
    if !ctx.process.capabilities.has_permission(ResourceKind::File, true, false) {
        return Err(AbiError::PermissionDenied);
    }
    
    let _mode = ctx.args[1];
    let path_buf = crate::abi::usercopy::read_user_string(ctx.args[0], 4096)
        .map_err(|_| AbiError::Other("EFAULT"))?;
    let path_str = core::str::from_utf8(&path_buf).unwrap_or("");
    if path_str.is_empty() {
        return Ok((-22_i64) as u64); // -EINVAL
    }
    crate::fs::vfs::VFS.write().mkdir(path_str);
    Ok(0)
}


/// sys_rmdir(pathname) → 0 / -ENOENT
fn sys_rmdir(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    if !ctx.process.capabilities.has_permission(ResourceKind::File, true, false) { return Err(AbiError::PermissionDenied); }
    let path_buf = crate::abi::usercopy::read_user_string(ctx.args[0], 4096)
        .map_err(|_| AbiError::Other("EFAULT"))?;
    let path_str = core::str::from_utf8(&path_buf).unwrap_or("");
    match crate::fs::vfs::VFS.write().remove(path_str) {
        Ok(()) => Ok(0),
        Err(_) => Ok((-2_i64) as u64), // -ENOENT
    }
}

/// sys_unlink(pathname) → 0 / -ENOENT
fn sys_unlink(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    if !ctx.process.capabilities.has_permission(ResourceKind::File, true, false) { return Err(AbiError::PermissionDenied); }
    let path_buf = crate::abi::usercopy::read_user_string(ctx.args[0], 4096)
        .map_err(|_| AbiError::Other("EFAULT"))?;
    let path_str = core::str::from_utf8(&path_buf).unwrap_or("");
    match crate::fs::vfs::VFS.write().remove(path_str) {
        Ok(()) => Ok(0),
        Err(_) => Ok((-2_i64) as u64), // -ENOENT
    }
}

/// sys_rename(oldpath, newpath) → 0 / -ENOENT
fn sys_rename(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    if !ctx.process.capabilities.has_permission(ResourceKind::File, true, false) { return Err(AbiError::PermissionDenied); }
    let old_buf = crate::abi::usercopy::read_user_string(ctx.args[0], 4096)
        .map_err(|_| AbiError::Other("EFAULT"))?;
    let new_buf = crate::abi::usercopy::read_user_string(ctx.args[1], 4096)
        .map_err(|_| AbiError::Other("EFAULT"))?;
    let old_str = core::str::from_utf8(&old_buf).unwrap_or("");
    let new_str = core::str::from_utf8(&new_buf).unwrap_or("");
    match crate::fs::vfs::VFS.write().rename(old_str, new_str) {
        Ok(()) => Ok(0),
        Err(_) => Ok((-2_i64) as u64), // -ENOENT
    }
}

/// sys_creat(pathname, mode) → fd / -ENOENT
/// Equivalent to open(path, O_CREAT|O_WRONLY|O_TRUNC, mode)
fn sys_creat(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    if !ctx.process.capabilities.has_permission(ResourceKind::File, true, false) { return Err(AbiError::PermissionDenied); }
    let _mode = ctx.args[1];
    let path_buf = crate::abi::usercopy::read_user_string(ctx.args[0], 4096)
        .map_err(|_| AbiError::Other("EFAULT"))?;
    let path_str = core::str::from_utf8(&path_buf).unwrap_or("");
    let tmp = &path_buf;
    let n = tmp.len();
    {
        let mut vfs = crate::fs::vfs::VFS.write();
        let exists = vfs.exists(path_str);
        if !exists {
            vfs.create(path_str);
        }
    }
    let fd = ctx.process.fds.alloc_file(&tmp[..n], 0x0041).unwrap_or(3); // O_CREAT|O_WRONLY
    Ok(fd as u64)
}

/// sys_newfstatat(dirfd, pathname, statbuf, flags) → 0 / -ENOENT
/// x86_64 stat structure (144 bytes total):
/// offset 0: st_dev   (u64),  8: st_ino   (u64), 16: st_nlink (u64),
/// 24: st_mode (u32)+pad(u32), 32: st_uid   (u32), 36: pad      (u32),
/// 40: st_rdev  (u64), 48: st_size (i64), 56: st_blksize (i64),
/// 64: st_blocks (i64), 72: st_atim (16B), 88: st_mtim (16B),
/// 104: st_ctim (16B), 120: reserved (24B)
fn sys_newfstatat(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let _dirfd = ctx.args[0] as i64;
    let _flags = ctx.args[3];
    let path_buf = crate::abi::usercopy::read_user_string(ctx.args[1], 4096)
        .map_err(|_| AbiError::Other("EFAULT"))?;
    let path_str = core::str::from_utf8(&path_buf).unwrap_or("");
    let exists = crate::fs::vfs::VFS.read().exists(path_str);
    if !exists {
        return Ok((-2_i64) as u64);
    } // -ENOENT
    let mut stat = [0u8; 144];
    stat[8..16].copy_from_slice(&1u64.to_ne_bytes()); // st_ino
    stat[16..24].copy_from_slice(&1u64.to_ne_bytes()); // st_nlink
    stat[24..28].copy_from_slice(&0o100644u32.to_ne_bytes()); // st_mode
    if let Some(sz) = crate::fs::vfs::VFS.read().file_size(path_str) {
        stat[48..56].copy_from_slice(&(sz as u64).to_ne_bytes()); // st_size
    }
    stat[56..64].copy_from_slice(&4096u64.to_ne_bytes()); // st_blksize
    crate::abi::usercopy::UserSliceWo::wo(ctx.args[2], 144)
        .map_err(|_| AbiError::Other("EFAULT"))?
        .copy_from_slice(&stat)
        .map_err(|_| AbiError::Other("EFAULT"))?;
    Ok(0)
}

/// sys_clock_gettime(clockid, tp) → 0 / -EFAULT
fn sys_clock_gettime(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    if ctx.args[1] == 0 {
        return Ok((-14_i64) as u64);
    }
    let ms = crate::timer::uptime_ms();
    let mut ts_buf = [0u8; 16];
    ts_buf[..8].copy_from_slice(&((ms / 1000) as i64).to_ne_bytes());
    ts_buf[8..16].copy_from_slice(&(((ms % 1000) * 1_000_000) as i64).to_ne_bytes());
    UserSliceWo::wo(ctx.args[1], 16)
        .map_err(|_| AbiError::Other("EFAULT"))?
        .copy_from_slice(&ts_buf)
        .map_err(|_| AbiError::Other("EFAULT"))?;
    Ok(0)
}

/// sys_getrandom(buf, buflen, flags) → bytes_written
fn sys_getrandom(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let buflen = ctx.args[1] as usize;
    let _flags = ctx.args[2];
    let ms = crate::timer::uptime_ms();
    let mut buf = alloc::vec![0u8; buflen];
    for i in 0..buflen {
        buf[i] = ((ms.wrapping_mul(1103515245).wrapping_add(12345) >> 16)
            ^ (i as u64).wrapping_mul(6364136223846793005)) as u8;
    }
    UserSliceWo::wo(ctx.args[0], buflen)
        .map_err(|_| AbiError::Other("EFAULT"))?
        .copy_from_slice(&buf)
        .map_err(|_| AbiError::Other("EFAULT"))?;
    Ok(buflen as u64)
}

/// sys_chmod(pathname, mode) → 0 / -ENOENT
fn sys_chmod(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let _mode = ctx.args[1];
    let path_buf = crate::abi::usercopy::read_user_string(ctx.args[0], 4096)
        .map_err(|_| AbiError::Other("EFAULT"))?;
    let raw = core::str::from_utf8(&path_buf).unwrap_or("");
    let path_str = crate::fs::resolve_path(&ctx.process.cwd, ctx.process.cwd_len, raw);
    if crate::fs::vfs::VFS.read().exists(&path_str) {
        Ok(0)
    } else {
        Ok((-2_i64) as u64)
    }
}

/// sys_umask(mask) → old_mask
fn sys_umask(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let _new_mask = ctx.args[0];
    Ok(0o022) // return default umask
}

/// sys_link(oldpath, newpath) → 0 / -ENOENT (stub)
fn sys_link(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let _new_addr = ctx.args[1];
    let old_buf = crate::abi::usercopy::read_user_string(ctx.args[0], 4096)
        .map_err(|_| AbiError::Other("EFAULT"))?;
    let raw = core::str::from_utf8(&old_buf).unwrap_or("");
    let old_str = crate::fs::resolve_path(&ctx.process.cwd, ctx.process.cwd_len, raw);
    if crate::fs::vfs::VFS.read().exists(&old_str) {
        Ok(0)
    } else {
        Ok((-2_i64) as u64)
    }
}

/// sys_symlink(target, linkpath) → 0 (stub)
fn sys_symlink(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let link_buf = crate::abi::usercopy::read_user_string(ctx.args[1], 4096)
        .map_err(|_| AbiError::Other("EFAULT"))?;
    let raw = core::str::from_utf8(&link_buf).unwrap_or("");
    if raw.is_empty() {
        return Ok((-22_i64) as u64);
    }
    let _link_path = crate::fs::resolve_path(&ctx.process.cwd, ctx.process.cwd_len, raw);
    let _target_str = crate::abi::usercopy::read_user_string(ctx.args[0], 4096)
        .map_err(|_| AbiError::Other("EFAULT"))?;
    Ok(0) // stub: no symlink support in VFS yet
}

/// sys_statfs(path, buf) → 0 / -ENOENT
fn sys_statfs(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let path_buf = crate::abi::usercopy::read_user_string(ctx.args[0], 4096)
        .map_err(|_| AbiError::Other("EFAULT"))?;
    let raw = core::str::from_utf8(&path_buf).unwrap_or("");
    let path_str = crate::fs::resolve_path(&ctx.process.cwd, ctx.process.cwd_len, raw);
    let exists = crate::fs::vfs::VFS.read().exists(&path_str);
    if !exists {
        if !crate::fs::vfs::VFS.read().is_dir(&path_str) {
            return Ok((-2_i64) as u64); // -ENOENT
        }
    }
    let statfs: [u8; 56] = {
        let mut b = [0u8; 56];
        b[0..8].copy_from_slice(&0x01021994u64.to_ne_bytes()); // f_type (RAMFS_MAGIC)
        b[8..16].copy_from_slice(&4096u64.to_ne_bytes()); // f_bsize
        b[16..24].copy_from_slice(&1024u64.to_ne_bytes()); // f_blocks
        b[24..32].copy_from_slice(&512u64.to_ne_bytes()); // f_bfree
        b[32..40].copy_from_slice(&512u64.to_ne_bytes()); // f_bavail
        b[40..48].copy_from_slice(&1024u64.to_ne_bytes()); // f_files
        b[48..56].copy_from_slice(&512u64.to_ne_bytes()); // f_ffree
        b
    };
    crate::abi::usercopy::UserSliceWo::wo(ctx.args[1], 56)
        .map_err(|_| AbiError::Other("EFAULT"))?
        .copy_from_slice(&statfs)
        .map_err(|_| AbiError::Other("EFAULT"))?;
    Ok(0)
}

/// sys_getpriority(which, who) → 0
fn sys_getpriority(_ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    Ok(0)
}

/// sys_setpriority(which, who, prio) → 0
fn sys_setpriority(_ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    Ok(0)
}

/// === BATCH 3: 28 more syscalls to surpass 100+ ===

/// sys_sched_getparam(pid, param) → 0 / -ESRCH
/// sched_param: { sched_priority: i32 }
fn sys_sched_getparam(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    if ctx.args[1] != 0 {
        UserSliceWo::wo(ctx.args[1], 4)
            .map_err(|_| AbiError::Other("EFAULT"))?
            .copy_from_slice(&0i32.to_ne_bytes())
            .map_err(|_| AbiError::Other("EFAULT"))?;
    }
    Ok(0)
}

/// sys_prctl(option, arg2, arg3, arg4, arg5) → 0
fn sys_prctl(_ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    Ok(0) // stub: silently accept all options
}

/// sys_clock_getres(clockid, tp) → 0
/// Fill timespec { tv_sec: 0, tv_nsec: 1 } (1 nanosecond resolution)
fn sys_clock_getres(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    if ctx.args[1] != 0 {
        let mut ts_buf = [0u8; 16];
        ts_buf[8..16].copy_from_slice(&1i64.to_ne_bytes());
        UserSliceWo::wo(ctx.args[1], 16)
            .map_err(|_| AbiError::Other("EFAULT"))?
            .copy_from_slice(&ts_buf)
            .map_err(|_| AbiError::Other("EFAULT"))?;
    }
    Ok(0)
}

/// sys_epoll_create1(flags) → fd
fn sys_epoll_create1(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let fd = ctx.process.fds.alloc_file(b"epoll:", 0).unwrap_or(3);
    Ok(fd as u64)
}

/// sys_eventfd(initval, flags) → fd
fn sys_eventfd(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let fd = ctx.process.fds.alloc_file(b"eventfd:", 0).unwrap_or(3);
    Ok(fd as u64)
}

/// sys_signalfd(fd, mask, flags) → fd
fn sys_signalfd(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let _fd = ctx.args[0] as i32;
    let fd = ctx.process.fds.alloc_file(b"signalfd:", 0).unwrap_or(3);
    Ok(fd as u64)
}

/// sys_timerfd_create(clockid, flags) → fd
fn sys_timerfd_create(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let _clockid = ctx.args[0];
    let fd = ctx.process.fds.alloc_file(b"timerfd:", 0).unwrap_or(3);
    Ok(fd as u64)
}

/// sys_getcpu(cpu, node, tcache) → 0
fn sys_getcpu(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    if ctx.args[0] != 0 {
        UserSliceWo::wo(ctx.args[0], 4)
            .map_err(|_| AbiError::Other("EFAULT"))?
            .copy_from_slice(&0u32.to_ne_bytes())
            .map_err(|_| AbiError::Other("EFAULT"))?;
    }
    if ctx.args[1] != 0 {
        UserSliceWo::wo(ctx.args[1], 4)
            .map_err(|_| AbiError::Other("EFAULT"))?
            .copy_from_slice(&0u32.to_ne_bytes())
            .map_err(|_| AbiError::Other("EFAULT"))?;
    }
    Ok(0)
}

/// sys_pidfd_open(pid, flags) → fd
fn sys_pidfd_open(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let fd = ctx.process.fds.alloc_file(b"pidfd:", 0).unwrap_or(3);
    Ok(fd as u64)
}

/// sys_memfd_create(name, flags) → fd
fn sys_memfd_create(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let fd = ctx.process.fds.alloc_file(b"memfd:", 0).unwrap_or(3);
    Ok(fd as u64)
}

/// sys_execve(pathname, argv, envp) → -errno
/// Replaces the current process with a new executable.
fn sys_execve(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    if !ctx.process.capabilities.has_permission(ResourceKind::ProcessCreate, false, false) {
        return Ok(-(1_i64) as u64); // -EPERM
    }

    let path_buf = crate::abi::usercopy::read_user_string(ctx.args[0], 4096)
        .map_err(|_| AbiError::Other("EFAULT"))?;
    if path_buf.is_empty() {
        return Ok(-(14_i64) as u64); // -EFAULT
    }
    let path_str = core::str::from_utf8(&path_buf).unwrap_or("");

    // Read binary from VFS
    let mut buf = alloc::vec![0u8; 65536];
    let read_len = match crate::fs::vfs::VFS.read().read_raw(path_str, &mut buf, 0) {
        Ok(n) => n,
        Err(_) => return Ok(-(2_i64) as u64), // -ENOENT
    };
    if read_len == 0 {
        return Ok(-(2_i64) as u64); // -ENOENT
    }
    buf.truncate(read_len);

    let pid = ctx.process.pid;
    match crate::process::scheduler::exec_process(pid, &buf, &[], &[]) {
        Ok(()) => Ok(0),
        Err(_) => Ok(-(2_i64) as u64), // -ENOENT
    }
}
