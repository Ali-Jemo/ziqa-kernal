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
    print_banner();
    KLOG.lock().min_level = Level::Debug;
    ziqa_kernel::klog!(Level::Info, "ZiqaKernel v1.0 booting");

    // 2. Subsystem Init
    ziqa_kernel::scheme::init();
    init_subsystems();

    // 3. Service/FS Setup
    init_services();

    // 4. Verification/Demos
    run_verification();

    // 5. Startup and shell
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

fn print_banner() {
    vga::clear_screen();
    set_fg(Color::LightCyan);
    println!("╔══════════════════════════════════════════════════════════════════════════════╗");
    set_fg(Color::White);
    println!("║                                                                              ║");
    set_fg(Color::LightGreen);
    println!("║                         ░░  ZIQA KERNEL  ░░  v1.0                            ║");
    set_fg(Color::Yellow);
    println!("║                       ░░░░  From scratch, for learning  ░░░░                  ║");
    set_fg(Color::White);
    println!("║                                                                              ║");
    set_fg(Color::LightCyan);
    println!("║        ▓ 23 modules   ▓ 100+ syscalls  ▓ MLFQ sched   ▓ eBPF VM              ║");
    println!("║        ▓ DRM/KMS      ▓ io_uring       ▓ IPC/SHM       ▓ Capability sec       ║");
    println!("║        ▓ Userspace Drv ▓ Ring 3 DRM    ▓ Microkernel   ▓ Hardware Cap         ║");
    set_fg(Color::White);
    println!("║                                                                              ║");
    set_fg(Color::LightCyan);
    println!("╚══════════════════════════════════════════════════════════════════════════════╝");
    println!();
}

fn init_subsystems() {
    set_fg(Color::LightGreen);
    #[cfg(feature = "net")]
    ziqa_kernel::net::init();
    ziqa_kernel::drivers::ps2_mouse::init();
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
        vfs.mount("/bin/test", Arc::new(Mutex::new(RamFile::from_bytes(include_bytes!("../assets/test_elf.bin")))));
        #[cfg(feature = "wasm")]
        vfs.mount("/bin/hello.wasm", Arc::new(Mutex::new(RamFile::from_bytes(ziqa_kernel::abi::wasm::TEST_WASM))));
        
        // Busybox binary
        vfs.mount("/bin/busybox", Arc::new(Mutex::new(RamFile::from_bytes(include_bytes!("../userspace/busybox-1.36.1/busybox")))));
        // Keyboard driver
        vfs.mount("/bin/keyboard_driver", Arc::new(Mutex::new(RamFile::from_bytes(include_bytes!("../userspace/keyboard_driver.elf")))));
        // Verification script
        vfs.mount("/bin/test.sh", Arc::new(Mutex::new(RamFile::from_bytes(include_bytes!("../userspace/hush_test.sh")))));

        // Doom ELF binary (compiled by build.zig from src/zig/doom_port.zig)
        #[cfg(feature = "games")]
        vfs.mount("/bin/doom", Arc::new(Mutex::new(RamFile::from_bytes(include_bytes!("../zig-out/bin/doom")))));
    }

    // Disk filesystems
    {
        block_registry::print_devices();

        if let Some(entry) = block_registry::first() {
            let disk = entry.device.clone();
            println!(" ~ root disk .......................... /dev/{} ({})", entry.name, entry.driver);

            // 1. Try to mount FAT32 (host-editable)
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
                ziqa_kernel::fs::vfs::register_mount(&alloc::format!("/dev/{}", entry.name), "/disk", "ziqafs");
                println!(" ~ ZiqaFS ............................. mounted at /disk");
            } else if !fat32_mounted {
                // Disk is blank or unrecognized, and no FAT32 data to protect. Safe to format.
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

fn run_startup() {
    ziqa_kernel::boot_screen::show_boot_screen();
    ziqa_kernel::drivers::vga::clear_screen();
    ziqa_kernel::drivers::uart::VGA_ENABLED.store(true, core::sync::atomic::Ordering::Relaxed);
    section("Startup");
    set_fg(Color::LightGreen);

    // Spawn the built-in test ELF as a user process
    let binary = include_bytes!("../assets/test_elf.bin");
    if let Some(pid) = ziqa_kernel::process::scheduler::spawn_elf(binary) {
        println!(" ✓ Spawned user process pid={} ............ from test_elf.bin", pid.0);
    } else {
        println!(" ! Failed to spawn user process");
    }

    // Spawn the userspace keyboard driver
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
        println!(" ✓ Spawned Userspace Keyboard Driver pid={} .... from userspace/keyboard_driver.elf", pid.0);
    } else {
        println!(" ! Failed to spawn Userspace Keyboard Driver");
    }

    // Spawn Doom as a user-space process
    #[cfg(feature = "games")]
    {
        let doom_binary = include_bytes!("../zig-out/bin/doom");
        if let Some(pid) = ziqa_kernel::process::scheduler::spawn_elf(doom_binary) {
            println!(" ✓ Spawned Doom pid={} ..................... from zig-out/bin/doom", pid.0);
        } else {
            println!(" ! Failed to spawn Doom process (games feature enabled but doom.elf may need rebuilding)");
        }
    }

    // Restore any saved snapshots for instant-on resume
    let restored = ziqa_kernel::process::snapshot::restore_all_at_boot(binary);
    if restored > 0 {
        set_fg(Color::LightGreen);
        println!(" ✓ Instant-on resume: {} processes restored from snapshot", restored);
        set_fg(Color::White);
    }

    println!(" ✓ ZiqaKernel v1.0 ready ................ type 'help' for shell");
    set_fg(Color::White);
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("\n[PANIC] {}", info);
    loop {
        x86_64::instructions::hlt();
    }
}
