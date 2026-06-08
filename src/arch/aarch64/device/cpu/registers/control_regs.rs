/// AArch64 control registers (TTBRn, SPSR, TPIDR, etc.)
/// Ported from Redox OS.

pub unsafe fn ttbr0_el1() -> u64 {
    let ret: u64;
    unsafe { core::arch::asm!("mrs {}, ttbr0_el1", out(reg) ret) };
    ret
}

pub unsafe fn ttbr0_el1_write(val: u64) {
    unsafe { core::arch::asm!("msr ttbr0_el1, {}", in(reg) val) };
}

pub unsafe fn ttbr1_el1() -> u64 {
    let ret: u64;
    unsafe { core::arch::asm!("mrs {}, ttbr1_el1", out(reg) ret) };
    ret
}

pub unsafe fn ttbr1_el1_write(val: u64) {
    unsafe { core::arch::asm!("msr ttbr1_el1, {}", in(reg) val) };
}

pub unsafe fn tpidr_el1() -> u64 {
    let ret: u64;
    unsafe { core::arch::asm!("mrs {}, tpidr_el1", out(reg) ret) };
    ret
}

pub unsafe fn tpidr_el1_write(val: u64) {
    unsafe { core::arch::asm!("msr tpidr_el1, {}", in(reg) val) };
}

pub unsafe fn esr_el1() -> u32 {
    let ret: u32;
    unsafe { core::arch::asm!("mrs {0:w}, esr_el1", out(reg) ret) };
    ret
}

pub unsafe fn cntfrq_el0() -> u32 {
    let ret: u32;
    unsafe { core::arch::asm!("mrs {0:w}, cntfrq_el0", out(reg) ret) };
    ret
}

pub unsafe fn midr() -> u32 {
    let ret: u32;
    unsafe { core::arch::asm!("mrs {0:w}, midr_el1", out(reg) ret) };
    ret
}
