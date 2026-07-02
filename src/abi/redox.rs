//! Redox OS ABI Plugin for ZiqaKernel
//!
//! Handles Redox-specific syscall numbers and scheme protocol.
//! Redox uses a unified syscall number space with bitmask encoding:
//!   - Bits 32-63: scheme ID
//!   - Bits 16-31: flags (read/write, slice, etc.)
//!   - Bits 0-15: syscall number
//!
//! The critical syscall for Orbital is SYS_FMAP (0x20000384) which maps
//! the framebuffer into the process address space.
//!
//! Common I/O syscalls (open, read, write, close, mmap, etc.) use the
//! same numbers as Linux x86_64 and are delegated to the Linux ABI
//! handler modules for shared implementation.

use crate::abi::{AbiError, AbiPlugin};
use crate::process::{AbiKind, Process};
use crate::abi::syscall::SyscallContext;
use crate::println;

/// Redox syscall numbers (from Redox kernel syscall crate)
mod nr {
    /// Map a file into memory (fmap) - the critical one for Orbital
    pub const SYS_FMAP: u64 = 0x2100_0384;
    pub const SYS_FPATH: u64 = 0x2200_03a0;
    pub const SYS_CLOSE: u64 = 0x2000_0006;
    pub const SYS_READ: u64 = 0x2200_0003;
    pub const SYS_WRITE: u64 = 0x2100_0004;
    pub const SYS_OPENAT: u64 = 0x1010_0007;
    pub const SYS_DUP: u64 = 0x2010_0029;
    pub const SYS_FCNTL: u64 = 0x2000_0037;
    pub const SYS_FUNMAP: u64 = 0x2000_005c;
    pub const SYS_CALL: u64 = 0x2300_ca11;
}

/// Redox ABI plugin
pub struct RedoxAbiPlugin;

/// Static instance
pub static REDOX_PLUGIN: RedoxAbiPlugin = RedoxAbiPlugin;

impl AbiPlugin for RedoxAbiPlugin {
    fn name(&self) -> &'static str {
        "Redox ELF"
    }

    fn kind(&self) -> AbiKind {
        AbiKind::RedoxElf
    }

    fn can_load(&self, binary: &[u8]) -> bool {
        binary.len() >= 4
            && binary[0] == 0x7F
            && binary[1] == b'E'
            && binary[2] == b'L'
            && binary[3] == b'F'
    }

    fn load(&self, binary: &[u8], process: &mut Process) -> Result<(), AbiError> {
        crate::abi::linux::elf_loader::load_elf(binary, process)
    }

    fn handle_syscall(
        &self,
        _handler: &dyn crate::abi::handler::SyscallHandler,
        ctx: &mut SyscallContext,
    ) -> Result<u64, AbiError> {
        // Redox-specific syscalls checked first
        if ctx.number == nr::SYS_FMAP {
            return handle_fmap(ctx);
        }
        if ctx.number == nr::SYS_FPATH {
            // FIX: SYS_FPATH - get path for a file descriptor
            let fd = ctx.args[0] as usize;
            let buf_addr = ctx.args[1];
            let buf_len = ctx.args[2] as usize;
            
            let path = ctx.process.fds.path_of(fd);
            match path {
                Some(p) => {
                    let path_str = core::str::from_utf8(p).unwrap_or("");
                    let display_path = b"orbital:/1280/960";
                    let bytes = if path_str == "orbital:99.0" {
                        display_path.as_slice()
                    } else {
                        path_str.as_bytes()
                    };
                    let copy_len = bytes.len().min(buf_len);
                    let user_buf = crate::abi::usercopy::UserSliceWo::wo(buf_addr, copy_len)?;
                    user_buf.copy_from_slice(&bytes[..copy_len])?;
                    return Ok(copy_len as u64);
                }
                None => return Ok(neg_errno(ERR_EBADF)),
            }
        }
        match ctx.number {
            nr::SYS_CLOSE if is_ziqa_pseudo_fd(ctx.args[0] as usize) => return Ok(0),
            nr::SYS_CLOSE => return delegate_linux_number(ctx, crate::abi::linux::nr::SYS_CLOSE),
            nr::SYS_READ if is_ziqa_pseudo_fd(ctx.args[0] as usize) => return Ok(0),
            nr::SYS_READ => return delegate_linux_number(ctx, crate::abi::linux::nr::SYS_READ),
            nr::SYS_WRITE => return handle_write(ctx),
            nr::SYS_FUNMAP => return delegate_linux_number(ctx, crate::abi::linux::nr::SYS_MUNMAP),
            nr::SYS_DUP => return handle_dup(ctx),
            nr::SYS_FCNTL => return handle_fcntl(ctx),
            nr::SYS_CALL => return handle_sys_call(ctx),
            nr::SYS_OPENAT => return handle_openat(ctx),
            240 => return delegate_linux_number(ctx, crate::abi::linux::nr::SYS_FUTEX),
            162 => return delegate_linux_number(ctx, crate::abi::linux::nr::SYS_NANOSLEEP),
            265 => return delegate_linux_number(ctx, crate::abi::linux::nr::SYS_CLOCK_GETTIME),
            _ => {}
        }

        // Redox uses Linux-compatible syscall numbers for standard I/O,
        // memory, process, time, and signal operations. Delegate to the
        // shared Linux ABI handler modules so Orbital can open scheme
        // paths (display_v2:, input:), read/write FDs, map memory, etc.
        if let Some(result) = crate::abi::linux::memory::handle(ctx) {
            return result;
        }
        if let Some(result) = crate::abi::linux::process::handle(ctx) {
            return result;
        }
        if let Some(result) = crate::abi::linux::fs::handle(ctx) {
            return result;
        }
        if let Some(result) = crate::abi::linux::time::handle(ctx) {
            return result;
        }
        #[cfg(feature = "net")]
        if let Some(result) = crate::abi::linux::net::handle(ctx) {
            return result;
        }
        if let Some(result) = crate::abi::linux::signal::handle(ctx) {
            return result;
        }
        if let Some(result) = crate::abi::linux::misc::handle(ctx) {
            return result;
        }
        #[cfg(feature = "ebpf")]
        if let Some(result) = crate::abi::linux::ebpf::handle(ctx) {
            return result;
        }

        klog_syscall("unhandled_redox_syscall", ctx.number);
        Err(AbiError::UnsupportedSyscall(ctx.number))
    }
}

