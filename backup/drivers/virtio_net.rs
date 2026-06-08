#![allow(static_mut_refs)]
#![allow(dead_code)]

/// VirtIO Network Driver for ZiqaKernel
///
/// Supports VirtIO PCI legacy I/O port transport (transitional `virtio-net-pci`).
/// Uses VirtQueue rings for packet TX/RX.
///
/// ── Fixes applied vs. the original ──────────────────────────────────────
///
///  [1]  rx_mem/tx_mem use-after-move (compile error):
///       Queues are constructed exactly once via `new_rx` / `new_tx`; `mem`
///       is moved into the queue struct on construction and is never touched
///       again from the outside. No second owner, no aliasing.
///
///  [2]  Used-ring offset was 4-byte aligned — must be 4096-byte aligned:
///       The VirtIO legacy spec (and Linux's `vring_size()`) require the used
///       ring to start on a page boundary. For 256 descriptors the correct
///       offset is 8192, not 4616. Every `rx_available` / `receive` call was
///       reading garbage from the wrong memory location.
///
///  [3]  QUEUE_MEM_SIZE was 8192 B — too small for 256-descriptor queues:
///       With the page-aligned used ring, 256 descriptors need:
///         desc(4096) + avail(518) → page-align → 8192 + used(2054) = 10 246 B
///       Increased to 16 384 B (4 pages). A compile-time assertion enforces
///       this for any future change to NUM_DESC.
///
///  [4]  TX queue could silently overwrite in-flight descriptors:
///       `is_full()` guard added in `transmit()` — returns `Err(())` if all
///       descriptor slots are still owned by the device.
///
///  [5]  PCI BAR0 not verified as an I/O BAR:
///       Bit 0 of BAR0 distinguishes I/O space (1) from memory space (0).
///       The original code stripped the low bits unconditionally; if BAR0
///       were a memory BAR `io_base` would be wrong/zero.
///
///  [6]  PCI I/O space never enabled:
///       `pci_config_write` added; command register bit 0 is set before
///       accessing any port.
///
///  [7]  PCI vendor/device-ID register read twice:
///       Now cached in a single `id_reg` dword.
///
///  [8]  `tx_reclaim` used on the RX queue — wrong name, confusing semantics:
///       Renamed to `reclaim_completed()`, used uniformly for both directions.
///
///  [9]  `VIRTIO_NET` was a raw `static mut` with no synchronisation:
///       Wrapped in `spin::Mutex<Option<VirtioNet>>` — no unsafe access
///       required at call sites.
///
///  [10] `receive()` returned a 1536-byte array on the stack:
///       Signature changed to `receive(&mut self, out: &mut [u8]) -> Option<usize>`;
///       the caller provides the buffer, nothing large hits the stack.
///
///  [11] No `FAILED` status written when init fails mid-way:
///       The `fail!` macro writes STATUS_FAILED so the device resets cleanly.
///
///  [12] ISR register never cleared:
///       `PCI_ISR` is read (read-to-clear) at the start of every `receive`
///       call so the device can fire future interrupts.
///
///  [13] Link status never checked despite negotiating VIRTIO_NET_F_STATUS:
///       The link-status word at device-config offset +6 is read after
///       DRIVER_OK and logged.

use core::sync::atomic::{compiler_fence, Ordering};
use spin::Mutex;
use crate::drivers::pci::{pci_config_read, pci_config_write, PciDevice};
use crate::drivers::virtio_net_proto::*;
use crate::println;
use x86_64::instructions::port::Port;
use x86_64::VirtAddr;
use alloc::boxed::Box;
use crate::drivers::device_manager::Driver;

pub struct VirtioNetDriver;

impl Driver for VirtioNetDriver {
    fn name(&self) -> &str { "VirtIO Network" }
    fn pci_match(&self, device: &PciDevice) -> bool {
        device.vendor_id == 0x1AF4 && (device.device_id == 0x1000 || device.device_id == 0x1041)
    }
    fn init(&self, device: &PciDevice) -> Result<(), ()> {
        crate::drivers::virtio_net::init_with_device(device)
    }
}

pub fn register() {
    crate::drivers::device_manager::DEVICE_MANAGER.lock().register_driver(Box::new(VirtioNetDriver));
}

