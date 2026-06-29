//! Redox OS ABI Plugin for ZiqaKernel
//!
//! Handles Redox-specific syscall numbers and scheme protocol.
//! Redox uses a unified syscall number space with bitmask encoding:
//!   - Bits 32-63: scheme ID
//!   - Bits 16-31: flags (read/write, slice, etc.)
//!   - Bits 0-15: syscall number
//!
//! The critical syscall for Orbital is SYS_FMAP (0x21000384) which maps
//! the framebuffer into the process address space.
//!
//! Common I/O syscalls (open, read, write, close, mmap, etc.) use the
//! same numbers as Linux x86_64 and are delegated to the Linux ABI
//! handler modules for shared implementation.

use crate::abi::{AbiError, AbiPlugin};
use crate::process::{AbiKind, Process};
use crate::abi::syscall::SyscallContext;
use crate::process::ProcessState;
use crate::println;

/// Redox syscall numbers (from redox_syscall::number).
mod nr {
    pub const SYS_CLOSE: u64 = 0x2000_0006;
    pub const SYS_DUP: u64 = 0x2010_0029;
    pub const SYS_DUP2: u64 = 0x2010_003f;
    pub const SYS_READ: u64 = 0x2200_0003;
    pub const SYS_WRITE: u64 = 0x2100_0004;
    pub const SYS_LSEEK: u64 = 0x2000_0013;
    pub const SYS_FCNTL: u64 = 0x2000_0037;
    pub const SYS_FUTEX: u64 = 240;
    pub const SYS_CLOCK_GETTIME: u64 = 265;
    pub const SYS_NANOSLEEP: u64 = 162;
    pub const SYS_YIELD: u64 = 158;
    pub const SYS_FSTAT: u64 = 0x2200_001c;
    pub const SYS_GETDENTS: u64 = 0x2000_002b;
    pub const SYS_FEVENT: u64 = 0x2000_039f;
    pub const SYS_OPENAT: u64 = 0x1010_0007;
    pub const SYS_FMAP: u64 = 0x2100_0384;
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
        handler: &dyn crate::abi::handler::SyscallHandler,
        ctx: &mut SyscallContext,
    ) -> Result<u64, AbiError> {
        // Early stdout capture for debugging — print every Redox syscall
        if ctx.number == nr::SYS_WRITE && matches!(ctx.args[0], 1 | 2) {
            let len = (ctx.args[2] as usize).min(256);
            let mut buf = [0u8; 256];
            if crate::abi::usercopy::UserSliceRo::ro(ctx.args[1], len)
                .and_then(|s| s.copy_to_slice(&mut buf[..len]))
                .is_ok()
            {
                if let Ok(s) = core::str::from_utf8(&buf[..len]) {
                    crate::print!("{}", s);
                }
            }
            return Ok(ctx.args[2]);
        }
        // Magic FD range (31337-31339) — used by relibc for early fd probing
        let is_magic_fd = ctx.args[0] == 31337 || ctx.args[0] == 31338 || ctx.args[0] == 31339;
        if is_magic_fd {
            match ctx.number {
                nr::SYS_DUP | nr::SYS_FCNTL => return Ok(31338),
                nr::SYS_READ => {
                    let buf_addr = ctx.args[1];
                    let len = ctx.args[2] as usize;
                    let copy_len = len.min(16);
                    let zero = [0u8; 16];
                    let _ = crate::abi::usercopy::UserSliceWo::wo(buf_addr, copy_len)
                        .and_then(|s| s.copy_from_slice(&zero[..copy_len]));
                    return Ok(copy_len as u64);
                }
                nr::SYS_CLOSE => return Ok(0),
                _ => {}
            }
        }
        match ctx.number {
            nr::SYS_FMAP => return handle_fmap(ctx),
            nr::SYS_CALL => return handle_proc_call(ctx),
            nr::SYS_YIELD => {
                // SYS_YIELD (158) is multiplexed:
                //   - args[0] == 0x1001-0x1004 → ARCH_PRCTL (set FS/GS base)
                //   - otherwise → yield CPU (no-op for us)
                if matches!(ctx.args[0], 0x1001 | 0x1002 | 0x1003 | 0x1004) {
                    return with_linux_syscall(ctx, crate::abi::linux::nr::SYS_ARCH_PRCTL, |ctx| {
                        let linux = crate::abi::linux::LinuxAbiPlugin;
                        linux.handle_syscall(handler, ctx)
                    });
                }
                if ctx.process.state == ProcessState::Running {
                    ctx.process.state = ProcessState::Ready;
                }
                return Ok(0);
            }
            273 => return Ok(0), // set_robust_list: single-threaded userspace shim
            334 => return Ok(0), // rseq: accept registration; no restartable sequences yet
            nr::SYS_FEVENT => {
                let fd = ctx.args[0] as usize;
                let flags = ctx.args[1] as usize;
                let desc = ctx.process.fds.get(fd);
                if let Some(d) = desc {
                    if let crate::process::FdTarget::Scheme(_, handle_id) = d.target {
                        let registry = crate::scheme::SCHEME_REGISTRY.lock();
                        for name in ["display_v2", "display", "input"] {
                            if let Some(scheme) = registry.get(name) {
                                let ready = scheme.fevent(handle_id, flags).unwrap_or(0);
                                return Ok(ready as u64);
                            }
                        }
                    }
                }
                return Ok(0)
            }
            _ => {}
        }
        // SYS_EXIT (1) — some Redox binaries use this directly. Mark the
        // current process exited and let the syscall trap schedule away after
        // the process lock is released.
        if ctx.number == 1 {
            ctx.process.exit(ctx.args[0] as i64);
            return Ok(0);
        }
        // Try mapping to a Linux syscall number
        let mapped = redox_to_linux_syscall(ctx.number);
        #[cfg(feature = "redox-debug")]
        crate::println!("[Redox ABI DEBUG] ctx.number={}, mapped={:?}", ctx.number, mapped);
        if let Some(linux_number) = mapped {
            return with_linux_syscall(ctx, linux_number, |ctx| {
                let linux = crate::abi::linux::LinuxAbiPlugin;
                linux.handle_syscall(handler, ctx)
            });
        }
        // ponytail: unknown Redox syscall → return 0 instead of aborting.
        // relibc's startup path probes several syscalls during boot; returning
        // success lets the binary continue and fail on something we actually
        // need to handle (which will produce a visible error).
        crate::println!(
            "[Redox ABI] WARN: unhandled syscall {:#x} (args={:#x},{:#x},{:#x}) -> Ok(0)",
            ctx.number, ctx.args[0], ctx.args[1], ctx.args[2]
        );
        klog_syscall("unhandled_redox_syscall", ctx.number);
        Ok(0)
    }
}

