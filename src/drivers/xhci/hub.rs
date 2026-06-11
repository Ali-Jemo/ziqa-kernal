//! USB hub support for xHCI.
//!
//! Hub descriptor parsing, port-status query helpers, and interrupt-endpoint
//! discovery for class‑0x09 hub devices.

use super::regs::*;

/// Hub descriptor returned by GET_DESCRIPTOR(type=0x29).
#[repr(C, packed)]
pub struct HubDescriptor {
    pub desc_length: u8,
    pub desc_type: u8,
    pub num_ports: u8,
    pub characteristics: u16,
    pub power_on_to_good: u8,  // ×2 ms
    pub hub_control_current: u8,
}

/// Does the configuration descriptor describe a hub?
pub fn is_hub_device(cfg: &[u8]) -> bool {
    let mut pos = 0usize;
    while pos + 2 < cfg.len() {
        let len = cfg[pos] as usize;
        if len == 0 {
            break;
        }
        if cfg[pos + 1] == 0x04 // DESC_INTERFACE
            && pos + 9 <= cfg.len()
            && cfg[pos + 5] == USB_CLASS_HUB
        {
            return true;
        }
        pos += len;
    }
    false
}

/// Find the status‑change interrupt IN endpoint in a hub's configuration
/// descriptor.  Returns `(endpoint_addr, max_packet, interval)`.
pub fn find_hub_intr_ep(cfg: &[u8]) -> Option<(u8, u16, u8)> {
    let mut pos = 0usize;
    while pos + 2 < cfg.len() {
        let len = cfg[pos] as usize;
        if len == 0 {
            break;
        }
        if cfg[pos + 1] == 0x04 // DESC_INTERFACE
            && pos + 9 <= cfg.len()
            && cfg[pos + 5] == USB_CLASS_HUB
        {
            // Scan sub‑descriptors for interrupt IN
            let mut sub = pos + 9;
            while sub + 7 <= cfg.len() && cfg[sub] >= 7 {
                let slen = cfg[sub] as usize;
                if cfg[sub + 1] == 0x05 // DESC_ENDPOINT
                    && (cfg[sub + 3] & 0x03) == 0x03 // EP_TYPE_INTERRUPT
                {
                    return Some((
                        cfg[sub + 2],
                        u16::from_le_bytes([cfg[sub + 4], cfg[sub + 5]]),
                        cfg[sub + 6],
                    ));
                }
                sub += slen;
            }
            break;
        }
        pos += len;
    }
    None
}

/// Number of bytes needed for the hub status change bitmap.
pub fn hub_bitmap_size(num_ports: u8) -> u32 {
    ((num_ports as u32 + 7) / 8 + 1).max(2)
}
