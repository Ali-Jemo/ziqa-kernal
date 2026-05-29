extern crate alloc;
use crate::memory::VirtAddr;
use crate::process::signal::{default_action, DefaultDisposition, SignalAction};
use crate::process::{AbiKind, Pid, Process, ProcessState, ProcessTable};
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
use x86_64::registers::control::{Cr3, Cr3Flags};
use x86_64::structures::paging::FrameAllocator;

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

        // 1. Give the process its own page table
        if let Some(frame) = crate::memory::paging::create_process_page_table() {
            proc.page_table_frame = Some(frame);
        }

        // 2. Add stack region
        let stack_size = 256 * 1024;
        let stack_start = stack.as_u64().checked_sub(stack_size as u64)?;
        proc.add_region(crate::memory::MemoryRegion {
            start: VirtAddr::new(stack_start),
            size: stack_size as usize,
            flags: crate::memory::paging::MemoryRegionFlags::read_write(),
            is_file_backed: false,
            file_offset: 0,
        });

        // 3. Initialize process stack with TrapFrame
        init_process_stack(&mut proc);

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

    /// Spawn a native kernel thread (shares kernel address space, runs in Ring 0)
    pub fn spawn_kthread(&mut self, entry: fn()) -> Option<Pid> {
        if self.count >= MAX_TASKS {
            return None;
        }
        let pid = self.table.alloc_pid();
        // Use 1 as stack sentinel to trigger kstack allocation in Process::new
        let mut proc = Process::new(pid, AbiKind::ZiqaNative, VirtAddr::new(entry as u64), VirtAddr::new(1));
        
        // Kernel thread specifics:
        proc.cpu_state.cs = 0x8; // Kernel Code
        proc.cpu_state.ss = 0x10; // Kernel Data
        proc.cpu_state.rflags = 0x202; // IF enabled
        
        // Shares kernel page table
        proc.page_table_frame = None;

        proc.make_ready();

        for slot in self.tasks.iter_mut() {
            if let Some(p) = slot {
                if p.pid == pid {
                    // Safety check: don't overwrite
                    return None;
                }
            }
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
                        self.send_signal_inner(
                            Pid(parent_pid),
                            crate::process::signal::sig::SIGCHLD,
                        );
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
        let parent_index = self
            .tasks
            .iter()
            .position(|s| s.as_ref().map(|p| p.pid == parent_pid).unwrap_or(false))?;

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
        for region_opt in self.tasks[parent_index]
            .as_mut()
            .unwrap()
            .regions
            .iter_mut()
        {
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
                        self.count -= 1;
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
            if !proc.signals.has_pending() {
                return;
            }

            // Check for signals that should be delivered to userspace
            let signum = proc.signals.dequeue();
            if signum == 0 {
                return;
            }

            let action = proc.signals.get_action(signum);

            match action {
                SignalAction::Ignore => {}
                SignalAction::Handler(addr) => {
                    // Store signal info for context switch
                    proc.signals.pending_signal = signum;
                    proc.signals.handler_addr = addr;

                    crate::println!(
                        "[SIGNAL] PID {} handler at 0x{:x} for sig {}",
                        proc.pid.0,
                        addr,
                        signum
                    );
                }
                SignalAction::Default => match default_action(signum) {
                    DefaultDisposition::Terminate | DefaultDisposition::CoreDump => {
                        proc.exit_code = -(signum as i64);
                        proc.state = ProcessState::Exited(-(signum as i64));
                    }
                    DefaultDisposition::Stop => {
                        proc.state = ProcessState::Blocked;
                    }
                    DefaultDisposition::Ignore => {}
                },
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

    /// MLFQ bookkeeping — called from the timer ISR.
    /// Must NOT context-switch: the scheduler lock is held and switching to a
    /// user process before releasing it would deadlock when the new process
    /// page-faults or makes a syscall (both of which try to re-acquire the lock).
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
            let old_idx = self.current;
            let new_idx = idx;

            self.current = new_idx;

            // 1. Prepare new proc
            {
                let new_proc = self.tasks[new_idx].as_mut().unwrap();
                new_proc.state = ProcessState::Running;
                self.timeslice_remaining = match new_proc.priority {
                    0 => 5,
                    1 => 10,
                    2 => 20,
                    _ => 40,
                };

                // 2. Load page table
                if let Some(frame) = new_proc.page_table_frame {
                    unsafe {
                        Cr3::write(frame, Cr3Flags::empty());
                    }
                }
            }

            // 3. Switch
            if old_idx != new_idx {
                let old_ptr = self.tasks[old_idx].as_mut().map(|p| p as *mut Process);
                let new_ptr = self.tasks[new_idx].as_mut().map(|p| p as *mut Process);

                unsafe {
                    let new_proc = &mut *new_ptr.unwrap();

                    // Set TSS.RSP0 and KERNEL_STACK for Ring 3↔Ring 0 transitions.
                    // All processes now have a dedicated kernel stack.
                    crate::arch::x86_64::update_trap_stacks(new_proc.kernel_stack_top);

                    if let Some(old) = old_ptr.map(|p| &mut *p) {

                        if old.state == ProcessState::Running {
                            old.state = ProcessState::Ready;
                        }

                        crate::arch::x86_64::switch::switch_context(
                            &mut old.kernel_stack_ptr,
                            new_proc.kernel_stack_ptr,
                        );
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
        self.tasks
            .iter()
            .filter_map(|t| t.as_ref())
            .find(|p| p.pid == pid)
    }

    pub fn get_process_mut(&mut self, pid: Pid) -> Option<&mut Process> {
        self.tasks
            .iter_mut()
            .filter_map(|t| t.as_mut())
            .find(|p| p.pid == pid)
    }

    pub fn init_boot_process(&mut self) {
        let pid = self.table.alloc_pid();
        let mut proc = Process::new(pid, AbiKind::ZiqaNative, VirtAddr::new(0), VirtAddr::new(0));
        // Give the boot process a dedicated kernel stack so TSS.RSP0 always
        // points to a valid stack when switching back from a Ring 3 process.
        let kstack = alloc::vec![0u8; 8192];
        let top = kstack.as_ptr() as u64 + 8192;
        proc.kernel_stack = Some(kstack);
        proc.kernel_stack_top = top;
        // kernel_stack_ptr is set during the first context switch away
        // (switch_context saves the current bootloader stack there)
        // The boot process starts as running (it is the current kernel thread)
        self.tasks[0] = Some(proc);
        self.tasks[0].as_mut().unwrap().state = ProcessState::Running;
        self.current = 0;
        self.count = 1;
    }

    pub fn schedule(&mut self) {
        if let Some(proc) = &self.tasks[self.current] {
            if proc.state == ProcessState::Running {
                self.tasks[self.current].as_mut().unwrap().state = ProcessState::Ready;
            }
        }
        self.schedule_next();
    }

    pub fn poll_network(&mut self) {
        let mut stack = crate::net::stack::TCPIP.lock();
        if let Some(s) = stack.as_mut() {
            s.poll();
        }
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
        SCHEDULER
            .lock()
            .tasks
            .iter_mut()
            .filter_map(|t| t.as_mut())
            .find(|p| p.pid == pid)
            .map(f)
    })
}

/// Called from the timer subsystem to wake sleeping processes
pub fn wake_sleeping(woken_mask: u64) {
    x86_64::instructions::interrupts::without_interrupts(|| {
        SCHEDULER.lock().wake_sleeping_mask(woken_mask);
    })
}

pub fn init() {
    // Disable interrupts while taking the scheduler lock — init_pics() has
    // already enabled the PIT timer, and the timer ISR also takes this lock.
    x86_64::instructions::interrupts::without_interrupts(|| {
        SCHEDULER.lock().init_boot_process();
    });
}

impl Scheduler {
    pub fn print_process_list(&self) {
        crate::println!(" PID | State   | Priority | Parent ");
        crate::println!("-----|---------|----------|--------");
        for slot in self.tasks.iter() {
            if let Some(proc) = slot {
                crate::println!(
                    "{:4} | {:7?} | {:8} | {:6}",
                    proc.pid.0,
                    proc.state,
                    proc.priority,
                    proc.parent
                );
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

    // 1. Give the process its own page table
    if let Some(frame) = crate::memory::paging::create_process_page_table() {
        proc.page_table_frame = Some(frame);
    }

    // 2. Add stack region
    let stack_size = 256 * 1024;
    let stack_start = stack.as_u64() - stack_size;
    proc.add_region(crate::memory::MemoryRegion {
        start: VirtAddr::new(stack_start),
        size: stack_size as usize,
        flags: crate::memory::paging::MemoryRegionFlags::read_write(),
        is_file_backed: false,
        file_offset: 0,
    });

    match plugin.load(binary, &mut proc) {
        Ok(()) => {
            // 3. Initialize stack AFTER loading (so we have the real entry point)
            init_process_stack(&mut proc);
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

pub fn init_process_stack(proc: &mut Process) {
    let stack_top = proc.stack_top.as_u64();

    // 1. Map user stack page if the process has a user stack (stack_top != 0)
    if stack_top != 0 {
        let page_addr = (stack_top - 4096) & !0xFFF;
        let page = x86_64::structures::paging::Page::containing_address(VirtAddr::new(page_addr));

        let mut fa_guard = crate::memory::FRAME_ALLOCATOR.lock();
        let fa = fa_guard.as_mut().unwrap();
        let flags = x86_64::structures::paging::PageTableFlags::PRESENT
            | x86_64::structures::paging::PageTableFlags::WRITABLE
            | x86_64::structures::paging::PageTableFlags::USER_ACCESSIBLE;

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

    // 2. Initialize the kernel stack layout if the process has a dedicated kernel stack
    if proc.kernel_stack_top != 0 {
        unsafe {
            let kstack_top = proc.kernel_stack_top;
            let cpu_state_ptr = (kstack_top - core::mem::size_of::<crate::process::CpuState>() as u64)
                as *mut crate::process::CpuState;

            // Initialize CpuState on kernel stack
            cpu_state_ptr.write(crate::process::CpuState {
                r15: 0,
                r14: 0,
                r13: 0,
                r12: 0,
                r11: 0,
                r10: 0,
                r9: 0,
                r8: 0,
                rdi: 0,
                rsi: 0,
                rbp: 0,
                rbx: 0,
                rdx: 0,
                rcx: 0,
                rax: 0,
                rip: proc.entry_point.as_u64(),
                cs: crate::arch::x86_64::gdt::user_code_selector().0 as u64,
                rflags: 0x202, // Interrupts enabled
                rsp: stack_top,
                ss: crate::arch::x86_64::gdt::user_data_selector().0 as u64,
            });

            // Write the return address for switch_context (jump_to_user_stub)
            let ret_addr_ptr = (kstack_top - core::mem::size_of::<crate::process::CpuState>() as u64 - 8)
                as *mut u64;
            extern "C" {
                fn jump_to_user_stub();
            }
            ret_addr_ptr.write(jump_to_user_stub as *const () as u64);

            // Write 6 zeroed registers for switch_context (r15, r14, r13, r12, rbx, rbp)
            let context_ptr = (kstack_top - core::mem::size_of::<crate::process::CpuState>() as u64 - 8 - 48)
                as *mut u64;
            for i in 0..6 {
                context_ptr.add(i).write(0);
            }

            // Set the saved kernel stack pointer to point to the bottom of the switch context frame
            proc.kernel_stack_ptr = kstack_top - core::mem::size_of::<crate::process::CpuState>() as u64 - 8 - 48;
        }
    }
}

pub fn yield_now() {
    unsafe {
        core::arch::asm!("int 0x20");
    }
}
