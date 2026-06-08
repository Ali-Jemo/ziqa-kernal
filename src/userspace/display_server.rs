#![no_std]
#![no_main]

use core::panic::PanicInfo;
use crate::ipc::gui::{OpCode, CreateSurfaceMsg, FlushMsg};

#[no_mangle]
pub extern "C" fn _start() -> ! {
    sys_write(1, b"[Display Server] Initializing...\n");

    // 1. Discover GPU IPC channel
    let gpu_chan = syscall_get_gpu_chan();
    if gpu_chan == 0 {
        sys_write(1, b"[Display Server] Failed to get GPU channel\n");
        loop { syscall_yield(); }
    }
    sys_write(1, b"[Display Server] Connected. Listening for IPC messages...\n");

    // 2. Main Event Loop
    loop {
        let mut msg_data = [0u8; 256];
        // Wait for incoming IPC messages (Wait/Recv call)
        let n = syscall_ipc_recv(gpu_chan, msg_data.as_mut_ptr(), 256);
        
        if n > 0 {
            let opcode = msg_data[0];
            match opcode {
                x if x == OpCode::Connect as u8 => {
                    sys_write(1, b"[Display Server] Client connected.\n");
                }
                x if x == OpCode::CreateSurface as u8 => {
                    sys_write(1, b"[Display Server] Creating surface...\n");
                    // Logic to track surface in SHM
                }
                x if x == OpCode::Flush as u8 => {
                    sys_write(1, b"[Display Server] Flushing region...\n");
                    // Forward Flush command to kernel driver (Code 1)
                    let cmd = [1u8];
                    syscall_ipc_send(gpu_chan, cmd.as_ptr(), 1);
                }
                _ => {
                    sys_write(1, b"[Display Server] Unknown opcode received\n");
                }
            }
        }
        syscall_yield();
    }
}

#[inline(always)]
fn syscall_ipc_send(chan: u32, ptr: *const u8, len: usize) {
    unsafe {
        core::arch::asm!("syscall", in("rax") 1021, in("rdi") chan as u64, in("rsi") ptr as u64, in("rdx") len as u64);
    }
}

// ── Syscall Wrappers ─────────────────────────────────────────────────────────
// (Included for local compilation of the Display Server)

#[inline(always)]
fn sys_write(fd: u64, buf: &[u8]) {
    unsafe {
        core::arch::asm!("syscall", in("rax") 1, in("rdi") fd, in("rsi") buf.as_ptr() as u64, in("rdx") buf.len() as u64);
    }
}

#[inline(always)]
fn syscall_get_gpu_chan() -> u32 {
    let res: u64;
    unsafe {
        core::arch::asm!("syscall", in("rax") 1040, lateout("rax") res);
    }
    res as u32
}

#[inline(always)]
fn syscall_ipc_recv(chan: u32, ptr: *mut u8, len: usize) -> usize {
    let res: u64;
    unsafe {
        core::arch::asm!("syscall", in("rax") 1022, in("rdi") chan as u64, in("rsi") ptr as u64, in("rdx") len as u64, lateout("rax") res);
    }
    res as usize
}

#[inline(always)]
fn syscall_yield() {
    unsafe {
        core::arch::asm!("syscall", in("rax") 24);
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {
        syscall_yield();
    }
}
