use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use spin::Mutex;
use crate::sync::WaitCondition;
use crate::scheme::{Scheme, SchemeResult};
use crate::process::Pid;
use crate::abi::AbiError;
use core::sync::atomic::{AtomicUsize, Ordering};

const PTY_BUF_CAP: usize = 4096;

pub struct RingBuf {
    buf: [u8; PTY_BUF_CAP],
    head: usize,
    tail: usize,
    count: usize,
}

impl RingBuf {
    const fn new() -> Self {
        Self {
            buf: [0; PTY_BUF_CAP],
            head: 0,
            tail: 0,
            count: 0,
        }
    }

    fn push(&mut self, byte: u8) -> bool {
        if self.count < PTY_BUF_CAP {
            self.buf[self.tail] = byte;
            self.tail = (self.tail + 1) % PTY_BUF_CAP;
            self.count += 1;
            true
        } else {
            false
        }
    }

    fn pop(&mut self) -> Option<u8> {
        if self.count == 0 {
            None
        } else {
            let b = self.buf[self.head];
            self.head = (self.head + 1) % PTY_BUF_CAP;
            self.count -= 1;
            Some(b)
        }
    }

    fn is_empty(&self) -> bool {
        self.count == 0
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Winsize {
    pub ws_row: u16,
    pub ws_col: u16,
    pub ws_xpixel: u16,
    pub ws_ypixel: u16,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Termios {
    pub c_iflag: u32,
    pub c_oflag: u32,
    pub c_cflag: u32,
    pub c_lflag: u32,
    pub c_line: u8,
    pub c_cc: [u8; 32],
}

// POSIX terminal flags (simplified)
pub const ICANON: u32 = 0o0000002;
pub const ECHO: u32   = 0o0000010;
pub const ISIG: u32   = 0o0000001;
pub const OPOST: u32  = 0o0000001;
pub const ONLCR: u32  = 0o0000004;
pub const ICRNL: u32  = 0o0000400;

pub struct PtyPair {
    pub id: usize,
    pub master_to_slave: Mutex<RingBuf>,
    pub slave_to_master: Mutex<RingBuf>,
    pub termios: Mutex<Termios>,
    pub winsize: Mutex<Winsize>,
    pub fg_pgid: Mutex<Option<Pid>>,
    pub read_condition: WaitCondition,
    pub master_read_condition: WaitCondition,
}

impl PtyPair {
    pub fn new(id: usize) -> Self {
        let mut termios = Termios {
            c_iflag: ICRNL,
            c_oflag: OPOST | ONLCR,
            c_cflag: 0,
            c_lflag: ISIG | ICANON | ECHO,
            c_line: 0,
            c_cc: [0; 32],
        };
        // VINTR = ^C
        termios.c_cc[0] = 3; 
        
        Self {
            id,
            master_to_slave: Mutex::new(RingBuf::new()),
            slave_to_master: Mutex::new(RingBuf::new()),
            termios: Mutex::new(termios),
            winsize: Mutex::new(Winsize {
                ws_row: 25,
                ws_col: 80,
                ws_xpixel: 0,
                ws_ypixel: 0,
            }),
            fg_pgid: Mutex::new(None),
            read_condition: WaitCondition::new(),
            master_read_condition: WaitCondition::new(),
        }
    }
}

pub struct PtyScheme {
    pairs: Mutex<BTreeMap<usize, Arc<PtyPair>>>,
    next_id: AtomicUsize,
}

impl PtyScheme {
    pub fn new() -> Self {
        Self {
            pairs: Mutex::new(BTreeMap::new()),
            next_id: AtomicUsize::new(1), // IDs start from 1
        }
    }
}

// IOCTL commands
const TIOCGWINSZ: usize = 0x5413;
const TIOCSWINSZ: usize = 0x5414;
const TCGETS: usize     = 0x5401;
const TCSETS: usize     = 0x5402;
const TIOCGPGRP: usize  = 0x540F;
const TIOCSPGRP: usize  = 0x5410;
const TIOCSCTTY: usize  = 0x540E;

impl Scheme for PtyScheme {
    fn open(&self, path: &str, _flags: usize) -> SchemeResult<usize> {
        if path == "pty:ptmx" || path == "ptmx" {
            let pair_id = self.next_id.fetch_add(1, Ordering::SeqCst);
            let pair = Arc::new(PtyPair::new(pair_id));
            self.pairs.lock().insert(pair_id, pair);
            Ok(pair_id << 1) // Bit 0 = 0 means master
        } else if let Some(stripped) = path.strip_prefix("pty:pts/") {
            let id = stripped.parse::<usize>().map_err(|_| AbiError::Other("Invalid pts ID"))?;
            let pairs = self.pairs.lock();
            if pairs.contains_key(&id) {
                Ok((id << 1) | 1) // Bit 0 = 1 means slave
            } else {
                Err(AbiError::Other("PTS not found"))
            }
        } else {
            Err(AbiError::Other("File not found"))
        }
    }

    fn read(&self, id: usize, buf: &mut [u8]) -> SchemeResult<usize> {
        let is_slave = (id & 1) == 1;
        let pair_id = id >> 1;
        let pair = self.pairs.lock().get(&pair_id).cloned().ok_or(AbiError::Other("Bad FD"))?;

        // Block until data is available using WaitCondition.
        // We keep `pair` (Arc reference) alive while waiting;
        // only the pairs-map lock is dropped when the block scope exits.
        if is_slave {
            loop {
                if !pair.master_to_slave.lock().is_empty() {
                    break;
                }
                pair.read_condition.wait("pty: slave read");
            }
        } else {
            loop {
                if !pair.slave_to_master.lock().is_empty() {
                    break;
                }
                pair.master_read_condition.wait("pty: master read");
            }
        }

        let mut count = 0;

        if is_slave {
            let termios = pair.termios.lock();
            let icanon = (termios.c_lflag & ICANON) != 0;
            drop(termios);

            let mut master_to_slave = pair.master_to_slave.lock();
            for b in buf.iter_mut() {
                if let Some(byte) = master_to_slave.pop() {
                    *b = byte;
                    count += 1;
                    if icanon && byte == b'\n' {
                        break;
                    }
                } else {
                    break;
                }
            }
        } else {
            let mut slave_to_master = pair.slave_to_master.lock();
            for b in buf.iter_mut() {
                if let Some(byte) = slave_to_master.pop() {
                    *b = byte;
                    count += 1;
                } else {
                    break;
                }
            }
        }

        Ok(count.max(1))
    }

    fn write(&self, id: usize, buf: &[u8]) -> SchemeResult<usize> {
        let is_slave = (id & 1) == 1;
        let pair_id = id >> 1;
        let pair = self.pairs.lock().get(&pair_id).cloned().ok_or(AbiError::Other("Bad FD"))?;
        
        let mut count = 0;
        
        if is_slave {
            let termios = pair.termios.lock();
            let opost = (termios.c_oflag & OPOST) != 0;
            let onlcr = (termios.c_oflag & ONLCR) != 0;
            drop(termios);
            
            let mut slave_to_master = pair.slave_to_master.lock();
            for &b in buf {
                if opost && onlcr && b == b'\n' {
                    slave_to_master.push(b'\r');
                }
                if slave_to_master.push(b) {
                    count += 1;
                } else {
                    break;
                }
            }
            pair.master_read_condition.notify();
        } else {
            let termios = pair.termios.lock();
            let isig = (termios.c_lflag & ISIG) != 0;
            let echo = (termios.c_lflag & ECHO) != 0;
            let icrnl = (termios.c_iflag & ICRNL) != 0;
            // POSIX c_cc indices: VINTR=0, VQUIT=1, VERASE=2, VKILL=3, VEOF=4, VSUSP=10
            let intr_char = termios.c_cc[0];
            let quit_char = termios.c_cc[1];
            let susp_char = termios.c_cc[10];
            drop(termios);

            let mut master_to_slave = pair.master_to_slave.lock();
            let mut slave_to_master = pair.slave_to_master.lock();

            for mut b in buf.iter().copied() {
                if isig {
                    if b == intr_char {
                        if let Some(pgid) = *pair.fg_pgid.lock() {
                            crate::process::scheduler::SCHEDULER.send_signal_to_process_group(
                                pgid, crate::process::signal::sig::SIGINT,
                            );
                        }
                        if echo {
                            slave_to_master.push(b'^');
                            slave_to_master.push(b'C');
                        }
                        count += 1;
                        continue;
                    }
                    if b == quit_char {
                        if let Some(pgid) = *pair.fg_pgid.lock() {
                            crate::process::scheduler::SCHEDULER.send_signal_to_process_group(
                                pgid, crate::process::signal::sig::SIGQUIT,
                            );
                        }
                        if echo {
                            slave_to_master.push(b'^');
                            slave_to_master.push(b'\\');
                        }
                        count += 1;
                        continue;
                    }
                    if b == susp_char {
                        if let Some(pgid) = *pair.fg_pgid.lock() {
                            crate::process::scheduler::SCHEDULER.send_signal_to_process_group(
                                pgid, crate::process::signal::sig::SIGTSTP,
                            );
                        }
                        if echo {
                            slave_to_master.push(b'^');
                            slave_to_master.push(b'Z');
                        }
                        count += 1;
                        continue;
                    }
                }

                if icrnl && b == b'\r' {
                    b = b'\n';
                }

                if echo {
                    slave_to_master.push(b);
                }

                if master_to_slave.push(b) {
                    count += 1;
                } else {
                    break;
                }
            }
            pair.read_condition.notify();
            pair.master_read_condition.notify(); // For echo
        }
        
        return Ok(count);
    }

    fn close(&self, id: usize) -> SchemeResult<()> {
        let is_slave = (id & 1) == 1;
        let pair_id = id >> 1;
        // In a real implementation we track ref counts of master and slave.
        // For simplicity, we keep it alive, or remove if master closes.
        if !is_slave {
            self.pairs.lock().remove(&pair_id);
        }
        Ok(())
    }

    fn ioctl(&self, id: usize, request: usize, arg: usize) -> SchemeResult<usize> {
        let is_slave = (id & 1) == 1;
        let pair_id = id >> 1;
        let pair = self.pairs.lock().get(&pair_id).cloned().ok_or(AbiError::Other("Bad FD"))?;
        
        match request {
            TIOCGWINSZ => unsafe {
                if arg == 0 { return Err(AbiError::Other("Invalid argument")); }
                let state = pair.winsize.lock();
                core::ptr::copy_nonoverlapping(
                    & *state as *const Winsize as *const u8,
                    arg as *mut u8,
                    core::mem::size_of::<Winsize>(),
                );
                Ok(0)
            },
            TIOCSWINSZ => unsafe {
                if arg == 0 { return Err(AbiError::Other("Invalid argument")); }
                let mut state = pair.winsize.lock();
                core::ptr::copy_nonoverlapping(
                    arg as *const u8,
                    &mut *state as *mut Winsize as *mut u8,
                    core::mem::size_of::<Winsize>(),
                );
                Ok(0)
            },
            TCGETS => unsafe {
                if arg == 0 { return Err(AbiError::Other("Invalid argument")); }
                let state = pair.termios.lock();
                core::ptr::copy_nonoverlapping(
                    & *state as *const Termios as *const u8,
                    arg as *mut u8,
                    core::mem::size_of::<Termios>(),
                );
                Ok(0)
            },
            TCSETS => unsafe {
                if arg == 0 { return Err(AbiError::Other("Invalid argument")); }
                let mut state = pair.termios.lock();
                core::ptr::copy_nonoverlapping(
                    arg as *const u8,
                    &mut *state as *mut Termios as *mut u8,
                    core::mem::size_of::<Termios>(),
                );
                Ok(0)
            },
            TIOCGPGRP => unsafe {
                if arg == 0 { return Err(AbiError::Other("Invalid argument")); }
                let pgid = pair.fg_pgid.lock().unwrap_or(Pid(0)).0 as u32;
                *(arg as *mut u32) = pgid;
                Ok(0)
            },
            TIOCSPGRP => unsafe {
                if arg == 0 { return Err(AbiError::Other("Invalid argument")); }
                let pgid = Pid(*(arg as *const u32) as u64);
                *pair.fg_pgid.lock() = Some(pgid);
                Ok(0)
            },
            TIOCSCTTY => {
                if is_slave {
                    crate::process::scheduler::with_current_task_mut(|p| p.ctty = Some(pair_id));
                }
                Ok(0)
            },
            0x80045430 => unsafe { // TIOCGPTN
                if !is_slave {
                    if arg == 0 { return Err(AbiError::Other("Invalid argument")); }
                    *(arg as *mut u32) = pair_id as u32;
                    Ok(0)
                } else {
                    Err(AbiError::Other("Not a master PTY"))
                }
            },
            0x40045431 => { // TIOCSPTLCK
                // Just pretend we unlocked it
                Ok(0)
            },
            _ => Err(AbiError::Other("Unsupported ioctl")),
        }
    }
}
