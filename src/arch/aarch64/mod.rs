/// ARM64 (AArch64) specific architecture support for ZiqaKernel
/// Based on Redox OS implementation.

pub mod consts;
pub mod device;
pub mod interrupt;
pub mod ipi;
pub mod misc;
pub mod paging;
pub mod stop;
pub mod time;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CpuState {
    pub x19: u64,
    pub x20: u64,
    pub x21: u64,
    pub x22: u64,
    pub x23: u64,
    pub x24: u64,
    pub x25: u64,
    pub x26: u64,
    pub x27: u64,
    pub x28: u64,
    pub x29: u64, // FP
    pub x30: u64, // LR
    pub sp: u64,
    pub elr_el1: u64,
    pub spsr_el1: u64,
    pub tpidr_el0: u64,
}

impl CpuState {
    pub const fn zero() -> Self {
        Self {
            x19: 0, x20: 0, x21: 0, x22: 0, x23: 0, x24: 0, x25: 0, x26: 0,
            x27: 0, x28: 0, x29: 0, x30: 0, sp: 0, elr_el1: 0, spsr_el1: 0,
            tpidr_el0: 0,
        }
    }
}

pub fn new_user_state(entry: crate::memory::VirtAddr, stack: crate::memory::VirtAddr) -> CpuState {
    let mut cpu = CpuState::zero();
    cpu.elr_el1 = entry.as_u64();
    cpu.sp = stack.as_u64();
    cpu.spsr_el1 = 0; // User mode (EL0)
    cpu
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct TrapFrame {
    pub x0: u64,
    pub x1: u64,
    pub x2: u64,
    pub x3: u64,
    pub x4: u64,
    pub x5: u64,
    pub x6: u64,
    pub x7: u64,
    pub x8: u64,
    pub x9: u64,
    pub x10: u64,
    pub x11: u64,
    pub x12: u64,
    pub x13: u64,
    pub x14: u64,
    pub x15: u64,
    pub x16: u64,
    pub x17: u64,
    pub x18: u64,
    pub x19: u64,
    pub x20: u64,
    pub x21: u64,
    pub x22: u64,
    pub x23: u64,
    pub x24: u64,
    pub x25: u64,
    pub x26: u64,
    pub x27: u64,
    pub x28: u64,
    pub x29: u64,
    pub x30: u64,
    pub elr_el1: u64,
    pub spsr_el1: u64,
    pub sp_el0: u64,
}

pub fn update_trap_stacks(_stack_top: u64) {
    // ARM64 specific: update TPIDR_EL1 or similar if used for per-cpu data
}

#[unsafe(naked)]
pub unsafe extern "C" fn switch_context(old_sp: *mut u64, new_sp: u64) {
    core::arch::naked_asm!(
        "
        str x19, [sp, #-8]!
        str x20, [sp, #-8]!
        str x21, [sp, #-8]!
        str x22, [sp, #-8]!
        str x23, [sp, #-8]!
        str x24, [sp, #-8]!
        str x25, [sp, #-8]!
        str x26, [sp, #-8]!
        str x27, [sp, #-8]!
        str x28, [sp, #-8]!
        str x29, [sp, #-8]!
        str x30, [sp, #-8]!

        mov x2, sp
        str x2, [x0]
        mov sp, x1

        ldr x30, [sp], #8
        ldr x29, [sp], #8
        ldr x28, [sp], #8
        ldr x27, [sp], #8
        ldr x26, [sp], #8
        ldr x25, [sp], #8
        ldr x24, [sp], #8
        ldr x23, [sp], #8
        ldr x22, [sp], #8
        ldr x21, [sp], #8
        ldr x20, [sp], #8
        ldr x19, [sp], #8
        ret
        "
    );
}

pub fn init_kthread_stack(proc: &mut crate::process::Process, entry: u64, arg: u64) {
    if let Some(ref mut _kstack) = proc.kernel_stack {
        let kstack_top = proc.kernel_stack_top;
        unsafe {
            let mut sp = kstack_top;
            sp -= 8; *(sp as *mut u64) = proc.entry_point.as_u64(); // LR (X30)
            sp -= 8; *(sp as *mut u64) = 0;   // X29 (FP)
            sp -= 8; *(sp as *mut u64) = 0;   // X28
            sp -= 8; *(sp as *mut u64) = 0;   // X27
            sp -= 8; *(sp as *mut u64) = 0;   // X26
            sp -= 8; *(sp as *mut u64) = 0;   // X25
            sp -= 8; *(sp as *mut u64) = 0;   // X24
            sp -= 8; *(sp as *mut u64) = 0;   // X23
            sp -= 8; *(sp as *mut u64) = 0;   // X22
            sp -= 8; *(sp as *mut u64) = 0;   // X21
            sp -= 8; *(sp as *mut u64) = entry; // X20 -> entry
            sp -= 8; *(sp as *mut u64) = arg;   // X19 -> arg
            proc.kernel_stack_ptr = sp;
        }
    }
}

#[unsafe(naked)]
pub unsafe extern "C" fn kthread_trampoline() -> ! {
    core::arch::naked_asm!(
        "
        mov x0, x19
        blr x20
        mov x0, #0
        b kthread_exit_trampoline
        "
    );
}
