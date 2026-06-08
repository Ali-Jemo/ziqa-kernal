/// AArch64 Generic Timer driver
/// Ported from Redox OS.

use core::ptr::{read_volatile, write_volatile};

const CNTFRQ: u64 = 0;
const CNTPCT: u64 = 0;
const CNTP_CVAL: u64 = 0;
const CNTP_CTL: u64 = 0;
const CNTV_CVAL: u64 = 0;
const CNTV_CTL: u64 = 0;

pub struct GenericTimer;

impl GenericTimer {
    pub fn new() -> Self {
        Self
    }

    pub fn init(&mut self) {
        // Timer is initialized by reading cntfrq_el0
        let _freq: u64;
        unsafe { core::arch::asm!("mrs {}, cntfrq_el0", out(reg) _freq) };
    }

    pub fn read_count(&self) -> u64 {
        let count: u64;
        unsafe { core::arch::asm!("mrs {}, cntpct_el0", out(reg) count) };
        count
    }

    pub fn set_timer(&self, value: u64) {
        unsafe {
            core::arch::asm!("msr cntp_tval_el0, {}", in(reg) value);
            // Enable timer, mask interrupt
            core::arch::asm!("msr cntp_ctl_el0, {}", in(reg) 1u64);
        }
    }

    pub fn clear_timer(&self) {
        unsafe {
            core::arch::asm!("msr cntp_ctl_el0, {}", in(reg) 0u64);
        }
    }
}
