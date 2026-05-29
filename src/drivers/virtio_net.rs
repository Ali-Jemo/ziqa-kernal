#![allow(static_mut_refs)]

/// VirtIO Network Driver for ZiqaKernel
///
/// Supports VirtIO PCI legacy I/O port transport (transitional `virtio-net-pci`).
///
/// Uses the VirtQueue structure for packet transmission and reception.
use core::sync::atomic::{compiler_fence, Ordering};
use x86_64::instructions::port::Port;

use crate::drivers::virtio_net_proto::*;
use crate::println;
use crate::zig_kernel_ops;

// ── Legacy PCI I/O register offsets ───────────────────────────────────────
const PCI_HOST_FEATURES: u16 = 0x00;
const PCI_GUEST_FEATURES: u16 = 0x04;
const PCI_QUEUE_ADDRESS: u16 = 0x08;
const PCI_QUEUE_SIZE: u16 = 0x0C;
const PCI_QUEUE_SEL: u16 = 0x0E;
const PCI_QUEUE_NOTIFY: u16 = 0x10;
const PCI_DEVICE_STATUS: u16 = 0x12;
const PCI_ISR: u16 = 0x14;
const PCI_DEVICE_CFG: u16 = 0x18;

/// Contiguous virtqueue memory for legacy layout.
/// Descriptors, avail ring, used ring live in one physically-contiguous block.
const QUEUE_MEM_SIZE: usize = 8192;

struct VirtQueueLegacy {
    mem: &'static mut [u8],
    size: u16,
    last_used_idx: u16,
    last_avail_idx: u16,
}

impl VirtQueueLegacy {
    fn desc(&mut self, i: usize) -> &mut VirtQueueDesc {
        let ptr = self.mem.as_ptr() as *mut VirtQueueDesc;
        unsafe { &mut *ptr.add(i) }
    }

    fn avail(&self) -> &VirtQueueAvail {
        let offset = (self.size as usize * 16 + 1) & !1;
        unsafe { &*(self.mem.as_ptr().add(offset) as *const VirtQueueAvail) }
    }

    fn avail_mut(&mut self) -> &mut VirtQueueAvail {
        let offset = (self.size as usize * 16 + 1) & !1;
        unsafe { &mut *(self.mem.as_mut_ptr().add(offset) as *mut VirtQueueAvail) }
    }

    fn used(&self) -> &VirtQueueUsed {
        let avail_end = 4 + self.size as usize * 2;
        let offset = ((self.size as usize * 16) + avail_end + 3) & !3;
        unsafe { &*(self.mem.as_ptr().add(offset) as *const VirtQueueUsed) }
    }

    fn used_mut(&mut self) -> &mut VirtQueueUsed {
        let avail_end = 4 + self.size as usize * 2;
        let offset = ((self.size as usize * 16) + avail_end + 3) & !3;
        unsafe { &mut *(self.mem.as_mut_ptr().add(offset) as *mut VirtQueueUsed) }
    }

    fn pfn(&self) -> u32 {
        (self.mem.as_ptr() as u64 / 4096) as u32
    }

    fn rx_available(&self) -> bool {
        compiler_fence(Ordering::Acquire);
        self.last_used_idx != self.used().idx
    }

    fn tx_reclaim(&mut self) {
        while self.last_used_idx != self.used().idx {
            compiler_fence(Ordering::Acquire);
            self.last_used_idx = self.last_used_idx.wrapping_add(1);
        }
    }
}

/// VirtIO-net device configuration
pub struct VirtioNet {
    pub io_base: u16,
    pub mac: [u8; 6],
    rx_queue: VirtQueueLegacy,
    tx_queue: VirtQueueLegacy,
}

// ── Static queue memory ───────────────────────────────────────────────────
static mut RX_QUEUE_MEM: [u8; QUEUE_MEM_SIZE] = [0; QUEUE_MEM_SIZE];
static mut TX_QUEUE_MEM: [u8; QUEUE_MEM_SIZE] = [0; QUEUE_MEM_SIZE];

static mut RX_BUFFERS: [[u8; 1536]; 256] = [[0; 1536]; 256];
static mut TX_BUFFERS: [[u8; 1536]; 256] = [[0; 1536]; 256];

