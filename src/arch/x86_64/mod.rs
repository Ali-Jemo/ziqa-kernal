pub mod apic;
pub mod cpu_features;
pub mod gdt;
pub mod interrupts;
pub mod per_cpu;
pub mod smp;
pub mod switch;

// Re-export the IDT init from interrupts module for backward compat
pub mod idt {
    pub fn init_idt() {
        super::interrupts::init_idt();
    }
}

/// Atomically update the stacks used for Ring 3 -> Ring 0 transitions.
/// This includes both the TSS.RSP0 (for interrupts) and the KERNEL_STACK 
/// global (for syscalls).
pub fn update_trap_stacks(stack_top: u64) {
    x86_64::instructions::interrupts::without_interrupts(|| {
        gdt::set_tss_stack(x86_64::VirtAddr::new(stack_top));
        switch::set_kernel_stack(stack_top);
    });
}
