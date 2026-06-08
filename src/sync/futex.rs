//! Physical-address-keyed Futex system
//!
//! Provides inter-process synchronization using memory physical addresses as unique keys.
//! This is critical for synchronizing shared memory (MAP_SHARED) across address spaces.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use spin::Mutex;
use crate::process::Pid;
use crate::process::scheduler::SCHEDULER;

pub struct FutexManager {
    // Map physical address to a list of waiting Pids
    queues: BTreeMap<u64, Vec<Pid>>,
}

impl FutexManager {
    pub const fn new() -> Self {
        Self {
            queues: BTreeMap::new(),
        }
    }

    /// Wait on a futex at the given physical address.
    /// Returns true if it actually blocked, false otherwise.
    pub fn wait(&mut self, phys_addr: u64, expected_val: u32) -> bool {
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

        self.queues.entry(phys_addr).or_insert_with(Vec::new).push(pid);

        // Block the current process
        crate::process::scheduler::with_current_task_mut(|proc| {
            proc.block();
        });
        
        true
    }

    /// Wake processes waiting on a futex at the given physical address.
    /// Returns number of woken processes.
    pub fn wake(&mut self, phys_addr: u64, count: usize) -> usize {
        let mut woken = 0;
        if let Some(waiters) = self.queues.get_mut(&phys_addr) {
            let wake_count = count.min(waiters.len());
            for pid in waiters.drain(..wake_count) {
                if let Some(proc_arc) = SCHEDULER.get_process(pid) {
                    let mut proc = proc_arc.lock();
                    proc.unblock();
                    woken += 1;
                }
            }
            if waiters.is_empty() {
                self.queues.remove(&phys_addr);
            }
        }
        woken
    }
}

pub static FUTEX_MANAGER: Mutex<FutexManager> = Mutex::new(FutexManager::new());