fn delegate_linux_number(ctx: &mut SyscallContext, linux_number: u64) -> Result<u64, AbiError> {
    let redox_number = ctx.number;
    ctx.number = linux_number;
    let result = if let Some(result) = crate::abi::linux::memory::handle(ctx) {
        result
    } else if let Some(result) = crate::abi::linux::process::handle(ctx) {
        result
    } else if let Some(result) = crate::abi::linux::fs::handle(ctx) {
        result
    } else if let Some(result) = crate::abi::linux::time::handle(ctx) {
        result
    } else if let Some(result) = crate::abi::linux::signal::handle(ctx) {
        result
    } else if let Some(result) = crate::abi::linux::misc::handle(ctx) {
        result
    } else {
        Err(AbiError::UnsupportedSyscall(redox_number))
    };
    ctx.number = redox_number;
    result
}

const ERR_EBADF: i64 = 9;
const ERR_ENOMEM: i64 = 12;
const ERR_EFAULT: i64 = 14;
const ERR_EEXIST: i64 = 17;
const ERR_ENODEV: i64 = 19;
const ERR_EINVAL: i64 = 22;

const F_DUPFD: usize = 0;
const F_GETFD: usize = 1;
const F_SETFD: usize = 2;
const F_GETFL: usize = 3;
const F_SETFL: usize = 4;
const F_DUPFD_CLOEXEC: usize = 1030;

const REDOX_MAP_PROT_EXEC: usize = 0x0001_0000;
const REDOX_MAP_PROT_WRITE: usize = 0x0002_0000;
const REDOX_MAP_PROT_READ: usize = 0x0004_0000;
const ZIQA_MAGIC_FD: usize = 0x7a69;
const ZIQA_REGS_ENV_FD: usize = 0x7a70;
const ZIQA_SIGACTIONS_FD: usize = 0x7a71;
const REDOX_MAP_FIXED: usize = 0x0004;
const REDOX_MAP_SIZE: usize = core::mem::size_of::<usize>() * 4;

#[derive(Clone, Copy)]
struct RedoxMap {
    offset: usize,
    size: usize,
    flags: usize,
    address: usize,
}

