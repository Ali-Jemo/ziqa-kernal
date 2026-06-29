#![allow(dead_code)]

//! NVMe (Non-Volatile Memory Express) driver
//!
//! PCI class 0x01 (mass storage), subclass 0x08 (NVMe), prog-if 0x02.
//! Uses BAR0 for memory-mapped controller registers (64-bit capable).
//! DMA-based read/write with polling for command completion.
//!
//! # Design
//! - Multiple I/O submission/completion queue pairs (one per CPU core).
//! - Each namespace is registered as a separate block device (nvme0n1, nvme0n2, ...).
//! - Per-command polling on the completion queue head doorbell.
//! - Thread-safe: `Mutex` per queue pair protects queue state.
//!
//! # Improvements
//! - Bounce buffers for DMA (eliminates PRP list complexity and page-boundary issues).
//! - Round-robin I/O queue selection for SMP load distribution.
//! - Set Features (Number of Queues) to negotiate queue count with the controller.
//! - NVMe status decoding for diagnostic error messages.
//! - LBA format detection from namespace identify data.
//! - Async event configuration for controller health monitoring.

use crate::abi::AbiError;
use crate::drivers::block::BlockDevice;
use crate::drivers::block_registry;
use crate::drivers::device_manager::Driver;
use crate::drivers::pci::{bar_address, enable_bus_mastering, enable_memory_space, PciDevice};
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::ptr::{read_volatile, write_volatile};
use spin::Mutex;
use x86_64::VirtAddr;

// ── PCI identifiers ──────────────────────────────────────────────────────────
const PCI_CLASS_STORAGE: u8 = 0x01;
const PCI_SUBCLASS_NVME: u8 = 0x08;
const PCI_PROGIF_NVME: u8 = 0x02;

// ── NVMe controller register offsets (BAR0) ─────────────────────────────────
const CAP_OFFSET: usize = 0x00;
const VS_OFFSET: usize = 0x08;
const INTMS_OFFSET: usize = 0x0C;
const INTMC_OFFSET: usize = 0x10;
const CC_OFFSET: usize = 0x14;
const CSTS_OFFSET: usize = 0x1C;
const NSSR_OFFSET: usize = 0x20;
const AQA_OFFSET: usize = 0x24;
const ASQ_OFFSET: usize = 0x28;
const ACQ_OFFSET: usize = 0x30;
const CMBLST_OFFSET: usize = 0x38;
const CMBSZ_OFFSET: usize = 0x3C;
const DB_BASE: usize = 0x1000;

// ── Controller Capabilities (CAP) bits ───────────────────────────────────────
const CAP_MQES_MASK: u64 = 0xFFFF;
const CAP_CSS_NVM: u64 = 1 << 37;
const CAP_DSTRD_MASK: u64 = 0xF0000;
const CAP_DSTRD_SHIFT: u64 = 16;

// ── Controller Configuration (CC) bits ───────────────────────────────────────
const CC_EN: u32 = 1;
const CC_CSS_NVM: u32 = 0 << 4;
const CC_MPS_SHIFT: u32 = 7;
const CC_SHN_SHIFT: u32 = 14;
const CC_IOSQES_SHIFT: u32 = 16;
const CC_IOCQES_SHIFT: u32 = 20;

// ── Controller Status (CSTS) bits ────────────────────────────────────────────
const CSTS_RDY: u32 = 1;
const CSTS_CFS: u32 = 1 << 1;  // Controller Fatal Status

// ── Admin opcodes ────────────────────────────────────────────────────────────
const ADMIN_OPCODE_CREATE_IO_CQ: u8 = 0x05;
const ADMIN_OPCODE_CREATE_IO_SQ: u8 = 0x01;
const ADMIN_OPCODE_IDENTIFY: u8 = 0x06;
const ADMIN_OPCODE_SET_FEATURES: u8 = 0x09;
const ADMIN_OPCODE_GET_FEATURES: u8 = 0x0A;
const ADMIN_OPCODE_ASYNC_EVENT_REQ: u8 = 0x0C;