// ... rest of the constants ...
const PCI_HOST_FEATURES:  u16 = 0x00;
const PCI_GUEST_FEATURES: u16 = 0x04;
const PCI_QUEUE_ADDRESS:  u16 = 0x08;
const PCI_QUEUE_SIZE:     u16 = 0x0C;
const PCI_QUEUE_SEL:      u16 = 0x0E;
const PCI_QUEUE_NOTIFY:   u16 = 0x10;
const PCI_DEVICE_STATUS:  u16 = 0x12;
const PCI_ISR:            u16 = 0x13;
const PCI_DEVICE_CFG:     u16 = 0x14;

// ── VirtIO device status bits ─────────────────────────────────────────────
const STATUS_RESET:       u8 = 0x00;
const STATUS_ACKNOWLEDGE: u8 = 0x01;
const STATUS_DRIVER:      u8 = 0x02;
const STATUS_DRIVER_OK:   u8 = 0x04;
const STATUS_FAILED:      u8 = 0x80;

// ── VirtIO network feature bits ───────────────────────────────────────────
const VIRTIO_NET_F_CSUM:   u32 = 1 << 0;
const VIRTIO_NET_F_MAC:    u32 = 1 << 5;
const VIRTIO_NET_F_STATUS: u32 = 1 << 16;

// ── DMA layout constants ──────────────────────────────────────────────────
const NUM_DESC:       usize = 256;
const BUF_SIZE:       usize = 1544; // 1536 data + 12-byte VirtioNetHdr

/// 4 pages per queue direction.
///
/// Memory map for NUM_DESC = 256 (verified by the compile-time assert below):
///   offset     0 .. 4096   — Descriptor Table  (256 × 16 B)
///   offset  4096 .. 4614   — Available Ring     (6 + 256×2 B)
///   offset  8192 .. 10246  — Used Ring          (6 + 256×8 B)  ← page-aligned
///   Total needed: 10 246 B  →  16 384 B (4 pages) is safe.
const QUEUE_MEM_SIZE: usize = 4 * 4096; // 16 384 B

// ── Compile-time layout sanity check ─────────────────────────────────────
const _LAYOUT_CHECK: () = {
    let used_start = used_ring_offset_ct(NUM_DESC as u16);
    let used_end   = used_start + 6 + NUM_DESC * 8; // flags+idx + ring[] + avail_event
    assert!(
        QUEUE_MEM_SIZE >= used_end,
        "QUEUE_MEM_SIZE is too small — increase it or reduce NUM_DESC"
    );
};

// ── Static DMA buffers ────────────────────────────────────────────────────
// SAFETY: each DMA region is given exclusively to one VirtQueueLegacy on init
//         and is never aliased.  All subsequent access is serialised through
//         VIRTIO_NET.lock().

#[repr(C, align(4096))]
struct DmaBlock([u8; QUEUE_MEM_SIZE]);

static mut RX_QUEUE_DMA: DmaBlock = DmaBlock([0u8; QUEUE_MEM_SIZE]);
static mut TX_QUEUE_DMA: DmaBlock = DmaBlock([0u8; QUEUE_MEM_SIZE]);

static mut RX_BUFS: [[u8; BUF_SIZE]; NUM_DESC] = [[0u8; BUF_SIZE]; NUM_DESC];
static mut TX_BUFS: [[u8; BUF_SIZE]; NUM_DESC] = [[0u8; BUF_SIZE]; NUM_DESC];

// ── VirtQueue ring layout helpers ─────────────────────────────────────────
//
// Legacy VirtIO queue layout (single physically-contiguous DMA block):
//
//   ┌ offset 0 ──────────────── size × 16 B ─────────────────────────────┐
//   │ Descriptor Table                                                     │
//   ├ offset size×16 ──────────── 6 + size×2 B ──────────────────────────┤
//   │ Available Ring  (flags u16 | idx u16 | ring[size] u16 | evt u16)    │
//   ├ offset page_align(desc_end + avail_end) ── 4096-byte boundary ──────┤
//   │ Used Ring       (flags u16 | idx u16 | ring[size] VirtUsedElem |    │
//   │                  avail_event u16)                                    │
//   └──────────────────────────────────────────────────────────────────────┘
//
// Arithmetic for NUM_DESC = 256:
//   avail_ring_offset = 256 × 16          =  4 096
//   avail ring size   = 6 + 256×2         =    518
//   used_ring_offset  = page_align(4 614) =  8 192
//   used ring size    = 6 + 256×8         =  2 054
//   total             =                     10 246  →  fits in 16 384 B  ✓

