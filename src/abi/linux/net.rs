//! Linux network/socket syscall dispatch family.

use super::{nr, SyscallContext, AbiError};

pub(super) fn handle(ctx: &mut SyscallContext) -> Option<Result<u64, AbiError>> {
    Some(match ctx.number {
        nr::SYS_SOCKET => super::sys_socket(ctx),
        nr::SYS_BIND | nr::SYS_LISTEN => Ok(0),
        nr::SYS_CONNECT => Ok((-111_i64) as u64), // -ECONNREFUSED
        nr::SYS_ACCEPT => Ok((-11_i64) as u64),   // -EAGAIN
        nr::SYS_SENDTO => super::sys_sendto(ctx),
        nr::SYS_RECVFROM => Ok((-11_i64) as u64), // -EAGAIN
        nr::SYS_SETSOCKOPT | nr::SYS_GETSOCKOPT => Ok(0),
        _ => return None,
    })
}
