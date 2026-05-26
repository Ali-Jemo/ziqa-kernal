/// Kernel self-tests — run at boot to verify subsystems.
/// Each test prints PASS or FAIL via serial.

use crate::println;

pub fn run_all() {
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
        use crate::process::{AbiKind};
        use crate::memory::VirtAddr;
        use crate::process::scheduler::SCHEDULER;
        let pid = SCHEDULER.lock().spawn(
            AbiKind::ZiqaNative,
            VirtAddr::new(0),
            VirtAddr::new(0),
        );
        pid.is_some()
    });

    test!("scheduler: tick advances counter", {
        use crate::process::scheduler::SCHEDULER;
        let before = SCHEDULER.lock().total_ticks();
        SCHEDULER.lock().tick();
        let after = SCHEDULER.lock().total_ticks();
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
        addr.align_down(0x1000).as_u64() == 0x1000
    });

    test!("memory: VirtAddr align_up", {
        use crate::memory::VirtAddr;
        let addr = VirtAddr::new(0x1001);
        addr.align_up(0x1000).as_u64() == 0x2000
    });

    test!("memory: VirtAddr align_up exact", {
        use crate::memory::VirtAddr;
        let addr = VirtAddr::new(0x1000);
        addr.align_up(0x1000).as_u64() == 0x1000
    });

    // ── Capability tests ─────────────────────────────────────────────────────
    test!("capability: grant and lookup", {
        use crate::capability::{CapabilitySpace, ResourceKind, Permissions};
        let mut space = CapabilitySpace::new();
        let id = space.grant(ResourceKind::Memory, Permissions::read_write(), 0x1000);
        id.is_some() && space.lookup(id.unwrap()).is_some()
    });

    test!("capability: revoke removes cap", {
        use crate::capability::{CapabilitySpace, ResourceKind, Permissions};
        let mut space = CapabilitySpace::new();
        let id = space.grant(ResourceKind::File, Permissions::read_only(), 3).unwrap();
        space.revoke(id);
        space.lookup(id).is_none()
    });

    println!("[TEST] Results: {}/{} passed", passed, passed + failed);
    if failed > 0 {
        println!("[TEST] WARNING: {} test(s) FAILED", failed);
    } else {
        println!("[TEST] All tests passed!");
    }
}
