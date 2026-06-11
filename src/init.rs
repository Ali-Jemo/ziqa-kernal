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

    crate::println!("init: starting bsp");
    // Initialize per-CPU data for BSP early (sets up GS.base)
    arch::x86_64::per_cpu::init_bsp(0);
    crate::println!("init: bsp done, starting GDT");

    // Hardware init.
    arch::x86_64::gdt::init();
    crate::println!("init: GDT done, starting IDT");
    arch::x86_64::interrupts::init_idt();
    crate::println!("init: IDT done, starting PIC");
    arch::x86_64::interrupts::init_pics();
    crate::println!("init: PIC done, starting Syscalls");
    arch::x86_64::switch::init_syscalls();
    crate::println!("init: early hardware done");

    crate::println!("init: klogs done, starting memory init");

    // Memory and heap init.
    use memory::paging::MemoryMapper;
    use x86_64::VirtAddr;
    let phys_offset = VirtAddr::new(boot_info.physical_memory_offset);
    crate::println!("init: physical_memory_offset = {:#x}", phys_offset.as_u64());
    crate::println!("init: doing frame allocator init");

    unsafe {
        memory::frame_allocator::rmm_init_from_bootinfo(boot_info);
    }
    *memory::FRAME_ALLOCATOR.lock() = Some(memory::BootInfoFrameAllocator);
    crate::println!("init: frame allocator done, starting heap init");

    let mut mapper = unsafe { MemoryMapper::new(phys_offset) };
    memory::heap::init_heap(
        &mut mapper.mapper,
        &mut *memory::FRAME_ALLOCATOR.lock().as_mut().unwrap(),
    )
    .expect("heap init failed");
    crate::println!("init: heap done, starting kernel mapper");

    memory::paging::init_kernel_mapper(VirtAddr::new(boot_info.physical_memory_offset));
    crate::println!("init: kernel mapper done");

    crate::println!("init: initializing scheduler");
    process::scheduler::init();
    crate::println!("init: scheduler initialized (running as PID 0)");
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
    // Used by the kernel-mode compositor thread and userspace clients.
    #[cfg(feature = "games")]
    {
        crate::ipc::register_channel(3, alloc::sync::Arc::new(crate::ipc::Channel::new()));
        crate::println!(" ~ Compositor channel 3 ................ registered");
    }

    // Register Compositor Input Event Channel (Channel ID 4)
    // Compositor sends keyboard/mouse events here for any client to read.
    #[cfg(feature = "games")]
    {
        crate::ipc::register_channel(4, alloc::sync::Arc::new(crate::ipc::Channel::new()));
        crate::println!(" ~ Compositor event channel 4 ............ registered");
    }

    // ── SMP / APIC Init ──────────────────────────────────────────────────
    if let Some(acpi_info) = &*drivers::acpi::ACPI_INFO.lock() {
        crate::println!("init: SMP/APIC init started");
        crate::klog!(crate::klog::Level::Info, "APIC: initializing...");

        crate::println!("init: enabling lapic on BSP");
        arch::x86_64::apic::enable_lapic_in_bsp();
        crate::println!("init: enabling lapic on BSP done, initializing APIC");
        arch::x86_64::apic::init(acpi_info);

        crate::println!("init: initializing BSP per_cpu");
        let bsp_apic_id = arch::x86_64::apic::lapic_id();
        arch::x86_64::per_cpu::init_bsp(bsp_apic_id);

        crate::println!("init: enabling APIC");
        arch::x86_64::apic::enable();
        crate::println!("init: disabling PIC");
        arch::x86_64::apic::disable_pic();

        crate::println!("init: calibrating APIC timer");
        let timer_count = arch::x86_64::apic::calibrate_timer(5);
        crate::println!("init: starting APIC timer");
        arch::x86_64::apic::start_periodic_timer(timer_count);

        crate::println!("init: booting APs");
        arch::x86_64::smp::boot_aps(acpi_info);
        crate::println!("init: SMP/APIC init done");
    } else {
        crate::klog!(
            crate::klog::Level::Warn,
            "APIC: ACPI info not available, using legacy PIC"
        );
    }

    #[cfg(feature = "drm")]
    {
        drivers::drm::init();
    }

    // Compositor channel is registered above (ID 3).
    // The compositor kernel thread is spawned in main.rs after init_display.
    #[cfg(not(feature = "games"))]
    crate::println!(" ~ Compositor .......................... disabled (games feature)");
    // Enable SMEP/SMAP/UMIP if the CPU supports them (CR4 bits 20/21/11).
    // CPU security features are enabled LAST. SMAP in particular would
    // fault the moment the kernel touches a user-accessible page without
    // a STAC bracket; we need heap-backed printing to be safe first.
    let cpu_features = arch::x86_64::cpu_features::init();
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
        cpu_features.contains(crate::arch::x86_64::cpu_features::CpuFeatures::SMEP),
        cpu_features.contains(crate::arch::x86_64::cpu_features::CpuFeatures::SMAP),
        cpu_features.contains(crate::arch::x86_64::cpu_features::CpuFeatures::UMIP),
    );

    crate::println!("init: finishing hardware setup");

    crate::println!(" ~ GDT, IDT, APIC, Heap ................ loaded");
    crate::println!(" ~ Memory mapper ...................... initialized");
    crate::println!(" ~ ABI plugins ........................ 2 registered");
}

/// Build the ABI registry and register built-in ABI plugins.
pub fn init_abi_registry() -> abi::AbiRegistry {
    let mut registry = abi::AbiRegistry::new();

    registry.register(&abi::linux::LINUX_PLUGIN).ok();
    #[cfg(feature = "wasm")]
    registry.register(&abi::wasm::WASM_PLUGIN).ok();

    registry
}
