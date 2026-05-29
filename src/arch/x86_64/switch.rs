// src/arch/x86_64/switch.rs

use core::arch::global_asm;

global_asm!(
    r#"
    .global switch_context
    switch_context:
        # Save caller-saved (callee-saved in System V)
        push rbp
        push rbx
        push r12
        push r13
        push r14
        push r15

        # Save current RSP to the location pointed to by RDI
        mov [rdi], rsp

        # Load new RSP from RSI
        mov rsp, rsi

        # Restore registers from the new stack
        pop r15
        pop r14
        pop r13
        pop r12
        pop rbx
        pop rbp

        # Return to the new context (pops RIP)
        ret

    .global jump_to_user_stub
    jump_to_user_stub:
        # RSP points to CpuState. We pop r15-rax (15 registers)
        pop r15
        pop r14
        pop r13
        pop r12
        pop r11
        pop r10
        pop r9
        pop r8
        pop rdi
        pop rsi
        pop rbp
        pop rbx
        pop rdx
        pop rcx
        pop rax
        iretq

    .global jump_to_user
    jump_to_user:
        # RDI points to the TrapFrame
        mov rsp, rdi
        iretq

    .global syscall_entry
    syscall_entry:
        # 1. Swap RSP with KERNEL_STACK
        xchg rsp, [rip + KERNEL_STACK]

        # 2. Push fake interrupt frame
        push 0x1B                   # ss (User DS)
        push qword ptr [rip + KERNEL_STACK] # rsp (User RSP)
        push r11                    # rflags (User RFLAGS)
        push 0x23                   # cs (User CS)
        push rcx                    # rip (User RIP)

        # 3. Push general purpose registers in reverse order of CpuState
        push rax
        push rcx
        push rdx
        push rbx
        push rbp
        push rsi
        push rdi
        push r8
        push r9
        push r10
        push r11
        push r12
        push r13
        push r14
        push r15

        # 4. Call Rust handler
        mov rdi, rsp
        call rust_syscall_handler

        # 5. Restore registers
        pop r15
        pop r14
        pop r13
        pop r12
        pop r11
        pop r10
        pop r9
        pop r8
        pop rdi
        pop rsi
        pop rbp
        pop rbx
        pop rdx
        pop rcx
        pop rax

        # 6. Restore user RIP and RFLAGS
        mov rcx, [rsp]
        mov r11, [rsp + 16]

        # 7. Restore user RSP to KERNEL_STACK
        mov rdi, [rsp + 24]
        mov [rip + KERNEL_STACK], rdi

        # 8. Clean up fake interrupt frame
        add rsp, 40

        # 9. Swap back to user stack
        mov rsp, [rip + KERNEL_STACK]
        sysretq
    "#
);

extern "C" {
    /// Save current RSP to `old_rsp`, then switch to `new_rsp`.
    pub fn switch_context(old_rsp: *mut u64, new_rsp: u64);
    /// Jump to user-mode by restoring TrapFrame and executing iretq.
    pub fn jump_to_user(trap_frame: *const TrapFrame) -> !;
    /// Entry point for jumping to user mode via context switch return path.
    pub fn jump_to_user_stub();
}

// Dummy reference for graph analysis
#[allow(unused_imports)]
use crate::process::scheduler as _ref_to_scheduler;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct TrapFrame {
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u64,
}

#[no_mangle]
pub static mut KERNEL_STACK: u64 = 0;

pub fn set_kernel_stack(stack: u64) {
    unsafe {
        KERNEL_STACK = stack;
    }
}

pub fn init_syscalls() {
    use x86_64::registers::model_specific::{Efer, EferFlags, Msr};

    // 1. Enable syscall/sysret in EFER MSR
    unsafe {
        Efer::write(Efer::read() | EferFlags::SYSTEM_CALL_EXTENSIONS);
    }

    // 2. Configure STAR MSR (0xC0000081)
    // STAR[47:32] = Kernel CS (0x08)
    // STAR[63:48] = User CS Base (0x10) -> User SS = 0x1B, User CS = 0x23
    let mut star = Msr::new(0xC0000081);
    let star_val = ((0x10 as u64) << 48) | ((0x08 as u64) << 32);
    unsafe {
        star.write(star_val);
    }

    // 3. Configure LSTAR MSR (0xC0000082) with syscall_entry
    extern "C" {
        fn syscall_entry();
    }
    let mut lstar = Msr::new(0xC0000082);
    unsafe {
        lstar.write(syscall_entry as *const () as u64);
    }

    // 4. Configure SFMASK MSR (0xC0000084) to mask flags (IF = 0x200, TF = 0x100, etc.)
    let mut sfmask = Msr::new(0xC0000084);
    unsafe {
        sfmask.write(0x25700); // Mask IF, TF, DF, IOPL, AC
    }
}

#[no_mangle]
pub unsafe extern "C" fn rust_syscall_handler(frame: &mut crate::process::CpuState) {
    let num = frame.rax;
    let args = [
        frame.rdi,
        frame.rsi,
        frame.rdx,
        frame.r10,
        frame.r8,
        frame.r9,
    ];

    let registry = crate::init_abi_registry();
    let mut scheduler = crate::process::scheduler::SCHEDULER.lock();
    if let Some(proc) = scheduler.current_task_mut() {
        proc.cpu_state = *frame;

        let mut ctx = crate::abi::syscall::SyscallContext::new(num, args, proc);
        let handler = crate::abi::handler::KernelSyscallHandler;

        let res = crate::abi::syscall::dispatch_syscall(&registry, &handler, &mut ctx);

        let current_proc = scheduler.current_task_mut().unwrap();
        match res {
            Ok(v) => {
                current_proc.cpu_state.rax = v;
            }
            Err(e) => {
                let errno_val = match e {
                    crate::abi::AbiError::UnsupportedSyscall(_) => 38, // ENOSYS
                    crate::abi::AbiError::PermissionDenied => 13, // EACCES
                    _ => 1, // EPERM or default error
                };
                current_proc.cpu_state.rax = -(errno_val as i64) as u64;
            }
        }
        *frame = current_proc.cpu_state;
    }
}
