//! Linux time and timer syscall dispatch family.

use super::{nr, SyscallContext, AbiError};

pub(crate) fn handle(ctx: &mut SyscallContext) -> Option<Result<u64, AbiError>> {
    Some(match ctx.number {
        nr::SYS_NANOSLEEP => super::sys_nanosleep(ctx),
        nr::SYS_GETTIMEOFDAY => super::sys_gettimeofday(ctx),
        nr::SYS_TIME => super::sys_time(ctx),
        nr::SYS_CLOCK_GETTIME => super::sys_clock_gettime(ctx),
        nr::SYS_CLOCK_GETRES => super::sys_clock_getres(ctx),
        nr::SYS_TIMERFD_CREATE => super::sys_timerfd_create(ctx),
        nr::SYS_TIMERFD_SETTIME => Ok(0),
        _ => return None,
    })
}