#[inline(always)]
fn neg_errno(errno: i64) -> u64 {
    (-errno) as u64
}

fn decode_usize(buf: &[u8], offset: usize) -> usize {
    let mut raw = [0u8; core::mem::size_of::<usize>()];
    raw.copy_from_slice(&buf[offset..offset + core::mem::size_of::<usize>()]);
    usize::from_ne_bytes(raw)
}

fn read_redox_map(ctx: &mut SyscallContext) -> Result<RedoxMap, u64> {
    if ctx.args[2] as usize != REDOX_MAP_SIZE {
        return Err(neg_errno(ERR_EINVAL));
    }

    let mut raw = [0u8; REDOX_MAP_SIZE];
    crate::abi::usercopy::UserSliceRo::ro(ctx.args[1], REDOX_MAP_SIZE)
        .and_then(|src| src.copy_to_slice(&mut raw))
        .map_err(|_| neg_errno(ERR_EFAULT))?;

    Ok(RedoxMap {
        offset: decode_usize(&raw, 0),
        size: decode_usize(&raw, core::mem::size_of::<usize>()),
        flags: decode_usize(&raw, core::mem::size_of::<usize>() * 2),
        address: decode_usize(&raw, core::mem::size_of::<usize>() * 3),
    })
}

fn read_user_u64(addr: u64) -> Result<u64, u64> {
    let mut raw = [0u8; core::mem::size_of::<u64>()];
    crate::abi::usercopy::UserSliceRo::ro(addr, raw.len())
        .and_then(|src| src.copy_to_slice(&mut raw))
        .map_err(|_| neg_errno(ERR_EFAULT))?;
    Ok(u64::from_ne_bytes(raw))
}

fn handle_sys_call(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let metadata_count = (ctx.args[3] as usize) & 0xff;
    let metadata_ptr = ctx.args[4];
    if metadata_count == 0 {
        return Ok(neg_errno(ERR_EINVAL));
    }

    let verb = match read_user_u64(metadata_ptr) {
        Ok(verb) => verb,
        Err(errno) => return Ok(errno),
    };

    match verb {
        // ProcCall::Exit
        2 => {
            let status = if metadata_count > 1 {
                read_user_u64(metadata_ptr + 8).unwrap_or(0)
            } else {
                0
            };
            ctx.process.state = crate::process::ProcessState::Exited((status & 0xff) as i64);
            Ok(0)
        }
        // ProcCall::Getppid
        12 => Ok(ctx.process.parent),
        // ProcCall::SetResugid, Setpgid, Getsid, Setsid, DisableSetpgid, priority.
        // ponytail: Orbital only needs these as successful no-ops until ZiqaKernel
        // grows a real Redox proc scheme.
        0 | 4 | 5 | 6 | 7 | 10 | 14 | 16 | 17 => Ok(0),
        _ => Ok(neg_errno(ERR_EINVAL)),
    }
}

fn align_page(size: usize) -> Option<usize> {
    if size == 0 {
        return None;
    }
    size.checked_add(4095).map(|value| value & !4095)
}

fn parse_orbital_window_geometry(path: &str) -> Option<(usize, usize, usize, usize)> {
    let rest = path.strip_prefix("orbital:")?.trim_start_matches('/');
    if rest.is_empty() || !rest.contains('/') {
        return None;
    }

    let mut parts = rest.splitn(6, '/');
    let _flags = parts.next()?;
    let x = parts.next()?.parse::<isize>().ok()?.max(0) as usize;
    let y = parts.next()?.parse::<isize>().ok()?.max(0) as usize;
    let w = parts.next()?.parse::<usize>().ok()?;
    let h = parts.next()?.parse::<usize>().ok()?;
    Some((x, y, w, h))
}


fn redox_map_flags_to_region_flags(flags: usize) -> crate::memory::paging::MemoryRegionFlags {
    let has_prot = flags & (REDOX_MAP_PROT_EXEC | REDOX_MAP_PROT_WRITE | REDOX_MAP_PROT_READ) != 0;
    crate::memory::paging::MemoryRegionFlags {
        readable: if has_prot { flags & REDOX_MAP_PROT_READ != 0 } else { true },
        writable: if has_prot { flags & REDOX_MAP_PROT_WRITE != 0 } else { true },
        executable: flags & REDOX_MAP_PROT_EXEC != 0,
        user_accessible: true,
        copy_on_write: false,
    }
}

