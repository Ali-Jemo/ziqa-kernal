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
    pub const FORK:       u64 = 57;
    pub const WAITPID:    u64 = 61;  // wait4 in Linux; simplified as waitpid
    pub const MMAP:       u64 = 9;
    pub const MUNMAP:     u64 = 11;
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

        nr::FORK => {
            // Clone the current process; child gets pid, parent gets child pid
            let parent_pid = ctx.process.pid;
            let child = crate::process::scheduler::SCHEDULER.lock().fork(parent_pid);
            klog_syscall("fork", child.map(|p| p.0).unwrap_or(u64::MAX));
            match child {
                Some(child_pid) => return Ok(child_pid.0), // parent sees child pid
                None => return Err(crate::abi::AbiError::Other("fork: out of slots")),
            }
        }

        nr::WAITPID => {
            // args: [child_pid_or_neg1, status_ptr (ignored), options (ignored)]
            let child_arg = ctx.args[0] as i64;
            let parent_pid = ctx.process.pid;
            let result = crate::process::scheduler::SCHEDULER.lock().waitpid(parent_pid, child_arg);
            klog_syscall("waitpid", ctx.args[0]);
            match result {
                Some((pid, _code)) => return Ok(pid.0),
                None => return Ok(0), // no zombie child yet
            }
        }

        nr::MMAP => {
            // args: [addr_hint, length, prot, flags, fd, offset]
            let length = ctx.args[1] as usize;
            if length == 0 {
                return Err(crate::abi::AbiError::Other("mmap: zero length"));
            }
            use crate::memory::{MemoryRegion, paging::{MemoryRegionFlags}};
            use crate::memory::VirtAddr as KVirtAddr;
            // Allocate a virtual region above 0x1000_0000 based on region count
            let base = 0x1000_0000u64 + (ctx.process.region_count as u64) * 0x10_0000;
            let region = MemoryRegion {
                start: KVirtAddr::new(base),
                size: length,
                flags: MemoryRegionFlags::read_write(),
                is_file_backed: ctx.args[4] as i64 >= 0,
                file_offset: ctx.args[5],
            };
            ctx.process.add_region(region);
            klog_syscall("mmap", base);
            return Ok(base);
        }

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
