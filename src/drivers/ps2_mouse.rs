//! PS/2 Mouse Driver for ZiqaKernel
//! 
//! Handles mouse interrupts and maintains (x, y) coordinates for the compositor.

use x86_64::instructions::port::Port;
use spin::Mutex;

static MOUSE_STATE: Mutex<MouseState> = Mutex::new(MouseState::new());

pub struct MouseState {
    pub x: i32,
    pub y: i32,
    pub left_pressed: bool,
    pub right_pressed: bool,
    
    // Internal state
    cycle: u8,
    data: [u8; 3],
}

impl MouseState {
    pub const fn new() -> Self {
        Self {
            x: 400,
            y: 300,
            left_pressed: false,
            right_pressed: false,
            cycle: 0,
            data: [0; 3],
        }
    }
}

pub fn get_mouse_pos() -> (i32, i32) {
    let state = MOUSE_STATE.lock();
    (state.x, state.y)
}

/// Initialize PS/2 Mouse
pub fn init() {
    let mut port_64 = Port::<u8>::new(0x64);
    let mut port_60 = Port::<u8>::new(0x60);

    // Enable Auxiliary Device
    unsafe {
        wait_write();
        port_64.write(0xA8);
        
        // Enable interrupts
        wait_write();
        port_64.write(0x20);
        wait_read();
        let status = port_60.read() | 2;
        wait_write();
        port_64.write(0x60);
        wait_write();
        port_60.write(status);
        
        // Tell mouse to use default settings
        mouse_write(0xF6);
        let _ = mouse_read();
        
        // Enable data reporting
        mouse_write(0xF4);
        let _ = mouse_read();
    }
    
    crate::println!(" ~ PS/2 Mouse ........................... ready");
}

fn wait_read() {
    let mut port = Port::<u8>::new(0x64);
    while (unsafe { port.read() } & 1) == 0 {}
}

fn wait_write() {
    let mut port = Port::<u8>::new(0x64);
    while (unsafe { port.read() } & 2) != 0 {}
}

fn mouse_write(data: u8) {
    let mut port_64 = Port::<u8>::new(0x64);
    let mut port_60 = Port::<u8>::new(0x60);
    wait_write();
    unsafe { port_64.write(0xD4); }
    wait_write();
    unsafe { port_60.write(data); }
}

fn mouse_read() -> u8 {
    let mut port = Port::<u8>::new(0x60);
    wait_read();
    unsafe { port.read() }
}

/// Called from the keyboard/mouse interrupt handler
pub fn on_interrupt() {
    let mut port = Port::<u8>::new(0x60);
    let b = unsafe { port.read() };
    
    let mut state = MOUSE_STATE.lock();
    match state.cycle {
        0 => {
            state.data[0] = b;
            if (b & 0x08) != 0 { state.cycle = 1; } // Check bit 3 is set (alignment)
        }
        1 => {
            state.data[1] = b;
            state.cycle = 2;
        }
        2 => {
            state.data[2] = b;
            state.cycle = 0;
            
            // Process packet
            state.left_pressed = (state.data[0] & 1) != 0;
            state.right_pressed = (state.data[0] & 2) != 0;
            
            let dx = state.data[1] as i32 - ((state.data[0] as i32 << 4) & 0x100);
            let dy = state.data[2] as i32 - ((state.data[0] as i32 << 3) & 0x100);
            
            state.x = (state.x + dx).clamp(0, 1919);
            state.y = (state.y - dy).clamp(0, 1079);
        }
        _ => state.cycle = 0,
    }
}
