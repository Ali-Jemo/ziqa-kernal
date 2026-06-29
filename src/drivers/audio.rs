//! Intel High Definition Audio (HDA) Driver for ZiqaKernel.
//!
//! PCI class 0x04 (Multimedia), subclass 0x03 (Audio Device).
//! Provides a kernel audio subsystem skeleton that detects HDA controllers,
//! allocates DMA buffers, and initialises the CORB/RIRB command interface.
//!
//! # Architecture
//! - PCI enumeration via the generic `Driver` trait
//! - Memory-mapped I/O (BAR 0) for all register access
//! - CORB (Command Output Ring Buffer) for codec commands
//! - RIRB (Response Input Ring Buffer) for codec responses
//! - Immediate command (polled) for simple codec interaction
//!
//! # References
//! - Intel High Definition Audio Specification (rev 1.0a)
//! - Linux `sound/pci/hda/hda_controller.h` for register definitions

#![allow(dead_code)]

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::ptr::{read_volatile, write_volatile};
use spin::Mutex;
use crate::drivers::device_manager::Driver;
use crate::drivers::pci::{bar_address, PciDevice};

// ── PCI identifiers ───────────────────────────────────────────────────────────

/// PCI class 0x04 = Multimedia controller
const PCI_CLASS_MULTIMEDIA: u8 = 0x04;
/// Subclass 0x03 = Audio device
const PCI_SUBCLASS_AUDIO: u8 = 0x03;

/// Known HDA vendor IDs
const VENDOR_INTEL: u16 = 0x8086;

// ── Global register offsets (0x00–0x0F) ──────────────────────────────────

/// Global Capabilities (16-bit)
const REG_GCAP: u32 = 0x00;
/// Global Control (32-bit)
const REG_GCTL: u32 = 0x04;
/// Wake Enable (16-bit)
const REG_WAKEEN: u32 = 0x08;
/// State Change Status (16-bit)
const REG_STATESTS: u32 = 0x0A;
/// Global Status (16-bit)
const REG_GSTS: u32 = 0x0C;

// ── CORB registers (0x10–0x1F) ───────────────────────────────────────────

/// CORB Lower Base Address (32-bit)
const REG_CORBLBASE: u32 = 0x10;
/// CORB Upper Base Address (32-bit)
const REG_CORBUBASE: u32 = 0x14;
/// CORB Write Pointer (16-bit)
const REG_CORBWP: u32 = 0x18;
/// CORB Read Pointer (16-bit)
const REG_CORBRP: u32 = 0x1A;
/// CORB Control (8-bit)
const REG_CORBCTL: u32 = 0x1C;
/// CORB Status (8-bit)
const REG_CORBSTS: u32 = 0x1D;

// ── RIRB registers (0x20–0x2F) ───────────────────────────────────────────

/// RIRB Lower Base Address (32-bit)
const REG_RIRBLBASE: u32 = 0x20;
/// RIRB Upper Base Address (32-bit)
const REG_RIRBUBASE: u32 = 0x24;
/// RIRB Write Pointer (16-bit)
const REG_RIRBWP: u32 = 0x28;
/// Response Interrupt Count (16-bit)
const REG_RINTCNT: u32 = 0x2A;
/// RIRB Control (8-bit)
const REG_RIRBCTL: u32 = 0x2C;
/// RIRB Status (8-bit)
const REG_RIRBSTS: u32 = 0x2D;

// ── Immediate Command registers (0x60–0x68) ──────────────────────────────

/// Immediate Command Write (32-bit)
const REG_ICW: u32 = 0x60;
/// Immediate Response Read (32-bit)
const REG_IRR: u32 = 0x64;
/// Immediate Command Status (16-bit)
const REG_IRS: u32 = 0x68;

// ── GCTL bits ──────────────────────────────────────────────────────────────

/// Controller reset (1 = reset, 0 = running)
const GCTL_RESET: u32 = 1;
/// Flush Control
const GCTL_FC: u32 = 1 << 1;
/// Accept Unsolicited Response Enable
const GCTL_UNSOL: u32 = 1 << 8;

// ── CORBCTL bits ────────────────────────────────────────────────────────────

/// CORB Enable
const CORBCTL_ENABLE: u8 = 1 << 0;
/// CORB DMA Run
const CORBCTL_RUN: u8 = 1 << 1;

// ── RIRBCTL bits ────────────────────────────────────────────────────────────

/// RIRB Enable
const RIRBCTL_ENABLE: u8 = 1 << 0;
/// RIRB DMA Run
const RIRBCTL_RUN: u8 = 1 << 1;
/// RIRB Interrupt on Response
const RIRBCTL_INT: u8 = 1 << 2;

// ── IRS bits ────────────────────────────────────────────────────────────────

/// ICW valid / pending
const IRS_VALID: u16 = 1 << 0;
/// ICW busy
const IRS_BUSY: u16 = 1 << 1;