fn handle_proc_call(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let packet_ptr = ctx.args[4];
    let mut packet = [0u8; 16];
    crate::abi::usercopy::UserSliceRo::ro(packet_ptr, packet.len())?
        .copy_to_slice(&mut packet)?;

    let op = u64::from_le_bytes([
        packet[0], packet[1], packet[2], packet[3],
        packet[4], packet[5], packet[6], packet[7],
    ]);
    let status = u64::from_le_bytes([
        packet[8], packet[9], packet[10], packet[11],
        packet[12], packet[13], packet[14], packet[15],
    ]);

    if op == 2 {
        ctx.process.exit(status as i64);
    }

    Ok(0)
}

fn redox_to_linux_syscall(number: u64) -> Option<u64> {
    let linux = match number {
        218 => crate::abi::linux::nr::SYS_SET_TID_ADDRESS,
        // The locally linked Orbital binary still emits a few plain
        // x86_64 Linux-style memory syscalls from its Rust/std startup path.
        // Route those through the existing Linux memory handlers instead of
        // killing the Redox process.
        9 => crate::abi::linux::nr::SYS_MMAP,
        12 => crate::abi::linux::nr::SYS_BRK,
        10 => crate::abi::linux::nr::SYS_MPROTECT,
        11 => crate::abi::linux::nr::SYS_MUNMAP,
        nr::SYS_READ => crate::abi::linux::nr::SYS_READ,
        nr::SYS_WRITE => crate::abi::linux::nr::SYS_WRITE,
        nr::SYS_CLOSE => crate::abi::linux::nr::SYS_CLOSE,
        nr::SYS_DUP => crate::abi::linux::nr::SYS_DUP,
        nr::SYS_DUP2 => crate::abi::linux::nr::SYS_DUP2,
        nr::SYS_LSEEK => crate::abi::linux::nr::SYS_LSEEK,
        nr::SYS_FCNTL => crate::abi::linux::nr::SYS_FCNTL,
        nr::SYS_FSTAT => crate::abi::linux::nr::SYS_FSTAT,
        nr::SYS_GETDENTS => crate::abi::linux::nr::SYS_GETDENTS64,
        nr::SYS_FUTEX => crate::abi::linux::nr::SYS_FUTEX,
        nr::SYS_CLOCK_GETTIME => crate::abi::linux::nr::SYS_CLOCK_GETTIME,
        nr::SYS_NANOSLEEP => crate::abi::linux::nr::SYS_NANOSLEEP,
        nr::SYS_OPENAT => crate::abi::linux::nr::SYS_OPENAT,
        nr::SYS_FUNMAP => crate::abi::linux::nr::SYS_MUNMAP,
        _ => return None,
    };
    Some(linux)
}

