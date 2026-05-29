/// Abstraction for system resource operations to decouple syscalls from scheduler.
pub trait SyscallHandler {
    fn kill(&self, pid: u64, sig: u8) -> bool;
    fn fork(&self, parent_pid: u64) -> Option<u64>;
    fn waitpid(&self, parent_pid: u64, child_pid: i64) -> Option<(u64, i64)>;
    // Add other methods as needed
}

/// Implementation using the real scheduler.
pub struct KernelSyscallHandler;

impl SyscallHandler for KernelSyscallHandler {
    fn kill(&self, pid: u64, sig: u8) -> bool {
        crate::process::scheduler::SCHEDULER.lock().send_signal(crate::process::Pid(pid), sig)
    }

    fn fork(&self, parent_pid: u64) -> Option<u64> {
        crate::process::scheduler::SCHEDULER.lock().fork(crate::process::Pid(parent_pid)).map(|p| p.0)
    }

    fn waitpid(&self, parent_pid: u64, child_pid: i64) -> Option<(u64, i64)> {
        crate::process::scheduler::SCHEDULER.lock().waitpid(crate::process::Pid(parent_pid), child_pid).map(|(p, code)| (p.0, code))
    }
}
