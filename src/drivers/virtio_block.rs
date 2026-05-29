use crate::abi::AbiError;
/// VirtIO Block Driver for ZiqaKernel
///
/// Implements the VirtIO 1.0 legacy MMIO transport for block devices.
/// Compatible with QEMU's `-device virtio-blk-device` (MMIO variant).
///
/// Memory layout at `base_addr`:
///   +0x000  MagicValue        (R)  0x74726976
///   +0x004  Version           (R)  1 (legacy) or 2
///   +0x008  DeviceID          (R)  2 = block device
///   +0x00C  VendorID          (R)
///   +0x010  HostFeatures      (R)
///   +0x014  HostFeaturesSel   (W)
///   +0x020  GuestFeatures     (W)
///   +0x024  GuestFeaturesSel  (W)
///   +0x028  GuestPageSize     (W)  legacy only
///   +0x030  QueueSel          (W)
///   +0x034  QueueNumMax       (R)
///   +0x038  QueueNum          (W)
///   +0x03C  QueueAlign        (W)  legacy only
///   +0x040  QueuePFN          (W)  legacy: page frame number
///   +0x050  QueueNotify       (W)
///   +0x060  InterruptStatus   (R)
///   +0x064  InterruptACK      (W)
///   +0x070  Status            (RW)
///   +0x100  Config            (R)  device-specific config
use crate::drivers::block::BlockDevice;
use alloc::boxed::Box;
use spin::Mutex;

// ── MMIO register offsets ─────────────────────────────────────────────────────
const VIRTIO_MMIO_MAGIC: usize = 0x000;
#[allow(dead_code)]
const VIRTIO_MMIO_VERSION: usize = 0x004;
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
const VIRTIO_MMIO_INTERRUPT_ACK: usize = 0x064;
const VIRTIO_MMIO_STATUS: usize = 0x070;
#[allow(dead_code)]
const VIRTIO_MMIO_CONFIG: usize = 0x100;

// ── Device status bits ────────────────────────────────────────────────────────
const STATUS_ACKNOWLEDGE: u32 = 1;
const STATUS_DRIVER: u32 = 2;
const STATUS_DRIVER_OK: u32 = 4;
const STATUS_FEATURES_OK: u32 = 8;
#[allow(dead_code)]
const STATUS_FAILED: u32 = 128;

// ── VirtIO block request types ────────────────────────────────────────────────
const VIRTIO_BLK_T_IN: u32 = 0; // read
const VIRTIO_BLK_T_OUT: u32 = 1; // write

// ── Virtqueue constants ───────────────────────────────────────────────────────
const QUEUE_SIZE: usize = 8; // must be power of 2, ≤ QueueNumMax
const PAGE_SIZE: usize = 4096;

// ── Virtqueue descriptor flags ────────────────────────────────────────────────
const VRING_DESC_F_NEXT: u16 = 1;
const VRING_DESC_F_WRITE: u16 = 2; // device writes to this descriptor

// ── On-device structures (must be repr(C) and correctly aligned) ──────────────

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
    ring: [u16; QUEUE_SIZE],
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
    ring: [VirtqUsedElem; QUEUE_SIZE],
}

// ── VirtIO block request header ───────────────────────────────────────────────
#[repr(C)]
struct BlkReqHeader {
    req_type: u32,
    reserved: u32,
    sector: u64,
}

// ── Virtqueue memory layout ───────────────────────────────────────────────────
// We allocate one page-aligned buffer that holds:
//   [VirtqDesc × QUEUE_SIZE] [VirtqAvail] [padding to PAGE_SIZE] [VirtqUsed]
//
// Legacy VirtIO requires the used ring to start at the next page boundary.

const DESC_TABLE_SIZE: usize = core::mem::size_of::<VirtqDesc>() * QUEUE_SIZE;
#[allow(dead_code)]
const AVAIL_RING_SIZE: usize = core::mem::size_of::<VirtqAvail>();
const USED_RING_OFFSET: usize = PAGE_SIZE; // second page
const QUEUE_PAGES: usize = 2;
const QUEUE_BYTES: usize = QUEUE_PAGES * PAGE_SIZE;

// ── Driver state ──────────────────────────────────────────────────────────────

