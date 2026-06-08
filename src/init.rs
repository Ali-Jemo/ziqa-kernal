//! Kernel initialization boundary.
//!
//! Graphify flagged the former `lib.rs`/`main.rs`/syscall/heap cluster as a
//! low-cohesion community because boot orchestration, ABI registration, memory
//! setup, and demos all crossed in one place. This module owns only early kernel
//! initialization and ABI plugin registration. Runtime demos remain in
//! `main.rs`; subsystem implementations remain under their owning modules.

use crate::{abi, arch, drivers, memory, process, BOOT_INFO};
use alloc::boxed::Box;

fn raw_serial_log(s: &str) {
    unsafe {
        let mut serial = uart_16550::SerialPort::new(0x3f8);
        serial.init();
        let _ = core::fmt::Write::write_str(&mut serial, s);
    }
}

/// Initialize hardware, memory, scheduler, and core registries needed before
/// higher-level startup/demo code runs.
pub fn init(boot_info: &'static bootloader::BootInfo) {
    *BOOT_INFO.lock() = Some(boot_info);

    raw_serial_log("init: starting bsp\n");
    // Initialize per-CPU data for BSP early (sets up GS.base)
    arch::x86_64::per_cpu::init_bsp(0);
    raw_serial_log("init: bsp done, starting GDT\n");

    // Hardware init.
    arch::x86_64::gdt::init();
    raw_serial_log("init: GDT done, starting IDT\n");
    arch::x86_64::interrupts::init_idt();
    raw_serial_log("init: IDT done, starting PIC\n");
    arch::x86_64::interrupts::init_pics();
    raw_serial_log("init: PIC done, starting Syscalls\n");
    arch::x86_64::switch::init_syscalls();
    raw_serial_log("init: early hardware done\n");

    raw_serial_log("init: klogs done, starting memory init\n");

    // Memory and heap init.
    use memory::BootInfoFrameAllocator;
    use memory::paging::MemoryMapper;
    use x86_64::VirtAddr;
    let phys_offset = VirtAddr::new(boot_info.physical_memory_offset);
    raw_serial_log("init: doing frame allocator init\n");

    *memory::FRAME_ALLOCATOR.lock() = Some(BootInfoFrameAllocator);
    raw_serial_log("init: frame allocator done, starting heap init\n");

    let mut mapper = unsafe { MemoryMapper::new(phys_offset) };
    memory::heap::init_heap(
        &mut mapper.mapper,
        &mut *memory::FRAME_ALLOCATOR.lock().as_mut().unwrap(),
    )
    .expect("heap init failed");
    raw_serial_log("init: heap done, starting kernel mapper\n");

    memory::paging::init_kernel_mapper(VirtAddr::new(boot_info.physical_memory_offset));
    raw_serial_log("init: kernel mapper done\n");

    // Device/scheduler init.
    drivers::pci::init();
    drivers::device_manager::init();
    
    drivers::device_manager::DEVICE_MANAGER.lock().register_driver(Box::new(crate::drivers::virtio_block_new::VirtioBlockDriverNew));
    drivers::virtio_block::register();
    drivers::ata::register();
    
    drivers::device_manager::DEVICE_MANAGER.lock().scan_and_match();
    
    drivers::acpi::init();

    // Register Mouse IPC Channel (Channel ID 1)
    crate::ipc::register_channel(1, alloc::sync::Arc::new(crate::ipc::Channel::new()));
    // IPC Server Spawn: crate::process::scheduler::spawn_kthread(move || crate::drivers::mouse_server::run_mouse_server(1));

    // ── SMP / APIC Init ──────────────────────────────────────────────────
    if let Some(acpi_info) = &*drivers::acpi::ACPI_INFO.lock() {
        raw_serial_log("init: SMP/APIC init started\n");
        crate::klog!(crate::klog::Level::Info, "APIC: initializing...");

        raw_serial_log("init: enabling lapic on BSP\n");
        arch::x86_64::apic::enable_lapic_in_bsp();
        raw_serial_log("init: enabling lapic on BSP done, initializing APIC\n");
        arch::x86_64::apic::init(acpi_info);

        raw_serial_log("init: initializing BSP per_cpu\n");
        let bsp_apic_id = arch::x86_64::apic::lapic_id();
        arch::x86_64::per_cpu::init_bsp(bsp_apic_id);

        raw_serial_log("init: enabling APIC\n");
        arch::x86_64::apic::enable();
        raw_serial_log("init: disabling PIC\n");
        arch::x86_64::apic::disable_pic();

        raw_serial_log("init: calibrating APIC timer\n");
        let timer_count = arch::x86_64::apic::calibrate_timer(5);
        raw_serial_log("init: starting APIC timer\n");
        arch::x86_64::apic::start_periodic_timer(timer_count);

        raw_serial_log("init: booting APs\n");
        arch::x86_64::smp::boot_aps(acpi_info);
        raw_serial_log("init: SMP/APIC init done\n");
    } else {
        crate::klog!(crate::klog::Level::Warn, "APIC: ACPI info not available, using legacy PIC");
    }

    #[cfg(feature = "drm")]
    drivers::drm::init();

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
        cpu_features.contains(arch::x86_64::cpu_features::CpuFeatures::SMEP),
        cpu_features.contains(arch::x86_64::cpu_features::CpuFeatures::SMAP),
        cpu_features.contains(arch::x86_64::cpu_features::CpuFeatures::UMIP),
    );

    raw_serial_log("init: calling scheduler::init\n");
    process::scheduler::init();
    raw_serial_log("init: scheduler::init completed\n");

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
