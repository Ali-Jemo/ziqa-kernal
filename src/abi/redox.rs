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

use crate::abi::{AbiError, AbiPlugin};
use crate::process::{AbiKind, Process};
use crate::abi::syscall::SyscallContext;

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
        // ELF magic: 0x7F 'E' 'L' 'F'
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
        match ctx.number {
            // Redox fmap syscall - map framebuffer for Orbital
            nr::SYS_FMAP => {
                let _fd = ctx.args[0] as usize;
                let _offset = ctx.args[1];
                let _size = ctx.args[2] as usize;

                // Get framebuffer info from VirtIO GPU or BGA
                let fb_info = crate::drivers::virtio_gpu::get_fb_info()
                    .or_else(|| crate::drivers::framebuffer::get_bga_fb_info());

                if let Some((fb_addr, _w, _h, _bpp)) = fb_info {
                    klog_syscall("fmap", fb_addr);
                    // Return the framebuffer address - allows Orbital to mmap screen directly
                    return Ok(fb_addr);
                }

                // Fallback: no framebuffer available
                klog_syscall("fmap", 0);
                Err(AbiError::Other("ENODEV: no framebuffer"))
            }
            _ => Err(AbiError::UnsupportedSyscall(ctx.number)),
        }
    }
}

#[inline(always)]
fn klog_syscall(name: &'static str, val: u64) {
    crate::klog!(crate::klog::Level::Debug, "syscall {} -> {}", name, val);
}