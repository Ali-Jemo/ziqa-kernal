//! AArch64 Generic Timer driver
//! Based on Redox OS implementation.

use alloc::boxed::Box;

use super::ic_for_chip;
use crate::{
    arch::device::cpu::registers::control_regs,
    context::{self, timeout},
    dtb,
    scheme::irq::irq_trigger,
    sync::CleanLockToken,
    time,
};
use fdt::Fdt;

bitflags::bitflags! {
    struct TimerCtrlFlags: u32 {
        const ENABLE = 1 << 0;
        const IMASK = 1 << 1;
        const ISTATUS = 1 << 2;
    }
}

pub unsafe fn init(fdt: &Fdt) {
    unsafe {
        if let Some(node) = fdt.find_compatible(&["arm,armv8-timer"]) {
            let interrupt = node
                .property("interrupts")
                .and_then(|p| p.as_usize())
                .unwrap_or(0);

            let irq_idx = ic_for_chip(fdt, &node).unwrap_or(0);

            let mut timer = GenericTimer::new();
            timer.init();

            let token = CleanLockToken::new();
            context::timeout::register_timer(irq_idx, interrupt, Box::new(timer), token.token());
        }
    }
}

pub struct GenericTimer {
    pub use_virtual_timer: bool,
    pub clk_freq: u32,
    pub reload_count: u32,
}

impl GenericTimer {
    pub fn new() -> Self {
        Self {
            use_virtual_timer: false,
            clk_freq: 0,
            reload_count: 0,
        }
    }

    pub fn init(&mut self) {
        // Read timer frequency
        self.clk_freq = unsafe { control_regs::cntfrq_el0() };
        if self.clk_freq == 0 {
            self.clk_freq = 24_000_000; // Default 24 MHz
        }
        self.reload_count = self.clk_freq / 1000; // 1ms tick
    }

    fn read_tmr_ctrl(&self) -> TimerCtrlFlags {
        let val: u32;
        if self.use_virtual_timer {
            unsafe { core::arch::asm!("mrs {0:w}, cntv_ctl_el0", out(reg) val) };
        } else {
            unsafe { core::arch::asm!("mrs {0:w}, cntp_ctl_el0", out(reg) val) };
        }
        TimerCtrlFlags::from_bits_truncate(val)
    }

    fn write_tmr_ctrl(&self, ctrl: TimerCtrlFlags) {
        if self.use_virtual_timer {
            unsafe { core::arch::asm!("msr cntv_ctl_el0, {}", in(reg) ctrl.bits()) };
        } else {
            unsafe { core::arch::asm!("msr cntp_ctl_el0, {}", in(reg) ctrl.bits()) };
        }
    }

    #[allow(unused)]
    fn disable(&self) {
        self.write_tmr_ctrl(TimerCtrlFlags::IMASK);
    }

    #[allow(unused)]
    pub fn set_irq(&mut self) {
        let ctrl = self.read_tmr_ctrl();
        self.write_tmr_ctrl(ctrl - TimerCtrlFlags::IMASK);
    }

    pub fn clear_irq(&mut self) {
        self.write_tmr_ctrl(TimerCtrlFlags::IMASK | TimerCtrlFlags::ENABLE);
    }

    pub fn reload_count(&mut self) {
        let now: u64;
        unsafe { core::arch::asm!("mrs {}, cntpct_el0", out(reg) now) };
        let next = now + self.reload_count as u64;
        if self.use_virtual_timer {
            unsafe { core::arch::asm!("msr cntv_cval_el0, {}", in(reg) next) };
        } else {
            unsafe { core::arch::asm!("msr cntp_cval_el0, {}", in(reg) next) };
        }
    }
}

impl crate::scheme::irq::InterruptHandler for GenericTimer {
    fn handle(&mut self, _irq: u32) {
        self.clear_irq();
        self.reload_count();
        time::tick();
    }
}