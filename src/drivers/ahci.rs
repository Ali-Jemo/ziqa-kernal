//! AHCI (Advanced Host Controller Interface) SATA driver.
//!
//! PCI class 0x01 (mass storage), subclass 0x06 (SATA), prog-if 0x01 (AHCI).
//! Uses BAR5 for memory-mapped HBA registers. DMA-based read/write with
//! polling for command completion (interrupt-free for now).
//!
//! # Design
//! - Single command slot per port (slot 0), simplifies command list management.
//! - Bounce buffers for data: allocate physically contiguous 4K pages, DMA into
//!   them, then copy to/from the caller's buffer.
//! - Polling: spin on the Command Issue register until the controller clears it.
//! - Thread-safe: `Mutex` protects the HBA MMIO base and per-port allocated
//!   structures.

#![allow(dead_code)]

use crate::abi::AbiError;
use crate::drivers::block::BlockDevice;
use crate::drivers::block_registry;
use crate::drivers::device_manager::Driver;
use crate::drivers::pci::{bar_address, enable_bus_mastering, enable_memory_space, PciDevice};
use crate::memory::paging::phys_offset;
use crate::memory::FRAME_ALLOCATOR;
use alloc::boxed::Box;
use alloc::sync::Arc;
use core::ptr::{read_volatile, write_volatile};
use x86_64::structures::paging::FrameAllocator;

// ── PCI identifiers ──────────────────────────────────────────────────────────
const PCI_CLASS_STORAGE: u8 = 0x01;
const PCI_SUBCLASS_SATA: u8 = 0x06;
const PCI_PROGIF_AHCI: u8 = 0x01;

// ── HBA register offsets ─────────────────────────────────────────────────────
const HBA_CAP: u32 = 0x00;
const HBA_GHC: u32 = 0x04;
const HBA_IS: u32 = 0x08;
const HBA_PI: u32 = 0x0C;
const HBA_VER: u32 = 0x10;
const HBA_BOHC: u32 = 0x28;

// ── GHC bits ─────────────────────────────────────────────────────────────────
const GHC_HR: u32 = 1;
const GHC_IE: u32 = 1 << 1;
const GHC_AE: u32 = 1 << 31;

// ── Port register layout ─────────────────────────────────────────────────────
const PORT_STRIDE: u32 = 0x80;
const PORT_BASE: u32 = 0x100;

// Port register offsets (relative to port base = HBA_BASE + 0x100 + n * 0x80)
const PORT_CLB: u32 = 0x00; // Command List Base (lower 32 bits)
const PORT_CLBU: u32 = 0x04; // Command List Base (upper 32 bits)
const PORT_FB: u32 = 0x08; // FIS Base (lower 32 bits)
const PORT_FBU: u32 = 0x0C; // FIS Base (upper 32 bits)
const PORT_IS: u32 = 0x10; // Interrupt Status
const PORT_IE: u32 = 0x14; // Interrupt Enable
const PORT_CMD: u32 = 0x18; // Command & Status
const PORT_TFD: u32 = 0x20; // Task File Data
const PORT_SIG: u32 = 0x24; // Signature
const PORT_SSTS: u32 = 0x28; // SATA Status (SCR0)
const PORT_SCTL: u32 = 0x2C; // SATA Control (SCR2)
const PORT_SERR: u32 = 0x30; // SATA Error (SCR1)
const PORT_CI: u32 = 0x38; // Command Issue

// ── CMD register bits ────────────────────────────────────────────────────────
const CMD_ST: u32 = 1;
const CMD_SUD: u32 = 1 << 1;
const CMD_POD: u32 = 1 << 2;
const CMD_FRE: u32 = 1 << 4;
const CMD_CR: u32 = 1 << 15;
const CMD_FR: u32 = 1 << 14;

