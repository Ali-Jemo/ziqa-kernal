use crate::scheme::{Scheme, SchemeResult};
use crate::sync::WaitQueue;

#[derive(Debug, Clone, Copy)]
pub enum PacketKind {
    Open,
    Read,
    Write,
    Close,
}

#[derive(Debug, Clone, Copy)]
pub struct Packet {
    pub id: usize,
    pub kind: PacketKind,
    pub a: usize,
    pub b: usize,
    pub c: usize,
}

pub struct UserScheme {
    pub todo: WaitQueue<Packet>,
}

impl UserScheme {
    pub fn new() -> Self {
        Self {
            todo: WaitQueue::new(),
        }
    }
}

impl Scheme for UserScheme {
    fn open(&self, _path: &str, _flags: usize) -> SchemeResult<usize> {
        // Create a packet and send it to the userspace handler
        // For now, return a dummy ID
        Ok(0)
    }

    fn read(&self, _id: usize, _buf: &mut [u8]) -> SchemeResult<usize> {
        Ok(0)
    }

    fn write(&self, _id: usize, _buf: &[u8]) -> SchemeResult<usize> {
        Ok(0)
    }

    fn close(&self, _id: usize) -> SchemeResult<()> {
        Ok(())
    }
}
