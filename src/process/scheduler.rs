extern crate alloc;
use alloc::collections::BTreeMap;
use crate::arch::x86_64::{init_kthread_stack, kthread_trampoline_wrapper};
use crate::memory::VirtAddr;
use crate::process::{AbiKind, Pid, Process, ProcessState, ProcessTable as PidAllocator};
use crate::process::vma::Vma;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::{Mutex, RwLock};
use x86_64::registers::control::{Cr3, Cr3Flags};
use x86_64::structures::paging::FrameAllocator;

const PRIORITY_LEVELS: u8 = 4;
const BOOST_INTERVAL: u64 = 1000;

/// The global process table, allowing concurrent access to different processes.
pub struct GlobalProcessTable {
    pub tasks: BTreeMap<Pid, Arc<Mutex<Process>>>,
}

impl GlobalProcessTable {
    pub const fn new() -> Self {
        Self {
            tasks: BTreeMap::new(),
        }
    }

    pub fn get(&self, pid: Pid) -> Option<Arc<Mutex<Process>>> {
        self.tasks.get(&pid).cloned()
    }
}

/// Ready queues for Multi-Level Feedback Queue (MLFQ) scheduling.
struct ReadyQueues {
    queues: [Vec<Pid>; PRIORITY_LEVELS as usize],
}

impl ReadyQueues {
    pub const fn new() -> Self {
        Self {
            queues: [Vec::new(), Vec::new(), Vec::new(), Vec::new()],
        }
    }

    pub fn push(&mut self, pid: Pid, priority: u8) {
        let p = priority.min(PRIORITY_LEVELS - 1) as usize;
        if !self.queues[p].contains(&pid) {
            self.queues[p].push(pid);
        }
    }

    pub fn pop_highest(&mut self) -> Option<Pid> {
        for q in self.queues.iter_mut() {
            if !q.is_empty() {
                return Some(q.remove(0));
            }
        }
        None
    }
}

/// The Scalable Scheduler for ZiqaKernel.
pub struct Scheduler {
    pub process_table: RwLock<GlobalProcessTable>,
    ready_queues: Mutex<ReadyQueues>,
    ticks: AtomicU64,
    pid_allocator: Mutex<PidAllocator>,
}

impl Scheduler {
    pub const fn new() -> Self {
        Scheduler {
            process_table: RwLock::new(GlobalProcessTable::new()),
            ready_queues: Mutex::new(ReadyQueues::new()),
            ticks: AtomicU64::new(0),
            pid_allocator: Mutex::new(PidAllocator::new()),
        }
    }

    pub fn current_pid(&self) -> Option<Pid> {
        crate::arch::x86_64::per_cpu::current_cpu().current_pid()
    }

    pub fn set_current_pid(&self, pid: Option<Pid>) {
        crate::arch::x86_64::per_cpu::current_cpu_mut().set_current_pid(pid);
    }

    pub fn total_ticks(&self) -> u64 {
        self.ticks.load(Ordering::Relaxed)
    }

    pub fn spawn(&self, abi: AbiKind, entry: VirtAddr, stack: VirtAddr) -> Option<Pid> {
        let pid = self.pid_allocator.lock().alloc_pid();
        let mut proc = Process::new(pid, abi, entry, stack);
        proc.set_priority(0);

        if let Some(frame) = crate::memory::paging::create_process_page_table() {
            proc.page_table_frame = Some(frame);
        }

        let stack_size = 256 * 1024;
        let stack_start = stack.as_u64().checked_sub(stack_size as u64)?;
        proc.add_region(Vma::from(crate::memory::MemoryRegion {
            start: VirtAddr::new(stack_start),
            size: stack_size as usize,
            flags: crate::memory::paging::MemoryRegionFlags::read_write(),
            is_file_backed: false,
            file_path: None,
            file_offset: 0,
            bco_hook: None,
        }));

        init_process_stack(&mut proc);
        proc.make_ready();

        let mut table = self.process_table.write();
        table.tasks.insert(pid, Arc::new(Mutex::new(proc)));
        self.ready_queues.lock().push(pid, 0);
        Some(pid)
    }

