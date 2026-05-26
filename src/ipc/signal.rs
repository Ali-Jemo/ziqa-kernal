/// Signal System for ZiqaKernel
///
/// High-speed asynchronous notifications between processes.
/// Used for process control (SIGKILL) and custom IPC (SIGUSR).


#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signal {
    Kill = 9,
    Stop = 19,
    Continue = 18,
    Usr1 = 10,
    Usr2 = 12,
}

/// Signal queue for a process
pub struct SignalQueue {
    pending: u32, // Bitmask of pending signals
}

impl SignalQueue {
    pub const fn new() -> Self {
        Self { pending: 0 }
    }

    pub fn push(&mut self, signal: Signal) {
        self.pending |= 1 << (signal as u8);
    }

    pub fn pop(&mut self) -> Option<Signal> {
        if self.pending == 0 { return None; }
        
        for i in 0..32 {
            if (self.pending >> i) & 1 != 0 {
                self.pending &= !(1 << i);
                // In a real impl, we'd map bit index back to enum
                return Some(Signal::Kill); 
            }
        }
        None
    }
}
