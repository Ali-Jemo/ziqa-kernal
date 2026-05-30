// src/userspace/net_driver.rs
#![no_std]
#![no_main]

use core::panic::PanicInfo;

/// VirtIO-Net I/O base (example, should be discovered via PCI in a real driver)
const IO_BASE: u16 = 0xC000;
const IRQ_VECTOR: u8 = 43; // Example IRQ

#[no_mangle]
pub extern "C" fn _start() -> ! {
    sys_write(1, b"[Userspace Net] Driver starting...\n");

    // 1. Initialize VirtIO-Net via I/O Ports
    // Reset device
    syscall_dev_port_out(IO_BASE + 18, 1, 0); // PCI_DEVICE_STATUS = 0
    
    // Acknowledge + Driver
    syscall_dev_port_out(IO_BASE + 18, 1, 1);
    syscall_dev_port_out(IO_BASE + 18, 1, 3);

    sys_write(1, b"[Userspace Net] Device acknowledged. Waiting for interrupts...\n");

    // 2. Main Event Loop
    loop {
        // Wait for IRQ from kernel
        syscall_dev_irq_wait(IRQ_VECTOR);
        
        // IRQ fired! Check ISR
        let isr = syscall_dev_port_in(IO_BASE + 20, 1) as u8; // PCI_ISR
        if isr & 1 != 0 {
            sys_write(1, b"[Userspace Net] Received Packet IRQ!\n");
            // Perform packet RX logic...
        }
        
        // Yield if nothing else to do
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
fn syscall_dev_port_in(port: u16, size: u64) -> u64 {
    let res: u64;
    unsafe {
        core::arch::asm!("syscall", in("rax") 1031, in("rdi") port as u64, in("rsi") size, lateout("rax") res);
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
fn syscall_dev_irq_wait(irq: u8) {
    unsafe {
        core::arch::asm!("syscall", in("rax") 1034, in("rdi") irq as u64);
    }
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