    pub fn spawn_kthread(&self, entry: fn(*const ()), arg: *const ()) -> Option<Pid> {
        let pid = self.pid_allocator.lock().alloc_pid();
        // The kthread's first RIP is the asm trampoline (defined below),
        // which forwards r12=arg and r13=entry to the user entry and exits
        // cleanly when the entry returns. `entry_point` is the only field
        // the kernel uses to find the initial RIP, so we set it to the
        // trampoline address.
        let trampoline = kthread_trampoline_wrapper as *const () as u64;
        let mut proc = Process::new(pid, AbiKind::ZiqaNative, VirtAddr::new(trampoline), VirtAddr::new(0));
        // Kthreads share the kernel's CS/SS and stay in ring 0; we never
        // iretq out to user mode.
        proc.cpu_state.cs = 0x8;
        proc.cpu_state.ss = 0x10;
        proc.cpu_state.rflags = 0x202;
        proc.page_table_frame = None;
        proc.make_ready();
        init_kthread_stack(&mut proc, entry as u64, arg as u64);

        let mut table = self.process_table.write();
        table.tasks.insert(pid, Arc::new(Mutex::new(proc)));
        self.ready_queues.lock().push(pid, 0);
        Some(pid)
    }

    pub fn spawn_elf(&self, binary: &[u8]) -> Option<Pid> {
        let registry = crate::init_abi_registry();
        let plugin = registry.detect(binary)?;
        let pid = self.pid_allocator.lock().alloc_pid();
        let entry = VirtAddr::new(0x1000000); // Dummy, will be overwritten by load_elf
        let stack = VirtAddr::new(0x7FFF_FFFF_000);
        let mut proc = Process::new(pid, plugin.kind(), entry, stack);
        proc.binary_data = binary.to_vec();

        if let Some(frame) = crate::memory::paging::create_process_page_table() {
            proc.page_table_frame = Some(frame);
        }

        let stack_size = 256 * 1024;
        let stack_start = stack.as_u64().checked_sub(stack_size as u64).unwrap_or(0);
        proc.add_region(Vma::from(crate::memory::MemoryRegion {
            start: VirtAddr::new(stack_start),
            size: stack_size as usize,
            flags: crate::memory::paging::MemoryRegionFlags::read_write(),
            is_file_backed: false,
            file_path: None,
            file_offset: 0,
            bco_hook: None,
        }));

        match plugin.load(binary, &mut proc) {
            Ok(()) => {
                init_process_stack(&mut proc);
                proc.make_ready();
                let mut table = self.process_table.write();
                table.tasks.insert(pid, Arc::new(Mutex::new(proc)));
                self.ready_queues.lock().push(pid, 0);
                Some(pid)
            }
            Err(_) => None,
        }
    }

    pub fn ptrace_read_mem(&self, pid: Pid, addr: u64, buf: &mut [u8]) -> bool {
        let proc_arc = match self.get_process(pid) {
            Some(p) => p,
            None => return false,
        };
        let proc = proc_arc.lock();
        let target_frame = match proc.page_table_frame {
            Some(f) => f,
            None => return false, // Cannot access shared kernel table via ptrace
        };

        x86_64::instructions::interrupts::without_interrupts(|| {
            let old_cr3 = Cr3::read();
            unsafe { Cr3::write(target_frame, Cr3Flags::empty()); }
            
            let slice = unsafe { core::slice::from_raw_parts(addr as *const u8, buf.len()) };
            buf.copy_from_slice(slice);

            unsafe { Cr3::write(old_cr3.0, old_cr3.1); }
        });
        true
    }

    pub fn ptrace_write_mem(&self, pid: Pid, addr: u64, buf: &[u8]) -> bool {
        let proc_arc = match self.get_process(pid) {
            Some(p) => p,
            None => return false,
        };
        let mut proc = proc_arc.lock();
        let target_frame = match proc.page_table_frame {
            Some(f) => f,
            None => return false, // Cannot access shared kernel table via ptrace
        };

        x86_64::instructions::interrupts::without_interrupts(|| {
            let old_cr3 = Cr3::read();
            unsafe { Cr3::write(target_frame, Cr3Flags::empty()); }
            
            let slice = unsafe { core::slice::from_raw_parts_mut(addr as *mut u8, buf.len()) };
            slice.copy_from_slice(buf);

            unsafe { Cr3::write(old_cr3.0, old_cr3.1); }
        });
        true
    }