impl VirtioNet {
    pub fn new(io_base: u16, mac: [u8; 6]) -> Self {
        Self {
            io_base,
            mac,
            rx_queue: VirtQueueLegacy {
                mem: unsafe { &mut RX_QUEUE_MEM },
                size: 256,
                last_used_idx: 0,
                last_avail_idx: 0,
            },
            tx_queue: VirtQueueLegacy {
                mem: unsafe { &mut TX_QUEUE_MEM },
                size: 256,
                last_used_idx: 0,
                last_avail_idx: 0,
            },
        }
    }

    // ── Legacy PCI I/O port access ─────────────────────────────────────────

    fn io_read32(&self, reg: u16) -> u32 {
        unsafe { Port::new(self.io_base + reg).read() }
    }

    fn io_write32(&self, reg: u16, val: u32) {
        unsafe { Port::new(self.io_base + reg).write(val) }
    }

    fn io_write16(&self, reg: u16, val: u16) {
        unsafe { Port::<u16>::new(self.io_base + reg).write(val) }
    }

    fn io_write8(&self, reg: u16, val: u8) {
        unsafe { Port::<u8>::new(self.io_base + reg).write(val) }
    }

    fn io_read8(&self, reg: u16) -> u8 {
        unsafe { Port::<u8>::new(self.io_base + reg).read() }
    }

    /// Write to a device register (used by syscall interface).
    /// Maps old MMIO offsets to PCI I/O port offsets.
    pub fn write_config_32(&self, mmio_offset: u32, value: u32) {
        #[allow(clippy::single_match)]
        match mmio_offset {
            0x050 => {
                // MMIO QueueNotify → PCI QueueNotify at offset 0x10
                self.io_write16(PCI_QUEUE_NOTIFY, value as u16);
            }
            _ => {}
        }
    }

    /// Acknowledge an interrupt
    pub fn ack_interrupt(&self) {
        let _ = self.io_read8(PCI_ISR);
    }

    // ── Virtqueue setup ────────────────────────────────────────────────────

    fn setup_legacy_queue(io_base: u16, q: &mut VirtQueueLegacy, queue_index: u16) {
        unsafe { Port::<u16>::new(io_base + PCI_QUEUE_SEL).write(queue_index) };
        let max_size: u16 = unsafe { Port::<u16>::new(io_base + PCI_QUEUE_SIZE).read() };
        let size = q.size.min(max_size);
        q.size = size;
        unsafe { Port::<u16>::new(io_base + PCI_QUEUE_SIZE).write(size) };
        unsafe { Port::<u32>::new(io_base + PCI_QUEUE_ADDRESS).write(q.pfn()) };

        for i in 0..size as usize {
            let desc = q.desc(i);
            desc.addr = 0;
            desc.len = 0;
            desc.flags = 0;
            desc.next = 0;
        }
        let avail = q.avail_mut();
        avail.flags = 0;
        avail.idx = 0;
        let used = q.used_mut();
        used.flags = 0;
        used.idx = 0;
        q.last_used_idx = 0;
        q.last_avail_idx = 0;
    }

    fn fill_rx_buffers(&mut self) {
        let q = &mut self.rx_queue;
        for i in 0..q.size as usize {
            let desc = q.desc(i);
            desc.addr = unsafe { RX_BUFFERS[i].as_ptr() as u64 };
            desc.len = 1536;
            desc.flags = VQ_DESC_F_WRITE;
            desc.next = 0;
            q.avail_mut().ring[i] = i as u16;
        }
        q.avail_mut().idx = q.size;
        q.last_avail_idx = q.size;
        compiler_fence(Ordering::Release);
    }

    // ── Packet operations ──────────────────────────────────────────────────

    /// Check if a packet is available to receive
    pub fn rx_available(&self) -> bool {
        self.rx_queue.rx_available()
    }

