//! xHCI slot and endpoint context builders.

use core::ptr::write_bytes;

pub struct InputContext {
    pub virt: *mut u8,
    pub phys: u64,
    pub ctx_size: usize,
}

impl InputContext {
    pub fn new(virt: *mut u8, phys: u64, ctx_size: usize) -> Self {
        unsafe { write_bytes(virt, 0, ctx_size * 32) };
        Self { virt, phys, ctx_size }
    }

    fn ctx_ptr(&self, index: usize) -> *mut u32 {
        unsafe { self.virt.add(index * self.ctx_size) as *mut u32 }
    }

    pub fn set_add_flags(&self, slot: bool, ep_mask: u32) {
        let ctrl = self.ctx_ptr(0);
        let mut dw0 = 0u32;
        if slot { dw0 |= 1; }
        unsafe {
            *ctrl = dw0;
            *ctrl.add(1) = ep_mask;
        }
    }

    pub fn init_slot(&self, ctx_index: usize, route_string: u32, speed: u32, root_port: u8, parent_slot_id: u8, parent_port: u8, hub_port_count: u8) {
        let ctx = self.ctx_ptr(ctx_index);
        unsafe {
            *ctx = (route_string & 0xFFFFF) | ((speed & 0xF) << 20);
            *ctx.add(1) = (root_port as u32)
                | ((hub_port_count as u32) << 8)
                | ((parent_port as u32) << 24)
                | ((parent_slot_id as u32) << 27);
            *ctx.add(3) = 1;
        }
    }

    pub fn init_ep0(&self, ctx_index: usize, max_packet: u16, transfer_ring_phys: u64) {
        let ctx = self.ctx_ptr(ctx_index);
        unsafe {
            *ctx.add(1) = (4 << 3) | ((max_packet as u32) << 16);
            *ctx.add(2) = transfer_ring_phys as u32 | 1;
            *ctx.add(3) = (transfer_ring_phys >> 32) as u32;
            *ctx.add(4) = 8;
        }
    }

    pub fn init_interrupt_in(
        &self,
        ctx_index: usize,
        max_packet: u16,
        interval: u8,
        transfer_ring_phys: u64,
    ) {
        let ctx = self.ctx_ptr(ctx_index);
        unsafe {
            *ctx.add(1) = (7 << 3) | (3 << 1) | ((max_packet as u32) << 16);
            *ctx.add(2) = transfer_ring_phys as u32 | 1;
            *ctx.add(3) = (transfer_ring_phys >> 32) as u32;
            *ctx.add(4) = 8;
            *ctx.add(7) = interval as u32;
        }
    }

    pub fn init_bulk_ep(
        &self,
        ctx_index: usize,
        max_packet: u16,
        transfer_ring_phys: u64,
        dir_in: bool,
    ) {
        let ctx = self.ctx_ptr(ctx_index);
        unsafe {
            // EP type = 2 (Bulk) at bits 3..5, D2 bit at bit 5 for direction
            let mut dw1 = (2u32 << 3) | ((max_packet as u32) << 16);
            if dir_in {
                dw1 |= 1u32 << 5;
            }
            *ctx.add(1) = dw1;
            *ctx.add(2) = transfer_ring_phys as u32 | 1;
            *ctx.add(3) = (transfer_ring_phys >> 32) as u32;
            *ctx.add(4) = 8;
        }
    }
}

pub struct DeviceContext {
    pub virt: *mut u8,
    pub phys: u64,
    pub ctx_size: usize,
}

impl DeviceContext {
    pub fn new(virt: *mut u8, phys: u64, ctx_size: usize) -> Self {
        unsafe { write_bytes(virt, 0, ctx_size * 32) };
        Self { virt, phys, ctx_size }
    }
}

pub fn ep0_max_packet(speed: u32) -> u16 {
    match speed {
        super::regs::SPEED_SUPER => 512,
        super::regs::SPEED_HIGH => 64,
        _ => 8,
    }
}

pub fn port_speed_to_ctx(speed: u32) -> u32 {
    match speed {
        super::regs::SPEED_LOW => 2,
        super::regs::SPEED_FULL => 1,
        super::regs::SPEED_HIGH => 3,
        super::regs::SPEED_SUPER => 4,
        _ => 1,
    }
}
