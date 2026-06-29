//! xHCI USB Host Controller Driver — STUB
//!
//! The original module was lost due to an editing accident.
//! This stub provides the public API needed for compilation.
//! Full implementation should restore from the subfiles in this directory.

use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

static USB_KEYBOARD_ACTIVE: AtomicBool = AtomicBool::new(false);
static USB_MOUSE_ACTIVE: AtomicBool = AtomicBool::new(false);
pub static USB_STORAGE_COUNT: AtomicU32 = AtomicU32::new(0);

pub fn has_usb_keyboard() -> bool {
    USB_KEYBOARD_ACTIVE.load(Ordering::Relaxed)
}

pub fn has_usb_mouse() -> bool {
    USB_MOUSE_ACTIVE.load(Ordering::Relaxed)
}

pub fn has_usb_storage() -> bool {
    USB_STORAGE_COUNT.load(Ordering::Relaxed) > 0
}

/// Register the xHCI driver with the device manager.
/// Stub — does nothing.
pub fn register() {
    crate::println!(" ~ xHCI (USB) ........................... stub (disabled)");
}

/// Poll USB interrupt transfer events. Called from the input server thread.
/// Stub — does nothing.
pub fn poll() {}
