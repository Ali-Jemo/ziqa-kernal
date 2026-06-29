use crate::abi::AbiError;
/// Shared Memory IPC for ZiqaKernel
///
/// Provides the fastest possible communication by allowing processes
/// to share the same physical memory frames. Zero-copy.
use crate::process::Pid;
use crate::memory::paging::{current_mapper};
use crate::memory::FRAME_ALLOCATOR;
use alloc::collections::BTreeMap;
use spin::Mutex;
use x86_64::structures::paging::{Page, PageTableFlags, PhysFrame, Mapper, FrameAllocator};
use x86_64::VirtAddr;

/// A shared memory segment
pub struct ShmRegion {
    pub id: u32,
    pub owner: Pid,
    pub phys_frames: alloc::vec::Vec<PhysFrame>, 
    pub size: usize,
}

pub struct ShmManager {
    regions: BTreeMap<u32, ShmRegion>,
    next_id: u32,
}

impl ShmManager {
    pub const fn new() -> Self {
        Self {
            regions: BTreeMap::new(),
            next_id: 1,
        }
    }

    /// Create a new shared memory region
    pub fn create(&mut self, owner: Pid, size: usize) -> Result<u32, AbiError> {
        let mut frames = alloc::vec::Vec::new();
        let num_pages = (size + 4095) / 4096;

        let mut fa_guard = FRAME_ALLOCATOR.lock();
        if let Some(fa) = fa_guard.as_mut() {
            for _ in 0..num_pages {
                let frame = fa.allocate_frame().ok_or(AbiError::Other("OOM in SHM"))?;
                frames.push(frame);
            }
        }

        let id = self.next_id;
        self.next_id += 1;

        let region = ShmRegion {
            id,
            owner,
            phys_frames: frames,
            size,
        };

        self.regions.insert(id, region);
        Ok(id)
    }

    /// Attach a region to a process address space
    pub fn attach(&self, id: u32, pid: Pid) -> Result<u64, AbiError> {
        if let Some(region) = self.regions.get(&id) {
            let mut mapper = unsafe { current_mapper() };
            let mut fa_guard = FRAME_ALLOCATOR.lock();
            let fa = fa_guard.as_mut().ok_or(AbiError::Other("FA missing"))?;

            // Pick a virtual address (simplified: static range for SHM)
            let virt_base = 0x8000_0000 + (id as u64 * 0x100_0000); // Each SHM gets 16MB virtual slot
            
            for (i, &frame) in region.phys_frames.iter().enumerate() {
                let page: Page<x86_64::structures::paging::Size4KiB> = Page::containing_address(VirtAddr::new(virt_base + (i as u64 * 4096)));
                let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE;
                
                unsafe {
                    match mapper.map_to(page, frame, flags, fa) {
                        Ok(flusher) => flusher.flush(),
                        Err(x86_64::structures::paging::mapper::MapToError::PageAlreadyMapped(_)) => {
                            // Processes currently share the kernel page table, so a second
                            // attach of the same SHM slot may already be mapped at virt_base.
                        }
                        Err(_) => return Err(AbiError::Other("SHM mapping failed")),
                    }
                }
            }

            crate::klog!(crate::klog::Level::Info, "SHM: Attached region {} to PID {} at 0x{:x}", id, pid.0, virt_base);
            Ok(virt_base) // Return virtual address where it's attached
        } else {
            Err(AbiError::Other("SHM region not found"))
        }
    }

    /// Find which SHM region contains this virtual address for a given process (Simplified)
    pub fn find_region_by_vaddr(&self, vaddr: u64) -> Option<u32> {
        // In our simplified SHM, they live in 0x8000_0000 + (id * 16MB)
        if vaddr >= 0x8000_0000 && vaddr < 0xA000_0000 {
            let id = ((vaddr - 0x8000_0000) / 0x100_0000) as u32;
            if self.regions.contains_key(&id) {
                return Some(id);
            }
        }
        None
    }

    /// Translate a virtual address within an SHM region to its physical address
    pub fn translate(&self, id: u32, vaddr: u64) -> u64 {
        if let Some(region) = self.regions.get(&id) {
            let virt_base = 0x8000_0000 + (id as u64 * 0x100_0000);
            let offset = vaddr - virt_base;
            let page_idx = (offset / 4096) as usize;
            if page_idx < region.phys_frames.len() {
                return region.phys_frames[page_idx].start_address().as_u64() + (offset % 4096);
            }
        }
        0
    }
}

pub static SHM: Mutex<ShmManager> = Mutex::new(ShmManager::new());