// ── Constants ────────────────────────────────────────────────────────────────

/// Maximum number of codecs on the HDA link
const MAX_CODECS: usize = 16;
/// CORB entries (must be power of 2)
const CORB_SIZE: usize = 256;
/// RIRB entries (must be power of 2)
const RIRB_SIZE: usize = 256;
/// Immediate command poll timeout (μs × 10)
const CMD_TIMEOUT: u32 = 100;

// ── Structures ───────────────────────────────────────────────────────────────

/// Detected HDA codec
#[derive(Debug, Clone)]
struct HdaCodec {
    address: u8,
    vendor_id: u32,
}

/// HDA controller state
pub struct HdaController {
    /// MMIO base address from BAR 0
    mmio_base: *mut u8,
    /// CORB ring buffer (host → codec)
    corb_buf: *mut u32,
    corb_phys: u64,
    /// RIRB ring buffer (codec → host)
    rirb_buf: *mut u32,
    rirb_phys: u64,
    /// Detected codecs
    codecs: Vec<HdaCodec>,
}

unsafe impl Send for HdaController {}
unsafe impl Sync for HdaController {}

impl HdaController {
    /// Create and initialise a new HDA controller from a PCI device.
    fn new(pci: &PciDevice) -> Result<Self, ()> {
        let base = bar_address(pci.bars[0]).0;
        let mmio_base = base as *mut u8;

        // Allocate DMA buffers for CORB and RIRB
        let (corb_phys, corb_virt) = allocate_dma_buffer(CORB_SIZE * 4)?;
        let (rirb_phys, rirb_virt) = allocate_dma_buffer(RIRB_SIZE * 4)?;

        Ok(Self {
            mmio_base,
            corb_buf: corb_virt as *mut u32,
            corb_phys,
            rirb_buf: rirb_virt as *mut u32,
            rirb_phys,
            codecs: Vec::new(),
        })
    }

    /// Reset the HDA controller.
    fn controller_reset(&self) {
        unsafe {
            // Assert reset
            write_volatile(self.mmio_base.add(REG_GCTL as usize) as *mut u32, GCTL_RESET);
            // Wait for reset to complete (poll until bit clears)
            for _ in 0..CMD_TIMEOUT {
                if read_volatile(self.mmio_base.add(REG_GCTL as usize) as *const u32) & GCTL_RESET == 0 {
                    break;
                }
            }
        }
    }

    /// Initialise CORB and RIRB DMA engines.
    fn init_corb_rirb(&self) {
        unsafe {
            // Stop CORB/RIRB
            write_volatile(self.mmio_base.add(REG_CORBCTL as usize) as *mut u8, 0);
            write_volatile(self.mmio_base.add(REG_RIRBCTL as usize) as *mut u8, 0);

            // Clear buffers
            core::ptr::write_bytes(self.corb_buf, 0, CORB_SIZE * 4);
            core::ptr::write_bytes(self.rirb_buf, 0, RIRB_SIZE * 4);

            // Set CORB base addresses (lower 32 bits only for now)
            write_volatile(
                self.mmio_base.add(REG_CORBLBASE as usize) as *mut u32,
                self.corb_phys as u32,
            );
            // Clear upper base
            write_volatile(self.mmio_base.add(REG_CORBUBASE as usize) as *mut u32, 0);

            // Set RIRB base addresses
            write_volatile(
                self.mmio_base.add(REG_RIRBLBASE as usize) as *mut u32,
                self.rirb_phys as u32,
            );
            write_volatile(self.mmio_base.add(REG_RIRBUBASE as usize) as *mut u32, 0);

            // Reset CORB read pointer and clear status
            write_volatile(self.mmio_base.add(REG_CORBRP as usize) as *mut u16, 0u16);
            write_volatile(self.mmio_base.add(REG_CORBSTS as usize) as *mut u8, 0xFF);

            // Reset RIRB write pointer and clear status
            write_volatile(self.mmio_base.add(REG_RIRBWP as usize) as *mut u16, 0u16);
            write_volatile(self.mmio_base.add(REG_RIRBSTS as usize) as *mut u8, 0xFF);

            // Set response interrupt count to 1
            write_volatile(self.mmio_base.add(REG_RINTCNT as usize) as *mut u16, 1u16);

            // Start CORB and RIRB
            let corb_ctl = CORBCTL_ENABLE | CORBCTL_RUN;
            write_volatile(self.mmio_base.add(REG_CORBCTL as usize) as *mut u8, corb_ctl);

            let rirb_ctl = RIRBCTL_ENABLE | RIRBCTL_RUN;
            write_volatile(self.mmio_base.add(REG_RIRBCTL as usize) as *mut u8, rirb_ctl);
        }
    }

