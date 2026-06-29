#![allow(dead_code)]
use crate::abi::AbiError;
use crate::drivers::block::BlockDevice;
use crate::drivers::pci::{pci_config_read, pci_config_write, PciDevice};
use crate::drivers::device_manager::Driver;
use alloc::boxed::Box;
use alloc::sync::Arc;
use spin::Mutex;
use x86_64::VirtAddr;
use x86_64::instructions::port::Port;
use core::sync::atomic::{AtomicU64, Ordering};

// ── MMIO register offsets ─────────────────────────────────────────────────────
const VIRTIO_MMIO_MAGIC: usize = 0x000;
const VIRTIO_MMIO_DEVICE_ID: usize = 0x008;
const VIRTIO_MMIO_HOST_FEATURES: usize = 0x010;
const VIRTIO_MMIO_GUEST_FEATURES: usize = 0x020;
const VIRTIO_MMIO_GUEST_PAGE_SIZE: usize = 0x028;
const VIRTIO_MMIO_QUEUE_SEL: usize = 0x030;
const VIRTIO_MMIO_QUEUE_NUM_MAX: usize = 0x034;
const VIRTIO_MMIO_QUEUE_NUM: usize = 0x038;
const VIRTIO_MMIO_QUEUE_ALIGN: usize = 0x03C;
const VIRTIO_MMIO_QUEUE_PFN: usize = 0x040;
const VIRTIO_MMIO_QUEUE_NOTIFY: usize = 0x050;
const VIRTIO_MMIO_INTERRUPT_STATUS: usize = 0x060;
const VIRTIO_MMIO_INTERRUPT_ACK: usize = 0x064;
const VIRTIO_MMIO_STATUS: usize = 0x070;
const VIRTIO_MMIO_CONFIG: usize = 0x100;

// ── PCI legacy offsets ────────────────────────────────────────────────────────
const PCI_HOST_FEATURES:  u16 = 0x00;
const PCI_GUEST_FEATURES: u16 = 0x04;
const PCI_QUEUE_ADDRESS:  u16 = 0x08;
const PCI_QUEUE_SIZE:     u16 = 0x0C;
const PCI_QUEUE_SEL:      u16 = 0x0E;
const PCI_QUEUE_NOTIFY:   u16 = 0x10;
const PCI_DEVICE_STATUS:  u16 = 0x12;
const PCI_ISR:            u16 = 0x13;

// ── Device status bits ────────────────────────────────────────────────────────
const STATUS_RESET: u32 = 0;
const STATUS_ACKNOWLEDGE: u32 = 1;
const STATUS_DRIVER: u32 = 2;
const STATUS_DRIVER_OK: u32 = 4;
const STATUS_FEATURES_OK: u32 = 8;
const STATUS_FAILED: u32 = 128;

// ── VirtIO block request types ────────────────────────────────────────────────
const VIRTIO_BLK_T_IN: u32 = 0; // read
const VIRTIO_BLK_T_OUT: u32 = 1; // write

// ── Virtqueue constants ───────────────────────────────────────────────────────
const QUEUE_SIZE: usize = 8; // must be power of 2
const PAGE_SIZE: usize = 4096;

// ── Virtqueue descriptor flags ────────────────────────────────────────────────
const VRING_DESC_F_NEXT: u16 = 1;
const VRING_DESC_F_WRITE: u16 = 2; // device writes to this descriptor

// ── On-device structures ──────────────────────────────────────────────────────
#[repr(C)]
struct VirtqDesc {
    addr: u64,
    len: u32,
    flags: u16,
    next: u16,
}

#[repr(C)]
struct VirtqAvail {
    flags: u16,
    idx: u16,
    ring: [u16; 512],
}

#[repr(C)]
struct VirtqUsedElem {
    id: u32,
    len: u32,
}

#[repr(C)]
struct VirtqUsed {
    flags: u16,
    idx: u16,
    ring: [VirtqUsedElem; 512],
}

