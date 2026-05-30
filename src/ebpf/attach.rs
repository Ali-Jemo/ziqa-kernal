/// eBPF attach points for ZiqaKernel
///
/// Allows attaching eBPF programs to kernel tracepoints (e.g., syscall entry/exit)
// Useful for tracing, monitoring, and security auditing.

use crate::abi::syscall::{SyscallContext};
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
    // TODO: Add more tracepoints (sched_switch, irq_handler, etc.)
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
    /// We use a simple Vec for now; could be a list.
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
    /// Returns true if successful.
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
    /// Returns true if any program executed successfully.
    pub fn run(&self, tp: TracepointType, ctx: &mut SyscallContext) -> bool {
        let lock = self.entries.lock();
        let mut any = false;
        for (tp_ref, prog) in lock.iter() {
            if *tp_ref == tp {
                // Execute the program
                let mut vm = BpfVm::new();
                // Initialize registers from syscall context
                vm.registers[0] = ctx.number; // R0: syscall number
                vm.registers[1] = ctx.args[0]; // R1: arg0
                vm.registers[2] = ctx.args[1]; // R2: arg1
                vm.registers[3] = ctx.args[2]; // R3: arg2
                vm.registers[4] = ctx.args[3]; // R4: arg3
                vm.registers[5] = ctx.args[4]; // R5: arg4
                vm.registers[6] = ctx.args[5]; // R6: arg5
                // For exit tracepoint, also set retval in R7
                if tp == TracepointType::SyscallExit {
                    vm.registers[7] = ctx.retval;
                }
                // Execute the program
                match vm.execute(&prog.instructions) {
                    Ok(_retval) => {
                        // Optionally collect retval? For now, just count as success.
                        any = true;
                        // If we wanted to modify the syscall retval, we could do:
                        // if tp == TracepointType::SyscallExit {
                        //     ctx.retval = _retval;
                        // }
                    }
                    Err(e) => {
                // Log error but continue
                crate::klog!(Level::Error, "eBPF program execution failed: {:?}", e);
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