fn is_ziqa_pseudo_fd(fd: usize) -> bool {
    fd == ZIQA_MAGIC_FD || fd == ZIQA_REGS_ENV_FD || fd == ZIQA_SIGACTIONS_FD
}

fn handle_write(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let fd = ctx.args[0] as usize;
    let count = ctx.args[2];
    #[cfg(feature = "redox-debug")]
    if fd == 1 || fd == 2 {
        let len = (count as usize).min(256);
        let mut buf = [0u8; 256];
        if crate::abi::usercopy::UserSliceRo::ro(ctx.args[1], len)
            .and_then(|src| src.copy_to_slice(&mut buf[..len]))
            .is_ok()
        {
            let text = core::str::from_utf8(&buf[..len]).unwrap_or("<non-utf8>");
            crate::println!("[redox fd{}] {}", fd, text);
        }
    }
    if fd == ZIQA_REGS_ENV_FD && count >= 8 {
        let fs_base = match read_user_u64(ctx.args[1]) {
            Ok(value) => value,
            Err(errno) => return Ok(errno),
        };
        ctx.process.fs_base = fs_base;
        #[cfg(target_arch = "x86_64")]
        unsafe {
            x86_64::registers::model_specific::Msr::new(0xC000_0100).write(fs_base);
        }
        return Ok(count);
    }
    if fd <= 2 || fd == ZIQA_MAGIC_FD || fd == ZIQA_REGS_ENV_FD || fd == ZIQA_SIGACTIONS_FD {
        return Ok(count);
    }
    delegate_linux_number(ctx, crate::abi::linux::nr::SYS_WRITE)
}

fn handle_dup(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let fd = ctx.args[0] as usize;
    if fd == ZIQA_MAGIC_FD || fd == ZIQA_REGS_ENV_FD || fd == ZIQA_SIGACTIONS_FD {
        return Ok(fd as u64);
    }
    if fd < 3 && ctx.args[2] != 0 {
        let len = (ctx.args[2] as usize).min(32);
        let mut path = [0u8; 32];
        crate::abi::usercopy::UserSliceRo::ro(ctx.args[1], len)?.copy_to_slice(&mut path[..len])?;
        if &path[..len] == b"regs/env" {
            return Ok(ZIQA_REGS_ENV_FD as u64);
        }
        if &path[..len] == b"sigactions" {
            return Ok(ZIQA_SIGACTIONS_FD as u64);
        }
        return Ok(fd as u64);
    }
    match ctx.process.fds.dup(fd, None) {
        Some(new_fd) => Ok(new_fd as u64),
        None => Ok(neg_errno(ERR_EBADF)),
    }
}

fn handle_fcntl(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let fd = ctx.args[0] as usize;
    let cmd = ctx.args[1] as usize;
    let arg = ctx.args[2] as usize;

    if is_ziqa_pseudo_fd(fd) {
        return Ok(0);
    }
    if ctx.process.fds.get(fd).is_none() {
        return Ok(neg_errno(ERR_EBADF));
    }

    match cmd {
        F_DUPFD | F_DUPFD_CLOEXEC => match ctx.process.fds.dup(fd, None) {
            Some(new_fd) => Ok(new_fd as u64),
            None => Ok(neg_errno(ERR_EBADF)),
        },
        F_GETFD => Ok(0),
        F_SETFD => Ok(0),
        F_GETFL => Ok(ctx.process.fds.get(fd).map(|desc| desc.flags as u64).unwrap_or(0)),
        F_SETFL => {
            if let Some(desc) = ctx.process.fds.get_mut(fd) {
                desc.flags = arg as u32;
            }
            Ok(0)
        }
        _ => Ok(neg_errno(ERR_EINVAL)),
    }
}

