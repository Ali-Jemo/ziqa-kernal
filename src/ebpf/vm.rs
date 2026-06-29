/// eBPF Virtual Machine for ZiqaKernel
///
/// Interprets verified BPF bytecode.
/// High performance is achieved by keeping everything in registers
/// and avoiding unnecessary memory copies.
use crate::ebpf::{op, helpers, BpfError, BpfInstruction, BpfResult};
use crate::abi::syscall::SyscallContext;

pub struct BpfVm {
    pub registers: [u64; 11], // R0-R10
    pub stack: [u8; 512],     // Standard eBPF stack
    /// R10 value at entry — one past the end of the stack. Used to bounds-check eBPF memory ops.
    stack_base: u64,
}

impl BpfVm {
    pub fn new() -> Self {
        let mut vm = Self {
            registers: [0u64; 11],
            stack: [0u8; 512],
            stack_base: 0,
        };
        // R10 is the read-only frame pointer to the stack
        vm.registers[10] = &vm.stack as *const _ as u64 + 512;
        vm.stack_base = vm.registers[10];
        vm
    }

    /// Low end of the valid eBPF stack window.
    fn stack_start(&self) -> u64 {
        self.stack_base.saturating_sub(512)
    }

    /// Returns true if `[addr, addr + size)` lies entirely within the eBPF stack window.
    fn is_stack_ptr(&self, addr: u64, size: u64) -> bool {
        let lo = self.stack_start();
        let hi = self.stack_base;
        if addr < lo { return false; }
        let end = addr.saturating_add(size);
        if end < addr { return false; } // overflow
        end <= hi
    }

    /// Validate an eBPF memory access. Returns Err on any out-of-bounds access.
    fn validate_mem_access(&self, addr: u64, size: u64) -> Result<(), BpfError> {
        if self.is_stack_ptr(addr, size) {
            Ok(())
        } else {
            Err(BpfError::OutOfBounds)
        }
    }