struct VirtqueueState {
    /// Raw page-aligned memory for the virtqueue
    mem: Box<[u8; QUEUE_BYTES]>,
    /// Next descriptor index to use (cycles 0..QUEUE_SIZE)
    next_desc: usize,
    /// Last used index we processed
    last_used: u16,
}

impl VirtqueueState {
    fn desc_ptr(&mut self) -> *mut VirtqDesc {
        self.mem.as_mut_ptr() as *mut VirtqDesc
    }
    fn avail_ptr(&mut self) -> *mut VirtqAvail {
        unsafe { (self.mem.as_mut_ptr().add(DESC_TABLE_SIZE)) as *mut VirtqAvail }
    }
    fn used_ptr(&mut self) -> *const VirtqUsed {
        unsafe { (self.mem.as_ptr().add(USED_RING_OFFSET)) as *const VirtqUsed }
    }
    #[allow(dead_code)]
    fn phys_base(&self) -> u64 {
        // In a bare-metal kernel with identity mapping, virt == phys
        self.mem.as_ptr() as u64
    }
}

pub struct VirtioBlock {
    pub base_addr: u64,
    pub total_sectors: u64,
    queue: Mutex<Option<VirtqueueState>>,
}

impl VirtioBlock {
    pub fn new(base: u64, sectors: u64) -> Self {
        Self {
            base_addr: base,
            total_sectors: sectors,
            queue: Mutex::new(None),
        }
    }

    /// Initialise the VirtIO MMIO device and set up virtqueue 0.
    /// Call once after constructing the driver.
    pub fn init(&self) -> Result<(), AbiError> {
        // Verify magic and device type
        let magic = self.mmio_read(VIRTIO_MMIO_MAGIC);
        if magic != 0x74726976 {
            return Err(AbiError::Other("VirtIO magic mismatch"));
        }
        let dev_id = self.mmio_read(VIRTIO_MMIO_DEVICE_ID);
        if dev_id != 2 {
            return Err(AbiError::Other("Not a VirtIO block device"));
        }

        // Reset device
        self.mmio_write(VIRTIO_MMIO_STATUS, 0);
        // Acknowledge + Driver
        self.mmio_write(VIRTIO_MMIO_STATUS, STATUS_ACKNOWLEDGE | STATUS_DRIVER);
        // Accept all offered features (we don't negotiate anything special)
        let features = self.mmio_read(VIRTIO_MMIO_HOST_FEATURES);
        self.mmio_write(VIRTIO_MMIO_GUEST_FEATURES, features);
        // Legacy: set guest page size
        self.mmio_write(VIRTIO_MMIO_GUEST_PAGE_SIZE, PAGE_SIZE as u32);

        // Set up virtqueue 0
        self.mmio_write(VIRTIO_MMIO_QUEUE_SEL, 0);
        let qmax = self.mmio_read(VIRTIO_MMIO_QUEUE_NUM_MAX) as usize;
        if qmax == 0 {
            return Err(AbiError::Other("VirtIO queue not available"));
        }
        let qsize = QUEUE_SIZE.min(qmax);
        self.mmio_write(VIRTIO_MMIO_QUEUE_NUM, qsize as u32);
        self.mmio_write(VIRTIO_MMIO_QUEUE_ALIGN, PAGE_SIZE as u32);

        // Allocate virtqueue memory (zero-initialised)
        let mem = Box::new([0u8; QUEUE_BYTES]);
        let pfn = (mem.as_ptr() as u64) / PAGE_SIZE as u64;
        self.mmio_write(VIRTIO_MMIO_QUEUE_PFN, pfn as u32);

        // Driver OK
        self.mmio_write(
            VIRTIO_MMIO_STATUS,
            STATUS_ACKNOWLEDGE | STATUS_DRIVER | STATUS_FEATURES_OK | STATUS_DRIVER_OK,
        );

        // Read capacity from config space (2 × u32 = u64 sectors)
        // (we trust the caller-supplied total_sectors for now)

        *self.queue.lock() = Some(VirtqueueState {
            mem,
            next_desc: 0,
            last_used: 0,
        });
        Ok(())
    }

    // ── MMIO helpers ──────────────────────────────────────────────────────────