    pub fn exit_process(&self, pid: Pid, code: i64) {
        // The set of PIDs waiting to join the exiting process; we drain it
        // from `join_waiters` so they can all be unblocked atomically.
        let waiters = {
            let proc_arc = match self.get_process(pid) {
                Some(p) => p,
                None => return,
            };
            let mut proc = proc_arc.lock();
            proc.exit_code = code;
            proc.state = ProcessState::Exited(code);
            let parent_pid = proc.parent;
            if parent_pid != 0 {
                self.send_signal(Pid(parent_pid), crate::process::signal::sig::SIGCHLD);
            }
            core::mem::replace(&mut proc.join_waiters, alloc::vec::Vec::new())
        };

        for waiter_pid in waiters {
            if let Some(proc_mutex) = self.get_process(Pid(waiter_pid)) {
                let mut proc = proc_mutex.lock();
                proc.make_ready();
                self.ready_queues.lock().push(proc.pid, proc.priority);
            }
        }
        
        if with_current_task(|p| p.pid) == Some(pid) {
            self.schedule();
        }
    }

    pub fn join_kthread(&self, pid: Pid) -> i64 {
        let current_pid = with_current_task(|p| p.pid).unwrap();
        
        {
            let proc_arc = self.get_process(pid).expect("Process not found");
            let mut proc = proc_arc.lock();
            if let ProcessState::Exited(code) = proc.state {
                return code;
            }
            proc.join_waiters.push(current_pid.0);
        }

        // Block current task
        self.block_current_task();
        0
    }

    pub fn cancel_kthread(&self, pid: Pid) -> bool {
        let proc_arc = match self.get_process(pid) {
            Some(p) => p,
            None => return false,
        };
        
        let mut proc = proc_arc.lock();
        if let ProcessState::Exited(_) = proc.state {
            return false; // Already exited
        }
        proc.state = ProcessState::Canceled;
        
        drop(proc);
        self.exit_process(pid, -1);
        true
    }

    /// Mark the currently running task as Blocked and yield the CPU.
    ///
    /// The caller is expected to have already registered itself on whatever
    /// wait-list it is blocking on (e.g. `proc.join_waiters` for `join_kthread`).
    /// We transition to Blocked, then call `schedule()` so the next ready
    /// task can take over. The caller will be re-marked Ready by whoever
    /// wakes it (e.g. `exit_process` pops waiters and pushes them back).
    pub fn block_current_task(&self) {
        // We MUST disable interrupts while flipping our state to Blocked;
        // otherwise a timer tick could observe the inconsistent state.
        x86_64::instructions::interrupts::without_interrupts(|| {
            let current_pid = match self.current_pid() {
                Some(p) => p,
                None => return,
            };
            if let Some(proc_arc) = self.get_process(current_pid) {
                let mut proc = proc_arc.lock();
                if matches!(proc.state, ProcessState::Running) {
                    proc.state = ProcessState::Blocked;
                }
            }
            self.schedule();
        });
    }

    /// Exit the currently running kthread with the given exit code.
    ///
    /// Kthreads call this when their entry function returns. Unlike user
    /// processes, kthreads share the kernel's address space, so we don't
    /// need a iretq back to ring 3 — the call to `exit_process` invokes
    /// `schedule()` which never returns to us.
    pub fn exit_current_kthread(&self, code: i64) -> ! {
        let current_pid = with_current_task(|p| p.pid);
        if let Some(pid) = current_pid {
            self.exit_process(pid, code);
        }
        // If we somehow got here without a current pid (defensive),
        // spin — the scheduler will pick us up on the next tick.
        loop {
            unsafe { core::arch::asm!("hlt"); }
        }
    }


