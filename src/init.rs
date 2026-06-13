//! Kernel initialization boundary.
//!
//! Graphify flagged the former `lib.rs`/`main.rs`/syscall/heap cluster as a
//! low-cohesion community because boot orchestration, ABI registration, memory
//! setup, and demos all crossed in one place. This module owns only early kernel
//! initialization and ABI plugin registration. Runtime demos remain in
//! `main.rs`; subsystem implementations remain under their owning modules.

use crate::{abi, arch, drivers, memory, process, BOOT_INFO};
use alloc::boxed::Box;

/// Initialize hardware, memory, scheduler, and core registries needed before
/// higher-level startup/demo code runs.
pub fn init(boot_info: &'static bootloader::BootInfo) {
    *BOOT_INFO.lock() = Some(boot_info);

    // Initialize per-CPU data for BSP early (sets up GS.base)
    arch::x86_64::per_cpu::init_bsp(0);

    // Hardware init.
    arch::x86_64::gdt::init();
    arch::x86_64::interrupts::init_idt();
    arch::x86_64::interrupts::init_pics();
    arch::x86_64::switch::init_syscalls();

    // Memory and heap init.
    use memory::paging::MemoryMapper;
    use x86_64::VirtAddr;
    let phys_offset = VirtAddr::new(boot_info.physical_memory_offset);

    unsafe {
        memory::frame_allocator::rmm_init_from_bootinfo(boot_info);
    }
    *memory::FRAME_ALLOCATOR.lock() = Some(memory::BootInfoFrameAllocator);

    let mut mapper = unsafe { MemoryMapper::new(phys_offset) };
    memory::heap::init_heap(
        &mut mapper.mapper,
        &mut *memory::FRAME_ALLOCATOR.lock().as_mut().unwrap(),
    )
    .expect("heap init failed");

    memory::paging::init_kernel_mapper(VirtAddr::new(boot_info.physical_memory_offset));

    // Scheduler
    process::scheduler::init();

    // Device/scheduler init.
    drivers::pci::init();
    drivers::device_manager::init();

    drivers::device_manager::DEVICE_MANAGER
        .lock()
        .register_driver(Box::new(
            crate::drivers::virtio_block_new::VirtioBlockDriverNew,
        ));
    drivers::virtio_block::register();
    drivers::ata::register();
    drivers::device_manager::DEVICE_MANAGER
        .lock()
        .register_driver(Box::new(crate::drivers::virtio_gpu::VirtioGpuDriver));
    drivers::xhci::register();
    drivers::ahci::register();
    drivers::nvme::register();
    drivers::audio::register();

    // Scan — match all registered drivers against PCI devices
    drivers::device_manager::scan();

    // Initialize Bochs Graphics Adapter if present
    crate::drivers::framebuffer::init_bga();

    drivers::acpi::init();

    // Register Mouse IPC Channel (Channel ID 1)
    crate::ipc::register_channel(1, alloc::sync::Arc::new(crate::ipc::Channel::new()));

    // Register Compositor IPC Channel (Channel ID 3)
    #[cfg(feature = "games")]
    {
        crate::ipc::register_channel(3, alloc::sync::Arc::new(crate::ipc::Channel::new()));
    }

    // Register Compositor Input Event Channel (Channel ID 4)
    #[cfg(feature = "games")]
    {
        crate::ipc::register_channel(4, alloc::sync::Arc::new(crate::ipc::Channel::new()));
    }

    // ── SMP / APIC Init ──────────────────────────────────────────────────
    if let Some(acpi_info) = &*drivers::acpi::ACPI_INFO.lock() {
        arch::x86_64::apic::enable_lapic_in_bsp();
        arch::x86_64::apic::init(acpi_info);
        let bsp_apic_id = arch::x86_64::apic::lapic_id();
        arch::x86_64::per_cpu::init_bsp(bsp_apic_id);
        arch::x86_64::apic::enable();
        arch::x86_64::apic::disable_pic();
        arch::x86_64::apic::redirect_irq(1, crate::arch::x86_64::interrupts::InterruptIndex::Keyboard as u8, bsp_apic_id);
        arch::x86_64::apic::redirect_irq(12, crate::arch::x86_64::interrupts::InterruptIndex::Mouse as u8, bsp_apic_id);
        let timer_count = arch::x86_64::apic::calibrate_timer(10);
        arch::x86_64::apic::start_periodic_timer(timer_count);
        arch::x86_64::smp::boot_aps(acpi_info);
    } else {
        crate::klog!(
            crate::klog::Level::Warn,
            "APIC: ACPI info not available, using legacy PIC"
        );
    }
    crate::println!(" ~ GDT, IDT, APIC, Heap, PCI, Sched ..... loaded");
    crate::println!(" ~ Memory mapper ...................... initialized");
    crate::println!(" ~ ABI plugins ........................ 2 registered");
}

/// Build the ABI registry and register built-in ABI plugins.
pub fn init_abi_registry() -> abi::AbiRegistry {
    let mut registry = abi::AbiRegistry::new();

    // Redox must be registered before Linux — both check ELF magic only,
    // and orbital (a Redox compositor) needs Redox syscall dispatching.
    registry.register(&abi::redox::REDOX_PLUGIN).ok();
    registry.register(&abi::linux::LINUX_PLUGIN).ok();
    #[cfg(feature = "wasm")]
    registry.register(&abi::wasm::WASM_PLUGIN).ok();

    registry
}