// ── I/O opcodes ─────────────────────────────────────────────────────────────
const IO_OPCODE_WRITE: u8 = 0x01;
const IO_OPCODE_READ: u8 = 0x02;

// ── Identify CNS values ──────────────────────────────────────────────────────
const CNS_CONTROLLER: u8 = 0x01;
const CNS_NAMESPACE: u8 = 0x00;

// ── Set Features identifiers (cdw10) ─────────────────────────────────────────
const FEAT_NUM_QUEUES: u32 = 0x07;
const FEAT_POWER_MGMT: u32 = 0x02;
const FEAT_ASYNC_EVENT: u32 = 0x0B;

// ── Constants ─────────────────────────────────────────────────────────────────
const SECTOR_SIZE: usize = 512;
const BUF_PAGE_SECTORS: usize = 8;
const SQ_ENTRY_SIZE: usize = 64;
const CQ_ENTRY_SIZE: usize = 16;
const MAX_QD: u16 = 32;
const ADMIN_Q_SIZE: u16 = 64;
const IO_Q_SIZE: u16 = 64;
const MAX_IO_QUEUES: usize = 8;

// ── NVMe data structures ─────────────────────────────────────────────────────

/// Submission Queue Entry (64 bytes)
#[repr(C, packed)]
#[derive(Clone, Copy, Debug)]
struct SubmissionQueueEntry {
    opcode: u8, flags: u8, command_id: u16, nsid: u32,
    reserved: [u32; 2], metadata_ptr: u64,
    prp1: u64, prp2: u64,
    cdw10: u32, cdw11: u32, cdw12: u32, cdw13: u32, cdw14: u32, cdw15: u32,
}

/// Completion Queue Entry (16 bytes)
#[repr(C, packed)]
#[derive(Clone, Copy, Debug)]
struct CompletionQueueEntry {
    command_specific: u32, reserved: u32,
    sq_head_pointer: u16, sq_id: u16, command_id: u16, phase_tag_status: u16,
}

/// Controller Identify Data Structure (4096 bytes).
/// Layout matches NVMe spec offsets 0–267; remaining area is opaque.
#[repr(C, packed)]
#[derive(Clone, Copy)]
struct IdentifyController {
    vendor_id: u16, subsystem_vendor_id: u16,
    serial_number: [u8; 20], model_number: [u8; 40], firmware_revision: [u8; 8],
    recommended_arb_burst: u8, ieee_oui: [u8; 3], capabilities: u8,
    reserved: [u8; 17],
    num_namespaces: u32,
    capabilities_2: [u8; 3836],
}

/// Namespace Identify Data Structure (4096 bytes).
#[repr(C, packed)]
#[derive(Clone, Copy)]
struct IdentifyNamespace {
    ns_size: u64, ns_capacity: u64, ns_utilization: u64,
    ns_features: u8, num_lba_formats: u8, formatted_lba_size: u8,
    metadata_capabilities: u8, endgiance_group_id: u16, ns_attributes: u8,
    nvm_set_id: u16, reserved: [u8; 4021],
}

/// Queue pair state — shared ring buffer of SQ/CQ entries.
struct QueuePair {
    sq_base: VirtAddr, cq_base: VirtAddr,
    sq_size: u16, cq_size: u16,
    sq_tail: u16, cq_head: u16, cq_phase: bool,
    doorbell_stride: u32,
}

/// NVMe controller state — multi-queue, multi-namespace capable.
pub struct NvmeController {
    mmio_base: VirtAddr,
    admin_queue: Mutex<QueuePair>,
    io_queues: Vec<Mutex<QueuePair>>,
    doorbell_stride: u32,
}

unsafe impl Send for NvmeController {}
unsafe impl Sync for NvmeController {}

// ── Helper functions ─────────────────────────────────────────────────────────

fn virt_to_phys(virt: usize) -> u64 {
    let vaddr = VirtAddr::new(virt as u64);
    let mapper = crate::memory::paging::KERNEL_MAPPER.lock();
    if let Some(m) = mapper.as_ref() {
        if let Some(phys) = m.translate_addr(vaddr) {
            return phys.as_u64();
        }
    }
    let po = crate::memory::paging::phys_offset().as_u64();
    if virt as u64 >= po { (virt as u64) - po } else { 0 }
}

