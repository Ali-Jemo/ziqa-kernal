use alloc::vec::Vec;
use core::sync::atomic::{AtomicUsize, Ordering};
use x86_64::{
    structures::paging::{PhysFrame, Size4KiB},
    VirtAddr,
};
use spin::Mutex;
use crate::memory::PAGE_SIZE;
use super::tier::CompressionTier;

#[derive(Debug, Clone, Copy)]
pub struct CompressedPageLocation {
    pub phys_frame: PhysFrame<Size4KiB>,
    pub tier: CompressionTier,
    pub original_size: usize,
}

pub struct CompressedPageStore {
    allocated_frames: Mutex<Vec<PhysFrame<Size4KiB>>>,
    page_map: Mutex<alloc::collections::BTreeMap<u64, CompressedPageLocation>>,
}

impl CompressedPageStore {
    pub const fn new() -> Self {
        Self {
            allocated_frames: Mutex::new(Vec::new()),
            page_map: Mutex::new(alloc::collections::BTreeMap::new()),
        }
    }
    
    pub fn store(&self, vaddr: VirtAddr, data: &[u8], tier: CompressionTier) -> bool {
        let page_start = vaddr.as_u64() & !(PAGE_SIZE as u64 - 1);
        let mut page_map = self.page_map.lock();
        
        let mut frame_allocator = crate::memory::FRAME_ALLOCATOR.lock();
        let frame_alloc = frame_allocator.as_mut().expect("FRAME_ALLOCATOR not initialized");
        let phys_frame = match frame_alloc.allocate_frame() {
            Some(f) => f,
            None => { return false; }
        };
        drop(frame_allocator);
        
        let phys_addr = phys_frame.start_address();
        let virt_addr = crate::memory::paging::phys_offset() + phys_addr.as_u64();
        let dst_ptr = virt_addr.as_mut_ptr::<u8>();
        
        unsafe {
            core::ptr::copy_nonoverlapping(data.as_ptr(), dst_ptr, data.len());
        }
        
        page_map.insert(page_start, CompressedPageLocation {
            phys_frame,
            tier,
            original_size: data.len(), // Actually original is PAGE_SIZE
        });
        
        self.allocated_frames.lock().push(phys_frame);
        true
    }
    
    pub fn is_compressed(&self, vaddr: VirtAddr) -> bool {
        let page_start = vaddr.as_u64() & !(PAGE_SIZE as u64 - 1);
        self.page_map.lock().contains_key(&page_start)
    }
    
    pub fn retrieve(&self, _vaddr: VirtAddr) -> Option<Vec<u8>> {
        None // Placeholder for Stage 2
    }
    
    pub fn release(&self, _vaddr: VirtAddr) {
        // Placeholder for Stage 2
    }
}
