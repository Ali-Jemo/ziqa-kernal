/// Linux ABI - Filesystem Syscall Handlers
use super::{nr, SyscallContext, AbiError};

pub fn handle(ctx: &mut SyscallContext) -> Option<Result<u64, AbiError>> {
    match ctx.number {
        nr::SYS_OPEN => Some(super::sys_open(ctx)),
        nr::SYS_READ => Some(super::sys_read(ctx)),
        nr::SYS_WRITE => Some(super::sys_write(ctx)),
        nr::SYS_CLOSE => Some(super::sys_close(ctx)),
        nr::SYS_STAT => Some(super::sys_stat(ctx)),
        nr::SYS_FSTAT => Some(super::sys_fstat(ctx)),
        nr::SYS_LSTAT => Some(super::sys_stat(ctx)),
        nr::SYS_LSEEK => Some(super::sys_lseek(ctx)),
        nr::SYS_DUP => Some(super::sys_dup(ctx)),
        nr::SYS_DUP2 => Some(super::sys_dup2(ctx)),
        nr::SYS_PIPE => Some(super::sys_pipe(ctx)),
        nr::SYS_PIPE2 => Some(super::sys_pipe2(ctx)),
        nr::SYS_GETCWD => Some(super::sys_getcwd(ctx)),
        nr::SYS_CHDIR => Some(super::sys_chdir(ctx)),
        nr::SYS_RENAME => Some(super::sys_rename(ctx)),
        nr::SYS_MKDIR => Some(super::sys_mkdir(ctx)),
        nr::SYS_RMDIR => Some(super::sys_rmdir(ctx)),
        nr::SYS_CREAT => Some(super::sys_creat(ctx)),
        nr::SYS_LINK => Some(super::sys_link(ctx)),
        nr::SYS_UNLINK => Some(super::sys_unlink(ctx)),
        nr::SYS_SYMLINK => Some(super::sys_symlink(ctx)),
        nr::SYS_CHMOD => Some(super::sys_chmod(ctx)),
        nr::SYS_UMASK => Some(super::sys_umask(ctx)),
        nr::SYS_STATFS => Some(super::sys_statfs(ctx)),
        nr::SYS_GETDENTS64 => Some(super::sys_getdents64(ctx)),
        nr::SYS_NEWFSTATAT => Some(super::sys_newfstatat(ctx)),
        nr::SYS_PREAD64 => Some(super::sys_pread64(ctx)),
        nr::SYS_READV => Some(super::sys_readv(ctx)),
        nr::SYS_WRITEV => Some(super::sys_writev(ctx)),
        nr::SYS_OPENAT => Some(super::sys_openat(ctx)),
        nr::SYS_READLINK => Some(super::sys_readlink(ctx)),
        nr::SYS_ACCESS => Some(super::sys_access(ctx)),
        nr::SYS_FCNTL => Some(super::sys_fcntl(ctx)),
        _ => None,
    }
}

// (Migration of all sys_* filesystem functions here...)
// [Functions: sys_write, sys_read, sys_close, sys_fstat, sys_open, sys_stat, sys_lseek, sys_dup, sys_dup2, sys_pipe, sys_getcwd, sys_chdir, sys_writev, sys_access, sys_readv, sys_openat, sys_readlink, sys_fcntl, sys_getdents64, sys_mkdir, sys_rmdir, sys_unlink, sys_rename, sys_creat, sys_newfstatat, sys_chmod, sys_umask, sys_link, sys_symlink, sys_statfs, sys_pread64, known_path]
// ...
