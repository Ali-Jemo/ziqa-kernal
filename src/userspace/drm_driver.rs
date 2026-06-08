// src/userspace/drm_driver.rs
#![no_std]
#![no_main]

use core::panic::PanicInfo;

/// DRM ioctl command numbers (matching kernel)
pub const MODE_FB_CREATE: u64 = 0xc0286417;
pub const MODE_PAGE_FLIP: u64 = 0xc0206407;

#[no_mangle]
pub extern "C" fn _start() -> ! {
    sys_write(1, b"[Userspace DRM] Driver started\n");

    // 1. Discover GPU IPC channel
    let gpu_chan = syscall_get_gpu_chan();
    if gpu_chan == 0 {
        sys_write(1, b"[Userspace DRM] Failed to get GPU channel\n");
        loop { syscall_yield(); }
    }
    sys_write(1, b"[Userspace DRM] Connected to GPU channel\n");

    // 2. Main Event Loop
    loop {
        // Send a draw test pattern command (code 2)
        let cmd = [2u8]; 
        syscall_ipc_send(gpu_chan, cmd.as_ptr(), 1);
        
        // Wait and flush
        let flush = [1u8];
        syscall_ipc_send(gpu_chan, flush.as_ptr(), 1);

        for _ in 0..1000000 { syscall_yield(); }
    }
}

// ── Syscall Wrappers ─────────────────────────────────────────────────────────

#[inline(always)]
fn syscall_get_gpu_chan() -> u32 {
    let res: u64;
    unsafe {
        core::arch::asm!("syscall", in("rax") 1040, lateout("rax") res);
    }
    res as u32
}

#[inline(always)]
fn syscall_ipc_send(chan: u32, ptr: *const u8, len: usize) {
    unsafe {
        core::arch::asm!("syscall", in("rax") 1021, in("rdi") chan as u64, in("rsi") ptr as u64, in("rdx") len as u64);
    }
}
// ... (rest of the file)

// ── Syscall Wrappers ─────────────────────────────────────────────────────────

#[inline(always)]
fn sys_write(fd: u64, buf: &[u8]) {
    unsafe {
        core::arch::asm!("syscall", in("rax") 1, in("rdi") fd, in("rsi") buf.as_ptr() as u64, in("rdx") buf.len() as u64);
    }
}

#[inline(always)]
fn syscall_dev_map(phys: u64, size: usize) -> u64 {
    let res: u64;
    unsafe {
        core::arch::asm!("syscall", in("rax") 1033, in("rdi") phys, in("rsi") size as u64, lateout("rax") res);
    }
    res
}

#[inline(always)]
fn syscall_dev_port_out(port: u16, size: u64, val: u64) {
    unsafe {
        core::arch::asm!("syscall", in("rax") 1032, in("rdi") port as u64, in("rsi") size, in("rdx") val);
    }
}

#[inline(always)]
fn syscall_ipc_create() -> u32 {
    let res: u64;
    unsafe {
        core::arch::asm!("syscall", in("rax") 1020, lateout("rax") res);
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
