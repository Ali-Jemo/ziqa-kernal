/// Kernel ring-buffer logger (klog) for ZiqaKernel
///
/// Stores log entries in a fixed-size circular buffer.
/// Supports log levels: Error, Warn, Info, Debug.
/// Replaces scattered println! calls with structured, queryable log entries.

use spin::Mutex;

const KLOG_CAPACITY: usize = 256;
const MSG_LEN: usize = 128;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
#[repr(u8)]
pub enum Level {
    Error = 0,
    Warn  = 1,
    Info  = 2,
    Debug = 3,
}

impl Level {
    pub fn as_str(self) -> &'static str {
        match self {
            Level::Error => "ERROR",
            Level::Warn  => "WARN ",
            Level::Info  => "INFO ",
            Level::Debug => "DEBUG",
        }
    }
}

#[derive(Clone, Copy)]
pub struct LogEntry {
    pub level: Level,
    pub tick: u64,
    pub msg: [u8; MSG_LEN],
    pub msg_len: usize,
}

impl LogEntry {
    const fn empty() -> Self {
        Self {
            level: Level::Info,
            tick: 0,
            msg: [0u8; MSG_LEN],
            msg_len: 0,
        }
    }

    pub fn message(&self) -> &str {
        core::str::from_utf8(&self.msg[..self.msg_len]).unwrap_or("<invalid utf8>")
    }
}

pub struct KernelLog {
    entries: [LogEntry; KLOG_CAPACITY],
    head: usize,   // oldest entry
    tail: usize,   // next write position
    count: usize,
    /// Minimum level to store (entries below this are dropped)
    pub min_level: Level,
}

impl KernelLog {
    pub const fn new() -> Self {
        Self {
            entries: [LogEntry::empty(); KLOG_CAPACITY],
            head: 0,
            tail: 0,
            count: 0,
            min_level: Level::Info,
        }
    }

    pub fn log(&mut self, level: Level, tick: u64, msg: &str) {
        if level > self.min_level { return; }

        let bytes = msg.as_bytes();
        let len = bytes.len().min(MSG_LEN);

        let mut entry = LogEntry::empty();
        entry.level = level;
        entry.tick = tick;
        entry.msg[..len].copy_from_slice(&bytes[..len]);
        entry.msg_len = len;

        self.entries[self.tail] = entry;
        self.tail = (self.tail + 1) % KLOG_CAPACITY;

        if self.count < KLOG_CAPACITY {
            self.count += 1;
        } else {
            // Overwrite oldest
            self.head = (self.head + 1) % KLOG_CAPACITY;
        }
    }

    /// Iterate entries from oldest to newest
    pub fn iter(&self) -> KlogIter<'_> {
        KlogIter { log: self, pos: 0 }
    }

    pub fn count(&self) -> usize { self.count }

    /// Dump all entries via the serial/VGA println macro
    pub fn dump(&self) {
        for entry in self.iter() {
            crate::println!("[klog][{}][t={}] {}",
                entry.level.as_str(), entry.tick, entry.message());
        }
    }

    /// Dump only entries at or above a given level
    pub fn dump_level(&self, min: Level) {
        for entry in self.iter() {
            if entry.level <= min {
                crate::println!("[klog][{}][t={}] {}",
                    entry.level.as_str(), entry.tick, entry.message());
            }
        }
    }
}

pub struct KlogIter<'a> {
    log: &'a KernelLog,
    pos: usize,
}

impl<'a> Iterator for KlogIter<'a> {
    type Item = &'a LogEntry;
    fn next(&mut self) -> Option<Self::Item> {
        if self.pos >= self.log.count { return None; }
        let idx = (self.log.head + self.pos) % KLOG_CAPACITY;
        self.pos += 1;
        Some(&self.log.entries[idx])
    }
}

pub static KLOG: Mutex<KernelLog> = Mutex::new(KernelLog::new());

/// Log at a given level using the current scheduler tick as timestamp
#[macro_export]
macro_rules! klog {
    ($level:expr, $($arg:tt)*) => {{
        use $crate::klog::KLOG;
        let tick = $crate::timer::uptime_ticks();
        // Format into a fixed buffer without heap
        let mut buf = [0u8; 128];
        let s = $crate::klog::fmt_to_buf(&mut buf, format_args!($($arg)*));
        KLOG.lock().log($level, tick, s);
    }};
}

/// Format into a fixed stack buffer (no alloc)
pub fn fmt_to_buf<'a>(buf: &'a mut [u8; 128], args: core::fmt::Arguments<'_>) -> &'a str {
    use core::fmt::Write;
    struct BufWriter<'a> { buf: &'a mut [u8; 128], pos: usize }
    impl<'a> Write for BufWriter<'a> {
        fn write_str(&mut self, s: &str) -> core::fmt::Result {
            let bytes = s.as_bytes();
            let space = self.buf.len() - self.pos;
            let n = bytes.len().min(space);
            self.buf[self.pos..self.pos + n].copy_from_slice(&bytes[..n]);
            self.pos += n;
            Ok(())
        }
    }
    let mut w = BufWriter { buf, pos: 0 };
    let _ = core::fmt::write(&mut w, args);
    let len = w.pos;
    core::str::from_utf8(&w.buf[..len]).unwrap_or("")
}
