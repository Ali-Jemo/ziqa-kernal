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

fn print_hex(val: u64) {
    let chars = b"0123456789ABCDEF";
    let mut buf = [0u8; 18];
    buf[0] = b'0';
    buf[1] = b'x';
    for i in 0..16 {
        buf[17 - i] = chars[((val >> (i * 4)) & 0xF) as usize];
    }
    unsafe {
        let mut serial = uart_16550::SerialPort::new(0x3f8);
        serial.init();
        let _ = core::fmt::Write::write_str(&mut serial, core::str::from_utf8_unchecked(&buf));
        let _ = core::fmt::Write::write_str(&mut serial, "\n");
    }
}

fn kernel_main(boot_info: &'static BootInfo) -> ! {
    unsafe {
        let mut serial = uart_16550::SerialPort::new(0x3f8);
        serial.init();
        let _ = core::fmt::Write::write_str(&mut serial, "=== KERNEL_MAIN ENTRY ===\n");
        let _rsp_val: u64;
        core::arch::asm!("mov {}, rsp", out(reg) _rsp_val);
        let _ = core::fmt::Write::write_str(&mut serial, "rsp at entry: ");
    }
    unsafe {
        let rsp: u64;
        core::arch::asm!("mov {}, rsp", out(reg) rsp);
        print_hex(rsp);
    }
    // 1. Core Init
    ziqa_kernel::init(boot_info);
    // print_banner(); // Removed to allow new boot screen
    KLOG.lock().min_level = Level::Debug;
    ziqa_kernel::klog!(Level::Info, "ZiqaKernel v1.0 booting");

    // 2. Subsystem Init
    ziqa_kernel::scheme::init();
    init_subsystems();

    // 3. Service/FS Setup
    init_services();

    // ── GPU Display / Compositor Init ───────────────────────────────────
    section("Display");
    if ziqa_kernel::drivers::virtio_gpu::is_available() {
        if !ziqa_kernel::drivers::framebuffer::is_bga_available() {
            ziqa_kernel::drivers::virtio_gpu::init_display();
            ziqa_kernel::process::scheduler::spawn_kthread(
                ziqa_kernel::drivers::virtio_gpu::gpu_ipc_listener,
                core::ptr::null(),
            );
            crate::println!(" ~ VirtIO GPU display ................... ready");
        } else {
            crate::println!(" ~ BGA Display .......................... ready");
        }
        // Spawn compositor kernel thread for window management
        ziqa_kernel::process::scheduler::spawn_kthread(
            ziqa_kernel::userspace::compositor::compositor_main,
            core::ptr::null(),
        );
    } else {
        crate::println!(" ~ VirtIO GPU / BGA Display ............ not available");
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
    ziqa_kernel::drivers::ps2_mouse::init();
    // Initialize VFS before running tests (tests may need filesystem access)
    ziqa_kernel::fs::vfs::VFS.write().init();
    set_fg(Color::White);
    section("Self-tests");
    ziqa_kernel::tests::run_all();
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

        // Busybox binary
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

    // Disk filesystems
    {
        block_registry::print_devices();

        if let Some(entry) = block_registry::first() {
            let disk = entry.device.clone();
            println!(
                " ~ root disk .......................... /dev/{} ({})",
                entry.name, entry.driver
            );

            // 1. Try to mount FAT32 (host-editable)
            #[cfg(feature = "fat32")]
            let fat32_mounted = {
                use ziqa_kernel::fs::fat32;
                if let Some((start, _size)) = fat32::find_fat32_partition(&*disk) {
                    println!(" ~ FAT32 partition found at sector {}", start);
                    match fat32::mount_fat32(disk.clone(), start, "/fat") {
                        Ok(()) => {
                            ziqa_kernel::fs::vfs::register_mount(
                                &alloc::format!("/dev/{}", entry.name),
                                "/fat",
                                "fat32",
                            );
                            println!(" ~ FAT32 ............................. mounted at /fat");
                            true
                        }
                        Err(e) => {
                            println!(" ~ FAT32 mount failed: {}", e);
                            false
                        }
                    }
                } else {
                    false
                }
            };
            #[cfg(not(feature = "fat32"))]
            let fat32_mounted = false;

            // 2. Try to mount ZiqaFS
            let ziqafs_result = ZiqaFs::mount(disk.clone());

            if let Ok(ziqafs) = ziqafs_result {
                ziqa_kernel::fs::ziqafs::mount_into_vfs(&ziqafs);
                ziqa_kernel::fs::vfs::register_mount(
                    &alloc::format!("/dev/{}", entry.name),
                    "/disk",
                    "ziqafs",
                );
                println!(" ~ ZiqaFS ............................. mounted at /disk");
            } else if !fat32_mounted {
                // Disk is blank or unrecognized, and no FAT32 data to protect. Safe to format.
                let ziqafs = ZiqaFs::format(disk.clone()).expect("Failed to format ZiqaFS");
                ziqa_kernel::fs::ziqafs::mount_into_vfs(&ziqafs);
                ziqa_kernel::fs::vfs::register_mount(
                    &alloc::format!("/dev/{}", entry.name),
                    "/disk",
                    "ziqafs",
                );
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
fn run_startup() {
    // ziqa_kernel::drivers::vga::clear_screen(); // Removed to keep new boot screen visible
    ziqa_kernel::drivers::uart::VGA_ENABLED.store(true, core::sync::atomic::Ordering::Relaxed);
    section("Startup");
    set_fg(Color::LightGreen);

    if let Some(pid) = ziqa_kernel::process::scheduler::spawn_kthread(
        |_| ziqa_kernel::drivers::mouse_server::run_mouse_server(1),
        core::ptr::null(),
    ) {
        println!(
            " ✓ PS/2 Mouse IPC Server ............... spawned: {:?}",
            pid
        );
    } else {
        println!(" ! Failed to spawn PS/2 Mouse IPC Server");
    }

    // Spawn the built-in test ELF as a user process (DEFERRED)
    println!(" ~ Deferred user process spawn");
    /*
    let binary = include_bytes!("../assets/test_elf.bin");
    if let Some(pid) = ziqa_kernel::process::scheduler::spawn_elf(binary) {
        println!(
            " ✓ Spawned user process pid={} ............ from test_elf.bin",
            pid.0
        );
    } else {
        println!(" ! Failed to spawn user process");
    }
    */

    // Spawn built-in compositor demo client (kernel thread)
    println!(" ~ Deferred compositor demo client spawn");
    /*
    #[cfg(feature = "games")]
    {
        println!(" ~ Spawning compositor demo client...");
        ziqa_kernel::process::scheduler::spawn_kthread(
            ziqa_kernel::userspace::demo_client::demo_client_main,
            core::ptr::null(),
        );
        println!(" ✓ Demo compositor client ................ spawned");
    }
    */

    // Spawn verification log dumper
    #[cfg(feature = "games")]
    {
        ziqa_kernel::process::scheduler::spawn_kthread(verify_logger, core::ptr::null());
    }

    // Spawn the userspace keyboard driver
    println!(" ~ Spawning keyboard driver...");
    let kb_driver_bin = include_bytes!("../userspace/keyboard_driver.elf");
    if let Some(pid) = ziqa_kernel::process::scheduler::spawn_elf(kb_driver_bin) {
        println!("[DEBUG startup] spawn_elf returned pid={}", pid.0);
        ziqa_kernel::process::scheduler::with_process_mut(pid, |proc| {
            proc.capabilities.grant(
                ziqa_kernel::capability::ResourceKind::DeviceIo,
                ziqa_kernel::capability::Permissions::full(),
                0,
                None,
            );
        });
        println!("[DEBUG startup] with_process_mut done");
        println!(
            " ✓ Spawned Userspace Keyboard Driver pid={} .... from userspace/keyboard_driver.elf",
            pid.0
        );
    } else {
        println!(" ! Failed to spawn Userspace Keyboard Driver");
    }
    // Spawn Doom as a user-space process
    #[cfg(feature = "games")]
    {
        let doom_binary = include_bytes!("../zig-out/bin/doom");
        if let Some(pid) = ziqa_kernel::process::scheduler::spawn_elf(doom_binary) {
            println!(
                " ✓ Spawned Doom pid={} ..................... from zig-out/bin/doom",
                pid.0
            );
        } else {
            println!(" ! Failed to spawn Doom process (games feature enabled but doom.elf may need rebuilding)");
        }
    }
    // Do not yield here: preemption is intentionally gated until the shell has
    // printed its first prompt, so user tasks cannot steal the boot handoff.

    // Restore any saved snapshots for instant-on resume
    let binary = include_bytes!("../assets/test_elf.bin");
    let restored = ziqa_kernel::process::snapshot::restore_all_at_boot(binary);
    if restored > 0 {
        set_fg(Color::LightGreen);
        println!(
            " ✓ Instant-on resume: {} processes restored from snapshot",
            restored
        );
        set_fg(Color::White);
    }

    println!(" ✓ ZiqaKernel v1.0 ready ................ type 'help' for shell");
    set_fg(Color::White);

    // Dump boot logs for verification
    #[cfg(feature = "games")]
    {
        println!("\n── Boot log dump ──");
        ziqa_kernel::klog::KLOG.lock().dump();
        println!("───────────────────\n");
    }
    ziqa_kernel::boot_screen::show_boot_screen();
}
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("\n[PANIC] {}", info);
    loop {
        x86_64::instructions::hlt();
    }
}
