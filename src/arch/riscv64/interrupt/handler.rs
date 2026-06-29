/// RISC-V register state and interrupt stack definitions
/// Based on Redox OS architecture.

use crate::println;

#[derive(Default, Debug, Clone, Copy)]
#[repr(C)]
pub struct Registers {
    pub x1: usize,  // ra
    pub x2: usize,  // sp
    pub x3: usize,  // gp
    pub x4: usize,  // tp
    pub x5: usize,  // t0
    pub x6: usize,  // t1
    pub x7: usize,  // t2
    pub x8: usize,  // s0/fp
    pub x9: usize,  // s1
    pub x10: usize, // a0
    pub x11: usize,
    pub x12: usize,
    pub x13: usize,
    pub x14: usize,
    pub x15: usize,
    pub x16: usize,
    pub x17: usize, // a7
    pub x18: usize, // s2
    pub x19: usize,
    pub x20: usize,
    pub x21: usize,
    pub x22: usize,
    pub x23: usize,
    pub x24: usize,
    pub x25: usize,
    pub x26: usize,
    pub x27: usize, // s11
    pub x28: usize, // t3
    pub x29: usize,
    pub x30: usize,
    pub x31: usize, // t6
}

#[derive(Default, Debug, Clone, Copy)]
#[repr(C)]
pub struct IretRegisters {
    pub sepc: usize,
}

#[derive(Default, Debug, Clone, Copy)]
#[repr(C)]
pub struct InterruptStack {
    pub registers: Registers,
    pub iret: IretRegisters,
}

impl InterruptStack {
    pub fn dump(&self) {
        println!("SEPC: 0x{:016x}", self.iret.sepc);
        println!("RA:   0x{:016x}", self.registers.x1);
        println!("SP:   0x{:016x}", self.registers.x2);
        println!("A0:   0x{:016x}", self.registers.x10);
        println!("A7:   0x{:016x}", self.registers.x17);
    }
    
    pub fn init(&mut self) {
        // Architecture specific initialization
    }
}

#[macro_export]
macro_rules! push_registers {
    () => {
        "
        addi    sp, sp, -32 * 8
        sd      x1, (0 * 8)(sp)
        sd      x3, (2 * 8)(sp)
        sd      x5, (4 * 8)(sp)
        sd      x6, (5 * 8)(sp)
        sd      x7, (6 * 8)(sp)
        sd      x8, (7 * 8)(sp)
        sd      x9, (8 * 8)(sp)
        sd      x10, (9 * 8)(sp)
        sd      x11, (10 * 8)(sp)
        sd      x12, (11 * 8)(sp)
        sd      x13, (12 * 8)(sp)
        sd      x14, (13 * 8)(sp)
        sd      x15, (14 * 8)(sp)
        sd      x16, (15 * 8)(sp)
        sd      x17, (16 * 8)(sp)
        sd      x18, (17 * 8)(sp)
        sd      x19, (18 * 8)(sp)
        sd      x20, (19 * 8)(sp)
        sd      x21, (20 * 8)(sp)
        sd      x22, (21 * 8)(sp)
        sd      x23, (22 * 8)(sp)
        sd      x24, (23 * 8)(sp)
        sd      x25, (24 * 8)(sp)
        sd      x26, (25 * 8)(sp)
        sd      x27, (26 * 8)(sp)
        sd      x28, (27 * 8)(sp)
        sd      x29, (28 * 8)(sp)
        sd      x30, (29 * 8)(sp)
        sd      x31, (30 * 8)(sp)

        csrr    t0, sepc
        sd      t0, (31 * 8)(sp)
        "
    };
}

#[macro_export]
macro_rules! pop_registers {
    () => {
        "
        ld      t0, (31 * 8)(sp)
        csrw    sepc, t0

        ld      x1, (0 * 8)(sp)
        ld      x3, (2 * 8)(sp)
        ld      x4, (3 * 8)(sp)
        ld      x5, (4 * 8)(sp)
        ld      x6, (5 * 8)(sp)
        ld      x7, (6 * 8)(sp)
        ld      x8, (7 * 8)(sp)
        ld      x9, (8 * 8)(sp)
        ld      x10, (9 * 8)(sp)
        ld      x11, (10 * 8)(sp)
        ld      x12, (11 * 8)(sp)
        ld      x13, (12 * 8)(sp)
        ld      x14, (13 * 8)(sp)
        ld      x15, (14 * 8)(sp)
        ld      x16, (15 * 8)(sp)
        ld      x17, (16 * 8)(sp)
        ld      x18, (17 * 8)(sp)
        ld      x19, (18 * 8)(sp)
        ld      x20, (19 * 8)(sp)
        ld      x21, (20 * 8)(sp)
        ld      x22, (21 * 8)(sp)
        ld      x23, (22 * 8)(sp)
        ld      x24, (23 * 8)(sp)
        ld      x25, (24 * 8)(sp)
        ld      x26, (25 * 8)(sp)
        ld      x27, (26 * 8)(sp)
        ld      x28, (27 * 8)(sp)
        ld      x29, (28 * 8)(sp)
        ld      x30, (29 * 8)(sp)
        ld      x31, (30 * 8)(sp)
        ld      sp, (1 * 8)(sp)
        "
    };
}
