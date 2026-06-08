/// ARM Generic Interrupt Controller (GIC) driver
/// Ported from Redox OS.

use core::ptr::{read_volatile, write_volatile};

const GICD_CTLR: u32 = 0x000;
const GICD_TYPER: u32 = 0x004;
const GICD_ISENABLER: u32 = 0x100;
const GICD_ICENABLER: u32 = 0x180;
const GICD_IPRIORITY: u32 = 0x400;
const GICD_ITARGETSR: u32 = 0x800;
const GICD_ICFGR: u32 = 0xC00;

const GICC_CTLR: u32 = 0x0000;
const GICC_PMR: u32 = 0x0004;
const GICC_IAR: u32 = 0x000C;
const GICC_EOIR: u32 = 0x0010;

pub struct GicDistributor {
    base: usize,
    nirqs: u32,
}

pub struct GicCpuInterface {
    base: usize,
}

pub struct Gic {
    pub dist: GicDistributor,
    pub cpu: GicCpuInterface,
}

impl Gic {
    pub fn new(dist_base: usize, cpu_base: usize) -> Self {
        let mut gic = Self {
            dist: GicDistributor {
                base: dist_base,
                nirqs: 0,
            },
            cpu: GicCpuInterface { base: cpu_base },
        };
        gic.init();
        gic
    }

    pub fn init(&mut self) {
        unsafe {
            // Disable distributor
            write_volatile((self.dist.base + GICD_CTLR as usize) as *mut u32, 0);

            // Read number of interrupts
            let typer = read_volatile((self.dist.base + GICD_TYPER as usize) as *const u32);
            self.dist.nirqs = ((typer & 0x1F) + 1) * 32;

            // Set all SPI interrupts to level-triggered
            for irq in (32..self.dist.nirqs).step_by(16) {
                write_volatile((self.dist.base + GICD_ICFGR as usize + (irq / 16) * 4) as *mut u32, 0);
            }

            // Disable all SPIs
            for irq in (32..self.dist.nirqs).step_by(32) {
                write_volatile((self.dist.base + GICD_ICENABLER as usize + (irq / 32) * 4) as *mut u32, 0xFFFFFFFF);
            }

            // Affine all SPIs to CPU0
            for irq in 0..self.dist.nirqs {
                if irq > 31 {
                    let ext = GICD_ITARGETSR + 4 * (irq / 4);
                    let int_off = irq % 4;
                    let mut val = read_volatile((self.dist.base + ext as usize) as *const u32);
                    val |= 0b1 << (8 * int_off);
                    write_volatile((self.dist.base + ext as usize) as *mut u32, val);
                }
            }

            // Enable distributor
            write_volatile((self.dist.base + GICD_CTLR as usize) as *mut u32, 0x3);

            // Enable CPU interface
            write_volatile((self.cpu.base + GICC_CTLR as usize) as *mut u32, 1);
            // Set priority mask
            write_volatile((self.cpu.base + GICC_PMR as usize) as *mut u32, 0xFF);
        }
    }

    pub fn enable_irq(&self, irq: u32) {
        let offset = GICD_ISENABLER + 4 * (irq / 32);
        let shift = 1 << (irq % 32);
        unsafe {
            write_volatile((self.dist.base + offset as usize) as *mut u32, shift);
        }
    }

    pub fn disable_irq(&self, irq: u32) {
        let offset = GICD_ICENABLER + 4 * (irq / 32);
        let shift = 1 << (irq % 32);
        unsafe {
            write_volatile((self.dist.base + offset as usize) as *mut u32, shift);
        }
    }

    pub fn ack_irq(&self) -> u32 {
        unsafe { read_volatile((self.cpu.base + GICC_IAR as usize) as *const u32) & 0x1FF }
    }

    pub fn eoi_irq(&self, irq: u32) {
        unsafe { write_volatile((self.cpu.base + GICC_EOIR as usize) as *mut u32, irq) };
    }
}
