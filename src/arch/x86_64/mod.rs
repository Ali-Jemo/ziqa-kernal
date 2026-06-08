pub mod apic;
pub mod cpu_features;
pub mod gdt;
pub mod interrupts;
pub mod per_cpu;
pub mod smp;
pub mod switch;

pub use switch::TrapFrame;
pub use per_cpu::{current_pid, set_current_pid};

// Re-export the IDT init from interrupts module for backward compat
pub mod idt {
    pub fn init_idt() {
        super::interrupts::init_idt();
    }
}

#[repr(C)]
#[repr(align(16))]
#[derive(Debug, Clone, Copy)]
pub struct FpuState {
    pub data: [u8; 512],
}

impl FpuState {
    pub fn new() -> Self {
        Self { data: [0u8; 512] }
    }
}

pub unsafe fn save_fpu(state: *mut FpuState) {
    core::arch::asm!("fxsave [{}]", in(reg) state);
}

pub unsafe fn restore_fpu(state: *const FpuState) {
    core::arch::asm!("fxrstor [{}]", in(reg) state);
}

pub struct CpuState {
    // Pushed by assembly stub (pop order)
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub r11: u64,
    pub r10: u64,
    pub r9: u64,
    pub r8: u64,
    pub rdi: u64,
    pub rsi: u64,
    pub rbp: u64,
    pub rbx: u64,
    pub rdx: u64,
    pub rcx: u64,
    pub rax: u64,

    // Pushed automatically by CPU on interrupt
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u64,
}

impl CpuState {
    pub const fn zero() -> Self {
        Self {
            r15: 0,
            r14: 0,
            r13: 0,
            r12: 0,
            r11: 0,
            r10: 0,
            r9: 0,
            r8: 0,
            rdi: 0,
            rsi: 0,
            rbp: 0,
            rbx: 0,
            rdx: 0,
            rcx: 0,
            rax: 0,
            rip: 0,
            rflags: 0x202,
            cs: 8,
            ss: 0,
            rsp: 0,
        }
    }
}

pub fn new_user_state(entry: crate::memory::VirtAddr, stack: crate::memory::VirtAddr) -> CpuState {
    let mut cpu = CpuState::zero();
    cpu.rip = entry.as_u64();
    cpu.rsp = stack.as_u64();
    cpu.cs = gdt::user_code_selector().0 as u64;
    cpu.ss = gdt::user_data_selector().0 as u64;
    cpu
}

pub fn init_kthread_stack(proc: &mut crate::process::Process, entry: u64, arg: u64) {
    if let Some(ref mut _kstack) = proc.kernel_stack {
        let kstack_top = proc.kernel_stack_top;
        unsafe {
            // Layout for the first `switch_context` into this kthread.
            // `switch_context` saves 6 callee-saved regs in the order
            //   push rbp, rbx, r12, r13, r14, r15
            // and pops them in reverse: r15, r14, r13, r12, rbx, rbp.
            let slots = (kstack_top - 56) as *mut u64;
            slots.add(0).write(0);                  // rbp
            slots.add(1).write(0);                  // rbx
            slots.add(2).write(arg);                // r12 -> arg
            slots.add(3).write(entry);              // r13 -> entry
            slots.add(4).write(0);                  // r14
            slots.add(5).write(0);                  // r15
            // Return address for the kthread's first `switch_context` ret.
            (kstack_top as *mut u64).sub(1).write(proc.entry_point.as_u64());
            proc.kernel_stack_ptr = kstack_top - 56;
        }
    }
}

#[unsafe(naked)]
pub unsafe extern "C" fn kthread_trampoline() -> ! {
    core::arch::naked_asm!(
        "
        mov rdi, r12
        call r13
        xor edi, edi
        jmp kthread_exit_trampoline
        "
    );
}

/// Alias for kthread_trampoline — used by scheduler to get its address
pub unsafe extern "C" fn kthread_trampoline_wrapper() -> ! {
    unsafe { kthread_trampoline() }
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
