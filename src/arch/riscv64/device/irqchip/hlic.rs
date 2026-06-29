/// RISC-V Hart-Level Interrupt Controller (HLIC)
/// Ported from Redox OS.

pub struct Hlic;

impl Hlic {
    pub fn new() -> Self {
        Self
    }

    pub fn init(&self) {
        // Enable all S-mode interrupts in sie
        unsafe {
            core::arch::asm!("csrs sie, {}", in(reg) 0xFFFFu64);
        }
    }

    pub fn is_interrupt_pending(&self) -> bool {
        let sip: u64;
        unsafe { core::arch::asm!("csrr {}, sip", out(reg) sip) };
        sip != 0
    }
}
