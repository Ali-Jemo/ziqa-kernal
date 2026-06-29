//! Linux miscellaneous and compatibility syscall dispatch family.

use super::{nr, SyscallContext, AbiError};

pub(crate) fn handle(ctx: &mut SyscallContext) -> Option<Result<u64, AbiError>> {
    let handled = match ctx.number {
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
        // System V IPC syscalls
        nr::SYS_SHMGET => super::sys_shmget(ctx),
        nr::SYS_SHMAT => super::sys_shmat(ctx), // shmat always shared memory
        nr::SYS_SHMDT => Ok(0), // shmdt - unmapping handled by process teardown
        nr::SYS_SHM_OPEN | nr::SYS_SHM_UNLINK => Ok(0), // POSIX shm stubs
        nr::SYS_SEMGET => super::sys_semget(ctx),
        nr::SYS_SEMOP => super::sys_semop(ctx),
        nr::SYS_SEMCTL => super::sys_semctl(ctx),
        nr::SYS_MSGGET | nr::SYS_MSGSND | nr::SYS_MSGRCV => Ok(0), // Message queue stubs
        _ => return None,
    };
    Some(handled)
}