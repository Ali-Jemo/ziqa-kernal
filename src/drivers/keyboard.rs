use lazy_static::lazy_static;
use pc_keyboard::{layouts, DecodedKey, HandleControl, KeyCode, Keyboard, ScancodeSet1};
/// Keyboard input ring buffer for ZiqaKernel
///
/// The keyboard ISR pushes raw scancodes here.
/// sys_read(stdin) drains decoded characters from this buffer.
use spin::Mutex;

const BUF_CAP: usize = 256;

struct RingBuf {
    buf: [u8; BUF_CAP],
    head: usize,
    tail: usize,
    count: usize,
}

impl RingBuf {
    const fn new() -> Self {
        Self {
            buf: [0; BUF_CAP],
            head: 0,
            tail: 0,
            count: 0,
        }
    }

    fn push(&mut self, byte: u8) {
        if self.count < BUF_CAP {
            self.buf[self.tail] = byte;
            self.tail = (self.tail + 1) % BUF_CAP;
            self.count += 1;
        }
    }

    fn pop(&mut self) -> Option<u8> {
        if self.count == 0 {
            return None;
        }
        let b = self.buf[self.head];
        self.head = (self.head + 1) % BUF_CAP;
        self.count -= 1;
        Some(b)
    }

    fn is_empty(&self) -> bool {
        self.count == 0
    }
}

static INPUT_BUF: Mutex<RingBuf> = Mutex::new(RingBuf::new());
static EDITOR_BUF: Mutex<RingBuf> = Mutex::new(RingBuf::new());
static ECHO_ENABLED: Mutex<bool> = Mutex::new(true);

lazy_static! {
    static ref KB: Mutex<Keyboard<layouts::Us104Key, ScancodeSet1>> = Mutex::new(Keyboard::new(
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
                    if *ECHO_ENABLED.lock() {
                        if c >= ' ' || c == '\n' || c == '\r' {
                            crate::print!("{}", c);
                        }
                    }
                    if c.is_ascii() {
                        let b = c as u8;
                        INPUT_BUF.lock().push(b);
                        EDITOR_BUF.lock().push(b);
                    }
                }
                DecodedKey::RawKey(k) => {
                    let code: u8 = match k {
                        KeyCode::ArrowUp => 0x80,
                        KeyCode::ArrowDown => 0x81,
                        KeyCode::ArrowLeft => 0x82,
                        KeyCode::ArrowRight => 0x83,
                        KeyCode::Home => 0x84,
                        KeyCode::End => 0x85,
                        KeyCode::PageUp => 0x86,
                        KeyCode::PageDown => 0x87,
                        KeyCode::Delete => 0x88,
                        _ => return,
                    };
                    // Pass navigation keys to shell for input handling
                    if code >= 0x80 && code <= 0x83 || code == 0x86 || code == 0x87 {
                        INPUT_BUF.lock().push(code);
                    }
                    EDITOR_BUF.lock().push(code);
                }
            }
        }
    }
}

/// Read up to `buf.len()` bytes from the keyboard buffer.
/// Returns number of bytes read (0 = no input available).
pub fn read_stdin(buf: &mut [u8]) -> usize {
    // Poll serial port (COM1) for any incoming bytes
    unsafe {
        use x86_64::instructions::port::Port;
        let mut lsr: Port<u8> = Port::new(0x3FD); // Line Status Register
        let mut rbr: Port<u8> = Port::new(0x3F8); // Receiver Buffer
        while lsr.read() & 1 != 0 {
            let byte = rbr.read();
            if byte == b'\r' {
                INPUT_BUF.lock().push(b'\n');
            } else {
                INPUT_BUF.lock().push(byte);
            }
        }
    }
    let mut ring = INPUT_BUF.lock();
    let mut n = 0;
    while n < buf.len() {
        match ring.pop() {
            Some(b) => {
                buf[n] = b;
                n += 1;
            }
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

/// Read a byte from the editor input buffer
pub fn read_editor_byte(buf: &mut [u8]) -> usize {
    let mut ring = EDITOR_BUF.lock();
    let mut n = 0;
    while n < buf.len() {
        match ring.pop() {
            Some(b) => {
                buf[n] = b;
                n += 1;
            }
            None => break,
        }
    }
    n
}

/// Clear all pending editor input
pub fn clear_editor_buf() {
    let mut ring = EDITOR_BUF.lock();
    ring.head = 0;
    ring.tail = 0;
    ring.count = 0;
}

/// Enable or disable character echo (used by editor)
pub fn set_echo(enabled: bool) {
    *ECHO_ENABLED.lock() = enabled;
}
