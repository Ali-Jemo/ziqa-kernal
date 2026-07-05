use core::ptr::{read_volatile, write_volatile};

const IA32_APIC_BASE_MSR: u32 = 0x1B;

pub const LAPIC_ID: usize = 0x020;
pub const LAPIC_VERSION: usize = 0x030;
pub const LAPIC_TPR: usize = 0x080;
pub const LAPIC_APR: usize = 0x090;
pub const LAPIC_PPR: usize = 0x0A0;
pub const LAPIC_EOI: usize = 0x0B0;
pub const LAPIC_LDR: usize = 0x0D0;
pub const LAPIC_SVR: usize = 0x0F0;
pub const LAPIC_ISR_BASE: usize = 0x100;
pub const LAPIC_TMR_BASE: usize = 0x180;
pub const LAPIC_IRR_BASE: usize = 0x200;
pub const LAPIC_ICR_LOW: usize = 0x300;
pub const LAPIC_ICR_HIGH: usize = 0x310;
pub const LAPIC_LVT_TIMER: usize = 0x320;
pub const LAPIC_LVT_THERMAL: usize = 0x330;
pub const LAPIC_LVT_PERFMON: usize = 0x340;
pub const LAPIC_LVT_LINT0: usize = 0x350;
pub const LAPIC_LVT_LINT1: usize = 0x360;
pub const LAPIC_LVT_ERROR: usize = 0x370;
pub const LAPIC_TIMER_INITCNT: usize = 0x380;
pub const LAPIC_TIMER_CURCNT: usize = 0x390;
pub const LAPIC_TIMER_DIV: usize = 0x3E0;

const IOAPIC_IOREGSEL: usize = 0x00;
const IOAPIC_IOWIN: usize = 0x10;

const IOAPIC_VERSION: u32 = 0x01;
const IOAPIC_REDIR_TBL: u32 = 0x10;

pub static mut LAPIC_VADDR: u64 = 0;
pub static mut IOAPIC_VADDR: u64 = 0;

pub static mut IOAPIC_GSI_BASE: u32 = 0;
pub static mut IOAPIC_COUNT: u32 = 0;

#[derive(Debug, Clone, Copy)]
pub enum IcrDeliveryMode {
    Fixed = 0b000,
    LowestPriority = 0b001,
    SMI = 0b010,
    NMI = 0b100,
    INIT = 0b101,
    StartUp = 0b110,
}

#[derive(Debug, Clone, Copy)]
pub enum IcrDestShorthand {
    None = 0b00,
    Self_ = 0b01,
    AllIncludingSelf = 0b10,
    AllExcludingSelf = 0b11,
}

pub fn init(acpi: &crate::drivers::acpi::AcpiInfo) {
    let phys_offset = get_phys_offset();

    let lapic_phys = acpi.local_apic_address;
    let lapic_virt = phys_offset + lapic_phys;
    unsafe {
        LAPIC_VADDR = lapic_virt;
    }

    let ioapic_phys = acpi.io_apic_address as u64;
    let ioapic_virt = phys_offset + ioapic_phys;
    unsafe {
        IOAPIC_VADDR = ioapic_virt;
        IOAPIC_GSI_BASE = acpi.io_apic_gsi_base;
        IOAPIC_COUNT = read_ioapic(IOAPIC_VERSION) >> 16;
    }

    crate::klog!(
        crate::klog::Level::Info,
        "APIC: Local APIC at {:#x} (virt {:#x}), I/O APIC at {:#x} (virt {:#x}) ({} entries)",
        lapic_phys, lapic_virt, ioapic_phys, ioapic_virt, unsafe { IOAPIC_COUNT },
    );
}

pub fn enable() {
    write_lapic(LAPIC_TPR, 0);
    let svr = read_lapic(LAPIC_SVR);
    write_lapic(LAPIC_SVR, svr | 0x100 | SPURIOUS_VECTOR as u32);
    crate::klog!(crate::klog::Level::Info, "APIC: enabled (SVR={:#x})", svr | 0x100 | SPURIOUS_VECTOR as u32);
}

pub fn disable_pic() {
    unsafe {
        use x86_64::instructions::port::Port;
        let mut cmd: Port<u8> = Port::new(0x20);
        let mut data: Port<u8> = Port::new(0x21);
        cmd.write(0xD1);
        data.write(0xED);
        cmd.write(0x20);
        data.write(0xFF);
        let mut cmd2: Port<u8> = Port::new(0xA0);
        let mut data2: Port<u8> = Port::new(0xA1);
        cmd2.write(0xD1);
        data2.write(0xED);
        cmd2.write(0x20);
        data2.write(0xFF);
    }
}

pub fn read_lapic(reg: usize) -> u32 {
    unsafe {
        let addr = (LAPIC_VADDR + reg as u64) as *const u32;
        read_volatile(addr)
    }
}

pub fn write_lapic(reg: usize, val: u32) {
    unsafe {
        let addr = (LAPIC_VADDR + reg as u64) as *mut u32;
        write_volatile(addr, val);
    }
}

pub fn eoi() {
    write_lapic(LAPIC_EOI, 0);
}

pub fn lapic_id() -> u32 {
    read_lapic(LAPIC_ID) >> 24
}

pub fn read_ioapic(reg: u32) -> u32 {
    unsafe {
        let select_addr = (IOAPIC_VADDR + IOAPIC_IOREGSEL as u64) as *mut u32;
        let win_addr = (IOAPIC_VADDR + IOAPIC_IOWIN as u64) as *mut u32;
        write_volatile(select_addr, reg);
        read_volatile(win_addr)
    }
}

