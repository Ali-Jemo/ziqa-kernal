extern crate alloc;
use crate::memory::VirtAddr;
use crate::process::{AbiKind, Pid, Process, ProcessState, ProcessTable as PidAllocator};
use crate::process::vma::Vma;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::{Mutex, RwLock};
use x86_64::registers::control::{Cr3, Cr3Flags};
use x86_64::structures::paging::FrameAllocator;

const MAX_TASKS: usize = 64;
const PRIORITY_LEVELS: u8 = 4;
const BOOST_INTERVAL: u64 = 1000;

/// The global process table, allowing concurrent access to different processes.
pub struct GlobalProcessTable {
    pub tasks: [Option<Arc<Mutex<Process>>>; MAX_TASKS],
}

impl GlobalProcessTable {
    pub const fn new() -> Self {
        const NONE_PROC: Option<Arc<Mutex<Process>>> = None;
        Self {
            tasks: [NONE_PROC; MAX_TASKS],
        }
    }

    pub fn get(&self, pid: Pid) -> Option<Arc<Mutex<Process>>> {
        self.tasks.iter().filter_map(|t| t.as_ref().cloned()).find(|p| p.lock().pid == pid)
    }

    fn find_free_slot(&self) -> Option<usize> {
        self.tasks.iter().position(|s| s.is_none())
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
            file_offset: 0,
        }));

        init_process_stack(&mut proc);
        proc.make_ready();

        let mut table = self.process_table.write();
        if let Some(slot) = table.find_free_slot() {
            table.tasks[slot] = Some(Arc::new(Mutex::new(proc)));
            self.ready_queues.lock().push(pid, 0);
            return Some(pid);
        }
        None
    }

    pub fn spawn_kthread(&self, entry: fn()) -> Option<Pid> {
        let pid = self.pid_allocator.lock().alloc_pid();
        let mut proc = Process::new(pid, AbiKind::ZiqaNative, VirtAddr::new(entry as u64), VirtAddr::new(1));
        proc.cpu_state.cs = 0x8;
        proc.cpu_state.ss = 0x10;
        proc.cpu_state.rflags = 0x202;
        proc.page_table_frame = None;
        proc.make_ready();
        init_kthread_stack(&mut proc);

        let mut table = self.process_table.write();
        if let Some(slot) = table.find_free_slot() {
            table.tasks[slot] = Some(Arc::new(Mutex::new(proc)));
            self.ready_queues.lock().push(pid, 0);
            return Some(pid);
        }
        None
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

        match plugin.load(binary, &mut proc) {
            Ok(()) => {
                init_process_stack(&mut proc);
                proc.make_ready();
                let mut table = self.process_table.write();
                if let Some(slot) = table.find_free_slot() {
                    table.tasks[slot] = Some(Arc::new(Mutex::new(proc)));
                    self.ready_queues.lock().push(pid, 0);
                    return Some(pid);
                }
                None
            }
            Err(_) => None,
        }
    }

    pub fn exit_process(&self, pid: Pid, code: i64) {
        if let Some(proc_arc) = self.get_process(pid) {
            let mut proc = proc_arc.lock();
            proc.exit_code = code;
            proc.state = ProcessState::Exited(code);
            let parent_pid = proc.parent;
            if parent_pid != 0 {
                self.send_signal(Pid(parent_pid), crate::process::signal::sig::SIGCHLD);
            }
        }
    }

    pub fn fork(&self, parent_pid: Pid) -> Option<Pid> {
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
        if let Some(slot) = table.find_free_slot() {
            table.tasks[slot] = Some(Arc::new(Mutex::new(child)));
            self.ready_queues.lock().push(child_pid, 0);
            return Some(child_pid);
        }
        None
    }

    pub fn waitpid(&self, parent: Pid, child_pid: i64) -> Option<(Pid, i64)> {
        let table = self.process_table.read();
        for slot in table.tasks.iter() {
            if let Some(proc_arc) = slot {
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
                        if let Some(idx) = write_table.tasks.iter().position(|t| {
                            t.as_ref().map(|p| p.lock().pid == pid).unwrap_or(false)
                        }) {
                            write_table.tasks[idx] = None;
                            return Some((pid, code));
                        }
                        return Some((pid, code));
                    }
                }
            }
        }
        None
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

    pub fn tick(&self) {
        let t = self.ticks.fetch_add(1, Ordering::Relaxed) + 1;

        if let Some(pid) = self.current_pid() {
            if let Some(proc_arc) = self.get_process(pid) {
                let mut proc = proc_arc.lock();
                if t % 10 == 0 {
                    if proc.priority < PRIORITY_LEVELS - 1 {
                        proc.priority += 1;
                    }
                }
            }
        }

        if t % BOOST_INTERVAL == 0 {
            let table = self.process_table.read();
            for slot in table.tasks.iter() {
                if let Some(p) = slot {
                    p.lock().priority = 0;
                }
            }
        }
    }

    pub fn schedule(&self) {
        let next_pid = self.ready_queues.lock().pop_highest();
        
        if let Some(new_pid) = next_pid {
            let old_pid = self.current_pid();
            
            if old_pid == Some(new_pid) {
                return;
            }

            self.set_current_pid(Some(new_pid));

            let old_proc_arc = old_pid.and_then(|id| self.get_process(id));
            let new_proc_arc = self.get_process(new_pid).expect("Ready task missing from table");

            let mut new_proc = new_proc_arc.lock();
            new_proc.state = ProcessState::Running;

            if let Some(frame) = new_proc.page_table_frame {
                unsafe { Cr3::write(frame, Cr3Flags::empty()); }
            }

            crate::arch::x86_64::update_trap_stacks(new_proc.kernel_stack_top);

            if let Some(old_arc) = old_proc_arc {
                let mut old_proc = old_arc.lock();
                if old_proc.state == ProcessState::Running {
                    old_proc.state = ProcessState::Ready;
                    self.ready_queues.lock().push(old_proc.pid, old_proc.priority);
                }
                
                unsafe {
                    crate::arch::x86_64::switch::switch_context(
                        &mut old_proc.kernel_stack_ptr,
                        new_proc.kernel_stack_ptr,
                    );
                }
            }
        }
    }

    pub fn get_process(&self, pid: Pid) -> Option<Arc<Mutex<Process>>> {
        self.process_table.read().get(pid)
    }

    pub fn wake_sleeping_mask(&self, woken_mask: u64) {
        let table = self.process_table.read();
        for slot in table.tasks.iter() {
            if let Some(p) = slot {
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
        table.tasks.iter().filter_map(|t| t.as_ref().map(|p| p.lock().pid)).collect()
    }

    pub fn init_boot_process(&self) {
        let pid = self.pid_allocator.lock().alloc_pid();
        let mut proc = Process::new(pid, AbiKind::ZiqaNative, VirtAddr::new(0), VirtAddr::new(0));
        let kstack = alloc::vec![0u8; 65536];
        let top = kstack.as_ptr() as u64 + 65536;
        proc.kernel_stack = Some(kstack);
        proc.kernel_stack_top = top;
        proc.state = ProcessState::Running;
        
        let mut table = self.process_table.write();
        table.tasks[0] = Some(Arc::new(Mutex::new(proc)));
        self.set_current_pid(Some(pid));
    }
}

pub static SCHEDULER: Scheduler = Scheduler::new();

pub fn tick() { SCHEDULER.tick(); }
pub fn spawn(abi: AbiKind, entry: VirtAddr, stack: VirtAddr) -> Option<Pid> { SCHEDULER.spawn(abi, entry, stack) }
pub fn spawn_kthread(entry: fn()) -> Option<Pid> { SCHEDULER.spawn_kthread(entry) }
pub fn spawn_elf(binary: &[u8]) -> Option<Pid> { SCHEDULER.spawn_elf(binary) }
pub fn with_process<F, R>(pid: Pid, f: F) -> Option<R> where F: FnOnce(&Process) -> R, { SCHEDULER.get_process(pid).map(|p| f(&p.lock())) }
pub fn with_process_mut<F, R>(pid: Pid, f: F) -> Option<R> where F: FnOnce(&mut Process) -> R, { SCHEDULER.get_process(pid).map(|p| f(&mut p.lock())) }
pub fn current_task() -> Option<Arc<Mutex<Process>>> { SCHEDULER.current_pid().and_then(|pid| SCHEDULER.get_process(pid)) }
pub fn current_task_mut() -> Option<Arc<Mutex<Process>>> { current_task() }
pub fn wake_sleeping(woken_mask: u64) { SCHEDULER.wake_sleeping_mask(woken_mask); }
pub fn init() { SCHEDULER.init_boot_process(); }
pub fn list_pids() -> Vec<Pid> { SCHEDULER.list_pids() }
pub fn yield_now() { unsafe { core::arch::asm!("int 0x20"); } }

/// Replace a process with a new ELF binary (execve).
/// This clears old mappings, loads the new binary, and resets CPU state.
pub fn exec_process(
    pid: Pid,
    binary: &[u8],
    _args: &[&[u8]],
    _env: &[&[u8]],
) -> Result<(), &'static str> {
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
    // A proper implementation would push argv/envp arrays onto the user stack
    proc.state = ProcessState::Ready;

    // Free old page table frames (best-effort)
    let _ = old_frame;

    Ok(())
}

fn init_kthread_stack(proc: &mut Process) {
    if let Some(ref mut _kstack) = proc.kernel_stack {
        let kstack_top = proc.kernel_stack_top;
        unsafe {
            let ret_ptr = (kstack_top - 8) as *mut u64;
            ret_ptr.write(proc.entry_point.as_u64());
            let context_ptr = (kstack_top - 8 - 48) as *mut u64;
            for i in 0..6 { context_ptr.add(i).write(0); }
            proc.kernel_stack_ptr = kstack_top - 8 - 48;
        }
    }
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
