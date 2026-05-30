use crate::memory::MemoryRegionFlags;
use x86_64::VirtAddr;
use crate::memory::MemoryRegion;
use alloc::vec::Vec;

#[derive(Debug, Clone)]
pub struct Vma {
    pub start: VirtAddr,
    pub end: VirtAddr,
    pub flags: MemoryRegionFlags,
    pub is_file_backed: bool,
    pub file_path: Option<alloc::string::String>,
    pub file_offset: u64,
}

impl Vma {
    pub fn new(start: VirtAddr, size: usize, flags: MemoryRegionFlags) -> Self {
        Self {
            start,
            end: start + size as u64,
            flags,
            is_file_backed: false,
            file_path: None,
            file_offset: 0,
        }
    }

    pub fn contains(&self, addr: VirtAddr) -> bool {
        addr >= self.start && addr < self.end
    }
}

impl From<MemoryRegion> for Vma {
    fn from(region: MemoryRegion) -> Self {
        Self {
            start: region.start,
            end: region.start + region.size as u64,
            flags: region.flags,
            is_file_backed: region.is_file_backed,
            file_path: None,
            file_offset: region.file_offset,
        }
    }
}

/// Helper to find a free range of virtual addresses for mmap.
pub fn find_free_range(vmas: &[Vma], size: usize, start_hint: VirtAddr) -> Option<VirtAddr> {
    let size = size as u64;
    let mut current = start_hint;
    
    // Simple bump allocator approach for now, sorted by start address.
    // In a mature system, this would manage a gap-free list.
    let mut sorted_vmas = vmas.to_vec();
    sorted_vmas.sort_by_key(|vma| vma.start);
    
    for vma in &sorted_vmas {
        if current + size <= vma.start {
            return Some(current);
        }
        if current < vma.end {
            current = vma.end;
        }
    }
    Some(current)
}
