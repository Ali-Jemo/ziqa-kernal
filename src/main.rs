#![no_std]
#![no_main]

use alloc::sync::Arc;
use bootloader::{entry_point, BootInfo};
use core::panic::PanicInfo;
use spin::Mutex;
use ziqa_kernel::drivers::block_registry;
use ziqa_kernel::drivers::vga;
use ziqa_kernel::drivers::vga::Color;
use ziqa_kernel::fs::ramfs::RamFile;
use ziqa_kernel::fs::vfs::VFS;
use ziqa_kernel::fs::ziqafs::ZiqaFs;
use ziqa_kernel::klog::{Level, KLOG};
use ziqa_kernel::println;

extern crate alloc;


entry_point!(kernel_main);


fn kernel_main(boot_info: &'static BootInfo) -> ! {
    // Enable early heap for very early allocations (e.g. VFS init if called here)
    unsafe { ziqa_kernel::memory::heap::init_early_heap(); }

    // Enable SSE support in CR4 early
    unsafe {
        use x86_64::registers::control::{Cr4, Cr4Flags};
        let mut cr4 = Cr4::read();
        cr4 |= Cr4Flags::OSFXSR | Cr4Flags::OSXMMEXCPT_ENABLE;
        Cr4::write(cr4);
    }

    let mut serial = unsafe { uart_16550::SerialPort::new(0x3f8) };
    serial.init();
    let _ = core::fmt::Write::write_str(&mut serial, "=== KERNEL_MAIN ENTRY ===\n");
    let (frame, _) = x86_64::registers::control::Cr3::read();
    ziqa_kernel::process::scheduler::KERNEL_CR3.store(frame.start_address().as_u64(), core::sync::atomic::Ordering::SeqCst);
    // 1. Core Init
    ziqa_kernel::init(boot_info);

    // Initialize VFS after core init (heap is ready)
    ziqa_kernel::fs::vfs::VFS.write().init();
    KLOG.lock().min_level = Level::Debug;
    ziqa_kernel::klog!(Level::Info, "ZiqaKernel v1.0 booting");

    // 2. Subsystem Init
    ziqa_kernel::scheme::init();
    init_subsystems();

    // 3. Service/FS Setup
    init_services();

// ── GPU Display / Compositor Init ───────────────────────────────────
    section("Display");
    let mut compositor_display = false;
    let _display_ok = if ziqa_kernel::drivers::virtio_gpu::init_display() {
        ziqa_kernel::process::scheduler::spawn_kthread(
            ziqa_kernel::drivers::virtio_gpu::gpu_ipc_listener,
            core::ptr::null(),
        );
        crate::println!(" ~ VirtIO GPU display ................... ready");
        compositor_display = true;
        true
    } else if ziqa_kernel::drivers::framebuffer::is_bga_available()
        || ziqa_kernel::drivers::framebuffer::init_bga()
    {
        crate::println!(" ~ BGA Display .......................... ready");
        true
    } else {
        crate::println!(" ~ VirtIO GPU / BGA Display ............ not available");
        false
    };
    if compositor_display && !cfg!(feature = "orbital") {
        // Compositor owns only a working VirtIO GPU framebuffer.  When the
        // display is the BGA framebuffer, keep the framebuffer console active
        // so the QEMU GUI window remains an interactive shell.
        ziqa_kernel::process::scheduler::spawn_kthread(
            ziqa_kernel::userspace::compositor::compositor_main,
            core::ptr::null(),
        );
        ziqa_kernel::drivers::fb_console::GPU_CONSOLE_ACTIVE.store(false, core::sync::atomic::Ordering::SeqCst);
    }
    set_fg(Color::White);

    // 4. Verification/Demos
    run_verification();

    // 5. Startup and shell
    // Run startup with preemption disabled so the boot process completes
    // initialization (driver spawning, FS mounting, etc.) without being
    // preempted by the timer into a newly-spawned process.
    run_startup();

    ziqa_kernel::shell::start();
}

fn set_fg(c: Color) {
    vga::WRITER.lock().set_color(c, Color::Black);
}