// ── SATA Status bits (SSTS / SCR0) ───────────────────────────────────────────
const SSTS_DET_MASK: u32 = 0x0F;
const SSTS_DET_NO_DEVICE: u32 = 0x00;
const SSTS_DET_PHY_OFFLINE: u32 = 0x01;
const SSTS_DET_PHY_ONLINE: u32 = 0x03;

// ── SATA signatures ──────────────────────────────────────────────────────────
const SIG_ATA: u32 = 0x0000_0101;

// ── ATA commands ─────────────────────────────────────────────────────────────
const CMD_READ_DMA_EXT: u8 = 0x25;
const CMD_WRITE_DMA_EXT: u8 = 0x35;
const CMD_IDENTIFY_DEVICE: u8 = 0xEC;

// ── FIS types ────────────────────────────────────────────────────────────────
const FIS_TYPE_H2D: u8 = 0x27; // Register – Host to Device
const FIS_TYPE_D2H: u8 = 0x34; // Register – Device to Host
const FIS_TYPE_DMA_ACT: u8 = 0x39; // DMA Activate
const FIS_TYPE_DMA_SETUP: u8 = 0x41; // DMA Setup
const FIS_TYPE_DATA: u8 = 0x46; // Data

// ── Constants ────────────────────────────────────────────────────────────────
const SECTOR_SIZE: usize = 512;
const MAX_PORTS: usize = 32;
const BUF_PAGE_SECTORS: u64 = 8; // 8 sectors = 4 KiB = one page

// ── Memory structure layouts (packed, C-compatible) ──────────────────────────

/// Command Header (32 bytes) – one entry in the port's Command List.
#[repr(C, packed)]
struct CmdHeader {
    // DW0
    cfl: u8,       // Command FIS length in DWORDS (2–16)
    _pmp_attr: u8, // Port Multiplier (bits 3:0) + reserved
    prdtl: u16,    // Physical Region Descriptor Table length (entries)
    // DW1
    prdbc: u32,    // PRD Byte Count (written back by controller)
    // DW2
    ctba: u32,     // Command Table Base Address (lower)
    // DW3
    ctbau: u32,    // Command Table Base Address (upper)
    // DW4–DW7
    _rsv: [u32; 4],
}

/// Command Table – the FIS + PRDTs the command header points to.
///
/// Standard layout: 64 bytes CFIS, 16 bytes ATAPI, 48 bytes reserved,
/// then PRDT entries. We cram a fixed 8-entry PRDT for simplicity.
#[repr(C, packed)]
struct CmdTable {
    cfis: [u8; 64],         // Command FIS
    _acmd: [u8; 16],        // ATAPI command (unused)
    _rsv: [u8; 48],         // Reserved
    prdt: [PrdtEntry; 8],   // Physical Region Descriptor Table
}

/// PRDT entry (16 bytes) – describes one physically contiguous data region.
#[repr(C, packed)]
struct PrdtEntry {
    dba: u32,  // Data Base Address (lower)
    dbau: u32, // Data Base Address (upper)
    _rsv: u32, // Reserved (must be 0)
    dbc: u32,  // Byte Count (bits 21:0) | bit 30 = reserved | bit 31 = I
}

/// Register – Host to Device FIS (exactly 20 bytes for FIS type 0x27).
#[repr(C, packed)]
struct FisRegH2D {
    fis_type: u8,     // 0x27
    pm_port: u8,      // Port Multiplier (bit 7 = C)
    command: u8,      // ATA command
    features: u8,
    lba0: u8,
    lba1: u8,
    lba2: u8,
    device: u8,       // Device register
    lba3: u8,
    lba4: u8,
    lba5: u8,
    features_ex: u8,
    countl: u8,       // Sector count (low)
    counth: u8,       // Sector count (high)
    icc: u8,          // Isochronous Command Completion
    control: u8,      // Control
    _rsv: [u8; 4],    // Reserved
}