    pub fn fork(&self, parent_pid: Pid) -> Option<Pid> {
        x86_64::instructions::interrupts::without_interrupts(|| {
            let child_l4_frame = crate::memory::paging::cow_fork_parent();
            let parent_arc = self.get_process(parent_pid)?;
            let mut parent = parent_arc.lock();

            let child_pid = self.pid_allocator.lock().alloc_pid();
            let mut child = Process::new(child_pid, parent.abi, parent.entry_point, parent.stack_top);
            child.cpu_state = parent.cpu_state;
            child.priority = parent.priority;
            child.parent = parent_pid.0;
            child.vmas = parent.vmas.clone();
            child.binary_data = parent.binary_data.clone();
            child.state = ProcessState::Ready;
            child.fds.clone_from(&parent.fds);

            if let Some(frame) = child_l4_frame {
                child.page_table_frame = Some(frame);
            }

            for vma in parent.vmas.iter_mut() {
                if vma.flags.writable { vma.flags.copy_on_write = true; }
            }
            for vma in child.vmas.iter_mut() {
                if vma.flags.writable { vma.flags.copy_on_write = true; }
            }

            let mut table = self.process_table.write();
            table.tasks.insert(child_pid, Arc::new(Mutex::new(child)));
            self.ready_queues.lock().push(child_pid, 0);
            Some(child_pid)
        })
    }

    pub fn waitpid(&self, parent: Pid, child_pid: i64, _options: i32) -> Option<(Pid, i64)> {
        x86_64::instructions::interrupts::without_interrupts(|| {
            let table = self.process_table.read();
            for (&pid, proc_arc) in table.tasks.iter() {
                let proc = proc_arc.lock();
                let matches = if child_pid == -1 {
                    proc.parent == parent.0
                } else {
                    proc.pid.0 == child_pid as u64 && proc.parent == parent.0
                };

                if matches {
                    if let ProcessState::Exited(code) = proc.state {
                        let pid = proc.pid;
                        drop(proc);
                        drop(table);
                        
                        let mut write_table = self.process_table.write();
                        write_table.tasks.remove(&pid);
                        return Some((pid, code));
                    }
                }
            }
            None
        })
    }

    pub fn send_signal(&self, target: Pid, signum: u8) -> bool {
        if let Some(proc_arc) = self.get_process(target) {
            let mut proc = proc_arc.lock();
            proc.signals.send(signum);
            if signum == crate::process::signal::sig::SIGKILL {
                proc.exit_code = -1;
                proc.state = ProcessState::Exited(-1);
            } else if signum == crate::process::signal::sig::SIGSTOP {
                proc.state = ProcessState::Blocked;
            } else if signum == crate::process::signal::sig::SIGCONT {
                if proc.state == ProcessState::Blocked {
                    proc.state = ProcessState::Ready;
                    self.ready_queues.lock().push(target, proc.priority);
                }
            }
            return true;
        }
        false
    }

    /// Called from the APIC/PIT timer ISR (interrupt context).
    /// MUST NOT block on a process lock — the kernel often holds a process
    /// lock with interrupts enabled, and an ISR that spins on it is a
    /// classic hard deadlock (the holder can never run to release the lock
    /// because this ISR is hogging the CPU). Use `try_lock` and skip work
    /// we can't grab right now — the next tick will retry.
    pub fn tick(&self) {
        let t = self.ticks.fetch_add(1, Ordering::Relaxed) + 1;

        if let Some(pid) = self.current_pid() {
            if let Some(proc_arc) = self.get_process(pid) {
                if let Some(mut proc) = proc_arc.try_lock() {
                    if t % 10 == 0 {
                        if proc.priority < PRIORITY_LEVELS - 1 {
                            proc.priority += 1;
                        }
                    }
                }
                // else: another CPU/context holds the lock; drop the tick's
                // MLFQ promotion and let the next tick handle it. Process
                // state is otherwise consistent because we never mutated it.
            }
        }

        if t % BOOST_INTERVAL == 0 {
            let table = self.process_table.read();
            for p in table.tasks.values() {
                if let Some(mut proc) = p.try_lock() {
                    proc.priority = 0;
                }
            }
        }
    }