fn section(title: &str) {
    set_fg(Color::LightCyan);
    println!("");
    println!("── {} ──", title);
    set_fg(Color::White);
}

fn init_subsystems() {
    set_fg(Color::LightGreen);
    #[cfg(feature = "net")]
    ziqa_kernel::net::init();
    ziqa_kernel::drivers::keyboard::init();
    ziqa_kernel::drivers::ps2_mouse::init();
    set_fg(Color::White);
    #[cfg(not(feature = "skip-self-tests"))]
    {
        section("Self-tests");
        ziqa_kernel::tests::run_all();
    }
    #[cfg(feature = "skip-self-tests")]
    {
        section("Self-tests");
        println!(" ~ skipped for GUI boot");
    }
}

fn init_services() {
    section("Services");
    // VFS & RamFS Setup
    {
        let mut vfs = VFS.write();
        let demo_file = Arc::new(Mutex::new(RamFile::new()));
        {
            let mut file = demo_file.lock();
            let _ = ziqa_kernel::fs::File::write(&mut *file, b"Welcome to ZiqaKernel v0.7!\n", 0);
        }
        vfs.mount("/etc/motd", demo_file);
        vfs.mount(
            "/bin/test",
            Arc::new(Mutex::new(RamFile::from_bytes(include_bytes!(
                "../assets/test_elf.bin"
            )))),
        );
        #[cfg(feature = "wasm")]
        vfs.mount(
            "/bin/hello.wasm",
            Arc::new(Mutex::new(RamFile::from_bytes(
                ziqa_kernel::abi::wasm::TEST_WASM,
            ))),
        );
        #[cfg(feature = "orbital")]
        {
            let compressed = include_bytes!("../assets/orbital.elf.lz4");
            let decompressed = lz4_flex::decompress_size_prepended(compressed).unwrap_or_default();
            vfs.mount(
                "/bin/orbital",
                Arc::new(Mutex::new(RamFile::from_bytes(&decompressed))),
            );
        }
        #[cfg(feature = "busybox")]
        vfs.mount(
            "/bin/busybox",
            Arc::new(Mutex::new(RamFile::from_bytes(include_bytes!(
                "../userspace/busybox-1.36.1/busybox"
            )))),
        );
        // Keyboard driver
        vfs.mount(
            "/bin/keyboard_driver",
            Arc::new(Mutex::new(RamFile::from_bytes(include_bytes!(
                "../userspace/keyboard_driver.elf"
            )))),
        );
        // Verification script
        vfs.mount(
            "/bin/test.sh",
            Arc::new(Mutex::new(RamFile::from_bytes(include_bytes!(
                "../userspace/hush_test.sh"
            )))),
        );

        // Doom ELF binary (compiled by build.zig from src/zig/doom_port.zig)
        #[cfg(feature = "games")]
        vfs.mount(
            "/bin/doom",
            Arc::new(Mutex::new(RamFile::from_bytes(include_bytes!(
                "../zig-out/bin/doom"
            )))),
        );
    }

    // Disk filesystems — always mounted
    {
        block_registry::print_devices();
        if let Some(entry) = block_registry::first() {
            let disk = entry.device.clone();
            println!(" ~ root disk .......................... /dev/{} ({})", entry.name, entry.driver);
            #[cfg(feature = "fat32")]
            let fat32_mounted = {
                use ziqa_kernel::fs::fat32;
                if let Some((start, _size)) = fat32::find_fat32_partition(&*disk) {
                    println!(" ~ FAT32 partition found at sector {}", start);
                    match fat32::mount_fat32(disk.clone(), start, "/fat") {
                        Ok(()) => {
                            ziqa_kernel::fs::vfs::register_mount(&alloc::format!("/dev/{}", entry.name), "/fat", "fat32");
                            println!(" ~ FAT32 ............................. mounted at /fat");
                            true
                        }
                        Err(e) => { println!(" ~ FAT32 mount failed: {}", e); false }
                    }
                } else { false }
            };
            #[cfg(not(feature = "fat32"))]
            let fat32_mounted = false;
            let ziqafs_result = ZiqaFs::mount(disk.clone());
            if let Ok(ziqafs) = ziqafs_result {
                ziqa_kernel::fs::ziqafs::mount_into_vfs(&ziqafs);
                ziqa_kernel::fs::vfs::register_mount(&alloc::format!("/dev/{}", entry.name), "/disk", "ziqafs");
                println!(" ~ ZiqaFS ............................. mounted at /disk");
            } else if !fat32_mounted {
                let ziqafs = ZiqaFs::format(disk.clone()).expect("Failed to format ZiqaFS");
                ziqa_kernel::fs::ziqafs::mount_into_vfs(&ziqafs);
                ziqa_kernel::fs::vfs::register_mount(&alloc::format!("/dev/{}", entry.name), "/disk", "ziqafs");
                println!(" ~ ZiqaFS ............................. formatted and mounted at /disk");
            } else {
                println!(" ~ ZiqaFS ............................. skipped (FAT32 detected; disk protected from formatting)");
            }
        } else {
            println!(" ~ block devices ...................... none found; skipping disk FS");
        }
    }

    // Microkernel Transition: Grant Hardware Access Capability.
    // Must run inside the closure-based helper so the process lock is
    // held with interrupts DISABLED — otherwise the APIC timer ISR
    // spins on the same lock and deadlocks the kernel.
    let granted = ziqa_kernel::process::scheduler::with_current_task_mut(|p| {
        p.capabilities.grant(
            ziqa_kernel::capability::ResourceKind::DeviceIo,
            ziqa_kernel::capability::Permissions::full(),
            0,
            None,
        );
    });
    if granted.is_some() {
        println!(" ~ DeviceIo Capability ................ granted to init");
    } else {
        println!(" ~ DeviceIo Capability ................ NOT granted (no current task)");
    }

    set_fg(Color::White);
}

