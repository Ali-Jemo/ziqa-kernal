///
/// Minimal `usercopy` shim to satisfy existing syscall call sites
///
/// Provides `UserSliceRo` / `UserSliceWo` types with `ro()` / `wo()` constructors
/// plus `copy_to_slice()` / `copy_from_slice()` helpers used throughout
/// `src/abi/linux/mod.rs` and `src/abi/syscall.rs`.
///
/// Delegates the actual copies to `crate::memory::{copy_from_user, copy_to_user}`.
extern crate alloc;

use crate::abi::AbiError;
use crate::memory::copy_from_user;
use crate::memory::copy_to_user;

#[derive(Clone, Copy, Debug)]
pub struct UserSliceRo {
    addr: u64,
    len: usize,
}

#[derive(Clone, Copy, Debug)]
pub struct UserSliceWo {
    addr: u64,
    len: usize,
}

impl UserSliceRo {
    pub fn ro(addr: u64, len: usize) -> Result<Self, AbiError> {
        Ok(Self { addr, len })
    }

    pub fn addr(&self) -> u64 {
        self.addr
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn copy_to_slice(&self, dst: &mut [u8]) -> Result<(), AbiError> {
        if self.len != dst.len() {
            return Err(AbiError::Other("usercopy len mismatch"));
        }
        copy_from_user(dst, self.addr).map_err(|_| AbiError::Other("EFAULT: read buffer"))
    }
}

impl UserSliceWo {
    pub fn wo(addr: u64, len: usize) -> Result<Self, AbiError> {
        Ok(Self { addr, len })
    }

    pub fn addr(&self) -> u64 {
        self.addr
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn copy_from_slice(&self, src: &[u8]) -> Result<(), AbiError> {
        if self.len != src.len() {
            return Err(AbiError::Other("usercopy len mismatch"));
        }
        copy_to_user(self.addr, src).map_err(|_| AbiError::Other("EFAULT: write buffer"))
    }
}

/// Read a NUL-terminated string from userspace.
///
/// Returns an owned kernel-side string. On any fault/invalid access,
/// returns an empty `String`.
pub fn read_user_string(addr: u64, max_len: usize) -> Result<alloc::vec::Vec<u8>, AbiError> {
    let mut out = alloc::vec::Vec::new();
    for i in 0..max_len {
        let off = addr + i as u64;
        let mut b = [0u8; 1];
        copy_from_user(&mut b, off).map_err(|_| AbiError::Other("EFAULT: read_user_string"))?;
        if b[0] == 0 {
            break;
        }
        out.push(b[0]);
    }
    Ok(out)
}