fn map_redox_anonymous(ctx: &mut SyscallContext, map: RedoxMap) -> Result<u64, AbiError> {
    if !ctx
        .process
        .capabilities
        .has_permission(crate::capability::ResourceKind::Memory, true, false)
    {
        return Err(AbiError::PermissionDenied);
    }

    let size = match align_page(map.size) {
        Some(size) => size,
        None => return Ok(neg_errno(ERR_EINVAL)),
    };
    let fixed = map.flags & REDOX_MAP_FIXED != 0;
    let start_hint = if map.address != 0 {
        crate::memory::VirtAddr::new(map.address as u64)
    } else {
        crate::memory::VirtAddr::new(ctx.process.mmap_bump)
    };
    let base = if fixed && map.address != 0 {
        if !crate::process::vma::is_range_free(&ctx.process.vmas, start_hint, size) {
            return Ok(neg_errno(ERR_EEXIST));
        }
        start_hint
    } else {
        match crate::process::vma::find_free_range(&ctx.process.vmas, size, start_hint) {
            Some(base) => base,
            None => return Ok(neg_errno(ERR_ENOMEM)),
        }
    };

    ctx.process.mmap_bump = core::cmp::max(ctx.process.mmap_bump, base.as_u64() + size as u64);
    let region_flags = redox_map_flags_to_region_flags(map.flags);
    let page_flags = crate::memory::paging::region_flags_to_page_flags(&region_flags);
    let mut mapped = 0usize;
    while mapped < size {
        if !crate::memory::paging::demand_page_for_root(
            ctx.process.page_table_frame,
            base + mapped as u64,
            page_flags,
            None,
        ) {
            return Ok(neg_errno(ERR_ENOMEM));
        }
        mapped += 4096;
    }
    ctx.process.add_region(crate::process::vma::Vma {
        start: base,
        end: base + size as u64,
        flags: region_flags,
        is_file_backed: false,
        file_path: None,
        file_offset: map.offset as u64,
        file_size: 0,
        bco_hook: None,
    });

    Ok(base.as_u64())
}

fn handle_openat(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    if !ctx
        .process
        .capabilities
        .has_permission(crate::capability::ResourceKind::File, false, false)
    {
        return Err(AbiError::PermissionDenied);
    }

    let path_ptr = ctx.args[1];
    let path_len = (ctx.args[2] as usize).min(4096);
    let flags = ctx.args[3] as usize;
    let mut path_buf = alloc::vec![0u8; path_len];
    crate::abi::usercopy::UserSliceRo::ro(path_ptr, path_len)?.copy_to_slice(&mut path_buf)?;
    let path_str = core::str::from_utf8(&path_buf).unwrap_or("");

    let translated_path = if let Some(rest) = path_str.strip_prefix("/scheme/") {
        match rest.find('/') {
            Some(pos) => {
                let mut path = alloc::string::String::from(&rest[..pos]);
                path.push(':');
                path.push_str(&rest[pos + 1..]);
                path
            }
            None => {
                let mut path = alloc::string::String::from(rest);
                path.push(':');
                path
            }
        }
    } else {
        alloc::string::String::from(path_str)
    };

    if let Some(result) = crate::fs::vfs::VFS.read().handle_scheme(&translated_path, flags) {
        return match result {
            Ok(id) => {
                let fd = ctx
                    .process
                    .fds
                    .alloc_scheme(translated_path.as_bytes(), flags as u32, id)
                    .ok_or(AbiError::Other("EMFILE"))?;
                Ok(fd as u64)
            }
            Err(err) => Err(err),
        };
    }

    Ok((-2_i64) as u64)
}