    /// Send an immediate verb and return the response (polled).
    fn send_immediate_verb(&self, codec: u8, verb: u32) -> Result<u32, ()> {
        let cmd = (codec as u32) << 28 | verb;
        unsafe {
            // Wait for ICW to be available
            for _ in 0..CMD_TIMEOUT {
                let irs = read_volatile(self.mmio_base.add(REG_IRS as usize) as *const u16);
                if irs & IRS_VALID == 0 {
                    break;
                }
            }

            // Write command
            write_volatile(self.mmio_base.add(REG_ICW as usize) as *mut u32, cmd);

            // Wait for response
            for _ in 0..CMD_TIMEOUT {
                let irs = read_volatile(self.mmio_base.add(REG_IRS as usize) as *const u16);
                if irs & IRS_VALID != 0 {
                    let resp = read_volatile(self.mmio_base.add(REG_IRR as usize) as *const u32);
                    return Ok(resp);
                }
            }
        }
        Err(())
    }

    /// Enumerate codecs on the HDA link.
    fn enumerate_codecs(&mut self) {
        for addr in 0u8..=15 {
            // Root Compound (codec 0) is always present on most hardware
            if addr == 0 {
                if let Ok(vendor_id) = self.send_immediate_verb(addr, 0x000F0000) {
                    self.codecs.push(HdaCodec { address: addr, vendor_id });
                }
            } else {
                // Probe other codec addresses
                if let Ok(vendor_id) = self.send_immediate_verb(addr, 0x000F0000) {
                    self.codecs.push(HdaCodec { address: addr, vendor_id });
                }
            }
        }
    }

    /// Full controller initialisation sequence.
    fn initialize(&mut self) {
        self.controller_reset();

        // Release reset
        unsafe {
            write_volatile(self.mmio_base.add(REG_GCTL as usize) as *mut u32, 0u32);
        }

        self.init_corb_rirb();
        self.enumerate_codecs();

        crate::println!(
            "[HDA] Controller initialised, {} codec(s) found",
            self.codecs.len()
        );
        for codec in &self.codecs {
            crate::println!(
                "[HDA]   Codec {}: vendor 0x{:04X}",
                codec.address,
                (codec.vendor_id >> 16) as u16,
            );
        }
    }
}

/// Global HDA controller singleton
pub static HDA_CONTROLLER: Mutex<Option<HdaController>> = Mutex::new(None);

/// Allocate a DMA-capable buffer (physically contiguous, page-aligned).
fn allocate_dma_buffer(size: usize) -> Result<(u64, *mut u8), ()> {
    use crate::memory::paging::phys_offset;
    use crate::memory::FRAME_ALLOCATOR;
    use x86_64::structures::paging::FrameAllocator;
    use x86_64::VirtAddr;

    let pages_needed = (size + 4095) / 4096;
    let mut fa_guard = FRAME_ALLOCATOR.lock();
    let fa = fa_guard.as_mut().expect("frame allocator not ready");

    let first_addr = fa.allocate_frame().ok_or(())?.start_address().as_u64();
    for _ in 1..pages_needed {
        fa.allocate_frame().ok_or(())?;
    }

    let virt_addr = VirtAddr::new(phys_offset().as_u64() + first_addr);
    Ok((first_addr, virt_addr.as_mut_ptr()))
}

// ── Driver registration ──────────────────────────────────────────────────────

pub struct HdaDriver;

impl Driver for HdaDriver {
    fn name(&self) -> &str {
        "hda"
    }

    fn pci_match(&self, device: &PciDevice) -> bool {
        device.class == PCI_CLASS_MULTIMEDIA
            && device.subclass == PCI_SUBCLASS_AUDIO
            && (device.vendor_id == VENDOR_INTEL
                || device.vendor_id == 0x1002  // AMD
                || device.vendor_id == 0x10de  // NVIDIA
                || device.vendor_id == 0x1022) // AMD Family
    }

    fn init(&self, device: &PciDevice) -> Result<(), ()> {
        crate::println!(
            "[HDA] Initialising HDA controller {:04X}:{:04X}",
            device.vendor_id,
            device.device_id
        );

        let mut controller = HdaController::new(device)?;
        controller.initialize();

        *HDA_CONTROLLER.lock() = Some(controller);
        Ok(())
    }
}

/// Register the HDA driver with the global device manager.
pub fn register() {
    crate::drivers::device_manager::DEVICE_MANAGER
        .lock()
        .register_driver(Box::new(HdaDriver));
    crate::println!("[HDA] Driver registered");
}

// ── Public API ───────────────────────────────────────────────────────────────

/// Check if an HDA controller was found and initialised.
pub fn is_available() -> bool {
    HDA_CONTROLLER.lock().is_some()
}

/// Number of detected audio codecs.
pub fn codec_count() -> usize {
    HDA_CONTROLLER
        .lock()
        .as_ref()
        .map(|c| c.codecs.len())
        .unwrap_or(0)
}