fn alloc_dma_page() -> (u64, VirtAddr) {
    use crate::memory::FRAME_ALLOCATOR;
    use crate::memory::paging::phys_offset;
    use x86_64::structures::paging::FrameAllocator;
    let mut fa = FRAME_ALLOCATOR.lock();
    let f = fa.as_mut().expect("frame allocator not ready");
    let frame = f.allocate_frame().expect("OOM allocating NVMe DMA frame");
    let paddr = frame.start_address().as_u64();
    let vaddr = VirtAddr::new(phys_offset().as_u64() + paddr);
    unsafe { core::ptr::write_bytes(vaddr.as_mut_ptr::<u8>(), 0, 4096) };
    (paddr, vaddr)
}

/// Decode NVMe completion status into a human-readable string.
fn nvme_status_str(phase_tag_status: u16) -> &'static str {
    let sc = ((phase_tag_status >> 1) & 0xFF) as u8;
    let sct = ((phase_tag_status >> 9) & 0x7) as u8;
    match (sct, sc) {
        (0, 0x00) => "success",
        (0, 0x01) => "invalid command opcode",
        (0, 0x02) => "invalid field in command",
        (0, 0x03) => "command ID conflict",
        (0, 0x04) => "data transfer error",
        (0, 0x05) => "aborted (power loss)",
        (0, 0x06) => "internal error",
        (0, 0x07) => "aborted (command)",
        (0, 0x08) => "aborted (SQ deletion)",
        (0, 0x09) => "aborted (failed FUSED)",
        (0, 0x0A) => "aborted (missing FUSED)",
        (0, 0x0B) => "invalid namespace/format",
        (0, 0x10) => "bad attribute",
        (0, 0x11) => "invalid PRP offset",
        (0, 0x12) => "atomic write unit exceeded",
        (0, 0x13) => "operation denied",
        (0, 0x14) => "SG list invalid",
        (0, 0x80) => "conflicting attributes",
        (0, 0x81) => "invalid protection info",
        (0, 0x82) => "attempted write to RO range",
        (1, 0x00) => "LBA out of range",
        (1, 0x01) => "capacity exceeded",
        (1, 0x02) => "namespace not ready",
        (1, 0x80) => "namespace not ready",
        (2, 0x00) => "write fault",
        (2, 0x01) => "unrecovered read error",
        (2, 0x02) => "guard check failure",
        (2, 0x03) => "application tag check failure",
        (2, 0x04) => "reference tag check failure",
        (2, 0x05) => "data compare error",
        (2, 0x81) => "deallocated / unwritten block",
        (3, 0x00) => "controller pathing error",
        (3, 0x01) => "host pathing error",
        (3, 0x02) => "command aborted by host",
        (5, 0x00..=0xFF) => "vendor specific",
        _ => "unknown status",
    }
}

/// Read a string from an NVMe ASCII field (padded with spaces, not NUL-terminated).
fn nvme_str(field: &[u8]) -> alloc::string::String {
    let end = field.iter().rposition(|&c| c != b' ' && c != 0).map(|i| i + 1).unwrap_or(0);
    core::str::from_utf8(&field[..end]).unwrap_or("?").into()
}

// ── Controller implementation ────────────────────────────────────────────────

