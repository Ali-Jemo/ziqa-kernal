use lazy_static::lazy_static;
use pc_keyboard::{layouts, DecodedKey, HandleControl, KeyCode, Keyboard, ScancodeSet1};
/// Keyboard input ring buffer for ZiqaKernel
///
/// The keyboard ISR pushes raw scancodes here.
/// sys_read(stdin) drains decoded characters from this buffer.
use spin::Mutex;
use core::sync::atomic::{AtomicU16, Ordering};

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

/// Global flag for Ctrl+C interruption.
/// Set when 0x03 is received, cleared by the consumer.
pub static CTRL_C_PRESSED: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

/// Last decoded key available for compositor polling.
/// 0 = no pending event; bit 8 set = Unicode; bits 0-7 = key data.
/// Written from the keyboard ISR, read+cleared by compositor thread.
pub static COMPOSITOR_LAST_KEY: AtomicU16 = AtomicU16::new(0);

lazy_static! {
    static ref KB: Mutex<Keyboard<layouts::Us104Key, ScancodeSet1>> = Mutex::new(Keyboard::new(
        ScancodeSet1::new(),
        layouts::Us104Key,
        HandleControl::Ignore,
    ));
}

fn wait_controller_write_ready() -> bool {
    let mut status = x86_64::instructions::port::Port::<u8>::new(0x64);
    for _ in 0..1_000_000 {
        if unsafe { status.read() } & 0x02 == 0 {
            return true;
        }
        core::hint::spin_loop();
    }
    false
}

fn wait_controller_read_ready() -> bool {
    let mut status = x86_64::instructions::port::Port::<u8>::new(0x64);
    for _ in 0..1_000_000 {
        if unsafe { status.read() } & 0x01 != 0 {
            return true;
        }
        core::hint::spin_loop();
    }
    false
}

fn write_controller_command(command: u8) -> bool {
    if !wait_controller_write_ready() {
        return false;
    }
    unsafe {
        x86_64::instructions::port::Port::<u8>::new(0x64).write(command);
    }
    true
}

fn write_controller_data(data: u8) -> bool {
    if !wait_controller_write_ready() {
        return false;
    }
    unsafe {
        x86_64::instructions::port::Port::<u8>::new(0x60).write(data);
    }
    true
}

fn read_controller_data() -> Option<u8> {
    if !wait_controller_read_ready() {
        return None;
    }
    Some(unsafe { x86_64::instructions::port::Port::<u8>::new(0x60).read() })
}

fn flush_controller_output() {
    let mut status = x86_64::instructions::port::Port::<u8>::new(0x64);
    let mut data = x86_64::instructions::port::Port::<u8>::new(0x60);
    for _ in 0..32 {
        if unsafe { status.read() } & 0x01 == 0 {
            break;
        }
        let _ = unsafe { data.read() };
    }
}

/// Enable the first PS/2 port and keyboard scanning.
///
/// QEMU's graphical window delivers normal key presses through the i8042
/// keyboard device. Mouse setup only enables the auxiliary IRQ, so initialize
/// the keyboard port explicitly before the shell starts polling stdin.
///
/// The PS/2 controller port enable (0xAE) triggers a BAT (Basic Assurance
/// Test) on the keyboard device, which sends result 0xAA (passed) to the
/// output buffer. We must wait for and drain this result BEFORE sending the
/// 0xF4 (enable scanning) command, otherwise read_controller_data() after
/// 0xF4 would consume the BAT result instead of the ACK (0xFA).
pub fn init() {
    let _ = write_controller_command(0xAD); // Disable first PS/2 port while editing config.
    flush_controller_output();

    let mut config = if write_controller_command(0x20) {
        read_controller_data().unwrap_or(0)
    } else {
        0
    };
    config &= !0x01; // IRQ1 disabled; shell/readers poll the controller directly.
    config |= 0x40; // Translate device scancodes to set 1 for pc-keyboard.
    config &= !0x10; // First PS/2 port clock enabled.

    if !write_controller_command(0x60) || !write_controller_data(config) {
        crate::println!(" ~ PS/2 Keyboard ........................ init timed out");
        return;
    }

    let _ = write_controller_command(0xAE); // Enable first PS/2 port.
    flush_controller_output();

    // Enable keyboard scanning
    write_controller_data(0xF4);
    
    // Poll the output-buffer bit directly here. Calling read_controller_data()
    // in this loop would perform a full timeout wait on every iteration when
    // no ACK arrives, stretching boot into a long apparent hang.
    let mut status = x86_64::instructions::port::Port::<u8>::new(0x64);
    let mut data = x86_64::instructions::port::Port::<u8>::new(0x60);
    for _ in 0..100_000 {
        if unsafe { status.read() } & 0x01 != 0 {
            if unsafe { data.read() } == 0xFA {
                break;
            }
        }
        core::hint::spin_loop();
    }

    crate::println!(" ~ PS/2 Keyboard ........................ ready");
}

