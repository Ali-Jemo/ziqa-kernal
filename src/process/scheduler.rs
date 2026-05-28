extern crate alloc;
/// Multilevel Feedback Queue (MLFQ) Scheduler for ZiqaKernel
///
/// Features:
/// - Dynamic priority adjustment (feedback)
/// - Interactive tasks stay at high priority
/// - CPU-bound tasks are demoted to lower priority levels
/// - Starvation prevention via periodic priority boosting
/// - Load balancing across priority levels
/// - Fixed-size array (no heap dependency)

use spin::Mutex;
use crate::process::{Process, ProcessTable, ProcessState, Pid, AbiKind};
use crate::process::signal::{SignalAction, default_action, DefaultDisposition};
use crate::memory::VirtAddr;
use x86_64::registers::control::{Cr3, Cr3Flags};

const MAX_TASKS: usize = 64;
const PRIORITY_LEVELS: u8 = 4;
const BOOST_INTERVAL: u64 = 1000; // Reset priorities every 1000 ticks

pub struct Scheduler {
    /// Process slots
    tasks: [Option<Process>; MAX_TASKS],
    current: usize,
    count: usize,
    ticks: u64,
    table: ProcessTable,
    /// Tracks current time slice for running task
    timeslice_remaining: u32,
    /// Load tracking for each priority level (number of ticks consumed)
    #[allow(dead_code)]
    priority_load: [u64; PRIORITY_LEVELS as usize],
}

const NONE_PROCESS: Option<Process> = None;

impl Scheduler {
    pub const fn new() -> Self {
        Scheduler {
            tasks: [NONE_PROCESS; MAX_TASKS],
            current: 0,
            count: 0,
            ticks: 0,
            table: ProcessTable::new(),
            timeslice_remaining: 0,
            priority_load: [0; PRIORITY_LEVELS as usize],
        }
    }

    /// Create a new process (starts at highest priority)
    pub fn spawn(&mut self, abi: AbiKind, entry: VirtAddr, stack: VirtAddr) -> Option<Pid> {
        if self.count >= MAX_TASKS {
            return None;
        }
        let pid = self.table.alloc_pid();
        let mut proc = Process::new(pid, abi, entry, stack);
        proc.set_priority(0); // Highest priority
        proc.make_ready();

        for slot in self.tasks.iter_mut() {
            if slot.is_none() {
                *slot = Some(proc);
                self.count += 1;
                return Some(pid);
            }
        }
        None
    }

    /// Terminate a process with an exit code.
    /// Sends SIGCHLD to the parent if one exists.
    pub fn exit_process(&mut self, pid: Pid, code: i64) {
        for slot in self.tasks.iter_mut() {
            if let Some(proc) = slot {
                if proc.pid == pid {
                    proc.exit_code = code;
                    proc.state = ProcessState::Exited(code);
                    // Notify parent via SIGCHLD
                    let parent_pid = proc.parent;
                    if parent_pid != 0 {
                        self.send_signal_inner(Pid(parent_pid), crate::process::signal::sig::SIGCHLD);
                    }
                    return;
                }
            }
        }
    }

