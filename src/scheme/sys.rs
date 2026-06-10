use crate::scheme::{Scheme, SchemeResult};
use crate::abi::AbiError;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicUsize, Ordering};
use spin::Mutex;
use alloc::collections::BTreeMap;

pub struct SysScheme {
    next_handle: AtomicUsize,
    handles: Mutex<BTreeMap<usize, SysHandle>>,
}

struct SysHandle {
    data: Vec<u8>,
    offset: usize,
}

impl SysScheme {
    pub fn new() -> Self {
        Self {
            next_handle: AtomicUsize::new(1),
            handles: Mutex::new(BTreeMap::new()),
        }
    }
}

impl Scheme for SysScheme {
    fn open(&self, path: &str, _flags: usize) -> SchemeResult<usize> {
        let path = path.trim_start_matches('/');
        let data = match path {
            "uname" => {
                b"ZiqaKernel 1.0\nx86_64\n".to_vec()
            }
            "uptime" => {
                let ticks = crate::timer::uptime_ticks();
                alloc::format!("{}\n", ticks).into_bytes()
            }
            "scheme" => {
                let mut s = String::new();
                let names = crate::scheme::SCHEME_REGISTRY.lock().iter_names();
                for name in names {
                    s.push_str(&name);
                    s.push('\n');
                }
                s.into_bytes()
            }
            "context" => {
                let mut s = String::new();
                let _ = core::fmt::Write::write_fmt(
                    &mut s,
                    format_args!("{:<6}{:<6}{:<10}{:<8}{}\n", "PID", "PPID", "STATE", "VMAS", "NAME")
                );

                let table = crate::process::scheduler::SCHEDULER.process_table.read();
                for (pid, process_arc) in table.tasks.iter() {
                    let proc = process_arc.lock();
                    let state_str = match proc.state {
                        crate::process::ProcessState::Created => "Created",
                        crate::process::ProcessState::Ready => "Ready",
                        crate::process::ProcessState::Running => "Running",
                        crate::process::ProcessState::Blocked => "Blocked",
                        crate::process::ProcessState::Canceled => "Canceled",
                        crate::process::ProcessState::Exited(_) => "Exited",
                    };
                    
                    let _ = core::fmt::Write::write_fmt(
                        &mut s,
                        format_args!("{:<6}{:<6}{:<10}{:<8}{}\n", 
                            pid.0, 
                            proc.parent, 
                            state_str, 
                            proc.vmas.len(),
                            "(unknown)"
                        )
                    );
                }
                s.into_bytes()
            }
            "cpu" => {
                let mut s = String::new();
                s.push_str("CPU: Generic x86_64\n");
                s.into_bytes()
            }
            _ => return Err(AbiError::Other("No such file or directory")),
        };

        let handle = self.next_handle.fetch_add(1, Ordering::SeqCst);
        self.handles.lock().insert(handle, SysHandle {
            data,
            offset: 0,
        });

        Ok(handle)
    }

    fn read(&self, id: usize, buf: &mut [u8]) -> SchemeResult<usize> {
        let mut handles = self.handles.lock();
        let handle = handles.get_mut(&id).ok_or(AbiError::Other("Bad file descriptor"))?;

        if handle.offset >= handle.data.len() {
            return Ok(0);
        }

        let remaining = handle.data.len() - handle.offset;
        let to_copy = core::cmp::min(remaining, buf.len());
        buf[..to_copy].copy_from_slice(&handle.data[handle.offset..handle.offset + to_copy]);
        handle.offset += to_copy;

        Ok(to_copy)
    }

    fn write(&self, _id: usize, _buf: &[u8]) -> SchemeResult<usize> {
        Err(AbiError::Other("Read only file system"))
    }

    fn close(&self, id: usize) -> SchemeResult<()> {
        self.handles.lock().remove(&id).ok_or(AbiError::Other("Bad file descriptor"))?;
        Ok(())
    }
}