pub fn write_ioapic(reg: u32, val: u32) {
    unsafe {
        let select_addr = (IOAPIC_VADDR + IOAPIC_IOREGSEL as u64) as *mut u32;
        let win_addr = (IOAPIC_VADDR + IOAPIC_IOWIN as u64) as *mut u32;
        write_volatile(select_addr, reg);
        write_volatile(win_addr, val);
    }
}

pub fn redirect_irq(irq: u8, vector: u8, apic_id: u32) {
    let entry = IOAPIC_REDIR_TBL + (irq as u32) * 2;
    let lo = vector as u32 | (0 << 8) | (0 << 11) | (0 << 13) | (0 << 15) | (0 << 16);
    let hi = (apic_id as u32) << 24;
    write_ioapic(entry, lo);
    write_ioapic(entry + 1, hi);
    crate::klog!(
        crate::klog::Level::Info,
        "IOAPIC: IRQ {} -> vector {} (dest APIC {})",
        irq, vector, apic_id,
    );
}

pub fn mask_irq(irq: u8) {
    let entry = IOAPIC_REDIR_TBL + (irq as u32) * 2;
    let lo = read_ioapic(entry);
    write_ioapic(entry, lo | (1 << 16));
}

pub fn unmask_irq(irq: u8) {
    let entry = IOAPIC_REDIR_TBL + (irq as u32) * 2;
    let lo = read_ioapic(entry);
    write_ioapic(entry, lo & !(1 << 16));
}

pub fn send_init_ipi(apic_id: u32) {
    write_lapic(LAPIC_ICR_HIGH, apic_id << 24);
    write_lapic(
        LAPIC_ICR_LOW,
        0x4000 | (IcrDeliveryMode::INIT as u32) << 8 | (1 << 14),
    );
    wait_icr_idle();
}

pub fn send_sipi(apic_id: u32, vector: u8) {
    write_lapic(LAPIC_ICR_HIGH, apic_id << 24);
    write_lapic(
        LAPIC_ICR_LOW,
        (vector as u32) | (IcrDeliveryMode::StartUp as u32) << 8,
    );
    wait_icr_idle();
}

pub fn send_ipi_fixed(apic_id: u32, vector: u8) {
    write_lapic(LAPIC_ICR_HIGH, apic_id << 24);
    write_lapic(
        LAPIC_ICR_LOW,
        (vector as u32) | (IcrDeliveryMode::Fixed as u32) << 8,
    );
    wait_icr_idle();
}

pub fn send_ipi_broadcast(vector: u8, shorthand: IcrDestShorthand) {
    write_lapic(
        LAPIC_ICR_LOW,
        (vector as u32)
            | (IcrDeliveryMode::Fixed as u32) << 8
            | (shorthand as u32) << 18,
    );
    wait_icr_idle();
}

fn wait_icr_idle() {
    while read_lapic(LAPIC_ICR_LOW) & (1 << 12) != 0 {
        core::hint::spin_loop();
    }
}

pub fn calibrate_timer(pit_ticks_ms: u64) -> u32 {
    write_lapic(LAPIC_TIMER_DIV, 0x03);
    write_lapic(LAPIC_LVT_TIMER, TIMER_VECTOR as u32 | (3 << 17));
    write_lapic(LAPIC_TIMER_INITCNT, 0xFFFFFFFF);

    // Spin delay for rough calibration. Cannot use PIT-based waiting
    // because TIMER.lock() deadlocks against the timer ISR.
    let loop_count = pit_ticks_ms * 500_000;
    for _ in 0..loop_count {
        core::hint::spin_loop();
    }

    write_lapic(LAPIC_LVT_TIMER, 1 << 16);
    let count = read_lapic(LAPIC_TIMER_CURCNT);
    0xFFFFFFFF - count
}

pub fn start_periodic_timer(init_count: u32) {
    write_lapic(LAPIC_TIMER_DIV, 0x03);
    write_lapic(LAPIC_LVT_TIMER, TIMER_VECTOR as u32 | (1 << 17));
    write_lapic(LAPIC_TIMER_INITCNT, init_count);
    crate::klog!(
        crate::klog::Level::Info,
        "APIC: periodic timer started (init_count={})",
        init_count,
    );
}

#[inline(always)]
pub fn enable_lapic_in_bsp() {
    let msr_val = read_ia32_apic_base();
    write_ia32_apic_base(msr_val | 0x800);
}

fn read_ia32_apic_base() -> u64 {
    let hi: u32;
    let lo: u32;
    unsafe {
        core::arch::asm!(
            "rdmsr",
            in("ecx") IA32_APIC_BASE_MSR,
            out("eax") lo,
            out("edx") hi,
            options(nostack, preserves_flags),
        );
    }
    (hi as u64) << 32 | lo as u64
}

fn write_ia32_apic_base(val: u64) {
    let lo = val as u32;
    let hi = (val >> 32) as u32;
    unsafe {
        core::arch::asm!(
            "wrmsr",
            in("ecx") IA32_APIC_BASE_MSR,
            in("eax") lo,
            in("edx") hi,
            options(nostack, preserves_flags),
        );
    }
}

fn get_phys_offset() -> u64 {
    let guard = crate::BOOT_INFO.lock();
    guard
        .as_ref()
        .expect("BOOT_INFO not initialized before APIC init")
        .physical_memory_offset
}

pub const SPURIOUS_VECTOR: u8 = 0x30;
pub const TIMER_VECTOR: u8 = 0x20;
pub const ERROR_VECTOR: u8 = 0x31;

pub const IPI_RESCHEDULE_VECTOR: u8 = 0x34;
pub const IPI_TLB_SHOOTDOWN_VECTOR: u8 = 0x35;
pub const IPI_HALT_VECTOR: u8 = 0x36;