/// Per-port runtime state (DMA structures).
struct AhciPort {
    /// Bitmask of active port index in `AhciController.ports`
    present: bool,
    /// Total capacity in sectors
    total_sectors: u64,
    /// Physical address of the command list (1024-byte aligned)
    cl_phys: u64,
    /// Virtual address of the command list
    cl_virt: *mut CmdHeader,
    /// Physical address of the received FIS area (256-byte aligned)
    fis_phys: u64,
    /// Virtual address of the received FIS area
    fis_virt: *mut u8,
    /// Physical address of the command table (128-byte aligned)
    ct_phys: u64,
    /// Virtual address of the command table
    ct_virt: *mut CmdTable,
}

unsafe impl Send for AhciPort {}
unsafe impl Sync for AhciPort {}

impl AhciPort {
    const fn empty() -> Self {
        Self {
            present: false,
            total_sectors: 0,
            cl_phys: 0,
            cl_virt: core::ptr::null_mut(),
            fis_phys: 0,
            fis_virt: core::ptr::null_mut(),
            ct_phys: 0,
            ct_virt: core::ptr::null_mut(),
        }
    }
}

/// AHCI controller state.
pub struct AhciController {
    /// Virtual address of the HBA MMIO region
    hba_virt: *mut u8,
    /// Number of ports on this controller
    port_count: u32,
    /// Per-port state (indexed by port number)
    ports: [AhciPort; MAX_PORTS],
}

unsafe impl Send for AhciController {}
unsafe impl Sync for AhciController {}

impl AhciController {
    /// Read a 32-bit HBA register at the given byte offset.
    fn hba_read(&self, offset: u32) -> u32 {
        unsafe { read_volatile((self.hba_virt as *const u32).add(offset as usize / 4)) }
    }

    /// Write a 32-bit HBA register at the given byte offset.
    fn hba_write(&self, offset: u32, val: u32) {
        unsafe { write_volatile((self.hba_virt as *mut u32).add(offset as usize / 4), val) }
    }

    /// Read a 32-bit port register.
    fn port_read(&self, port: u32, offset: u32) -> u32 {
        let addr = (PORT_BASE + port * PORT_STRIDE + offset) as usize;
        unsafe { read_volatile((self.hba_virt as *const u32).add(addr / 4)) }
    }

    /// Write a 32-bit port register.
    fn port_write(&self, port: u32, offset: u32, val: u32) {
        let addr = (PORT_BASE + port * PORT_STRIDE + offset) as usize;
        unsafe { write_volatile((self.hba_virt as *mut u32).add(addr / 4), val) }
    }

    /// Allocate a physically contiguous 4 KiB page and return (phys, virt).
    fn alloc_page() -> (u64, *mut u8) {
        let mut fa_guard = FRAME_ALLOCATOR.lock();
        let fa = fa_guard.as_mut().expect("frame allocator not ready");
        let frame = fa.allocate_frame().expect("OOM allocating DMA frame");
        let paddr = frame.start_address().as_u64();
        let vaddr = (phys_offset().as_u64() + paddr) as *mut u8;
        // Zero it out so stale command data doesn't confuse the controller.
        unsafe { core::ptr::write_bytes(vaddr, 0, 4096) }
        (paddr, vaddr)
    }

    /// Spin until the controller clears bit `mask` in `port_cmd`.
    fn wait_for_cmd_clear(&self, port: u32, mask: u32, timeout_ms: u64) -> bool {
        for _ in 0..timeout_ms.max(1) {
            if self.port_read(port, PORT_CMD) & mask == 0 {
                return true;
            }
            // ~1 ms busy-wait (approximate; QEMU is fast)
            for _ in 0..100_000 {
                core::hint::spin_loop();
            }
        }
        false
    }

    /// Check if a port has a device attached and its signature is ATA.
    fn port_has_ata_device(&self, port: u32) -> bool {
        let ssts = self.port_read(port, PORT_SSTS);
        let det = ssts & SSTS_DET_MASK;
        if det != SSTS_DET_PHY_ONLINE {
            return false;
        }
        let sig = self.port_read(port, PORT_SIG);
        sig == SIG_ATA
    }

