#![no_std]
#![feature(abi_x86_interrupt)]

extern crate alloc;
pub mod abi;
pub mod arch;
pub mod boot_screen;
pub mod capability;
pub mod doom;
pub mod drivers;
pub mod ebpf;
pub mod edit;
pub mod fs;
pub mod init;
pub mod io;
pub mod ipc;
pub mod klog;
pub mod memory;
pub mod net;
pub mod perf;
pub mod process;
pub mod shell;
pub mod tests;
pub mod tetris;
pub mod timer;
pub mod zig_ffi;

pub use init::{init, init_abi_registry};

// Store boot info for later use
pub static BOOT_INFO: spin::Mutex<Option<&'static bootloader::BootInfo>> = spin::Mutex::new(None);