    fn mmio_read(&self, offset: usize) -> u32 {
        unsafe {
            let ptr = (self.base_addr as usize + offset) as *const u32;
            core::ptr::read_volatile(ptr)
        }
    }

    fn mmio_write(&self, offset: usize, val: u32) {
        unsafe {
            let ptr = (self.base_addr as usize + offset) as *mut u32;
            core::ptr::write_volatile(ptr, val);
        }
    }

    // ── Submit a 3-descriptor chain and poll for completion ───────────────────
    //
    // Chain layout:
    //   desc[0]  BlkReqHeader  (device reads)
    //   desc[1]  data buffer   (device reads for OUT, writes for IN)
    //   desc[2]  status byte   (device writes)

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
        let mut status: u8 = 0xFF;

        let d0 = q.next_desc % QUEUE_SIZE;
        let d1 = (q.next_desc + 1) % QUEUE_SIZE;
        let d2 = (q.next_desc + 2) % QUEUE_SIZE;
        q.next_desc = (q.next_desc + 3) % QUEUE_SIZE;

        unsafe {
            let descs = q.desc_ptr();

            // desc[0]: header (read-only for device)
            (*descs.add(d0)) = VirtqDesc {
                addr: &header as *const BlkReqHeader as u64,
                len: core::mem::size_of::<BlkReqHeader>() as u32,
                flags: VRING_DESC_F_NEXT,
                next: d1 as u16,
            };

            // desc[1]: data buffer
            (*descs.add(d1)) = VirtqDesc {
                addr: buf.as_mut_ptr() as u64,
                len: buf.len() as u32,
                flags: VRING_DESC_F_NEXT | if write { 0 } else { VRING_DESC_F_WRITE },
                next: d2 as u16,
            };

            // desc[2]: status byte (device writes)
            (*descs.add(d2)) = VirtqDesc {
                addr: &mut status as *mut u8 as u64,
                len: 1,
                flags: VRING_DESC_F_WRITE,
                next: 0,
            };

            // Put head descriptor into available ring
            let avail = q.avail_ptr();
            let avail_idx = ((*avail).idx as usize) % QUEUE_SIZE;
            (*avail).ring[avail_idx] = d0 as u16;
            // Memory barrier before updating idx
            core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
            (*avail).idx = (*avail).idx.wrapping_add(1);
            core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
        }

        // Notify device (queue 0)
        self.mmio_write(VIRTIO_MMIO_QUEUE_NOTIFY, 0);

        // Poll used ring until device completes (busy-wait; no IRQ needed)
        let timeout = 1_000_000u32;
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

        // ACK interrupt
        self.mmio_write(
            VIRTIO_MMIO_INTERRUPT_ACK,
            self.mmio_read(VIRTIO_MMIO_INTERRUPT_ACK),
        );

        if status != 0 {
            return Err(AbiError::Other("VirtIO request failed"));
        }
        Ok(())
    }
}

impl BlockDevice for VirtioBlock {
    fn read_sectors(&self, sector: u64, count: u32, buf: &mut [u8]) -> Result<(), AbiError> {
        if sector + count as u64 > self.total_sectors {
            return Err(AbiError::OutOfBounds);
        }
        let sector_size = 512usize;
        for i in 0..count as usize {
            let slice = &mut buf[i * sector_size..(i + 1) * sector_size];
            // do_request needs a &mut [u8]; we have a sub-slice
            let mut tmp = [0u8; 512];
            self.do_request(VIRTIO_BLK_T_IN, sector + i as u64, &mut tmp, false)?;
            slice.copy_from_slice(&tmp);
        }
        Ok(())
    }

    fn write_sectors(&self, sector: u64, count: u32, buf: &[u8]) -> Result<(), AbiError> {
        if sector + count as u64 > self.total_sectors {
            return Err(AbiError::OutOfBounds);
        }
        let sector_size = 512usize;
        for i in 0..count as usize {
            let mut tmp = [0u8; 512];
            tmp.copy_from_slice(&buf[i * sector_size..(i + 1) * sector_size]);
            self.do_request(VIRTIO_BLK_T_OUT, sector + i as u64, &mut tmp, true)?;
        }
        Ok(())
    }

    fn total_sectors(&self) -> u64 {
        self.total_sectors
    }
}