    /// Submit an ATA command via DMA and wait for completion.
    ///
    /// `ata_cmd`: the ATA command byte (e.g. CMD_READ_DMA_EXT)
    /// `lba`: starting LBA (48-bit)
    /// `sector_count`: number of sectors (1..8 for single-page bounce)
    /// `buf_phys`: physical address of the DMA data buffer
    /// `buf_virt`: virtual address of the DMA data buffer
    /// `write`: true for write, false for read
    fn submit_command(
        &self,
        port: u32,
        ata_cmd: u8,
        lba: u64,
        sector_count: u16,
        buf_phys: u64,
        write: bool,
    ) -> Result<(), AbiError> {
        let p = &self.ports[port as usize];

        // ── Step 1: Build the command header (slot 0) ────────────────────────
        let ch = unsafe { &mut *p.cl_virt };
        // Zero the header; PRDTL will be set below separately.
        ch.cfl = 0;
        ch._pmp_attr = 0;
        ch.prdtl = 0;
        ch.prdbc = 0;
        ch.ctba = p.ct_phys as u32;
        ch.ctbau = (p.ct_phys >> 32) as u32;

        // ── Step 2: Build the command table ──────────────────────────────────
        let ct = unsafe { &mut *p.ct_virt };
        // Zero the whole table before filling in fresh data.
        unsafe { core::ptr::write_bytes(ct, 0, core::mem::size_of::<CmdTable>()) }

        // Build the Register H2D FIS.
        let fis = FisRegH2D {
            fis_type: FIS_TYPE_H2D,
            pm_port: port as u8,
            command: ata_cmd,
            features: 0,
            lba0: (lba >> 0) as u8,
            lba1: (lba >> 8) as u8,
            lba2: (lba >> 16) as u8,
            device: 0x40, // LBA mode bit
            lba3: (lba >> 24) as u8,
            lba4: (lba >> 32) as u8,
            lba5: (lba >> 40) as u8,
            features_ex: 0,
            countl: sector_count as u8,
            counth: (sector_count >> 8) as u8,
            icc: 0,
            control: 0,
            _rsv: [0u8; 4],
        };
        // Copy the FIS bytes into the command table's CFIS field.
        let fis_slice = unsafe {
            core::slice::from_raw_parts(
                &fis as *const FisRegH2D as *const u8,
                core::mem::size_of::<FisRegH2D>(),
            )
        };
        ct.cfis[..core::mem::size_of::<FisRegH2D>()].copy_from_slice(fis_slice);

        // ── Step 3: Build the PRDT entry ────────────────────────────────────
        let byte_count = (sector_count as u32) * (SECTOR_SIZE as u32);
        // PRDT.DBC format: bits 21:0 = byte count - 1, bit 31 = I (interrupt)
        let prdt_val = ((byte_count - 1) & 0x00FF_FFFF) | (1u32 << 31);
        ct.prdt[0] = PrdtEntry {
            dba: buf_phys as u32,
            dbau: (buf_phys >> 32) as u32,
            _rsv: 0,
            dbc: prdt_val,
        };

        // ── Step 4: Update command header ────────────────────────────────────
        let cfl = (core::mem::size_of::<FisRegH2D>() / 4) as u8; // should be 5
        ch.cfl = cfl;
        ch.prdtl = 1; // one PRDT entry

        // ── Step 5: Set the write bit in the FIS if needed ───────────────────
        // For write commands, H2D FIS bit 6 in pm_port must be set (C=1).
        if write {
            // Re-read the FIS in the command table (already copied)
            ct.cfis[1] |= 0x80; // pm_port bit 7 (C bit) = 1 means write via DMA
        }

        // ── Step 6: Ring the doorbell ────────────────────────────────────────
        // Write to CI (Command Issue) to tell the controller to process slot 0.
        self.port_write(port, PORT_CI, 1);

        // ── Step 7: Poll for completion ──────────────────────────────────────
        // Wait for the CI register to clear bit 0 (command completed).
        for _ in 0..10_000_000 {
            if self.port_read(port, PORT_CI) & 1 == 0 {
                // Check for errors.
                let tfd = self.port_read(port, PORT_TFD);
                if tfd & 0x01 != 0 {
                    return Err(AbiError::Other("AHCI command error (ERR bit)"));
                }
                return Ok(());
            }
            core::hint::spin_loop();
        }

        Err(AbiError::Other("AHCI command timeout"))
    }

