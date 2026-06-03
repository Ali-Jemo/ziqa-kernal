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
/// **Key fix**: Tracks the current region index and current address in that
/// region to provide O(1) allocation time, avoiding O(n²) re-iteration on every
/// allocation call. Also skips the first `SKIP_INITIAL` frames to protect page
/// tables created by the bootloader.
pub struct BootInfoFrameAllocator {
    memory_map: &'static MemoryMap,
    current_region_idx: usize,
    current_addr: u64,
}

/// Number of initial usable frames to skip.
/// The bootloader typically uses 50-100 frames for its page tables,
/// but marks them as "Usable" in the memory map. Skipping 512 frames
/// (2 MiB) gives a safe margin.
const SKIP_INITIAL: usize = 512;

impl BootInfoFrameAllocator {
    pub unsafe fn init(memory_map: &'static MemoryMap) -> Self {
        let mut allocator = BootInfoFrameAllocator {
            memory_map,
            current_region_idx: 0,
            current_addr: 0,
        };
        // Skip the first SKIP_INITIAL frames
        for _ in 0..SKIP_INITIAL {
            allocator.allocate_frame();
        }
        allocator
    }
}

unsafe impl FrameAllocator<Size4KiB> for BootInfoFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame> {
        while self.current_region_idx < self.memory_map.len() {
            let region = &self.memory_map[self.current_region_idx];
            if region.region_type == MemoryRegionType::Usable {
                let start = region.range.start_addr();
                let end = region.range.end_addr();
                if self.current_addr < start || self.current_addr >= end {
                    self.current_addr = start;
                }
                
                // Align to 4096 page boundary
                self.current_addr = (self.current_addr + 4095) & !4095;
                
                if self.current_addr < end {
                    let frame_addr = self.current_addr;
                    self.current_addr += 4096;
                    return Some(PhysFrame::containing_address(PhysAddr::new(frame_addr)));
                }
            }
            self.current_region_idx += 1;
            self.current_addr = 0;
        }
        None
    }
}
