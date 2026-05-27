#![no_std]
#![no_main]

use bootloader::{BootInfo, entry_point};
use core::panic::PanicInfo;
use ziqa_kernel::println;
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

    ziqa_kernel::drivers::vga::WRITER.lock().set_color(
        ziqa_kernel::drivers::vga::Color::White,
        ziqa_kernel::drivers::vga::Color::Black,
    );

    println!("[ZIQA] ZiqaKernel v0.8: fork + waitpid + mmap + Network Stack Edition");

    // ── klog: set level and log boot messages ─────────────────────────────────
    KLOG.lock().min_level = Level::Debug;
    ziqa_kernel::klog!(Level::Info, "ZiqaKernel v0.8 booting");
    ziqa_kernel::klog!(Level::Info, "Hardware: GDT, IDT, PIC, Heap initialized");

    // ── Network Stack ─────────────────────────────────────────────────────────
    ziqa_kernel::net::init();
    ziqa_kernel::klog!(Level::Info, "Network stack initialized");

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

        // ── fork / waitpid demo ───────────────────────────────────────────────
        demo_fork_waitpid(p);
    }

    // ── Timer / uptime ────────────────────────────────────────────────────────
    println!("\n━━━ Timer / Uptime ━━━");
    let uptime = ziqa_kernel::timer::uptime_ms();
    println!("  Uptime: {} ms ({} ticks)", uptime, ziqa_kernel::timer::uptime_ticks());
    ziqa_kernel::klog!(Level::Info, "Uptime at end of init: {} ms", uptime);

    // ── klog dump ─────────────────────────────────────────────────────────────
    println!("\n━━━ Kernel Log (last {} entries) ━━━", KLOG.lock().count());
    KLOG.lock().dump_level(Level::Info);


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

    ziqa_kernel::process::scheduler::with_process_mut(parent, |proc| {
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
    });

    // Exit the child so waitpid can reap it
    if let Ok(child_pid_val) = {
        let mut tmp = 0u64;
        ziqa_kernel::process::scheduler::with_process_mut(parent, |proc| {
            use ziqa_kernel::abi::syscall::{SyscallContext, nr};
            let registry2 = ziqa_kernel::init_abi_registry();
            let mut ctx = SyscallContext::new(nr::FORK, [0; 6], proc);
            let r = ziqa_kernel::abi::syscall::dispatch_syscall(&registry2, &mut ctx);
            if let Ok(v) = r { tmp = v; }
        });
        if tmp > 0 { Ok::<u64, ()>(tmp) } else { Err(()) }
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
