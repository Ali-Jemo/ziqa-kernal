/// Memory subsystem for ZiqaKernel
/// Re-exports existing frame allocator and heap, plus adds
/// types needed by the process manager and ELF loader.
pub mod frame_allocator;
pub mod heap;
pub mod heapstats;
pub mod paging;

pub use frame_allocator::BootInfoFrameAllocator;
pub use paging::{AddressSpace, MemoryRegion, MemoryRegionFlags};
pub use x86_64::VirtAddr;

use spin::Mutex;
pub static FRAME_ALLOCATOR: Mutex<Option<BootInfoFrameAllocator>> = Mutex::new(None);

/// Page size on x86_64
pub const PAGE_SIZE: usize = 4096;
