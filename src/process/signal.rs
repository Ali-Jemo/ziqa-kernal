/// POSIX-style signal subsystem for ZiqaKernel
///
/// Supports 32 standard signals (SIGKILL, SIGTERM, SIGCHLD, etc.)
/// Each process has a pending bitmask and a per-signal handler table.

/// Standard signal numbers (POSIX subset)
pub mod sig {
    pub const SIGHUP: u8 = 1;
    pub const SIGINT: u8 = 2;
    pub const SIGQUIT: u8 = 3;
    pub const SIGILL: u8 = 4;
    pub const SIGTRAP: u8 = 5;
    pub const SIGABRT: u8 = 6;
    pub const SIGBUS: u8 = 7;
    pub const SIGFPE: u8 = 8;
    pub const SIGKILL: u8 = 9;
    pub const SIGUSR1: u8 = 10;
    pub const SIGSEGV: u8 = 11;
    pub const SIGUSR2: u8 = 12;
    pub const SIGPIPE: u8 = 13;
    pub const SIGALRM: u8 = 14;
    pub const SIGTERM: u8 = 15;
    pub const SIGCHLD: u8 = 17;
    pub const SIGCONT: u8 = 18;
    pub const SIGSTOP: u8 = 19;
    pub const MAX: u8 = 32;
}

/// What to do when a signal is delivered
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SignalAction {
    /// Default kernel action (terminate, ignore, stop, etc.)
    Default,
    /// Ignore the signal
    Ignore,
    /// User-space handler at this virtual address
    Handler(u64),
}

/// Per-process signal state
pub struct SignalState {
    /// Bitmask of pending signals (bit N = signal N+1 is pending)
    pub pending: u32,
    /// Bitmask of blocked signals (signal mask)
    pub blocked: u32,
    /// Per-signal action table
    pub actions: [SignalAction; sig::MAX as usize],
    /// Pending signal number to deliver (when scheduled)
    pub pending_signal: u8,
    /// Signal handler address
    pub handler_addr: u64,
}

impl SignalState {
    pub const fn new() -> Self {
        Self {
            pending: 0,
            blocked: 0,
            actions: [SignalAction::Default; sig::MAX as usize],
            pending_signal: 0,
            handler_addr: 0,
        }
    }

    /// Send signal `signum` to this process (sets pending bit)
    pub fn send(&mut self, signum: u8) -> bool {
        if signum == 0 || signum > sig::MAX {
            return false;
        }
        self.pending |= 1 << (signum - 1);
        true
    }

    /// Returns true if there are unblocked pending signals
    pub fn has_pending(&self) -> bool {
        (self.pending & !self.blocked) != 0
    }

    /// Dequeue the highest-priority unblocked pending signal.
    /// Returns the signal number (1-based) or 0 if none.
    pub fn dequeue(&mut self) -> u8 {
        let deliverable = self.pending & !self.blocked;
        if deliverable == 0 {
            return 0;
        }
        // Find lowest set bit (lowest signal number first)
        let bit = deliverable.trailing_zeros() as u8;
        self.pending &= !(1 << bit);
        bit + 1
    }

    /// Set the action for a signal
    pub fn set_action(&mut self, signum: u8, action: SignalAction) -> bool {
        if signum == 0 || signum > sig::MAX {
            return false;
        }
        // SIGKILL and SIGSTOP cannot be caught or ignored
        if signum == sig::SIGKILL || signum == sig::SIGSTOP {
            return false;
        }
        self.actions[(signum - 1) as usize] = action;
        true
    }

    /// Get the action for a signal
    pub fn get_action(&self, signum: u8) -> SignalAction {
        if signum == 0 || signum > sig::MAX {
            return SignalAction::Default;
        }
        self.actions[(signum - 1) as usize]
    }
}

/// Determine the default action for a signal (for kernel-side delivery)
pub fn default_action(signum: u8) -> DefaultDisposition {
    match signum {
        sig::SIGCHLD | sig::SIGCONT => DefaultDisposition::Ignore,
        sig::SIGSTOP => DefaultDisposition::Stop,
        sig::SIGKILL
        | sig::SIGTERM
        | sig::SIGHUP
        | sig::SIGINT
        | sig::SIGQUIT
        | sig::SIGPIPE
        | sig::SIGALRM
        | sig::SIGUSR1
        | sig::SIGUSR2 => DefaultDisposition::Terminate,
        sig::SIGSEGV | sig::SIGBUS | sig::SIGFPE | sig::SIGILL | sig::SIGTRAP | sig::SIGABRT => {
            DefaultDisposition::CoreDump
        }
        _ => DefaultDisposition::Terminate,
    }
}

/// Default disposition for signals
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefaultDisposition {
    Terminate,
    CoreDump,
    Ignore,
    Stop,
}

/// x86_64 Signal frame layout (Linux-compatible)
///
/// When a signal is delivered, the kernel:
/// 1. Pushes a sigframe onto the user stack
/// 2. Modifies the process CPU state to jump to the handler
/// 3. Sets sigreturn as the return address
#[repr(C, packed)]
pub struct SignalFrame {
    /// Return address (points to sigreturn code)
    pub return_ip: u64,
    /// Signal number
    pub signal: u64,
    /// CPU state (restored by sigreturn)
    pub cpu_state: super::CpuState,
}

impl SignalFrame {
    pub const fn new(signal: u8, cpu: &super::CpuState) -> Self {
        Self {
            return_ip: 0, // Will be set to sigreturn trampoline
            signal: signal as u64,
            cpu_state: *cpu,
        }
    }
}

/// Deliver a signal to a process by setting up the signal frame
/// Returns true if the signal was delivered (context modified)
pub fn deliver_signal_to_user(pid: super::Pid) -> bool {
    // Find the process
    let result = crate::process::scheduler::with_process_mut(pid, |p| {
        // Get pending signal info
        let sig = p.signals.dequeue();
        if sig == 0 {
            return None;
        }

        let action = p.signals.get_action(sig);

        match action {
            SignalAction::Ignore => None,
            SignalAction::Handler(handler_addr) => {
                // Store signal info for the handler
                crate::println!(
                    "[SIGNAL] Delivering signal {} to PID {}, handler at 0x{:x}",
                    sig,
                    pid.0,
                    handler_addr
                );

                // Store in process state for scheduler to pick up
                p.signals.pending_signal = sig;
                p.signals.handler_addr = handler_addr;

                Some(())
            }

            SignalAction::Default => {
                // Default action is handled by deliver_signals()
                None
            }
        }
    });

    result.is_some()
}
