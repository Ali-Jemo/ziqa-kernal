//! Input event bridge — converts kernel input events into `orbclient::Event`
//! format for the Orbital compositor's input consumer.
//!
//! The keyboard driver pushes raw scancodes here. The OrbitalBridge drains
//! them when Orbital's `ConsumerHandle::read_events()` reads from the
//! `input:` scheme.

use alloc::collections::VecDeque;
use spin::Mutex;

/// orbclient `Event` struct — what `ConsumerHandle::read_events()` expects.
///
/// Layout matches `orbclient::Event`:
/// ```rust
/// #[repr(packed)]
/// pub struct Event { pub code: i64, pub a: i64, pub b: i64 }
/// ```
#[derive(Copy, Clone, Debug, Default)]
#[repr(packed)]
pub struct OrbitalEvent {
    pub code: i64,
    pub a: i64,
    pub b: i64,
}

pub const EVENT_KEY: i64 = 1;
pub const EVENT_MOUSE: i64 = 2;
pub const EVENT_BUTTON: i64 = 3;
pub const EVENT_SCROLL: i64 = 4;
pub const EVENT_MOVE: i64 = 7;
pub const EVENT_RESIZE: i64 = 8;

/// Shared ring buffer of pending input events.
static INPUT_EVENTS: Mutex<VecDeque<OrbitalEvent>> = Mutex::new(VecDeque::new());

/// Push a keyboard event into the shared queue.
///
/// * `scancode` — raw scancode (from `KeyCode` enum)
/// * `character` — decoded Unicode character, or `\0` for non-printable
/// * `pressed` — true when key is pressed, false on release
pub fn push_key_event(scancode: u8, character: char, pressed: bool) {
    let mut events = INPUT_EVENTS.lock();
    if events.len() >= 256 {
        events.pop_front();
    }
    events.push_back(OrbitalEvent {
        code: EVENT_KEY,
        a: character as i64,
        b: scancode as i64 | (if pressed { 0x100 } else { 0 }),
    });
}

/// Push a mouse motion event into the shared queue.
pub fn push_mouse_event(x: i32, y: i32) {
    let mut events = INPUT_EVENTS.lock();
    if events.len() >= 256 {
        events.pop_front();
    }
    events.push_back(OrbitalEvent {
        code: EVENT_MOUSE,
        a: x as i64,
        b: y as i64,
    });
}

/// Pop pending events into `buf`, returning the number of events written.
pub fn pop_events(buf: &mut [OrbitalEvent]) -> usize {
    let mut events = INPUT_EVENTS.lock();
    let mut count = 0;
    for dest in buf.iter_mut() {
        match events.pop_front() {
            Some(ev) => {
                *dest = ev;
                count += 1;
            }
            None => break,
        }
    }
    count
}

/// Returns true if there are pending events.
pub fn has_events() -> bool {
    !INPUT_EVENTS.lock().is_empty()
}

/// Push a mouse motion event with screen-pixel coordinates.
///
/// `x`/`y` are already in screen-pixel space: the PS/2 driver clamps the
/// cursor to the BGA framebuffer dimensions before pushing, so the values are
/// forwarded to `orbclient::MouseEvent` unchanged (no HID 0..65535 scaling).
pub fn push_mouse_screen_event(x: i32, y: i32) {
    let mut events = INPUT_EVENTS.lock();
    if events.len() >= 256 {
        events.pop_front();
    }
    events.push_back(OrbitalEvent {
        code: EVENT_MOUSE,
        a: x as i64,
        b: y as i64,
    });
}
/// Push a mouse button event into the shared queue.
///
/// `button_mask` is the current button state bitmask (left=1, right=4, middle=2).
/// This matches `orbclient::ButtonEvent::from_event` which reads the bitmask from `event.a`.
pub fn push_mouse_button_event(button_mask: u8) {
    let mut events = INPUT_EVENTS.lock();
    if events.len() >= 256 {
        events.pop_front();
    }
    events.push_back(OrbitalEvent {
        code: EVENT_BUTTON,
        a: button_mask as i64,
        b: 0,
    });
}
