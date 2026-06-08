use crate::scheme::{Scheme, SchemeResult};
use crate::timer;
use crate::abi::AbiError;

pub struct TimeScheme;

impl TimeScheme {
    pub const fn new() -> Self {
        Self
    }
}

impl Scheme for TimeScheme {
    fn open(&self, _path: &str, _flags: usize) -> SchemeResult<usize> {
        // For now, return a dummy handle ID.
        // In the future, we can manage handles here to track different time-based resources.
        Ok(1)
    }

    fn read(&self, _id: usize, buf: &mut [u8]) -> SchemeResult<usize> {
        // Return current uptime in milliseconds as a string
        let ms = timer::uptime_ms();
        let s = alloc::format!("{}\n", ms);
        let bytes = s.as_bytes();
        let len = core::cmp::min(buf.len(), bytes.len());
        buf[..len].copy_from_slice(&bytes[..len]);
        Ok(len)
    }

    fn write(&self, _id: usize, _buf: &[u8]) -> SchemeResult<usize> {
        // Writes not supported for basic time reading
        Err(AbiError::Other("Not implemented"))
    }

    fn close(&self, _id: usize) -> SchemeResult<()> {
        Ok(())
    }
}
