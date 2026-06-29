//! USB HID boot-protocol report decoding for keyboard and mouse.

use super::regs::*;

/// USB HID boot keyboard usage -> ASCII (no shift). 0 = no mapping.
const HID_KEY_TO_ASCII: [u8; 128] = {
    let mut table = [0u8; 128];
    let mut i = 0u8;
    while i < 26 {
        table[(0x04 + i) as usize] = b'a' + i;
        i += 1;
    }
    let mut d = 0u8;
    while d < 10 {
        table[(0x1E + d) as usize] = b'1' + d;
        d += 1;
    }
    table[0x2C as usize] = b' ';
    table[0x2D as usize] = b'-';
    table[0x2E as usize] = b'=';
    table[0x2F as usize] = b'[';
    table[0x30 as usize] = b']';
    table[0x31 as usize] = b'\\';
    table[0x33 as usize] = b';';
    table[0x34 as usize] = b'\'';
    table[0x35 as usize] = b'`';
    table[0x36 as usize] = b',';
    table[0x37 as usize] = b'.';
    table[0x38 as usize] = b'/';
    table[0x28 as usize] = b'\n';
    table
};

const HID_SHIFTED: [u8; 128] = {
    let mut table = [0u8; 128];
    let mut i = 0u8;
    while i < 26 {
        table[(0x04 + i) as usize] = b'A' + i;
        i += 1;
    }
    table
};

pub fn handle_keyboard_report(report: &[u8]) {
    if report.len() < 8 {
        return;
    }
    let modifiers = report[0];
    let shift = modifiers & 0x22 != 0; // L or R shift
    for &key in &report[2..8] {
        if key == 0 || key >= 128 {
            continue;
        }
        let ch = if shift {
            HID_SHIFTED[key as usize]
        } else {
            HID_KEY_TO_ASCII[key as usize]
        };
        if ch != 0 {
            crate::drivers::keyboard::push_raw_byte(ch);
        }
    }
}

pub fn handle_mouse_report(report: &[u8]) {
    if report.len() < 3 {
        return;
    }
    let buttons = report[0];
    let dx = report[1] as i8;
    let dy = report[2] as i8;
    crate::drivers::ps2_mouse::apply_usb_report(buttons, dx, dy);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HidKind {
    Keyboard,
    Mouse,
    Unknown,
}

pub fn classify_interface(class: u8, protocol: u8) -> HidKind {
    if class != USB_CLASS_HID {
        return HidKind::Unknown;
    }
    match protocol {
        USB_PROTO_KEYBOARD => HidKind::Keyboard,
        USB_PROTO_MOUSE => HidKind::Mouse,
        _ => HidKind::Unknown,
    }
}

/// Parse config descriptor bytes for the first HID interrupt-IN endpoint.
pub fn find_hid_interrupt_ep(
    config: &[u8],
    kind: HidKind,
) -> Option<(u8, u16, u8)> {
    // (endpoint_address, max_packet, interval)
    let mut i = 0usize;
    let mut in_hid_iface = false;
    while i + 2 <= config.len() {
        let len = config[i] as usize;
        if len < 2 || i + len > config.len() {
            break;
        }
        let desc_type = config[i + 1];
        if desc_type == 0x04 {
            // Interface
            let class = config[i + 5];
            let protocol = config[i + 7];
            in_hid_iface = classify_interface(class, protocol) == kind;
        } else if desc_type == 0x05 && in_hid_iface {
            let addr = config[i + 2];
            let attr = config[i + 3];
            let max_pkt = u16::from_le_bytes([config[i + 4], config[i + 5]]);
            let interval = config[i + 6];
            if attr & 0x03 == 0x03 && addr & 0x80 != 0 {
                return Some((addr, max_pkt, interval));
            }
        }
        i += len;
    }
    None
}

/// Detect the first HID boot keyboard or mouse interface in a config descriptor.
pub fn detect_hid_kind(config: &[u8]) -> Option<HidKind> {
    let mut i = 0usize;
    while i + 2 <= config.len() {
        let len = config[i] as usize;
        if len < 2 || i + len > config.len() {
            break;
        }
        if config[i + 1] == 0x04 {
            let kind = classify_interface(config[i + 5], config[i + 7]);
            if kind != HidKind::Unknown {
                return Some(kind);
            }
        }
        i += len;
    }
    None
}
