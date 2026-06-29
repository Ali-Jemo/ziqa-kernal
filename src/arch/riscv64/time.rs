/// RISC-V 64 monotonic clock via rdtime
/// Ported from Redox OS.

use core::sync::atomic::{AtomicUsize, Ordering};

static MTIME_FREQ_HZ: AtomicUsize = AtomicUsize::new(0);

pub fn init(freq_hz: usize) {
    MTIME_FREQ_HZ.store(freq_hz, Ordering::Relaxed);
}

pub fn monotonic_absolute() -> u128 {
    let freq_hz = MTIME_FREQ_HZ.load(Ordering::Relaxed);
    if freq_hz > 0 {
        let counter: u64;
        unsafe { core::arch::asm!("rdtime {0}", out(reg) counter) };
        counter as u128 * 1_000_000_000 / freq_hz as u128
    } else {
        0
    }
}

pub fn monotonic_resolution() -> u128 {
    let freq_hz = MTIME_FREQ_HZ.load(Ordering::Relaxed);
    if freq_hz > 0 {
        1_000_000_000 / freq_hz as u128
    } else {
        1
    }
}