#[inline]
const fn avail_ring_offset_ct(size: u16) -> usize {
    // Descriptor table is already 2-byte aligned (16 is a multiple of 2).
    size as usize * 16
}

#[inline]
const fn used_ring_offset_ct(size: u16) -> usize {
    let desc_bytes  = size as usize * 16;
    // flags(2) + idx(2) + ring[size](size×2) + used_event(2)
    let avail_bytes = 6 + size as usize * 2;
    // MUST be page-aligned per the VirtIO legacy spec.
    (desc_bytes + avail_bytes + 4095) & !4095
}

// Runtime wrappers (same logic, avoids requiring const context at call sites)
#[inline]
fn avail_ring_offset(size: u16) -> usize { avail_ring_offset_ct(size) }
#[inline]
fn used_ring_offset(size: u16)  -> usize { used_ring_offset_ct(size)  }

// ── VirtQueue wrapper ─────────────────────────────────────────────────────
struct VirtQueueLegacy {
    mem:            &'static mut [u8],
    size:           u16,
    last_used_idx:  u16,
    last_avail_idx: u16,
}

impl VirtQueueLegacy {
    // ── Constructors ──────────────────────────────────────────────────────

    /// Build and initialise an RX queue from a freshly-allocated DMA region.
    ///
    /// `mem` is consumed exactly once here.  The returned `VirtQueueLegacy`
    /// owns it for the rest of the driver's lifetime — no double-move.
    fn new_rx(mem: &'static mut [u8], size: u16) -> (Self, u32) {
        let mut q = Self {
            mem,
            size,
            last_used_idx:  0,
            last_avail_idx: size, // all buffers pre-posted to the avail ring
        };
        // Zero the entire DMA block.
        q.mem.iter_mut().for_each(|b| *b = 0);

        // Descriptors: device-writable slots pointing at RX_BUFS.
        for i in 0..size as usize {
            let phys = virt_to_phys(unsafe { RX_BUFS[i].as_ptr() as u64 });
            let d = q.desc(i);
            d.addr  = phys;
            d.len   = BUF_SIZE as u32;
            d.flags = VQ_DESC_F_WRITE;
            d.next  = 0;
        }

        // Post every descriptor to the available ring immediately.
        {
            let a = q.avail_mut();
            for i in 0..size as usize { a.ring[i] = i as u16; }
            a.idx = size;
        }

        let pfn = q.pfn();
        (q, pfn)
    }

    /// Build and initialise a TX queue.
    fn new_tx(mem: &'static mut [u8], size: u16) -> (Self, u32) {
        let mut q = Self {
            mem,
            size,
            last_used_idx:  0,
            last_avail_idx: 0,
        };
        q.mem.iter_mut().for_each(|b| *b = 0);

        // Descriptors: host-readable slots pointing at TX_BUFS.
        for i in 0..size as usize {
            let phys = virt_to_phys(unsafe { TX_BUFS[i].as_ptr() as u64 });
            let d = q.desc(i);
            d.addr  = phys;
            d.len   = BUF_SIZE as u32;
            d.flags = 0;
            d.next  = 0;
        }

        let pfn = q.pfn();
        (q, pfn)
    }

    // ── Ring accessors ────────────────────────────────────────────────────

    fn desc(&mut self, i: usize) -> &mut VirtQueueDesc {
        unsafe { &mut *(self.mem.as_mut_ptr() as *mut VirtQueueDesc).add(i) }
    }

    fn avail_mut(&mut self) -> &mut VirtQueueAvail {
        let off = avail_ring_offset(self.size);
        unsafe { &mut *(self.mem.as_mut_ptr().add(off) as *mut VirtQueueAvail) }
    }

    fn used(&self) -> &VirtQueueUsed {
        let off = used_ring_offset(self.size);
        unsafe { &*(self.mem.as_ptr().add(off) as *const VirtQueueUsed) }
    }

    // ── Queue state helpers ───────────────────────────────────────────────

    fn pfn(&self) -> u32 {
        (virt_to_phys(self.mem.as_ptr() as u64) / 4096) as u32
    }

    /// True when the device has placed at least one entry in the used ring.
    fn rx_available(&self) -> bool {
        compiler_fence(Ordering::Acquire);
        self.last_used_idx != self.used().idx
    }

