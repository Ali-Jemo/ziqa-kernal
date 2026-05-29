//! Kernel initialization boundary.
//!
//! Graphify flagged the former `lib.rs`/`main.rs`/syscall/heap cluster as a
//! low-cohesion community because boot orchestration, ABI registration, memory
//! setup, and demos all crossed in one place. This module owns only early kernel
//! initialization and ABI plugin registration. Runtime demos remain in
//! `main.rs`; subsystem implementations remain under their owning modules.

use crate::{abi, arch, drivers, memory, process, BOOT_INFO};

/// Initialize hardware, memory, scheduler, and core registries needed before
/// higher-level startup/demo code runs.
pub fn init(boot_info: &'static bootloader::BootInfo) {
    *BOOT_INFO.lock() = Some(boot_info);

    // Hardware init.
    arch::x86_64::gdt::init();
    arch::x86_64::interrupts::init_idt();
    arch::x86_64::interrupts::init_pics();
    arch::x86_64::switch::init_syscalls();

    // Enable SMEP/SMAP/UMIP if the CPU supports them (CR4 bits 20/21/11).
    // Must run after GDT/IDT are loaded so a #PF from a bad kernel pointer
    // is handled rather than triple-faulting.
    let cpu_features = arch::x86_64::cpu_features::init();
    // Verify the bits actually stuck in CR4 (paranoid check).
    if let Err(missing) = arch::x86_64::cpu_features::verify(cpu_features) {
        crate::klog!(
            crate::klog::Level::Warn,
            "CPU features: CR4 write did not stick for bits 0x{:02x}",
            missing.0,
        );
    }
    crate::klog!(
        crate::klog::Level::Info,
        "CPU features: SMEP={} SMAP={} UMIP={}",
        cpu_features.contains(arch::x86_64::cpu_features::CpuFeatures::SMEP),
        cpu_features.contains(arch::x86_64::cpu_features::CpuFeatures::SMAP),
        cpu_features.contains(arch::x86_64::cpu_features::CpuFeatures::UMIP),
    );

    // Memory and heap init.
    use memory::frame_allocator::BootInfoFrameAllocator;
    use x86_64::VirtAddr;
    let phys_offset = VirtAddr::new(boot_info.physical_memory_offset);
    let mut mapper = unsafe { memory::frame_allocator::init(phys_offset) };
    let frame_alloc = unsafe { BootInfoFrameAllocator::init(&boot_info.memory_map) };

    *memory::FRAME_ALLOCATOR.lock() = Some(frame_alloc);

    memory::heap::init_heap(
        &mut mapper,
        &mut *memory::FRAME_ALLOCATOR.lock().as_mut().unwrap(),
    )
    .expect("heap init failed");

    memory::paging::init_kernel_mapper(VirtAddr::new(boot_info.physical_memory_offset));

    // Device/scheduler init.
    drivers::drm::init();
    process::scheduler::init();

    crate::println!(" ~ GDT, IDT, PIC, Heap ................ loaded");
    crate::println!(" ~ Memory mapper ...................... initialized");
    crate::println!(" ~ ABI plugins ........................ 2 registered");
}

/// Build the ABI registry and register built-in ABI plugins.
pub fn init_abi_registry() -> abi::AbiRegistry {
    let mut registry = abi::AbiRegistry::new();

    registry.register(&abi::linux::LINUX_PLUGIN).ok();
    registry.register(&abi::wasm::WASM_PLUGIN).ok();

    registry
}
