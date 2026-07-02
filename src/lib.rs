#![no_std]
#![feature(abi_x86_interrupt)]

extern crate alloc;
pub mod abi;
pub mod arch;
pub mod boot_screen;
pub mod capability;
pub mod common;
pub mod drivers;
pub mod event;
pub mod fs;
pub mod scheme;

#[cfg(feature = "shell")]
pub mod edit;
pub mod init;
pub mod io;
pub mod ipc;
pub mod klog;
pub mod memory;
pub mod process;
pub mod sync;
pub mod timer;
pub mod tracepoints;
pub mod bench_fork;
pub mod bench_tlb;
pub mod worker_pool;

#[cfg(feature = "ebpf")]
pub mod ebpf;
#[cfg(feature = "games")]
pub mod doom;
#[cfg(feature = "games")]
pub mod tetris;
pub mod userspace;
#[cfg(feature = "net")]
pub mod net;
#[cfg(feature = "shell")]
pub mod shell;
#[cfg(feature = "zig-hotpaths")]
pub mod zig_ffi;
#[cfg(feature = "zig-hotpaths")]
pub mod zig_kernel_ops;

// tests always available but some test bodies may be cfg-gated
pub mod tests;
#[cfg(feature = "net")]
pub mod tests_net;
#[cfg(feature = "ebpf")]
pub mod tests_fix;

pub use init::{init, init_abi_registry};

// Store boot info for later use
pub static BOOT_INFO: spin::Mutex<Option<&'static bootloader::BootInfo>> = spin::Mutex::new(None);