    pub fn schedule(&self) {
        let interrupts_enabled = x86_64::instructions::interrupts::are_enabled();
        if interrupts_enabled {
            x86_64::instructions::interrupts::disable();
        }

        let next_pid = self.ready_queues.lock().pop_highest();
        
        if let Some(new_pid) = next_pid {
            let old_pid = self.current_pid();
            
            if old_pid == Some(new_pid) {
                if interrupts_enabled {
                    x86_64::instructions::interrupts::enable();
                }
                return;
            }

            self.set_current_pid(Some(new_pid));

            let old_proc_arc = old_pid.and_then(|id| self.get_process(id));
            let new_proc_arc = self.get_process(new_pid).expect("Ready task missing from table");
            let new_cr3 = {
                let mut new_proc = new_proc_arc.lock();
                new_proc.state = ProcessState::Running;
                new_proc.page_table_frame
            };
            let new_kstack_top = new_proc_arc.lock().kernel_stack_top;
            let new_sp = new_proc_arc.lock().kernel_stack_ptr;
            if let Some(frame) = new_cr3 {
                unsafe { Cr3::write(frame, Cr3Flags::empty()); }
            }
            crate::arch::x86_64::update_trap_stacks(new_kstack_top);
            if let Some(old_arc) = old_proc_arc {
                let old_sp_ptr = {
                    let mut old_proc = old_arc.lock();
                    unsafe { crate::arch::x86_64::save_fpu(&mut old_proc.fpu_state); }
                    if old_proc.state == ProcessState::Running {
                        old_proc.state = ProcessState::Ready;
                        self.ready_queues.lock().push(old_proc.pid, old_proc.priority);
                    }
                    &mut old_proc.kernel_stack_ptr as *mut u64
                };
                unsafe {
                    crate::arch::x86_64::switch::switch_context(
                        old_sp_ptr,
                        new_sp,
                    );
                }
            } else {
                let mut dummy: u64 = 0;
                unsafe {
                    crate::arch::x86_64::switch::switch_context(
                        &mut dummy as *mut u64,
                        new_sp,
                    );
                }
            }
            
            // Restore FPU state for the newly scheduled process
            if let Some(current_pid) = self.current_pid() {
                if let Some(proc_arc) = self.get_process(current_pid) {
                    let proc = proc_arc.lock();
                    unsafe { crate::arch::x86_64::restore_fpu(&proc.fpu_state); }
                }
            }
        }

        if interrupts_enabled {
            x86_64::instructions::interrupts::enable();
        }
    }

    pub fn get_process(&self, pid: Pid) -> Option<Arc<Mutex<Process>>> {
        self.process_table.read().get(pid)
    }

    pub fn wake_sleeping_mask(&self, woken_mask: u64) {
        let table = self.process_table.read();
        for p in table.tasks.values() {
            let mut proc = p.lock();
            if proc.state == ProcessState::Blocked {
                let bit = proc.pid.0;
                if bit < 64 && (woken_mask & (1 << bit)) != 0 {
                    proc.state = ProcessState::Ready;
                    self.ready_queues.lock().push(proc.pid, proc.priority);
                }
            }
        }
    }

    pub fn set_current(&self, pid: Pid) -> bool {
        if self.get_process(pid).is_some() {
            self.set_current_pid(Some(pid));
            true
        } else {
            false
        }
    }

    pub fn list_pids(&self) -> Vec<Pid> {
        let table = self.process_table.read();
        table.tasks.keys().cloned().collect()
    }

    pub fn init_boot_process(&self) {
        x86_64::instructions::interrupts::without_interrupts(|| {
            let pid = self.pid_allocator.lock().alloc_pid();
            let mut proc = Process::new(pid, AbiKind::ZiqaNative, VirtAddr::new(0), VirtAddr::new(0));
            let kstack = alloc::vec![0u8; 65536];
            let top = kstack.as_ptr() as u64 + 65536;
            proc.kernel_stack = Some(kstack);
            proc.kernel_stack_top = top;
            proc.state = ProcessState::Running;
            
            let mut table = self.process_table.write();
            table.tasks.insert(pid, Arc::new(Mutex::new(proc)));
            self.set_current_pid(Some(pid));
        });
    }
}

