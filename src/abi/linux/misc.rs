//! Linux miscellaneous and compatibility syscall dispatch family.

use super::{nr, SyscallContext, AbiError};

pub(crate) fn handle(ctx: &mut SyscallContext) -> Option<Result<u64, AbiError>> {
    match ctx.number {
        nr::SYS_UNAME => Some(super::sys_uname(ctx)),
        nr::SYS_IOCTL => Some(super::sys_ioctl(ctx)),
        nr::SYS_POLL | nr::SYS_SELECT | nr::SYS_PPOLL | nr::SYS_PSELECT6 => Some(super::sys_poll(ctx)),
        nr::SYS_GETRLIMIT => Some(super::sys_getrlimit(ctx)),
        nr::SYS_SETRLIMIT => Some(super::sys_setrlimit(ctx)),
        nr::SYS_SYSINFO => Some(super::sys_sysinfo(ctx)),
        nr::SYS_PRLIMIT64 => Some(super::sys_prlimit64(ctx)),
        nr::SYS_IOPL => Some(Ok(0)),
        nr::SYS_GETRANDOM => Some(super::sys_getrandom(ctx)),
        nr::SYS_PERSONALITY => Some(Ok(0)),
        nr::SYS_EPOLL_CREATE1 => Some(super::sys_epoll_create1(ctx)),
        nr::SYS_EPOLL_CTL | nr::SYS_EPOLL_WAIT => Some(Ok(0)),
        nr::SYS_EVENTFD => Some(super::sys_eventfd(ctx)),
        nr::SYS_SIGNALFD => Some(super::sys_signalfd(ctx)),
        nr::SYS_GETCPU => Some(super::sys_getcpu(ctx)),
        // Redox/Orbital specific syscalls (high numbers from relibc)
        536870967 => Some(Ok(0)),  // ioprio_set variant
        536870918 => Some(Ok(0)),  // fork variant
        537919529 => Some(Ok(0)),  // relibc internal
        570425347 => Some(Ok(0)),  // relibc internal
        _ => None,
    }
}
