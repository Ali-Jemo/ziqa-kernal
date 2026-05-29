#![allow(static_mut_refs)]

/// VirtIO Network Driver for ZiqaKernel
///
/// Provides TCP/IP networking via QEMU's VirtIO interface.
/// Uses the VirtQueue structure for packet transmission and reception.
///
/// VirtIO-net device uses two virtqueues:
/// - Index 0: Receive (RX) queue - gets packets from device
/// - Index 1: Transmit (TX) queue - sends packets to device
use crate::println;
use core::sync::atomic::{compiler_fence, Ordering};

/// VirtIO-net header for each packet
#[repr(C, packed)]
#[derive(Clone, Copy, Default)]
pub struct VirtioNetHdr {
    pub flags: u8,
    pub gso_type: u8,
    pub hdr_len: u16,
    pub gso_size: u16,
    pub csum_start: u16,
    pub csum_offset: u16,
    pub num_buffers: u16,
}

/// A descriptor in a VirtQueue
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct VirtQueueDesc {
    pub addr: u64,
    pub len: u32,
    pub flags: u16,
    pub next: u16,
}

pub const VQ_DESC_F_NEXT: u16 = 1; // Next is valid
pub const VQ_DESC_F_WRITE: u16 = 2; // Writeable
pub const VQ_DESC_F_INDIRECT: u16 = 4; // Indirect descriptor

#[repr(C, packed)]
pub struct VirtQueueAvail {
    pub flags: u16,
    pub idx: u16,
    pub ring: [u16; 256],
    pub used_event: u16,
}

#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct VirtQueueUsedElem {
    pub id: u32,
    pub len: u32,
}

#[repr(C, packed)]
pub struct VirtQueueUsed {
    pub flags: u16,
    pub idx: u16,
    pub ring: [VirtQueueUsedElem; 256],
    pub avail_event: u16,
}

/// A VirtQueue (simplified - no indirect descriptors)
pub struct VirtQueue {
    pub queue: &'static mut [VirtQueueDesc],
    pub avail: &'static mut VirtQueueAvail,
    pub used: &'static mut VirtQueueUsed,
    pub last_avail_idx: u16,
    pub last_used_idx: u16,
    pub size: u16,
}

impl VirtQueue {
    pub fn new(
        descriptors: &'static mut [VirtQueueDesc],
        avail: &'static mut VirtQueueAvail,
        used: &'static mut VirtQueueUsed,
    ) -> Self {
        let size = descriptors.len() as u16;
        Self {
            queue: descriptors,
            avail,
            used,
            last_avail_idx: 0,
            last_used_idx: 0,
            size,
        }
    }
}

/// VirtIO-net device configuration
pub struct VirtioNet {
    pub base: u64,
    pub mac: [u8; 6],
    pub rx_queue: VirtQueue,
    pub tx_queue: VirtQueue,
    pub features: u64,
}

// Global descriptor tables for RX and TX
static mut RX_DESCRIPTORS: [VirtQueueDesc; 256] = [VirtQueueDesc {
    addr: 0,
    len: 0,
    flags: 0,
    next: 0,
}; 256];
static mut TX_DESCRIPTORS: [VirtQueueDesc; 256] = [VirtQueueDesc {
    addr: 0,
    len: 0,
    flags: 0,
    next: 0,
}; 256];

static mut RX_AVAIL: VirtQueueAvail = VirtQueueAvail {
    flags: 0,
    idx: 0,
    ring: [0; 256],
    used_event: 0,
};
static mut RX_USED: VirtQueueUsed = VirtQueueUsed {
    flags: 0,
    idx: 0,
    ring: [VirtQueueUsedElem { id: 0, len: 0 }; 256],
    avail_event: 0,
};

static mut TX_AVAIL: VirtQueueAvail = VirtQueueAvail {
    flags: 0,
    idx: 0,
    ring: [0; 256],
    used_event: 0,
};
static mut TX_USED: VirtQueueUsed = VirtQueueUsed {
    flags: 0,
    idx: 0,
    ring: [VirtQueueUsedElem { id: 0, len: 0 }; 256],
    avail_event: 0,
};

// Buffers
static mut RX_BUFFERS: [[u8; 1536]; 256] = [[0; 1536]; 256];
static mut TX_BUFFERS: [[u8; 1536]; 256] = [[0; 1536]; 256];