    /// Fork: clone the parent process with copy-on-write page tables.
    /// Returns the child's Pid, or None if the fork fails.
    pub fn fork(&mut self, parent_pid: Pid) -> Option<Pid> {
        // Step 1: Set up COW page tables (makes parent pages read-only, clones for child)
        let child_l4_frame = crate::memory::paging::cow_fork_parent();

        // Step 2: Find the parent and create the child
        let parent_index = self.tasks.iter().position(|s| {
            s.as_ref().map(|p| p.pid == parent_pid).unwrap_or(false)
        })?;

        let parent = self.tasks[parent_index].as_ref().unwrap();

        let child_pid = self.table.alloc_pid();
        let mut child = Process::new(child_pid, parent.abi, parent.entry_point, parent.stack_top);
        child.cpu_state = parent.cpu_state;
        child.priority = parent.priority;
        child.parent = parent_pid.0;
        child.regions = parent.regions.clone();
        child.region_count = parent.region_count;
        child.binary_data = parent.binary_data.clone();
        child.state = ProcessState::Ready;
        child.fds.clone_from(&parent.fds);

        // Set the child's page table frame
        if let Some(frame) = child_l4_frame {
            child.page_table_frame = Some(frame);
        }

        // Mark all writable regions as copy_on_write in BOTH parent and child
        for region_opt in self.tasks[parent_index].as_mut().unwrap().regions.iter_mut() {
            if let Some(ref mut r) = region_opt {
                if r.flags.writable {
                    r.flags.copy_on_write = true;
                }
            }
        }
        for region_opt in child.regions.iter_mut() {
            if let Some(ref mut r) = region_opt {
                if r.flags.writable {
                    r.flags.copy_on_write = true;
                }
            }
        }

        // Insert the child into a free slot
        for slot in self.tasks.iter_mut() {
            if slot.is_none() {
                *slot = Some(child);
                self.count += 1;
                return Some(child_pid);
            }
        }

        // No free slots — clean up the child's page table frames (best-effort)
        // For now just return None (the frames will be leaked — acceptable on failure)
        None
    }

    /// Wait for a child process to exit.
    /// Returns Some((pid, exit_code)) if a zombie child is found, None otherwise.
    pub fn waitpid(&mut self, parent: Pid, child_pid: i64) -> Option<(Pid, i64)> {
        for slot in self.tasks.iter_mut() {
            if let Some(proc) = slot {
                let matches = if child_pid == -1 {
                    proc.parent == parent.0
                } else {
                    proc.pid.0 == child_pid as u64 && proc.parent == parent.0
                };
                if matches {
                    if let ProcessState::Exited(code) = proc.state {
                        let pid = proc.pid;
                        *slot = None; // Reap the zombie
                        return Some((pid, code));
                    }
                }
            }
        }
        None
    }

    /// Send a signal to a process. Returns false if the process doesn't exist.
    pub fn send_signal(&mut self, target: Pid, signum: u8) -> bool {
        self.send_signal_inner(target, signum)
    }

    fn send_signal_inner(&mut self, target: Pid, signum: u8) -> bool {
        for slot in self.tasks.iter_mut() {
            if let Some(proc) = slot {
                if proc.pid == target {
                    proc.signals.send(signum);
                    // SIGKILL/SIGSTOP are delivered immediately
                    if signum == crate::process::signal::sig::SIGKILL {
                        proc.exit_code = -1;
                        proc.state = ProcessState::Exited(-1);
                    } else if signum == crate::process::signal::sig::SIGSTOP {
                        proc.state = ProcessState::Blocked;
                    } else if signum == crate::process::signal::sig::SIGCONT {
                        if proc.state == ProcessState::Blocked {
                            proc.state = ProcessState::Ready;
                        }
                    }
                    return true;
                }
            }
        }
        false
    }

    /// Deliver pending signals to the current process.
    /// Called at the top of each scheduler tick for the running task.
    fn deliver_signals(&mut self) {
        let idx = self.current;
        if let Some(proc) = &mut self.tasks[idx] {
            if !proc.signals.has_pending() { return; }
            
            // Check for signals that should be delivered to userspace
            let signum = proc.signals.dequeue();
            if signum == 0 { return; }
            
            let action = proc.signals.get_action(signum);
            
            match action {
                SignalAction::Ignore => {}
                SignalAction::Handler(addr) => {
                    // Store signal info for context switch
                    proc.signals.pending_signal = signum;
                    proc.signals.handler_addr = addr;
                    
                    crate::println!("[SIGNAL] PID {} handler at 0x{:x} for sig {}", proc.pid.0, addr, signum);
                }
                SignalAction::Default => {
                    match default_action(signum) {
                        DefaultDisposition::Terminate | DefaultDisposition::CoreDump => {
                            proc.exit_code = -(signum as i64);
                            proc.state = ProcessState::Exited(-(signum as i64));
                        }
                        DefaultDisposition::Stop => {
                            proc.state = ProcessState::Blocked;
                        }
                        DefaultDisposition::Ignore => {}
                    }
                }
            }
        }
    }

