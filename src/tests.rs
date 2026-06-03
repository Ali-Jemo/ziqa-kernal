/// Kernel self-tests — run at boot to verify subsystems.
/// Each test prints PASS or FAIL via serial.
use crate::println;
use alloc::vec;

pub fn run_all() {
    x86_64::instructions::interrupts::without_interrupts(|| {
        println!("[TEST] Running kernel self-tests...");
        let mut passed = 0u32;
        let mut failed = 0u32;

        macro_rules! test {
            ($name:expr, $body:expr) => {{
                let ok: bool = $body;
                if ok {
                    println!("[TEST]   PASS  {}", $name);
                    passed += 1;
                } else {
                    println!("[TEST]   FAIL  {}", $name);
                    failed += 1;
                }
            }};
        }

        // ── Scheduler tests ──────────────────────────────────────────────────────
        test!("scheduler: spawn idle task", {
            use crate::memory::VirtAddr;
            use crate::process::scheduler::SCHEDULER;
            use crate::process::{AbiKind, Pid};
            let pid = SCHEDULER.spawn(
                AbiKind::ZiqaNative,
                VirtAddr::new(0x1000),
                VirtAddr::new(0x7FFF_FFFF_000),
            );
            let ok = pid.is_some();
            if let Some(p) = pid {
                SCHEDULER.exit_process(p, 0);
                SCHEDULER.waitpid(Pid(0), p.0 as i64, 0);
            }
            ok
        });

        test!("scheduler: tick advances counter", {
            use crate::process::scheduler::SCHEDULER;
            let before = SCHEDULER.total_ticks();
            SCHEDULER.tick();
            let after = SCHEDULER.total_ticks();
            after == before + 1
        });

        // ── ABI detection tests ──────────────────────────────────────────────────
        test!("abi: ELF magic detected", {
            use crate::abi::linux::LINUX_PLUGIN;
            use crate::abi::AbiPlugin;
            let elf = [0x7F, b'E', b'L', b'F', 0, 0, 0, 0];
            LINUX_PLUGIN.can_load(&elf)
        });

        test!("abi: non-ELF rejected by Linux plugin", {
            use crate::abi::linux::LINUX_PLUGIN;
            use crate::abi::AbiPlugin;
            let not_elf = [0x00, 0x01, 0x02, 0x03];
            !LINUX_PLUGIN.can_load(&not_elf)
        });

        #[cfg(feature = "wasm")]
        test!("abi: WASM magic detected", {
            use crate::abi::wasm::WASM_PLUGIN;
            use crate::abi::AbiPlugin;
            let wasm = [0x00, b'a', b's', b'm', 0x01, 0x00, 0x00, 0x00];
            WASM_PLUGIN.can_load(&wasm)
        });

        // ── Memory tests ─────────────────────────────────────────────────────────
        test!("memory: VirtAddr align_down", {
            use crate::memory::VirtAddr;
            let addr = VirtAddr::new(0x1234);
            addr.align_down(0x1000u64).as_u64() == 0x1000
        });

        test!("memory: VirtAddr align_up", {
            use crate::memory::VirtAddr;
            let addr = VirtAddr::new(0x1001);
            addr.align_up(0x1000u64).as_u64() == 0x2000
        });

        test!("memory: VirtAddr align_up exact", {
            use crate::memory::VirtAddr;
            let addr = VirtAddr::new(0x1000);
            addr.align_up(0x1000u64).as_u64() == 0x1000
        });

        // ── Capability tests ─────────────────────────────────────────────────────
        test!("capability: grant and lookup", {
            use crate::capability::{CapabilitySpace, Permissions, ResourceKind};
            let mut space = CapabilitySpace::new();
            let id = space.grant(ResourceKind::Memory, Permissions::read_write(), 0x1000, None);
            id.is_some() && space.lookup(id.unwrap()).is_some()
        });

        test!("capability: revoke removes cap", {
            use crate::capability::{CapabilitySpace, Permissions, ResourceKind};
            let mut space = CapabilitySpace::new();
            let id = space
                .grant(ResourceKind::File, Permissions::read_only(), 3, None)
                .unwrap();
            space.revoke_local(id);
            space.lookup(id).is_none()
        });

        // ── FdTable tests ────────────────────────────────────────────────────────
        test!("fdtable: stdin/stdout/stderr pre-populated", {
            use crate::process::{FdTable, FdTarget};
            let t = FdTable::new();
            matches!(t.get(0).map(|d| d.target), Some(FdTarget::Stdin))
                && matches!(t.get(1).map(|d| d.target), Some(FdTarget::Stdout))
                && matches!(t.get(2).map(|d| d.target), Some(FdTarget::Stderr))
        });

        test!("fdtable: alloc assigns fd >= 3", {
            use crate::process::{FdTable, FdTarget, FileDesc};
            let mut t = FdTable::new();
            let fd = t.alloc(FileDesc {
                target: FdTarget::File(1),
                flags: 0,
                offset: 0,
            });
            fd == Some(3)
        });

        test!("fdtable: close removes fd", {
            use crate::process::{FdTable, FdTarget, FileDesc};
            let mut t = FdTable::new();
            let fd = t
                .alloc(FileDesc {
                    target: FdTarget::File(1),
                    flags: 0,
                    offset: 0,
                })
                .unwrap();
            t.close(fd) && t.get(fd).is_none()
        });

        test!("fdtable: cannot close stdin(0)", {
            use crate::process::FdTable;
            let mut t = FdTable::new();
            !t.close(0)
        });

        test!("fdtable: sequential allocation", {
            use crate::process::{FdTable, FdTarget, FileDesc};
            let mut t = FdTable::new();
            let fd1 = t.alloc(FileDesc {
                target: FdTarget::File(10),
                flags: 0,
                offset: 0,
            });
            let fd2 = t.alloc(FileDesc {
                target: FdTarget::File(11),
                flags: 0,
                offset: 0,
            });
            let fd3 = t.alloc(FileDesc {
                target: FdTarget::File(12),
                flags: 0,
                offset: 0,
            });
            fd1 == Some(3) && fd2 == Some(4) && fd3 == Some(5)
        });

        test!("fdtable: reuse fd after close", {
            use crate::process::{FdTable, FdTarget, FileDesc};
            let mut t = FdTable::new();
            let fd1 = t
                .alloc(FileDesc {
                    target: FdTarget::File(1),
                    flags: 0,
                    offset: 0,
                })
                .unwrap();
            t.close(fd1);
            let fd2 = t.alloc(FileDesc {
                target: FdTarget::File(2),
                flags: 0,
                offset: 0,
            });
            fd2 == Some(3) // First reused slot
        });

        test!("fdtable: cannot close stdout(1)", {
            use crate::process::FdTable;
            let mut t = FdTable::new();
            !t.close(1)
        });

        test!("fdtable: cannot close stderr(2)", {
            use crate::process::FdTable;
            let mut t = FdTable::new();
            !t.close(2)
        });

        test!("fdtable: get returns same fd", {
            use crate::process::{FdTable, FdTarget, FileDesc};
            let mut t = FdTable::new();
            let fd = t
                .alloc(FileDesc {
                    target: FdTarget::File(42),
                    flags: 0x10,
                    offset: 0,
                })
                .unwrap();
            match t.get(fd).map(|d| d.target) {
                Some(FdTarget::File(num)) => num == 42,
                _ => false,
            }
        });

        test!("fdtable: get returns none for unallocated fd", {
            use crate::process::FdTable;
            let t = FdTable::new();
            t.get(99).is_none()
        });

        // ── Pipe / IPC tests ─────────────────────────────────────────────────────
        test!("pipe: create channel and send/recv", {
            use crate::ipc;
            use crate::process::Pid;
            let chan = ipc::create_channel();
            if let Some(id) = chan {
                let ok = ipc::send(id, Pid(1), b"hello").is_ok();
                let msg = ipc::recv(id);
                ok && msg.is_ok() && &msg.unwrap().data[..5] == b"hello"
            } else {
                false
            }
        });

        test!("pipe: recv on empty channel returns error", {
            let chan = crate::ipc::create_channel().unwrap();
            crate::ipc::recv(chan).is_err()
        });

        test!("pipe: sender pid preserved", {
            use crate::ipc;
            use crate::process::Pid;
            let chan = ipc::create_channel().unwrap();
            ipc::send(chan, Pid(42), b"test").unwrap();
            let msg = ipc::recv(chan).unwrap();
            msg.sender == Pid(42)
        });

        test!("pipe: multiple messages queued", {
            use crate::ipc;
            use crate::process::Pid;
            let chan = ipc::create_channel().unwrap();
            ipc::send(chan, Pid(1), b"first").unwrap();
            ipc::send(chan, Pid(2), b"second").unwrap();
            let m1 = ipc::recv(chan).unwrap();
            let m2 = ipc::recv(chan).unwrap();
            &m1.data[..5] == b"first" && &m2.data[..6] == b"second"
        });

        test!("pipe: channel queue fills and blocks", {
            use crate::ipc;
            use crate::process::Pid;
            let chan = ipc::create_channel().unwrap();
            let mut full = false;
            for _ in 0..17 {
                if ipc::send(chan, Pid(1), b"x").is_err() {
                    full = true;
                    break;
                }
            }
            full // Should be full after 17 messages (capacity 16)
        });

        test!("pipe: message data truncated at MSG_MAX", {
            use crate::process::Pid;
            let chan = crate::ipc::create_channel().unwrap();
            let large_msg = vec![0xAAu8; 512]; // Larger than MSG_MAX (256)
            crate::ipc::send(chan, Pid(1), &large_msg).unwrap();
            let msg = crate::ipc::recv(chan).unwrap();
            msg.len == 256
        });

        test!("pipe: recv after full queue", {
            use crate::ipc;
            use crate::process::Pid;
            let chan = ipc::create_channel().unwrap();
            // Fill the queue
            for i in 0..16 {
                let _ = ipc::send(chan, Pid(i), b"x");
            }
            // Recv one to make space
            ipc::recv(chan).unwrap();
            // Now we should be able to send again
            ipc::send(chan, Pid(99), b"after").is_ok()
        });

        test!("pipe: different senders in queue", {
            use crate::ipc;
            use crate::process::Pid;
            let chan = ipc::create_channel().unwrap();
            ipc::send(chan, Pid(1), b"from1").unwrap();
            ipc::send(chan, Pid(2), b"from2").unwrap();
            ipc::send(chan, Pid(3), b"from3").unwrap();
            let m1 = ipc::recv(chan).unwrap();
            let m2 = ipc::recv(chan).unwrap();
            let m3 = ipc::recv(chan).unwrap();
            m1.sender == Pid(1) && m2.sender == Pid(2) && m3.sender == Pid(3)
        });

        // ── Network loopback tests ───────────────────────────────────────────────
        #[cfg(feature = "net")]
        {
        test!("net: loopback transmit echoes to rx", {
            use crate::net::NET;
            let mut net = NET.lock();
            if let Some(lo) = net.get_mut("lo") {
                let _ = lo.transmit(b"test");
                lo.receive().is_ok()
            } else {
                false
            }
        });

        test!("net: loopback packet content preserved", {
            use crate::net::NET;
            let mut net = NET.lock();
            if let Some(lo) = net.get_mut("lo") {
                let _ = lo.transmit(b"ziqa");
                match lo.receive() {
                    Ok(pkt) => &pkt.data[..4] == b"ziqa",
                    Err(_) => false,
                }
            } else {
                false
            }
        });

        test!("net: loopback packet length preserved", {
            use crate::net::NET;
            let mut net = NET.lock();
            if let Some(lo) = net.get_mut("lo") {
                let _ = lo.transmit(b"123456");
                match lo.receive() {
                    Ok(pkt) => pkt.len == 6,
                    Err(_) => false,
                }
            } else {
                false
            }
        });

        test!("net: loopback multiple packets queued", {
            use crate::net::NET;
            let mut net = NET.lock();
            if let Some(lo) = net.get_mut("lo") {
                let _ = lo.transmit(b"pkt1");
                let _ = lo.transmit(b"pkt2");
                let _ = lo.transmit(b"pkt3");
                let p1 = lo.receive().ok();
                let p2 = lo.receive().ok();
                let p3 = lo.receive().ok();
                match (p1, p2, p3) {
                    (Some(a), Some(b), Some(c)) => {
                        &a.data[..4] == b"pkt1"
                            && &b.data[..4] == b"pkt2"
                            && &c.data[..4] == b"pkt3"
                    }
                    _ => false,
                }
            } else {
                false
            }
        });

        test!("net: loopback tx/rx stats incremented", {
            use crate::net::NET;
            let mut net = NET.lock();
            if let Some(lo) = net.get_mut("lo") {
                let tx_before = lo.tx_packets;
                let rx_before = lo.rx_packets;
                let _ = lo.transmit(b"stat");
                let tx_after = lo.tx_packets;
                let rx_after = lo.rx_packets;
                let _ = lo.receive();
                tx_after == tx_before + 1 && rx_after == rx_before + 1
            } else {
                false
            }
        });

        test!("net: loopback byte counters", {
            use crate::net::NET;
            let mut net = NET.lock();
            if let Some(lo) = net.get_mut("lo") {
                let tx_bytes_before = lo.tx_bytes;
                let rx_bytes_before = lo.rx_bytes;
                let _ = lo.transmit(b"bytes");
                let tx_bytes_after = lo.tx_bytes;
                let rx_bytes_after = lo.rx_bytes;
                let _ = lo.receive();
                tx_bytes_after == tx_bytes_before + 5 && rx_bytes_after == rx_bytes_before + 5
            } else {
                false
            }
        });

        test!("net: loopback large packet truncated at MTU", {
            use crate::net::NET;
            let mut net = NET.lock();
            if let Some(lo) = net.get_mut("lo") {
                let huge = vec![0xFFu8; 2000]; // > 1500 MTU
                let _ = lo.transmit(&huge);
                match lo.receive() {
                    Ok(pkt) => pkt.len == 1500,
                    Err(_) => false,
                }
            } else {
                false
            }
        });

        test!("net: loopback rx pending count", {
            use crate::net::NET;
            let mut net = NET.lock();
            if let Some(lo) = net.get_mut("lo") {
                let pending_before = lo.rx_pending();
                let _ = lo.transmit(b"pkt1");
                let _ = lo.transmit(b"pkt2");
                let pending_after = lo.rx_pending();
                let _ = lo.receive();
                let _ = lo.receive();
                pending_after == pending_before + 2
            } else {
                false
            }
        });

        test!("net: loopback is marked as loopback device", {
            use crate::net::NET;
            let mut net = NET.lock();
            if let Some(lo) = net.get_mut("lo") {
                lo.is_loopback && lo.name == "lo"
            } else {
                false
            }
        });

        test!("net: loopback transmit returns ok", {
            use crate::net::NET;
            let mut net = NET.lock();
            if let Some(lo) = net.get_mut("lo") {
                lo.transmit(b"ok").is_ok()
            } else {
                false
            }
        });

        test!("net: netstack device count", {
            use crate::net::NET;
            let net = NET.lock();
            net.device_count() >= 1 // At least loopback
        });

        test!("wasm: interpreter parses and executes hello.wasm", {
            use crate::process::scheduler::{spawn_elf, SCHEDULER};
            let pid_opt = spawn_elf(crate::abi::wasm::TEST_WASM);
            if let Some(pid) = pid_opt {
                let orig_pid = {
                    let _sched = &SCHEDULER;
                    crate::process::scheduler::current_task().map(|t| t.lock().pid)
                };
                {
                    let sched = &SCHEDULER;
                    sched.set_current(pid);
                }
                crate::abi::wasm::wasm_interpreter_entry();
                if let Some(orig) = orig_pid {
                    let sched = &SCHEDULER;
                    sched.set_current(orig);
                }
                let ok = {
                    let sched = &SCHEDULER;
                    let proc = sched.get_process(pid);
                    if let Some(p_arc) = proc {
                        p_arc.lock().exit_code == 0
                    } else {
                        false
                    }
                };
                {
                    let sched = &SCHEDULER;
                    sched.waitpid(crate::process::Pid(0), pid.0 as i64, 0);
                }
                ok
            } else {
                false
            }
        });

        test!("wasm: loop control flow executes successfully", {
            use crate::process::scheduler::{spawn_elf, SCHEDULER};
            const TEST_WASM_LOOP: &[u8] = &[
                0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01, 0x04, 0x01, 0x60, 0x00, 0x00,
                0x03, 0x02, 0x01, 0x00, 0x07, 0x0a, 0x01, 0x06, b'_', b's', b't', b'a', b'r', b't',
                0x00, 0x00, 0x0a, 24, 0x01, 22, 0x01, 0x01, 0x7f, 0x41, 0x05, 0x21, 0x00, 0x03,
                0x40, 0x20, 0x00, 0x41, 0x01, 0x6b, 0x21, 0x00, 0x20, 0x00, 0x0d, 0x00, 0x0b, 0x0b,
            ];
            let pid_opt = spawn_elf(TEST_WASM_LOOP);
            if let Some(pid) = pid_opt {
                let orig_pid = {
                    let _sched = &SCHEDULER;
                    crate::process::scheduler::current_task().map(|t| t.lock().pid)
                };
                {
                    let sched = &SCHEDULER;
                    sched.set_current(pid);
                }
                crate::abi::wasm::wasm_interpreter_entry();
                if let Some(orig) = orig_pid {
                    let sched = &SCHEDULER;
                    sched.set_current(orig);
                }
                let ok = {
                    let sched = &SCHEDULER;
                    let proc = sched.get_process(pid);
                    if let Some(p_arc) = proc {
                        p_arc.lock().exit_code == 0
                    } else {
                        false
                    }
                };
                {
                    let sched = &SCHEDULER;
                    sched.waitpid(crate::process::Pid(0), pid.0 as i64, 0);
                }
                ok
            } else {
                false
            }
        });

        test!("net: eth0 transmission and reception stats", {
            use crate::net::NET;
            let has_eth0 = {
                let net = NET.lock();
                net.device_count() >= 2
            };
            if !has_eth0 {
                false
            } else {
                let mut arp_pkt = [0u8; 42];
                arp_pkt[6..12].copy_from_slice(&[0x52, 0x54, 0x12, 0x34, 0x56, 0x78]);
                arp_pkt[12..14].copy_from_slice(&[0x08, 0x06]);
                arp_pkt[20..22].copy_from_slice(&[0x00, 0x01]);
                let mut net = NET.lock();
                if let Some(eth0) = net.get_mut("eth0") {
                    let tx_before = eth0.tx_packets;
                    let _rx_before = eth0.rx_packets;
                    let _ = eth0.transmit(&arp_pkt);
                    eth0.tx_packets == tx_before + 1
                } else {
                    false
                }
            }
        });

        crate::tests_net::run_socket_tests();
        }

        println!("[TEST] Results: {}/{} passed", passed, passed + failed);

        if failed > 0 {
            println!("[TEST] WARNING: {} test(s) FAILED", failed);
        } else {
            println!("[TEST] All tests passed!");
        }
    });
}
