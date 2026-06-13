/// Process management for ZiqaKernel
pub mod scheduler;
pub mod signal;
pub mod snapshot;
pub mod vma;

use crate::arch::x86_64::switch::TrapFrame;
use crate::capability::CapabilitySpace;
use crate::memory::VirtAddr;
use crate::process::vma::Vma;
use signal::SignalState;
use x86_64::structures::paging::PhysFrame;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Pid(pub u64);

impl Pid {
    pub fn as_usize(&self) -> usize {
        self.0 as usize
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbiKind {
    LinuxElf,
    Wasm,
    ZiqaNative,
    RedoxElf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessState {
    Created,
    Ready,
    Running,
    Blocked,
    Canceled,
    Exited(i64),
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CpuState {
    // Pushed by assembly stub (pop order)
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub r11: u64,
    pub r10: u64,
    pub r9: u64,
    pub r8: u64,
    pub rdi: u64,
    pub rsi: u64,
    pub rbp: u64,
    pub rbx: u64,
    pub rdx: u64,
    pub rcx: u64,
    pub rax: u64,

    // Pushed automatically by CPU on interrupt
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u64,
}

impl CpuState {
    pub const fn zero() -> Self {
        Self {
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
            rip: 0,
            rflags: 0x202,
            cs: 8,
            ss: 0,
            rsp: 0,
        }
    }
}

// ── File Descriptor Table ─────────────────────────────────────────────────────

/// What a file descriptor points to
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FdTarget {
    Stdin,
    Stdout,
    Stderr,
    /// Pipe read end — backed by IPC channel id
    PipeRead(u32),
    /// Pipe write end — backed by IPC channel id
    PipeWrite(u32),
    /// Regular file — index into FdTable::paths
    File(u8),
    /// Scheme-backed file — index into FdTable::paths, and scheme handle ID
    Scheme(u8, usize),
}

#[derive(Debug, Clone, Copy)]
pub struct FileDesc {
    pub target: FdTarget,
    pub flags: u32,
    pub offset: usize,
}

const MAX_FDS: usize = 8;

pub struct FdTable {
    entries: [Option<FileDesc>; MAX_FDS],
    /// Paths for File fds — indexed by FdTarget::File(idx)
    pub paths: [[u8; 64]; MAX_FDS],
    pub path_lens: [usize; MAX_FDS],
}

impl FdTable {
    pub const fn new() -> Self {
        const NONE: Option<FileDesc> = None;
        let mut t = Self {
            entries: [NONE; MAX_FDS],
            paths: [[0u8; 64]; MAX_FDS],
            path_lens: [0usize; MAX_FDS],
        };
        t.entries[0] = Some(FileDesc {
            target: FdTarget::Stdin,
            flags: 0,
            offset: 0,
        });
        t.entries[1] = Some(FileDesc {
            target: FdTarget::Stdout,
            flags: 0,
            offset: 0,
        });
        t.entries[2] = Some(FileDesc {
            target: FdTarget::Stderr,
            flags: 0,
            offset: 0,
        });
        t
    }

    /// Allocate the lowest free fd >= 3 with a VFS path; returns the fd number.
    pub fn alloc_file(&mut self, path: &[u8], flags: u32) -> Option<usize> {
        for (i, slot) in self.entries.iter_mut().enumerate().skip(3) {
            if slot.is_none() {
                let n = path.len().min(63);
                self.paths[i][..n].copy_from_slice(&path[..n]);
                self.paths[i][n] = 0;
                self.path_lens[i] = n;
                *slot = Some(FileDesc {
                    target: FdTarget::File(i as u8),
                    flags,
                    offset: 0,
                });
                return Some(i);
            }
        }
        None
    }

    /// Allocate a scheme-backed fd.
    pub fn alloc_scheme(&mut self, path: &[u8], flags: u32, handle_id: usize) -> Option<usize> {
        for (i, slot) in self.entries.iter_mut().enumerate().skip(3) {
            if slot.is_none() {
                let n = path.len().min(63);
                self.paths[i][..n].copy_from_slice(&path[..n]);
                self.paths[i][n] = 0;
                self.path_lens[i] = n;
                *slot = Some(FileDesc {
                    target: FdTarget::Scheme(i as u8, handle_id),
                    flags,
                    offset: 0,
                });
                return Some(i);
            }
        }
        None
    }

    /// Allocate a generic fd (pipe, etc.)
    pub fn alloc(&mut self, desc: FileDesc) -> Option<usize> {
        for (i, slot) in self.entries.iter_mut().enumerate().skip(3) {
            if slot.is_none() {
                *slot = Some(desc);
                return Some(i);
            }
        }
        None
    }

    pub fn get(&self, fd: usize) -> Option<&FileDesc> {
        self.entries.get(fd)?.as_ref()
    }

    pub fn get_mut(&mut self, fd: usize) -> Option<&mut FileDesc> {
        self.entries.get_mut(fd)?.as_mut()
    }

    /// Get the VFS path for a File or Scheme fd.
    pub fn path_of(&self, fd: usize) -> Option<&[u8]> {
        let desc = self.entries.get(fd)?.as_ref()?;
        match desc.target {
            FdTarget::File(_) | FdTarget::Scheme(_, _) => {
                Some(&self.paths[fd][..self.path_lens[fd]])
            }
            _ => None,
        }
    }

    /// Close fd >= 3; returns true if it was open.
    pub fn close(&mut self, fd: usize) -> bool {
        if fd < 3 {
            return false;
        }
        if let Some(slot) = self.entries.get_mut(fd) {
            if slot.is_some() {
                *slot = None;
                self.path_lens[fd] = 0;
                return true;
            }
        }
        false
    }

    /// Duplicate src_fd into dst_fd (or lowest free if dst_fd is None).
    pub fn dup(&mut self, src_fd: usize, dst_fd: Option<usize>) -> Option<usize> {
        let desc = *self.entries.get(src_fd)?.as_ref()?;
        match dst_fd {
            Some(d) => {
                if d < MAX_FDS {
                    self.entries[d] = Some(FileDesc { offset: 0, ..desc });
                    self.paths[d] = self.paths[src_fd];
                    self.path_lens[d] = self.path_lens[src_fd];
                    return Some(d);
                }
                None
            }
            None => {
                for (i, slot) in self.entries.iter_mut().enumerate().skip(3) {
                    if slot.is_none() {
                        *slot = Some(FileDesc { offset: 0, ..desc });
                        self.paths[i] = self.paths[src_fd];
                        self.path_lens[i] = self.path_lens[src_fd];
                        return Some(i);
                    }
                }
                None
            }
        }
    }

    pub fn open_count(&self) -> usize {
        self.entries.iter().filter(|e| e.is_some()).count()
    }

    pub fn clone_from(&mut self, other: &Self) {
        for i in 0..MAX_FDS {
            self.entries[i] = other.entries[i];
            self.paths[i] = other.paths[i];
            self.path_lens[i] = other.path_lens[i];
        }
    }
}

// ── Process ───────────────────────────────────────────────────────────────────

pub struct Process {
    pub pid: Pid,
    pub state: ProcessState,
    pub abi: AbiKind,
    pub priority: u8,
    pub capabilities: CapabilitySpace,
    pub cpu_state: CpuState,
    pub fpu_state: crate::arch::x86_64::FpuState,
    pub vmas: alloc::vec::Vec<Vma>,
    pub entry_point: VirtAddr,
    pub stack_top: VirtAddr,
    pub signals: SignalState,
    pub parent: u64,
    pub exit_code: i64,
    pub join_waiters: alloc::vec::Vec<u64>,
    pub fds: FdTable,
    /// Program break (heap top) for sys_brk
    pub brk: u64,
    /// Current working directory (null-terminated, max 128 bytes)
    pub cwd: [u8; 128],
    pub cwd_len: usize,
    /// Next available virtual address for mmap (bump allocator)
    pub mmap_bump: u64,
    /// Raw ELF binary data for page-fault-on-demand copying
    pub binary_data: alloc::vec::Vec<u8>,
    /// Physical frame of this process's root (L4) page table.
    /// `None` means it shares the bootloader's / kernel's page table.
    pub page_table_frame: Option<PhysFrame>,
    /// Saved kernel stack pointer for context switching
    pub kernel_stack_ptr: u64,
    /// Dedicated kernel stack memory
    pub kernel_stack: Option<alloc::vec::Vec<u8>>,
    /// Top of the dedicated kernel stack
    pub kernel_stack_top: u64,
    /// Total CPU ticks consumed by this process
    pub total_ticks: u64,
    /// Virtual runtime for CFS-style scheduling
    pub vruntime: u64,
    /// FS base for thread-local storage (TLS)
    pub fs_base: u64,
    /// GS base for thread-local storage (TLS)
    pub gs_base: u64,
    /// Whether the last block operation timed out
    pub timed_out: bool,
    /// Path to the ELF interpreter (PT_INTERP)
    pub interpreter: Option<alloc::string::String>,
}
impl Process {
    pub fn new(pid: Pid, abi: AbiKind, entry: VirtAddr, stack: VirtAddr) -> Self {
        let mut cpu = CpuState::zero();
        cpu.rip = entry.as_u64();
        cpu.rsp = stack.as_u64();
        cpu.cs = crate::arch::x86_64::gdt::user_code_selector().0 as u64;
        cpu.ss = crate::arch::x86_64::gdt::user_data_selector().0 as u64;

        // Allocate a dedicated kernel stack (64KB) for user mode execution
        // Boot process (indicated by stack == 0) shares kernel context/stack
        let (kstack, kstack_top) = if stack.as_u64() != 0 {
            let mut kstack = alloc::vec![0u8; 65536];
            let top = kstack.as_mut_ptr() as u64 + 65536;
            (Some(kstack), top)
        } else {
            (None, 0)
        };

        Self {
            pid,
            state: ProcessState::Created,
            abi,
            priority: 2,
            capabilities: CapabilitySpace::new(),
            cpu_state: cpu,
            fpu_state: crate::arch::x86_64::FpuState::new(),
            vmas: alloc::vec::Vec::new(),
            entry_point: entry,
            stack_top: stack,
            signals: SignalState::new(),
            parent: 0,
            exit_code: 0,
            join_waiters: alloc::vec::Vec::new(),
            fds: FdTable::new(),
            brk: 0x2000_0000,
            cwd: {
                let mut b = [0u8; 128];
                b[0] = b'/';
                b
            },
            cwd_len: 1,
            mmap_bump: 0x7000_0000,
            binary_data: alloc::vec::Vec::new(),
            page_table_frame: None,
            kernel_stack_ptr: kstack_top,
            kernel_stack: kstack,
            kernel_stack_top: kstack_top,
            total_ticks: 0,
            vruntime: 0,
            fs_base: 0,
            gs_base: 0,
            timed_out: false,
            interpreter: None,
        }
    }

    pub fn add_region(&mut self, vma: Vma) {
        let pos = self.vmas.binary_search_by_key(&vma.start, |v| v.start)
            .unwrap_or_else(|e| e);
        self.vmas.insert(pos, vma);
    }

    pub fn get_vfs_handle(&self, fd: usize) -> Option<crate::fs::vfs::VfsHandle> {
        use crate::process::FdTarget;
        let desc = self.fds.get(fd)?;
        match desc.target {
            FdTarget::File(_) => {
                let path_bytes = self.fds.path_of(fd)?;
                let path = core::str::from_utf8(path_bytes).ok()?;
                let node = crate::fs::vfs::VFS.read().resolve_node(path).ok()?;
                let guard = node.read();
                match &*guard {
                    crate::fs::vfs::VfsNode::File { handle } => Some(crate::fs::vfs::VfsHandle::File(handle.clone())),
                    _ => None,
                }
            }
            FdTarget::Scheme(_, handle_id) => {
                let path_bytes = self.fds.path_of(fd)?;
                let path = core::str::from_utf8(path_bytes).ok()?;
                let pos = path.find(':')?;
                let scheme_name = &path[..pos];
                Some(crate::fs::vfs::VfsHandle::Scheme {
                    scheme: alloc::string::String::from(scheme_name),
                    handle_id,
                })
            }
            _ => None,
        }
    }

    pub fn set_state_ready(&mut self) {
        self.state = ProcessState::Ready;
    }

    pub fn schedule_ready(&mut self) {
        self.set_state_ready();
        crate::process::scheduler::SCHEDULER.ready_queues.lock().push(self.pid, self.vruntime);
    }
    pub fn exit(&mut self, code: i64) {
        self.state = ProcessState::Exited(code);
    }
    pub fn set_priority(&mut self, priority: u8) {
        self.priority = priority.min(4);
    }

    pub fn block(&mut self) {
        self.state = ProcessState::Blocked;
    }

    pub fn unblock(&mut self) {
        self.state = ProcessState::Ready;
    }

    pub fn prepare_trap_frame(&self) -> TrapFrame {
        TrapFrame {
            rip: self.cpu_state.rip,
            cs: self.cpu_state.cs,
            rflags: self.cpu_state.rflags,
            rsp: self.cpu_state.rsp,
            ss: self.cpu_state.ss,
        }
    }
}

pub struct ProcessTable {
    next_pid: u64,
}

impl ProcessTable {
    pub const fn new() -> Self {
        Self { next_pid: 1 }
    }
    pub fn alloc_pid(&mut self) -> Pid {
        let pid = Pid(self.next_pid);
        self.next_pid += 1;
        pid
    }
}
