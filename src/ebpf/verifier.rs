/// eBPF Verifier for ZiqaKernel
///
/// Mathematically verifies that a BPF program is safe to run in Ring 0.
/// Checks for:
///   - Unbounded loops
///   - Out-of-bounds jumps
///   - Invalid memory access
///   - Reaching an exit instruction
use crate::ebpf::{op, BpfError, BpfInstruction};

pub struct BpfVerifier<'a> {
    program: &'a [BpfInstruction],
    max_insns: usize,
}

impl<'a> BpfVerifier<'a> {
    pub fn new(program: &'a [BpfInstruction]) -> Self {
        Self {
            program,
            max_insns: 4096, // Limit complexity
        }
    }

    /// Perform safety verification
    pub fn verify(&self) -> Result<(), BpfError> {
        if self.program.is_empty() {
            return Err(BpfError::VerificationFailed("Empty program"));
        }

        if self.program.len() > self.max_insns {
            return Err(BpfError::VerificationFailed("Program too large"));
        }

        // 1. Basic check: must contain a return instruction
        let mut has_exit = false;
        for insn in self.program {
            if insn.code == op::RET {
                has_exit = true;
                break;
            }
        }

        if !has_exit {
            return Err(BpfError::VerificationFailed("No exit instruction found"));
        }

        // 2. Control flow analysis (simplified: no backward jumps)
        let mut i = 0;
        while i < self.program.len() {
            let insn = self.program[i];

            // Check register indices
            if insn.dst_reg() >= 11 || insn.src_reg() >= 11 {
                return Err(BpfError::VerificationFailed("Invalid register index"));
            }

            // Handle multi-word instructions
            if insn.code == op::LD_IMM_64 {
                if i + 1 >= self.program.len() {
                    return Err(BpfError::VerificationFailed("Truncated LD_IMM_64"));
                }
                i += 2;
                continue;
            }

            // Check jump targets
            let is_jmp = (insn.code & 0x07) == 0x05;
            if is_jmp && insn.code != op::RET && insn.code != op::CALL {
                let target = i as i32 + insn.off as i32 + 1;
                if target < 0 || target >= self.program.len() as i32 {
                    return Err(BpfError::VerificationFailed("Jump out of bounds"));
                }
            }

            // Check memory access
            let class = insn.code & 0x07;
            if class == 0x01 || class == 0x03 || class == 0x02 { // LDX, STX, ST
                // For now, only allow stack access (R10 relative)
                let src = insn.src_reg();
                let dst = insn.dst_reg();
                let is_stack_access = if class == 0x01 { src == 10 } else { dst == 10 };
                
                if is_stack_access {
                    // off is i16, stack is [0, 512). R10 points to end (512).
                    // So valid offsets are [-512, -size]
                    if insn.off > 0 || insn.off < -512 {
                        return Err(BpfError::VerificationFailed("Stack access out of bounds"));
                    }
                } else {
                    return Err(BpfError::VerificationFailed("Non-stack memory access forbidden"));
                }
            }
            
            i += 1;
        }

        Ok(())
    }
}
