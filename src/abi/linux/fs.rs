//! Linux filesystem syscall dispatch family.
//!
//! This module is the first cohesion boundary for Graphify Community 0. The
//! handler bodies still live in `mod.rs` during migration, but syscall routing is
//! grouped here so filesystem calls stop expanding the Linux ABI facade.

use super::{nr, SyscallContext, AbiError};

pub(super) fn handle(ctx: &mut SyscallContext) -> Option<Result<u64, AbiError>> {
    Some(match ctx.number {
        nr::SYS_WRITE => super::sys_write(ctx),
        nr::SYS_READ => super::sys_read(ctx),
        nr::SYS_CLOSE => super::sys_close(ctx),
        nr::SYS_FSTAT => super::sys_fstat(ctx),
        nr::SYS_WRITEV => super::sys_writev(ctx),
        nr::SYS_ACCESS | nr::SYS_FACCESSAT => super::sys_access(ctx),
        nr::SYS_OPEN => super::sys_open(ctx),
        nr::SYS_STAT | nr::SYS_LSTAT => super::sys_stat(ctx),
        nr::SYS_LSEEK => super::sys_lseek(ctx),
        nr::SYS_DUP => super::sys_dup(ctx),
        nr::SYS_DUP2 => super::sys_dup2(ctx),
        nr::SYS_PIPE => super::sys_pipe(ctx),
        nr::SYS_GETCWD => super::sys_getcwd(ctx),
        nr::SYS_CHDIR => super::sys_chdir(ctx),
        nr::SYS_PREAD64 => super::sys_pread64(ctx),
        nr::SYS_READV => super::sys_readv(ctx),
        nr::SYS_OPENAT => super::sys_openat(ctx),
        nr::SYS_READLINK => super::sys_readlink(ctx),
        nr::SYS_FCNTL => super::sys_fcntl(ctx),
        nr::SYS_FTRUNCATE => Ok(0),
        nr::SYS_FSYNC | nr::SYS_FDATASYNC => Ok(0),
        nr::SYS_SENDFILE => Ok(0),
        nr::SYS_SYNC => Ok(0),
        nr::SYS_FLOCK => Ok(0),
        nr::SYS_UTIMES | nr::SYS_UTIMENSAT => Ok(0),
        nr::SYS_GETDENTS64 => super::sys_getdents64(ctx),
        nr::SYS_MKDIR => super::sys_mkdir(ctx),
        nr::SYS_RMDIR => super::sys_rmdir(ctx),
        nr::SYS_UNLINK => super::sys_unlink(ctx),
        nr::SYS_RENAME => super::sys_rename(ctx),
        nr::SYS_CREAT => super::sys_creat(ctx),
        nr::SYS_NEWFSTATAT => super::sys_newfstatat(ctx),
        nr::SYS_CHMOD => super::sys_chmod(ctx),
        nr::SYS_UMASK => super::sys_umask(ctx),
        nr::SYS_LINK => super::sys_link(ctx),
        nr::SYS_SYMLINK => super::sys_symlink(ctx),
        nr::SYS_STATFS => super::sys_statfs(ctx),
        nr::SYS_MKNOD => Ok(0),
        nr::SYS_FALLOCATE => Ok(0),
        nr::SYS_COPY_FILE_RANGE => Ok(0),
        _ => return None,
    })
}
