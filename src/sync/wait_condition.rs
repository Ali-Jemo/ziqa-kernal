//! Process blocking and wakeup mechanism.
//! Ported and simplified from Redox OS.

use alloc::vec::Vec;
use spin::Mutex;
use crate::process::Pid;
use crate::process::scheduler::SCHEDULER;

#[derive(Debug)]
pub struct WaitCondition {
    waiters: Mutex<Vec<Pid>>,
}

impl WaitCondition {
    pub const fn new() -> Self {
        Self {
            waiters: Mutex::new(Vec::new()),
        }
    }

    /// Notify all waiting processes to wake up.
    pub fn notify(&self) -> usize {
        let mut waiters = self.waiters.lock();
        let count = waiters.len();
        for pid in waiters.drain(..) {
            if let Some(proc_arc) = SCHEDULER.get_process(pid) {
                let mut proc = proc_arc.lock();
                proc.unblock();
            }
        }
        count
    }

    /// Wait until notified. Blocks the current process.
    pub fn wait(&self, _reason: &'static str) {
        let pid = SCHEDULER.current_pid().expect("wait called outside of process");
        
        {
            let mut waiters = self.waiters.lock();
            waiters.push(pid);
        }

        // Block the current process
        crate::process::scheduler::with_current_task_mut(|proc| {
            proc.block();
        });

        // Trigger a context switch
        SCHEDULER.schedule();
    }
}
