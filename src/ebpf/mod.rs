pub mod verifier;
/// eBPF (extended Berkeley Packet Filter) subsystem for ZiqaKernel
///
/// Allows running safe, verified code in kernel space for
/// tracing, networking, and security auditing.
pub mod vm;
pub mod attach;
pub mod map;

/// eBPF Instruction representation (Standard 8-byte format)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct BpfInstruction {
    pub code: u8,
    /// Packed registers: dst_reg (low 4 bits), src_reg (high 4 bits)
    pub regs: u8,
    pub off: i16,
    pub imm: i32,
}

impl BpfInstruction {
    #[inline(always)]
    pub fn dst_reg(&self) -> u8 {
        self.regs & 0x0F
    }

    #[inline(always)]
    pub fn src_reg(&self) -> u8 {
        (self.regs >> 4) & 0x0F
    }
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

    pub const ALU_ADD_X: u8 = 0x0f;
    pub const ALU_SUB_X: u8 = 0x1f;
    pub const ALU_MUL_X: u8 = 0x2f;
    pub const ALU_DIV_X: u8 = 0x3f;
    pub const ALU_AND_X: u8 = 0x5f;
    pub const ALU_OR_X: u8 = 0x4f;
    pub const ALU_XOR_X: u8 = 0xaf;
    pub const ALU_LSH_X: u8 = 0x6f;
    pub const ALU_RSH_X: u8 = 0x7f;

    pub const JMP_JA: u8 = 0x05;
    pub const JMP_JEQ: u8 = 0x15;
    pub const JMP_JNE: u8 = 0x55;
    pub const JMP_JGT: u8 = 0x25;
    pub const JMP_JGE: u8 = 0x35;

    pub const JMP_JEQ_X: u8 = 0x1d;
    pub const JMP_JNE_X: u8 = 0x5d;
    pub const JMP_JGT_X: u8 = 0x2d;
    pub const JMP_JGE_X: u8 = 0x3d;

    pub const RET: u8 = 0x95;
    pub const MOV: u8 = 0xb7;
    pub const MOV_X: u8 = 0xbf;
    pub const CALL: u8 = 0x85;
    pub const LD_IMM_64: u8 = 0x18;

    // Memory Access (Load/Store)
    pub const LDX_W: u8 = 0x61;   // R0 = *(u32 *)(R1 + off)
    pub const LDX_DW: u8 = 0x79;  // R0 = *(u64 *)(R1 + off)
    pub const STX_W: u8 = 0x63;   // *(u32 *)(R0 + off) = R1
    pub const STX_DW: u8 = 0x7b;  // *(u64 *)(R0 + off) = R1
    pub const ST_W: u8 = 0x62;    // *(u32 *)(R0 + off) = imm
    pub const ST_DW: u8 = 0x7a;   // *(u64 *)(R0 + off) = imm
}

/// eBPF Helper Function IDs
pub mod helpers {
    pub const MAP_LOOKUP_ELEM: i32 = 1;
    pub const MAP_UPDATE_ELEM: i32 = 2;
    pub const MAP_DELETE_ELEM: i32 = 3;
    pub const KTIME_GET_NS: i32 = 4;
    pub const TRACE_PRINTK: i32 = 5;
    pub const GET_CURRENT_PID: i32 = 6;
    pub const GET_CURRENT_COMM: i32 = 7;
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
