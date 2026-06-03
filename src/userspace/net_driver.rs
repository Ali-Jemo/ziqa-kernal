// src/userspace/net_driver.rs
#![no_std]
#![no_main]

use core::panic::PanicInfo;

const PCI_VENDOR_VIRTIO: u16 = 0x1AF4;
const PCI_DEVICE_VIRTIO_NET_LEGACY: u16 = 0x1000;
const PCI_WILDCARD: u16 = 0xFFFF;
const BAR_IS_IO: u64 = 1 << 63;

const PCI_HOST_FEATURES: u16 = 0x00;
const PCI_GUEST_FEATURES: u16 = 0x04;
const PCI_DEVICE_STATUS: u16 = 0x12;
const PCI_ISR: u16 = 0x13;

const STATUS_RESET: u8 = 0x00;
const STATUS_ACKNOWLEDGE: u8 = 0x01;
const STATUS_DRIVER: u8 = 0x02;

#[no_mangle]
pub extern "C" fn _start() -> ! {
    sys_write(1, b"[Userspace Net] Driver starting...\n");

    let bdf = syscall_dev_pci_find(
        PCI_VENDOR_VIRTIO,
        PCI_DEVICE_VIRTIO_NET_LEGACY,
        PCI_WILDCARD,
        PCI_WILDCARD,
    );
    if is_err(bdf) {
        sys_write(1, b"[Userspace Net] virtio-net PCI device not found\n");
        park();
    }

    let bar0 = syscall_dev_pci_bar(bdf, 0);
    if is_err(bar0) || (bar0 & BAR_IS_IO) == 0 {
        sys_write(1, b"[Userspace Net] virtio-net BAR0 is not I/O space\n");
        park();
    }
    let io_base = (bar0 & !BAR_IS_IO) as u16;

    let irq = syscall_dev_pci_irq(bdf);
    if is_err(irq) {
        sys_write(1, b"[Userspace Net] virtio-net IRQ line unavailable\n");
        park();
    }

    // 1. Initialize VirtIO-Net via the discovered legacy I/O BAR.
    syscall_dev_port_out(io_base + PCI_DEVICE_STATUS, 1, STATUS_RESET as u64);
    
    // Acknowledge + Driver
    syscall_dev_port_out(io_base + PCI_DEVICE_STATUS, 1, STATUS_ACKNOWLEDGE as u64);
    syscall_dev_port_out(
        io_base + PCI_DEVICE_STATUS,
        1,
        (STATUS_ACKNOWLEDGE | STATUS_DRIVER) as u64,
    );

    let _features = syscall_dev_port_in(io_base + PCI_HOST_FEATURES, 4);
    syscall_dev_port_out(io_base + PCI_GUEST_FEATURES, 4, 0);

    sys_write(1, b"[Userspace Net] Device acknowledged. Waiting for interrupts...\n");

    // 2. Main Event Loop
    loop {
        // Wait for IRQ from kernel
        syscall_dev_irq_wait(irq as u8);
        
        // IRQ fired! Check ISR
        let isr = syscall_dev_port_in(io_base + PCI_ISR, 1) as u8;
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
fn is_err(value: u64) -> bool {
    (value as i64) < 0
}

fn park() -> ! {
    loop {
        syscall_yield();
    }
}

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
fn syscall_dev_pci_find(vendor: u16, device: u16, class: u16, subclass: u16) -> u64 {
    let res: u64;
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") 1035,
            in("rdi") vendor as u64,
            in("rsi") device as u64,
            in("rdx") class as u64,
            in("r10") subclass as u64,
            lateout("rax") res,
        );
    }
    res
}

#[inline(always)]
fn syscall_dev_pci_bar(bdf: u64, bar: u64) -> u64 {
    let res: u64;
    unsafe {
        core::arch::asm!("syscall", in("rax") 1036, in("rdi") bdf, in("rsi") bar, lateout("rax") res);
    }
    res
}

#[inline(always)]
fn syscall_dev_pci_irq(bdf: u64) -> u64 {
    let res: u64;
    unsafe {
        core::arch::asm!("syscall", in("rax") 1037, in("rdi") bdf, lateout("rax") res);
    }
    res
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
