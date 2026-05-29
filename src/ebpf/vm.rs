/// eBPF Virtual Machine for ZiqaKernel
///
/// Interprets verified BPF bytecode.
/// High performance is achieved by keeping everything in registers
/// and avoiding unnecessary memory copies.
use crate::ebpf::{op, BpfError, BpfInstruction, BpfResult};

pub struct BpfVm {
    registers: [u64; 11], // R0-R10
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
}