pub static SCHEDULER: Scheduler = Scheduler::new();

pub fn tick() { SCHEDULER.tick(); }
pub fn spawn(abi: AbiKind, entry: VirtAddr, stack: VirtAddr) -> Option<Pid> { x86_64::instructions::interrupts::without_interrupts(|| SCHEDULER.spawn(abi, entry, stack)) }
pub fn spawn_kthread(entry: fn(*const ()), arg: *const ()) -> Option<Pid> { x86_64::instructions::interrupts::without_interrupts(|| SCHEDULER.spawn_kthread(entry, arg)) }
pub fn spawn_elf(binary: &[u8]) -> Option<Pid> { x86_64::instructions::interrupts::without_interrupts(|| SCHEDULER.spawn_elf(binary)) }
pub fn with_process<F, R>(pid: Pid, f: F) -> Option<R> where F: FnOnce(&Process) -> R, { x86_64::instructions::interrupts::without_interrupts(|| SCHEDULER.get_process(pid).map(|p| f(&p.lock()))) }
pub fn with_process_mut<F, R>(pid: Pid, f: F) -> Option<R> where F: FnOnce(&mut Process) -> R, { x86_64::instructions::interrupts::without_interrupts(|| SCHEDULER.get_process(pid).map(|p| f(&mut p.lock()))) }
pub fn wake_sleeping(woken_mask: u64) { x86_64::instructions::interrupts::without_interrupts(|| SCHEDULER.wake_sleeping_mask(woken_mask)); }
// ── Kernel-thread public API ──────────────────────────────────────────────────
//
// Free-function shims around the scheduler's kthread methods. These exist
// so the rest of the kernel (drivers, IP stack, worker pool) can spawn,
// join, and cancel kthreads without having to lock `SCHEDULER` directly.
// All of them wrap the underlying call in `without_interrupts` so the
// process state and ready queues are updated atomically.

/// Block the current task until `pid` exits (or is canceled) and return
/// its exit code. Returns `-1` if the pid is unknown.
pub fn join_kthread(pid: Pid) -> i64 {
    x86_64::instructions::interrupts::without_interrupts(|| SCHEDULER.join_kthread(pid))
}

/// Force a kthread (or any task) into `Canceled` state and unblock its
/// joiners. Returns `false` if the pid is unknown or already exited.
pub fn cancel_kthread(pid: Pid) -> bool {
    x86_64::instructions::interrupts::without_interrupts(|| SCHEDULER.cancel_kthread(pid))
}

/// Voluntarily terminate the current kthread. Never returns.
pub fn exit_current_kthread(code: i64) -> ! {
    // No IRQ-off wrapper needed — `exit_current_kthread` itself disables
    // interrupts before flipping state and calling `schedule`.
    SCHEDULER.exit_current_kthread(code)
}

/// One-shot scheduler bootstrap. Initializes the PID allocator and installs
/// PID 0 (the boot/idle process) into the process table.
pub fn init() { SCHEDULER.init_boot_process(); }



// ── Safe current-task accessors ───────────────────────────────────────────────
//
// `current_task()` / `current_task_mut()` return an `Arc<Mutex<Process>>` whose
// lock is then held by the caller **with interrupts enabled**. If the APIC
// timer ISR fires before the caller drops the lock, `scheduler::tick()` would
// try to re-lock the same process and spin forever, deadlocking the kernel.
//
// Use the `with_current_task` / `with_current_task_mut` helpers below instead:
// they run the closure with interrupts disabled AND the process lock held, so
// the lock is never observable to the ISR in an inconsistent state.
//
// The raw `current_task` accessors are kept for now (with a `try_lock` flavor
// for ISR context) but new code should prefer the closure-based helpers.

