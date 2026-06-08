use crate::process::{Pid, scheduler};
use alloc::vec::Vec;
use spin::Mutex;

/// Event that processes can block on until signaled
pub struct Event {
    waiters: Mutex<Vec<Pid>>,
}

impl Event {
    pub fn new() -> Self {
        Self {
            waiters: Mutex::new(Vec::new()),
        }
    }

    /// Block the current process until this event is signaled
    pub fn wait(&self) {
        let pid = scheduler::SCHEDULER.current_pid().expect("No current process");
        self.waiters.lock().push(pid);
        scheduler::SCHEDULER.block_current_task();
    }

    /// Signal all processes waiting on this event to wake up
    pub fn signal(&self) {
        let mut waiters = self.waiters.lock();
        for pid in waiters.drain(..) {
            if let Some(proc_arc) = scheduler::SCHEDULER.get_process(pid) {
                let mut proc = proc_arc.lock();
                proc.make_ready();
                scheduler::SCHEDULER.ready_queues.lock().push(proc.pid, proc.priority);
            }
        }
    }
}
