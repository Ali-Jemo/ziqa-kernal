#![no_std]
#![no_main]

use bootloader::{BootInfo, entry_point};
use core::panic::PanicInfo;
use ziqa_kernel::{println, vga_println};
use ziqa_kernel::klog::{Level, KLOG};
use ziqa_kernel::ebpf::{BpfInstruction, op as bpf_op, verifier::BpfVerifier, vm::BpfVm};
use ziqa_kernel::io::uring::{IoUring, SqEntry, op as io_op};
use ziqa_kernel::fs::vfs::VFS;
use ziqa_kernel::fs::ramfs::RamFile;
use ziqa_kernel::fs::ziqafs::ZiqaFs;
use ziqa_kernel::drivers::virtio_block::VirtioBlock;
use ziqa_kernel::process::{Pid, AbiKind, scheduler::SCHEDULER};
use ziqa_kernel::process::signal::{self, sig};
use ziqa_kernel::memory::VirtAddr;
use ziqa_kernel::capability::{ResourceKind, Permissions};
use alloc::sync::Arc;
use spin::Mutex;

extern crate alloc;

entry_point!(kernel_main);

fn kernel_main(boot_info: &'static BootInfo) -> ! {
    ziqa_kernel::init(boot_info);

    // ── VGA banner ────────────────────────────────────────────────────────────
    ziqa_kernel::drivers::vga::WRITER.lock().set_color(
        ziqa_kernel::drivers::vga::Color::LightCyan,
        ziqa_kernel::drivers::vga::Color::Black,
    );
    vga_println!("╔══════════════════════════════════════════╗");
    vga_println!("║   ZiqaKernel v0.7  —  نواة زيقا          ║");
    vga_println!("╠══════════════════════════════════════════╣");
    vga_println!("║  [OK] Signals  [OK] klog  [OK] Timer     ║");
    vga_println!("║  [OK] Syscalls [OK] ZiqaFS [OK] eBPF     ║");
    vga_println!("╚══════════════════════════════════════════╝");

    ziqa_kernel::drivers::vga::WRITER.lock().set_color(
        ziqa_kernel::drivers::vga::Color::White,
        ziqa_kernel::drivers::vga::Color::Black,
    );

    println!("[ZIQA] ZiqaKernel v0.7: Signals + klog + Timer + Syscall Edition");

    // ── klog: set level and log boot messages ─────────────────────────────────
    KLOG.lock().min_level = Level::Debug;
    ziqa_kernel::klog!(Level::Info, "ZiqaKernel v0.7 booting");
    ziqa_kernel::klog!(Level::Info, "Hardware: GDT, IDT, PIC, Heap initialized");

    // ── VFS & RamFS Setup ─────────────────────────────────────────────────────
    {
        let mut vfs = VFS.lock();
        let demo_file = Arc::new(Mutex::new(RamFile::new()));
        {
            let mut file = demo_file.lock();
            let greeting = b"Welcome to ZiqaKernel v0.7!\n";
            let _ = ziqa_kernel::fs::File::write(&mut *file, greeting, 0);
        }
        vfs.mount("/etc/motd", demo_file);
        ziqa_kernel::klog!(Level::Info, "VFS: mounted RamFS at /etc/motd");
    }

    // ── Native Persistent Filesystem (ZiqaFS) ─────────────────────────────────
    {
        let virtio_disk = Arc::new(VirtioBlock::new(0x10001000, 2048));
        let ziqafs = ZiqaFs::new(virtio_disk).expect("Failed to init ZiqaFS");
        let block_size = ziqafs.superblock.block_size;
        let total_blocks = ziqafs.superblock.total_blocks;
        ziqa_kernel::klog!(Level::Info, "ZiqaFS: block_size={} total_blocks={}",
            block_size, total_blocks);
        println!("[ZiqaFS] block_size={} total_blocks={}",
            block_size, total_blocks);
    }

    // ── MLFQ Scheduler + Process spawn ───────────────────────────────────────
    println!("\n━━━ Process & Signal Demo ━━━");
    let pid = SCHEDULER.lock().spawn(
        AbiKind::LinuxElf,
        VirtAddr::new(0x400000),
        VirtAddr::new(0x7FFFFFFF000)
    );

    if let Some(p) = pid {
        println!("  [OK] Spawned process PID={}", p.0);
        ziqa_kernel::klog!(Level::Info, "Spawned PID={}", p.0);

        // Grant File capability
        ziqa_kernel::process::scheduler::with_process_mut(p, |proc| {
            proc.capabilities.grant(ResourceKind::File, Permissions::full(), 0);
        });

        // ── Signal demo ───────────────────────────────────────────────────────
        demo_signals(p);

        // ── Syscall demo ──────────────────────────────────────────────────────
        demo_syscalls(p);

        // ── eBPF & io_uring ───────────────────────────────────────────────────
        demo_advanced_subsystems(p);

        // ── IPC & Shared Memory demo ──────────────────────────────────────────
        demo_ipc_shm(p);
    }

    // ── Timer / uptime ────────────────────────────────────────────────────────
    println!("\n━━━ Timer / Uptime ━━━");
    let uptime = ziqa_kernel::timer::uptime_ms();
    println!("  Uptime: {} ms ({} ticks)", uptime, ziqa_kernel::timer::uptime_ticks());
    ziqa_kernel::klog!(Level::Info, "Uptime at end of init: {} ms", uptime);

    // ── klog dump ─────────────────────────────────────────────────────────────
    println!("\n━━━ Kernel Log (last {} entries) ━━━", KLOG.lock().count());
    KLOG.lock().dump_level(Level::Info);

    vga_println!("");
    vga_println!("ZiqaKernel v0.7 operational.");

    ziqa_kernel::shell::start();
}