/// Handle the Redox SYS_FMAP call: maps the physical framebuffer into the
/// process address space and returns the user-space virtual address.
fn handle_fmap(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let fd = ctx.args[0] as usize;
    let map = match read_redox_map(ctx) {
        Ok(map) => map,
        Err(errno) => return Ok(errno),
    };

    println!(
        "[SYS_FMAP] called fd={} offset={} size={} flags={:#x} address={:#x}",
        fd, map.offset, map.size, map.flags, map.address
    );

    if fd == usize::MAX {
        return map_redox_anonymous(ctx, map);
    }

    // Orbital's ziqa-bga-direct path calls fmap on fd 0.  Treat any
    // non-anonymous fmap as the current display framebuffer until ZiqaKernel
    // has real file-backed mmap support.
    let (fb_kernel_virt, width, height, bpp) = match crate::drivers::virtio_gpu::get_fb_info()
        .or_else(|| crate::drivers::framebuffer::get_bga_fb_info())
    {
        Some(info) => {
            println!("[SYS_FMAP] fb_info: virt={:#x} {}x{} {}bpp", info.0, info.1, info.2, info.3);
            info
        }
        None => {
            println!("[SYS_FMAP] ERROR: no framebuffer available!");
            return Ok(neg_errno(ERR_ENODEV));
        }
    };

    let bpp = bpp as usize;
    let fb_size = (width as usize) * (height as usize) * (bpp / 8);
    let map_size = align_page(map.size.min(fb_size)).unwrap_or(fb_size);
    let page_count = (map_size + 4095) / 4096;

    let po = crate::memory::paging::phys_offset().as_u64();
    let mut fb_phys = fb_kernel_virt.wrapping_sub(po);
    if let Some(path_bytes) = ctx.process.fds.path_of(fd) {
        if let Ok(path) = core::str::from_utf8(path_bytes) {
            if let Some((x, y, _w, _h)) = parse_orbital_window_geometry(path) {
                if x >= width as usize || y >= height as usize {
                    return map_redox_anonymous(ctx, map);
                }
                let byte_offset = ((y * width as usize) + x) * (bpp / 8);
                fb_phys = fb_phys.saturating_add((byte_offset & !4095) as u64);
            }
        }
    }

    println!("[SYS_FMAP] phys={:#x} size={} ({} pages)", fb_phys, map_size, page_count);

    let fb_user_virt = if map.address != 0 {
        map.address as u64
    } else {
        0x4000_0000_000u64
    };
    let user_start = x86_64::VirtAddr::new(fb_user_virt);

    use x86_64::structures::paging::{
        Mapper, OffsetPageTable, Page, PageTableFlags, PhysFrame, Size4KiB,
    };

    let offset = crate::memory::paging::phys_offset();
    let mut mapper = match ctx.process.page_table_frame {
        Some(pt_frame) => {
            println!("[SYS_FMAP] page_table_frame: {:#x}", pt_frame.start_address().as_u64());
            let l4_virt = offset + pt_frame.start_address().as_u64();
            let l4 = unsafe { &mut *(l4_virt.as_mut_ptr()) };
            unsafe { OffsetPageTable::new(l4, offset) }
        }
        None => {
            println!("[SYS_FMAP] using shared kernel page table");
            unsafe { crate::memory::paging::current_mapper() }
        }
    };

    let flags = PageTableFlags::PRESENT
        | PageTableFlags::WRITABLE
        | PageTableFlags::USER_ACCESSIBLE
        | PageTableFlags::NO_CACHE;

    let mut fa_guard = crate::memory::FRAME_ALLOCATOR.lock();
    let fa = match fa_guard.as_mut() {
        Some(fa) => fa,
        None => {
            println!("[SYS_FMAP] ERROR: no frame allocator!");
            return Ok(neg_errno(ERR_ENOMEM));
        }
    };

    for i in 0..page_count {
        let page = Page::<Size4KiB>::containing_address(user_start + (i * 4096) as u64);
        let frame = PhysFrame::containing_address(x86_64::PhysAddr::new(fb_phys + (i * 4096) as u64));
        let result = unsafe { mapper.map_to(page, frame, flags, fa) };
        match result {
            Ok(mapping) => mapping.flush(),
            Err(e) => {
                println!("[SYS_FMAP] ERROR mapping page {}: {:?}", i, e);
                return Ok(neg_errno(ERR_ENOMEM));
            }
        }
    }

    ctx.process.add_region(crate::process::vma::Vma {
        start: crate::memory::VirtAddr::new(fb_user_virt),
        end: crate::memory::VirtAddr::new(fb_user_virt + map_size as u64),
        flags: crate::memory::paging::MemoryRegionFlags {
            readable: true,
            writable: true,
            executable: false,
            user_accessible: true,
            copy_on_write: false,
        },
        is_file_backed: false,
        file_path: None,
        file_offset: 0,
        file_size: 0,
        bco_hook: None,
    });

    println!(
        "[SYS_FMAP] SUCCESS: mapped at user {:#x} ({}.{} MiB)",
        fb_user_virt,
        map_size / (1024 * 1024),
        (map_size % (1024 * 1024)) * 100 / (1024 * 1024)
    );
    Ok(fb_user_virt)
}

#[inline(always)]
fn klog_syscall(name: &'static str, val: u64) {
    crate::klog!(crate::klog::Level::Debug, "syscall {} -> {}", name, val);
}
