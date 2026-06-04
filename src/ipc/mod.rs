/// Inter-Process Communication (IPC) for ZiqaKernel
///
/// Simple synchronous message-passing channels.
/// Each channel has a fixed-size ring buffer; no heap allocation required.
pub mod shm;
pub mod signal;

use crate::process::Pid;
use spin::{Mutex, RwLock};
use alloc::sync::Arc;

/// Maximum bytes in a single IPC message
pub const MSG_MAX: usize = 256;
/// Ring buffer capacity (number of messages)
const RING_CAP: usize = 16;
/// Maximum number of channels in the global table
const MAX_CHANNELS: usize = 32;

/// A single IPC message
#[derive(Clone, Copy)]
pub struct Message {
    pub sender: Pid,
    pub len: usize,
    pub data: [u8; MSG_MAX],
}

impl Message {
    pub fn new(sender: Pid, data: &[u8]) -> Self {
        let len = data.len().min(MSG_MAX);
        let mut buf = [0u8; MSG_MAX];
        buf[..len].copy_from_slice(&data[..len]);
        Self {
            sender,
            len,
            data: buf,
        }
    }
}

/// IPC error
#[derive(Debug)]
pub enum IpcError {
    Full,
    Empty,
    InvalidChannel,
}

/// Internal state of a channel, protected by a per-channel Mutex
struct ChannelInner {
    ring: [Option<Message>; RING_CAP],
    head: usize,
    tail: usize,
    count: usize,
}

const NONE_MSG: Option<Message> = None;

impl ChannelInner {
    pub const fn new() -> Self {
        Self {
            ring: [NONE_MSG; RING_CAP],
            head: 0,
            tail: 0,
            count: 0,
        }
    }

    pub fn send(&mut self, msg: Message) -> Result<(), IpcError> {
        if self.count >= RING_CAP {
            return Err(IpcError::Full);
        }
        self.ring[self.tail] = Some(msg);
        self.tail = (self.tail + 1) % RING_CAP;
        self.count += 1;
        Ok(())
    }

    pub fn recv(&mut self) -> Result<Message, IpcError> {
        if self.count == 0 {
            return Err(IpcError::Empty);
        }
        let msg = self.ring[self.head].take().unwrap();
        self.head = (self.head + 1) % RING_CAP;
        self.count -= 1;
        Ok(msg)
    }
}

/// A thread-safe IPC channel
pub struct Channel {
    pub id: u32,
    inner: Mutex<ChannelInner>,
}

impl Channel {
    pub fn new(id: u32) -> Self {
        Self {
            id,
            inner: Mutex::new(ChannelInner::new()),
        }
    }

    pub fn send(&self, msg: Message) -> Result<(), IpcError> {
        self.inner.lock().send(msg)
    }

    pub fn recv(&self) -> Result<Message, IpcError> {
        self.inner.lock().recv()
    }
}

/// Global channel table with fine-grained locking
pub struct ChannelTable {
    channels: [Option<Arc<Channel>>; MAX_CHANNELS],
    next_id: u32,
}

impl ChannelTable {
    pub const fn new() -> Self {
        const NONE_CHAN: Option<Arc<Channel>> = None;
        Self {
            channels: [NONE_CHAN; MAX_CHANNELS],
            next_id: 1,
        }
    }

    /// Create a new channel, returns its id
    pub fn create(&mut self) -> Option<u32> {
        for slot in self.channels.iter_mut() {
            if slot.is_none() {
                let id = self.next_id;
                self.next_id += 1;
                *slot = Some(Arc::new(Channel::new(id)));
                return Some(id);
            }
        }
        None
    }

    pub fn get(&self, id: u32) -> Option<Arc<Channel>> {
        self.channels
            .iter()
            .filter_map(|s| s.as_ref().cloned())
            .find(|c| c.id == id)
    }

    pub fn destroy(&mut self, id: u32) -> bool {
        for slot in self.channels.iter_mut() {
            if slot.as_ref().map(|c| c.id == id).unwrap_or(false) {
                *slot = None;
                return true;
            }
        }
        false
    }
}

pub static IPC: RwLock<ChannelTable> = RwLock::new(ChannelTable::new());

/// Convenience wrappers
pub fn create_channel() -> Option<u32> {
    IPC.write().create()
}

pub fn destroy_channel(id: u32) {
    IPC.write().destroy(id);
}

pub fn send(channel_id: u32, sender: Pid, data: &[u8]) -> Result<(), IpcError> {
    // Read-lock the table to find the channel, then perform per-channel locked send
    let chan = IPC.read().get(channel_id).ok_or(IpcError::InvalidChannel)?;
    chan.send(Message::new(sender, data))
}

pub fn recv(channel_id: u32) -> Result<Message, IpcError> {
    // Read-lock the table to find the channel, then perform per-channel locked recv
    let chan = IPC.read().get(channel_id).ok_or(IpcError::InvalidChannel)?;
    chan.recv()
}