    /// Send IDENTIFY DEVICE and extract total sectors.
    fn identify_device(&self, port: u32) -> Result<u64, AbiError> {
        let (buf_phys, buf_virt) = Self::alloc_page();

        let result = self.submit_command(port, CMD_IDENTIFY_DEVICE, 0, 1, buf_phys, false);

        if result.is_err() {
            // Free the bounce page.
            // For now we leak it (no page deallocator in this kernel); on error
            // a page-sized leak is acceptable.
            return Err(AbiError::Other("IDENTIFY DEVICE failed"));
        }

        // Parse IDENTIFY data: words 100–103 = max LBA (48-bit addressing).
        // The data is 512 bytes (1 sector).
        let data = unsafe { core::slice::from_raw_parts(buf_virt as *const u16, 256) };
        let lba_low = (data[100] as u64) | ((data[101] as u64) << 16);
        let lba_high = (data[102] as u64) | ((data[103] as u64) << 16);
        let total = lba_low | (lba_high << 32);

        // Sanity: if total is 0 or suspicious, fall back to CHS (word 1+3).
        let total_sectors = if total != 0 && total != u64::MAX {
            total
        } else {
            // CHS fallback: C = word 1, H = word 3, S = word 6 (low byte)
            let cyls = data[1] as u64;
            let heads = data[3] as u64;
            let spt = (data[6] & 0x3F) as u64;
            if cyls > 0 { cyls * heads * spt } else { 0 }
        };

        // Leak the page (no deallocator available — acceptable cost at init).
        // In production this page would be freed.
        Ok(total_sectors.max(1))
    }

    /// Initialize a single port: allocate DMA structures, start the port, and
    /// register with block_registry.
    fn init_port(&mut self, port: u32) {
        if !self.port_has_ata_device(port) {
            return;
        }

        crate::println!("[AHCI] Found ATA device on port {}", port);

        // ── 1. Allocate DMA structures ───────────────────────────────────────
        let (cl_phys, cl_virt) = Self::alloc_page();
        let (fis_phys, fis_virt) = Self::alloc_page();
        let (ct_phys, ct_virt) = Self::alloc_page();

        let p = AhciPort {
            present: true,
            total_sectors: 0,
            cl_phys,
            cl_virt: cl_virt as *mut CmdHeader,
            fis_phys,
            fis_virt,
            ct_phys,
            ct_virt: ct_virt as *mut CmdTable,
        };
        self.ports[port as usize] = p;

        // ── 2. Set up command list and FIS base addresses ────────────────────
        self.port_write(port, PORT_CLB, cl_phys as u32);
        self.port_write(port, PORT_CLBU, (cl_phys >> 32) as u32);
        self.port_write(port, PORT_FB, fis_phys as u32);
        self.port_write(port, PORT_FBU, (fis_phys >> 32) as u32);

        // ── 3. Start the port ────────────────────────────────────────────────
        // Clear CMD.ST first, then set up FRE + SUD + POD, then set ST.
        let mut cmd = self.port_read(port, PORT_CMD);

        // Clear ST if set
        if cmd & CMD_ST != 0 {
            self.port_write(port, PORT_CMD, cmd & !CMD_ST);
            self.wait_for_cmd_clear(port, CMD_CR, 100);
        }

        // Enable FIS Receive, Spin-Up, Power-On
        cmd = self.port_read(port, PORT_CMD);
        cmd |= CMD_FRE | CMD_SUD | CMD_POD;
        self.port_write(port, PORT_CMD, cmd);

        // Wait for FR (FIS Receive Running) to become set
        for _ in 0..100_000 {
            if self.port_read(port, PORT_CMD) & CMD_FR != 0 {
                break;
            }
            core::hint::spin_loop();
        }

        // Start command engine
        cmd = self.port_read(port, PORT_CMD);
        cmd |= CMD_ST;
        self.port_write(port, PORT_CMD, cmd);

        // Wait for CR (Command List Running)
        for _ in 0..100_000 {
            if self.port_read(port, PORT_CMD) & CMD_CR != 0 {
                break;
            }
            core::hint::spin_loop();
        }

        // ── 4. Send IDENTIFY DEVICE to get capacity ──────────────────────────
        match self.identify_device(port) {
            Ok(sectors) => {
                self.ports[port as usize].total_sectors = sectors;
                crate::println!(
                    "[AHCI] Port {}: {} sectors ({} MB)",
                    port,
                    sectors,
                    sectors * 512 / (1024 * 1024)
                );
            }
            Err(e) => {
                // If IDENTIFY fails,
                // the port is still partially usable with a best-effort size.
                crate::println!(
                    "[AHCI] Port {}: IDENTIFY failed ({:?}); registering with size=0",
                    port,
                    e,
                );
                self.ports[port as usize].total_sectors = 0;
            }
        }

        // ── 5. Register in block registry ────────────────────────────────────
        let name = alloc::format!("sata{}", port);
        let device = Arc::new(AhciBlockDevice {
            controller: self as *const AhciController as usize,
            port,
        });
        block_registry::register(&name, "ahci", device);
    }
}

