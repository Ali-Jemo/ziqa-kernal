//! VirtIO data structures shared between the net driver and protocol layer.

pub const VQ_DESC_F_WRITE: u16 = 2;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct VirtQueueDesc {
    pub addr: u64,
    pub len: u32,
    pub flags: u16,
    pub next: u16,
}

#[repr(C)]
pub struct VirtQueueAvail {
    pub flags: u16,
    pub idx: u16,
    pub ring: [u16; 256],
    pub used_event: u16,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct VirtQueueUsedElem {
    pub id: u32,
    pub len: u32,
}

#[repr(C)]
pub struct VirtQueueUsed {
    pub flags: u16,
    pub idx: u16,
    pub ring: [VirtQueueUsedElem; 256],
    pub avail_event: u16,
}

pub struct VirtQueue {
    pub queue: &'static mut [VirtQueueDesc],
    pub avail: &'static mut VirtQueueAvail,
    pub used: &'static mut VirtQueueUsed,
    pub size: u16,
    pub last_used_idx: u16,
    pub last_avail_idx: u16,
}

impl VirtQueue {
    pub fn new(
        queue: &'static mut [VirtQueueDesc],
        avail: &'static mut VirtQueueAvail,
        used: &'static mut VirtQueueUsed,
    ) -> Self {
        let size = queue.len() as u16;
        Self { queue, avail, used, size, last_used_idx: 0, last_avail_idx: 0 }
    }
}

#[repr(C, packed)]
#[derive(Clone, Copy, Default)]
pub struct VirtioNetHdr {
    pub flags: u8,
    pub gso_type: u8,
    pub hdr_len: u16,
    pub gso_size: u16,
    pub csum_start: u16,
    pub csum_offset: u16,
    pub num_buffers: u16,
    pub _pad: u16,
}
