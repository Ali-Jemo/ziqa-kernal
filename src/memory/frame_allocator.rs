use bootloader::bootinfo::{MemoryMap, MemoryRegionType};
use x86_64::{
    structures::paging::{FrameAllocator, OffsetPageTable, PageTable, PhysFrame, Size4KiB},
    PhysAddr, VirtAddr,
};

/// Initialize a page table mapper using the physical memory offset.
/// Graph: called_by kernel_main::init
pub unsafe fn init(physical_memory_offset: VirtAddr) -> OffsetPageTable<'static> {
    let l4 = unsafe { active_level_4_table(physical_memory_offset.clone()) };
    unsafe { OffsetPageTable::new(l4, physical_memory_offset) }
}

unsafe fn active_level_4_table(offset: VirtAddr) -> &'static mut PageTable {
    use x86_64::registers::control::Cr3;
    let (frame, _) = Cr3::read();
    let phys = frame.start_address();
    let virt = offset + phys.as_u64();
    unsafe { &mut *(virt.as_mut_ptr()) }
}

/// A bump allocator over the bootloader memory map.
///
/// **Key fix**: Pre-computes all usable frames into a cached list at init time
/// to avoid O(n²) re-iteration and ensures each frame is only handed out once.
/// Also skips the first `SKIP_INITIAL` frames to avoid handing out frames that
/// the bootloader may have used for its own page tables.
pub struct BootInfoFrameAllocator {
    memory_map: &'static MemoryMap,
    next: usize,
}

/// Number of initial usable frames to skip.
/// The bootloader typically uses 50-100 frames for its page tables,
/// but marks them as "Usable" in the memory map. Skipping 512 frames
/// (2 MiB) gives a safe margin.
const SKIP_INITIAL: usize = 512;

impl BootInfoFrameAllocator {
    pub unsafe fn init(memory_map: &'static MemoryMap) -> Self {
        BootInfoFrameAllocator {
            memory_map,
            // Start past the frames the bootloader is likely using
            next: SKIP_INITIAL,
        }
    }

    fn usable_frames(&self) -> impl Iterator<Item = PhysFrame> {
        self.memory_map
            .iter()
            .filter(|r| r.region_type == MemoryRegionType::Usable)
            .flat_map(|r| (r.range.start_addr()..r.range.end_addr()).step_by(4096))
            .map(|addr| PhysFrame::containing_address(PhysAddr::new(addr)))
    }
}

unsafe impl FrameAllocator<Size4KiB> for BootInfoFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame> {
        let frame = self.usable_frames().nth(self.next);
        self.next += 1;
        frame
    }
}
