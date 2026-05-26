/// WASM ABI Plugin for ZiqaKernel
///
/// This plugin allows ZiqaKernel to run WebAssembly modules.
/// WASM is a portable, sandboxed format that is ideal for a modern kernel.
/// It uses a WASI-like interface for system calls.

use crate::abi::{AbiPlugin, AbiError};
use crate::abi::syscall::SyscallContext;
use crate::process::{Process, AbiKind};
use crate::println;

/// The WASM ABI plugin instance
pub struct WasmAbiPlugin;

/// Static instance so it can be registered in the ABI registry
pub static WASM_PLUGIN: WasmAbiPlugin = WasmAbiPlugin;

impl AbiPlugin for WasmAbiPlugin {
    fn name(&self) -> &'static str {
        "WebAssembly (WASI)"
    }

    fn kind(&self) -> AbiKind {
        AbiKind::Wasm
    }

    fn can_load(&self, binary: &[u8]) -> bool {
        // WASM magic: 0x00 'a' 's' 'm'
        binary.len() >= 4
            && binary[0] == 0x00
            && binary[1] == b'a'
            && binary[2] == b's'
            && binary[3] == b'm'
    }

    fn load(&self, _binary: &[u8], _process: &mut Process) -> Result<(), AbiError> {
        // In a real implementation, we'd use a WASM runtime (like wasm3 or a custom JIT)
        // to parse and prepare the module. For now, we stub this.
        println!("[WASM ABI] Loading WASM module...");
        Ok(())
    }

    fn handle_syscall(&self, ctx: &mut SyscallContext) -> Result<u64, AbiError> {
        // WASI syscalls have different numbers and conventions.
        // For now, we'll map a few common ones.
        match ctx.number {
            // proc_exit (id varies by WASI version, let's use a dummy for now)
            99 => {
                let status = ctx.args[0] as i64;
                println!("[WASM ABI] WASM module exiting with status {}", status);
                ctx.process.exit(status);
                Ok(0)
            }
            // fd_write
            64 => {
                println!("[WASM ABI] fd_write called from WASM");
                Ok(ctx.args[2]) // return count
            }
            unknown => {
                println!("[WASM ABI] Unimplemented syscall: {}", unknown);
                Err(AbiError::UnsupportedSyscall(unknown))
            }
        }
    }
}
