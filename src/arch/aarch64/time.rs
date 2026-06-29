/// AArch64 monotonic clock via the Generic Timer
/// Ported from Redox OS.

pub fn monotonic_absolute() -> u128 {
    let ticks: u64;
    let freq: u64;
    unsafe {
        core::arch::asm!("mrs {}, cntpct_el0", out(reg) ticks);
        core::arch::asm!("mrs {}, cntfrq_el0", out(reg) freq);
    }
    if freq == 0 {
        return 0;
    }
    (ticks as u128) * 1_000_000_000 / (freq as u128)
}

pub fn monotonic_resolution() -> u128 {
    let freq: u64;
    unsafe {
        core::arch::asm!("mrs {}, cntfrq_el0", out(reg) freq);
    }
    if freq == 0 {
        return 1;
    }
    1_000_000_000 / (freq as u128)
}
