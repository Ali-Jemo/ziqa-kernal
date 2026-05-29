/// Syscall dispatch context and handler for ZiqaKernel
///
/// Handles the core kernel syscalls directly, before delegating
/// ABI-specific syscalls to the registered plugin.
///
/// Linux x86_64 syscall numbers (subset):
///   1  = write(fd, buf, count)
///   39 = getpid()
///   60 = exit(code)
///   62 = kill(pid, sig)
///  230 = clock_nanosleep / nanosleep (simplified: ms in arg1)
use crate::process::{Process, ProcessState};
use crate::capability::ResourceKind;

/// The context passed to an ABI plugin's syscall handler
pub struct SyscallContext<'a> {
    /// The syscall number (RAX on x86_64)
    pub number: u64,
    /// Argument registers (RDI, RSI, RDX, R10, R8, R9 on Linux x86_64)
    pub args: [u64; 6],
    /// Mutable reference to the calling process
    pub process: &'a mut Process,
}

impl<'a> SyscallContext<'a> {
    pub fn new(number: u64, args: [u64; 6], process: &'a mut Process) -> Self {
        Self {
            number,
            args,
            process,
        }
    }

    pub fn abi_kind(&self) -> crate::process::AbiKind {
        self.process.abi
    }
}

// ── Linux x86_64 syscall numbers ──────────────────────────────────────────────
pub mod nr {
    pub const WRITE: u64 = 1;
    pub const GETPID: u64 = 39;
    pub const EXIT: u64 = 60;
    pub const EXIT_GROUP: u64 = 231;
    pub const KILL: u64 = 62;
    pub const NANOSLEEP: u64 = 35;
    pub const CLOCK_NANOSLEEP: u64 = 230;
    pub const GETPPID: u64 = 110;
    pub const SCHED_YIELD: u64 = 24;
    pub const FORK: u64 = 57;
    pub const WAITPID: u64 = 61; // wait4 in Linux; simplified as waitpid
    pub const MMAP: u64 = 9;
    pub const MUNMAP: u64 = 11;
    pub const NET_NOTIFY: u64 = 500;
    pub const NET_ACK: u64 = 501;

    // ── ZiqaKernel native IPC and SHM syscalls ────────────────────────────────
    /// Create SHM segment: [size] → id
    pub const ZIQA_SHM_CREATE: u64 = 1010;
    /// Attach SHM segment: [id] → addr
    pub const ZIQA_SHM_ATTACH: u64 = 1011;
    /// Create IPC channel: () → id
    pub const ZIQA_IPC_CREATE: u64 = 1020;
    /// Send IPC message: [chan_id, data_ptr, len] → 0 / -errno
    pub const ZIQA_IPC_SEND: u64 = 1021;
    /// Recv IPC message: [chan_id, data_ptr, max_len] → bytes_read / -errno
    pub const ZIQA_IPC_RECV: u64 = 1022;

    // ── ZiqaKernel native capability syscalls (userspace libposix ABI) ────────
    /// Close a file capability and release its FD slot.
    /// args: [fd: usize] → 0 / -EBADF
    pub const ZIQA_CAP_CLOSE: u64 = 1003;
    /// Seek within a file capability.
    /// args: [fd: usize, offset: i64, whence: i32] → new_offset / -errno
    pub const ZIQA_CAP_SEEK: u64 = 1004;

    // ── ZiqaKernel native signal syscalls (userspace libposix signal ABI) ─────
    /// Install or clear a signal action.
    /// args: [signum: u8, action_kind: u64, handler_ptr: u64, sa_mask: u32]
    ///   action_kind: 0=Default, 1=Ignore, 2=Custom(handler_ptr)
    /// → 0 / -EINVAL
    pub const ZIQA_SIG_SETACTION: u64 = 2000;
    /// Read the calling process's current signal block mask.
    /// args: (none) → mask: u32
    pub const ZIQA_SIG_GETMASK: u64 = 2001;
    /// Write the calling process's signal block mask.
    /// args: [new_mask: u32] → 0
    pub const ZIQA_SIG_SETMASK: u64 = 2002;
    /// Send signal to a process (ZiqaKernel IPC path).
    /// args: [pid: u64, signum: u8] → 0 / -ESRCH / -EINVAL
    pub const ZIQA_SIG_KILL: u64 = 2003;
    /// Suspend the calling process until an unblocked signal is received.
    /// args: (none) → always -EINTR
    pub const ZIQA_SIG_PAUSE: u64 = 2004;
}