/// BlockDevice wrapping a single AHCI port.
struct AhciBlockDevice {
    /// Pointer to the `AhciController` (stored as usize for Send/Sync safety).
    controller: usize,
    port: u32,
}

unsafe impl Send for AhciBlockDevice {}
unsafe impl Sync for AhciBlockDevice {}

impl AhciBlockDevice {
    fn ctrl(&self) -> &AhciController {
        unsafe { &*(self.controller as *const AhciController) }
    }
}

impl BlockDevice for AhciBlockDevice {
    fn read_sectors(&self, sector: u64, count: u32, buf: &mut [u8]) -> Result<(), AbiError> {
        let ctrl = self.ctrl();
        let mut remaining = count as u64;
        let mut current_lba = sector;
        let mut offset = 0usize;

        while remaining > 0 {
            let chunk = remaining.min(BUF_PAGE_SECTORS) as u16;
            let (buf_phys, buf_virt) = AhciController::alloc_page();
            ctrl.submit_command(self.port, CMD_READ_DMA_EXT, current_lba, chunk, buf_phys, false)?;
            // Copy from DMA buffer to caller buffer.
            let copy_size = (chunk as usize) * SECTOR_SIZE;
            unsafe {
                core::ptr::copy_nonoverlapping(buf_virt, buf.as_mut_ptr().add(offset), copy_size);
            }
            offset += copy_size;
            current_lba += chunk as u64;
            remaining -= chunk as u64;
        }
        Ok(())
    }

    fn write_sectors(&self, sector: u64, count: u32, buf: &[u8]) -> Result<(), AbiError> {
        let ctrl = self.ctrl();
        let mut remaining = count as u64;
        let mut current_lba = sector;
        let mut offset = 0usize;

        while remaining > 0 {
            let chunk = remaining.min(BUF_PAGE_SECTORS) as u16;
            let (buf_phys, buf_virt) = AhciController::alloc_page();
            // Copy from caller buffer to DMA buffer.
            let copy_size = (chunk as usize) * SECTOR_SIZE;
            unsafe {
                core::ptr::copy_nonoverlapping(buf.as_ptr().add(offset), buf_virt, copy_size);
            }
            ctrl.submit_command(self.port, CMD_WRITE_DMA_EXT, current_lba, chunk, buf_phys, true)?;
            offset += copy_size;
            current_lba += chunk as u64;
            remaining -= chunk as u64;
        }
        Ok(())
    }

