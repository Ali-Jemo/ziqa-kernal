/// RISC-V (RV64) specific architecture support for ZiqaKernel
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

pub fn update_trap_stacks(stack_top: u64) {
    // On RISC-V, we often use sscratch to store the kernel stack top
    unsafe {
        core::arch::asm!("csrw sscratch, {}", in(reg) stack_top);
    }
}

pub unsafe fn jump_to_user(trap_frame: *const TrapFrame) -> ! {
    // Restore state from TrapFrame and SRET to User Mode
    unsafe {
        core::arch::asm!(
            "
            mv sp, {tf}
            ld t0, 0(sp)    // pc
            csrw sepc, t0
            ld t0, 256(sp)  // sstatus
            csrw sstatus, t0
            
            ld ra, 8(sp)
            // skip user sp (16) until the very end
            ld gp, 24(sp)
            ld tp, 32(sp)
            ld t0, 40(sp)
            ld t1, 48(sp)
            ld t2, 56(sp)
            ld s0, 64(sp)
            ld s1, 72(sp)
            ld a0, 80(sp)
            ld a1, 88(sp)
            ld a2, 96(sp)
            ld a3, 104(sp)
            ld a4, 112(sp)
            ld a5, 120(sp)
            ld a6, 128(sp)
            ld a7, 136(sp)
            ld s2, 144(sp)
            ld s3, 152(sp)
            ld s4, 160(sp)
            ld s5, 168(sp)
            ld s6, 176(sp)
            ld s7, 184(sp)
            ld s8, 192(sp)
            ld s9, 200(sp)
            ld s10, 208(sp)
            ld s11, 216(sp)
            ld t3, 224(sp)
            ld t4, 232(sp)
            ld t5, 240(sp)
            ld t6, 248(sp)
            
            ld sp, 16(sp) // restore user sp
            
            sret
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
            slots.add(11).write(arg); // S1 -> arg
            proc.kernel_stack_ptr = sp;
        }
    }
}

pub fn current_pid() -> Option<crate::process::Pid> {
    None // TODO: implement per-cpu data for riscv64
}

pub fn set_current_pid(_pid: Option<crate::process::Pid>) {
    // TODO: implement per-cpu data for riscv64
}

pub fn interrupts_enabled() -> bool {
    let sstatus: u64;
    unsafe {
        core::arch::asm!("csrr {}, sstatus", out(reg) sstatus);
    }
    (sstatus & (1 << 1)) != 0 // SIE bit
}

pub fn enable_interrupts() {
    unsafe {
        core::arch::asm!("csrrs zero, sstatus, {}", in(reg) (1 << 1));
    }
}

pub fn disable_interrupts() {
    unsafe {
        core::arch::asm!("csrrc zero, sstatus, {}", in(reg) (1 << 1));
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
    unsafe { core::arch::asm!("ecall"); }
}

pub fn init_process_stack(proc: &mut crate::process::Process) {
    // TODO: Implement page mapping for stack on RISC-V
    if proc.kernel_stack_top != 0 {
        unsafe {
            let kstack_top = proc.kernel_stack_top;
            // Setup CpuState on kernel stack for jump_to_user_stub
            let mut sp = kstack_top;
            sp -= 8; *(sp as *mut u64) = 0x20; // sstatus (User mode + SPIE)
            sp -= 8; *(sp as *mut u64) = proc.entry_point.as_u64(); // sepc
            
            // Registers x31 down to x0 (32 registers)
            // x2 is user sp
            for i in (0..32).rev() {
                sp -= 8;
                if i == 2 {
                    *(sp as *mut u64) = proc.stack_top.as_u64();
                } else {
                    *(sp as *mut u64) = 0;
                }
            }
            
            let ret_addr_ptr = (sp - 8) as *mut u64;
            ret_addr_ptr.write(jump_to_user_stub as *const () as u64);
            
            // Context switch preserved registers (ra, s0-s11 - 13 registers)
            let mut ctx_sp = sp - 8;
            for _ in 0..14 { // ra + s0-s11 + padding
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
        mv a0, s1
        jalr s11
        li a0, 0
        j kthread_exit_trampoline
        "
    );
}

core::arch::global_asm!(
    "
    .global jump_to_user_stub
    jump_to_user_stub:
        // RSP points to CpuState
        // Restore all registers from CpuState and then SRET
        
        ld ra, (0 * 8)(sp)
        // sp is at (1 * 8), we'll restore it last
        ld gp, (2 * 8)(sp)
        ld tp, (3 * 8)(sp)
        ld t0, (4 * 8)(sp)
        ld t1, (5 * 8)(sp)
        ld t2, (6 * 8)(sp)
        ld s0, (7 * 8)(sp)
        ld s1, (8 * 8)(sp)
        ld a0, (9 * 8)(sp)
        ld a1, (10 * 8)(sp)
        ld a2, (11 * 8)(sp)
        ld a3, (12 * 8)(sp)
        ld a4, (13 * 8)(sp)
        ld a5, (14 * 8)(sp)
        ld a6, (15 * 8)(sp)
        ld a7, (16 * 8)(sp)
        ld s2, (17 * 8)(sp)
        ld s3, (18 * 8)(sp)
        ld s4, (19 * 8)(sp)
        ld s5, (20 * 8)(sp)
        ld s6, (21 * 8)(sp)
        ld s7, (22 * 8)(sp)
        ld s8, (23 * 8)(sp)
        ld s9, (24 * 8)(sp)
        ld s10, (25 * 8)(sp)
        ld s11, (26 * 8)(sp)
        ld t3, (27 * 8)(sp)
        ld t4, (28 * 8)(sp)
        ld t5, (29 * 8)(sp)
        ld t6, (30 * 8)(sp)
        
        ld t0, (31 * 8)(sp) // sepc
        csrw sepc, t0
        
        ld t0, (32 * 8)(sp) // sstatus
        csrw sstatus, t0
        
        ld sp, (1 * 8)(sp)  // Restore user SP
        
        sret
    "
);
