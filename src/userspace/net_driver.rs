// src/userspace/net_driver.rs
#![no_std]
#![no_main]

use core::panic::PanicInfo;

const PCI_VENDOR_VIRTIO: u16 = 0x1AF4;
const PCI_DEVICE_VIRTIO_NET_LEGACY: u16 = 0x1000;
const PCI_WILDCARD: u16 = 0xFFFF;
const BAR_IS_IO: u64 = 1 << 63;

#[repr(C)]
#[derive(Clone, Copy)]
struct VirtQueueDesc {
    addr: u64,
    len: u32,
    flags: u16,
    next: u16,
}

#[repr(C)]
struct VirtQueueAvail {
    flags: u16,
    idx: u16,
    ring: [u16; 256],
    used_event: u16,
}

#[repr(C)]
struct VirtQueueUsedElem {
    id: u32,
    len: u32,
}

#[repr(C)]
struct VirtQueueUsed {
    flags: u16,
    idx: u16,
    ring: [VirtQueueUsedElem; 256],
    avail_event: u16,
}

const VQ_DESC_F_WRITE: u16 = 2;
const NUM_DESC: usize = 256;
const QUEUE_MEM_SIZE: usize = 16384;
static mut RX_QUEUE_DMA: [u8; QUEUE_MEM_SIZE] = [0u8; QUEUE_MEM_SIZE];
static mut TX_QUEUE_DMA: [u8; QUEUE_MEM_SIZE] = [0u8; QUEUE_MEM_SIZE];
const RX_BUF_SIZE: usize = 1526;
static mut RX_BUFFERS: [[u8; RX_BUF_SIZE]; 256] = [[0u8; RX_BUF_SIZE]; 256];
    mem: *mut u8,
    size: u16,
    last_used_idx: u16,
}

impl VirtQueueLegacy {
    fn new(mem: *mut u8, size: u16) -> Self {
        Self { mem, size, last_used_idx: 0 }
    }

    fn desc(&self, i: usize) -> &mut VirtQueueDesc {
        unsafe { &mut *(self.mem as *mut VirtQueueDesc).add(i) }
    }

    fn avail_mut(&self) -> &mut VirtQueueAvail {
        let off = self.size as usize * 16;
        unsafe { &mut *(self.mem.add(off) as *mut VirtQueueAvail) }
    }

    fn used(&self) -> &VirtQueueUsed {
        let desc_bytes = self.size as usize * 16;
        let avail_bytes = 6 + self.size as usize * 2;
        let off = (desc_bytes + avail_bytes + 4095) & !4095;
        unsafe { &*(self.mem.add(off) as *const VirtQueueUsed) }
    }

    fn reclaim_completed(&mut self) {
        while self.last_used_idx != self.used().idx {
            self.last_used_idx = self.last_used_idx.wrapping_add(1);
        }
    }
}

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
    
    // We need the physical addresses of the RX and TX queues for virtio ring setup.
    let rx_phys = syscall_dev_virt_to_phys(unsafe { RX_QUEUE_DMA.as_ptr() as u64 });
    let tx_phys = syscall_dev_virt_to_phys(unsafe { TX_QUEUE_DMA.as_ptr() as u64 });
    if is_err(rx_phys) || is_err(tx_phys) {
        sys_write(1, b"[Userspace Net] Failed to get physical address for DMA queues\n");
        park();
    }
    // Pass the physical addresses (PFN) to the device
    // PFN = physical address / 4096 (assuming 4KB pages)
    // Select RX Queue (index 0)
    syscall_dev_port_out(io_base + 0x0E, 2, 0);
    syscall_dev_port_out(io_base + 0x08, 4, rx_phys / 4096);
    // Select TX Queue (index 1)
    syscall_dev_port_out(io_base + 0x0E, 2, 1);
    syscall_dev_port_out(io_base + 0x08, 4, tx_phys / 4096);
    let mut rx_q = VirtQueueLegacy::new(unsafe { RX_QUEUE_DMA.as_mut_ptr() }, 256);
    let mut tx_q = VirtQueueLegacy::new(unsafe { TX_QUEUE_DMA.as_mut_ptr() }, 256);
    // Populate RX queue with buffers so we can actually receive packets
    for i in 0..256 {
        let phys = syscall_dev_virt_to_phys(unsafe { RX_BUFFERS[i].as_ptr() as u64 });
        let desc = rx_q.desc(i);
        desc.addr = phys;
        desc.len = RX_BUF_SIZE as u32;
        desc.flags = VQ_DESC_F_WRITE;
        desc.next = 0;
        rx_q.avail_mut().ring[i] = i as u16;
    }
    rx_q.avail_mut().idx = 256;
    // Kick RX Queue
    syscall_dev_port_out(io_base + 0x10, 2, 0);
    sys_write(1, b"[Userspace Net] Device acknowledged. Starting event loop...\n");

    // 2. Main Event Loop
    loop {
        // Wait for IRQ from kernel
        syscall_dev_irq_wait(irq as u8);
        
        // IRQ fired! Check ISR
        let isr = syscall_dev_port_in(io_base + PCI_ISR, 1) as u8;
        if isr & 1 != 0 {
            sys_write(1, b"[Userspace Net] Received Packet IRQ!\n");
            rx_q.reclaim_completed();
            tx_q.reclaim_completed();
            // Check for new RX packets
            while rx_q.last_used_idx != rx_q.used().idx {
                let used_idx = rx_q.last_used_idx as usize % 256;
                let elem = &rx_q.used().ring[used_idx];
                let desc_id = elem.id as usize;
                // We got a packet!
                sys_write(1, b"[Userspace Net] Packet received!\n");
                // Re-enqueue the buffer
                let avail_idx = rx_q.avail_mut().idx as usize % 256;
                rx_q.avail_mut().ring[avail_idx] = desc_id as u16;
                // barrier here in real impl
                rx_q.avail_mut().idx = rx_q.avail_mut().idx.wrapping_add(1);
                rx_q.last_used_idx = rx_q.last_used_idx.wrapping_add(1);
            }
            // Kick RX Queue again
            syscall_dev_port_out(io_base + 0x10, 2, 0);
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
fn syscall_dev_virt_to_phys(virt: u64) -> u64 {
    let res: u64;
    unsafe {
        core::arch::asm!("syscall", in("rax") 1038, in("rdi") virt, lateout("rax") res);
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
