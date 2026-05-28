/// Keyboard input ring buffer for ZiqaKernel
///
/// The keyboard ISR pushes raw scancodes here.
/// sys_read(stdin) drains decoded characters from this buffer.

use spin::Mutex;
use pc_keyboard::{layouts, DecodedKey, HandleControl, Keyboard, ScancodeSet1};
use lazy_static::lazy_static;

const BUF_CAP: usize = 256;

struct RingBuf {
    buf: [u8; BUF_CAP],
    head: usize,
    tail: usize,
    count: usize,
}

impl RingBuf {
    const fn new() -> Self {
        Self { buf: [0; BUF_CAP], head: 0, tail: 0, count: 0 }
    }

    fn push(&mut self, byte: u8) {
        if self.count < BUF_CAP {
            self.buf[self.tail] = byte;
            self.tail = (self.tail + 1) % BUF_CAP;
            self.count += 1;
        }
        // Drop oldest if full (overwrite)
    }

    fn pop(&mut self) -> Option<u8> {
        if self.count == 0 { return None; }
        let b = self.buf[self.head];
        self.head = (self.head + 1) % BUF_CAP;
        self.count -= 1;
        Some(b)
    }

    fn is_empty(&self) -> bool { self.count == 0 }
}

static INPUT_BUF: Mutex<RingBuf> = Mutex::new(RingBuf::new());

lazy_static! {
    static ref KB: Mutex<Keyboard<layouts::Us104Key, ScancodeSet1>> =
        Mutex::new(Keyboard::new(
            ScancodeSet1::new(),
            layouts::Us104Key,
            HandleControl::Ignore,
        ));
}

/// Called from the keyboard ISR with the raw scancode.
pub fn push_scancode(scancode: u8) {
    let mut kb = KB.lock();
    if let Ok(Some(key_event)) = kb.add_byte(scancode) {
        if let Some(key) = kb.process_keyevent(key_event) {
            match key {
                DecodedKey::Unicode(c) => {
                    // Echo to serial
                    crate::print!("{}", c);
                    // Store in ring buffer (UTF-8, only ASCII range for now)
                    if c.is_ascii() {
                        INPUT_BUF.lock().push(c as u8);
                    }
                }
                DecodedKey::RawKey(_) => {}
            }
        }
    }
}

/// Read up to `buf.len()` bytes from the keyboard buffer.
/// Returns number of bytes read (0 = no input available).
pub fn read_stdin(buf: &mut [u8]) -> usize {
    let mut ring = INPUT_BUF.lock();
    let mut n = 0;
    while n < buf.len() {
        match ring.pop() {
            Some(b) => { buf[n] = b; n += 1; }
            None => break,
        }
    }
    n
}

/// Clear all pending input in the keyboard buffer.
pub fn clear_stdin() {
    let mut ring = INPUT_BUF.lock();
    ring.head = 0;
    ring.tail = 0;
    ring.count = 0;
}


/// Returns true if there is pending keyboard input.
pub fn has_input() -> bool {
    !INPUT_BUF.lock().is_empty()
}
