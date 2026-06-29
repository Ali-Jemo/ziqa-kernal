/// AArch64 ID register access (ID_AA64*)
/// Ported from Redox OS.

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy)]
    pub struct AA64Isar0: u64 {
        const RNDR = 0xF << 60;
        const TLB  = 0xF << 56;
        const TS   = 0xF << 52;
        const FHM  = 0xF << 48;
        const DP   = 0xF << 44;
        const SM4  = 0xF << 40;
        const SM3  = 0xF << 36;
        const SHA3 = 0xF << 32;
        const RDM  = 0xF << 28;
        const ATOMIC = 0xF << 20;
        const CRC32 = 0xF << 16;
        const SHA2 = 0xF << 12;
        const SHA1 = 0xF << 8;
        const AES  = 0xF << 4;
    }
}

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy)]
    pub struct AA64Isar1: u64 {
        const I8MM   = 0xF << 52;
        const BF16   = 0xF << 44;
        const SB     = 0xF << 36;
        const FRINTTS = 0xF << 32;
        const GPI    = 0xF << 28;
        const GPA    = 0xF << 24;
        const LRCPC  = 0xF << 20;
        const FCMA   = 0xF << 16;
        const JSCVT  = 0xF << 12;
        const API    = 0xF << 8;
        const APA    = 0xF << 4;
        const DPB    = 0xF << 0;
    }
}

pub fn aa64isar0() -> AA64Isar0 {
    let ret: u64;
    unsafe { core::arch::asm!("mrs {}, ID_AA64ISAR0_EL1", out(reg) ret) };
    AA64Isar0::from_bits_truncate(ret)
}

pub fn aa64isar1() -> AA64Isar1 {
    let ret: u64;
    unsafe { core::arch::asm!("mrs {}, ID_AA64ISAR1_EL1", out(reg) ret) };
    AA64Isar1::from_bits_truncate(ret)
}
