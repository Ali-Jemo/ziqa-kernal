use crate::abi::AbiError;
use crate::println;

pub struct WasiHandler;

impl WasiHandler {
    /// Maps WASI fd_write to ZiqaKernel ZIQA_CAP_WRITE
    pub fn fd_write(fd: i32, buf: &[u8]) -> Result<u32, AbiError> {
        // We use libposix's approach: mapping to ZIQA_CAP_WRITE (1002)
        // In a real WASI implementation, we'd need to resolve the capability ID
        // from the FD in the process's FD table.
        // For now, we assume fd 1/2 maps to stdout/stderr capability.
        
        let cap_id = if fd == 1 {
            1 // stdout cap_id
        } else if fd == 2 {
            2 // stderr cap_id
        } else {
            return Err(AbiError::Other("Unsupported FD"));
        };

        // Invoke syscall (this is a simplified example; would need raw syscall asm)
        // For now, we just perform the write operation directly as done previously.
        // Invoke syscall
        for &b in buf {
            crate::print!("{}", b as char);
        }
        
        Ok(buf.len() as u32)
    }

    pub fn fd_read(fd: i32, buf: &mut [u8]) -> Result<u32, AbiError> {
        if fd == 0 {
            // Use stdin reader
            let count = crate::drivers::keyboard::read_stdin(buf);
            Ok(count as u32)
        } else {
            Err(AbiError::Other("Unsupported FD for read"))
        }
    }

    pub fn proc_exit(status: i64) {
        println!("[WASM Runtime] proc_exit called with status {}", status);
        let current_pid = crate::process::scheduler::with_current_task(|p| p.pid);
        if let Some(pid) = current_pid {
            x86_64::instructions::interrupts::without_interrupts(|| {
                crate::process::scheduler::SCHEDULER.exit_process(pid, status);
            });
        }
    }
}