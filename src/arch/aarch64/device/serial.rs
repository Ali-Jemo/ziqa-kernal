/// AArch64 UART serial driver (PL011)
/// Ported from Redox OS.

use crate::drivers::uart::Uart;
use core::ptr::{read_volatile, write_volatile};

const DR: u32 = 0x000;
const FR: u32 = 0x018;
const IBRD: u32 = 0x024;
const FBRD: u32 = 0x028;
const LCR_H: u32 = 0x02C;
const CR: u32 = 0x030;
const IMSC: u32 = 0x038;
const MIS: u32 = 0x040;
const ICR: u32 = 0x044;

const FR_TXFF: u16 = 1 << 5;
const FR_RXFE: u16 = 1 << 4;

pub struct Pl011Uart {
    base: usize,
}

impl Pl011Uart {
    pub fn new(base: usize) -> Self {
        Self { base }
    }

    pub fn init(&mut self) {
        unsafe {
            // Disable UART
            write_volatile((self.base + CR as usize) as *mut u32, 0);

            // Set baud rate (assuming 40MHz clock, 115200 baud)
            write_volatile((self.base + IBRD as usize) as *mut u32, 21);
            write_volatile((self.base + FBRD as usize) as *mut u32, 10);

            // Line control: 8-bit, enable FIFO
            write_volatile((self.base + LCR_H as usize) as *mut u32, 0x70);

            // Mask all interrupts
            write_volatile((self.base + IMSC as usize) as *mut u32, 0);

            // Clear any pending interrupts
            write_volatile((self.base + ICR as usize) as *mut u32, 0x7FF);

            // Enable UART, TX, RX
            write_volatile((self.base + CR as usize) as *mut u32, 0x301);
        }
    }

    fn read_reg(&self, reg: u32) -> u32 {
        unsafe { read_volatile((self.base + reg as usize) as *const u32) }
    }

    fn write_reg(&self, reg: u32, val: u32) {
        unsafe { write_volatile((self.base + reg as usize) as *mut u32, val) }
    }
}

impl Uart for Pl011Uart {
    fn put_char(&mut self, c: u8) {
        while self.read_reg(FR) & FR_TXFF as u32 != 0 {}
        self.write_reg(DR, c as u32);
    }

    fn get_char(&mut self) -> Option<u8> {
        if self.read_reg(FR) & FR_RXFE as u32 == 0 {
            Some(self.read_reg(DR) as u8)
        } else {
            None
        }
    }
}