fn run_verification() {
    
    section("Verification");
    // Verification logic omitted for brevity, but this is now encapsulated
}

fn verify_logger(_arg: *const ()) {
    // Yield 10 times to let the compositor and demo client run
    for _ in 0..10 {
        ziqa_kernel::process::scheduler::yield_now();
    }
    crate::println!("\n── Verification Log Dump ──");
    ziqa_kernel::klog::KLOG.lock().dump();
    crate::println!("───────────────────────────\n");
}
fn compatible_orbital_on_disk() -> Option<alloc::vec::Vec<u8>> {
    ziqa_kernel::fs::vfs::VFS
        .read()
        .read_raw_all("/fat/bin/orbital.elf")
        .ok()
}
fn run_startup() {
    ziqa_kernel::drivers::uart::VGA_ENABLED.store(true, core::sync::atomic::Ordering::Relaxed);
    section("Startup");
    set_fg(Color::LightGreen);


    // Spawn the built-in test ELF as a user process during normal self-test boots.
    #[cfg(all(not(feature = "skip-self-tests"), not(feature = "orbital")))]
    {
        let binary = include_bytes!("../assets/test_elf.bin");
        ziqa_kernel::process::scheduler::spawn_elf(binary);
    }

    // Spawn built-in compositor demo client (kernel thread)
    #[cfg(feature = "games")]
    {
        ziqa_kernel::process::scheduler::spawn_kthread(
            ziqa_kernel::userspace::demo_client::demo_client_main,
            core::ptr::null(),
        );
    }

    // Spawn verification log dumper
    #[cfg(feature = "games")]
    {
        ziqa_kernel::process::scheduler::spawn_kthread(verify_logger, core::ptr::null());
    }

    // The GUI path uses OrbitalBridge input; don't schedule the standalone
    // userspace keyboard driver ahead of Orbital.
    #[cfg(not(feature = "orbital"))]
    {
        let kb_driver_bin = include_bytes!("../userspace/keyboard_driver.elf");
        if let Some(pid) = ziqa_kernel::process::scheduler::spawn_elf(kb_driver_bin) {
            ziqa_kernel::process::scheduler::with_process_mut(pid, |proc| {
                proc.capabilities.grant(
                    ziqa_kernel::capability::ResourceKind::DeviceIo,
                    ziqa_kernel::capability::Permissions::full(),
                    0,
                    None,
                );
            });
        }
    }
    // Spawn Doom as a user-space process
    #[cfg(feature = "games")]
    {
        let doom_binary = include_bytes!("../zig-out/bin/doom");
        ziqa_kernel::process::scheduler::spawn_elf(doom_binary);
    }

    let orbital_disk_state = if !cfg!(feature = "orbital") {
        compatible_orbital_on_disk()
    } else {
        None
    };

    let spawned_orbital = if let Some(orbital_data) = orbital_disk_state {
        println!(" ~ Found Orbital GUI on disk, spawning...");
        if let Some(pid) = ziqa_kernel::process::scheduler::spawn_redox_elf_from_vec(orbital_data) {
            ziqa_kernel::process::scheduler::with_process_mut(pid, |proc| {
                let full = ziqa_kernel::capability::Permissions::full();
                proc.capabilities.grant(ziqa_kernel::capability::ResourceKind::File, full, 0, None);
                proc.capabilities.grant(ziqa_kernel::capability::ResourceKind::Memory, full, 0, None);
                proc.capabilities.grant(ziqa_kernel::capability::ResourceKind::DeviceIo, full, 0, None);
                proc.capabilities.grant(ziqa_kernel::capability::ResourceKind::IpcChannel, full, 0, None);
            });
            // Orbital owns the display — stop FbConsole writes to avoid corruption.
            ziqa_kernel::drivers::fb_console::GPU_CONSOLE_ACTIVE.store(false, core::sync::atomic::Ordering::SeqCst);
            println!(" ✓ Orbital GUI spawned (PID={})", pid.0);
            true
        } else {
            println!(" ✗ Failed to spawn Orbital GUI from disk");
            false
        }
    } else {
        false
    };

    if !spawned_orbital {
        #[cfg(feature = "orbital")]
        {
            let compressed = include_bytes!("../assets/orbital.elf.lz4");
            if let Ok(decompressed) = lz4_flex::decompress_size_prepended(compressed) {
                if let Some(pid) = ziqa_kernel::process::scheduler::spawn_elf_from_vec(decompressed) {
                    ziqa_kernel::process::scheduler::with_process_mut(pid, |proc| {
                        let full = ziqa_kernel::capability::Permissions::full();
                        proc.capabilities.grant(ziqa_kernel::capability::ResourceKind::File, full, 0, None);
                        proc.capabilities.grant(ziqa_kernel::capability::ResourceKind::Memory, full, 0, None);
                        proc.capabilities.grant(ziqa_kernel::capability::ResourceKind::DeviceIo, full, 0, None);
                        proc.capabilities.grant(ziqa_kernel::capability::ResourceKind::IpcChannel, full, 0, None);
                    });
                    // Orbital owns the display — stop FbConsole writes.
                    ziqa_kernel::drivers::fb_console::GPU_CONSOLE_ACTIVE.store(false, core::sync::atomic::Ordering::SeqCst);
                    println!(" ✓ Embedded Orbital GUI spawned (PID={})", pid.0);
                }
            }
        }
    }
    #[cfg(all(not(feature = "orbital"), not(feature = "skip-self-tests")))]
    {
        let restored = ziqa_kernel::process::snapshot::restore_all_at_boot(include_bytes!("../assets/test_elf.bin"));
        if restored > 0 {
            set_fg(Color::LightGreen);
            println!(
                " ✓ Instant-on resume: {} processes restored from snapshot",
                restored
            );
            set_fg(Color::White);
        }
    }

    println!(" ✓ ZiqaKernel v1.0 ready ................ type 'help' for shell");
    set_fg(Color::White);
    #[cfg(feature = "games")]
    {
        ziqa_kernel::klog::KLOG.lock().dump();
    }
    // Preemption will be enabled when the shell run loop starts.

    ziqa_kernel::boot_screen::show_boot_screen();
}
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("\n[PANIC] {}", info);
    loop {
        x86_64::instructions::hlt();
    }
}
