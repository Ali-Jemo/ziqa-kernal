use alloc::vec::Vec;
use x86_64::{
    structures::paging::{PhysFrame, Size4KiB, FrameAllocator},
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
    pub compressed_len: usize,
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
            original_size: PAGE_SIZE, // The page was PAGE_SIZE before compression
            compressed_len: data.len(),
        });
        
        self.allocated_frames.lock().push(phys_frame);
        true
    }
    
    /// Look up the compression metadata for a given virtual address.
    /// Used by the page-fault handler to find out which tier and how
    /// large the original page was.
    pub fn get_location(&self, vaddr: VirtAddr) -> Option<CompressedPageLocation> {
        let page_start = vaddr.as_u64() & !(PAGE_SIZE as u64 - 1);
        self.page_map.lock().get(&page_start).copied()
    }
    
    pub fn is_compressed(&self, vaddr: VirtAddr) -> bool {
        let page_start = vaddr.as_u64() & !(PAGE_SIZE as u64 - 1);
        self.page_map.lock().contains_key(&page_start)
    }
    
    /// Read the compressed bytes back from the physical frame we stored them in.
    pub fn retrieve(&self, vaddr: VirtAddr) -> Option<Vec<u8>> {
        let page_start = vaddr.as_u64() & !(PAGE_SIZE as u64 - 1);
        let page_map = self.page_map.lock();
        let location = page_map.get(&page_start)?;
        
        let phys_addr = location.phys_frame.start_address();
        let virt_addr = crate::memory::paging::phys_offset() + phys_addr.as_u64();
        let src_ptr = virt_addr.as_ptr::<u8>();
        
        let len = location.compressed_len;
        let mut buf = Vec::with_capacity(len);
        unsafe {
            buf.set_len(len);
            core::ptr::copy_nonoverlapping(src_ptr, buf.as_mut_ptr(), len);
        }
        
        Some(buf)
    }
    
    /// Release a compressed page slot. Removes the metadata entry.
    /// Note: the physical frame is leaked because BootInfoFrameAllocator
    /// is bump-only. A future free-list allocator will reclaim these.
    pub fn release(&self, vaddr: VirtAddr) {
        let page_start = vaddr.as_u64() & !(PAGE_SIZE as u64 - 1);
        let mut page_map = self.page_map.lock();
        
        if let Some(location) = page_map.remove(&page_start) {
            let mut allocated_frames = self.allocated_frames.lock();
            if let Some(pos) = allocated_frames.iter().position(|&f| f == location.phys_frame) {
                allocated_frames.remove(pos);
            }
            // TODO: Return frame to free-list when we have a real frame deallocator.
        }
    }
}
