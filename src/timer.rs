/// Monotonic timer and clock subsystem for ZiqaKernel
///
/// - Global tick counter incremented by the PIT/APIC timer ISR
/// - Uptime in milliseconds (assuming 100 Hz PIT = 10 ms/tick)
/// - Sleep queue: processes can register a wake-up tick

use spin::Mutex;
use crate::process::Pid;

/// PIT configured at 100 Hz → 10 ms per tick
pub const TICKS_PER_SEC: u64 = 100;
pub const MS_PER_TICK:   u64 = 1000 / TICKS_PER_SEC;

const MAX_SLEEPERS: usize = 32;

#[derive(Clone, Copy)]
struct SleepEntry {
    pid:       Pid,
    wake_tick: u64,
}

pub struct Timer {
    ticks: u64,
    sleepers: [Option<SleepEntry>; MAX_SLEEPERS],
    sleeper_count: usize,
}

impl Timer {
    pub const fn new() -> Self {
        const NONE: Option<SleepEntry> = None;
        Self {
            ticks: 0,
            sleepers: [NONE; MAX_SLEEPERS],
            sleeper_count: 0,
        }
    }

    /// Reset timer state (called at kernel init)
    pub fn reset(&mut self) {
        self.ticks = 0;
        for slot in self.sleepers.iter_mut() {
            *slot = None;
        }
        self.sleeper_count = 0;
    }

    /// Called from the timer ISR (every tick)
    pub fn tick(&mut self) {
        self.ticks += 1;
        self.wake_sleepers();
    }

    pub fn ticks(&self) -> u64 { self.ticks }

    /// Uptime in milliseconds
    pub fn uptime_ms(&self) -> u64 { self.ticks * MS_PER_TICK }

    /// Uptime in seconds
    pub fn uptime_secs(&self) -> u64 { self.ticks / TICKS_PER_SEC }

    /// Register a process to be woken after `ms` milliseconds
    pub fn sleep_ms(&mut self, pid: Pid, ms: u64) -> bool {
        if self.sleeper_count >= MAX_SLEEPERS { return false; }
        let wake_tick = self.ticks + (ms + MS_PER_TICK - 1) / MS_PER_TICK;
        for slot in self.sleepers.iter_mut() {
            if slot.is_none() {
                *slot = Some(SleepEntry { pid, wake_tick });
                self.sleeper_count += 1;
                return true;
            }
        }
        false
    }

    /// Wake any processes whose sleep deadline has passed
    fn wake_sleepers(&mut self) {
        for slot in self.sleepers.iter_mut() {
            if let Some(entry) = slot {
                if self.ticks >= entry.wake_tick {
                    // Transition process back to Ready
                    let pid = entry.pid;
                    crate::process::scheduler::with_process_mut(pid, |proc| {
                        if proc.state == crate::process::ProcessState::Blocked {
                            proc.make_ready();
                        }
                    });
                    *slot = None;
                    self.sleeper_count = self.sleeper_count.saturating_sub(1);
                }
            }
        }
    }
}

pub static TIMER: Mutex<Timer> = Mutex::new(Timer::new());

/// Called from the timer interrupt handler
pub fn tick() {
    TIMER.lock().tick();
}

/// Current tick count (lock-free read via brief lock)
pub fn uptime_ticks() -> u64 {
    TIMER.lock().ticks()
}

/// Uptime in milliseconds
pub fn uptime_ms() -> u64 {
    TIMER.lock().uptime_ms()
}

/// Block a process for `ms` milliseconds
pub fn sleep_ms(pid: Pid, ms: u64) {
    // Mark process as Blocked first
    crate::process::scheduler::with_process_mut(pid, |proc| {
        proc.state = crate::process::ProcessState::Blocked;
    });
    TIMER.lock().sleep_ms(pid, ms);
}
