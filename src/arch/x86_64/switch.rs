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

        # Sanitize rflags: clear IOPL/TF/RF/NT/VM/AC/DF, set IF
        # Bits cleared: TF(8), DF(10), IOPL(12-13), NT(14), RF(16), VM(17), AC(18)
        # Mask: ~0x77500 = 0xFFF88AFF, OR: IF(9) = 0x200
        # Use 32-bit ops (ecx) for simpler immediate encoding
        mov ecx, [rsp + 16]         # load saved rflags (lower 32 bits)
        and ecx, 0xFFF88AFF         # clear dangerous bits
        or  ecx, 0x200              # ensure IF is set
        mov [rsp + 16], rcx         # write back sanitized rflags

        # Paranoid: clear XMM registers to prevent leakage of kernel data
        pxor xmm0, xmm0
        pxor xmm1, xmm1
        pxor xmm2, xmm2
        pxor xmm3, xmm3
        pxor xmm4, xmm4
        pxor xmm5, xmm5
        pxor xmm6, xmm6
        pxor xmm7, xmm7
        pxor xmm8, xmm8
        pxor xmm9, xmm9
        pxor xmm10, xmm10
        pxor xmm11, xmm11
        pxor xmm12, xmm12
        pxor xmm13, xmm13
        pxor xmm14, xmm14
        pxor xmm15, xmm15

        iretq

    .global jump_to_user
    jump_to_user:
        # RDI points to the TrapFrame (rip, cs, rflags, rsp, ss)
        mov rsp, rdi

        # Sanitize rflags: clear IOPL/TF/RF/NT/VM/AC/DF, set IF
        # Use 32-bit ops (ecx) for simpler immediate encoding
        mov ecx, [rsp + 16]         # load saved rflags (lower 32 bits)
        and ecx, 0xFFF88AFF         # clear dangerous bits
        or  ecx, 0x200              # ensure IF is set
        mov [rsp + 16], rcx         # write back sanitized rflags

        # Paranoid: clear general purpose registers (not in TrapFrame)
        xor rax, rax
        xor rbx, rbx
        xor rcx, rcx
        xor rdx, rdx
        xor rsi, rsi
        xor rbp, rbp
        xor r8, r8
        xor r9, r9
        xor r10, r10
        xor r11, r11
        xor r12, r12
        xor r13, r13
        xor r14, r14
        xor r15, r15
        xor rdi, rdi

        iretq

    .global syscall_entry
    syscall_entry:
        # 1. Save user RSP, load kernel stack (KERNEL_STACK is preserved)
        mov [rip + KERNEL_STACK_SAVE], rsp
        mov rsp, [rip + KERNEL_STACK]

        # 2. Push fake interrupt frame
        push 0x1B                   # ss (User DS)
        push [rip + KERNEL_STACK_SAVE]  # rsp (User RSP)
        push r11                    # rflags (User RFLAGS)
        push 0x23                   # cs (User CS)
        push rcx                    # rip (User RIP)

        # 3. Push general purpose registers in CpuState order (r15 lowest addr)
        push r15
        push r14
        push r13
        push r12
        push r11
        push r10
        push r9
        push r8
        push rdi
        push rsi
        push rbp
        push rbx
        push rdx
        push rcx
        push rax

        # 4. Call Rust handler (RDI = &mut CpuState matching struct layout)
        mov rdi, rsp
        call rust_syscall_handler

        # 5. Restore registers (reverse of push: rax first, r15 last)
        pop rax
        pop rcx
        pop rdx
        pop rbx
        pop rbp
        pop rsi
        pop rdi
        pop r8
        pop r9
        pop r10
        pop r11
        pop r12
        pop r13
        pop r14
        pop r15

        # 6. Sanitize rflags in fake frame before restoring
        # Clear: IOPL(12-13), TF(8), DF(10), NT(14), RF(16), VM(17), AC(18)
        # Mask: ~0x77500 = 0xFFF88AFF | Set: IF(9) = 0x200
        # Use 32-bit ops (ecx) for simpler immediate encoding
        mov ecx, [rsp + 16]         # load rflags from fake frame (lower 32 bits)
        and ecx, 0xFFF88AFF         # clear dangerous bits
        or  ecx, 0x200              # ensure IF is set
        mov [rsp + 16], rcx         # store sanitized rflags

        # 7. Restore user RIP and RFLAGS from fake frame
        mov rcx, [rsp]      # rip (overwrites scratch rcx)
        mov r11, [rsp + 16] # rflags (now sanitized)

        # 7. Restore user RSP (switches back to user stack)
        mov rsp, [rsp + 24] # user RSP

        # Paranoid: clear XMM registers before returning to user space
        pxor xmm0, xmm0
        pxor xmm1, xmm1
        pxor xmm2, xmm2
        pxor xmm3, xmm3
        pxor xmm4, xmm4
        pxor xmm5, xmm5
        pxor xmm6, xmm6
        pxor xmm7, xmm7
        pxor xmm8, xmm8
        pxor xmm9, xmm9
        pxor xmm10, xmm10
        pxor xmm11, xmm11
        pxor xmm12, xmm12
        pxor xmm13, xmm13
        pxor xmm14, xmm14
        pxor xmm15, xmm15

        # KERNEL_STACK still holds the kernel stack top (preserved through
        # step 1 — never overwritten). Return to Ring 3 via sysretq.
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

#[no_mangle]
pub static mut KERNEL_STACK_SAVE: u64 = 0;

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
        // Sanitize rflags before returning to Ring 3:
        // clear IOPL(12-13), TF(8), DF(10), NT(14), RF(16), VM(17), AC(18)
        // set IF(9) so the user process can receive interrupts
        current_proc.cpu_state.rflags &= 0xFFF88AFF;
        current_proc.cpu_state.rflags |= 0x200;
        *frame = current_proc.cpu_state;
    }
}
