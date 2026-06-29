/// ARM64 register state and interrupt stack definitions
/// Based on Redox OS architecture.

use crate::println;

#[derive(Default, Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct ScratchRegisters {
    pub x0: usize,
    pub x1: usize,
    pub x2: usize,
    pub x3: usize,
    pub x4: usize,
    pub x5: usize,
    pub x6: usize,
    pub x7: usize,
    pub x8: usize,
    pub x9: usize,
    pub x10: usize,
    pub x11: usize,
    pub x12: usize,
    pub x13: usize,
    pub x14: usize,
    pub x15: usize,
    pub x16: usize,
    pub x17: usize,
    pub x18: usize,
    pub _padding: usize,
}

#[derive(Default, Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct PreservedRegisters {
    pub x19: usize,
    pub x20: usize,
    pub x21: usize,
    pub x22: usize,
    pub x23: usize,
    pub x24: usize,
    pub x25: usize,
    pub x26: usize,
    pub x27: usize,
    pub x28: usize,
    pub x29: usize,
    pub x30: usize,
}

#[derive(Default, Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct IretRegisters {
    pub sp_el0: usize,
    pub esr_el1: usize,
    pub spsr_el1: usize,
    pub elr_el1: usize,
}

#[derive(Default, Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct InterruptStack {
    pub iret: IretRegisters,
    pub scratch: ScratchRegisters,
    pub preserved: PreservedRegisters,
}

impl InterruptStack {
    pub fn dump(&self) {
        println!("ELR_EL1:  0x{:016x}", self.iret.elr_el1);
        println!("SPSR_EL1: 0x{:016x}", self.iret.spsr_el1);
        println!("ESR_EL1:  0x{:016x}", self.iret.esr_el1);
        println!("X0:       0x{:016x}", self.scratch.x0);
        println!("X8:       0x{:016x}", self.scratch.x8);
        println!("LR:       0x{:016x}", self.preserved.x30);
        println!("SP_EL0:   0x{:016x}", self.iret.sp_el0);
    }
    
    pub fn init(&mut self) {
        // Architecture specific initialization
    }
}

// Macros for assembly stubs
#[macro_export]
macro_rules! push_scratch {
    () => {
        "
        str     x18,      [sp, #-16]!
        stp     x16, x17, [sp, #-16]!
        stp     x14, x15, [sp, #-16]!
        stp     x12, x13, [sp, #-16]!
        stp     x10, x11, [sp, #-16]!
        stp     x8, x9, [sp, #-16]!
        stp     x6, x7, [sp, #-16]!
        stp     x4, x5, [sp, #-16]!
        stp     x2, x3, [sp, #-16]!
        stp     x0, x1, [sp, #-16]!
        "
    };
}

#[macro_export]
macro_rules! pop_scratch {
    () => {
        "
        ldp     x0, x1, [sp], #16
        ldp     x2, x3, [sp], #16
        ldp     x4, x5, [sp], #16
        ldp     x6, x7, [sp], #16
        ldp     x8, x9, [sp], #16
        ldp     x10, x11, [sp], #16
        ldp     x12, x13, [sp], #16
        ldp     x14, x15, [sp], #16
        ldp     x16, x17, [sp], #16
        ldr     x18,      [sp], #16
        "
    };
}

#[macro_export]
macro_rules! push_preserved {
    () => {
        "
        stp     x29, x30, [sp, #-16]!
        stp     x27, x28, [sp, #-16]!
        stp     x25, x26, [sp, #-16]!
        stp     x23, x24, [sp, #-16]!
        stp     x21, x22, [sp, #-16]!
        stp     x19, x20, [sp, #-16]!
        "
    };
}

#[macro_export]
macro_rules! pop_preserved {
    () => {
        "
        ldp     x19, x20, [sp], #16
        ldp     x21, x22, [sp], #16
        ldp     x23, x24, [sp], #16
        ldp     x25, x26, [sp], #16
        ldp     x27, x28, [sp], #16
        ldp     x29, x30, [sp], #16
        "
    };
}

#[macro_export]
macro_rules! push_special {
    () => {
        "
        mrs     x14, spsr_el1
        mrs     x15, elr_el1
        stp     x14, x15, [sp, #-16]!
        mrs     x14, sp_el0
        mrs     x15, esr_el1
        stp     x14, x15, [sp, #-16]!
        "
    };
}

#[macro_export]
macro_rules! pop_special {
    () => {
        "
        ldp     x14, x15, [sp], #16
        msr     esr_el1, x15
        msr     sp_el0, x14
        ldp     x14, x15, [sp], #16
        msr     elr_el1, x15
        msr     spsr_el1, x14
        "
    };
}

#[macro_export]
macro_rules! exception_stack {
    ($name:ident, $inner:path) => {
        #[unsafe(naked)]
        #[no_mangle]
        pub unsafe extern \"C\" fn $name() {
            core::arch::naked_asm!(
                push_preserved!(),
                push_scratch!(),
                push_special!(),
                \"mov x0, sp\",
                \"bl {}\",
                pop_special!(),
                pop_scratch!(),
                pop_preserved!(),
                \"eret\",
                sym $inner
            );
        }
    };
}
