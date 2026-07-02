use core::sync::atomic::{AtomicU64, Ordering};

use crate::scheme::{Scheme, SchemeResult};

pub struct RandScheme {
    state: AtomicU64,
}

impl RandScheme {
    pub const fn new() -> Self {
        Self {
            state: AtomicU64::new(0x9E37_79B9_7F4A_7C15),
        }
    }

    fn next(&self) -> u64 {
        // ponytail: timer-mixed PRNG; replace with hardware entropy when rand: becomes security-sensitive.
        let counter = self
            .state
            .fetch_add(0x9E37_79B9_7F4A_7C15, Ordering::Relaxed);
        let mut x = counter ^ (crate::timer::uptime_ms() as u64).rotate_left(17);
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
}

impl Scheme for RandScheme {
    fn open(&self, _path: &str, _flags: usize) -> SchemeResult<usize> {
        Ok(0)
    }

    fn read(&self, _id: usize, buf: &mut [u8]) -> SchemeResult<usize> {
        let mut offset = 0;
        while offset < buf.len() {
            let bytes = self.next().to_ne_bytes();
            let len = core::cmp::min(bytes.len(), buf.len() - offset);
            buf[offset..offset + len].copy_from_slice(&bytes[..len]);
            offset += len;
        }
        Ok(buf.len())
    }

    fn write(&self, _id: usize, buf: &[u8]) -> SchemeResult<usize> {
        Ok(buf.len())
    }

    fn close(&self, _id: usize) -> SchemeResult<()> {
        Ok(())
    }
}