    /// Receive a packet from the device
    pub fn receive(&mut self) -> Option<([u8; 1500], usize)> {
        if !self.rx_queue.rx_available() {
            return None;
        }

        let q = &mut self.rx_queue;
        let used_ring = q.used().ring;
        let used_idx = q.last_used_idx % q.size;
        let used_elem = &used_ring[used_idx as usize];
        let id = used_elem.id as usize;
        let total_len = used_elem.len as usize;

        let hdr_size = core::mem::size_of::<VirtioNetHdr>();
        if total_len <= hdr_size {
            q.last_used_idx = q.last_used_idx.wrapping_add(1);
            return None;
        }

        let packet_len = total_len - hdr_size;
        let mut packet = [0u8; 1500];
        let len_to_copy = core::cmp::min(packet_len, 1500);

        unsafe {
            zig_kernel_ops::packet_copy(
                &mut packet,
                &RX_BUFFERS[id][hdr_size..hdr_size + len_to_copy],
            );
        }

        q.last_used_idx = q.last_used_idx.wrapping_add(1);

        let avail_idx = q.avail().idx % q.size;
        q.avail_mut().ring[avail_idx as usize] = id as u16;
        compiler_fence(Ordering::Release);
        q.avail_mut().idx = q.avail().idx.wrapping_add(1);
        q.last_avail_idx = q.last_avail_idx.wrapping_add(1);
        compiler_fence(Ordering::SeqCst);

        self.io_write16(PCI_QUEUE_NOTIFY, 0);

        Some((packet, len_to_copy))
    }

    /// Transmit a packet to the device
    pub fn transmit(&mut self, data: &[u8]) -> Result<(), ()> {
        let q = &mut self.tx_queue;
        q.tx_reclaim();

        let id = (q.last_avail_idx % q.size) as usize;
        let hdr_size = core::mem::size_of::<VirtioNetHdr>();
        let mut buf = [0u8; 1536];

        let hdr = VirtioNetHdr::default();
        unsafe {
            core::ptr::copy_nonoverlapping(
                &hdr as *const _ as *const u8,
                buf.as_mut_ptr(),
                hdr_size,
            );
        }

        let len_to_copy = zig_kernel_ops::packet_copy(&mut buf[hdr_size..], data);

        unsafe {
            TX_BUFFERS[id] = buf;
            q.desc(id).addr = TX_BUFFERS[id].as_ptr() as u64;
            q.desc(id).len = (hdr_size + len_to_copy) as u32;
            q.desc(id).flags = 0;
        }

        let avail_idx = q.avail().idx % q.size;
        q.avail_mut().ring[avail_idx as usize] = id as u16;

        compiler_fence(Ordering::Release);
        q.avail_mut().idx = q.avail().idx.wrapping_add(1);
        q.last_avail_idx = q.last_avail_idx.wrapping_add(1);
        compiler_fence(Ordering::SeqCst);

        self.io_write16(PCI_QUEUE_NOTIFY, 1);

        Ok(())
    }

    /// Test hook: inject a packet into the RX VirtQueue
    pub fn inject_rx_for_test(&mut self, reply_data: &[u8]) {
        let q = &mut self.rx_queue;
        let id = (q.last_used_idx % q.size) as usize;

        let hdr_size = core::mem::size_of::<VirtioNetHdr>();
        let mut buf = [0u8; 1536];

        let hdr = VirtioNetHdr::default();
        unsafe {
            core::ptr::copy_nonoverlapping(
                &hdr as *const _ as *const u8,
                buf.as_mut_ptr(),
                hdr_size,
            );
        }

        let len_to_copy = zig_kernel_ops::packet_copy(&mut buf[hdr_size..], reply_data);

        unsafe {
            RX_BUFFERS[id] = buf;
            let used_idx = q.used().idx % q.size;
            q.used_mut().ring[used_idx as usize].id = id as u32;
            q.used_mut().ring[used_idx as usize].len = (hdr_size + len_to_copy) as u32;

            compiler_fence(Ordering::Release);
            q.used_mut().idx = q.used().idx.wrapping_add(1);
            compiler_fence(Ordering::SeqCst);
        }
    }
}

// ── Global instance ───────────────────────────────────────────────────────

pub static mut VIRTIO_NET: Option<VirtioNet> = None;

// ── PCI bus scan ──────────────────────────────────────────────────────────

const VIRTIO_VENDOR: u16 = 0x1AF4;
const VIRTIO_DEVICE_NET: u16 = 0x1000; // transitional

