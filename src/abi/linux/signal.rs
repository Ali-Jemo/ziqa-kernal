use crate::abi::syscall::SyscallContext;
use crate::abi::{AbiError};
use crate::process::signal::{SignalFrame};

use crate::process::signal::SignalAction;

pub fn handle(ctx: &mut SyscallContext) -> Option<Result<u64, AbiError>> {
    match ctx.number {
        super::nr::SYS_RT_SIGRETURN => Some(sys_rt_sigreturn(ctx)),
        super::nr::SYS_RT_SIGACTION => Some(sys_rt_sigaction(ctx)),
        _ => None,
    }
}

/// sys_rt_sigaction(signum, act, oldact, sigsetsize)
fn sys_rt_sigaction(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    let signum = ctx.args[0] as u8;
    let act_ptr = ctx.args[1] as u64;

    if act_ptr != 0 {
        // Read the struct sigaction from user memory
        // Simplified: assumes 16 bytes for sigaction (struct sigaction { void (*handler)(int); unsigned long sa_mask; ... })
        let mut sa_handler = 0u64;
        crate::memory::copy_from_user(
            unsafe { core::slice::from_raw_parts_mut(&mut sa_handler as *mut _ as *mut u8, 8) },
            act_ptr,
        ).map_err(|_| AbiError::Other("EFAULT"))?;
        
        if !ctx.process.signals.set_action(signum, SignalAction::Handler(sa_handler)) {
            return Ok(-(22_i64) as u64); // -EINVAL
        }
    }

    Ok(0)
}

/// sys_rt_sigreturn() → never returns to handler
fn sys_rt_sigreturn(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    // 1. Locate the SignalFrame on the user stack
    // In Linux ABI, it's just below RSP when the signal handler was called.
    let rsp = ctx.process.cpu_state.rsp;
    let frame_ptr = rsp as *const SignalFrame;
    
    // 2. Read the SignalFrame
    let mut frame = SignalFrame::new(0, &crate::process::CpuState::zero());
    if crate::memory::copy_from_user(
        unsafe { core::slice::from_raw_parts_mut(&mut frame as *mut _ as *mut u8, core::mem::size_of::<SignalFrame>()) },
        frame_ptr as u64,
    ).is_err() {
        return Ok(-(14_i64) as u64); // -EFAULT
    }
    
    // 3. Restore process CPU state
    ctx.process.cpu_state = frame.cpu_state;
    
    crate::println!("[Linux ABI] sigreturn: restoring context for PID {}", ctx.process.pid.0);
    
    // 4. Return the result (the handler is no longer running)
    Ok(0)
}
