//! USB Mass Storage Bulk-Only Transport (BOT) protocol helpers.
//!
//! Provides the CBW/CSW transfer primitive and SCSI command wrappers.
//! The actual xHCI ring/doorbell operations are done via the global
//! CONTROLLER in `super::mod.rs`.

use super::regs::*;

// ── BOT constants ──────────────────────────────────────────────────────────

pub const CBW_SIGNATURE: u32 = 0x43425355; // "USBC"
pub const CSW_SIGNATURE: u32 = 0x53425355; // "USBS"
pub const CBW_SIZE: u32 = 31;
pub const CSW_SIZE: u32 = 13;

// ── SCSI opcodes ───────────────────────────────────────────────────────────

pub const SCSI_INQUIRY: u8 = 0x12;
pub const SCSI_TEST_UNIT_READY: u8 = 0x00;
pub const SCSI_READ_CAPACITY_10: u8 = 0x25;
pub const SCSI_READ_10: u8 = 0x28;
pub const SCSI_WRITE_10: u8 = 0x2A;

// ── CBW / CSW structures (packed for DMA) ──────────────────────────────────

#[repr(C, packed)]
pub struct Cbw {
    pub signature: u32,
    pub tag: u32,
    pub data_length: u32,
    pub flags: u8,
    pub lun: u8,
    pub cdb_len: u8,
    pub cdb: [u8; 16],
}

#[repr(C, packed)]
pub struct Csw {
    pub signature: u32,
    pub tag: u32,
    pub data_residue: u32,
    pub status: u8,
}

// ── Config descriptor parser ───────────────────────────────────────────────

/// Parse a USB configuration descriptor for a Mass Storage BOT interface.
/// Returns `(ep_out_addr, ep_in_addr, max_pkt_out, max_pkt_in)` on success.
pub fn find_bulk_eps(cfg: &[u8]) -> Option<(u8, u8, u16, u16)> {
    let mut pos = 0usize;
    while pos + 2 < cfg.len() {
        let len = cfg[pos] as usize;
        if len == 0 {
            break;
        }
        let desc_type = cfg[pos + 1];
        if desc_type == 0x04 // DESC_INTERFACE
            && pos + 9 <= cfg.len()
            && cfg[pos + 5] == USB_CLASS_MASS_STORAGE
            && cfg[pos + 6] == USB_SUBCLASS_SCSI
            && cfg[pos + 7] == USB_PROTO_BOT
        {
            // Found mass storage interface — scan sub-descriptors
            let mut ep_out = None;
            let mut ep_in = None;
            let mut mps_out = 0u16;
            let mut mps_in = 0u16;
            let mut sub = pos + 9;
            while sub + 7 <= cfg.len() && cfg[sub] >= 7 {
                let slen = cfg[sub] as usize;
                if cfg[sub + 1] == 0x05 // DESC_ENDPOINT
                    && (cfg[sub + 3] & 0x03) == 0x02 // EP_TYPE_BULK
                {
                    let ep_addr = cfg[sub + 2];
                    let mps = u16::from_le_bytes([cfg[sub + 4], cfg[sub + 5]]);
                    if ep_addr & 0x80 != 0 {
                        ep_in = Some(ep_addr);
                        mps_in = mps;
                    } else {
                        ep_out = Some(ep_addr);
                        mps_out = mps;
                    }
                }
                sub += slen;
            }
            if let (Some(out), Some(in_)) = (ep_out, ep_in) {
                return Some((out, in_, mps_out, mps_in));
            }
        }
        pos += len;
    }
    None
}
