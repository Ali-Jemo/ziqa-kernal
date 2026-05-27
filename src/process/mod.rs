/// Process management for ZiqaKernel

pub mod scheduler;
pub mod signal;

use crate::capability::CapabilitySpace;
use crate::memory::{VirtAddr, MemoryRegion};
use signal::SignalState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pid(pub u64);

impl Pid {
    pub fn as_usize(&self) -> usize { self.0 as usize }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbiKind { LinuxElf, Wasm, ZiqaNative }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessState { Created, Ready, Running, Blocked, Exited(i64) }

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CpuState {
    pub rax: u64, pub rbx: u64, pub rcx: u64, pub rdx: u64,
    pub rsi: u64, pub rdi: u64, pub rbp: u64, pub rsp: u64,
    pub r8: u64,  pub r9: u64,  pub r10: u64, pub r11: u64,
    pub r12: u64, pub r13: u64, pub r14: u64, pub r15: u64,
    pub rip: u64, pub rflags: u64,
    pub cs: u64,  pub ss: u64,
}

impl CpuState {
    pub const fn zero() -> Self {
        Self {
            rax: 0, rbx: 0, rcx: 0, rdx: 0,
            rsi: 0, rdi: 0, rbp: 0, rsp: 0,
            r8: 0,  r9: 0,  r10: 0, r11: 0,
            r12: 0, r13: 0, r14: 0, r15: 0,
            rip: 0, rflags: 0x202,
            cs: 0,  ss: 0,
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
    /// Regular file — stores a VFS path id / inode id
    File(u32),
}

#[derive(Debug, Clone, Copy)]
pub struct FileDesc {
    pub target: FdTarget,
    pub flags: u32,
}

const MAX_FDS: usize = 16;

pub struct FdTable {
    entries: [Option<FileDesc>; MAX_FDS],
}

impl FdTable {
    pub const fn new() -> Self {
        const NONE: Option<FileDesc> = None;
        let mut t = Self { entries: [NONE; MAX_FDS] };
        t.entries[0] = Some(FileDesc { target: FdTarget::Stdin,  flags: 0 });
        t.entries[1] = Some(FileDesc { target: FdTarget::Stdout, flags: 0 });
        t.entries[2] = Some(FileDesc { target: FdTarget::Stderr, flags: 0 });
        t
    }

    /// Allocate the lowest free fd >= 3; returns the fd number.
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

    /// Close fd >= 3; returns true if it was open.
    pub fn close(&mut self, fd: usize) -> bool {
        if fd < 3 { return false; }
        if let Some(slot) = self.entries.get_mut(fd) {
            if slot.is_some() { *slot = None; return true; }
        }
        false
    }

    pub fn open_count(&self) -> usize {
        self.entries.iter().filter(|e| e.is_some()).count()
    }
}

// ── Process ───────────────────────────────────────────────────────────────────

const MAX_REGIONS: usize = 16;

pub struct Process {
    pub pid: Pid,
    pub state: ProcessState,
    pub abi: AbiKind,
    pub priority: u8,
    pub capabilities: CapabilitySpace,
    pub cpu_state: CpuState,
    pub regions: [Option<MemoryRegion>; MAX_REGIONS],
    pub region_count: usize,
    pub entry_point: VirtAddr,
    pub stack_top: VirtAddr,
    pub signals: SignalState,
    pub parent: u64,
    pub exit_code: i64,
    pub fds: FdTable,
}

impl Process {
    pub fn new(pid: Pid, abi: AbiKind, entry: VirtAddr, stack: VirtAddr) -> Self {
        const NONE_REGION: Option<MemoryRegion> = None;
        let mut cpu = CpuState::zero();
        cpu.rip = entry.as_u64();
        cpu.rsp = stack.as_u64();
        Self {
            pid,
            state: ProcessState::Created,
            abi,
            priority: 2,
            capabilities: CapabilitySpace::new(),
            cpu_state: cpu,
            regions: [NONE_REGION; MAX_REGIONS],
            region_count: 0,
            entry_point: entry,
            stack_top: stack,
            signals: SignalState::new(),
            parent: 0,
            exit_code: 0,
            fds: FdTable::new(),
        }
    }

    pub fn add_region(&mut self, region: MemoryRegion) -> bool {
        if self.region_count >= MAX_REGIONS { return false; }
        for slot in self.regions.iter_mut() {
            if slot.is_none() {
                *slot = Some(region);
                self.region_count += 1;
                return true;
            }
        }
        false
    }

    pub fn make_ready(&mut self) { self.state = ProcessState::Ready; }
    pub fn exit(&mut self, code: i64) { self.state = ProcessState::Exited(code); }
    pub fn set_priority(&mut self, priority: u8) { self.priority = priority.min(4); }
}

pub struct ProcessTable {
    next_pid: u64,
}

impl ProcessTable {
    pub const fn new() -> Self { Self { next_pid: 1 } }
    pub fn alloc_pid(&mut self) -> Pid {
        let pid = Pid(self.next_pid);
        self.next_pid += 1;
        pid
    }
}
