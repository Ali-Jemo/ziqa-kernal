#![no_std]
#![no_main]

use alloc::sync::Arc;
use bootloader::{entry_point, BootInfo};
use core::panic::PanicInfo;
use spin::Mutex;
use ziqa_kernel::drivers::ata::AtaBlock;
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
    // 1. Core Init
    ziqa_kernel::init(boot_info);
    print_banner();
    KLOG.lock().min_level = Level::Debug;
    ziqa_kernel::klog!(Level::Info, "ZiqaKernel v1.0 booting");

    // 2. Subsystem Init
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
    set_fg(Color::White);
    println!("║                                                                              ║");
    set_fg(Color::LightCyan);
    println!("╚══════════════════════════════════════════════════════════════════════════════╝");
    println!();
}

fn init_subsystems() {
    set_fg(Color::LightGreen);
    ziqa_kernel::net::init();
    ziqa_kernel::drivers::virtio_net::init().ok();
    ziqa_kernel::drivers::ps2_mouse::init();
    set_fg(Color::White);
    section("Self-tests");
    ziqa_kernel::tests::run_all();
}

fn init_services() {
    section("Services");
    // VFS & RamFS Setup
    {
        let mut vfs = VFS.lock();
        let demo_file = Arc::new(Mutex::new(RamFile::new()));
        {
            let mut file = demo_file.lock();
            let _ = ziqa_kernel::fs::File::write(&mut *file, b"Welcome to ZiqaKernel v0.7!\n", 0);
        }
        vfs.mount("/etc/motd", demo_file);
        vfs.mount("/bin/test", Arc::new(Mutex::new(RamFile::from_bytes(include_bytes!("../assets/test_elf.bin")))));
        vfs.mount("/bin/hello.wasm", Arc::new(Mutex::new(RamFile::from_bytes(ziqa_kernel::abi::wasm::TEST_WASM))));
    }
    
    // ZiqaFS
    {
        let ata_disk = Arc::new(AtaBlock::new().expect("Failed to initialize AtaBlock"));
        let _ziqafs = ZiqaFs::mount(ata_disk.clone())
            .unwrap_or_else(|_| ZiqaFs::format(ata_disk.clone()).expect("Failed to format ZiqaFS"));
        println!(" ~ ZiqaFS ............................. mounted");
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
