//! TRB structures and ring buffer management.

use super::regs::*;
use core::ptr::{read_volatile, write_volatile};

#[repr(C, align(16))]
#[derive(Clone, Copy, Default)]
pub struct Trb {
    pub parameter: u64,
    pub status: u32,
    pub control: u32,
}

impl Trb {
    pub fn trb_type(&self) -> u32 {
        (self.control >> TRB_TYPE_SHIFT) & 0x3F
    }

    pub fn completion_code(&self) -> u32 {
        self.status >> 24
    }

    pub fn slot_id(&self) -> u8 {
        (self.control >> 24) as u8
    }

    pub fn cycle(&self) -> bool {
        self.control & TRB_CYCLE != 0
    }
}

pub struct ProducerRing {
    pub trbs: *mut Trb,
    pub phys: u64,
    pub count: usize,
    pub enqueue: usize,
    pub cycle: bool,
}

impl ProducerRing {
    pub fn new(trbs: *mut Trb, phys: u64, count: usize) -> Self {
        Self { trbs, phys, count, enqueue: 0, cycle: true }
    }

    pub fn push(&mut self, mut trb: Trb) -> usize {
        let idx = self.enqueue;
        if self.cycle {
            trb.control |= TRB_CYCLE;
        } else {
            trb.control &= !TRB_CYCLE;
        }
        unsafe { write_volatile(self.trbs.add(idx), trb) };
        self.enqueue += 1;
        if self.enqueue >= self.count {
            self.enqueue = 0;
            self.cycle = !self.cycle;
        }
        idx
    }
}

pub struct EventRing {
    pub trbs: *mut Trb,
    pub phys: u64,
    pub count: usize,
    pub dequeue: usize,
    pub cycle: bool,
}

impl EventRing {
    pub fn new(trbs: *mut Trb, phys: u64, count: usize) -> Self {
        Self { trbs, phys, count, dequeue: 0, cycle: true }
    }

    pub fn peek(&self) -> Option<Trb> {
        let trb = unsafe { read_volatile(self.trbs.add(self.dequeue)) };
        if trb.cycle() != self.cycle {
            return None;
        }
        Some(trb)
    }

    pub fn advance(&mut self) {
        self.dequeue += 1;
        if self.dequeue >= self.count {
            self.dequeue = 0;
            self.cycle = !self.cycle;
        }
    }
}

#[repr(C, align(16))]
pub struct ErstEntry {
    pub ring_phys: u64,
    pub ring_size: u32,
    pub _rsv: u32,
}

pub fn setup_trb(bm: u8, req: u8, val: u16, idx: u16, len: u16, data_dir: u32) -> Trb {
    let param = (len as u64) << 48
        | (idx as u64) << 32
        | (val as u64) << 16
        | ((req as u64) << 8)
        | bm as u64;
    Trb {
        parameter: param,
        status: 8,
        control: (TRB_SETUP << TRB_TYPE_SHIFT) | TRB_IDT | (data_dir << 16) | TRB_IOC,
    }
}

pub fn data_trb(buf_phys: u64, length: u32, dir_in: bool) -> Trb {
    let mut ctrl = (TRB_DATA << TRB_TYPE_SHIFT) | TRB_IOC;
    if dir_in { ctrl |= TRB_DIR_IN; }
    Trb { parameter: buf_phys, status: length, control: ctrl }
}

pub fn status_trb(dir_in: bool) -> Trb {
    let mut ctrl = (TRB_STATUS << TRB_TYPE_SHIFT) | TRB_IOC;
    if dir_in { ctrl |= TRB_DIR_IN; }
    Trb { parameter: 0, status: 0, control: ctrl }
}

pub fn normal_trb(buf_phys: u64, length: u32, dir_in: bool) -> Trb {
    let mut ctrl = (TRB_NORMAL << TRB_TYPE_SHIFT) | TRB_IOC;
    if dir_in { ctrl |= TRB_DIR_IN; }
    Trb { parameter: buf_phys, status: length, control: ctrl }
}

pub fn enable_slot_trb() -> Trb {
    Trb { parameter: 0, status: 0, control: (TRB_ENABLE_SLOT << TRB_TYPE_SHIFT) | TRB_IOC }
}

pub fn address_device_trb(input_ctx_phys: u64, slot_id: u8) -> Trb {
    Trb {
        parameter: input_ctx_phys,
        status: 0,
        control: (TRB_ADDR_DEV << TRB_TYPE_SHIFT) | TRB_IOC | ((slot_id as u32) << 24),
    }
}

pub fn config_ep_trb(input_ctx_phys: u64, slot_id: u8) -> Trb {
    Trb {
        parameter: input_ctx_phys,
        status: 0,
        control: (TRB_CONFIG_EP << TRB_TYPE_SHIFT) | TRB_IOC | ((slot_id as u32) << 24),
    }
}

unsafe impl Send for ProducerRing {}
unsafe impl Sync for ProducerRing {}
unsafe impl Send for EventRing {}
unsafe impl Sync for EventRing {}