    /// True when every descriptor slot is still in-flight (TX queue full).
    fn is_full(&self) -> bool {
        self.last_avail_idx.wrapping_sub(self.last_used_idx) >= self.size
    }

    /// Advance `last_used_idx` past all entries the device has finished with.
    /// Used for both TX (reclaiming sent buffers) and RX (bookkeeping).
    fn reclaim_completed(&mut self) {
        compiler_fence(Ordering::Acquire);
        while self.last_used_idx != self.used().idx {
            self.last_used_idx = self.last_used_idx.wrapping_add(1);
        }
    }
}

// ── VirtIO-net device ─────────────────────────────────────────────────────
pub struct VirtioNet {
    pub io_base: u16,
    pub mac:     [u8; 6],
    rx_queue:    VirtQueueLegacy,
    tx_queue:    VirtQueueLegacy,
}

// SAFETY: VirtioNet is exclusively accessed via VIRTIO_NET.lock().
//         &'static mut [u8] is Send (ownership transfer is safe); it is never
//         shared across concurrent lock guards.
unsafe impl Send for VirtioNet {}

// ── Global device handle (no raw `static mut`) ────────────────────────────
pub static VIRTIO_NET: Mutex<Option<VirtioNet>> = Mutex::new(None);

// ── Initialisation ────────────────────────────────────────────────────────

// ── Physical-address translation ──────────────────────────────────────────
fn virt_to_phys(virt: u64) -> u64 {
    let mapper = crate::memory::paging::KERNEL_MAPPER.lock();
    match mapper.as_ref() {
        Some(m) => m.translate_addr(VirtAddr::new(virt))
                    .map(|p| p.as_u64())
                    .unwrap_or(0),
        None => 0,
    }
}

// ── Port I/O helpers ──────────────────────────────────────────────────────
fn port_write32(b: u16, o: u16, v: u32) { unsafe { Port::<u32>::new(b+o).write(v); } }
fn port_read32 (b: u16, o: u16) -> u32  { unsafe { Port::<u32>::new(b+o).read()   } }
fn port_write16(b: u16, o: u16, v: u16) { unsafe { Port::<u16>::new(b+o).write(v); } }
fn port_read16 (b: u16, o: u16) -> u16  { unsafe { Port::<u16>::new(b+o).read()   } }
fn port_write8 (b: u16, o: u16, v: u8 ) { unsafe { Port::<u8 >::new(b+o).write(v); } }
fn port_read8  (b: u16, o: u16) -> u8   { unsafe { Port::<u8 >::new(b+o).read()   } }

