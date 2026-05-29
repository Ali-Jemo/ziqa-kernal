//! Linux process, identity, signal, and scheduler syscall dispatch family.

use super::{nr, SyscallContext, AbiError};

pub(super) fn handle(ctx: &mut SyscallContext) -> Option<Result<u64, AbiError>> {
    Some(match ctx.number {
        nr::SYS_EXIT | nr::SYS_EXIT_GROUP => super::sys_exit(ctx),
        nr::SYS_GETPID => super::sys_getpid(ctx),
        nr::SYS_KILL => super::sys_kill(ctx),
        nr::SYS_WAITPID => super::sys_waitpid(ctx),
        nr::SYS_ARCH_PRCTL => super::sys_arch_prctl(ctx),
        nr::SYS_SET_TID_ADDRESS => super::sys_set_tid_address(ctx),
        nr::SYS_GETUID | nr::SYS_GETGID | nr::SYS_GETEUID | nr::SYS_GETEGID => Ok(0),
        nr::SYS_FUTEX => super::sys_futex(ctx),
        nr::SYS_RT_SIGACTION => super::sys_rt_sigaction(ctx),
        nr::SYS_RT_SIGPROCMASK => Ok(0),
        nr::SYS_CLONE => super::sys_clone(ctx),
        nr::SYS_GETPPID => Ok(ctx.process.parent),
        nr::SYS_GETTID => Ok(ctx.process.pid.0),
        nr::SYS_TGKILL => super::sys_tgkill(ctx),
        nr::SYS_SETSID => Ok(ctx.process.pid.0),
        nr::SYS_SETPGID => Ok(0),
        nr::SYS_GETPGID | nr::SYS_GETSID => Ok(ctx.process.pid.0),
        nr::SYS_GETPRIORITY => super::sys_getpriority(ctx),
        nr::SYS_SETPRIORITY => super::sys_setpriority(ctx),
        nr::SYS_NICE => Ok(0),
        nr::SYS_SCHED_GETPARAM => super::sys_sched_getparam(ctx),
        nr::SYS_SCHED_SETPARAM => Ok(0),
        nr::SYS_SCHED_GETSCHEDULER => Ok(0),
        nr::SYS_SCHED_SETSCHEDULER => Ok(0),
        nr::SYS_SCHED_GET_PRIORITY_MAX => Ok(0),
        nr::SYS_SCHED_GET_PRIORITY_MIN => Ok(0),
        nr::SYS_SCHED_RR_GET_INTERVAL => Ok(0),
        nr::SYS_PRCTL => super::sys_prctl(ctx),
        nr::SYS_PIDFD_OPEN => super::sys_pidfd_open(ctx),
        _ => return None,
    })
}
