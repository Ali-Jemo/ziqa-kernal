use crate::scheme::{Scheme, SchemeResult};
use crate::abi::AbiError;

pub struct KeyboardScheme;

impl KeyboardScheme {
    pub const fn new() -> Self {
        Self
    }
}

impl Scheme for KeyboardScheme {
    fn open(&self, _path: &str, _flags: usize) -> SchemeResult<usize> {
        Ok(0)
    }

    fn read(&self, _id: usize, _buf: &mut [u8]) -> SchemeResult<usize> {
        Err(AbiError::Other("Not implemented"))
    }

    fn write(&self, _id: usize, buf: &[u8]) -> SchemeResult<usize> {
        for &b in buf {
            crate::drivers::keyboard::push_raw_byte(b);
        }
        Ok(buf.len())
    }

    fn close(&self, _id: usize) -> SchemeResult<()> {
        Ok(())
    }
}
