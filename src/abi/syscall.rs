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

use crate::process::{Process, ProcessState, Pid};

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
        Self { number, args, process }
    }

    pub fn abi_kind(&self) -> crate::process::AbiKind {
        self.process.abi
    }
}

// ── Linux x86_64 syscall numbers ──────────────────────────────────────────────
pub mod nr {
    pub const WRITE:      u64 = 1;
    pub const GETPID:     u64 = 39;
    pub const EXIT:       u64 = 60;
    pub const EXIT_GROUP: u64 = 231;
    pub const KILL:       u64 = 62;
    pub const NANOSLEEP:  u64 = 35;
    pub const CLOCK_NANOSLEEP: u64 = 230;
    pub const GETPPID:    u64 = 110;
    pub const SCHED_YIELD: u64 = 24;
}

/// Error codes (negated errno values)
pub mod errno {
    pub const EPERM:  u64 = 1;
    pub const ESRCH:  u64 = 3;
    pub const EINVAL: u64 = 22;
    pub const ENOSYS: u64 = 38;
}

/// Top-level syscall dispatcher
///
/// First tries to handle core kernel syscalls directly.
/// Falls back to the ABI plugin for ABI-specific syscalls.
pub fn dispatch_syscall(
    registry: &crate::abi::AbiRegistry,
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
            let target_pid = Pid(ctx.args[0]);
            let signum = ctx.args[1] as u8;
            let ok = crate::process::scheduler::SCHEDULER.lock().send_signal(target_pid, signum);
            klog_syscall("kill", ctx.args[0]);
            if ok { return Ok(0); }
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

        nr::WRITE => {
            let fd    = ctx.args[0];
            let _buf  = ctx.args[1] as *const u8;
            let count = ctx.args[2];
            // fd 1 = stdout, fd 2 = stderr → emit via serial
            if fd == 1 || fd == 2 {
                // In a real kernel we'd copy from user-space; here we just log the count
                klog_syscall("write", count);
                return Ok(count);
            }
            // Other fds: delegate to ABI plugin
        }

        _ => {}
    }

    // ── ABI-specific syscalls ─────────────────────────────────────────────────
    let kind = ctx.abi_kind();
    match registry.get(kind) {
        Some(plugin) => plugin.handle_syscall(ctx),
        None => Err(crate::abi::AbiError::UnsupportedSyscall(ctx.number)),
    }
}

#[inline(always)]
fn klog_syscall(name: &'static str, val: u64) {
    crate::klog!(crate::klog::Level::Debug, "syscall {} -> {}", name, val);
}
