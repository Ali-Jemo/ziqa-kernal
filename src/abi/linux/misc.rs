//! Linux miscellaneous and compatibility syscall dispatch family.

use super::{nr, SyscallContext, AbiError};

pub(crate) fn handle(ctx: &mut SyscallContext) -> Option<Result<u64, AbiError>> {
    Some(match ctx.number {
        nr::SYS_UNAME => super::sys_uname(ctx),
        nr::SYS_IOCTL => super::sys_ioctl(ctx),
        nr::SYS_POLL | nr::SYS_SELECT | nr::SYS_PPOLL | nr::SYS_PSELECT6 => super::sys_poll(ctx),
        nr::SYS_GETRLIMIT => super::sys_getrlimit(ctx),
        nr::SYS_SETRLIMIT => super::sys_setrlimit(ctx),
        nr::SYS_SYSINFO => super::sys_sysinfo(ctx),
        nr::SYS_PRLIMIT64 => super::sys_prlimit64(ctx),
        nr::SYS_IOPL => Ok(0),
        nr::SYS_GETRANDOM => super::sys_getrandom(ctx),
        nr::SYS_PERSONALITY => Ok(0),
        nr::SYS_EPOLL_CREATE1 => super::sys_epoll_create1(ctx),
        nr::SYS_EPOLL_CTL | nr::SYS_EPOLL_WAIT => Ok(0),
        nr::SYS_EVENTFD => super::sys_eventfd(ctx),
        nr::SYS_SIGNALFD => super::sys_signalfd(ctx),
        nr::SYS_GETCPU => super::sys_getcpu(ctx),
        _ => return None,
    })
}