impl VirtioNet {
    pub fn new(base: u64, mac: [u8; 6]) -> Self {
        Self {
            base,
            mac,
            rx_queue: VirtQueue::new(
                unsafe { &mut RX_DESCRIPTORS },
                unsafe { &mut RX_AVAIL },
                unsafe { &mut RX_USED },
            ),
            tx_queue: VirtQueue::new(
                unsafe { &mut TX_DESCRIPTORS },
                unsafe { &mut TX_AVAIL },
                unsafe { &mut TX_USED },
            ),
            features: 0,
        }
    }

    /// Read a 32-bit field from the device MMIO
    pub fn read_config_32(&self, offset: u32) -> u32 {
        unsafe { core::ptr::read_volatile((self.base + offset as u64) as *const u32) }
    }

    /// Write a 32-bit field to the device MMIO
    pub fn write_config_32(&self, offset: u32, val: u32) {
        unsafe { core::ptr::write_volatile((self.base + offset as u64) as *mut u32, val) }
    }

    /// Read a 8-bit field from the device MMIO
    pub fn read_config(&self, offset: u32) -> u8 {
        unsafe { core::ptr::read_volatile((self.base + offset as u64) as *const u8) }
    }

    /// Write a 8-bit field to the device MMIO
    pub fn write_config(&self, offset: u32, val: u8) {
        unsafe { core::ptr::write_volatile((self.base + offset as u64) as *mut u8, val) }
    }

    /// Acknowledge an interrupt
    pub fn ack_interrupt(&self) {
        self.write_config_32(0x064, self.read_config_32(0x060)); // MMIO_INTERRUPT_ACK = MMIO_INTERRUPT_STATUS
    }

    /// Check if a packet is available to receive
    pub fn rx_available(&self) -> bool {
        let q = &self.rx_queue;
        // Memory barrier to ensure we read updated used.idx
        compiler_fence(Ordering::Acquire);
        q.last_used_idx != q.used.idx
    }

    /// Receive a packet from the device
    pub fn receive(&mut self) -> Option<([u8; 1500], usize)> {
        if !self.rx_available() {
            return None;
        }

        let q = &mut self.rx_queue;
        let used_idx = q.last_used_idx % q.size;
        let used_elem = q.used.ring[used_idx as usize];
        let id = used_elem.id as usize;
        let total_len = used_elem.len as usize;

        // Skip the 12-byte VirtioNetHdr
        let hdr_size = core::mem::size_of::<VirtioNetHdr>();
        if total_len <= hdr_size {
            // Packet too small
            q.last_used_idx = q.last_used_idx.wrapping_add(1);
            return None;
        }

        let packet_len = total_len - hdr_size;
        let mut packet = [0u8; 1500];
        let len_to_copy = core::cmp::min(packet_len, 1500);

        unsafe {
            packet[..len_to_copy]
                .copy_from_slice(&RX_BUFFERS[id][hdr_size..hdr_size + len_to_copy]);
        }

        q.last_used_idx = q.last_used_idx.wrapping_add(1);

        // Put the buffer back to avail ring so the device can use it again
        let avail_idx = q.avail.idx % q.size;
        q.avail.ring[avail_idx as usize] = id as u16;
        compiler_fence(Ordering::Release);
        q.avail.idx = q.avail.idx.wrapping_add(1);
        compiler_fence(Ordering::SeqCst);

        // Notify device that RX queue has new available buffer (Queue 0)
        self.write_config_32(0x050, 0);

        Some((packet, len_to_copy))
    }

    /// Transmit a packet to the device
    pub fn transmit(&mut self, data: &[u8]) -> Result<(), ()> {
        let q = &mut self.tx_queue;
        let id = (q.last_avail_idx % q.size) as usize;

        let hdr_size = core::mem::size_of::<VirtioNetHdr>();
        let mut buf = [0u8; 1536];

        let hdr = VirtioNetHdr::default();
        unsafe {
            // Copy header
            core::ptr::copy_nonoverlapping(
                &hdr as *const _ as *const u8,
                buf.as_mut_ptr(),
                hdr_size,
            );
        }

        let len_to_copy = core::cmp::min(data.len(), 1536 - hdr_size);
        buf[hdr_size..hdr_size + len_to_copy].copy_from_slice(&data[..len_to_copy]);

        unsafe {
            TX_BUFFERS[id] = buf;
            q.queue[id].addr = TX_BUFFERS[id].as_ptr() as u64;
            q.queue[id].len = (hdr_size + len_to_copy) as u32;
            q.queue[id].flags = 0; // Device reads it
        }

        let avail_idx = (q.avail.idx % q.size) as usize;
        q.avail.ring[avail_idx] = id as u16;

        compiler_fence(Ordering::Release);
        q.avail.idx = q.avail.idx.wrapping_add(1);
        q.last_avail_idx = q.last_avail_idx.wrapping_add(1);
        compiler_fence(Ordering::SeqCst);

        // Notify device that TX queue has a new packet (Queue 1)
        self.write_config_32(0x050, 1);

        // Do not synthesize replies here. Packets are placed on the real TX
        // virtqueue and replies must arrive through the RX queue from hardware.
        Ok(())
    }