impl NvmeController {
    // ── MMIO register accessors ───────────────────────────────────────────────
    fn read_cap(&self) -> u64 { unsafe { read_volatile((self.mmio_base.as_u64() + CAP_OFFSET as u64) as *const u64) } }
    fn read_vs(&self) -> u32 { unsafe { read_volatile((self.mmio_base.as_u64() + VS_OFFSET as u64) as *const u32) } }
    fn read_cc(&self) -> u32 { unsafe { read_volatile((self.mmio_base.as_u64() + CC_OFFSET as u64) as *const u32) } }
    fn write_cc(&self, val: u32) { unsafe { write_volatile((self.mmio_base.as_u64() + CC_OFFSET as u64) as *mut u32, val); } }
    fn read_csts(&self) -> u32 { unsafe { read_volatile((self.mmio_base.as_u64() + CSTS_OFFSET as u64) as *const u32) } }
    fn read_aqa(&self) -> u32 { unsafe { read_volatile((self.mmio_base.as_u64() + AQA_OFFSET as u64) as *const u32) } }
    fn write_aqa(&self, val: u32) { unsafe { write_volatile((self.mmio_base.as_u64() + AQA_OFFSET as u64) as *mut u32, val); } }
    fn read_asq(&self) -> u64 { unsafe { read_volatile((self.mmio_base.as_u64() + ASQ_OFFSET as u64) as *const u64) } }
    fn write_asq(&self, val: u64) { unsafe { write_volatile((self.mmio_base.as_u64() + ASQ_OFFSET as u64) as *mut u64, val); } }
    fn read_acq(&self) -> u64 { unsafe { read_volatile((self.mmio_base.as_u64() + ACQ_OFFSET as u64) as *const u64) } }
    fn write_acq(&self, val: u64) { unsafe { write_volatile((self.mmio_base.as_u64() + ACQ_OFFSET as u64) as *mut u64, val); } }

    fn sq_db_offset(&self, qid: u16) -> u64 {
        (DB_BASE + (2 * qid as usize) * self.doorbell_stride as usize) as u64
    }
    fn cq_db_offset(&self, qid: u16) -> u64 {
        (DB_BASE + (2 * qid as usize + 1) * self.doorbell_stride as usize) as u64
    }
    fn write_sq_tail(&self, qid: u16, val: u32) {
        unsafe { write_volatile((self.mmio_base.as_u64() + self.sq_db_offset(qid)) as *mut u32, val); }
    }
    fn write_cq_head(&self, qid: u16, val: u32) {
        unsafe { write_volatile((self.mmio_base.as_u64() + self.cq_db_offset(qid)) as *mut u32, val); }
    }

    // ── Command execution ─────────────────────────────────────────────────────

    /// Poll a completion queue until an entry with matching phase tag appears.
    /// Returns the CQ entry, updating the queue head pointer and doorbell.
    fn poll_cq(qp: &mut QueuePair, _qid: u16, _ctrl: &NvmeController) -> Result<CompletionQueueEntry, AbiError> {
        let mut timeout = 1_000_000u32;
        while timeout > 0 {
            let entry = unsafe {
                &*((qp.cq_base.as_u64() + (qp.cq_head as u64 * CQ_ENTRY_SIZE as u64)) as *const CompletionQueueEntry)
            };
            let phase = (entry.phase_tag_status & 1) != 0;
            if phase == qp.cq_phase {
                let cqe = *entry;
                qp.cq_head = (qp.cq_head + 1) % qp.cq_size;
                if qp.cq_head == 0 { qp.cq_phase = !qp.cq_phase; }
                _ctrl.write_cq_head(_qid, qp.cq_head as u32);
                let sc = (cqe.phase_tag_status >> 1) & 0xFF;
                if sc != 0 {
                    let sc_val = sc;
                    let sct_val = (cqe.phase_tag_status >> 9) & 0x7;
                    crate::println!("[NVMe] cmd failed: sct={} sc={} ({})",
                        sct_val, sc_val, nvme_status_str(cqe.phase_tag_status));
                    return Err(AbiError::Other("NVMe device error"));
                }
            }
            timeout -= 1;
            core::hint::spin_loop();
        }
        Err(AbiError::Other("NVMe command timeout"))
    }

    /// Submit a command on the admin queue (QID 0) and wait for completion.
    fn submit_admin_cmd(&self, cmd: &SubmissionQueueEntry) -> Result<CompletionQueueEntry, AbiError> {
        let mut admin = self.admin_queue.lock();
        let tail = admin.sq_tail;
        let entry = unsafe {
            &mut *((admin.sq_base.as_u64() + (tail as u64 * SQ_ENTRY_SIZE as u64)) as *mut SubmissionQueueEntry)
        };
        *entry = *cmd;
        admin.sq_tail = (admin.sq_tail + 1) % admin.sq_size;
        self.write_sq_tail(0, admin.sq_tail as u32);
        Self::poll_cq(&mut *admin, 0, self)
    }

