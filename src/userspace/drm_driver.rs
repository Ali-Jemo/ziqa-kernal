// src/userspace/drm_driver.rs
#![no_std]
#![no_main]

use core::panic::PanicInfo;

/// DRM ioctl command numbers (matching kernel)
pub const MODE_FB_CREATE: u64 = 0xc0286417;
pub const MODE_PAGE_FLIP: u64 = 0xc0206407;

#[no_mangle]
pub extern "C" fn _start() -> ! {
    // 1. Log startup
    sys_write(1, b"[Userspace DRM] Driver started (Microkernel Mode)\n");

    // 2. Request Hardware Access (MMIO for Framebuffer)
    // In QEMU with virtio-gpu or standard VGA, BAR0 is often at 0xFD000000 or similar.
    let phys_gpu_bar = 0xFD000000;
    let bar_size = 0x1000000; // 16MB
    
    let virt_addr = syscall_dev_map(phys_gpu_bar, bar_size);
    if virt_addr > 0 {
        sys_write(1, b"[Userspace DRM] Successfully mapped GPU MMIO\n");
    } else {
        sys_write(1, b"[Userspace DRM] Failed to map GPU MMIO (Permission Denied?)\n");
    }

    // 3. Create an IPC channel to receive ioctls from the kernel gateway
    let chan_id = syscall_ipc_create();
    
    // 4. Main Event Loop
    loop {
        let mut msg = [0u8; 64];
        let n = syscall_ipc_recv(chan_id, msg.as_mut_ptr(), 64);
        
        if n > 0 {
            // In a real implementation, we would decode the message
            // and perform the hardware operations.
            sys_write(1, b"[Userspace DRM] Received redirected ioctl request\n");
            
            // Example: hardware touch via I/O ports
            syscall_dev_port_out(0x3D4, 1, 0x0E); // VGA CRTC Index
        }
        
        // Yield to other processes
        syscall_yield();
    }
}

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
