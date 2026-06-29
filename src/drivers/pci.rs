extern crate alloc;
use alloc::vec::Vec;
use spin::Mutex;
use x86_64::instructions::port::Port;
use pci_types::{PciAddress, PciHeader, ConfigRegionAccess, CommandRegister};

pub fn pci_config_read(bus: u8, dev: u8, func: u8, offset: u8) -> u32 {
    let addr = 0x8000_0000u32
        | ((bus  as u32) << 16)
        | ((dev  as u32) << 11)
        | ((func as u32) <<  8)
        | (offset as u32 & 0xFC);
    unsafe {
        let mut a: Port<u32> = Port::new(0xCF8);
        let mut d: Port<u32> = Port::new(0xCFC);
        a.write(addr);
        d.read()
    }
}

pub fn pci_config_write(bus: u8, dev: u8, func: u8, offset: u8, val: u32) {
    let addr = 0x8000_0000u32
        | ((bus  as u32) << 16)
        | ((dev  as u32) << 11)
        | ((func as u32) <<  8)
        | (offset as u32 & 0xFC);
    unsafe {
        let mut a: Port<u32> = Port::new(0xCF8);
        let mut d: Port<u32> = Port::new(0xCFC);
        a.write(addr);
        d.write(val);
    }
}

#[derive(Clone, Copy)]
pub struct PciAccess;

impl ConfigRegionAccess for PciAccess {
    unsafe fn read(&self, address: PciAddress, offset: u16) -> u32 {
        pci_config_read(address.bus(), address.device(), address.function(), offset as u8)
    }

    unsafe fn write(&self, address: PciAddress, offset: u16, value: u32) {
        pci_config_write(address.bus(), address.device(), address.function(), offset as u8, value)
    }
}

#[derive(Debug, Clone)]
pub struct PciDevice {
    pub address: PciAddress,
    pub bus: u8,
    pub dev: u8,
    pub func: u8,
    pub vendor_id: u16,
    pub device_id: u16,
    pub class: u8,
    pub subclass: u8,
    pub prog_if: u8,
    pub revision: u8,
    pub header_type: u8,
    pub interrupt_line: u8,
    pub interrupt_pin: u8,
    pub bars: [u32; 6],
}

pub static PCI_DEVICES: Mutex<Vec<PciDevice>> = Mutex::new(Vec::new());

pub fn find_device(vendor: u16, device: u16) -> Option<PciDevice> {
    PCI_DEVICES.lock().iter().find(|d| d.vendor_id == vendor && d.device_id == device).cloned()
}

pub fn find_by_class(class: u8, subclass: u8) -> Vec<PciDevice> {
    PCI_DEVICES.lock().iter().filter(|d| d.class == class && d.subclass == subclass).cloned().collect()
}

pub fn enable_bus_mastering(addr: PciAddress) {
    let mut header = PciHeader::new(addr);
    header.update_command(PciAccess, |cmd| cmd | CommandRegister::BUS_MASTER_ENABLE);
}

pub fn enable_io_space(addr: PciAddress) {
    let mut header = PciHeader::new(addr);
    header.update_command(PciAccess, |cmd| cmd | CommandRegister::IO_ENABLE);
}

pub fn enable_memory_space(addr: PciAddress) {
    let mut header = PciHeader::new(addr);
    header.update_command(PciAccess, |cmd| cmd | CommandRegister::MEMORY_ENABLE);
}

pub fn bar_address(bar_val: u32) -> (u64, bool) {
    let is_io = (bar_val & 1) != 0;
    let address = if is_io {
        (bar_val & !0b11) as u64
    } else {
        (bar_val & !0b1111) as u64
    };
    (address, is_io)
}
pub fn device_count() -> usize {
    PCI_DEVICES.lock().len()
}

pub fn class_name(class: u8) -> &'static str {
    match class {
        0x01 => "Storage",
        0x02 => "Network",
        0x03 => "Display",
        0x04 => "Multimedia",
        0x06 => "Bridge",
        0x0C => "Serial Bus",
        _ => "Other",
    }
}

// ponytail: exhaustive scan does 8192 port reads blocking CPU, DFS scans only valid bridges
fn check_bus(bus: u8, access: &PciAccess, devices: &mut Vec<PciDevice>, count: &mut u32) {
    for device in 0..=31 {
        let addr = PciAddress::new(0, bus, device, 0);
        let header = PciHeader::new(addr);
        let (vendor_id, _device_id) = header.id(access);
        if vendor_id == 0xFFFF {
            continue;
        }

        let is_multi = header.has_multiple_functions(access);
        let max_func = if is_multi { 8 } else { 1 };

        for func in 0..max_func {
            let func_addr = PciAddress::new(0, bus, device, func);
            let func_header = PciHeader::new(func_addr);
            let (vid, did) = func_header.id(access);
            if vid == 0xFFFF {
                continue;
            }
            *count += 1;

            let (revision, class, subclass, prog_if) = func_header.revision_and_class(access);
            let intr_data = unsafe { access.read(func_addr, 0x3C) };
            let line = (intr_data & 0xFF) as u8;
            let pin = ((intr_data >> 8) & 0xFF) as u8;

            let mut bars = [0u32; 6];
            for slot in 0..6 {
                let offset = 0x10 + slot as u16 * 4;
                bars[slot] = unsafe { access.read(func_addr, offset) };
            }
            let header_type = unsafe { (access.read(func_addr, 0x0C) >> 16) as u8 };

            devices.push(PciDevice {
                address: func_addr,
                bus,
                dev: device,
                func,
                vendor_id: vid,
                device_id: did,
                class,
                subclass,
                prog_if,
                revision,
                header_type,
                interrupt_line: line,
                interrupt_pin: pin,
                bars,
            });

            // Recurse into PCI-to-PCI bridges
            if class == 0x06 && subclass == 0x04 {
                let bus_numbers = unsafe { access.read(func_addr, 0x18) };
                let secondary_bus = ((bus_numbers >> 8) & 0xFF) as u8;
                if secondary_bus != 0 && secondary_bus > bus {
                    check_bus(secondary_bus, access, devices, count);
                }
            }
        }
    }
}

pub fn init() {
    let access = PciAccess;
    let mut devices = Vec::new();
    let mut count = 0u32;

    check_bus(0, &access, &mut devices, &mut count);

    *PCI_DEVICES.lock() = devices;
    crate::println!("[pci] {} device(s) found", count);
}
