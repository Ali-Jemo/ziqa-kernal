#![no_std]
#![feature(abi_x86_interrupt)]

extern crate alloc;
pub mod arch;
pub mod drivers;
pub mod memory;
pub mod capability;
pub mod process;
pub mod abi;
pub mod fs;
pub mod perf;
pub mod ebpf;
pub mod io;
pub mod ipc;
pub mod timer;
pub mod klog;
pub mod shell;

use abi::AbiPlugin;

// Store boot info for later use
pub static BOOT_INFO: spin::Mutex<Option<&'static bootloader::BootInfo>> = spin::Mutex::new(None);

pub fn init(boot_info: &'static bootloader::BootInfo) {
    // Store boot info for later use
    *BOOT_INFO.lock() = Some(boot_info);

    // ── Hardware init ──
    arch::x86_64::gdt::init();
    arch::x86_64::interrupts::init_idt();
    arch::x86_64::interrupts::init_pics();

    // ── Memory and Heap init ──
    use x86_64::VirtAddr;
    use memory::frame_allocator::BootInfoFrameAllocator;
    let phys_offset = VirtAddr::new(boot_info.physical_memory_offset);
    let mut mapper = unsafe { memory::frame_allocator::init(phys_offset) };
    let mut frame_alloc = unsafe { BootInfoFrameAllocator::init(&boot_info.memory_map) };
    memory::heap::init_heap(&mut mapper, &mut frame_alloc)
        .expect("heap init failed");

    // Initialize kernel memory mapper for higher-half mapping
    memory::paging::init_kernel_mapper(VirtAddr::new(boot_info.physical_memory_offset));

    // ── ABI subsystem init ──
    println!("[ZIQA] Hardware initialized (GDT, IDT, PIC, Heap)");
    println!("[ZIQA] Memory mapper initialized");
    println!("[ZIQA] ABI Plugin subsystem ready");
}

/// Initialize the ABI registry and register built-in plugins
pub fn init_abi_registry() -> abi::AbiRegistry {
    let mut registry = abi::AbiRegistry::new();

    // Register the Linux ELF plugin
    match registry.register(&abi::linux::LINUX_PLUGIN) {
        Ok(()) => println!("[ZIQA] Registered ABI plugin: {}", abi::linux::LINUX_PLUGIN.name()),
        Err(e) => println!("[ZIQA] Failed to register Linux plugin: {:?}", e),
    }

    match registry.register(&abi::wasm::WASM_PLUGIN) {
        Ok(()) => println!("[ZIQA] Registered ABI plugin: {}", abi::wasm::WASM_PLUGIN.name()),
        Err(e) => println!("[ZIQA] Failed to register WASM plugin: {:?}", e),
    }

    println!("[ZIQA] {} ABI plugin(s) loaded", registry.count());
    registry
}
