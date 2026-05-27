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
            let signum = proc.signals.dequeue();
            if signum == 0 { return; }
            let action = proc.signals.get_action(signum);
            match action {
                SignalAction::Ignore => {}
                SignalAction::Handler(addr) => {
                    // In a real kernel we'd push a signal frame onto the user stack.
                    // Here we just log it.
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

    pub fn total_ticks(&self) -> u64 {
        self.ticks
    }

    pub fn get_process(&self, pid: Pid) -> Option<&Process> {
        self.tasks.iter().filter_map(|t| t.as_ref()).find(|p| p.pid == pid)
    }
}

pub static SCHEDULER: Mutex<Scheduler> = Mutex::new(Scheduler::new());

pub fn tick() {
    SCHEDULER.lock().tick();
}

pub fn spawn(abi: AbiKind, entry: VirtAddr, stack: VirtAddr) -> Option<Pid> {
    SCHEDULER.lock().spawn(abi, entry, stack)
}

pub fn with_process<F, R>(pid: Pid, f: F) -> Option<R>
where
    F: FnOnce(&Process) -> R,
{
    SCHEDULER.lock().get_process(pid).map(f)
}

pub fn with_process_mut<F, R>(pid: Pid, f: F) -> Option<R>
where
    F: FnOnce(&mut Process) -> R,
{
    SCHEDULER.lock().tasks.iter_mut().filter_map(|t| t.as_mut()).find(|p| p.pid == pid).map(f)
}

/// Called from the timer subsystem to wake sleeping processes
pub fn wake_sleeping(woken_mask: u64) {
    SCHEDULER.lock().wake_sleeping_mask(woken_mask);
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
}