/// Run `f` with a shared borrow of the current process. Interrupts are
/// disabled for the duration of the call so the timer ISR can't observe
/// (or deadlock on) the process lock.
pub fn with_current_task<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&Process) -> R,
{
    x86_64::instructions::interrupts::without_interrupts(|| {
        let pid = SCHEDULER.current_pid()?;
        let proc_arc = SCHEDULER.get_process(pid)?;
        let proc = proc_arc.lock();
        Some(f(&proc))
    })
}

/// Run `f` with an exclusive borrow of the current process. See
/// `with_current_task` for the rationale.
pub fn with_current_task_mut<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&mut Process) -> R,
{
    x86_64::instructions::interrupts::without_interrupts(|| {
        let pid = SCHEDULER.current_pid()?;
        let proc_arc = SCHEDULER.get_process(pid)?;
        let mut proc = proc_arc.lock();
        Some(f(&mut proc))
    })
}

/// Try to lock the current process without blocking. Returns `None` if the
/// lock is contended — safe to call from interrupt context.
pub fn try_current_task<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&mut Process) -> R,
{
    let pid = SCHEDULER.current_pid()?;
    let proc_arc = SCHEDULER.get_process(pid)?;
    // spin::Mutex::try_lock returns `Option<MutexGuard>` (not Result), so
    // we pattern-match directly.
    let mut proc = proc_arc.try_lock()?;
    Some(f(&mut proc))
}

/// Read-only access to a non-current process. Safe in any context.
pub fn current_task() -> Option<Arc<Mutex<Process>>> {
    x86_64::instructions::interrupts::without_interrupts(|| {
        SCHEDULER.current_pid().and_then(|pid| SCHEDULER.get_process(pid))
    })
}

/// **Unsafe** raw accessor for the current process — kept only for legacy
/// callers. The returned `Arc<Mutex<Process>>` lock is held with interrupts
/// ENABLED, which can deadlock against the APIC timer ISR if held for any
/// non-trivial work. Prefer `with_current_task` / `with_current_task_mut`.
#[deprecated(note = "process lock is held with IRQs on; can deadlock against timer ISR. Use with_current_task(_mut) instead.")]
pub fn current_task_mut() -> Option<Arc<Mutex<Process>>> { current_task() }
pub fn list_pids() -> Vec<Pid> { x86_64::instructions::interrupts::without_interrupts(|| SCHEDULER.list_pids()) }
pub fn yield_now() { unsafe { core::arch::asm!("int 0x20"); } }

/// Replace a process with a new ELF binary (execve).
/// This clears old mappings, loads the new binary, and resets CPU state.
pub fn exec_process(
    pid: Pid,
    binary: &[u8],
    _args: &[&[u8]],
    _env: &[&[u8]],
) -> Result<(), &'static str> {
    x86_64::instructions::interrupts::without_interrupts(|| {
        let registry = crate::init_abi_registry();
        let plugin = registry.detect(binary).ok_or("exec: unrecognized binary format")?;

        let proc_arc = SCHEDULER.get_process(pid).ok_or("exec: process not found")?;
        let mut proc = proc_arc.lock();

        // Save old page table frame if using per-process tables
        let old_frame = proc.page_table_frame;

        // Create a fresh page table for the new address space
        if let Some(frame) = crate::memory::paging::create_process_page_table() {
            proc.page_table_frame = Some(frame);
        } else {
            proc.page_table_frame = None;
        }

        // Clear old VMAs
        proc.vmas.clear();

        // Reset binary data for demand paging
        proc.binary_data = binary.to_vec();
        proc.cpu_state = crate::process::CpuState::zero();

        // Set up initial process state
        proc.stack_top = VirtAddr::new(0x7FFF_FFFF_000);
        proc.brk = 0x2000_0000;
        proc.mmap_bump = 0x7000_0000;

        // Load the new ELF
        plugin.load(binary, &mut proc).map_err(|_| "exec: ELF load failed")?;

        // Reinit process stack
        init_process_stack(&mut proc);

        // Set the entry point in CPU state
        proc.cpu_state.rip = proc.entry_point.as_u64();
        proc.cpu_state.rsp = proc.stack_top.as_u64();
        proc.cpu_state.cs = crate::arch::x86_64::gdt::user_code_selector().0 as u64;
        proc.cpu_state.ss = crate::arch::x86_64::gdt::user_data_selector().0 as u64;
        proc.cpu_state.rflags = 0x202;

        // Set argv/envp on the stack (simplified: just push null)
        proc.state = ProcessState::Ready;

        // Free old page table frames (best-effort)
        let _ = old_frame;

        Ok(())
    })
}

