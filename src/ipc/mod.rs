/// Inter-Process Communication (IPC) for ZiqaKernel
///
/// Simple synchronous message-passing channels.
/// Each channel has a fixed-size ring buffer; no heap allocation required.
pub mod shm;
pub mod signal;
pub mod gui;
pub mod driver;

use crate::process::Pid;
use spin::{Mutex, RwLock};
use alloc::sync::Arc;
use alloc::vec::Vec;

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

/// Internal state of a channel
struct ChannelInner {
    ring: [Option<Message>; RING_CAP],
    head: usize,
    tail: usize,
}

pub struct Channel {
    inner: Mutex<ChannelInner>,
}

impl Channel {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(ChannelInner {
                ring: [None; RING_CAP],
                head: 0,
                tail: 0,
            }),
        }
    }

    pub fn send(&self, msg: Message) -> Result<(), IpcError> {
        let mut inner = self.inner.lock();
        let next_head = (inner.head + 1) % RING_CAP;
        if next_head == inner.tail {
            return Err(IpcError::Full);
        }
        let head = inner.head;
        inner.ring[head] = Some(msg);
        inner.head = next_head;
        Ok(())
    }

    pub fn recv(&self) -> Result<Message, IpcError> {
        let mut inner = self.inner.lock();
        if inner.head == inner.tail {
            return Err(IpcError::Empty);
        }
        let tail = inner.tail;
        let msg = inner.ring[tail].take().ok_or(IpcError::Empty)?;
        inner.tail = (inner.tail + 1) % RING_CAP;
        Ok(msg)
    }
}

static CHANNELS: Mutex<[Option<Arc<Channel>>; MAX_CHANNELS]> = Mutex::new([const { None }; MAX_CHANNELS]);

pub fn get_channel(id: usize) -> Result<Arc<Channel>, IpcError> {
    let channels = CHANNELS.lock();
    channels.get(id).ok_or(IpcError::InvalidChannel)?.as_ref().cloned().ok_or(IpcError::InvalidChannel)
}

pub fn register_channel(id: usize, chan: Arc<Channel>) {
    let mut channels = CHANNELS.lock();
    channels[id] = Some(chan);
}

pub struct ChannelTable {
    channels: [Option<Arc<Channel>>; MAX_CHANNELS],
}

impl ChannelTable {
    pub const fn new() -> Self {
        const NONE_CHAN: Option<Arc<Channel>> = None;
        Self {
            channels: [NONE_CHAN; MAX_CHANNELS],
        }
    }

    pub fn create(&mut self) -> Option<u32> {
        for (i, slot) in self.channels.iter_mut().enumerate() {
            if slot.is_none() {
                *slot = Some(Arc::new(Channel::new()));
                return Some((i + 1) as u32);
            }
        }
        None
    }

    pub fn get(&self, id: u32) -> Option<Arc<Channel>> {
        if id == 0 || id as usize > MAX_CHANNELS {
            return None;
        }
        self.channels[(id - 1) as usize].clone()
    }

    pub fn destroy(&mut self, id: u32) -> bool {
        if id == 0 || id as usize > MAX_CHANNELS {
            return false;
        }
        let slot = &mut self.channels[(id - 1) as usize];
        if slot.is_some() {
            *slot = None;
            true
        } else {
            false
        }
    }
}

pub static IPC: RwLock<ChannelTable> = RwLock::new(ChannelTable::new());

pub fn create_channel() -> Option<u32> {
    IPC.write().create()
}

pub fn destroy_channel(id: u32) {
    IPC.write().destroy(id);
}

pub fn send(channel_id: u32, sender: Pid, data: &[u8]) -> Result<(), IpcError> {
    let chan = IPC.read().get(channel_id).ok_or(IpcError::InvalidChannel)?;
    chan.send(Message::new(sender, data))
}

pub fn recv(channel_id: u32) -> Result<Message, IpcError> {
    let chan = IPC.read().get(channel_id).ok_or(IpcError::InvalidChannel)?;
    chan.recv()
}
