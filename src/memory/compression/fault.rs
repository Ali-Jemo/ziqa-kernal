use x86_64::VirtAddr;
use x86_64::structures::paging::{PageTableFlags, FrameAllocator};
use x86_64::structures::idt::PageFaultErrorCode;
use crate::memory::paging::get_leaf_entry_mut;

/// OS-specific PTE bit indicating the page is compressed and stored.
pub const COMPRESSED_BIT: PageTableFlags = PageTableFlags::BIT_9;

/// Handles a page fault for a compressed page.
pub fn handle_compressed_fault(addr: VirtAddr, _error_code: PageFaultErrorCode) -> bool {
    let pte = match get_leaf_entry_mut(addr) {
        Some(entry) => entry,
        None => return false,
    };
    
    let flags = pte.flags();
    if !flags.contains(COMPRESSED_BIT) {
        return false;
    }
    
    crate::println!("[MM] Compressed page fault at {:?}", addr);
    
    // 1. Extract location and compressed data from the CompressedPageStore
    let location = match crate::memory::compression::PAGE_STORE.get_location(addr) {
        Some(loc) => loc,
        None => {
            crate::println!("[MM] Fatal: Compressed page not found in store for {:?}", addr);
            return false;
        }
    };
    
    let compressed_data = match crate::memory::compression::PAGE_STORE.retrieve(addr) {
        Some(data) => data,
        None => {
            crate::println!("[MM] Fatal: Failed to retrieve compressed data for {:?}", addr);
            return false;
        }
    };
    
    // 2. Decompress via CompressionEngine
    let decompressed = match crate::memory::compression::COMPRESSION_ENGINE.decompress(
        &compressed_data, 
        location.original_size
    ) {
        Some(d) => d,
        None => {
            crate::println!("[MM] Fatal: Failed to decompress page at {:?}", addr);
            return false;
        }
    };
    
    // 3. Allocate a new physical frame
    let mut frame_allocator = crate::memory::FRAME_ALLOCATOR.lock();
    let frame_alloc = frame_allocator.as_mut().expect("FRAME_ALLOCATOR not initialized");
    let phys_frame = match frame_alloc.allocate_frame() {
        Some(f) => f,
        None => {
            crate::println!("[MM] OOM during decompression of {:?}", addr);
            return false;
        }
    };
    drop(frame_allocator);
    
    // 4. Copy the decompressed data into the new frame
    let phys_addr = phys_frame.start_address();
    let virt_addr = crate::memory::paging::phys_offset() + phys_addr.as_u64();
    let dst_ptr = virt_addr.as_mut_ptr::<u8>();
    unsafe {
        core::ptr::copy_nonoverlapping(decompressed.as_ptr(), dst_ptr, crate::memory::PAGE_SIZE);
    }
    
    // 5. Update the PTE: clear COMPRESSED_BIT, set PRESENT, update frame addr
    let mut new_flags = flags;
    new_flags.remove(COMPRESSED_BIT);
    new_flags.insert(PageTableFlags::PRESENT);
    pte.set_addr(phys_addr, new_flags);
    
    // 6. Flush TLB for this address
    crate::memory::paging::smp_tlb_flush(addr);
    
    // 7. Call `store.release()` to free the compressed store slot
    crate::memory::compression::PAGE_STORE.release(addr);
    
    true
}
