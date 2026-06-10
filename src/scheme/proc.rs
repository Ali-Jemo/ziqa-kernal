use crate::scheme::{Scheme, SchemeResult};
use crate::process::{self, Pid};
use crate::abi::AbiError;
use crate::capability::ResourceKind;
use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use spin::Mutex;

#[derive(Clone, Copy)]
pub enum ProcTarget {
    Mem(Pid, u64),
    Regs(Pid),
    Status(Pid),
}

pub struct ProcScheme {
    handles: Mutex<BTreeMap<usize, ProcTarget>>,
    next_handle: Mutex<usize>,
}

impl ProcScheme {
    pub const fn new() -> Self {
        Self {
            handles: Mutex::new(BTreeMap::new()),
            next_handle: Mutex::new(2),
        }
    }
}

impl Scheme for ProcScheme {
    fn open(&self, path: &str, _flags: usize) -> SchemeResult<usize> {
        let parts: Vec<&str> = path.split('/').collect();
        if parts.len() < 2 { return Err(AbiError::Other("Invalid path")); }
        
        let pid_str = parts[0].trim_start_matches("proc:");
        let pid = Pid(pid_str.parse::<u64>().map_err(|_| AbiError::Other("Invalid PID"))?);
        let target_type = parts[1];
        
        let target = match target_type {
            "mem" => {
                let addr_str = parts.get(2).unwrap_or(&"0");
                let addr = if addr_str.starts_with("0x") {
                    u64::from_str_radix(&addr_str[2..], 16)
                } else {
                    addr_str.parse::<u64>()
                }.map_err(|_| AbiError::Other("Invalid address"))?;
                ProcTarget::Mem(pid, addr)
            }
            "regs" => ProcTarget::Regs(pid),
            "status" => ProcTarget::Status(pid),
            _ => return Err(AbiError::Other("Invalid target")),
        };

        // Capability check
        let has_debug_cap = process::scheduler::with_current_task(|proc| {
            proc.capabilities.has_permission(ResourceKind::ProcessDebug, true, false)
        }).unwrap_or(false);

        if !has_debug_cap {
            return Err(AbiError::PermissionDenied);
        }

        let handle = {
            let mut next = self.next_handle.lock();
            let h = *next;
            *next += 1;
            h
        };
        
        self.handles.lock().insert(handle, target);
        Ok(handle)
    }

    fn read(&self, id: usize, buf: &mut [u8]) -> SchemeResult<usize> {
        let handles = self.handles.lock();
        let target = handles.get(&id).ok_or(AbiError::Other("Invalid handle"))?;

        match target {
            ProcTarget::Status(pid) => {
                let proc_arc = process::scheduler::SCHEDULER.get_process(*pid).ok_or(AbiError::Other("Process not found"))?;
                let proc = proc_arc.lock();
                let text = alloc::format!(
                    "pid={}\nstate={:?}\nabi={:?}\npriority={}\nexit_code={}\nparent={}\n",
                    proc.pid.0,
                    proc.state,
                    proc.abi,
                    proc.priority,
                    proc.exit_code,
                    proc.parent,
                );
                let bytes = text.as_bytes();
                let len = core::cmp::min(buf.len(), bytes.len());
                buf[..len].copy_from_slice(&bytes[..len]);
                Ok(len)
            }
            ProcTarget::Regs(pid) => {
                let proc_arc = process::scheduler::SCHEDULER.get_process(*pid).ok_or(AbiError::Other("Process not found"))?;
                let proc = proc_arc.lock();
                let state = proc.cpu_state;
                let data = unsafe { core::slice::from_raw_parts(&state as *const _ as *const u8, core::mem::size_of::<crate::process::CpuState>()) };
                let len = core::cmp::min(buf.len(), data.len());
                buf[..len].copy_from_slice(&data[..len]);
                Ok(len)
            }
            ProcTarget::Mem(pid, addr) => {
                let len = buf.len();
                if process::scheduler::SCHEDULER.ptrace_read_mem(*pid, *addr, buf) {
                    Ok(len)
                } else {
                    Err(AbiError::Other("Memory access failed"))
                }
            }
        }
    }

    fn write(&self, id: usize, buf: &[u8]) -> SchemeResult<usize> {
        let handles = self.handles.lock();
        let target = handles.get(&id).ok_or(AbiError::Other("Invalid handle"))?;

        match target {
            ProcTarget::Regs(pid) => {
                let proc_arc = process::scheduler::SCHEDULER.get_process(*pid).ok_or(AbiError::Other("Process not found"))?;
                let mut proc = proc_arc.lock();
                let size = core::mem::size_of::<crate::process::CpuState>();
                if buf.len() < size { return Err(AbiError::Other("Buffer too small")); }
                unsafe {
                    core::ptr::copy_nonoverlapping(buf.as_ptr() as *const crate::process::CpuState, &mut proc.cpu_state, 1);
                }
                Ok(size)
            }
            ProcTarget::Mem(pid, addr) => {
                if process::scheduler::SCHEDULER.ptrace_write_mem(*pid, *addr, buf) {
                    Ok(buf.len())
                } else {
                    Err(AbiError::Other("Memory write failed"))
                }
            }
            ProcTarget::Status(_) => {
                Err(AbiError::Other("Status is read-only"))
            }
        }
    }

    fn close(&self, id: usize) -> SchemeResult<()> {
        self.handles.lock().remove(&id);
        Ok(())
    }
}
