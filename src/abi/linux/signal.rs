use crate::abi::syscall::SyscallContext;
use crate::abi::{AbiError};
use crate::process::signal::{SignalFrame};


pub fn handle(ctx: &mut SyscallContext) -> Option<Result<u64, AbiError>> {
    match ctx.number {
        super::nr::SYS_RT_SIGRETURN => Some(sys_rt_sigreturn(ctx)),
        super::nr::SYS_RT_SIGACTION => Some(sys_rt_sigaction(ctx)),
        _ => None,
    }
}

/// sys_rt_sigaction(signum, act_ptr, oldact_ptr, sigsetsize)
///
/// The Linux `sigaction` layout (simplified for ZiqaKernel) is:
///   offset 0:  sa_handler  (u64) – pointer or SIG_DFL=0 / SIG_IGN=1
///   offset 8:  sa_flags    (u64)
///   offset 16: sa_restorer (u64) – ignored; kernel installs its own trampoline
///   offset 24: sa_mask     (u64) – first 64-bit word of the signal set
///
/// This implementation reads the full 32‑byte struct from user memory,
/// updates the process's `SignalState`, and optionally writes the previous
/// action back to `oldact_ptr`.
fn sys_rt_sigaction(ctx: &mut SyscallContext) -> Result<u64, AbiError> {
    use crate::process::signal::SignalAction;
    use crate::memory::{copy_from_user, copy_to_user};

    let signum = ctx.args[0] as u8;
    let act_ptr = ctx.args[1] as u64;
    let oldact_ptr = ctx.args[2] as u64;
    // let sigsetsize = ctx.args[3]; // ignored – we assume the 32‑byte layout

    // Validate signum
    if signum == 0 || signum > crate::process::signal::sig::MAX {
        return Ok(-(22_i64) as u64); // -EINVAL
    }

    // If the caller wants the old action, write it back first.
    if oldact_ptr != 0 {
        let current = ctx.process.signals.get_action(signum);
        let old_handler = match current {
            SignalAction::Default => 0u64,
            SignalAction::Ignore => 1u64,
            SignalAction::Handler(addr) => addr,
        };
        // Serialize a minimal sigaction: handler + zeroed flags/restorer/mask
        let mut out = [0u8; 32];
        out[0..8].copy_from_slice(&old_handler.to_ne_bytes());
        // flags, restorer, mask are left zeroed.
        copy_to_user(oldact_ptr, &out).map_err(|_| AbiError::Other("EFAULT"))?;
    }

    // If act_ptr is null, the caller only wanted to retrieve the old action.
    if act_ptr == 0 {
        return Ok(0);
    }

    // Read the full sigaction struct from user space.
    let mut buf = [0u8; 32];
    copy_from_user(&mut buf, act_ptr).map_err(|_| AbiError::Other("EFAULT"))?;
    let handler = u64::from_ne_bytes(buf[0..8].try_into().unwrap());
    // let flags = u64::from_ne_bytes(buf[8..16].try_into().unwrap()); // currently unused
    // let _restorer = u64::from_ne_bytes(buf[16..24].try_into().unwrap()); // ignored
    // let _mask = u64::from_ne_bytes(buf[24..32].try_into().unwrap()); // ignored for now

    let action = match handler {
        0 => SignalAction::Default,
        1 => SignalAction::Ignore,
        addr => SignalAction::Handler(addr),
    };

    if !ctx.process.signals.set_action(signum, action) {
        return Ok(-(22_i64) as u64); // -EINVAL
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