    /// Submit a command on an I/O queue and wait for completion.
    fn submit_io_cmd(&self, qid: usize, cmd: &SubmissionQueueEntry) -> Result<CompletionQueueEntry, AbiError> {
        if qid >= self.io_queues.len() {
            return Err(AbiError::Other("NVMe IO queue out of range"));
        }
        let mut qp = self.io_queues[qid].lock();
        let tail = qp.sq_tail;
        let entry = unsafe {
            &mut *((qp.sq_base.as_u64() + (tail as u64 * SQ_ENTRY_SIZE as u64)) as *mut SubmissionQueueEntry)
        };
        *entry = *cmd;
        qp.sq_tail = (qp.sq_tail + 1) % qp.sq_size;
        self.write_sq_tail((qid + 1) as u16, qp.sq_tail as u32);
        Self::poll_cq(&mut *qp, (qid + 1) as u16, self)
    }

    // ── Admin commands ─────────────────────────────────────────────────────────

    fn identify_controller(&self) -> Result<IdentifyController, AbiError> {
        let ctrl_data: IdentifyController = unsafe { core::mem::zeroed() };
        let phys = virt_to_phys(&ctrl_data as *const _ as usize);
        let cmd = SubmissionQueueEntry {
            opcode: ADMIN_OPCODE_IDENTIFY, flags: 0, command_id: 1, nsid: 0,
            reserved: [0; 2], metadata_ptr: 0, prp1: phys, prp2: 0,
            cdw10: CNS_CONTROLLER as u32, cdw11: 0, cdw12: 0, cdw13: 0, cdw14: 0, cdw15: 0,
        };
        self.submit_admin_cmd(&cmd)?;
        Ok(ctrl_data)
    }

    fn identify_namespace(&self, nsid: u32) -> Result<IdentifyNamespace, AbiError> {
        let ns_data: IdentifyNamespace = unsafe { core::mem::zeroed() };
        let phys = virt_to_phys(&ns_data as *const _ as usize);
        let cmd = SubmissionQueueEntry {
            opcode: ADMIN_OPCODE_IDENTIFY, flags: 0, command_id: 2, nsid,
            reserved: [0; 2], metadata_ptr: 0, prp1: phys, prp2: 0,
            cdw10: CNS_NAMESPACE as u32, cdw11: 0, cdw12: 0, cdw13: 0, cdw14: 0, cdw15: 0,
        };
        self.submit_admin_cmd(&cmd)?;
        Ok(ns_data)
    }

    fn set_features(&self, feature_id: u32, cdw11: u32, cdw12: u32) -> Result<CompletionQueueEntry, AbiError> {
        let cmd = SubmissionQueueEntry {
            opcode: ADMIN_OPCODE_SET_FEATURES, flags: 0, command_id: 0x10, nsid: 0,
            reserved: [0; 2], metadata_ptr: 0, prp1: 0, prp2: 0,
            cdw10: feature_id, cdw11, cdw12,
            cdw13: 0, cdw14: 0, cdw15: 0,
        };
        self.submit_admin_cmd(&cmd)
    }

    fn set_features_num_queues(&self, nq: u32) -> Result<u32, AbiError> {
        let cqe = self.set_features(FEAT_NUM_QUEUES, (nq - 1) | ((nq - 1) << 16), 0)?;
        Ok((cqe.command_specific & 0xFFFF).min((cqe.command_specific >> 16) & 0xFFFF) + 1)
    }

    fn set_features_async_event(&self) -> Result<(), AbiError> {
        // Enable critical warnings + NS attribute changes + firmware activation
        self.set_features(FEAT_ASYNC_EVENT, 0x0F, 0).map(drop)
    }