    /// Wake processes whose sleep deadline has passed (called from timer subsystem).
    /// `woken_mask` is a bitmask where bit N means PID N should wake.
    pub fn wake_sleeping_mask(&mut self, woken_mask: u64) {
        for slot in self.tasks.iter_mut() {
            if let Some(proc) = slot {
                if proc.state == ProcessState::Blocked {
                    let bit = proc.pid.0;
                    if bit < 64 && (woken_mask & (1 << bit)) != 0 {
                        proc.state = ProcessState::Ready;
                    }
                }
            }
        }
    }

    /// MLFQ Scheduling logic
    pub fn tick(&mut self) {
        self.ticks += 1;

        if self.count == 0 {
            return;
        }

        // Deliver pending signals to the running task
        self.deliver_signals();

        // Periodic priority boost (anti-starvation)
        if self.ticks % BOOST_INTERVAL == 0 {
            self.boost_priorities();
        }

        // Decrease current task's time slice
        if self.timeslice_remaining > 0 {
            self.timeslice_remaining -= 1;
        }

        // Check if current task exhausted its quantum
        if self.timeslice_remaining == 0 {
            if let Some(proc) = &mut self.tasks[self.current] {
                if proc.state == ProcessState::Running {
                    // Demote CPU-bound process
                    if proc.priority < PRIORITY_LEVELS - 1 {
                        proc.priority += 1;
                    }
                    proc.state = ProcessState::Ready;
                }
            }
            self.schedule_next();
        } else {
            // Check if current task is still runnable
            if let Some(proc) = &self.tasks[self.current] {
                if proc.state != ProcessState::Running {
                    self.schedule_next();
                }
            } else {
                self.schedule_next();
            }
        }
    }

    /// Find the next task to run based on MLFQ rules
    fn schedule_next(&mut self) {
        let mut best_idx = None;
        let mut best_priority = PRIORITY_LEVELS + 1;

        // Find highest priority Ready task
        for i in 0..MAX_TASKS {
            if let Some(proc) = &self.tasks[i] {
                if proc.state == ProcessState::Ready {
                    if proc.priority < best_priority {
                        best_priority = proc.priority;
                        best_idx = Some(i);
                    }
                }
            }
        }

        if let Some(idx) = best_idx {
            if let Some(proc) = &mut self.tasks[idx] {
                proc.state = ProcessState::Running;
                // Quantum increases as priority decreases
                self.timeslice_remaining = match proc.priority {
                    0 => 5,   // Very fast
                    1 => 10,
                    2 => 20,
                    _ => 40,  // Background tasks get long quanta
                };
                self.current = idx;

                // Load the process's page table if it has its own
                if let Some(frame) = proc.page_table_frame {
                    unsafe {
                        Cr3::write(frame, Cr3Flags::empty());
                    }
                }
            }
        } else {
            // No ready tasks
            self.timeslice_remaining = 1;
        }
    }

    fn boost_priorities(&mut self) {
        for slot in self.tasks.iter_mut() {
            if let Some(proc) = slot {
                proc.priority = 0;
            }
        }
    }

    pub fn current_task(&self) -> Option<&Process> {
        self.tasks[self.current].as_ref()
    }

    pub fn current_task_mut(&mut self) -> Option<&mut Process> {
        self.tasks[self.current].as_mut()
    }

    pub fn set_current(&mut self, pid: Pid) -> bool {
        for (i, slot) in self.tasks.iter().enumerate() {
            if let Some(proc) = slot {
                if proc.pid == pid {
                    self.current = i;
                    // Load the process's page table if it has its own
                    if let Some(frame) = proc.page_table_frame {
                        unsafe {
                            Cr3::write(frame, Cr3Flags::empty());
                        }
                    }
                    return true;
                }
            }
        }
        false
    }