fn demo_signals(pid: Pid) {
    println!("\n  [Signals]");

    // Send SIGUSR1 to the process
    let sent = SCHEDULER.lock().send_signal(pid, sig::SIGUSR1);
    println!("    send SIGUSR1 to PID={} -> {}", pid.0, sent);
    ziqa_kernel::klog!(Level::Debug, "sent SIGUSR1 to PID={}", pid.0);

    // Check pending and dequeue
    ziqa_kernel::process::scheduler::with_process_mut(pid, |proc| {
        println!("    pending mask: {:#010b}", proc.signals.pending);
        let signum = proc.signals.dequeue();
        if signum != 0 {
            println!("    dequeued signal {}", signum);
        }
        println!("    pending after dequeue: {:#010b}", proc.signals.pending);
    });

    // Send SIGTERM and show it would terminate
    SCHEDULER.lock().send_signal(pid, sig::SIGTERM);
    ziqa_kernel::process::scheduler::with_process_mut(pid, |proc| {
        let signum = proc.signals.dequeue();
        if signum != 0 {
            let action = signal::default_action(signum);
            let fatal = action == signal::DefaultDisposition::Terminate || action == signal::DefaultDisposition::CoreDump;
            println!("    SIGTERM ({}) is_fatal_default={}", signum, fatal);
            if fatal { proc.exit(-(signum as i64)); }
        }
    });
    println!("    [OK] Signal subsystem working");
}

fn demo_syscalls(_pid: Pid) {
    println!("\n  [Syscall Dispatcher]");

    // Re-spawn a fresh process for syscall demo (previous one exited via SIGTERM)
    let fresh = SCHEDULER.lock().spawn(
        AbiKind::LinuxElf,
        VirtAddr::new(0x400000),
        VirtAddr::new(0x7FFFFFFF000)
    );

    if let Some(p) = fresh {
        let registry = ziqa_kernel::init_abi_registry();

        ziqa_kernel::process::scheduler::with_process_mut(p, |proc| {
            use ziqa_kernel::abi::syscall::{SyscallContext, nr};

            // getpid
            let mut ctx = SyscallContext::new(nr::GETPID, [0; 6], proc);
            let ret = ziqa_kernel::abi::syscall::dispatch_syscall(&registry, &mut ctx);
            println!("    getpid() = {:?}", ret);

            // write(1, _, 13)
            let mut ctx2 = SyscallContext::new(nr::WRITE, [1, 0, 13, 0, 0, 0], proc);
            let ret2 = ziqa_kernel::abi::syscall::dispatch_syscall(&registry, &mut ctx2);
            println!("    write(1, _, 13) = {:?}", ret2);

            // exit(0)
            let mut ctx3 = SyscallContext::new(nr::EXIT, [0; 6], proc);
            let _ = ziqa_kernel::abi::syscall::dispatch_syscall(&registry, &mut ctx3);
            println!("    exit(0) -> state={:?}", proc.state);
        });
    }
    println!("    [OK] Syscall dispatcher working");
}

fn demo_ipc_shm(pid: Pid) {
    println!("\n  [IPC & Shared Memory]");

    use ziqa_kernel::ipc;
    use ziqa_kernel::ipc::shm;

    // 1. Channel IPC
    if let Some(chan_id) = ipc::create_channel() {
        println!("    Created IPC channel ID={}", chan_id);
        
        let msg_data = b"Hello from Ring 0!";
        if ipc::send(chan_id, pid, msg_data).is_ok() {
            println!("    Sent message to channel {}", chan_id);
        }

        if let Ok(msg) = ipc::recv(chan_id) {
            if let Ok(s) = core::str::from_utf8(&msg.data[..msg.len]) {
                println!("    Received: \"{}\" from PID={}", s, msg.sender.0);
            }
        }
    }

    // 2. Shared Memory
    let shm_id = shm::SHM.lock().create(pid, 4096);
    println!("    Created SHM segment ID={}", shm_id);
    
    if let Ok(vaddr) = shm::SHM.lock().attach(shm_id, pid) {
        println!("    Attached SHM at virtual address 0x{:x}", vaddr);
    }
}

fn demo_advanced_subsystems(pid: Pid) {
    println!("\n━━━ eBPF & io_uring Integration ━━━");

    let program = [
        BpfInstruction { code: bpf_op::MOV,     dst_reg: 0, src_reg: 0, off: 0, imm: 100 },
        BpfInstruction { code: bpf_op::ALU_ADD,  dst_reg: 0, src_reg: 0, off: 0, imm: 50  },
        BpfInstruction { code: bpf_op::RET,      dst_reg: 0, src_reg: 0, off: 0, imm: 0   },
    ];

    let verifier = BpfVerifier::new(&program);
    if verifier.verify().is_ok() {
        let mut vm = BpfVm::new();
        let res = vm.execute(&program).unwrap();
        println!("  [eBPF] result={}", res);
        ziqa_kernel::klog!(Level::Debug, "eBPF program result={}", res);
    }

    let mut read_buf = [0u8; 128];
    let mut uring = IoUring::new(pid, 16);
    let sqe = SqEntry {
        opcode: io_op::READ,
        flags: 0,
        fd: 3,
        addr: read_buf.as_mut_ptr() as u64,
        len: 128,
        user_data: 0xDEADBEEF,
    };
    uring.submit(sqe).unwrap();
    let processed = uring.process_requests();
    println!("  [io_uring] processed {} requests", processed);
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("\n[PANIC] {}", info);
    vga_println!("\n[PANIC] {}", info);
    loop { x86_64::instructions::hlt(); }
}