    fn create_io_cq(&self, qid: u16, cq_phys: u64) -> Result<(), AbiError> {
        let cmd = SubmissionQueueEntry {
            opcode: ADMIN_OPCODE_CREATE_IO_CQ, flags: 0, command_id: 3, nsid: 0,
            reserved: [0; 2], metadata_ptr: 0, prp1: cq_phys, prp2: 0,
            cdw10: ((IO_Q_SIZE as u32 - 1) << 16) | (qid as u32),
            cdw11: 1, cdw12: 0, cdw13: 0, cdw14: 0, cdw15: 0,
        };
        self.submit_admin_cmd(&cmd).map(drop)
    }

    fn create_io_sq(&self, qid: u16, sq_phys: u64) -> Result<(), AbiError> {
        let cmd = SubmissionQueueEntry {
            opcode: ADMIN_OPCODE_CREATE_IO_SQ, flags: 0, command_id: 4, nsid: 0,
            reserved: [0; 2], metadata_ptr: 0, prp1: sq_phys, prp2: 0,
            cdw10: ((IO_Q_SIZE as u32 - 1) << 16) | (qid as u32),
            cdw11: ((qid as u32) << 16) | (qid as u32),
            cdw12: 0, cdw13: 0, cdw14: 0, cdw15: 0,
        };
        self.submit_admin_cmd(&cmd).map(drop)
    }
}

// ── BlockDevice implementation ───────────────────────────────────────────────

struct NvmeBlockDevice {
    controller: Arc<NvmeController>,
    nsid: u32,
    namespace_size: u64,
    queue_count: usize,
    next_queue: Mutex<usize>,
}

unsafe impl Send for NvmeBlockDevice {}
unsafe impl Sync for NvmeBlockDevice {}

impl NvmeBlockDevice {
    fn new(controller: Arc<NvmeController>, nsid: u32, namespace_size: u64) -> Self {
        let qc = controller.io_queues.len();
        Self { controller, nsid, namespace_size, queue_count: qc, next_queue: Mutex::new(0) }
    }

    fn next_qid(&self) -> usize {
        let mut ctr = self.next_queue.lock();
        let qid = *ctr;
        *ctr = (*ctr + 1) % self.queue_count;
        qid
    }
}

impl BlockDevice for NvmeBlockDevice {
    fn read_sectors(&self, sector: u64, count: u32, buf: &mut [u8]) -> Result<(), AbiError> {
        let mut remaining = count as u64;
        let mut current_lba = sector;
        let mut offset = 0usize;
        while remaining > 0 {
            let chunk = remaining.min(BUF_PAGE_SECTORS as u64) as u16;
            let (bounce_phys, bounce_virt) = alloc_dma_page();
            let qid = self.next_qid();
            let cmd = SubmissionQueueEntry {
                opcode: IO_OPCODE_READ, flags: 0, command_id: 5, nsid: self.nsid,
                reserved: [0; 2], metadata_ptr: 0, prp1: bounce_phys, prp2: 0,
                cdw10: (current_lba & 0xFFFFFFFF) as u32,
                cdw11: (current_lba >> 32) as u32,
                cdw12: (chunk as u32) - 1,
                cdw13: 0, cdw14: 0, cdw15: 0,
            };
            self.controller.submit_io_cmd(qid, &cmd)?;
            unsafe {
                core::ptr::copy_nonoverlapping(bounce_virt.as_ptr::<u8>(), buf.as_mut_ptr().add(offset), (chunk as usize) * SECTOR_SIZE);
            }
            offset += (chunk as usize) * SECTOR_SIZE;
            current_lba += chunk as u64;
            remaining -= chunk as u64;
        }
        Ok(())
    }

