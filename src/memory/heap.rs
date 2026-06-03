use linked_list_allocator::LockedHeap;
use x86_64::{
    structures::paging::{
        mapper::MapToError, FrameAllocator, Mapper, Page, PageTableFlags, Size4KiB,
    },
    VirtAddr,
};

pub const HEAP_START: usize = 0x_4444_4444_0000;
pub const HEAP_SIZE: usize = 32 * 1024 * 1024; // 32 MiB

struct TrackedAllocator(LockedHeap);

unsafe impl core::alloc::GlobalAlloc for TrackedAllocator {
    unsafe fn alloc(&self, layout: core::alloc::Layout) -> *mut u8 {
        let ptr = self.0.alloc(layout);
        if !ptr.is_null() {
            crate::memory::heapstats::record_alloc(layout.size() as u64);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: core::alloc::Layout) {
        self.0.dealloc(ptr, layout);
        crate::memory::heapstats::record_dealloc(layout.size() as u64);
    }
}

#[global_allocator]
static ALLOCATOR: TrackedAllocator = TrackedAllocator(LockedHeap::empty());


pub fn init_heap(
    mapper: &mut impl Mapper<Size4KiB>,
    frame_allocator: &mut impl FrameAllocator<Size4KiB>,
) -> Result<(), MapToError<Size4KiB>> {
    let page_range = {
        let start = VirtAddr::new(HEAP_START as u64);
        let end = start.clone() + HEAP_SIZE as u64 - 1u64;
        Page::range_inclusive(
            Page::containing_address(start),
            Page::containing_address(end),
        )
    };

    for page in page_range {
        let frame = frame_allocator
            .allocate_frame()
            .ok_or(MapToError::FrameAllocationFailed)?;
        let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;
        unsafe { mapper.map_to(page, frame, flags, frame_allocator)?.flush() };
    }

    unsafe { ALLOCATOR.0.lock().init(HEAP_START as *mut u8, HEAP_SIZE) };
    Ok(())
}