pub fn init_with_device(device: &PciDevice) -> Result<(), ()> {
    let bus = device.address.bus();
    let dev = device.address.device();
    let func = device.address.function();

    // Enable I/O space in PCI command register (bit 0).
    let cmd = pci_config_read(bus, dev, func, 0x04);
    pci_config_write(bus, dev, func, 0x04, cmd | 0x0001);

    let bar0 = pci_config_read(bus, dev, func, 0x10);
    // Bit 0 of a BAR: 1 -> I/O space, 0 -> memory space.
    if bar0 & 0x1 == 0 {
        println!("[VirtIO-net] BAR0 is not an I/O BAR — skipping");
        return Err(());
    }


    let io_base = (bar0 & 0xFFFC) as u16;
    if io_base == 0 {
        return Err(());
    }

    // Writes STATUS_FAILED so the device can reset cleanly, then returns Err.
    macro_rules! fail {
        ($msg:literal) => {{
            println!("[VirtIO-net] Init failed: {}", $msg);
            port_write8(io_base, PCI_DEVICE_STATUS, STATUS_FAILED);
            return Err(());
        }};
    }

    // ── 1. Device reset ───────────────────────────────────────────────────
    port_write8(io_base, PCI_DEVICE_STATUS, STATUS_RESET);
    compiler_fence(Ordering::SeqCst);

    // ── 2. ACKNOWLEDGE ────────────────────────────────────────────────────
    port_write8(io_base, PCI_DEVICE_STATUS, STATUS_ACKNOWLEDGE);
    compiler_fence(Ordering::SeqCst);

    // ── 3. DRIVER ─────────────────────────────────────────────────────────
    port_write8(io_base, PCI_DEVICE_STATUS, STATUS_ACKNOWLEDGE | STATUS_DRIVER);
    compiler_fence(Ordering::SeqCst);

    // ── 4. Feature negotiation ────────────────────────────────────────────
    let host_features  = port_read32(io_base, PCI_HOST_FEATURES);
    let guest_features = host_features
        & (VIRTIO_NET_F_MAC | VIRTIO_NET_F_STATUS | VIRTIO_NET_F_CSUM);
    port_write32(io_base, PCI_GUEST_FEATURES, guest_features);
    compiler_fence(Ordering::SeqCst);

    // ── 5. Read MAC address ───────────────────────────────────────────────
    let mac = [
        port_read8(io_base, PCI_DEVICE_CFG + 0),
        port_read8(io_base, PCI_DEVICE_CFG + 1),
        port_read8(io_base, PCI_DEVICE_CFG + 2),
        port_read8(io_base, PCI_DEVICE_CFG + 3),
        port_read8(io_base, PCI_DEVICE_CFG + 4),
        port_read8(io_base, PCI_DEVICE_CFG + 5),
    ];
    println!("[VirtIO-net] MAC {:02x?}", mac);

    // ── 6. RX queue (index 0) ─────────────────────────────────────────────
    // `rx_mem` is moved into `new_rx` and lives inside the returned queue —
    // there is no second use, so the original use-after-move is gone.
    port_write16(io_base, PCI_QUEUE_SEL, 0);
    let rx_qsize = port_read16(io_base, PCI_QUEUE_SIZE);
    if rx_qsize == 0 { fail!("RX queue size reported as 0"); }
    let rx_size = rx_qsize.min(NUM_DESC as u16);
    println!("[VirtIO-net] RX queue size: {}", rx_size);

    let rx_mem: &'static mut [u8] = unsafe { &mut RX_QUEUE_DMA.0[..] };
    let (rx_queue, rx_pfn) = VirtQueueLegacy::new_rx(rx_mem, rx_size);
    port_write32(io_base, PCI_QUEUE_ADDRESS, rx_pfn);
    compiler_fence(Ordering::SeqCst);

    // ── 7. TX queue (index 1) ─────────────────────────────────────────────
    port_write16(io_base, PCI_QUEUE_SEL, 1);
    let tx_qsize = port_read16(io_base, PCI_QUEUE_SIZE);
    if tx_qsize == 0 { fail!("TX queue size reported as 0"); }
    let tx_size = tx_qsize.min(NUM_DESC as u16);
    println!("[VirtIO-net] TX queue size: {}", tx_size);

    let tx_mem: &'static mut [u8] = unsafe { &mut TX_QUEUE_DMA.0[..] };
    let (tx_queue, tx_pfn) = VirtQueueLegacy::new_tx(tx_mem, tx_size);
    port_write32(io_base, PCI_QUEUE_ADDRESS, tx_pfn);
    compiler_fence(Ordering::SeqCst);

    // ── 8. DRIVER_OK ──────────────────────────────────────────────────────
    port_write8(
        io_base,
        PCI_DEVICE_STATUS,
        STATUS_ACKNOWLEDGE | STATUS_DRIVER | STATUS_DRIVER_OK,
    );
    compiler_fence(Ordering::SeqCst);

    // ── 9. Link status (only if VIRTIO_NET_F_STATUS was accepted) ─────────
    if guest_features & VIRTIO_NET_F_STATUS != 0 {
        // Bit 0 of the link-status word at device-config offset +6.
        let link = port_read16(io_base, PCI_DEVICE_CFG + 6);
        if link & 1 == 0 {
            println!("[VirtIO-net] Warning: link is currently down");
        } else {
            println!("[VirtIO-net] Link is up");
        }
    }

    // ── 10. Kick RX queue so the device starts filling buffers ────────────
    port_write16(io_base, PCI_QUEUE_NOTIFY, 0);

    // ── 11. Commit ────────────────────────────────────────────────────────
    *VIRTIO_NET.lock() = Some(VirtioNet { io_base, mac, rx_queue, tx_queue });

    #[cfg(feature = "net")]
    {
        let eth = crate::net::NetDevice::physical("eth0", mac);
        crate::net::NET.lock().add_device(eth);
    }

    println!(
        "[VirtIO-net] Ready — eth0, {} RX + {} TX descriptors",
        rx_size, tx_size
    );
    Ok(())
}

