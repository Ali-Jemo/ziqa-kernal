/// eBPF (extended Berkeley Packet Filter) subsystem for ZiqaKernel
///
/// Allows running safe, verified code in kernel space for
/// tracing, networking, and security auditing.

pub mod vm;
pub mod verifier;

/// eBPF Instruction representation
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct BpfInstruction {
    pub code: u8,
    pub dst_reg: u8,
    pub src_reg: u8,
    pub off: i16,
    pub imm: i32,
}

/// Common eBPF Opcodes
pub mod op {
    pub const ALU_ADD: u8 = 0x07;
    pub const ALU_SUB: u8 = 0x17;
    pub const ALU_MUL: u8 = 0x27;
    pub const ALU_DIV: u8 = 0x37;
    pub const ALU_AND: u8 = 0x57;
    pub const ALU_OR: u8 = 0x47;
    pub const ALU_XOR: u8 = 0xa7;
    pub const ALU_LSH: u8 = 0x67;
    pub const ALU_RSH: u8 = 0x77;

    pub const JMP_JA: u8 = 0x05;
    pub const JMP_JEQ: u8 = 0x15;
    pub const JMP_JNE: u8 = 0x55;
    pub const JMP_JGT: u8 = 0x25;
    pub const JMP_JGE: u8 = 0x35;

    pub const RET: u8 = 0x95;
    pub const MOV: u8 = 0xb7;
}

/// The result of eBPF program execution
pub type BpfResult = Result<u64, BpfError>;

#[derive(Debug)]
pub enum BpfError {
    /// Verifier rejected the program
    VerificationFailed(&'static str),
    /// Execution error (division by zero, etc.)
    ExecutionError,
    /// Out of bounds memory access
    OutOfBounds,
}
