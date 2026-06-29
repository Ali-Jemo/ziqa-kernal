//! `event:` scheme — event notification bus.
//!
//! Provides queues that accumulate [`IoEvent`] notifications.  Consumers open
//! `event:` to obtain a queue handle, then write registration entries specifying
//! which (scheme, number) sources they are interested in.  Readers block until
//! an event arrives.
//!
//! # Write format (binary registration entries)
//!
//! Multiple entries can be written in one call; each is:
//!
//! | Offset | Size   | Field        |
//! |--------|--------|--------------|
//! | 0      | 1      | scheme_len   |
//! | 1      | N      | scheme bytes |
//! | 1+N    | 8      | number (LE)  |
//! | 9+N    | 8      | flags (LE)   |
//! | 17+N   | 8      | data (LE)    |
//!
//! Total per entry: `18 + scheme_len` bytes.

use alloc::sync::Arc;

use crate::abi::AbiError;
use crate::event::{self, EventQueue, IoEvent};
use crate::scheme::{Scheme, SchemeResult};

/// Event scheme — no per-instance state; queues live in the global registry.
pub struct EventScheme;

impl EventScheme {
    pub const fn new() -> Self {
        Self
    }
}

impl Scheme for EventScheme {
    fn open(&self, _path: &str, _flags: usize) -> SchemeResult<usize> {
        let id = event::next_queue_id();
        let queue = Arc::new(EventQueue::new(id));
        event::insert_queue(id, queue);
        Ok(id as usize)
    }

    fn read(&self, id: usize, buf: &mut [u8]) -> SchemeResult<usize> {
        let queue = event::get_queue(id as u64)
            .ok_or(AbiError::Other("EBADF"))?;

        let event = queue.receive();
        // Safety: IoEvent is packed POD; transmuting to &[u8] is safe.
        let event_bytes = unsafe {
            core::slice::from_raw_parts(
                &event as *const IoEvent as *const u8,
                core::mem::size_of::<IoEvent>(),
            )
        };

        let len = core::cmp::min(buf.len(), event_bytes.len());
        buf[..len].copy_from_slice(&event_bytes[..len]);
        Ok(len)
    }

    fn write(&self, id: usize, buf: &[u8]) -> SchemeResult<usize> {
        // Verify the queue exists (prevents orphan registries).
        let _queue = event::get_queue(id as u64)
            .ok_or(AbiError::Other("EBADF"))?;

        let mut off = 0;
        while off < buf.len() {
            if off + 1 > buf.len() {
                return Err(AbiError::Other("EINVAL"));
            }
            let scheme_len = buf[off] as usize;
            off += 1;

            if off + scheme_len > buf.len() {
                return Err(AbiError::Other("EINVAL"));
            }
            let scheme = core::str::from_utf8(&buf[off..off + scheme_len])
                .map_err(|_| AbiError::Other("EINVAL"))?;
            off += scheme_len;

            if off + 8 > buf.len() {
                return Err(AbiError::Other("EINVAL"));
            }
            let number = u64::from_le_bytes(buf[off..off + 8].try_into().unwrap());
            off += 8;

            if off + 8 > buf.len() {
                return Err(AbiError::Other("EINVAL"));
            }
            let flags = u64::from_le_bytes(buf[off..off + 8].try_into().unwrap());
            off += 8;

            if off + 8 > buf.len() {
                return Err(AbiError::Other("EINVAL"));
            }
            let data = u64::from_le_bytes(buf[off..off + 8].try_into().unwrap());
            off += 8;

            event::register(scheme, number, id as u64, flags, data);
        }

        Ok(buf.len())
    }

    fn close(&self, id: usize) -> SchemeResult<()> {
        event::remove_queue(id as u64);
        Ok(())
    }
}
