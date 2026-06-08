/// RISC-V Platform-Level Interrupt Controller (PLIC)
/// Ported from Redox OS.

use core::ptr::{read_volatile, write_volatile};

pub struct Plic {
    base: usize,
    ndev: usize,
    context: usize,
}

impl Plic {
    pub fn new(base: usize, ndev: usize) -> Self {
        Self {
            base,
            ndev,
            context: 0,
        }
    }

    pub fn init(&mut self) {
        // Disable all interrupts
        for i in 0..=self.ndev / 32 {
            unsafe {
                write_volatile(
                    (self.base + 0x2000 + self.context * 0x80 + i * 4) as *mut u32,
                    0,
                );
            }
        }
        // Set priority threshold to 0
        unsafe {
            write_volatile(
                (self.base + 0x200000 + self.context * 0x1000) as *mut u32,
                0,
            );
        }
    }

    pub fn enable_irq(&self, irq: u32) {
        let lane = irq as usize / 32;
        let bit = 1 << (irq % 32);
        unsafe {
            write_volatile(
                (self.base + 0x2000 + self.context * 0x80 + lane * 4) as *mut u32,
                bit,
            );
        }
        // Set priority
        unsafe {
            write_volatile((self.base + irq as usize * 4) as *mut u32, 1);
        }
    }

    pub fn disable_irq(&self, irq: u32) {
        let lane = irq as usize / 32;
        let bit = 1 << (irq % 32);
        unsafe {
            write_volatile(
                (self.base + 0x2000 + self.context * 0x80 + lane * 4) as *mut u32,
                bit,
            );
        }
        // Set priority to 0
        unsafe {
            write_volatile((self.base + irq as usize * 4) as *mut u32, 0);
        }
    }

    pub fn claim(&self) -> u32 {
        unsafe {
            read_volatile(
                (self.base + 0x200004 + self.context * 0x1000) as *const u32,
            )
        }
    }

    pub fn complete(&self, irq: u32) {
        unsafe {
            write_volatile(
                (self.base + 0x200004 + self.context * 0x1000) as *mut u32,
                irq,
            );
        }
    }
}
