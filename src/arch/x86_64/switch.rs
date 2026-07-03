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
        # RSP points to CpuState. Pop fields in #[repr(C)] order.
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

    .global int80_entry
    int80_entry:
        # CPU already switched to kernel stack via TSS.RSP0 and pushed
        # SS, RSP (user), RFLAGS, CS, RIP onto it.
        #
        # At this point RSP points to the interrupt frame:
        #   [RSP+0]  = RIP
        #   [RSP+8]  = CS
        #   [RSP+16] = RFLAGS
        #   [RSP+24] = RSP (user)
        #   [RSP+32] = SS
        #
        # We need to push GPRs in CpuState order (below the interrupt frame),
        # so that RSP points to r15 at the top of a full CpuState.

        # Make room for CpuState registers between current RSP and the int frame.
        # Push GPRs below the existing interrupt frame.
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

        # Now RSP points to saved_rax — which is the CpuState.rax field.
        # The interrupt frame is 40 bytes higher up:
        #   rax rcx rdx rbx rbp rsi rdi r8 r9 r10 r11 r12 r13 r14 r15 [gap] rip cs rflags rsp ss
        # This matches CpuState layout:
        #   struct CpuState {{
        #       r15..rax (15 regs = 120 bytes),
        #       rip, cs, rflags, rsp, ss (5 regs = 40 bytes)
        #   }}
        # But wait — we pushed in reverse order (rax last), which matches CpuState
        # where rax is at the highest offset within the GPR block:
        #   r15 (lowest addr), r14, ..., rax (highest addr)
        # Then the interrupt frame continues: rip, cs, rflags, rsp, ss.

        # Call Rust handler (RDI = &mut CpuState)
        mov rdi, rsp
        call rust_syscall_handler

        # Restore registers (reverse push order: rax first, r15 last)
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

        # Now RSP points to the original interrupt frame: rip, cs, rflags, rsp, ss
        # Sanitize rflags at [RSP+16]
        mov ecx, [rsp + 16]
        and ecx, 0xFFF88AFF
        or  ecx, 0x200
        mov [rsp + 16], rcx

        # Clear XMM registers
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

        # Return via iretq (pops rip, cs, rflags, rsp, ss)
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

unsafe extern "C" {
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

#[unsafe(no_mangle)]
pub static mut KERNEL_STACK: u64 = 0;

#[unsafe(no_mangle)]
pub static mut KERNEL_STACK_SAVE: u64 = 0;

/// Bounded debug-log budget for PID 2 (the embedded Orbital compositor).
/// Orbital issues a high volume of syscalls in its ~16 ms render loop; logging
/// all of them floods the serial console. After this many non-essential lines,
/// the debug prints stop — errors are still always logged by the caller's
/// `else if res.is_err()` branch.
const PID2_DEBUG_CAP: u64 = 30;
static PID2_DEBUG_COUNT: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

fn should_log_syscall(pid: u64, number: u64) -> bool {
    if pid != 2 {
        return true;
    }

    // Hot render-loop syscalls (read / clock / nanosleep) are never logged.
    if matches!(number, 162 | 265 | 570_425_347) {
        return false;
    }

    // Every other PID 2 syscall counts against the bounded budget above so the
    // console shows Orbital's startup (display fmap, input open, …) and then
    // goes quiet instead of streaming forever.
    let prev = PID2_DEBUG_COUNT.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    prev < PID2_DEBUG_CAP
}

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
    unsafe extern "C" {
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

#[unsafe(no_mangle)]
pub unsafe extern "C" fn rust_syscall_handler(frame: &mut crate::process::CpuState) { unsafe {
    let num = frame.rax;
    let args = [
        frame.rdi,
        frame.rsi,
        frame.rdx,
        frame.r10,
        frame.r8,
        frame.r9,
    ];
    if num == 158 && args[0] == u64::MAX && args[1] == u64::MAX && args[2] == u64::MAX {
        frame.rax = 0;
        return;
    }
    let pid = match crate::process::scheduler::SCHEDULER.current_pid() {
        Some(pid) => pid,
        None => return,
    };
    let cpu = crate::arch::x86_64::per_cpu::current_cpu();
    let raw = cpu
        .current_process_raw
        .load(core::sync::atomic::Ordering::Relaxed) as *mut crate::process::Process;
    let proc_ptr = if !raw.is_null() && (*raw).pid == pid {
        (*raw).cpu_state = *frame;
        raw
    } else {
        let proc_arc = match crate::process::scheduler::SCHEDULER.get_process(pid) {
            Some(proc_arc) => proc_arc,
            None => return,
        };
        let mut proc = proc_arc.lock();
        proc.cpu_state = *frame;
        &mut *proc as *mut crate::process::Process
    };
    cpu.current_process_raw.store(proc_ptr as u64, core::sync::atomic::Ordering::Relaxed);

    let registry = crate::init_abi_registry();
    let handler = crate::abi::handler::KernelSyscallHandler;
    let (cpu_state, is_exited) = {
        let proc = &mut *proc_ptr;
        let pid_val = proc.pid.0;
        let mut ctx = crate::abi::syscall::SyscallContext::new(num, args, proc);
        let log_syscall = should_log_syscall(pid_val, num);
        if log_syscall {
            crate::println!("[Syscall Debug] PID {}: number={} args={:x?}", pid_val, num, args);
        }
        let res = crate::abi::syscall::dispatch_syscall(&registry, &handler, &mut ctx);
        if log_syscall {
            crate::println!("[Syscall Debug] PID {}: res={:?}", pid_val, res);
        } else if res.is_err() {
            crate::println!("[Syscall Debug] PID {}: number={} res={:?}", pid_val, num, res);
        }

        match res {
            Ok(v) => {
                proc.cpu_state.rax = v;
            }
            Err(e) => {
                let errno_val = match e {
                    crate::abi::AbiError::UnsupportedSyscall(_) => 38, // ENOSYS
                    crate::abi::AbiError::PermissionDenied => 13, // EACCES
                    _ => 1, // EPERM or default error
                };
                proc.cpu_state.rax = -(errno_val as i64) as u64;
            }
        }

        // Sanitize rflags before returning to Ring 3:
        proc.cpu_state.rflags &= 0xFFF88AFF;
        proc.cpu_state.rflags |= 0x200;

        (proc.cpu_state, matches!(proc.state, crate::process::ProcessState::Exited(_)))
    };

    *frame = cpu_state;

    if is_exited {
        // Schedule another process instead of returning to user mode.
        crate::process::scheduler::SCHEDULER.schedule();
        // If schedule returns, we have no more work to do — loop halt.
        loop { x86_64::instructions::interrupts::enable_and_hlt(); }
    }
}}
