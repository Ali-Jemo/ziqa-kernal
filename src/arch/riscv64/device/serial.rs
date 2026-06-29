/// RISC-V 64 UART serial driver (NS16550A)
/// Ported from Redox OS.

use crate::drivers::uart::Uart;
use core::ptr::{read_volatile, write_volatile};

const RBR: u32 = 0x000;
const THR: u32 = 0x000;
const IER: u32 = 0x001;
const IIR: u32 = 0x002;
const FCR: u32 = 0x002;
const LCR: u32 = 0x003;
const MCR: u32 = 0x004;
const LSR: u32 = 0x005;
const MSR: u32 = 0x006;

const LSR_THRE: u8 = 1 << 5;
const LSR_DR: u8 = 1 << 0;

pub struct Ns16550Uart {
    base: usize,
}

impl Ns16550Uart {
    pub fn new(base: usize) -> Self {
        Self { base }
    }

    pub fn init(&mut self) {
        let base = self.base;
        unsafe {
            // Set DLAB
            write_volatile((base + LCR as usize) as *mut u8, 0x80);
            // Set baud rate (assuming 1.8432MHz clock, 115200)
            write_volatile((base + 0x000) as *mut u8, 1); // DLL
            write_volatile((base + 0x001) as *mut u8, 0); // DLM
            // Clear DLAB, set 8-bit, 1 stop, no parity
            write_volatile((base + LCR as usize) as *mut u8, 0x03);
            // Enable FIFO
            write_volatile((base + FCR as usize) as *mut u8, 0x07);
            // Enable interrupts
            write_volatile((base + IER as usize) as *mut u8, 0x01);
        }
    }
}

impl Uart for Ns16550Uart {
    fn put_char(&mut self, c: u8) {
        let base = self.base;
        loop {
            let lsr = unsafe { read_volatile((base + LSR as usize) as *const u8) };
            if lsr & LSR_THRE != 0 {
                break;
            }
        }
        unsafe { write_volatile((base + THR as usize) as *mut u8, c) };
    }

    fn get_char(&mut self) -> Option<u8> {
        let base = self.base;
        let lsr = unsafe { read_volatile((base + LSR as usize) as *const u8) };
        if lsr & LSR_DR != 0 {
            Some(unsafe { read_volatile((base + RBR as usize) as *const u8) })
        } else {
            None
        }
    }
}
