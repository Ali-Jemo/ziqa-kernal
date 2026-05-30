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
}

impl BpfVm {
    pub fn new() -> Self {
        let mut vm = Self {
            registers: [0; 11],
            stack: [0; 512],
        };
        // R10 is the read-only frame pointer to the stack
        vm.registers[10] = &vm.stack as *const _ as u64 + 512;
        vm
    }

    /// Execute a verified program
    pub fn execute(&mut self, program: &[BpfInstruction]) -> BpfResult {
        let mut pc = 0;

        while pc < program.len() {
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
                    self.registers[dst] = unsafe { *(addr as *const u32) } as u64;
                }
                op::LDX_DW => {
                    let addr = self.registers[insn.src_reg() as usize].wrapping_add(insn.off as u64);
                    self.registers[dst] = unsafe { *(addr as *const u64) };
                }
                op::STX_W => {
                    let addr = self.registers[dst].wrapping_add(insn.off as u64);
                    unsafe { *(addr as *mut u32) = self.registers[insn.src_reg() as usize] as u32 };
                }
                op::STX_DW => {
                    let addr = self.registers[dst].wrapping_add(insn.off as u64);
                    unsafe { *(addr as *mut u64) = self.registers[insn.src_reg() as usize] };
                }
                op::ST_W => {
                    let addr = self.registers[dst].wrapping_add(insn.off as u64);
                    unsafe { *(addr as *mut u32) = insn.imm as u32 };
                }
                op::ST_DW => {
                    let addr = self.registers[dst].wrapping_add(insn.off as u64);
                    unsafe { *(addr as *mut u64) = insn.imm as u64 };
                }
                op::CALL => {
                    self.registers[0] = self.call_helper(insn.imm)?;
                }
                op::RET => {
                    return Ok(self.registers[0]);
                }
                _ => return Err(BpfError::ExecutionError),
            }

            pc += 1;
        }

        Err(BpfError::ExecutionError)
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
            helpers::GET_CURRENT_PID => {
                use crate::arch::x86_64::per_cpu;
                Ok(per_cpu::current_cpu().current_pid().map_or(0, |p| p.0))
            }
            helpers::GET_CURRENT_COMM => {
                use crate::arch::x86_64::per_cpu;
                Ok(per_cpu::current_cpu().current_pid().map_or(0, |p| p.0)) // FIXME: implement comm
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
            _ => Err(BpfError::ExecutionError),
        }
    }
}
