/// RISC-V 64 memory layout and constant definitions
/// Ported from Redox OS.

use rmm::riscv64::RiscV64Sv39Arch;
use rmm::Arch;

const PML4_SHIFT: usize =
    (RiscV64Sv39Arch::PAGE_LEVELS - 1) * RiscV64Sv39Arch::PAGE_ENTRY_SHIFT + RiscV64Sv39Arch::PAGE_SHIFT;

pub const PML4_SIZE: usize = 1 << PML4_SHIFT;

pub const USER_END_OFFSET: usize = 1 << (RiscV64Sv39Arch::PAGE_ADDRESS_SHIFT - 1);

pub fn kernel_heap_offset() -> usize {
    crate::KERNEL_OFFSET - PML4_SIZE
}
