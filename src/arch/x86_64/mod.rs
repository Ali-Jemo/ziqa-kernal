pub mod gdt;
pub mod interrupts;
pub mod switch;

// Re-export the IDT init from interrupts module for backward compat
pub mod idt {
    pub fn init_idt() {
        super::interrupts::init_idt();
    }
}
