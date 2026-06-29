/// Supplementary kernel tests for fixed and new subsystems.
use crate::println;
use crate::process::{Process, Pid, AbiKind};
use x86_64::VirtAddr;

pub fn run_all() {
    println!("[TEST] Running fixed kernel tests...");
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

    // ── Capability tests ─────────────────────────────────────────────────────
    test!("capability: grant and lookup", {
        use crate::capability::{CapabilitySpace, ResourceKind, Permissions};
        let mut space = CapabilitySpace::new();
        let id = space.grant(ResourceKind::Memory, Permissions::read_write(), 0x1000, None);
        id.is_some() && space.lookup(id.unwrap()).is_some()
    });

    test!("capability: revoke removes cap", {
        use crate::capability::{CapabilitySpace, ResourceKind, Permissions};
        let mut space = CapabilitySpace::new();
        let id = space.grant(ResourceKind::File, Permissions::read_only(), 3, None).unwrap();
        space.revoke_local(id);
        space.lookup(id).is_none()
    });

    // ── eBPF Refinement tests ────────────────────────────────────────────────
    test!("ebpf: basic register mapping", {
        use crate::ebpf::vm::BpfVm;
        use crate::ebpf::{BpfInstruction, op};
        let prog = [
            BpfInstruction { code: op::MOV, regs: 0x00, off: 0, imm: 42 },
            BpfInstruction { code: op::RET, regs: 0x00, off: 0, imm: 0 },
        ];
        let mut vm = BpfVm::new();
        vm.execute(&prog) == Ok(42)
    });

    test!("ebpf: syscall context mapping", {
        use crate::ebpf::vm::BpfVm;
        use crate::ebpf::{BpfInstruction, op};
        use crate::abi::syscall::SyscallContext;
        let prog = [
            BpfInstruction { code: op::MOV_X, regs: 0x10, off: 0, imm: 0 }, // dst=0, src=1
            BpfInstruction { code: op::ALU_ADD_X, regs: 0x20, off: 0, imm: 0 }, // dst=0, src=2
            BpfInstruction { code: op::RET, regs: 0x00, off: 0, imm: 0 },
        ];
        let mut proc = Process::new(Pid(999), AbiKind::LinuxElf, VirtAddr::new(0), VirtAddr::new(0));
        let ctx = SyscallContext::new(100, [10, 20, 30, 40, 50, 60], &mut proc);
        let mut vm = BpfVm::new();
        vm.execute_with_syscall_context(&prog, &ctx) == Ok(110) // R0 = 100 + 10
    });

    test!("ebpf: packed register indices", {
        use crate::ebpf::BpfInstruction;
        let insn = BpfInstruction { code: 0, regs: 0x21, off: 0, imm: 0 };
        insn.dst_reg() == 1 && insn.src_reg() == 2
    });

    test!("ebpf: maps and helpers", {
        use crate::ebpf::vm::BpfVm;
        use crate::ebpf::{BpfInstruction, op, helpers};
        use crate::ebpf::map::{BPF_MAPS, BpfMap, BpfMapType};
        
        let map = BpfMap::new(BpfMapType::Array, 4, 8, 10);
        let map_id = BPF_MAPS.register(map);
        
        let key: u32 = 0;
        let key_ptr = &key as *const _ as u64;
        let value: u64 = 1234;
        let val_ptr = &value as *const _ as u64;

        let prog = [
            BpfInstruction { code: op::MOV, regs: 0x01, off: 0, imm: map_id as i32 },
            BpfInstruction { code: op::LD_IMM_64, regs: 0x02, off: 0, imm: (key_ptr & 0xFFFFFFFF) as i32 },
            BpfInstruction { code: 0, regs: 0, off: 0, imm: (key_ptr >> 32) as i32 },
            BpfInstruction { code: op::LD_IMM_64, regs: 0x03, off: 0, imm: (val_ptr & 0xFFFFFFFF) as i32 },
            BpfInstruction { code: 0, regs: 0, off: 0, imm: (val_ptr >> 32) as i32 },
            BpfInstruction { code: op::CALL, regs: 0x00, off: 0, imm: helpers::MAP_UPDATE_ELEM },
            BpfInstruction { code: op::RET, regs: 0x00, off: 0, imm: 0 },
        ];
        
        let mut vm = BpfVm::new();
        let res = vm.execute(&prog);
        
        if res == Ok(0) {
            if let Some(m) = BPF_MAPS.get(map_id) {
                if let Ok(ptr) = m.lookup(key_ptr) {
                    unsafe { *(ptr as *const u64) == 1234 }
                } else { false }
            } else { false }
        } else { false }
    });

    test!("ebpf: stack and memory ops", {
        use crate::ebpf::vm::BpfVm;
        use crate::ebpf::{BpfInstruction, op};
        
        let prog = [
            BpfInstruction { code: op::ST_W, regs: 0x0A, off: -4, imm: 0xDEADBEEF_u32 as i32 },
            BpfInstruction { code: op::LDX_W, regs: 0xA0, off: -4, imm: 0 }, // dst=0, src=10
            BpfInstruction { code: op::RET, regs: 0x00, off: 0, imm: 0 },
        ];
        
        let mut vm = BpfVm::new();
        vm.execute(&prog) == Ok(0xDEADBEEF)
    });

    test!("ebpf: hash maps", {
        use crate::ebpf::vm::BpfVm;
        use crate::ebpf::{BpfInstruction, op, helpers};
        use crate::ebpf::map::{BPF_MAPS, BpfMap, BpfMapType};
        
        let map = BpfMap::new(BpfMapType::Hash, 8, 8, 5);
        let map_id = BPF_MAPS.register(map);
        
        let key: u64 = 0xAAAA_BBBB;
        let value: u64 = 0x1111_2222;
        let key_ptr = &key as *const _ as u64;
        let val_ptr = &value as *const _ as u64;

        let prog = [
            BpfInstruction { code: op::MOV, regs: 0x01, off: 0, imm: map_id as i32 },
            BpfInstruction { code: op::LD_IMM_64, regs: 0x02, off: 0, imm: (key_ptr & 0xFFFFFFFF) as i32 },
            BpfInstruction { code: 0, regs: 0, off: 0, imm: (key_ptr >> 32) as i32 },
            BpfInstruction { code: op::LD_IMM_64, regs: 0x03, off: 0, imm: (val_ptr & 0xFFFFFFFF) as i32 },
            BpfInstruction { code: 0, regs: 0, off: 0, imm: (val_ptr >> 32) as i32 },
            BpfInstruction { code: op::CALL, regs: 0x00, off: 0, imm: helpers::MAP_UPDATE_ELEM },
            BpfInstruction { code: op::RET, regs: 0x00, off: 0, imm: 0 },
        ];
        
        let mut vm = BpfVm::new();
        if vm.execute(&prog) != Ok(0) { 
            false 
        } else {
            if let Some(m) = BPF_MAPS.get(map_id) {
                if let Ok(ptr) = m.lookup(key_ptr) {
                    unsafe { *(ptr as *const u64) == 0x1111_2222 }
                } else { false }
            } else { false }
        }
    });

    // ── Snapshot Persistence tests ───────────────────────────────────────────
    test!("[Path 1] snapshot: RAM serialization and restore", {
        use crate::process::snapshot::Snapshotable;
        
        let mut proc = Process::new(Pid(1234), AbiKind::ZiqaNative, VirtAddr::new(0x1000), VirtAddr::new(0x7000));
        proc.cpu_state.rax = 0xDEADBEEF;
        proc.cpu_state.rbx = 0x12345678;
        proc.brk = 0x5000_0000;
        
        let data = proc.snapshot();
        if data.is_empty() {
            false
        } else {
            let mut proc2 = Process::new(Pid(5678), AbiKind::ZiqaNative, VirtAddr::new(0), VirtAddr::new(0));
            if !proc2.restore(&data) { 
                false
            } else {
                proc2.cpu_state.rax == 0xDEADBEEF && 
                proc2.cpu_state.rbx == 0x12345678 &&
                proc2.brk == 0x5000_0000 &&
                proc2.abi == AbiKind::ZiqaNative
            }
        }
    });

    test!("[Path 1] snapshot: page content persistence", {
        use crate::process::snapshot::Snapshotable;
        use crate::process::vma::Vma;
        use crate::memory::paging::MemoryRegionFlags;
        
        let mut proc = Process::new(Pid(1234), AbiKind::ZiqaNative, VirtAddr::new(0x1000), VirtAddr::new(0x7000));
        
        let vaddr = VirtAddr::new(0x100_0000);
        let vma = Vma::new(vaddr, 4096, MemoryRegionFlags::read_write());
        proc.add_region(vma);
        
        let init_ok = if let Some(frame) = crate::memory::paging::create_process_page_table() {
            proc.page_table_frame = Some(frame);
            
            let mut fa_guard = crate::memory::FRAME_ALLOCATOR.lock();
            let fa = fa_guard.as_mut().unwrap();
            use x86_64::structures::paging::{FrameAllocator, Page, PageTableFlags, Mapper};
            let page_frame = fa.allocate_frame().unwrap();
            drop(fa_guard);
            
            let po = crate::memory::paging::phys_offset();
            unsafe {
                let l4 = &mut *( (po + frame.start_address().as_u64()).as_mut_ptr() );
                let mut mapper = x86_64::structures::paging::OffsetPageTable::new(l4, po);
                let page = Page::containing_address(vaddr);
                let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE;
                let mut fa_guard = crate::memory::FRAME_ALLOCATOR.lock();
                let fa = fa_guard.as_mut().unwrap();
                let _ = mapper.map_to(page, page_frame, flags, fa).unwrap().flush();
            }
            
            let virt = po + page_frame.start_address().as_u64();
            unsafe {
                core::ptr::write(virt.as_mut_ptr::<u32>(), 0xABCD_EF01);
            }
            true
        } else {
            false
        };

        if !init_ok {
            false
        } else {
            let data = proc.snapshot();
            let mut proc2 = Process::new(Pid(5678), AbiKind::ZiqaNative, VirtAddr::new(0), VirtAddr::new(0));
            if !proc2.restore(&data) { 
                false
            } else {
                if proc2.vmas.len() != 1 || proc2.vmas[0].start != vaddr { 
                    false
                } else {
                    if let Some(root) = proc2.page_table_frame {
                        let po = crate::memory::paging::phys_offset();
                        if let Some(f) = crate::memory::paging::get_phys_frame(root, vaddr) {
                            let val = unsafe { *( (po + f.start_address().as_u64()).as_ptr::<u32>() ) };
                            val == 0xABCD_EF01
                        } else { false }
                    } else { false }
                }
            }
        }
    });

    test!("[Path 2] userspace: capability-based I/O (libposix simulation)", {
        use crate::abi::syscall::{nr, SyscallContext};
        
        let mut proc = Process::new(Pid(777), AbiKind::ZiqaNative, VirtAddr::new(0), VirtAddr::new(0));
        
        // 1. ZIQA_CAP_REQUEST("pipe:test_cap") - use pipe scheme which exists
        let path = b"pipe:test_cap";
        let path_ptr = path.as_ptr() as u64;
        let mut ctx = SyscallContext::new(nr::ZIQA_CAP_REQUEST, [1, path_ptr, path.len() as u64, 0, 0, 0], &mut proc);
        let registry = crate::abi::AbiRegistry::new();
        let handler = crate::abi::handler::KernelSyscallHandler;
        
        let mut ok = false;
        let res = crate::abi::syscall::dispatch_syscall(&registry, &handler, &mut ctx);
        if let Ok(cap_id) = res {
            // 2. ZIQA_CAP_WRITE(cap_id, "cap-test", 8, 0)
            let data = b"cap-test";
            let mut ctx2 = SyscallContext::new(nr::ZIQA_CAP_WRITE, [cap_id, data.as_ptr() as u64, 8, 0, 0, 0], &mut proc);
            let res2 = crate::abi::syscall::dispatch_syscall(&registry, &handler, &mut ctx2);
            if let Ok(written) = res2 {
                if written == 8 {
                    // 3. ZIQA_CAP_READ(cap_id, buf, 8, 0)
                    let mut buf = [0u8; 8];
                    let mut ctx3 = SyscallContext::new(nr::ZIQA_CAP_READ, [cap_id, buf.as_mut_ptr() as u64, 8, 0, 0, 0], &mut proc);
                    let res3 = crate::abi::syscall::dispatch_syscall(&registry, &handler, &mut ctx3);
                    ok = res3.is_ok() && res3.unwrap() == 8 && &buf == b"cap-test";
                }
            }
        }
        ok
    });

    println!("[TEST] Fixed results: {}/{} passed", passed, passed + failed);
}
