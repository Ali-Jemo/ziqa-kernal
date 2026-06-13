//! Linux memory-management syscall dispatch family.

use super::{nr, SyscallContext, AbiError};

pub(super) fn handle(ctx: &mut SyscallContext) -> Option<Result<u64, AbiError>> {
    Some(match ctx.number {
        nr::SYS_BRK => super::sys_brk(ctx),
        nr::SYS_MMAP => super::sys_mmap(ctx),
        nr::SYS_MPROTECT => super::sys_mprotect(ctx),
        nr::SYS_MUNMAP => super::sys_munmap(ctx),
        nr::SYS_MADVISE => super::sys_madvise(ctx),
        nr::SYS_MSYNC => super::sys_msync(ctx),
        nr::SYS_MEMFD_CREATE => super::sys_memfd_create(ctx),
        _ => return None,
    })
}