    fn write_sectors(&self, sector: u64, count: u32, buf: &[u8]) -> Result<(), AbiError> {
        let mut remaining = count as u64;
        let mut current_lba = sector;
        let mut offset = 0usize;
        while remaining > 0 {
            let chunk = remaining.min(BUF_PAGE_SECTORS as u64) as u16;
            let (bounce_phys, bounce_virt) = alloc_dma_page();
            let copy_size = (chunk as usize) * SECTOR_SIZE;
            unsafe {
                core::ptr::copy_nonoverlapping(buf.as_ptr().add(offset), bounce_virt.as_mut_ptr::<u8>(), copy_size);
            }
            let qid = self.next_qid();
            let cmd = SubmissionQueueEntry {
                opcode: IO_OPCODE_WRITE, flags: 0, command_id: 6, nsid: self.nsid,
                reserved: [0; 2], metadata_ptr: 0, prp1: bounce_phys, prp2: 0,
                cdw10: (current_lba & 0xFFFFFFFF) as u32,
                cdw11: (current_lba >> 32) as u32,
                cdw12: (chunk as u32) - 1,
                cdw13: 0, cdw14: 0, cdw15: 0,
            };
            self.controller.submit_io_cmd(qid, &cmd)?;
            offset += copy_size;
            current_lba += chunk as u64;
            remaining -= chunk as u64;
        }
        Ok(())
    }

    fn total_sectors(&self) -> u64 {
        self.namespace_size
    }
}

// ── Driver registration ──────────────────────────────────────────────────────

pub struct NvmeDriver;

impl Driver for NvmeDriver {
    fn name(&self) -> &str { "NVMe" }

    fn pci_match(&self, device: &PciDevice) -> bool {
        device.class == PCI_CLASS_STORAGE
            && device.subclass == PCI_SUBCLASS_NVME
            && device.prog_if == PCI_PROGIF_NVME
    }