#[repr(C)]
struct BlkReqHeader {
    req_type: u32,
    reserved: u32,
    sector: u64,
}

const DESC_TABLE_SIZE: usize = core::mem::size_of::<VirtqDesc>() * QUEUE_SIZE;
const USED_RING_OFFSET: usize = PAGE_SIZE; // second page
const QUEUE_PAGES: usize = 2;
const QUEUE_BYTES: usize = QUEUE_PAGES * PAGE_SIZE;

struct VirtqueueState {
    ptr: *mut u8,
    layout: core::alloc::Layout,
    qsize: usize,
    used_off: usize,
    next_desc: usize,
    last_used: u16,
}

impl Drop for VirtqueueState {
    fn drop(&mut self) {
        unsafe {
            alloc::alloc::dealloc(self.ptr, self.layout);
        }
    }
}

impl VirtqueueState {
    fn desc_ptr(&mut self) -> *mut VirtqDesc {
        self.ptr as *mut VirtqDesc
    }
    fn avail_ptr(&mut self) -> *mut VirtqAvail {
        unsafe { self.ptr.add(16 * self.qsize) as *mut VirtqAvail }
    }
    fn used_ptr(&mut self) -> *const VirtqUsed {
        unsafe { self.ptr.add(self.used_off) as *const VirtqUsed }
    }
    fn req_header_ptr(&self) -> *mut BlkReqHeader {
        unsafe { self.ptr.add(2368) as *mut BlkReqHeader }
    }
    fn status_ptr(&self) -> *mut u8 {
        unsafe { self.ptr.add(2496) as *mut u8 }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum VirtioTransport {
    Mmio { base_addr: u64 },
    Pci { io_base: u16, config_off: u16 },
}

pub struct VirtioBlock {
    pub transport: VirtioTransport,
    pub total_sectors: AtomicU64,
    queue: Mutex<Option<VirtqueueState>>,
}

// SAFETY: VirtioBlock is Send/Sync due to Mutex and Atomic usage.
unsafe impl Send for VirtioBlock {}
unsafe impl Sync for VirtioBlock {}

fn virt_to_phys(virt: u64) -> u64 {
    let mapper = crate::memory::paging::KERNEL_MAPPER.lock();
    let res = match mapper.as_ref() {
        Some(m) => m.translate_addr(VirtAddr::new(virt))
                    .map(|p| p.as_u64())
                    .unwrap_or(0),
        None => 0,
    };
    if res == 0 {
        crate::println!("[VirtIO-blk] WARNING: virt_to_phys(0x{:X}) returned 0!", virt);
    }
    res
}

impl VirtioBlock {
    pub fn new(transport: VirtioTransport, sectors: u64) -> Self {
        Self {
            transport,
            total_sectors: AtomicU64::new(sectors),
            queue: Mutex::new(None),
        }
    }

    pub fn init(&self) -> Result<(), AbiError> {
        // For MMIO, verify magic and device type
        if let VirtioTransport::Mmio { .. } = self.transport {
            let magic = self.mmio_read(VIRTIO_MMIO_MAGIC);
            if magic != 0x74726976 {
                return Err(AbiError::Other("VirtIO magic mismatch"));
            }
            let dev_id = self.mmio_read(VIRTIO_MMIO_DEVICE_ID);
            if dev_id != 2 {
                return Err(AbiError::Other("Not a VirtIO block device"));
            }
        }

        // Reset device
        self.mmio_write(VIRTIO_MMIO_STATUS, STATUS_RESET);
        // Acknowledge + Driver
        self.mmio_write(VIRTIO_MMIO_STATUS, STATUS_ACKNOWLEDGE | STATUS_DRIVER);
        
        // Feature negotiation
        let features = self.mmio_read(VIRTIO_MMIO_HOST_FEATURES);
        let supported = 0u32;
        let guest_features = features & supported;
        self.mmio_write(VIRTIO_MMIO_GUEST_FEATURES, guest_features);
        
        // Legacy MMIO: set guest page size
        if let VirtioTransport::Mmio { .. } = self.transport {
            self.mmio_write(VIRTIO_MMIO_GUEST_PAGE_SIZE, PAGE_SIZE as u32);
        }

        // Set up virtqueue 0
        self.mmio_write(VIRTIO_MMIO_QUEUE_SEL, 0);
        let qmax = self.mmio_read(VIRTIO_MMIO_QUEUE_NUM_MAX) as usize;
        if qmax == 0 {
            return Err(AbiError::Other("VirtIO queue not available"));
        }
        let qsize = qmax;

        // Legacy MMIO: queue align
        if let VirtioTransport::Mmio { .. } = self.transport {
            self.mmio_write(VIRTIO_MMIO_QUEUE_ALIGN, PAGE_SIZE as u32);
        }

        let avail_end = 16 * qsize + 2 + 2 * qsize + 2;
        let used_off = (avail_end + 4095) & !4095;
        let used_end = used_off + 6 + 8 * qsize + 8;
        let queue_bytes = (used_end + 4095) & !4095;

        // Allocate virtqueue memory (zero-initialised and page-aligned)
        let layout = core::alloc::Layout::from_size_align(queue_bytes, PAGE_SIZE).unwrap();
        let ptr = unsafe { alloc::alloc::alloc_zeroed(layout) };
        if ptr.is_null() {
            return Err(AbiError::Other("Failed to allocate virtqueue memory"));
        }
        let pfn = virt_to_phys(ptr as u64) / PAGE_SIZE as u64;
        crate::println!("[VirtIO-blk] Virtqueue qsize={} allocated at virt=0x{:X}, phys=0x{:X}, PFN=0x{:X}, size={} bytes",
            qsize, ptr as u64, pfn * PAGE_SIZE as u64, pfn, queue_bytes);
        self.mmio_write(VIRTIO_MMIO_QUEUE_PFN, pfn as u32);

        // Driver OK
        self.mmio_write(
            VIRTIO_MMIO_STATUS,
            STATUS_ACKNOWLEDGE | STATUS_DRIVER | STATUS_DRIVER_OK,
        );

        // Read capacity from config space (offset 0x100 for MMIO, dynamic for PCI)
        if let VirtioTransport::Pci { io_base, config_off } = self.transport {
            unsafe {
                crate::println!("[VirtIO-blk] PCI Config space dump: config_off=0x{:X}, 0x14={:08X}, 0x18={:08X}, 0x1C={:08X}",
                    config_off,
                    Port::<u32>::new(io_base + 0x14).read(),
                    Port::<u32>::new(io_base + 0x18).read(),
                    Port::<u32>::new(io_base + 0x1C).read(),
                );
            }
        }
        let cap_lo = self.mmio_read(VIRTIO_MMIO_CONFIG) as u64;
        let cap_hi = self.mmio_read(VIRTIO_MMIO_CONFIG + 4) as u64;
        let device_capacity = cap_lo | (cap_hi << 32);
        if device_capacity > 0 {
            self.total_sectors.store(device_capacity, Ordering::SeqCst);
            crate::println!("[VirtIO-blk] Device reports {} sectors ({} MB)",
                device_capacity, device_capacity * 512 / 1024 / 1024);
        }

        *self.queue.lock() = Some(VirtqueueState {
            ptr,
            layout,
            qsize,
            used_off,
            next_desc: 0,
            last_used: 0,
        });
        Ok(())
    }

    fn mmio_read(&self, offset: usize) -> u32 {
        match self.transport {
            VirtioTransport::Mmio { base_addr } => unsafe {
                core::ptr::read_volatile((base_addr + offset as u64) as *const u32)
            },
            VirtioTransport::Pci { io_base, config_off } => {
                match offset {
                    VIRTIO_MMIO_HOST_FEATURES => unsafe { Port::<u32>::new(io_base + PCI_HOST_FEATURES).read() },
                    VIRTIO_MMIO_QUEUE_NUM_MAX => unsafe { Port::<u16>::new(io_base + PCI_QUEUE_SIZE).read() as u32 },
                    VIRTIO_MMIO_STATUS => unsafe { Port::<u8>::new(io_base + PCI_DEVICE_STATUS).read() as u32 },
                    VIRTIO_MMIO_INTERRUPT_STATUS => unsafe { Port::<u8>::new(io_base + PCI_ISR).read() as u32 },
                    VIRTIO_MMIO_CONFIG => unsafe { Port::<u32>::new(io_base + config_off).read() },
                    off if off == VIRTIO_MMIO_CONFIG + 4 => unsafe { Port::<u32>::new(io_base + config_off + 4).read() },
                    _ => 0,
                }
            }
        }
    }

    fn mmio_write(&self, offset: usize, val: u32) {
        match self.transport {
            VirtioTransport::Mmio { base_addr } => unsafe {
                core::ptr::write_volatile((base_addr + offset as u64) as *mut u32, val)
            },
            VirtioTransport::Pci { io_base, .. } => {
                match offset {
                    VIRTIO_MMIO_GUEST_FEATURES => unsafe { Port::<u32>::new(io_base + PCI_GUEST_FEATURES).write(val) },
                    VIRTIO_MMIO_QUEUE_SEL => unsafe { Port::<u16>::new(io_base + PCI_QUEUE_SEL).write(val as u16) },
                    VIRTIO_MMIO_QUEUE_NUM => unsafe { Port::<u16>::new(io_base + PCI_QUEUE_SIZE).write(val as u16) },
                    VIRTIO_MMIO_QUEUE_PFN => unsafe { Port::<u32>::new(io_base + PCI_QUEUE_ADDRESS).write(val) },
                    VIRTIO_MMIO_QUEUE_NOTIFY => unsafe { Port::<u16>::new(io_base + PCI_QUEUE_NOTIFY).write(val as u16) },
                    VIRTIO_MMIO_STATUS => unsafe { Port::<u8>::new(io_base + PCI_DEVICE_STATUS).write(val as u8) },
                    VIRTIO_MMIO_INTERRUPT_ACK => {
                        unsafe { Port::<u8>::new(io_base + PCI_ISR).read(); }
                    }
                    _ => {}
                }
            }
        }
    }

    fn do_request(
        &self,
        req_type: u32,
        sector: u64,
        buf: &mut [u8],
        write: bool,
    ) -> Result<(), AbiError> {
        let mut guard = self.queue.lock();
        let q = guard
            .as_mut()
            .ok_or(AbiError::Other("VirtIO not initialised"))?;

        let header = BlkReqHeader {
            req_type,
            reserved: 0,
            sector,
        };

        let d0 = q.next_desc % q.qsize;
        let d1 = (q.next_desc + 1) % q.qsize;
        let d2 = (q.next_desc + 2) % q.qsize;
        q.next_desc = (q.next_desc + 3) % q.qsize;

        unsafe {
            core::ptr::write(q.req_header_ptr(), header);
            core::ptr::write(q.status_ptr(), 0xFF);

            let descs = q.desc_ptr();

            // desc[0]: header
            (*descs.add(d0)) = VirtqDesc {
                addr: virt_to_phys(q.req_header_ptr() as u64),
                len: core::mem::size_of::<BlkReqHeader>() as u32,
                flags: VRING_DESC_F_NEXT,
                next: d1 as u16,
            };

            // desc[1]: data buffer
            (*descs.add(d1)) = VirtqDesc {
                addr: virt_to_phys(buf.as_mut_ptr() as u64),
                len: buf.len() as u32,
                flags: VRING_DESC_F_NEXT | if write { 0 } else { VRING_DESC_F_WRITE },
                next: d2 as u16,
            };

            // desc[2]: status byte
            (*descs.add(d2)) = VirtqDesc {
                addr: virt_to_phys(q.status_ptr() as u64),
                len: 1,
                flags: VRING_DESC_F_WRITE,
                next: 0,
            };

            let avail = q.avail_ptr();
            let avail_idx = ((*avail).idx as usize) % q.qsize;
            (*avail).ring[avail_idx] = d0 as u16;
            core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
            (*avail).idx = (*avail).idx.wrapping_add(1);
            core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
        }

        self.mmio_write(VIRTIO_MMIO_QUEUE_NOTIFY, 0);

        let timeout = 50_000_000u32;
        let mut i = 0u32;
        loop {
            core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
            let used_idx = unsafe { (*q.used_ptr()).idx };
            if used_idx != q.last_used {
                q.last_used = used_idx;
                break;
            }
            i += 1;
            if i >= timeout {
                return Err(AbiError::Other("VirtIO timeout"));
            }
            core::hint::spin_loop();
        }

        let isr = self.mmio_read(VIRTIO_MMIO_INTERRUPT_STATUS);
        self.mmio_write(VIRTIO_MMIO_INTERRUPT_ACK, isr);

        let status = unsafe { core::ptr::read(q.status_ptr()) };
        if status != 0 {
            return Err(AbiError::Other("VirtIO request failed"));
        }
        Ok(())
    }

    fn do_request_phys(
        &self,
        req_type: u32,
        sector: u64,
        phys_addr: u64,
        len: u32,
        write: bool,
    ) -> Result<(), AbiError> {
        let mut guard = self.queue.lock();
        let q = guard
            .as_mut()
            .ok_or(AbiError::Other("VirtIO not initialised"))?;

        let header = BlkReqHeader {
            req_type,
            reserved: 0,
            sector,
        };

        let d0 = q.next_desc % q.qsize;
        let d1 = (q.next_desc + 1) % q.qsize;
        let d2 = (q.next_desc + 2) % q.qsize;
        q.next_desc = (q.next_desc + 3) % q.qsize;

        unsafe {
            core::ptr::write(q.req_header_ptr(), header);
            core::ptr::write(q.status_ptr(), 0xFF);

            let descs = q.desc_ptr();

            // desc[0]: header
            (*descs.add(d0)) = VirtqDesc {
                addr: virt_to_phys(q.req_header_ptr() as u64),
                len: core::mem::size_of::<BlkReqHeader>() as u32,
                flags: VRING_DESC_F_NEXT,
                next: d1 as u16,
            };

            // desc[1]: data buffer
            (*descs.add(d1)) = VirtqDesc {
                addr: phys_addr,
                len,
                flags: VRING_DESC_F_NEXT | if write { 0 } else { VRING_DESC_F_WRITE },
                next: d2 as u16,
            };

            // desc[2]: status byte
            (*descs.add(d2)) = VirtqDesc {
                addr: virt_to_phys(q.status_ptr() as u64),
                len: 1,
                flags: VRING_DESC_F_WRITE,
                next: 0,
            };

            let avail = q.avail_ptr();
            let avail_idx = ((*avail).idx as usize) % q.qsize;
            (*avail).ring[avail_idx] = d0 as u16;
            core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
            (*avail).idx = (*avail).idx.wrapping_add(1);
        }

        self.mmio_write(VIRTIO_MMIO_QUEUE_NOTIFY, 0);

        let timeout = 50_000_000u32;
        let mut i = 0u32;
        loop {
            core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
            let used_idx = unsafe { (*q.used_ptr()).idx };
            if used_idx != q.last_used {
                q.last_used = used_idx;
                break;
            }
            i += 1;
            if i >= timeout {
                return Err(AbiError::Other("VirtIO timeout"));
            }
            core::hint::spin_loop();
        }

        let isr = self.mmio_read(VIRTIO_MMIO_INTERRUPT_STATUS);
        self.mmio_write(VIRTIO_MMIO_INTERRUPT_ACK, isr);

        let status = unsafe { core::ptr::read(q.status_ptr()) };
        if status != 0 {
            return Err(AbiError::Other("VirtIO request failed"));
        }
        Ok(())
    }

    pub fn device_status(&self) -> u32 {
        self.mmio_read(VIRTIO_MMIO_STATUS)
    }
}

impl BlockDevice for VirtioBlock {
    fn read_sectors(&self, sector: u64, count: u32, buf: &mut [u8]) -> Result<(), AbiError> {
        if sector + count as u64 > self.total_sectors.load(Ordering::SeqCst) {
            return Err(AbiError::OutOfBounds);
        }
        let phys = virt_to_phys(buf.as_mut_ptr() as u64);
        self.do_request_phys(
            VIRTIO_BLK_T_IN,
            sector,
            phys,
            (count as u32) * 512,
            false,
        )?;
        Ok(())
    }

    fn write_sectors(&self, sector: u64, count: u32, buf: &[u8]) -> Result<(), AbiError> {
        if sector + count as u64 > self.total_sectors.load(Ordering::SeqCst) {
            return Err(AbiError::OutOfBounds);
        }
        let phys = virt_to_phys(buf.as_ptr() as u64);
        self.do_request_phys(
            VIRTIO_BLK_T_OUT,
            sector,
            phys,
            (count as u32) * 512,
            true,
        )?;
        Ok(())
    }

    fn total_sectors(&self) -> u64 {
        self.total_sectors.load(Ordering::SeqCst)
    }
}

pub struct VirtioBlockDriver;

impl Driver for VirtioBlockDriver {
    fn name(&self) -> &str { "VirtIO Block (PCI Legacy)" }
    fn pci_match(&self, device: &PciDevice) -> bool {
        device.vendor_id == 0x1AF4 && (device.device_id == 0x1001 || device.device_id == 0x1042)
    }
    fn init(&self, device: &PciDevice) -> Result<(), ()> {
        crate::println!("[VirtIO-blk] Probing PCI device at {:02X}:{:02X}.{}", device.bus, device.dev, device.func);
        let bar0 = pci_config_read(device.bus, device.dev, device.func, 0x10);
        if bar0 & 0x1 == 0 {
            crate::println!("[VirtIO-blk] BAR0 is not an I/O BAR — skipping");
            return Err(());
        }
        let io_base = (bar0 & 0xFFFC) as u16;
        if io_base == 0 {
            return Err(());
        }

        // Enable I/O space in PCI command register
        let cmd = pci_config_read(device.bus, device.dev, device.func, 0x04);
        pci_config_write(device.bus, device.dev, device.func, 0x04, cmd | 0x0001);

        // Detect if MSI-X is enabled to determine configuration space offset
        let msix_detect_1 = unsafe { Port::<u16>::new(io_base + 0x14).read() };
        let msix_detect_2 = unsafe { Port::<u16>::new(io_base + 0x16).read() };
        let config_off = if msix_detect_1 == 0xFFFF && msix_detect_2 == 0xFFFF {
            0x18
        } else {
            0x14
        };
        crate::println!("[VirtIO-blk] MSI-X detect: 0x14={:04X}, 0x16={:04X} -> config_off=0x{:X}",
            msix_detect_1, msix_detect_2, config_off);

        let transport = VirtioTransport::Pci { io_base, config_off };
        let blk = VirtioBlock::new(transport, 0);
        if let Err(e) = blk.init() {
            crate::println!("[VirtIO-blk] Init failed: {:?}", e);
            return Err(());
        }

        let device: Arc<dyn BlockDevice> = Arc::new(blk);
        crate::drivers::block_registry::register("vda", "virtio-blk", device);
        Ok(())
    }
}

pub fn register() {
    crate::drivers::device_manager::DEVICE_MANAGER.lock().register_driver(Box::new(VirtioBlockDriver));
}