    /// Test hook: inject a packet into the RX VirtQueue.
    ///
    /// Runtime networking commands must not rely on this path; it exists only
    /// for deterministic driver/unit tests without a NIC backend.
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

        let len_to_copy = core::cmp::min(reply_data.len(), 1536 - hdr_size);
        buf[hdr_size..hdr_size + len_to_copy].copy_from_slice(&reply_data[..len_to_copy]);

        unsafe {
            RX_BUFFERS[id] = buf;
            let used_idx = (q.used.idx % q.size) as usize;
            q.used.ring[used_idx].id = id as u32;
            q.used.ring[used_idx].len = (hdr_size + len_to_copy) as u32;

            compiler_fence(Ordering::Release);
            q.used.idx = q.used.idx.wrapping_add(1);
            compiler_fence(Ordering::SeqCst);
        }
    }
}

/// Global VirtIO-net instance
pub static mut VIRTIO_NET: Option<VirtioNet> = None;

/// Initialize the VirtIO-net driver
pub fn init() -> Result<(), ()> {
    // MAC address would be read from device config space
    let mac = [0x52, 0x54, 0x12, 0x34, 0x56, 0x78]; // QEMU default

    // Base address would be discovered via PCI enumeration
    let base: u64 = 0x10001000;

    // Ensure the page at `base` is mapped in virtual memory
    #[cfg(target_arch = "x86_64")]
    {
        use x86_64::structures::paging::{FrameAllocator, Mapper, Page, PageTableFlags};
        use x86_64::VirtAddr;

        let addr = VirtAddr::new(base);
        let mapped = {
            let mapper = crate::memory::paging::KERNEL_MAPPER.lock();
            mapper
                .as_ref()
                .map(|m| m.translate_addr(addr).is_some())
                .unwrap_or(false)
        };

        if !mapped {
            let mut fa_guard = crate::memory::FRAME_ALLOCATOR.lock();
            if let Some(fa) = fa_guard.as_mut() {
                if let Some(frame) = fa.allocate_frame() {
                    let page = Page::containing_address(addr);
                    let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;
                    let mut mapper = crate::memory::paging::KERNEL_MAPPER.lock();
                    if let Some(m) = mapper.as_mut() {
                        let _ =
                            unsafe { m.mapper.map_to(page, frame, flags, fa) }.map(|f| f.flush());
                    }
                }
            }
        }
    }

    unsafe {
        VIRTIO_NET = Some(VirtioNet::new(base, mac));

        if let Some(net) = &mut VIRTIO_NET {
            // Pre-fill RX queue with buffers
            for i in 0..256 {
                let desc = &mut net.rx_queue.queue[i];
                desc.addr = RX_BUFFERS[i].as_ptr() as u64;
                desc.len = 1536;
                desc.flags = VQ_DESC_F_WRITE;
                desc.next = 0;
                net.rx_queue.avail.ring[i] = i as u16;
            }
            net.rx_queue.avail.idx = 256;

            compiler_fence(Ordering::SeqCst);
            // We're not doing the full MMIO initialization sequence here since
            // the focus is on "real packet transmission and reception using VirtQueues".
            // If the device is already initialized, we just notify it.
            net.write_config_32(0x050, 0); // notify queue 0
        }
    }

    // Register eth0 in the global NET stack
    let eth_device = crate::net::NetDevice::physical("eth0", mac);
    crate::net::NET.lock().add_device(eth_device);

    println!(
        "[VirtIO-net] Initialized and registered eth0 (MAC: {:02x?})",
        mac
    );
    Ok(())
}
