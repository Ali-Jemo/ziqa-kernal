/// AArch64 per-CPU initialization and miscellaneous operations
/// Ported from Redox OS.

use crate::memory::paging::PhysicalAddress;

pub fn init(cpu_id: usize) {
    // Initialize per-CPU data using TPIDR_EL1
    let percpu = crate::percpu::PerCpu::new(cpu_id);
    let ptr = &percpu as *const _ as u64;
    unsafe {
        core::arch::asm!("msr tpidr_el1, {}", in(reg) ptr);
    }
    // Leak percpu to make it static
    core::mem::forget(percpu);
}

pub fn current_cpu_id() -> usize {
    let ptr: u64;
    unsafe {
        core::arch::asm!("mrs {}, tpidr_el1", out(reg) ptr);
    }
    unsafe { (*(ptr as *const crate::percpu::PerCpu)).id }
}
