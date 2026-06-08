/// RISC-V 64 IPI support (via SBI or CLINT)
/// Ported from Redox OS.

#[derive(Clone, Copy, Debug)]
#[repr(u8)]
pub enum IpiKind {
    Wakeup = 0x40,
    Tlb = 0x41,
}

#[derive(Clone, Copy, Debug)]
#[repr(u8)]
pub enum IpiTarget {
    Other = 3,
}

pub fn ipi(_kind: IpiKind, _target: IpiTarget) {
    // TODO: implement IPI via SBI or CLINT
}

pub fn ipi_single(_kind: IpiKind, _target: &crate::percpu::PerCpu) {
    // TODO: implement IPI via SBI or CLINT
}