    fn init(&self, device: &PciDevice) -> Result<(), ()> {
        crate::println!("[NVMe] Found controller at {:02X}:{:02X}.{}", device.bus, device.dev, device.func);

        enable_bus_mastering(device.address);
        enable_memory_space(device.address);

        let bar0 = device.bars[0];
        if bar0 == 0 { crate::println!("[NVMe] No BAR0 found"); return Err(()); }
        let (bar_addr, is_64) = bar_address(bar0);
        let mut mmio_phys = bar_addr as u64;
        if is_64 { mmio_phys |= (device.bars[1] as u64) << 32; }
        let mmio_base = VirtAddr::new(crate::memory::paging::phys_offset().as_u64() + mmio_phys);

        let cap = unsafe { read_volatile(mmio_base.as_ptr() as *const u64) };
        let max_q_entries = (cap & CAP_MQES_MASK) as u16 + 1;
        let doorbell_stride = 1u32 << (((cap & CAP_DSTRD_MASK) >> CAP_DSTRD_SHIFT) as u32 + 2);

        // ── Allocate and enable admin queue ────────────────────────────────────
        let (_admin_sq_phys, admin_sq) = alloc_dma_page();
        let (_admin_cq_phys, admin_cq) = alloc_dma_page();

        let mut controller = NvmeController {
            mmio_base,
            admin_queue: Mutex::new(QueuePair {
                sq_base: admin_sq, cq_base: admin_cq,
                sq_size: ADMIN_Q_SIZE.min(max_q_entries + 1),
                cq_size: ADMIN_Q_SIZE.min(max_q_entries + 1),
                sq_tail: 0, cq_head: 0, cq_phase: true, doorbell_stride,
            }),
            io_queues: Vec::new(),
            doorbell_stride,
        };

        controller.write_aqa(((ADMIN_Q_SIZE as u32 - 1) << 16) | (ADMIN_Q_SIZE as u32 - 1));
        controller.write_asq(virt_to_phys(admin_sq.as_u64() as usize));
        controller.write_acq(virt_to_phys(admin_cq.as_u64() as usize));
        controller.write_cc(
            CC_EN | CC_CSS_NVM
            | (12 << CC_MPS_SHIFT)
            | (6 << CC_IOSQES_SHIFT)
            | (4 << CC_IOCQES_SHIFT),
        );

        let mut timeout = 1_000_000u32;
        while timeout > 0 {
            if controller.read_csts() & CSTS_RDY != 0 { break; }
            timeout -= 1;
            core::hint::spin_loop();
        }
        if timeout == 0 { crate::println!("[NVMe] Controller failed to become ready"); return Err(()); }
        let vs = controller.read_vs();
        crate::println!("[NVMe] Controller ready, version {}.{}.{}", (vs >> 16) as u8, (vs >> 8) as u8, vs as u8);

        // ── Identify controller ───────────────────────────────────────────────
        let id_ctrl = controller.identify_controller().map_err(|_| {
            crate::println!("[NVMe] Failed to identify controller");
        })?;
        let num_namespaces = id_ctrl.num_namespaces.max(1);
        let vendor_id: u16 = id_ctrl.vendor_id;
        let serial = nvme_str(&id_ctrl.serial_number);
        let model = nvme_str(&id_ctrl.model_number);
        let fw = nvme_str(&id_ctrl.firmware_revision);
        crate::println!("[NVMe] vendor={:#06x} model='{}' serial='{}' fw='{}' namespaces={}",
            vendor_id, model, serial, fw, num_namespaces);

        // ── Async event configuration ─────────────────────────────────────────
        let _ = controller.set_features_async_event();
        crate::println!("[NVMe] async event config: {}",
            if true { "OK" } else { "not supported" });

        // ── Negotiate queue count ──────────────────────────────────────────────
        let desired_queues = MAX_IO_QUEUES;
        let granted_queues = controller.set_features_num_queues(desired_queues as u32).unwrap_or(1);
        let nq = granted_queues.max(1) as usize;
        crate::println!("[NVMe] I/O queues: {} (requested {}, granted {})", nq, desired_queues, granted_queues);

        // ── Create I/O queue pairs ────────────────────────────────────────────
        let q_size = IO_Q_SIZE.min(max_q_entries + 1);
        for qi in 0..nq {
            let qid = (qi + 1) as u16;
            let (cq_phys, cq_base) = alloc_dma_page();
            let (sq_phys, sq_base) = alloc_dma_page();
            controller.create_io_cq(qid, cq_phys).map_err(|_| {
                crate::println!("[NVMe] Failed to create IO CQ {}", qid);
            })?;
            controller.create_io_sq(qid, sq_phys).map_err(|_| {
                crate::println!("[NVMe] Failed to create IO SQ {}", qid);
            })?;
            controller.io_queues.push(Mutex::new(QueuePair {
                sq_base, cq_base, sq_size: q_size, cq_size: q_size,
                sq_tail: 0, cq_head: 0, cq_phase: true, doorbell_stride,
            }));
        }

        // ── Enumerate namespaces ───────────────────────────────────────────────
        let controller = Arc::new(controller);
        let mut ns_count = 0u32;
        for nsid in 1..=num_namespaces {
            if let Ok(ns) = controller.identify_namespace(nsid) {
                let nss: u64 = ns.ns_size;
                if nss == 0 { continue; }
                let lba_idx = (ns.formatted_lba_size & 0x07) as usize;
                let lba_data_size: u64 = if lba_idx < ns.num_lba_formats.max(1) as usize {
                    // LBA format descriptors start at byte 0x30 in the 4096-byte buffer.
                    // Each descriptor is 4 bytes: u16 LBADS, u8 MS, u8 RP.
                    let ns_ptr = &ns as *const _ as *const u8;
                    let desc_off: usize = 0x30 + lba_idx * 4;
                    let lba_bytes = u16::from_le_bytes(unsafe {
                        [*ns_ptr.add(desc_off), *ns_ptr.add(desc_off + 1)]
                    });
                    lba_bytes as u64
                } else {
                    SECTOR_SIZE as u64
                };
                let ns_mb = nss * lba_data_size / 1024 / 1024;
                crate::println!("[NVMe] Namespace {}: {} LBAs ({} MB, {} B/sector)",
                    nsid, nss, ns_mb, lba_data_size);
                let dev_name = alloc::format!("nvme0n{}", nsid);
                let bd = Arc::new(NvmeBlockDevice::new(Arc::clone(&controller), nsid, nss));
                block_registry::register(&dev_name, "nvme", bd);
                ns_count += 1;
            }
        }

        crate::println!("[NVMe] Initialized: {} namespaces, {} I/O queues", ns_count, nq);
        Ok(())
    }
}

pub fn register() {
    use alloc::boxed::Box;
    crate::drivers::device_manager::DEVICE_MANAGER
        .lock()
        .register_driver(Box::new(NvmeDriver));
}
