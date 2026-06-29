/// RISC-V 64 per-CPU initialization and miscellaneous operations
/// Ported from Redox OS.

use core::arch::asm;

pub fn init(cpu_id: usize) {
    // Initialize per-CPU data using tp (thread pointer) register
    let percpu = crate::percpu::PerCpu::new(cpu_id);
    let ptr = &percpu as *const _ as usize;
    unsafe {
        asm!("mv tp, {}", in(reg) ptr);
    }
    core::mem::forget(percpu);
}

pub fn current_cpu_id() -> usize {
    let tp: usize;
    unsafe { asm!("mv {}, tp", out(reg) tp) };
    unsafe { (*(tp as *const crate::percpu::PerCpu)).id }
}