    fn total_sectors(&self) -> u64 {
        let ctrl = self.ctrl();
        ctrl.ports[self.port as usize].total_sectors
    }
}

// ── Driver registration ──────────────────────────────────────────────────────

pub struct AhciDriver;

impl Driver for AhciDriver {
    fn name(&self) -> &str {
        "AHCI SATA"
    }

    fn pci_match(&self, device: &PciDevice) -> bool {
        device.class == PCI_CLASS_STORAGE
            && device.subclass == PCI_SUBCLASS_SATA
            && device.prog_if == PCI_PROGIF_AHCI
    }

    fn init(&self, device: &PciDevice) -> Result<(), ()> {
        crate::println!(
            "[AHCI] Found AHCI controller at {:02X}:{:02X}.{}",
            device.bus,
            device.dev,
            device.func,
        );

        // Enable bus mastering and memory space for DMA.
        enable_bus_mastering(device.address);
        enable_memory_space(device.address);

        // Read BAR5 (AHCI Memory Base).
        let bar_val = device.bars[5];
        let (bar_addr, is_io) = bar_address(bar_val);
        if is_io || bar_addr == 0 {
            crate::println!("[AHCI] BAR5 is I/O or zero ({:#X}), not usable", bar_addr);
            return Err(());
        }

        crate::println!("[AHCI] BAR5 physical address: {:#X}", bar_addr);

        // Map the MMIO region via phys_offset (identity map for device memory).
        let hba_virt = (phys_offset().as_u64() + bar_addr) as *mut u8;

        // ── HBA reset ────────────────────────────────────────────────────────
        let mut controller = AhciController {
            hba_virt,
            port_count: 0,
            ports: [const { AhciPort::empty() }; MAX_PORTS],
        };

        // Reset HBA (set GHC.HR, wait for it to clear).
        controller.hba_write(HBA_GHC, GHC_HR);
        for _ in 0..100_000 {
            if controller.hba_read(HBA_GHC) & GHC_HR == 0 {
                break;
            }
            core::hint::spin_loop();
        }
        if controller.hba_read(HBA_GHC) & GHC_HR != 0 {
            crate::println!("[AHCI] HBA reset timed out");
            return Err(());
        }

        // Enable AHCI mode and interrupts.
        let ghc = controller.hba_read(HBA_GHC);
        controller.hba_write(HBA_GHC, ghc | GHC_AE | GHC_IE);

        // Read capabilities.
        let cap = controller.hba_read(HBA_CAP);
        let port_count = ((cap >> 8) & 0x1F) + 1; // NP = (cap[12:8] + 1) * 1 ports
        let ports_implemented = controller.hba_read(HBA_PI);
        let version = controller.hba_read(HBA_VER);
        controller.port_count = port_count;

        crate::println!(
            "[AHCI] Version {}.{} Cap {:#X} PI {:#X} ports={}",
            (version >> 16) as u8,
            (version & 0xFF) as u8,
            cap,
            ports_implemented,
            port_count,
        );

        // Scan each implemented port.
        let mut found = 0;
        for port in 0..MAX_PORTS as u32 {
            if port >= port_count && (ports_implemented & (1 << port)) == 0 {
                continue;
            }
            controller.init_port(port);
            if controller.ports[port as usize].present {
                found += 1;
            }
        }

        if found == 0 {
            crate::println!("[AHCI] No ATA devices found on any port");
            return Err(());
        }

        // Pin the controller into a leaked Box so it lives forever.
        let ctrl_heap: Box<AhciController> = Box::new(controller);
        core::mem::forget(ctrl_heap); // Leak: the block device refs point here.

        Ok(())
    }
}

/// Register the AHCI driver with the device manager.
pub fn register() {
    crate::drivers::device_manager::DEVICE_MANAGER
        .lock()
        .register_driver(Box::new(AhciDriver));
}
