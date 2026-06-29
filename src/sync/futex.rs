//! Physical-address-keyed Futex system
//!
//! Provides inter-process synchronization using memory physical addresses as unique keys.
//! This is critical for synchronizing shared memory (MAP_SHARED) across address spaces.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use spin::Mutex;
use crate::process::Pid;
use crate::process::scheduler::SCHEDULER;

#[derive(Clone, Copy)]
struct Waiter {
    pid: Pid,
    bitset: u32,
}

pub struct FutexManager {
    // Map physical address to a list of waiting processes
    queues: BTreeMap<u64, Vec<Waiter>>,
}

impl FutexManager {
    pub const fn new() -> Self {
        Self {
            queues: BTreeMap::new(),
        }
    }

    /// Wait on a futex at the given physical address.
    /// Returns true if it actually blocked, false otherwise.
    pub fn wait(&mut self, phys_addr: u64, expected_val: u32, bitset: u32) -> bool {
        let po = crate::memory::paging::phys_offset().as_u64();
        let ptr = (po + phys_addr) as *const u32;
        
        // Safety: We got the physical address from the page table entry, which is valid and mapped.
        let val = unsafe { *ptr };
        if val != expected_val {
            return false; // Value mismatch, do not block
        }

        let pid = match SCHEDULER.current_pid() {
            Some(p) => p,
            None => return false,
        };

        self.queues.entry(phys_addr).or_insert_with(Vec::new).push(Waiter { pid, bitset });

        // Block the current process
        crate::process::scheduler::with_current_task_mut(|proc| {
            proc.timed_out = false;
            proc.block();
        });
        
        true
    }

    /// Wake processes waiting on a futex at the given physical address.
    /// Returns number of woken processes.
    pub fn wake(&mut self, phys_addr: u64, count: usize, bitset: u32) -> usize {
        let mut woken = 0;
        if let Some(waiters) = self.queues.get_mut(&phys_addr) {
            let mut i = 0;
            while i < waiters.len() && woken < count {
                if (waiters[i].bitset & bitset) != 0 {
                    let waiter = waiters.remove(i);
                    if let Some(proc_arc) = SCHEDULER.get_process(waiter.pid) {
                        let mut proc = proc_arc.lock();
                        proc.unblock();
                        woken += 1;
                    }
                    // Do not increment i, as we removed an element
                } else {
                    i += 1;
                }
            }
            if waiters.is_empty() {
                self.queues.remove(&phys_addr);
            }
        }
        woken
    }

    /// Remove a process from the wait queue for a given futex.
    pub fn cancel_wait(&mut self, phys_addr: u64, pid: Pid) {
        if let Some(waiters) = self.queues.get_mut(&phys_addr) {
            waiters.retain(|w| w.pid != pid);
            if waiters.is_empty() {
                self.queues.remove(&phys_addr);
            }
        }
    }

    /// Requeue waiters from one futex to another.
    /// Returns total number of woken or requeued processes.
    pub fn requeue(&mut self, src_phys: u64, dst_phys: u64, wake_count: usize, requeue_count: usize) -> usize {
        let mut count = 0;
        
        // 1. Wake up to wake_count waiters
        // Note: Requeue usually doesn't use bitsets (default to 0xFFFFFFFF)
        let pids_to_wake = if let Some(waiters) = self.queues.get_mut(&src_phys) {
            let actual_wake = wake_count.min(waiters.len());
            let pids: Vec<_> = waiters.drain(..actual_wake).map(|w| w.pid).collect();
            if waiters.is_empty() {
                self.queues.remove(&src_phys);
            }
            pids
        } else {
            Vec::new()
        };

        for pid in pids_to_wake {
            if let Some(proc_arc) = SCHEDULER.get_process(pid) {
                let mut proc = proc_arc.lock();
                proc.unblock();
                count += 1;
            }
        }
        
        // 2. Requeue up to requeue_count remaining waiters
        let waiters_to_requeue = if let Some(waiters) = self.queues.get_mut(&src_phys) {
            let actual_requeue = requeue_count.min(waiters.len());
            let ws: Vec<_> = waiters.drain(..actual_requeue).collect();
            if waiters.is_empty() {
                self.queues.remove(&src_phys);
            }
            ws
        } else {
            Vec::new()
        };

        if !waiters_to_requeue.is_empty() {
            let dst_waiters = self.queues.entry(dst_phys).or_insert_with(Vec::new);
            for w in waiters_to_requeue {
                dst_waiters.push(w);
                count += 1;
            }
        }
        
        count
    }
}

pub static FUTEX_MANAGER: Mutex<FutexManager> = Mutex::new(FutexManager::new());
