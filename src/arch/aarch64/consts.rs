/// AArch64 memory layout and constant definitions
/// Ported from Redox OS.

use rmm::aarch64::AArch64Arch;
use rmm::Arch;

const PML4_SHIFT: usize =
    (AArch64Arch::PAGE_LEVELS - 1) * AArch64Arch::PAGE_ENTRY_SHIFT + AArch64Arch::PAGE_SHIFT;

pub const PML4_SIZE: usize = 1 << PML4_SHIFT;

pub const USER_END_OFFSET: usize = 256 * PML4_SIZE;

pub fn kernel_heap_offset() -> usize {
    crate::KERNEL_OFFSET - PML4_SIZE
}