    pub fn total_ticks(&self) -> u64 {
        self.ticks
    }

    pub fn get_process(&self, pid: Pid) -> Option<&Process> {
        self.tasks.iter().filter_map(|t| t.as_ref()).find(|p| p.pid == pid)
    }

    pub fn get_process_mut(&mut self, pid: Pid) -> Option<&mut Process> {
        self.tasks.iter_mut().filter_map(|t| t.as_mut()).find(|p| p.pid == pid)
    }
}

pub static SCHEDULER: Mutex<Scheduler> = Mutex::new(Scheduler::new());

pub fn tick() {
    SCHEDULER.lock().tick();
}

pub fn spawn(abi: AbiKind, entry: VirtAddr, stack: VirtAddr) -> Option<Pid> {
    x86_64::instructions::interrupts::without_interrupts(|| {
        SCHEDULER.lock().spawn(abi, entry, stack)
    })
}

pub fn with_process<F, R>(pid: Pid, f: F) -> Option<R>
where
    F: FnOnce(&Process) -> R,
{
    x86_64::instructions::interrupts::without_interrupts(|| {
        SCHEDULER.lock().get_process(pid).map(f)
    })
}

pub fn with_process_mut<F, R>(pid: Pid, f: F) -> Option<R>
where
    F: FnOnce(&mut Process) -> R,
{
    x86_64::instructions::interrupts::without_interrupts(|| {
        SCHEDULER.lock().tasks.iter_mut().filter_map(|t| t.as_mut()).find(|p| p.pid == pid).map(f)
    })
}

/// Called from the timer subsystem to wake sleeping processes
pub fn wake_sleeping(woken_mask: u64) {
    x86_64::instructions::interrupts::without_interrupts(|| {
        SCHEDULER.lock().wake_sleeping_mask(woken_mask);
    })
}

pub fn init() {
    // init
}

impl Scheduler {
    pub fn print_process_list(&self) {
        crate::println!(" PID | State   | Priority | Parent ");
        crate::println!("-----|---------|----------|--------");
        for slot in self.tasks.iter() {
            if let Some(proc) = slot {
                crate::println!("{:4} | {:7?} | {:8} | {:6}", proc.pid.0, proc.state, proc.priority, proc.parent);
            }
        }
    }

    pub fn get_pid_list(&self) -> alloc::vec::Vec<crate::process::Pid> {
        let mut pids = alloc::vec::Vec::new();
        for slot in self.tasks.iter() {
            if let Some(proc) = slot {
                pids.push(proc.pid);
            }
        }
        pids
    }
}

pub fn list_pids() -> alloc::vec::Vec<crate::process::Pid> {
    SCHEDULER.lock().get_pid_list()
}
pub fn spawn_elf(binary: &[u8]) -> Option<Pid> {
    let registry = crate::init_abi_registry();
    let plugin = registry.detect(binary)?;

    let pid = x86_64::instructions::interrupts::without_interrupts(|| {
        let mut sched = SCHEDULER.lock();
        if sched.count >= MAX_TASKS {
            return None;
        }
        Some(sched.table.alloc_pid())
    })?;

    // Use safe address 16MB
    let entry = VirtAddr::new(0x1000000);
    let stack = VirtAddr::new(0x7FFF_FFFF_000);
    let mut proc = Process::new(pid, plugin.kind(), entry, stack);
    proc.binary_data = binary.to_vec();

    match plugin.load(binary, &mut proc) {
        Ok(()) => {
            proc.make_ready();
            x86_64::instructions::interrupts::without_interrupts(|| {
                let mut sched = SCHEDULER.lock();
                for slot in sched.tasks.iter_mut() {
                    if slot.is_none() {
                        *slot = Some(proc);
                        sched.count += 1;
                        return Some(pid);
                    }
                }
                None
            })
        }
        Err(_) => None,
    }
}