fn pci_config_read(bus: u8, slot: u8, func: u8, offset: u8) -> u32 {
    let addr = 0x8000_0000u32
        | ((bus as u32) << 16)
        | ((slot as u32) << 11)
        | ((func as u32) << 8)
        | (offset as u32 & 0xFC);
    unsafe {
        Port::<u32>::new(0xCF8).write(addr);
        Port::<u32>::new(0xCFC).read()
    }
}

fn pci_find_virtio_net() -> Option<(u16, [u8; 6])> {
    for slot in 0..32 {
        let vid_did = pci_config_read(0, slot, 0, 0);
        let vendor = vid_did as u16;
        let device = (vid_did >> 16) as u16;
        if vendor != VIRTIO_VENDOR || (device != VIRTIO_DEVICE_NET && device != 0x1041) {
            continue;
        }
        let class_rev = pci_config_read(0, slot, 0, 0x08);
        let class = (class_rev >> 16) as u16;
        if class != 0x0200 {
            continue;
        }
        let bar0 = pci_config_read(0, slot, 0, 0x10);
        if bar0 & 1 == 0 {
            continue;
        }
        let io_base = (bar0 & !0x03) as u16;

        // Try reading MAC from device config (offset 0x18 from I/O base).
        // If the result is invalid (e.g. unmapped port returns 0xFF), fall back
        // to a reasonable default (QEMU's usual auto-assigned MAC prefix).
        let mac_bytes = [
            unsafe { Port::<u8>::new(io_base + PCI_DEVICE_CFG + 0).read() },
            unsafe { Port::<u8>::new(io_base + PCI_DEVICE_CFG + 1).read() },
            unsafe { Port::<u8>::new(io_base + PCI_DEVICE_CFG + 2).read() },
            unsafe { Port::<u8>::new(io_base + PCI_DEVICE_CFG + 3).read() },
            unsafe { Port::<u8>::new(io_base + PCI_DEVICE_CFG + 4).read() },
            unsafe { Port::<u8>::new(io_base + PCI_DEVICE_CFG + 5).read() },
        ];

        let mac = if mac_bytes[4] == 0xFF || mac_bytes[5] == 0xFF {
            // Device config read returned unmapped-port sentinel — use QEMU default
            [0x52, 0x54, 0x00, 0x12, 0x34, 0x56]
        } else {
            mac_bytes
        };

        return Some((io_base, mac));
    }
    None
}

// ── Initialization ────────────────────────────────────────────────────────

/// Initialize the VirtIO-net driver (PCI legacy I/O port transport).
pub fn init() -> Result<(), ()> {
    let (io_base, mac) = pci_find_virtio_net().ok_or(())?;

    println!(
        "[VirtIO-net] Found PCI device at I/O base 0x{:04x}, MAC: {:02x?}",
        io_base, mac
    );

    unsafe {
        VIRTIO_NET = Some(VirtioNet::new(io_base, mac));

        if let Some(net) = &mut VIRTIO_NET {
            // 1. Reset
            net.io_write8(PCI_DEVICE_STATUS, 0);

            // 2. Acknowledge + Driver
            net.io_write8(PCI_DEVICE_STATUS, 1);
            net.io_write8(PCI_DEVICE_STATUS, 3);

            // 3. Feature negotiation
            let features = net.io_read32(PCI_HOST_FEATURES);
            net.io_write32(PCI_GUEST_FEATURES, features);

            // 4. FEATURES_OK
            net.io_write8(PCI_DEVICE_STATUS, 11);

            // 5. Setup queues (1=TX, 0=RX)
            VirtioNet::setup_legacy_queue(net.io_base, &mut net.tx_queue, 1);
            VirtioNet::setup_legacy_queue(net.io_base, &mut net.rx_queue, 0);

            // 6. Fill RX
            net.fill_rx_buffers();

            // 7. DRIVER_OK
            net.io_write8(PCI_DEVICE_STATUS, 15);

            compiler_fence(Ordering::SeqCst);

            // Notify about RX
            net.io_write16(PCI_QUEUE_NOTIFY, 0);
        }
    }

    let eth_device = crate::net::NetDevice::physical("eth0", mac);
    crate::net::NET.lock().add_device(eth_device);

    println!(
        "[VirtIO-net] Initialized and registered eth0 (MAC: {:02x?})",
        mac
    );
    Ok(())
}
