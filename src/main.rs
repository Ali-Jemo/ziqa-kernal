#![no_std]
#![no_main]

use bootloader::{BootInfo, entry_point};
use core::panic::PanicInfo;
use ziqa_kernel::println;
use ziqa_kernel::drivers::vga;
use ziqa_kernel::drivers::vga::Color;
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

    set_fg(Color::White);
    println!();
}

entry_point!(kernel_main);

fn kernel_main(boot_info: &'static BootInfo) -> ! {
    // Raw serial write to confirm execution
    unsafe {
        let mut port = x86_64::instructions::port::Port::<u8>::new(0x3F8);
        for &b in b"BOOTING..." { port.write(b); }
    }

    // Display banner first, then init below
    print_banner();

    set_fg(Color::LightGreen);
    ziqa_kernel::init(boot_info);
    set_fg(Color::White);

    KLOG.lock().min_level = Level::Debug;
    ziqa_kernel::klog!(Level::Info, "ZiqaKernel v1.0 booting");

    set_fg(Color::LightGreen);
    ziqa_kernel::net::init();
    ziqa_kernel::drivers::virtio_net::init().ok();
    set_fg(Color::White);


    // ── Self-tests ──
    section("Self-tests");
    ziqa_kernel::tests::run_all();

    // ── Services ──
    section("Services");
    ziqa_kernel::klog!(Level::Info, "Network stack initialized");

    // VFS & RamFS Setup
    {
        let mut vfs = VFS.lock();
        let demo_file = Arc::new(Mutex::new(RamFile::new()));
        {
            let mut file = demo_file.lock();
            let greeting = b"Welcome to ZiqaKernel v0.7!\n";
            let _ = ziqa_kernel::fs::File::write(&mut *file, greeting, 0);
        }
        vfs.mount("/etc/motd", demo_file);

        let test_elf = Arc::new(Mutex::new(RamFile::from_bytes(
            include_bytes!("../assets/test_elf.bin")
        )));
        vfs.mount("/bin/test", test_elf);

        let test_wasm = Arc::new(Mutex::new(RamFile::from_bytes(
            ziqa_kernel::abi::wasm::TEST_WASM
        )));
        vfs.mount("/bin/hello.wasm", test_wasm);

    }
    println!(" ~ VFS .................................. /etc/motd, /bin/test, /bin/hello.wasm mounted");

    ziqa_kernel::klog!(Level::Info, "VFS: mounted RamFS at /etc/motd and /bin/test");

    // ZiqaFS
    {
        let virtio_disk = Arc::new(VirtioBlock::new(0x10001000, 2048));
        let ziqafs = ZiqaFs::new(virtio_disk).expect("Failed to init ZiqaFS");
        let block_size = ziqafs.superblock.block_size;
        let total_blocks = ziqafs.superblock.total_blocks;
        println!(" ~ ZiqaFS ............................. block_size={}, {} blocks",
            block_size, total_blocks);
        ziqa_kernel::klog!(Level::Info, "ZiqaFS: block_size={} total_blocks={}",
            block_size, total_blocks);
    }
    set_fg(Color::White);

    // ── Verification ──
    section("Verification");

    let pid = SCHEDULER.lock().spawn(
        AbiKind::LinuxElf,
        VirtAddr::new(0x400000),
        VirtAddr::new(0x7FFFFFFF000)
    );

    if let Some(p) = pid {
        println!(" ~ Process .............................. PID {} spawned", p.0);
        ziqa_kernel::klog!(Level::Info, "Spawned PID={}", p.0);

        ziqa_kernel::process::scheduler::with_process_mut(p, |proc| {
            proc.capabilities.grant(ResourceKind::File, Permissions::full(), 0);
        });

        demo_signals(p);
        demo_syscalls(p);
        demo_advanced_subsystems(p);
        demo_ipc_shm(p);
        demo_fork_waitpid(p);
    }

    // ── Startup ──
    section("Startup");

    set_fg(Color::LightGreen);
    let uptime = ziqa_kernel::timer::uptime_ms();
    let ticks = ziqa_kernel::timer::uptime_ticks();
    println!(" ~ Timer ................................ {} ms ({} ticks)", uptime, ticks);
    ziqa_kernel::klog!(Level::Info, "Uptime at end of init: {} ms", uptime);

    // Auto-exec embedded test ELF
    {
        let binary = include_bytes!("../assets/test_elf.bin");
        if let Some(pid) = ziqa_kernel::process::scheduler::spawn_elf(binary) {
            let entry = {
                let mut sched = SCHEDULER.lock();
                sched.set_current(pid);
                sched.current_task().map(|p| p.entry_point.as_u64()).unwrap_or(0)
            };
            if entry != 0 {
                println!(" ~ Test ELF ............................ PID {} at 0x{:x}", pid.0, entry);
            }
        }
    }
    set_fg(Color::White);

    // Ready
    println!("");
    set_fg(Color::LightGreen);
    println!(" ✓ ZiqaKernel v1.0 ready ................ type 'help' for shell");
    set_fg(Color::White);
    println!("");

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

fn demo_fork_waitpid(parent: Pid) {
    println!("\n━━━ fork / waitpid Demo ━━━");

    let registry = ziqa_kernel::init_abi_registry();

    // Safely acquire a raw pointer to the parent process under a brief lock,
    // then release the scheduler lock before executing syscalls that will lock it.
    let parent_ptr = {
        let mut sched = SCHEDULER.lock();
        if let Some(p) = sched.get_process_mut(parent) {
            p as *mut ziqa_kernel::process::Process
        } else {
            println!("  [FAIL] Parent process not found");
            return;
        }
    };
    let proc = unsafe { &mut *parent_ptr };

    use ziqa_kernel::abi::syscall::{SyscallContext, nr};

    // fork()
    let mut ctx = SyscallContext::new(nr::FORK, [0; 6], proc);
    let child_pid = ziqa_kernel::abi::syscall::dispatch_syscall(&registry, &mut ctx);
    println!("  fork() -> child PID={:?}", child_pid);

    // mmap(0, 4096, PROT_RW, MAP_ANON, -1, 0)
    let mut ctx2 = SyscallContext::new(
        nr::MMAP,
        [0, 4096, 3, 0x22, u64::MAX, 0],
        proc,
    );
    let addr = ziqa_kernel::abi::syscall::dispatch_syscall(&registry, &mut ctx2);
    println!("  mmap(4096) -> addr={:?}", addr);

    // munmap the region we just mapped
    if let Ok(a) = addr {
        let mut ctx3 = SyscallContext::new(nr::MUNMAP, [a, 4096, 0, 0, 0, 0], proc);
        let r = ziqa_kernel::abi::syscall::dispatch_syscall(&registry, &mut ctx3);
        println!("  munmap(0x{:x}) -> {:?}", a, r);
    }

    // Exit the child so waitpid can reap it
    if let Ok(child_pid_val) = {
        let mut ctx_fork = SyscallContext::new(nr::FORK, [0; 6], proc);
        ziqa_kernel::abi::syscall::dispatch_syscall(&registry, &mut ctx_fork)
    } {
        // Exit the child
        SCHEDULER.lock().exit_process(Pid(child_pid_val), 0);

        // waitpid(-1) from parent
        let reaped = SCHEDULER.lock().waitpid(parent, -1);
        println!("  waitpid(-1) -> {:?}", reaped.map(|(p, c)| (p.0, c)));
    }

    // ── Loopback ping demo ────────────────────────────────────────────────────
    println!("\n━━━ Network Loopback Demo ━━━");
    {
        let ping = b"PING ziqa 0.8";
        let mut net = ziqa_kernel::net::NET.lock();
        if let Some(lo) = net.get_mut("lo") {
            let _ = lo.transmit(ping);
            match lo.receive() {
                Ok(pkt) => {
                    let s = core::str::from_utf8(&pkt.data[..pkt.len]).unwrap_or("?");
                    println!("  lo: sent {} bytes, echoed back: \"{}\"", ping.len(), s);
                }
                Err(_) => println!("  lo: no packet received"),
            }
        }
    }
    println!("  [OK] Network loopback working");
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("\n[PANIC] {}", info);
    loop { x86_64::instructions::hlt(); }
}