fn push_to_buffers(b: u8) {
    if b == 0x03 {
        CTRL_C_PRESSED.store(true, Ordering::Release);
    }
    INPUT_BUF.lock().push(b);
    EDITOR_BUF.lock().push(b);
}

/// Called from the keyboard ISR with the raw scancode.
pub fn push_scancode(scancode: u8) {
    let mut kb = KB.lock();
    if let Ok(Some(key_event)) = kb.add_byte(scancode) {
        if let Some(key) = kb.process_keyevent(key_event) {
            match key {
                DecodedKey::Unicode(c) => {
                    if c.is_ascii() {
                        let b = c as u8;
                        push_to_buffers(b);
                        // Notify compositor (ISR-safe atomic store)
                        COMPOSITOR_LAST_KEY.store(b as u16 | 0x100, Ordering::Release);
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
                    // Notify compositor (ISR-safe atomic store)
                    COMPOSITOR_LAST_KEY.store(code as u16, Ordering::Release);
                }
            }
        }
    }
}

/// Poll PS/2 controller for pending keyboard scancodes.
///
/// QEMU GUI input is level-triggered through the i8042 output buffer.  Polling
/// from read_stdin avoids depending on keyboard IRQ delivery and keeps GUI input
/// aligned with serial input: bytes are consumed only when a reader is active.
fn poll_ps2_keyboard() {
    unsafe {
        use x86_64::instructions::port::Port;
        let mut status: Port<u8> = Port::new(0x64);
        let mut data: Port<u8> = Port::new(0x60);
        for _ in 0..32 {
            let s = status.read();
            if s & 1 == 0 {
                break;
            }

            let byte = data.read();
            if s & 0x20 != 0 {
                continue;
            }

            push_scancode(byte);
        }
    }
}

/// Read up to `buf.len()` bytes from the keyboard buffer.
/// Returns number of bytes read (0 = no input available).
pub fn read_stdin(buf: &mut [u8]) -> usize {
    x86_64::instructions::interrupts::without_interrupts(|| {
        // Poll serial port (COM1) for any incoming bytes using the global Mutex to avoid conflicts.
        {
            let mut _serial = crate::drivers::uart::SERIAL1.lock();
            // Check if data is available (Bit 0 of Line Status Register)
            unsafe {
                use x86_64::instructions::port::Port;
                let mut lsr: Port<u8> = Port::new(0x3FD);
                let mut rbr: Port<u8> = Port::new(0x3F8);
                while lsr.read() & 1 != 0 {
                    let byte = rbr.read();
                    if byte == b'\r' {
                        push_to_buffers(b'\n');
                    } else {
                        push_to_buffers(byte);
                    }
                }
            }
        }

        poll_ps2_keyboard();
    });

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

/// Check and clear the global Ctrl+C interrupt flag.
pub fn check_and_clear_interrupt() -> bool {
    CTRL_C_PRESSED.swap(false, Ordering::Acquire)
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

/// Push a raw byte into the input buffers (used by userspace keyboard driver)
pub fn push_raw_byte(b: u8) {
    if *ECHO_ENABLED.lock() {
        if (b >= b' ' && b <= 126) || b == b'\n' || b == b'\r' {
            crate::print!("{}", b as char);
        }
    }
    INPUT_BUF.lock().push(b);
    EDITOR_BUF.lock().push(b);
}

/// Read and clear the last compositor-relevant key event.
/// Returns 0 if no event pending, or a packed key value:
/// - bit 8 set  = Unicode character (bits 0-7 = ASCII)
/// - bit 8 clear = Raw key code (0x80-0x88)
/// Safe to call from any context (uses atomic swap).
pub fn poll_compositor_key() -> u16 {
    COMPOSITOR_LAST_KEY.swap(0, Ordering::Acquire)
}
