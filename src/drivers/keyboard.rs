use lazy_static::lazy_static;
use pc_keyboard::{layouts, DecodedKey, HandleControl, KeyCode, Keyboard, ScancodeSet1};
/// Keyboard input ring buffer for ZiqaKernel
///
/// The keyboard ISR pushes raw scancodes here.
/// sys_read(stdin) drains decoded characters from this buffer.
use spin::Mutex;
use core::sync::atomic::{AtomicU32, AtomicU8, Ordering};

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

/// Packed key event for compositor polling.
/// Bits 0-7:   KeyCode variant as u8 (0 = no pending event)
/// Bits 8-15:  Modifier state (MOD_SHIFT | MOD_CTRL | MOD_ALT | MOD_SUPER)
/// Bits 16-31: Decoded payload (Unicode char | 0x100, or raw key code; 0 = none)
pub static COMPOSITOR_KEY_EVENT: AtomicU32 = AtomicU32::new(0);

pub const MOD_SHIFT: u8 = 1;
pub const MOD_CTRL:  u8 = 2;
pub const MOD_ALT:   u8 = 4;
pub const MOD_SUPER: u8 = 8;

/// Current modifier state — updated by ISR, peeked (not consumed) by compositor.
pub static COMPOSITOR_MODS: AtomicU8 = AtomicU8::new(0);

lazy_static! {
    static ref KB: Mutex<Keyboard<layouts::Us104Key, ScancodeSet1>> = Mutex::new(Keyboard::new(
        ScancodeSet1::new(),
        layouts::Us104Key,
        HandleControl::Ignore,
    ));
}

#[inline]
fn route_ps2_to_kernel_shell() -> bool {
    !cfg!(feature = "orbital")
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
    if cfg!(feature = "orbital") {
        config |= 0x01; // IRQ1 enabled: GUI PS/2 keys go to Orbital input:.
    } else {
        config &= !0x01; // Text shell path polls PS/2 from read_stdin().
    }
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

fn store_compositor_key(keycode: u8, payload: u16) {
    let mods = COMPOSITOR_MODS.load(Ordering::Relaxed);
    let packed = (keycode as u32) | ((mods as u32) << 8) | ((payload as u32) << 16);
    COMPOSITOR_KEY_EVENT.store(packed, Ordering::Release);
}

/// Called from the keyboard ISR with the raw scancode.
pub fn push_scancode(scancode: u8) {
    let mut kb = KB.lock();
    if let Ok(Some(key_event)) = kb.add_byte(scancode) {
        let pressed = key_event.state == pc_keyboard::KeyState::Down;
        let key_code = key_event.code;
        // Update modifier state
        {
            let bit = match key_code {
                pc_keyboard::KeyCode::LShift | pc_keyboard::KeyCode::RShift => Some(MOD_SHIFT),
                pc_keyboard::KeyCode::LControl | pc_keyboard::KeyCode::RControl => Some(MOD_CTRL),
                pc_keyboard::KeyCode::LAlt | pc_keyboard::KeyCode::RAltGr => Some(MOD_ALT),
                pc_keyboard::KeyCode::LWin | pc_keyboard::KeyCode::RWin => Some(MOD_SUPER),
                _ => None,
            };
            if let Some(bit) = bit {
                let mods = COMPOSITOR_MODS.load(Ordering::Relaxed);
                let new = if pressed { mods | bit } else { mods & !bit };
                COMPOSITOR_MODS.store(new, Ordering::Release);
            }
        }

        // For orbclient format, scancode is always the make scancode (bit 7 = release flag)
        let make_scancode = scancode & 0x7F;
        if let Some(key) = kb.process_keyevent(key_event) {
            match key {
                DecodedKey::Unicode(c) => {
                    if route_ps2_to_kernel_shell() {
                        if c.is_ascii() {
                            push_to_buffers(c as u8);
                            store_compositor_key(key_code as u8, c as u8 as u16 | 0x100);
                        } else {
                            // Non-ASCII: still store keycode for keybinding resolution, no payload
                            store_compositor_key(key_code as u8, 0);
                        }
                    }
                    crate::scheme::input_bridge::push_key_event(make_scancode, c, pressed);
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
                        _ => {
                            // Non-arrow RawKey: store keycode for keybinding resolution, no payload
                            if route_ps2_to_kernel_shell() {
                                store_compositor_key(key_code as u8, 0);
                            }
                            crate::scheme::input_bridge::push_key_event(make_scancode, '\0', pressed);
                            return;
                        }
                    };
                    if route_ps2_to_kernel_shell() {
                        if code >= 0x80 && code <= 0x83 || code == 0x86 || code == 0x87 {
                            INPUT_BUF.lock().push(code);
                        }
                        EDITOR_BUF.lock().push(code);
                        store_compositor_key(key_code as u8, code as u16);
                    }
                    crate::scheme::input_bridge::push_key_event(make_scancode, '\0', pressed);
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

        if route_ps2_to_kernel_shell() {
            poll_ps2_keyboard();
        }
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

/// Atomically read and clear the pending key event.
/// Returns 0 if no event pending.
pub fn poll_compositor_key() -> u32 {
    COMPOSITOR_KEY_EVENT.swap(0, Ordering::AcqRel)
}

/// Peek current modifier state (does not clear).
pub fn get_compositor_mods() -> u8 {
    COMPOSITOR_MODS.load(Ordering::Acquire)
}
