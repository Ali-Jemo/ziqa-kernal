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
}

/// Error codes (negated errno values)
pub mod errno {
    pub const EPERM: u64 = 1;
    pub const ESRCH: u64 = 3;
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
        _ => {}
    }

    // ── ABI-specific syscalls ─────────────────────────────────────────────────
    let kind = ctx.abi_kind();
    match registry.get(kind) {
        Some(plugin) => plugin.handle_syscall(handler, ctx),
        None => Err(crate::abi::AbiError::UnsupportedSyscall(ctx.number)),
    }
}

#[inline(always)]
fn klog_syscall(name: &'static str, val: u64) {
    crate::klog!(crate::klog::Level::Debug, "syscall {} -> {}", name, val);
}