fn with_linux_syscall<F>(ctx: &mut SyscallContext, linux_number: u64, f: F) -> Result<u64, AbiError>
where
    F: FnOnce(&mut SyscallContext) -> Result<u64, AbiError>,
{
    ctx.number = linux_number;
    let result = f(ctx);
    result
}

/// Handle the Redox SYS_FMAP call: maps the physical framebuffer into the
/// process address space and returns the user-space virtual address.
fn handle_fmap(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let _fd = ctx.args[0] as usize;
    let _offset = ctx.args[1];
    let _size = ctx.args[2] as usize;

    println!("[SYS_FMAP] called fd={} offset={} size={}", _fd, _offset, _size);

    // Get framebuffer info (kernel virtual address, dimensions)
    let fb_info = crate::drivers::virtio_gpu::get_fb_info()
        .or_else(|| crate::drivers::framebuffer::get_bga_fb_info());

    let (fb_kernel_virt, width, height, bpp) = match fb_info {
        Some(info) => {
            println!("[SYS_FMAP] fb_info: virt={:#x} {}x{} {}bpp", info.0, info.1, info.2, info.3);
            info
        }
        None => {
            println!("[SYS_FMAP] ERROR: no framebuffer available!");
            return Err(AbiError::Other("ENODEV: no framebuffer"));
        }
    };

    let bpp = bpp as usize;
    let fb_size = (width as usize) * (height as usize) * (bpp / 8);
    let page_count = (fb_size + 4095) / 4096;

    // Convert kernel virtual address to physical address
    let po = crate::memory::paging::phys_offset();
    let fb_phys = fb_kernel_virt.wrapping_sub(po.as_u64());

    println!("[SYS_FMAP] phys={:#x} size={} ({} pages)", fb_phys, fb_size, page_count);

    // Choose user virtual address for mapping (high user address, below stack)
    let fb_user_virt = 0x4000_0000_000u64;
    let user_start = x86_64::VirtAddr::new(fb_user_virt);

    // Map framebuffer pages into the process's page table
    use x86_64::structures::paging::{Page, PhysFrame, Size4KiB, PageTableFlags, Mapper, PageTable};
    use crate::memory::paging::phys_offset as poff;

    let pt_frame = match ctx.process.page_table_frame {
        Some(f) => {
            println!("[SYS_FMAP] page_table_frame: {:#x}", f.start_address().as_u64());
            f
        }
        None => {
            println!("[SYS_FMAP] ERROR: no process page table!");
            return Err(AbiError::Other("ENOSYS: no process page table"));
        }
    };

    let l4_virt = poff() + pt_frame.start_address().as_u64();
    let l4 = unsafe { &mut *(l4_virt.as_mut_ptr::<PageTable>()) };
    let mut mapper = unsafe {
        x86_64::structures::paging::OffsetPageTable::new(l4, poff())
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
            return Err(AbiError::Other("ENOMEM: no frame allocator"));
        }
    };

    for i in 0..page_count {
        let page = Page::<Size4KiB>::containing_address(user_start + (i * 4096) as u64);
        let frame = PhysFrame::containing_address(x86_64::PhysAddr::new(fb_phys + (i * 4096) as u64));
        let result = unsafe { mapper.map_to(page, frame, flags, fa) };
        match result {
            Ok(m) => m.flush(),
            Err(e) => {
                println!("[SYS_FMAP] ERROR mapping page {}: {:?}", i, e);
                return Err(AbiError::Other("ENOMEM: page map failed"));
            }
        }
    }

    // Register VMA so munmap / cleanup works
    ctx.process.add_region(crate::process::vma::Vma {
        start: crate::memory::VirtAddr::new(fb_user_virt),
        end: crate::memory::VirtAddr::new(fb_user_virt + fb_size as u64),
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

    println!("[SYS_FMAP] SUCCESS: mapped at user {:#x} ({}.{} MiB)", fb_user_virt, fb_size / (1024*1024), (fb_size % (1024*1024)) * 100 / (1024*1024));
    Ok(fb_user_virt)
}

#[inline(always)]
fn klog_syscall(name: &'static str, val: u64) {
    crate::klog!(crate::klog::Level::Debug, "syscall {} -> {}", name, val);
}
