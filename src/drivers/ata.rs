use crate::abi::AbiError;
use crate::drivers::block::BlockDevice;

const SECTOR_SIZE: usize = 512;

fn io_delay() {
    unsafe {
        core::arch::asm!("outb %al, $0x80", in("al") 0u8, options(att_syntax));
    }
}

fn inb(port: u16) -> u8 {
    let val: u8;
    unsafe {
        core::arch::asm!("inb %dx, %al", out("al") val, in("dx") port, options(att_syntax));
    }
    val
}

fn outb(port: u16, val: u8) {
    unsafe {
        core::arch::asm!("outb %al, %dx", in("al") val, in("dx") port, options(att_syntax));
    }
}

fn inl(port: u16) -> u32 {
    let val: u32;
    unsafe {
        core::arch::asm!("inl %dx, %eax", out("eax") val, in("dx") port, options(att_syntax));
    }
    val
}

fn outl(port: u16, val: u32) {
    unsafe {
        core::arch::asm!("outl %eax, %dx", in("eax") val, in("dx") port, options(att_syntax));
    }
}


fn ide_write(base: u16, reg: u16, val: u8) {
    outb(base + reg, val);
    io_delay();
}

fn ide_poll(base: u16) -> Result<(), AbiError> {
    for _ in 0..65536 {
        io_delay();
        let s = inb(base + 7);
        if s & 0x80 == 0 {
            return Ok(());
        }
    }
    Err(AbiError::Other("ATA poll timeout (BSY stuck)"))
}

fn ide_wait_drq(base: u16) -> Result<(), AbiError> {
    for _ in 0..65536 {
        io_delay();
        let s = inb(base + 7);
        if s & 0x80 == 0 && s & 0x08 != 0 {
            return Ok(());
        }
        if s & 0x80 == 0 && s & 0x01 != 0 {
            return Err(AbiError::Other("ATA error during read/write"));
        }
    }
    Err(AbiError::Other("ATA timeout waiting for DRQ"))
}

pub struct AtaBlock {
    base: u16,
    ctrl: u16,
    total_sectors: u64,
}

impl AtaBlock {
    pub fn new() -> Result<Self, AbiError> {
        let channels = [(0x1F0u16, 0x3F6u16), (0x170u16, 0x376u16)];

        for &(base, ctrl) in &channels {
            for drv in [0xE0u8, 0xF0u8] {
                if base == 0x1F0 && drv == 0xE0 {
                    continue;
                }

                outb(ctrl, 2);
                io_delay();

                ide_write(base, 6, drv);
                ide_write(base, 2, 0);
                ide_write(base, 3, 0);
                ide_write(base, 4, 0);
                ide_write(base, 5, 0);

                outb(base + 7, 0xEC);
                io_delay();

                let st = inb(base + 7);
                if st == 0 {
                    continue;
                }

                match ide_poll(base) {
                    Ok(()) => {}
                    Err(_) => continue,
                }

                let st = inb(base + 7);
                if st & 0x01 != 0 || st & 0x08 == 0 {
                    continue;
                }

                let mut ident = alloc::boxed::Box::new([0u32; 128]);
                for word in ident.iter_mut() {
                    *word = inl(base + 0);
                }

                let sectors = u64::from(ident[61]) << 32 | u64::from(ident[60]);
                let total_sectors = if sectors > 0 { sectors } else { 0x200000 / 512 };

                return Ok(Self {
                    base,
                    ctrl,
                    total_sectors,
                });
            }
        }

        Err(AbiError::Other("No ATA device found"))
    }

    fn rw_sectors(
        &self,
        sector: u64,
        count: u32,
        buf: &mut [u8],
        write: bool,
    ) -> Result<(), AbiError> {
        if sector + count as u64 > self.total_sectors {
            return Err(AbiError::OutOfBounds);
        }
        let buf_len = count as usize * SECTOR_SIZE;
        if buf.len() < buf_len {
            return Err(AbiError::OutOfBounds);
        }

        let lba = sector;
        let drv_head = 0xE0u8 | ((lba >> 24) & 0x0F) as u8;

        outb(self.ctrl, 2);
        io_delay();
        ide_poll(self.base)?;

        ide_write(self.base, 6, drv_head);
        ide_write(self.base, 2, count as u8);
        ide_write(self.base, 3, (lba & 0xFF) as u8);
        ide_write(self.base, 4, ((lba >> 8) & 0xFF) as u8);
        ide_write(self.base, 5, ((lba >> 16) & 0xFF) as u8);

        outb(self.base + 7, if write { 0x30 } else { 0x20 });
        io_delay();

        for s in 0..count as usize {
            ide_wait_drq(self.base)?;

            let offset = s * SECTOR_SIZE;
            if write {
                for i in 0..128 {
                    let byte_off = offset + i * 4;
                    let val = if byte_off + 4 <= buf.len() {
                        u32::from(buf[byte_off])
                            | (u32::from(buf[byte_off + 1]) << 8)
                            | (u32::from(buf[byte_off + 2]) << 16)
                            | (u32::from(buf[byte_off + 3]) << 24)
                    } else {
                        0
                    };
                    outl(self.base, val);
                }
                outb(self.base + 7, 0xE7);
                io_delay();
                ide_poll(self.base)?;
            } else {
                for i in 0..128 {
                    let byte_off = offset + i * 4;
                    let val = inl(self.base);
                    if byte_off + 4 <= buf.len() {
                        buf[byte_off] = val as u8;
                        buf[byte_off + 1] = (val >> 8) as u8;
                        buf[byte_off + 2] = (val >> 16) as u8;
                        buf[byte_off + 3] = (val >> 24) as u8;
                    }
                }
            }
        }
        Ok(())
    }
}

impl BlockDevice for AtaBlock {
    fn read_sectors(&self, sector: u64, count: u32, buf: &mut [u8]) -> Result<(), AbiError> {
        self.rw_sectors(sector, count, buf, false)
    }

    fn write_sectors(&self, sector: u64, count: u32, buf: &[u8]) -> Result<(), AbiError> {
        let mut tmp = alloc::vec![0u8; (count as usize) * SECTOR_SIZE];
        tmp[..buf.len()].copy_from_slice(buf);
        self.rw_sectors(sector, count, &mut tmp, true)
    }

    fn total_sectors(&self) -> u64 {
        self.total_sectors
    }
}

pub struct AtaDriver;

impl crate::drivers::device_manager::Driver for AtaDriver {
    fn name(&self) -> &str { "ATA / IDE Controller" }
    fn pci_match(&self, device: &crate::drivers::pci::PciDevice) -> bool {
        (device.class == 0x01 && device.subclass == 0x01) ||
        (device.vendor_id == 0x8086 && device.device_id == 0x7010)
    }
    fn init(&self, device: &crate::drivers::pci::PciDevice) -> Result<(), ()> {
        crate::println!("[ATA] Probing IDE/ATA controller at {:02X}:{:02X}.{}", device.bus, device.dev, device.func);
        match AtaBlock::new() {
            Ok(disk) => {
                let disk: alloc::sync::Arc<dyn BlockDevice> = alloc::sync::Arc::new(disk);
                crate::drivers::block_registry::register("hda", "ata", disk);
                Ok(())
            }
            Err(e) => {
                crate::println!("[ATA] Probing failed: {:?}", e);
                Err(())
            }
        }
    }
}

pub fn register() {
    crate::drivers::device_manager::DEVICE_MANAGER.lock().register_driver(alloc::boxed::Box::new(AtaDriver));
}