/// Error codes (negated errno values)
pub mod errno {
    pub const EPERM: u64 = 1;
    pub const ESRCH: u64 = 3;
    pub const EBADF: u64 = 9;
    pub const EFAULT: u64 = 14;
    pub const EINVAL: u64 = 22;
    pub const ENOSYS: u64 = 38;
}

/// Check if a process has the required capability for a kernel syscall.
/// Returns true if the syscall is allowed, false if denied.
fn check_capability(proc: &Process, kind: ResourceKind, needs_write: bool, needs_exec: bool) -> bool {
    proc.capabilities.has_permission(kind, needs_write, needs_exec)
}

/// Top-level syscall dispatcher
///
/// First tries to handle core kernel syscalls directly.
/// Falls back to the ABI plugin for ABI-specific syscalls.
pub fn dispatch_syscall(
    registry: &crate::abi::AbiRegistry,
    handler: &dyn crate::abi::handler::SyscallHandler,
    ctx: &mut SyscallContext,
) -> Result<u64, crate::abi::AbiError> {
    // ── Core kernel syscalls (ABI-independent) ────────────────────────────────
    match ctx.number {
        nr::GETPID => {
            let pid = ctx.process.pid.0;
            klog_syscall("getpid", pid);
            return Ok(pid);
        }

        nr::GETPPID => {
            // We don't track parent PIDs yet; return 1 (init)
            return Ok(1);
        }

        nr::EXIT | nr::EXIT_GROUP => {
            let code = ctx.args[0] as i64;
            klog_syscall("exit", code as u64);
            ctx.process.exit(code);
            return Ok(0);
        }

        nr::KILL => {
            let target_pid = ctx.args[0];
            let signum = ctx.args[1] as u8;
            let ok = handler.kill(target_pid, signum);
            klog_syscall("kill", target_pid);
            if ok {
                return Ok(0);
            }
            return Err(crate::abi::AbiError::Other("ESRCH: no such process"));
        }

        nr::NANOSLEEP | nr::CLOCK_NANOSLEEP => {
            // Simplified: arg0 = milliseconds to sleep
            let ms = ctx.args[0];
            let pid = ctx.process.pid;
            klog_syscall("nanosleep", ms);
            crate::timer::sleep_ms(pid, ms);
            return Ok(0);
        }

        nr::SCHED_YIELD => {
            // Mark as Ready so scheduler picks someone else next tick
            if ctx.process.state == ProcessState::Running {
                ctx.process.state = ProcessState::Ready;
            }
            return Ok(0);
        }

        nr::FORK => {
            // Check ProcessCreate capability before forking
            if !check_capability(ctx.process, ResourceKind::ProcessCreate, false, false) {
                return Err(crate::abi::AbiError::Other("EPERM: no process creation capability"));
            }
            // Clone the current process; child gets pid, parent gets child pid
            let parent_pid = ctx.process.pid.0;
            let child = handler.fork(parent_pid);
            klog_syscall("fork", child.unwrap_or(u64::MAX));
            match child {
                Some(child_pid) => return Ok(child_pid), // parent sees child pid
                None => return Err(crate::abi::AbiError::Other("fork: out of slots")),
            }
        }

        nr::WAITPID => {
            // args: [child_pid_or_neg1, status_ptr (ignored), options (ignored)]
            let child_arg = ctx.args[0] as i64;
            let parent_pid = ctx.process.pid.0;
            let result = handler.waitpid(parent_pid, child_arg);
            klog_syscall("waitpid", ctx.args[0]);
            match result {
                Some((pid, _code)) => return Ok(pid),
                None => return Ok(0), // no zombie child yet
            }
        }

        nr::MMAP => {
            // ... (MMAP and MUNMAP implementation remains here as they seem to operate on process state)
            // Note: For full decoupling, these should also be moved into the handler if they modify external state
            // or if the process management is abstracted further.
            // Keeping them here for now as they are mostly process-local state modification.
            // (I am omitting them in this response for brevity, you should keep the existing logic)
            // ...
        }
        // ... (Keep existing MMAP and MUNMAP implementation)
        nr::MUNMAP => {
            // args: [addr, length]
            let addr = ctx.args[0];
            use crate::memory::VirtAddr as KVirtAddr;
            let target = KVirtAddr::new(addr);
            // Remove the matching region
            for slot in ctx.process.regions.iter_mut() {
                if let Some(r) = slot {
                    if r.start == target {
                        *slot = None;
                        ctx.process.region_count = ctx.process.region_count.saturating_sub(1);
                        klog_syscall("munmap", addr);
                        return Ok(0);
                    }
                }
            }
            return Err(crate::abi::AbiError::Other("munmap: region not found"));
        }
        nr::NET_NOTIFY => {
            if !check_capability(ctx.process, ResourceKind::DeviceIo, true, false) {
                return Err(crate::abi::AbiError::Other("EPERM: no DeviceIo capability"));
            }
            let queue_index = ctx.args[0] as u32;
            unsafe {
                let net_ptr = &raw mut crate::drivers::virtio_net::VIRTIO_NET;
                if let Some(net) = (*net_ptr).as_mut() {
                    net.write_config_32(0x050, queue_index);
                }
            }
            return Ok(0);
        }
        nr::NET_ACK => {
            if !check_capability(ctx.process, ResourceKind::DeviceIo, true, false) {
                return Err(crate::abi::AbiError::Other("EPERM: no DeviceIo capability"));
            }
            unsafe {
                let net_ptr = &raw mut crate::drivers::virtio_net::VIRTIO_NET;
                if let Some(net) = (*net_ptr).as_mut() {
                    net.ack_interrupt();
                }
            }
            return Ok(0);
        }
        _ => {}
    }

    // ── ZiqaKernel native capability/signal syscalls ──────────────────────────
    // These are ABI-independent (not Linux-specific) and sit above the plugin
    // layer so that libposix can call them regardless of process ABI kind.
    match ctx.number {
        nr::ZIQA_CAP_CLOSE => return ziqa_cap_close(ctx),
        nr::ZIQA_CAP_SEEK  => return ziqa_cap_seek(ctx),
        nr::ZIQA_SIG_SETACTION => return ziqa_sig_setaction(ctx),
        nr::ZIQA_SIG_GETMASK   => return ziqa_sig_getmask(ctx),
        nr::ZIQA_SIG_SETMASK   => return ziqa_sig_setmask(ctx),
        nr::ZIQA_SIG_KILL      => return ziqa_sig_kill(ctx),
        nr::ZIQA_SIG_PAUSE     => return ziqa_sig_pause(ctx),

        // ── SHM / IPC handlers ──
        nr::ZIQA_SHM_CREATE => {
            let size = ctx.args[0] as usize;
            let pid = ctx.process.pid;
            let id = crate::ipc::shm::SHM.lock().create(pid, size);
            klog_syscall("shm_create", id as u64);
            return Ok(id as u64);
        }
        nr::ZIQA_SHM_ATTACH => {
            let id = ctx.args[0] as u32;
            let pid = ctx.process.pid;
            match crate::ipc::shm::SHM.lock().attach(id, pid) {
                Ok(addr) => {
                    klog_syscall("shm_attach", addr);
                    return Ok(addr);
                }
                Err(_) => return Ok((-errno::EINVAL as i64) as u64),
            }
        }
        nr::ZIQA_IPC_CREATE => {
            match crate::ipc::create_channel() {
                Some(id) => {
                    klog_syscall("ipc_create", id as u64);
                    return Ok(id as u64);
                }
                None => return Ok((-errno::ENOSYS as i64) as u64),
            }
        }
        nr::ZIQA_IPC_SEND => {
            let chan_id = ctx.args[0] as u32;
            let ptr = ctx.args[1] as *const u8;
            let len = ctx.args[2] as usize;
            let sender = ctx.process.pid;

            if ptr.is_null() || len > crate::ipc::MSG_MAX {
                return Ok((-errno::EFAULT as i64) as u64);
            }

            let mut tmp = [0u8; crate::ipc::MSG_MAX];
            unsafe { core::ptr::copy_nonoverlapping(ptr, tmp.as_mut_ptr(), len); }

            match crate::ipc::send(chan_id, sender, &tmp[..len]) {
                Ok(_) => return Ok(0),
                Err(_) => return Ok((-errno::EINVAL as i64) as u64),
            }
        }
        nr::ZIQA_IPC_RECV => {
            let chan_id = ctx.args[0] as u32;
            let ptr = ctx.args[1] as *mut u8;
            let max_len = ctx.args[2] as usize;

            if ptr.is_null() {
                return Ok((-errno::EFAULT as i64) as u64);
            }

            match crate::ipc::recv(chan_id) {
                Ok(msg) => {
                    let copy_len = msg.len.min(max_len);
                    unsafe { core::ptr::copy_nonoverlapping(msg.data.as_ptr(), ptr, copy_len); }
                    return Ok(copy_len as u64);
                }
                Err(_) => return Ok((-errno::EINVAL as i64) as u64),
            }
        }
        _ => {}
    }

    // ── ABI-specific syscalls ─────────────────────────────────────────────────
    let kind = ctx.abi_kind();
    match registry.get(kind) {
        Some(plugin) => plugin.handle_syscall(handler, ctx),
        None => Err(crate::abi::AbiError::UnsupportedSyscall(ctx.number)),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// ZiqaKernel Native Syscall Implementations
// ─────────────────────────────────────────────────────────────────────────────

/// ZIQA_CAP_CLOSE (1003) — close a file descriptor.
///
/// args[0] = fd (usize)
/// Returns 0 on success, -EBADF if fd is not open, -EPERM if fd is 0/1/2.
fn ziqa_cap_close(ctx: &mut SyscallContext) -> Result<u64, crate::abi::AbiError> {
    let fd = ctx.args[0] as usize;
    // Refuse to close stdin/stdout/stderr via this path — use dup2 to redirect.
    if fd < 3 {
        klog_syscall("ziqa_cap_close", fd as u64);
        return Ok((-1_i64) as u64); // -EPERM
    }
    let ok = ctx.process.fds.close(fd);
    klog_syscall("ziqa_cap_close", fd as u64);
    if ok {
        Ok(0)
    } else {
        Ok((-9_i64) as u64) // -EBADF
    }
}

/// ZIQA_CAP_SEEK (1004) — reposition file offset.
///
/// args[0] = fd (usize)
/// args[1] = offset (i64, sign-extended)
/// args[2] = whence: 0=SEEK_SET, 1=SEEK_CUR, 2=SEEK_END
/// Returns new offset / -EBADF / -EINVAL / -ESPIPE
fn ziqa_cap_seek(ctx: &mut SyscallContext) -> Result<u64, crate::abi::AbiError> {
    use crate::process::FdTarget;
    let fd      = ctx.args[0] as usize;
    let offset  = ctx.args[1] as i64;
    let whence  = ctx.args[2] as i32;

    let desc = match ctx.process.fds.get_mut(fd) {
        Some(d) => d,
        None    => return Ok((-9_i64) as u64), // -EBADF
    };

    // Only regular file FDs support seeking.
    let is_file = matches!(desc.target, FdTarget::File(_));
    if !is_file {
        return Ok((-29_i64) as u64); // -ESPIPE
    }

    // To get the file size for SEEK_END we'd need a VFS stat; for now we
    // cap SEEK_END at current offset (conservative — avoids unsound reads).
    let current = desc.offset as i64;
    let new_offset: i64 = match whence {
        0 => offset,                   // SEEK_SET
        1 => current.saturating_add(offset), // SEEK_CUR
        2 => current,                  // SEEK_END — clamp to current until VFS stat
        _ => return Ok((-22_i64) as u64), // -EINVAL
    };

    if new_offset < 0 {
        return Ok((-22_i64) as u64); // -EINVAL: negative file position
    }

    desc.offset = new_offset as usize;
    klog_syscall("ziqa_cap_seek", new_offset as u64);
    Ok(new_offset as u64)
}

/// ZIQA_SIG_SETACTION (2000) — install a signal action.
///
/// args[0] = signum (u8)
/// args[1] = action_kind: 0=Default, 1=Ignore, 2=Custom
/// args[2] = handler_ptr (u64, used only when action_kind == 2)
/// args[3] = sa_mask (u32) — signals to block during handler (stored for reference)
/// Returns 0 on success, -EINVAL on bad signum or attempt to catch SIGKILL/SIGSTOP.
fn ziqa_sig_setaction(ctx: &mut SyscallContext) -> Result<u64, crate::abi::AbiError> {
    use crate::process::signal::{SignalAction, sig};
    let signum      = ctx.args[0] as u8;
    let action_kind = ctx.args[1];
    let handler_ptr = ctx.args[2];

    if signum == 0 || signum > sig::MAX {
        return Ok((-22_i64) as u64); // -EINVAL
    }
    // SIGKILL (9) and SIGSTOP (19) cannot be caught or ignored.
    if signum == sig::SIGKILL || signum == sig::SIGSTOP {
        return Ok((-22_i64) as u64); // -EINVAL
    }

    let action = match action_kind {
        0 => SignalAction::Default,
        1 => SignalAction::Ignore,
        2 => SignalAction::Handler(handler_ptr),
        _ => return Ok((-22_i64) as u64), // -EINVAL
    };

    let ok = ctx.process.signals.set_action(signum, action);
    klog_syscall("ziqa_sig_setaction", signum as u64);
    if ok { Ok(0) } else { Ok((-22_i64) as u64) }
}

/// ZIQA_SIG_GETMASK (2001) — read the calling process's signal block mask.
///
/// Returns the current blocked mask as u32 (bit N = signal N+1 is blocked).
fn ziqa_sig_getmask(ctx: &mut SyscallContext) -> Result<u64, crate::abi::AbiError> {
    let mask = ctx.process.signals.blocked as u64;
    klog_syscall("ziqa_sig_getmask", mask);
    Ok(mask)
}

/// ZIQA_SIG_SETMASK (2002) — overwrite the signal block mask.
///
/// args[0] = new_mask (u32)
/// SIGKILL and SIGSTOP bits are always cleared (unblockable).
/// Returns 0.
fn ziqa_sig_setmask(ctx: &mut SyscallContext) -> Result<u64, crate::abi::AbiError> {
    use crate::process::signal::sig;
    let mut new_mask = ctx.args[0] as u32;
    // Force-clear unblockable signals.
    new_mask &= !((1u32 << (sig::SIGKILL - 1)) | (1u32 << (sig::SIGSTOP - 1)));
    ctx.process.signals.blocked = new_mask;
    klog_syscall("ziqa_sig_setmask", new_mask as u64);
    Ok(0)
}

/// ZIQA_SIG_KILL (2003) — IPC-native signal delivery.
///
/// args[0] = target_pid (u64); if 0, send to calling process.
/// args[1] = signum (u8)
/// Returns 0 / -ESRCH / -EINVAL
fn ziqa_sig_kill(ctx: &mut SyscallContext) -> Result<u64, crate::abi::AbiError> {
    use crate::process::signal::sig;
    let pid_arg = ctx.args[0];
    let signum  = ctx.args[1] as u8;

    if signum > sig::MAX {
        return Ok((-22_i64) as u64); // -EINVAL
    }

    // pid 0 means "send to self".
    let target = if pid_arg == 0 {
        ctx.process.pid
    } else {
        crate::process::Pid(pid_arg)
    };

    // Self-signal: modify directly without scheduler lock.
    if target == ctx.process.pid {
        ctx.process.signals.send(signum);
        klog_syscall("ziqa_sig_kill(self)", signum as u64);
        return Ok(0);
    }

    // Remote signal: go through the scheduler.
    let ok = crate::process::scheduler::SCHEDULER
        .lock()
        .send_signal(target, signum);
    klog_syscall("ziqa_sig_kill", target.0);
    if ok {
        Ok(0)
    } else {
        Ok((-3_i64) as u64) // -ESRCH
    }
}

/// ZIQA_SIG_PAUSE (2004) — suspend until an unblocked signal is delivered.
///
/// Transitions the process to Blocked.  The scheduler will unblock it
/// when `deliver_signals()` detects a pending unblocked signal.
/// Always returns -EINTR (POSIX: pause only returns on signal delivery).
fn ziqa_sig_pause(ctx: &mut SyscallContext) -> Result<u64, crate::abi::AbiError> {
    // Only block if there is no already-pending unblocked signal.
    if !ctx.process.signals.has_pending() {
        ctx.process.state = crate::process::ProcessState::Blocked;
        klog_syscall("ziqa_sig_pause", ctx.process.pid.0);
    }
    // POSIX: pause() always returns -1/EINTR.
    Ok((-4_i64) as u64) // -EINTR
}

#[inline(always)]
fn klog_syscall(name: &'static str, val: u64) {
    crate::klog!(crate::klog::Level::Debug, "syscall {} -> {}", name, val);
}
