/// eBPF attach points for ZiqaKernel
///
/// Allows attaching eBPF programs to kernel tracepoints (e.g., syscall entry/exit)
// Useful for tracing, monitoring, and security auditing.

use crate::ebpf::BpfInstruction;
use crate::ebpf::vm::BpfVm;
use crate::klog::Level;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::Mutex;

/// Types of tracepoints that eBPF programs can attach to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TracepointType {
    /// Called at the beginning of a syscall, before processing.
    SyscallEntry,
    /// Called at the end of a syscall, after processing.
    SyscallExit,
    /// Context switch event
    SchedSwitch,
    /// Page fault event
    PageFault,
    /// IRQ handler entry
    IrqEntry,
}

/// Decoupled syscall information for eBPF tracepoints.
#[derive(Debug, Clone, Copy)]
pub struct SyscallInfo {
    pub number: u64,
    pub args: [u64; 6],
    pub retval: u64,
}

/// Event data passed to eBPF programs at each tracepoint.
pub enum TracepointCtx {
    Syscall(SyscallInfo),
    SchedSwitch { prev_pid: u64, next_pid: u64 },
    PageFault { addr: u64, error_code: u64 },
    Irq { vector: u8 },
}

/// An eBPF program that has been loaded and verified.
pub struct BpfProgram {
    /// The bytecode instructions
    pub instructions: Vec<BpfInstruction>,
    /// Reference count for detachment
    pub refs: AtomicU32,
}

impl BpfProgram {
    pub fn new(instructions: Vec<BpfInstruction>) -> Self {
        Self {
            instructions,
            refs: AtomicU32::new(1),
        }
    }

    /// Increment reference count
    pub fn inc_ref(&self) {
        self.refs.fetch_add(1, Ordering::Relaxed);
    }

    /// Decrement reference count, return true if zero
    pub fn dec_ref(&self) -> bool {
        self.refs.fetch_sub(1, Ordering::Release) == 1
    }
}

/// Global state for eBPF attach points.
pub struct EbpfAttachments {
    /// Programs attached to each tracepoint type.
    pub entries: Mutex<Vec<(TracepointType, BpfProgram)>>,
}

impl EbpfAttachments {
    pub const fn new() -> Self {
        Self {
            entries: Mutex::new(Vec::new()),
        }
    }

    /// Attach an eBPF program to a tracepoint.
    /// Returns an opaque ID that can be used to detach later.
    pub fn attach(&self, tp: TracepointType, prog: BpfProgram) -> Result<usize, crate::ebpf::BpfError> {
        crate::ebpf::verifier::BpfVerifier::new(&prog.instructions).verify()?;
        prog.inc_ref();
        let mut lock = self.entries.lock();
        lock.push((tp, prog));
        Ok(lock.len() - 1)
    }

    /// Detach an eBPF program by ID.
    pub fn detach(&self, id: usize) -> bool {
        let mut lock = self.entries.lock();
        if id < lock.len() {
            let (_, prog) = lock.remove(id);
            if prog.dec_ref() {
                // Program dropped
            }
            true
        } else {
            false
        }
    }

    /// Run all eBPF programs attached to a given tracepoint.
    pub fn run(&self, tp: TracepointType, ctx: TracepointCtx) -> bool {
        let lock = self.entries.lock();
        let mut any = false;
        for (tp_ref, prog) in lock.iter() {
            if *tp_ref == tp {
                let mut vm = BpfVm::new();
                
                // Initialize VM registers based on tracepoint context
                match &ctx {
                    TracepointCtx::Syscall(sc) => {
                        vm.registers[0] = 0;
                        vm.registers[1] = sc.number;
                        vm.registers[2] = sc.args[0];
                        vm.registers[3] = sc.args[1];
                        vm.registers[4] = sc.args[2];
                        vm.registers[5] = sc.args[3];
                        vm.registers[6] = sc.args[4];
                        vm.registers[7] = sc.args[5];
                        vm.registers[8] = sc.retval;
                    }
                    TracepointCtx::SchedSwitch { prev_pid, next_pid } => {
                        vm.registers[1] = *prev_pid;
                        vm.registers[2] = *next_pid;
                    }
                    TracepointCtx::PageFault { addr, error_code } => {
                        vm.registers[1] = *addr;
                        vm.registers[2] = *error_code;
                    }
                    TracepointCtx::Irq { vector } => {
                        vm.registers[1] = *vector as u64;
                    }
                }

                match vm.execute(&prog.instructions) {
                    Ok(_) => any = true,
                    Err(e) => {
                        crate::klog!(Level::Error, "eBPF run failed: {:?}", e);
                    }
                }
            }
        }
        any
    }
}

pub static EBPF_ATTACHMENTS: EbpfAttachments = EbpfAttachments::new();

/// Helper macro for logging eBPF events.
#[macro_export]
macro_rules! ebpf_log {
    ($lvl:expr, $fmt:expr) => {
        cklog!($lvl, concat!("eBPF: ", $fmt))
    };
    ($lvl:expr, $fmt:expr, $($arg:tt)+) => {
        cklog!($lvl, concat!("eBPF: ", $fmt), $($arg)+)
    };
}
