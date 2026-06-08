/// RISC-V (RV64) specific architecture support for ZiqaKernel
/// Based on Redox OS implementation.

pub mod interrupt;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CpuState {
    pub ra: u64,
    pub sp: u64,
    pub gp: u64,
    pub tp: u64,
    pub t0: u64,
    pub t1: u64,
    pub t2: u64,
    pub s0: u64, // FP
    pub s1: u64,
    pub a0: u64,
    pub a1: u64,
    pub a2: u64,
    pub a3: u64,
    pub a4: u64,
    pub a5: u64,
    pub a6: u64,
    pub a7: u64,
    pub s2: u64,
    pub s3: u64,
    pub s4: u64,
    pub s5: u64,
    pub s6: u64,
    pub s7: u64,
    pub s8: u64,
    pub s9: u64,
    pub s10: u64,
    pub s11: u64,
    pub t3: u64,
    pub t4: u64,
    pub t5: u64,
    pub t6: u64,
    pub sepc: u64,
    pub sstatus: u64,
}

impl CpuState {
    pub const fn zero() -> Self {
        Self {
            ra: 0, sp: 0, gp: 0, tp: 0, t0: 0, t1: 0, t2: 0, s0: 0, s1: 0,
            a0: 0, a1: 0, a2: 0, a3: 0, a4: 0, a5: 0, a6: 0, a7: 0, s2: 0,
            s3: 0, s4: 0, s5: 0, s6: 0, s7: 0, s8: 0, s9: 0, s10: 0, s11: 0,
            t3: 0, t4: 0, t5: 0, t6: 0, sepc: 0, sstatus: 0,
        }
    }
}

pub fn new_user_state(entry: crate::memory::VirtAddr, stack: crate::memory::VirtAddr) -> CpuState {
    let mut cpu = CpuState::zero();
    cpu.sepc = entry.as_u64();
    cpu.sp = stack.as_u64();
    cpu.sstatus = 0x20; // User mode (SPP=0) + SPIE
    cpu
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct TrapFrame {
    pub pc: u64,
    pub ra: u64,
    pub sp: u64,
    pub gp: u64,
    pub tp: u64,
    pub t0: u64,
    pub t1: u64,
    pub t2: u64,
    pub s0: u64,
    pub s1: u64,
    pub a0: u64,
    pub a1: u64,
    pub a2: u64,
    pub a3: u64,
    pub a4: u64,
    pub a5: u64,
    pub a6: u64,
    pub a7: u64,
    pub s2: u64,
    pub s3: u64,
    pub s4: u64,
    pub s5: u64,
    pub s6: u64,
    pub s7: u64,
    pub s8: u64,
    pub s9: u64,
    pub s10: u64,
    pub s11: u64,
    pub t3: u64,
    pub t4: u64,
    pub t5: u64,
    pub t6: u64,
    pub sstatus: u64,
}

pub fn update_trap_stacks(_stack_top: u64) {
    // RISC-V specific: update sscratch or similar if used for kernel stack
}

#[unsafe(naked)]
pub unsafe extern "C" fn switch_context(old_sp: *mut u64, new_sp: u64) {
    core::arch::naked_asm!(
        "
        addi sp, sp, -112
        sd ra, 104(sp)
        sd s0, 96(sp)
        sd s1, 88(sp)
        sd s2, 80(sp)
        sd s3, 72(sp)
        sd s4, 64(sp)
        sd s5, 56(sp)
        sd s6, 48(sp)
        sd s7, 40(sp)
        sd s8, 32(sp)
        sd s9, 24(sp)
        sd s10, 16(sp)
        sd s11, 8(sp)

        sd sp, (a0)
        mv sp, a1

        ld s11, 8(sp)
        ld s10, 16(sp)
        ld s9, 24(sp)
        ld s8, 32(sp)
        ld s7, 40(sp)
        ld s6, 48(sp)
        ld s5, 56(sp)
        ld s4, 64(sp)
        ld s3, 72(sp)
        ld s2, 80(sp)
        ld s1, 88(sp)
        ld s0, 96(sp)
        ld ra, 104(sp)
        addi sp, sp, 112
        ret
        "
    );
}

pub fn init_kthread_stack(proc: &mut crate::process::Process, entry: u64, arg: u64) {
    if let Some(ref mut _kstack) = proc.kernel_stack {
        let kstack_top = proc.kernel_stack_top;
        unsafe {
            let mut sp = kstack_top;
            sp -= 112;
            let slots = sp as *mut u64;
            slots.add(13).write(proc.entry_point.as_u64()); // RA
            slots.add(12).write(0);  // S0
            slots.add(11).write(0);  // S1
            slots.add(10).write(0);  // S2
            slots.add(9).write(0);   // S3
            slots.add(8).write(0);   // S4
            slots.add(7).write(0);   // S5
            slots.add(6).write(0);   // S6
            slots.add(5).write(0);   // S7
            slots.add(4).write(0);   // S8
            slots.add(3).write(0);   // S9
            slots.add(2).write(0);   // S10
            slots.add(1).write(entry); // S11 -> entry
            // S11 is used by trampoline to call the function
            // Arg will be in S1 -> a0
            slots.add(11).write(arg); // S1 -> arg
            proc.kernel_stack_ptr = sp;
        }
    }
}

#[unsafe(naked)]
pub unsafe extern "C" fn kthread_trampoline() -> ! {
    core::arch::naked_asm!(
        "
        mv a0, s1
        jalr s11
        li a0, 0
        j kthread_exit_trampoline
        "
    );
}
