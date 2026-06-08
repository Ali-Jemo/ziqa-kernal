/// Debug Scheme for ZiqaKernel
/// Maps to the kernel serial console.

use crate::scheme::{Scheme, SchemeResult};

pub struct DebugScheme {}

impl DebugScheme {
    pub fn new() -> Self {
        Self {}
    }
}

impl Scheme for DebugScheme {
    fn open(&self, _path: &str, _flags: usize) -> SchemeResult<usize> {
        Ok(0)
    }
    
    fn read(&self, _id: usize, _buf: &mut [u8]) -> SchemeResult<usize> {
        Err(crate::abi::AbiError::PermissionDenied)
    }
    
    fn write(&self, _id: usize, buf: &[u8]) -> SchemeResult<usize> {
        for &b in buf {
            crate::klog::putc(b as char);
        }
        Ok(buf.len())
    }
    
    fn close(&self, _id: usize) -> SchemeResult<()> {
        Ok(())
    }
}
