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
    pub const SYS_FMAP: u64 = 0x20000384;
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
        if let Some(result) = crate::abi::linux::ebpf::handle(ctx) {
            return result;
        }

        klog_syscall("unhandled_redox_syscall", ctx.number);
        Err(AbiError::UnsupportedSyscall(ctx.number))
    }
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
    let po = crate::memory::paging::phys_offset().as_u64();
    let fb_phys = fb_kernel_virt.wrapping_sub(po);

    println!("[SYS_FMAP] phys={:#x} size={} ({} pages)", fb_phys, fb_size, page_count);

    // Choose user virtual address for mapping (high user address, below stack)
    let fb_user_virt = 0x4000_0000_000u64;
    let user_start = x86_64::VirtAddr::new(fb_user_virt);

    // Map framebuffer pages into the process's page table
    use x86_64::structures::paging::{Page, PhysFrame, Size4KiB, PageTableFlags, Mapper};
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

    let offset = poff();
    let l4_virt = offset + pt_frame.start_address().as_u64();
    let l4 = unsafe { &mut *(l4_virt.as_mut_ptr()) };
    let mut mapper = unsafe {
        x86_64::structures::paging::OffsetPageTable::new(l4, offset)
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
