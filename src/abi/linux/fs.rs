/// Linux ABI - Filesystem Syscall Handlers
use crate::abi::syscall::SyscallContext;
use crate::abi::AbiError;

pub fn handle(ctx: &mut SyscallContext) -> Option<Result<u64, AbiError>> {
    match ctx.number {
        crate::abi::linux::nr::SYS_OPEN => Some(sys_open(ctx)),
        crate::abi::linux::nr::SYS_READ => Some(sys_read(ctx)),
        crate::abi::linux::nr::SYS_WRITE => Some(sys_write(ctx)),
        crate::abi::linux::nr::SYS_CLOSE => Some(sys_close(ctx)),
        crate::abi::linux::nr::SYS_STAT => Some(sys_stat(ctx)),
        crate::abi::linux::nr::SYS_FSTAT => Some(sys_fstat(ctx)),
        crate::abi::linux::nr::SYS_LSTAT => Some(sys_stat(ctx)),
        crate::abi::linux::nr::SYS_LSEEK => Some(sys_lseek(ctx)),
        crate::abi::linux::nr::SYS_DUP => Some(sys_dup(ctx)),
        crate::abi::linux::nr::SYS_DUP2 => Some(sys_dup2(ctx)),
        crate::abi::linux::nr::SYS_PIPE => Some(sys_pipe(ctx)),
        crate::abi::linux::nr::SYS_GETCWD => Some(sys_getcwd(ctx)),
        crate::abi::linux::nr::SYS_CHDIR => Some(sys_chdir(ctx)),
        crate::abi::linux::nr::SYS_RENAME => Some(sys_rename(ctx)),
        crate::abi::linux::nr::SYS_MKDIR => Some(sys_mkdir(ctx)),
        crate::abi::linux::nr::SYS_RMDIR => Some(sys_rmdir(ctx)),
        crate::abi::linux::nr::SYS_CREAT => Some(sys_creat(ctx)),
        crate::abi::linux::nr::SYS_LINK => Some(sys_link(ctx)),
        crate::abi::linux::nr::SYS_UNLINK => Some(sys_unlink(ctx)),
        crate::abi::linux::nr::SYS_SYMLINK => Some(sys_symlink(ctx)),
        crate::abi::linux::nr::SYS_CHMOD => Some(sys_chmod(ctx)),
        crate::abi::linux::nr::SYS_UMASK => Some(sys_umask(ctx)),
        crate::abi::linux::nr::SYS_STATFS => Some(sys_statfs(ctx)),
        crate::abi::linux::nr::SYS_GETDENTS64 => Some(sys_getdents64(ctx)),
        crate::abi::linux::nr::SYS_NEWFSTATAT => Some(sys_newfstatat(ctx)),
        crate::abi::linux::nr::SYS_PREAD64 => Some(sys_pread64(ctx)),
        crate::abi::linux::nr::SYS_READV => Some(sys_readv(ctx)),
        crate::abi::linux::nr::SYS_WRITEV => Some(sys_writev(ctx)),
        crate::abi::linux::nr::SYS_OPENAT => Some(sys_openat(ctx)),
        crate::abi::linux::nr::SYS_READLINK => Some(sys_readlink(ctx)),
        crate::abi::linux::nr::SYS_FCNTL => Some(sys_fcntl(ctx)),
        _ => None,
    }
}

// ... Move sys_open, sys_read, sys_write, etc. from mod.rs to here ...
// (I will leave the implementation in mod.rs for now and migrate step-by-step
// to ensure no breakage)
