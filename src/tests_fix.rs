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
        use crate::process::Process;
        let prog = [
            BpfInstruction { code: op::MOV_X, regs: 0x10, off: 0, imm: 0 }, // dst=0, src=1
            BpfInstruction { code: op::ALU_ADD_X, regs: 0x20, off: 0, imm: 0 }, // dst=0, src=2
            BpfInstruction { code: op::RET, regs: 0x00, off: 0, imm: 0 },
        ];
        let mut proc = Process::new(crate::process::Pid(999), crate::process::AbiKind::LinuxElf);
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
        
        let mut key: u32 = 0;
        let mut value: u64 = 1234;
        
        let key_ptr = &key as *const _ as u64;
        let val_ptr = &value as *const _ as u64;

        let prog = [
            BpfInstruction { code: op::MOV, regs: 0x01, off: 0, imm: map_id as i32 },
            // R2 = key_ptr (64-bit)
            BpfInstruction { code: op::LD_IMM_64, regs: 0x02, off: 0, imm: (key_ptr & 0xFFFFFFFF) as i32 },
            BpfInstruction { code: 0, regs: 0, off: 0, imm: (key_ptr >> 32) as i32 },
            // R3 = val_ptr (64-bit)
            BpfInstruction { code: op::LD_IMM_64, regs: 0x03, off: 0, imm: (val_ptr & 0xFFFFFFFF) as i32 },
            BpfInstruction { code: 0, regs: 0, off: 0, imm: (val_ptr >> 32) as i32 },
            BpfInstruction { code: op::CALL, regs: 0x00, off: 0, imm: helpers::MAP_UPDATE_ELEM },
            BpfInstruction { code: op::RET, regs: 0x00, off: 0, imm: 0 },
        ];
        
        let mut vm = BpfVm::new();
        let res = vm.execute(&prog);
        
        if res == Ok(0) {
            let mut check_key: u32 = 0;
            if let Some(m) = BPF_MAPS.get(map_id) {
                if let Ok(ptr) = m.lookup(&check_key as *const _ as u64) {
                    return unsafe { *(ptr as *const u64) } == 1234;
                }
            }
        }
        false
    });

    test!("ebpf: stack and memory ops", {
        use crate::ebpf::vm::BpfVm;
        use crate::ebpf::{BpfInstruction, op};
        
        // Program:
        // 1. ST_W [R10-4], 0xDEADBEEF  (Store to stack)
        // 2. LDX_W R0, [R10-4]         (Load from stack)
        // 3. RET                       (Return R0)
        let prog = [
            BpfInstruction { code: op::ST_W, regs: 0x0A, off: -4, imm: 0xDEADBEEF as i32 },
            BpfInstruction { code: op::LDX_W, regs: 0x0A, off: -4, imm: 0 }, // dst=0, src=10
            BpfInstruction { code: op::RET, regs: 0x00, off: 0, imm: 0 },
        ];
        
        let mut vm = BpfVm::new();
        vm.execute(&prog) == Ok(0xDEADBEEF)
    });