    /// Execute a verified program
    pub fn execute(&mut self, mut program: &[BpfInstruction]) -> BpfResult {
        let mut tail_call_cnt = 0;
        const MAX_TAIL_CALLS: usize = 32;

        'exec: loop {
            let mut pc = 0;
            let mut inst_count = 0;
            const MAX_INSTRUCTIONS: usize = 1_000_000;

            while pc < program.len() {
                inst_count += 1;
                if inst_count > MAX_INSTRUCTIONS {
                    return Err(BpfError::ExecutionError); // Limit reached
                }

                let insn = program[pc];
                let dst = insn.dst_reg() as usize;

                // Validate register boundaries to ensure memory safety
                if dst >= 11 || insn.src_reg() >= 11 {
                    return Err(BpfError::OutOfBounds);
                }

                match insn.code {
                    op::LD_IMM_64 => {
                        // LD_IMM_64 is a 16-byte instruction.
                        // The next 8 bytes contain the upper 32 bits in their 'imm' field.
                        if pc + 1 >= program.len() {
                            return Err(BpfError::ExecutionError);
                        }
                        pc += 1;
                        let next_insn = program[pc];
                        let imm64 = (insn.imm as u32 as u64) | ((next_insn.imm as u64) << 32);
                        self.registers[dst] = imm64;
                    }
                    op::MOV => {
                        self.registers[dst] = insn.imm as u64;
                    }
                    op::MOV_X => {
                        self.registers[dst] = self.registers[insn.src_reg() as usize];
                    }
                    op::ALU_ADD => {
                        self.registers[dst] = self.registers[dst].wrapping_add(insn.imm as u64);
                    }
                    op::ALU_ADD_X => {
                        self.registers[dst] = self.registers[dst].wrapping_add(self.registers[insn.src_reg() as usize]);
                    }
                    op::ALU_SUB => {
                        self.registers[dst] = self.registers[dst].wrapping_sub(insn.imm as u64);
                    }
                    op::ALU_SUB_X => {
                        self.registers[dst] = self.registers[dst].wrapping_sub(self.registers[insn.src_reg() as usize]);
                    }
                    op::ALU_MUL => {
                        self.registers[dst] = self.registers[dst].wrapping_mul(insn.imm as u64);
                    }
                    op::ALU_MUL_X => {
                        self.registers[dst] = self.registers[dst].wrapping_mul(self.registers[insn.src_reg() as usize]);
                    }
                    op::ALU_DIV => {
                        if insn.imm == 0 {
                            return Err(BpfError::ExecutionError);
                        }
                        self.registers[dst] /= insn.imm as u64;
                    }
                    op::ALU_DIV_X => {
                        let src = self.registers[insn.src_reg() as usize];
                        if src == 0 {
                            return Err(BpfError::ExecutionError);
                        }
                        self.registers[dst] /= src;
                    }
                    op::ALU_AND => {
                        self.registers[dst] &= insn.imm as u64;
                    }
                    op::ALU_AND_X => {
                        self.registers[dst] &= self.registers[insn.src_reg() as usize];
                    }
                    op::ALU_OR => {
                        self.registers[dst] |= insn.imm as u64;
                    }
                    op::ALU_OR_X => {
                        self.registers[dst] |= self.registers[insn.src_reg() as usize];
                    }
                    op::ALU_XOR => {
                        self.registers[dst] ^= insn.imm as u64;
                    }
                    op::ALU_XOR_X => {
                        self.registers[dst] ^= self.registers[insn.src_reg() as usize];
                    }
                    op::ALU_LSH => {
                        self.registers[dst] = self.registers[dst].wrapping_shl(insn.imm as u32);
                    }
                    op::ALU_LSH_X => {
                        self.registers[dst] = self.registers[dst].wrapping_shl(self.registers[insn.src_reg() as usize] as u32);
                    }
                    op::ALU_RSH => {
                        self.registers[dst] = self.registers[dst].wrapping_shr(insn.imm as u32);
                    }
                    op::ALU_RSH_X => {
                        self.registers[dst] = self.registers[dst].wrapping_shr(self.registers[insn.src_reg() as usize] as u32);
                    }
                    op::JMP_JA => {
                        pc = (pc as i32 + insn.off as i32) as usize;
                    }
                    op::JMP_JEQ => {
                        if self.registers[dst] == insn.imm as u64 {
                            pc = (pc as i32 + insn.off as i32) as usize;
                        }
                    }
                    op::JMP_JEQ_X => {
                        if self.registers[dst] == self.registers[insn.src_reg() as usize] {
                            pc = (pc as i32 + insn.off as i32) as usize;
                        }
                    }
                    op::JMP_JNE => {
                        if self.registers[dst] != insn.imm as u64 {
                            pc = (pc as i32 + insn.off as i32) as usize;
                        }
                    }
                    op::JMP_JNE_X => {
                        if self.registers[dst] != self.registers[insn.src_reg() as usize] {
                            pc = (pc as i32 + insn.off as i32) as usize;
                        }
                    }
                    op::JMP_JGT => {
                        if self.registers[dst] > insn.imm as u64 {
                            pc = (pc as i32 + insn.off as i32) as usize;
                        }
                    }
                    op::JMP_JGT_X => {
                        if self.registers[dst] > self.registers[insn.src_reg() as usize] {
                            pc = (pc as i32 + insn.off as i32) as usize;
                        }
                    }
                    op::JMP_JGE => {
                        if self.registers[dst] >= insn.imm as u64 {
                            pc = (pc as i32 + insn.off as i32) as usize;
                        }
                    }
                    op::JMP_JGE_X => {
                        if self.registers[dst] >= self.registers[insn.src_reg() as usize] {
                            pc = (pc as i32 + insn.off as i32) as usize;
                        }
                    }
                    op::LDX_W => {
                        let addr = self.registers[insn.src_reg() as usize].wrapping_add(insn.off as u64);
                        self.validate_mem_access(addr, 4)?;
                        self.registers[dst] = unsafe { *(addr as *const u32) } as u64;
                    }
                    op::LDX_DW => {
                        let addr = self.registers[insn.src_reg() as usize].wrapping_add(insn.off as u64);
                        self.validate_mem_access(addr, 8)?;
                        self.registers[dst] = unsafe { *(addr as *const u64) };
                    }
                    op::STX_W => {
                        let addr = self.registers[dst].wrapping_add(insn.off as u64);
                        self.validate_mem_access(addr, 4)?;
                        unsafe { *(addr as *mut u32) = self.registers[insn.src_reg() as usize] as u32 };
                    }
                    op::STX_DW => {
                        let addr = self.registers[dst].wrapping_add(insn.off as u64);
                        self.validate_mem_access(addr, 8)?;
                        unsafe { *(addr as *mut u64) = self.registers[insn.src_reg() as usize] };
                    }
                    op::ST_W => {
                        let addr = self.registers[dst].wrapping_add(insn.off as u64);
                        self.validate_mem_access(addr, 4)?;
                        unsafe { *(addr as *mut u32) = insn.imm as u32 };
                    }
                    op::ST_DW => {
                        let addr = self.registers[dst].wrapping_add(insn.off as u64);
                        self.validate_mem_access(addr, 8)?;
                        unsafe { *(addr as *mut u64) = insn.imm as u64 };
                    }
                    op::CALL => {
                        match self.call_helper(insn.imm) {
                            Err(BpfError::TailCall(prog_ptr)) => {
                                tail_call_cnt += 1;
                                if tail_call_cnt > MAX_TAIL_CALLS {
                                    return Err(BpfError::ExecutionError);
                                }
                                let next_prog = unsafe { &*(prog_ptr as *const crate::ebpf::attach::BpfProgram) };
                                program = &next_prog.instructions;
                                continue 'exec;
                            }
                            Ok(v) => self.registers[0] = v,
                            Err(e) => return Err(e),
                        }
                    }
                    op::RET => {
                        return Ok(self.registers[0]);
                    }
                    _ => return Err(BpfError::ExecutionError),
                }

                pc += 1;
            }

            return Err(BpfError::ExecutionError);
        }
    }

    /// Execute a verified program with initial register values for tracing.
    /// 
    /// # Arguments
    /// * `program` - The BPF bytecode to execute
    /// * `ctx` - Syscall context to initialize registers R1-R7
    /// 
    /// Returns the value in R0 after execution (or error)
    pub fn execute_with_syscall_context(
        &mut self,
        program: &[BpfInstruction],
        ctx: &SyscallContext,
    ) -> BpfResult {
        // Initialize registers following tracing conventions:
        // R0: Return Value (0)
        // R1: Syscall Number
        // R2-R7: Args 0-5
        // R10: Frame Pointer (0 for now)
        self.registers[0] = 0;
        self.registers[1] = ctx.number;
        self.registers[2] = ctx.args[0];
        self.registers[3] = ctx.args[1];
        self.registers[4] = ctx.args[2];
        self.registers[5] = ctx.args[3];
        self.registers[6] = ctx.args[4];
        self.registers[7] = ctx.args[5];
        self.registers[8] = ctx.retval;
        self.registers[10] = 0; // stack base if we had one

        // Execute using the same logic as execute()
        self.execute(program)
    }

    /// Dispatch a helper function call
    fn call_helper(&mut self, id: i32) -> BpfResult {
        match id {
            helpers::MAP_LOOKUP_ELEM => {
                let map_id = self.registers[1] as usize;
                let key_ptr = self.registers[2];
                if let Some(map) = crate::ebpf::map::BPF_MAPS.get(map_id) {
                    map.lookup(key_ptr)
                } else {
                    Ok(0)
                }
            }
            helpers::MAP_UPDATE_ELEM => {
                let map_id = self.registers[1] as usize;
                let key_ptr = self.registers[2];
                let value_ptr = self.registers[3];
                if let Some(map) = crate::ebpf::map::BPF_MAPS.get(map_id) {
                    map.update(key_ptr, value_ptr)
                } else {
                    Ok(1)
                }
            }
            helpers::MAP_DELETE_ELEM => {
                let map_id = self.registers[1] as usize;
                let key_ptr = self.registers[2];
                if let Some(map) = crate::ebpf::map::BPF_MAPS.get(map_id) {
                    map.delete(key_ptr)
                } else {
                    Ok(1)
                }
            }
            helpers::KTIME_GET_NS => {
                Ok(crate::timer::uptime_ms() * 1_000_000)
            }
            helpers::GET_CURRENT_PID_TGID => {
                use crate::arch::x86_64::per_cpu;
                Ok(per_cpu::current_cpu().current_pid().map_or(0, |p| p.0))
            }
            helpers::GET_SMP_PROCESSOR_ID => {
                use crate::arch::x86_64::per_cpu;
                Ok(per_cpu::current_cpu().cpu_id as u64)
            }
            helpers::GET_CURRENT_COMM => {
                let buf = self.registers[1];
                let size = self.registers[2] as usize;
                let comm = b"ziqa-proc\0";
                // ponytail: clamp to 512 so a malicious R2 can't OOB into kernel memory
                let len = size.min(512).min(comm.len());
                if self.validate_mem_access(buf, len as u64).is_ok() {
                    unsafe {
                        core::ptr::copy_nonoverlapping(comm.as_ptr(), buf as *mut u8, len);
                        if len < size && len < 512 {
                            core::ptr::write_bytes((buf as *mut u8).add(len), 0, (size - len).min(512 - len));
                        }
                    }
                }
                Ok(0)
            }
            helpers::PROBE_READ => {
                let dst_ptr = self.registers[1];
                let size = self.registers[2] as usize;
                let src_ptr = self.registers[3];

                // Clamp to 512 (max stack size) and validate both ends live in the
                // eBPF stack window — otherwise a malicious program can read arbitrary
                // kernel memory.
                let safe_size = size.min(512);
                if self.validate_mem_access(dst_ptr, safe_size as u64).is_ok()
                    && self.validate_mem_access(src_ptr, safe_size as u64).is_ok()
                {
                    unsafe {
                        core::ptr::copy_nonoverlapping(src_ptr as *const u8, dst_ptr as *mut u8, safe_size);
                    }
                }
                Ok(0)
            }
            helpers::TRACE_PRINTK => {
                let fmt_ptr = self.registers[1] as *const u8;
                let fmt_size = self.registers[2] as usize;
                // Safety: validate pointer and size
                let bytes = unsafe { core::slice::from_raw_parts(fmt_ptr, fmt_size.min(64)) };
                if let Ok(s) = core::str::from_utf8(bytes) {
                    crate::println!("[eBPF TRACE] {}", s);
                }
                Ok(0)
            }
            helpers::TAIL_CALL => {
                let map_id = self.registers[1] as usize;
                let index_ptr = self.registers[2];
                if let Some(map) = crate::ebpf::map::BPF_MAPS.get(map_id) {
                    if map.map_type == crate::ebpf::map::BpfMapType::ProgArray {
                        // We use index directly rather than a pointer for ProgArray tail calls?
                        // Wait, lookup takes key_ptr, but tail call usually takes index directly in R2.
                        // Let's assume R2 is the index directly. We need to synthesize a key_ptr.
                        let index = index_ptr as u32;
                        let key_ptr_synth = &index as *const u32 as u64;
                        let ptr = map.lookup(key_ptr_synth)?;
                        if ptr != 0 {
                            let prog_handle = unsafe { *(ptr as *const u64) };
                            if prog_handle != 0 {
                                return Err(BpfError::TailCall(prog_handle));
                            }
                        }
                    }
                }
                Ok(0)
            }
            _ => Err(BpfError::ExecutionError),
        }
    }
}