/// asm-friendly alias used by the trampoline to terminate the kthread.
/// Implemented in Rust so it can call into the scheduler without needing a
/// Rust→asm→Rust shim.
#[no_mangle]
pub extern "C" fn kthread_exit_trampoline() -> ! {
    SCHEDULER.exit_current_kthread(0)
}

/// `kthread_exit` — voluntarily terminate the current kthread.
///
/// Equivalent to `exit(0)` for a user process, but skips the user-stack
/// unwinding (kthreads don't have one). Use this from a kthread entry
/// function as an alternative to returning.
pub fn kthread_exit(code: i64) -> ! {
    SCHEDULER.exit_current_kthread(code)
}

fn init_process_stack(proc: &mut Process) {
    let stack_top = proc.stack_top.as_u64();
    if stack_top != 0 {
        let page_addr = (stack_top - 4096) & !0xFFF;
        let page = x86_64::structures::paging::Page::containing_address(VirtAddr::new(page_addr));
        let mut fa_guard = crate::memory::FRAME_ALLOCATOR.lock();
        let fa = fa_guard.as_mut().unwrap();
        let flags = x86_64::structures::paging::PageTableFlags::PRESENT | x86_64::structures::paging::PageTableFlags::WRITABLE | x86_64::structures::paging::PageTableFlags::USER_ACCESSIBLE;
        if let Some(frame) = fa.allocate_frame() {
            unsafe {
                use x86_64::structures::paging::Mapper;
                if let Some(pt_frame) = proc.page_table_frame {
                    let po = crate::memory::paging::phys_offset();
                    let l4_virt = po + pt_frame.start_address().as_u64();
                    let l4 = &mut *(l4_virt.as_mut_ptr());
                    let mut mapper = x86_64::structures::paging::OffsetPageTable::new(l4, po);
                    let _ = mapper.map_to(page, frame, flags, fa).unwrap().flush();
                } else {
                    let mut mapper = crate::memory::paging::current_mapper();
                    let _ = mapper.map_to(page, frame, flags, fa).unwrap().flush();
                }
            }
        }
    }
    if proc.kernel_stack_top != 0 {
        unsafe {
            let kstack_top = proc.kernel_stack_top;
            let cpu_state_ptr = (kstack_top - core::mem::size_of::<crate::process::CpuState>() as u64) as *mut crate::process::CpuState;
            cpu_state_ptr.write(crate::process::CpuState { r15: 0, r14: 0, r13: 0, r12: 0, r11: 0, r10: 0, r9: 0, r8: 0, rdi: 0, rsi: 0, rbp: 0, rbx: 0, rdx: 0, rcx: 0, rax: 0, rip: proc.entry_point.as_u64(), cs: crate::arch::x86_64::gdt::user_code_selector().0 as u64, rflags: 0x202, rsp: stack_top, ss: crate::arch::x86_64::gdt::user_data_selector().0 as u64, });
            let ret_addr_ptr = (kstack_top - core::mem::size_of::<crate::process::CpuState>() as u64 - 8) as *mut u64;
            extern "C" { fn jump_to_user_stub(); }
            ret_addr_ptr.write(jump_to_user_stub as *const () as u64);
            let context_ptr = (kstack_top - core::mem::size_of::<crate::process::CpuState>() as u64 - 8 - 48) as *mut u64;
            for i in 0..6 { context_ptr.add(i).write(0); }
            proc.kernel_stack_ptr = kstack_top - core::mem::size_of::<crate::process::CpuState>() as u64 - 8 - 48;
        }
    }
}
