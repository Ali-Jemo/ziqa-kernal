/// eBPF Virtual Machine for ZiqaKernel
///
/// Interprets verified BPF bytecode.
/// High performance is achieved by keeping everything in registers
/// and avoiding unnecessary memory copies.
use crate::ebpf::{op, BpfError, BpfInstruction, BpfResult};
use crate::abi::syscall::SyscallContext;

pub struct BpfVm {
    pub registers: [u64; 11], // R0-R10
}

impl BpfVm {
    pub fn new() -> Self {
        Self { registers: [0; 11] }
    }

    /// Execute a verified program
    pub fn execute(&mut self, program: &[BpfInstruction]) -> BpfResult {
        let mut pc = 0;

        while pc < program.len() {
            let insn = program[pc];

            // Validate register boundaries to ensure memory safety
            if insn.dst_reg >= 11 || insn.src_reg >= 11 {
                return Err(BpfError::OutOfBounds);
            }

            match insn.code {
                op::MOV => {
                    self.registers[insn.dst_reg as usize] = insn.imm as u64;
                }
                op::ALU_ADD => {
                    self.registers[insn.dst_reg as usize] += insn.imm as u64;
                }
                op::ALU_SUB => {
                    self.registers[insn.dst_reg as usize] =
                        self.registers[insn.dst_reg as usize].wrapping_sub(insn.imm as u64);
                }
                op::ALU_MUL => {
                    self.registers[insn.dst_reg as usize] =
                        self.registers[insn.dst_reg as usize].wrapping_mul(insn.imm as u64);
                }
                op::ALU_DIV => {
                    if insn.imm == 0 {
                        return Err(BpfError::ExecutionError);
                    }
                    self.registers[insn.dst_reg as usize] /= insn.imm as u64;
                }
                op::ALU_AND => {
                    self.registers[insn.dst_reg as usize] &= insn.imm as u64;
                }
                op::ALU_OR => {
                    self.registers[insn.dst_reg as usize] |= insn.imm as u64;
                }
                op::ALU_XOR => {
                    self.registers[insn.dst_reg as usize] ^= insn.imm as u64;
                }
                op::ALU_LSH => {
                    self.registers[insn.dst_reg as usize] =
                        self.registers[insn.dst_reg as usize].wrapping_shl(insn.imm as u32);
                }
                op::ALU_RSH => {
                    self.registers[insn.dst_reg as usize] =
                        self.registers[insn.dst_reg as usize].wrapping_shr(insn.imm as u32);
                }
                op::JMP_JA => {
                    pc = (pc as i32 + insn.off as i32) as usize;
                }
                op::JMP_JEQ => {
                    if self.registers[insn.dst_reg as usize] == insn.imm as u64 {
                        pc = (pc as i32 + insn.off as i32) as usize;
                    }
                }
                op::JMP_JNE => {
                    if self.registers[insn.dst_reg as usize] != insn.imm as u64 {
                        pc = (pc as i32 + insn.off as i32) as usize;
                    }
                }
                op::JMP_JGT => {
                    if self.registers[insn.dst_reg as usize] > insn.imm as u64 {
                        pc = (pc as i32 + insn.off as i32) as usize;
                    }
                }
                op::JMP_JGE => {
                    if self.registers[insn.dst_reg as usize] >= insn.imm as u64 {
                        pc = (pc as i32 + insn.off as i32) as usize;
                    }
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
    /// * `ctx` - Syscall context to initialize registers R0-R6
    /// 
    /// Returns the value in R0 after execution (or error)
    pub fn execute_with_syscall_context(
        &mut self,
        program: &[BpfInstruction],
        ctx: &SyscallContext,
    ) -> BpfResult {
        // Initialize registers from syscall context
        self.registers[0] = ctx.number; // R0: syscall number
        self.registers[1] = ctx.args[0]; // R1: arg0
        self.registers[2] = ctx.args[1]; // R2: arg1
        self.registers[3] = ctx.args[2]; // R3: arg2
        self.registers[4] = ctx.args[3]; // R4: arg3
        self.registers[5] = ctx.args[4]; // R5: arg4
        self.registers[6] = ctx.args[5]; // R6: arg5
        // R7-R10 remain zero (caller-saved? we don't care)
        // Clear retval register (R7) for exit? We'll set it separately if needed.
        // For entry, retval is not used.
        // For exit, we will set R7 to ctx.retval before calling.
        // Actually, we'll let the caller set R7 if they want to pass retval.
        // So we will not set R7 here; the caller should set it after initializing the first 7 registers.
        // We'll just set R0-R6 as above.

        // Execute the program
        let mut pc = 0;
        while pc < program.len() {
            let insn = program[pc];

            // Validate register boundaries to ensure memory safety
            if insn.dst_reg >= 11 || insn.src_reg >= 11 {
                return Err(BpfError::OutOfBounds);
            }

            match insn.code {
                op::MOV => {
                    self.registers[insn.dst_reg as usize] = insn.imm as u64;
                }
                op::ALU_ADD => {
                    self.registers[insn.dst_reg as usize] += insn.imm as u64;
                }
                op::ALU_SUB => {
                    self.registers[insn.dst_reg as usize] =
                        self.registers[insn.dst_reg as usize].wrapping_sub(insn.imm as u64);
                }
                op::ALU_MUL => {
                    self.registers[insn.dst_reg as usize] =
                        self.registers[insn.dst_reg as usize].wrapping_mul(insn.imm as u64);
                }
                op::ALU_DIV => {
                    if insn.imm == 0 {
                        return Err(BpfError::ExecutionError);
                    }
                    self.registers[insn.dst_reg as usize] /= insn.imm as u64;
                }
                op::ALU_AND => {
                    self.registers[insn.dst_reg as usize] &= insn.imm as u64;
                }
                op::ALU_OR => {
                    self.registers[insn.dst_reg as usize] |= insn.imm as u64;
                }
                op::ALU_XOR => {
                    self.registers[insn.dst_reg as usize] ^= insn.imm as u64;
                }
                op::ALU_LSH => {
                    self.registers[insn.dst_reg as usize] =
                        self.registers[insn.dst_reg as usize].wrapping_shl(insn.imm as u32);
                }
                op::ALU_RSH => {
                    self.registers[insn.dst_reg as usize] =
                        self.registers[insn.dst_reg as usize].wrapping_shr(insn.imm as u32);
                }
                op::JMP_JA => {
                    pc = (pc as i32 + insn.off as i32) as usize;
                }
                op::JMP_JEQ => {
                    if self.registers[insn.dst_reg as usize] == insn.imm as u64 {
                        pc = (pc as i32 + insn.off as i32) as usize;
                    }
                }
                op::JMP_JNE => {
                    if self.registers[insn.dst_reg as usize] != insn.imm as u64 {
                        pc = (pc as i32 + insn.off as i32) as usize;
                    }
                }
                op::JMP_JGT => {
                    if self.registers[insn.dst_reg as usize] > insn.imm as u64 {
                        pc = (pc as i32 + insn.off as i32) as usize;
                    }
                }
                op::JMP_JGE => {
                    if self.registers[insn.dst_reg as usize] >= insn.imm as u64 {
                        pc = (pc as i32 + insn.off as i32) as usize;
                    }
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
}