// ── TX / RX ───────────────────────────────────────────────────────────────
impl VirtioNet {
    /// Receive one packet into the caller-supplied buffer `out`.
    ///
    /// Returns `Some(n)` where `n` is the number of bytes written, or `None`
    /// if no packet is available.  The caller controls buffer size — nothing
    /// large is placed on the kernel stack.
    pub fn receive(&mut self, out: &mut [u8]) -> Option<usize> {
        // Read PCI_ISR to acknowledge the interrupt (read-to-clear).
        // This must happen even in polling mode so future IRQs can fire.
        let _isr = port_read8(self.io_base, PCI_ISR);

        self.rx_queue.reclaim_completed();

        if !self.rx_queue.rx_available() {
            return None;
        }

        let q        = &mut self.rx_queue;
        let used_idx = q.last_used_idx % q.size;
        let elem     = q.used().ring[used_idx as usize];
        let id       = elem.id  as usize;
        let total    = elem.len as usize;

        q.last_used_idx = q.last_used_idx.wrapping_add(1);

        let hdr = core::mem::size_of::<VirtioNetHdr>();
        if total <= hdr {
            // Header-only frame — re-post the buffer without returning data.
            Self::rx_repost(q, id);
            port_write16(self.io_base, PCI_QUEUE_NOTIFY, 0);
            return None;
        }

        let pkt_len = (total - hdr).min(out.len());
        unsafe {
            out[..pkt_len]
                .copy_from_slice(&RX_BUFS[id][hdr..hdr + pkt_len]);
        }

        // Return descriptor to the available ring for the device to refill.
        Self::rx_repost(q, id);
        compiler_fence(Ordering::SeqCst);
        port_write16(self.io_base, PCI_QUEUE_NOTIFY, 0);

        Some(pkt_len)
    }

    /// Transmit `data` on the TX virtqueue.
    ///
    /// Returns `Err(())` if the queue is full — caller should retry or drop.
    pub fn transmit(&mut self, data: &[u8]) -> Result<(), ()> {
        self.tx_queue.reclaim_completed();

        // Full-queue guard: prevent overwriting an in-flight descriptor.
        if self.tx_queue.is_full() {
            return Err(());
        }

        let q        = &mut self.tx_queue;
        let id       = (q.last_avail_idx % q.size) as usize;
        let hdr      = core::mem::size_of::<VirtioNetHdr>();
        let to_send  = data.len().min(BUF_SIZE - hdr);

        unsafe {
            let buf     = &mut TX_BUFS[id];
            let net_hdr = VirtioNetHdr::default();
            core::ptr::copy_nonoverlapping(
                &net_hdr as *const _ as *const u8,
                buf.as_mut_ptr(),
                hdr,
            );
            buf[hdr..hdr + to_send].copy_from_slice(&data[..to_send]);
            q.desc(id).len = (hdr + to_send) as u32;
        }

        let qsize = q.size;
        let avail  = q.avail_mut();
        let slot   = (avail.idx % qsize) as usize;
        avail.ring[slot] = id as u16;
        compiler_fence(Ordering::Release);
        avail.idx        = avail.idx.wrapping_add(1);
        q.last_avail_idx = q.last_avail_idx.wrapping_add(1);
        compiler_fence(Ordering::SeqCst);

        // Kick TX queue (index 1).
        port_write16(self.io_base, PCI_QUEUE_NOTIFY, 1);
        Ok(())
    }

    /// Placeholder — zero-copy TX from a physical address (not yet implemented).
    pub fn transmit_from_phys(&mut self, _src_phys: u64, _len: usize) -> Result<(), ()> {
        Err(())
    }

    /// Placeholder — zero-copy RX into a physical address (not yet implemented).
    pub fn receive_to_phys(&mut self, _target_phys: u64, _max_len: usize) -> Option<usize> {
        None
    }

    // ── Private helpers ───────────────────────────────────────────────────

    /// Put descriptor `id` back into the RX available ring so the device
    /// can write the next incoming packet into it.
    #[inline]
    fn rx_repost(q: &mut VirtQueueLegacy, id: usize) {
        let qsize = q.size;
        let a     = q.avail_mut();
        let slot  = (a.idx % qsize) as usize;
        a.ring[slot] = id as u16;
        compiler_fence(Ordering::Release);
        a.idx = a.idx.wrapping_add(1);
    }
}
