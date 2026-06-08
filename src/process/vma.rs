use crate::memory::MemoryRegionFlags;
use x86_64::VirtAddr;
use crate::memory::MemoryRegion;

#[derive(Debug, Clone)]
pub struct Vma {
    pub start: VirtAddr,
    pub end: VirtAddr,
    pub flags: MemoryRegionFlags,
    pub is_file_backed: bool,
    pub file_path: Option<alloc::string::String>,
    pub file_offset: u64,
    /// Optional eBPF program ID for behavioral monitoring
    pub bco_hook: Option<u32>,
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
            bco_hook: None,
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
            bco_hook: region.bco_hook,
        }
    }
}

pub fn is_range_free(vmas: &[Vma], start: VirtAddr, size: usize) -> bool {
    let start = start.as_u64();
    let end = start + size as u64;
    !vmas.iter().any(|vma| {
        let vma_start = vma.start.as_u64();
        let vma_end = vma.end.as_u64();
        start < vma_end && end > vma_start
    })
}

/// Helper to find a free range of virtual addresses for mmap.
pub fn find_free_range(vmas: &[Vma], size: usize, start_hint: VirtAddr) -> Option<VirtAddr> {
    let size = size as u64;
    
    // If start_hint is non-zero, check if the requested range is already free
    if start_hint.as_u64() != 0 && start_hint.is_aligned(4096u64) {
        if is_range_free(vmas, start_hint, size as usize) {
            return Some(start_hint);
        }
    }

    let mut current = if start_hint.as_u64() != 0 {
        start_hint.align_up(4096u64)
    } else {
        VirtAddr::new(0x4000_0000)
    };

    for vma in vmas {
        if vma.end <= current {
            continue;
        }
        if current + size <= vma.start {
            return Some(current);
        }
        current = vma.end;
    }
    Some(current)
}
