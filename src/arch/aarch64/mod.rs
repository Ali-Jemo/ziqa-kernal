///
/// ARM64 (AArch64) specific architecture support for ZiqaKernel
/// Based on Redox OS implementation.
///
pub mod consts;
pub mod device;
pub mod interrupt;
pub mod ipi;
pub mod misc;
pub mod paging;
pub mod stop;
pub mod time;
pub mod vectors;
pub mod start;
pub use ::rmm::aarch64::AArch64Arch as CurrentRmmArch;
pub use arch_copy_to_user as arch_copy_from_user;
#[unsafe(naked)]
pub unsafe extern "C" fn arch_copy_to_user(dst: usize, src: usize, len: usize) -> u8 {
    // x0, x1, x2
    core::arch::naked_asm!(
        "
    .global __usercopy_start
    __usercopy_start:
        mov x4, x0
        mov x0, 0
    2:
        cmp x2, 0
        b.eq 3f
        ldrb w3, [x1]
        strb w3, [x4]
        add x4, x4, 1
        add x1, x1, 1
        sub x2, x2, 1
        b 2b
    3:
        ret
    .global __usercopy_end
    __usercopy_end:
    "
    );
}
pub const KFX_SIZE: usize = 1024;
/// This function exists as the KFX size is dynamic on x86_64.
pub fn kfx_size() -> usize {
    KFX_SIZE
}
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

pub fn update_trap_stacks(stack_top: u64) {
    // On AArch64, we might want to store the kernel stack top in a per-cpu register
    // or use it during exception entry.
    unsafe {
        core::arch::asm!("msr tpidr_el1, {}", in(reg) stack_top);
    }
}

pub unsafe fn jump_to_user(trap_frame: *const TrapFrame) -> ! {
    // Restore state from TrapFrame and ERET to EL0
    unsafe {
        core::arch::asm!(
            "
            mov sp, {tf}
            ldp x0, x1, [sp], #16
            ldp x2, x3, [sp], #16
            ldp x4, x5, [sp], #16
            ldp x6, x7, [sp], #16
            ldp x8, x9, [sp], #16
            ldp x10, x11, [sp], #16
            ldp x12, x13, [sp], #16
            ldp x14, x15, [sp], #16
            ldp x16, x17, [sp], #16
            ldp x18, x19, [sp], #16
            ldp x20, x21, [sp], #16
            ldp x22, x23, [sp], #16
            ldp x24, x25, [sp], #16
            ldp x26, x27, [sp], #16
            ldp x28, x29, [sp], #16
            ldr x30, [sp], #8
            
            ldr x1, [sp], #8 // elr_el1
            msr elr_el1, x1
            ldr x1, [sp], #8 // spsr_el1
            msr spsr_el1, x1
            ldr x1, [sp], #8 // sp_el0
            msr sp_el0, x1
            
            eret
            ",
            tf = in(reg) trap_frame,
            options(noreturn)
        );
    }
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

pub fn current_pid() -> Option<crate::process::Pid> {
    None // TODO: implement per-cpu data for aarch64
}

pub fn set_current_pid(_pid: Option<crate::process::Pid>) {
    // TODO: implement per-cpu data for aarch64
}

pub fn interrupts_enabled() -> bool {
    let daif: u64;
    unsafe {
        core::arch::asm!("mrs {}, daif", out(reg) daif);
    }
    (daif & (1 << 7)) == 0 // I bit (IRQ mask)
}

pub fn enable_interrupts() {
    unsafe {
        core::arch::asm!("msr daifclr, #2");
    }
}

pub fn disable_interrupts() {
    unsafe {
        core::arch::asm!("msr daifset, #2");
    }
}

pub fn without_interrupts<F, R>(f: F) -> R
where
    F: FnOnce() -> R,
{
    let enabled = interrupts_enabled();
    if enabled {
        disable_interrupts();
    }
    let ret = f();
    if enabled {
        enable_interrupts();
    }
    ret
}

pub fn yield_now() {
    unsafe { core::arch::asm!("svc #0x20"); }
}

pub fn init_process_stack(proc: &mut crate::process::Process) {
    // TODO: Implement page mapping for stack on AArch64
    if proc.kernel_stack_top != 0 {
        unsafe {
            let kstack_top = proc.kernel_stack_top;
            // Setup CpuState on kernel stack for jump_to_user_stub
            let mut sp = kstack_top;
            sp -= 8; *(sp as *mut u64) = 0; // tpidr_el0
            sp -= 8; *(sp as *mut u64) = 0x05; // spsr_el1 (EL1h) - dummy user state should be 0
            sp -= 8; *(sp as *mut u64) = proc.entry_point.as_u64(); // elr_el1
            sp -= 8; *(sp as *mut u64) = proc.stack_top.as_u64(); // sp_el0
            
            // Registers x30 down to x19 (12 registers)
            for _ in 0..12 {
                sp -= 8; *(sp as *mut u64) = 0;
            }
            
            let ret_addr_ptr = (sp - 8) as *mut u64;
            ret_addr_ptr.write(jump_to_user_stub as *const () as u64);
            
            // Context switch preserved registers (x30 down to x19 - 12 registers)
            let mut ctx_sp = sp - 8;
            for _ in 0..12 {
                ctx_sp -= 8; *(ctx_sp as *mut u64) = 0;
            }
            proc.kernel_stack_ptr = ctx_sp;
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

core::arch::global_asm!(
    "
    .global jump_to_user_stub
    jump_to_user_stub:
        // RSP points to CpuState
        // We need to restore registers and then ERET
        // CpuState: x19-x30, sp, elr_el1, spsr_el1, tpidr_el0
        
        ldp x19, x20, [sp], #16
        ldp x21, x22, [sp], #16
        ldp x23, x24, [sp], #16
        ldp x25, x26, [sp], #16
        ldp x27, x28, [sp], #16
        ldp x29, x30, [sp], #16
        
        ldr x0, [sp], #8  // sp
        mov sp, x0
        
        ldr x0, [sp], #8  // elr_el1
        msr elr_el1, x0
        
        ldr x0, [sp], #8  // spsr_el1
        msr spsr_el1, x0
        
        ldr x0, [sp], #8  // tpidr_el0
        msr tpidr_el0, x0
        
        // Zero out scratch registers to prevent leakage
        mov x0,  #0
        mov x1,  #0
        mov x2,  #0
        mov x3,  #0
        mov x4,  #0
        mov x5,  #0
        mov x6,  #0
        mov x7,  #0
        mov x8,  #0
        mov x9,  #0
        mov x10, #0
        mov x11, #0
        mov x12, #0
        mov x13, #0
        mov x14, #0
        mov x15, #0
        mov x16, #0
        mov x17, #0
        mov x18, #0
        
        eret
    "
);
