use crate::abi::AbiError;
/// io_uring: High-performance asynchronous I/O for ZiqaKernel
///
/// Based on the Linux io_uring design. Uses shared-memory rings
/// between the kernel and user space to minimize syscall overhead.
use crate::process::Pid;

/// Submission Queue Entry (SQE)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SqEntry {
    pub opcode: u8,
    pub flags: u8,
    pub fd: i32,
    pub addr: u64,
    pub len: u32,
    pub user_data: u64,
}

/// Completion Queue Entry (CQE)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CqEntry {
    pub user_data: u64,
    pub res: i32,
    pub flags: u32,
}

pub mod op {
    pub const READ: u8 = 0;
    pub const WRITE: u8 = 1;
    pub const NOP: u8 = 3;
}

pub struct IoUring {
    pub pid: Pid,
    pub ring_size: usize,
    pub sq_entries: [Option<SqEntry>; 16], // Simplified for now
    pub cq_entries: [Option<CqEntry>; 16],
}

impl IoUring {
    pub fn new(pid: Pid, size: usize) -> Self {
        Self {
            pid,
            ring_size: size,
            sq_entries: [None; 16],
            cq_entries: [None; 16],
        }
    }

    /// Process submission queue entries
    pub fn submit(&mut self, entry: SqEntry) -> Result<(), AbiError> {
        // Find empty slot in SQ
        for slot in self.sq_entries.iter_mut() {
            if slot.is_none() {
                *slot = Some(entry);
                return Ok(());
            }
        }
        Err(AbiError::Other("SQ full"))
    }

    /// Kernel processes all pending requests
    pub fn process_requests(&mut self) -> usize {
        let mut count = 0;
        for i in 0..16 {
            if let Some(sqe) = self.sq_entries[i].take() {
                let res = self.handle_sqe(sqe);
                self.complete(sqe.user_data, res);
                count += 1;
            }
        }
        count
    }

    fn handle_sqe(&self, sqe: SqEntry) -> i32 {
        let pid = self.pid;
        let res = crate::process::scheduler::with_process(pid, |process| {
            match sqe.opcode {
                op::NOP => 0,
                op::READ => {
                    let path = if sqe.fd == 3 {
                        "/etc/motd"
                    } else {
                        return -9; // -EBADF (Bad File Descriptor)
                    };

                    let buf = unsafe {
                        core::slice::from_raw_parts_mut(sqe.addr as *mut u8, sqe.len as usize)
                    };

                    let vfs = crate::fs::vfs::VFS.lock();
                    match vfs.read(process, path, buf, 0) {
                        Ok(bytes) => bytes as i32,
                        Err(crate::abi::AbiError::PermissionDenied) => -1, // -EPERM (Permission Denied)
                        Err(_) => -5,                                      // -EIO (I/O error)
                    }
                }
                op::WRITE => {
                    let path = if sqe.fd == 3 {
                        "/etc/motd"
                    } else {
                        return -9; // -EBADF (Bad File Descriptor)
                    };

                    let buf = unsafe {
                        core::slice::from_raw_parts(sqe.addr as *const u8, sqe.len as usize)
                    };

                    let vfs = crate::fs::vfs::VFS.lock();
                    match vfs.write(process, path, buf, 0) {
                        Ok(bytes) => bytes as i32,
                        Err(crate::abi::AbiError::PermissionDenied) => -1, // -EPERM (Permission Denied)
                        Err(_) => -5,                                      // -EIO (I/O error)
                    }
                }
                _ => -38, // -ENOSYS (Function not implemented)
            }
        });

        // Return -3 (-ESRCH: No such process) if the PID wasn't found in the scheduler
        res.unwrap_or(-3)
    }

    fn complete(&mut self, user_data: u64, res: i32) {
        for slot in self.cq_entries.iter_mut() {
            if slot.is_none() {
                *slot = Some(CqEntry {
                    user_data,
                    res,
                    flags: 0,
                });
                break;
            }
        }
    }
}
