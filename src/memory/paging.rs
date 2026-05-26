use x86_64::{
    VirtAddr,
    structures::paging::{
        mapper::{MapToError, MapperFlush}, FrameAllocator, Mapper, Page, PageTable, PageTableFlags, Size4KiB, OffsetPageTable, PhysFrame,
        Translate,
    },
    PhysAddr,
    registers::control::{Cr3, Cr3Flags},
};
use crate::memory::frame_allocator::BootInfoFrameAllocator;
use alloc::vec::Vec;
use spin::Mutex;
use lazy_static::lazy_static;

/// A wrapper around a mapper that also tracks the frame allocator.
pub struct MemoryMapper {
    pub mapper: OffsetPageTable<'static>,
    pub frame_allocator: Mutex<BootInfoFrameAllocator>,
}

impl MemoryMapper {
    /// Create a new MemoryMapper from the physical memory offset.
    pub unsafe fn new(physical_memory_offset: VirtAddr) -> Self {
        let mapper = Self::init(physical_memory_offset);
        let boot_info_lock = crate::BOOT_INFO.lock();
        let boot_info = boot_info_lock.as_ref().expect("BOOT_INFO not initialized");
        let frame_allocator = BootInfoFrameAllocator::init(&boot_info.memory_map);
        MemoryMapper {
            mapper,
            frame_allocator: Mutex::new(frame_allocator),
        }
    }

    /// Initialize an offset page table.
    unsafe fn init(physical_memory_offset: VirtAddr) -> OffsetPageTable<'static> {
        let l4 = Self::active_level_4_table(physical_memory_offset.clone());
        OffsetPageTable::new(l4, physical_memory_offset)
    }

    /// Get a reference to the active level 4 page table.
    unsafe fn active_level_4_table(physical_memory_offset: VirtAddr) -> &'static mut PageTable {
        let (frame, _) = Cr3::read();
        let phys = frame.start_address();
        let virt = physical_memory_offset + phys.as_u64();
        &mut *(virt.as_mut_ptr())
    }

    /// Translate a virtual address to the mapped frame and page table flags.
    pub fn translate_addr(&self, addr: VirtAddr) -> Option<PhysAddr> {
        self.mapper.translate_addr(addr)
    }

    /// Map a page to a frame with the given flags.
    pub fn map_to<A>(
        &mut self,
        page: Page,
        frame: PhysFrame,
        flags: PageTableFlags,
        allocator: &mut A,
    ) -> Result<MapperFlush<Size4KiB>, MapToError<Size4KiB>>
    where
        A: FrameAllocator<Size4KiB>,
    {
        unsafe { self.mapper.map_to(page, frame, flags, allocator) }
    }

    /// Allocate a frame using the internal frame allocator.
    pub fn allocate_frame(&self) -> Option<PhysFrame> {
        self.frame_allocator.lock().allocate_frame()
    }
}

/// A global memory mapper for the kernel's own address space.
lazy_static! {
    pub static ref KERNEL_MAPPER: Mutex<Option<MemoryMapper>> = Mutex::new(None);
}

/// Initialize the kernel's memory mapper. This must be called after the boot information is available.
pub fn init_kernel_mapper(physical_memory_offset: VirtAddr) {
    let mut mapper = KERNEL_MAPPER.lock();
    *mapper = Some(unsafe { MemoryMapper::new(physical_memory_offset) });
}

/// Get a reference to the kernel's memory mapper.
pub fn kernel_mapper<'a>() -> &'a Mutex<Option<MemoryMapper>> {
    &KERNEL_MAPPER
}

/// A set of flags that describe the properties of a memory region.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryRegionFlags {
    pub readable: bool,
    pub writable: bool,
    pub executable: bool,
    pub user_accessible: bool,
}

impl MemoryRegionFlags {
    pub const fn empty() -> Self {
        MemoryRegionFlags {
            readable: false,
            writable: false,
            executable: false,
            user_accessible: false,
        }
    }

    pub const fn read_only() -> Self {
        MemoryRegionFlags {
            readable: true,
            writable: false,
            executable: false,
            user_accessible: true,
        }
    }

    pub const fn read_write() -> Self {
        MemoryRegionFlags {
            readable: true,
            writable: true,
            executable: false,
            user_accessible: true,
        }
    }

    pub const fn read_execute() -> Self {
        MemoryRegionFlags {
            readable: true,
            writable: false,
            executable: true,
            user_accessible: true,
        }
    }

    pub const fn user() -> Self {
        MemoryRegionFlags {
            readable: true,
            writable: true,
            executable: true,
            user_accessible: true,
        }
    }
}

/// A memory region within an address space.
#[derive(Debug, Clone)]
pub struct MemoryRegion {
    pub start: VirtAddr,
    pub size: usize,
    pub flags: MemoryRegionFlags,
    /// If true, the region is backed by a file (or will be filled with zeros on demand).
    pub is_file_backed: bool,
    /// The offset within the file for file-backed regions.
    pub file_offset: u64,
}

/// A process's address space.
pub struct AddressSpace {
    pub root_page_table: PageTable,
    pub regions: Vec<MemoryRegion>,
    pub entry_point: VirtAddr,
}

impl AddressSpace {
    /// Create a new, empty address space.
    pub fn new() -> Self {
        AddressSpace {
            root_page_table: PageTable::new(),
            regions: Vec::new(),
            entry_point: VirtAddr::new(0),
        }
    }

    /// Add a memory region to the address space.
    pub fn push(&mut self, region: MemoryRegion) {
        self.regions.push(region);
    }

    /// Activate this address space by loading its CR3 register.
    pub unsafe fn activate(&self) {
        use x86_64::registers::control::Cr3;
        use x86_64::structures::paging::PhysFrame;
        let frame = PhysFrame::containing_address(PhysAddr::new(
            &self.root_page_table as *const _ as u64,
        ));
        Cr3::write(frame, Cr3Flags::empty());
    }

    /// Handle a page fault at the given address.
    /// Returns true if the fault was handled, false otherwise.
    pub fn handle_page_fault(&self, _addr: VirtAddr, _error_code: &x86_64::structures::idt::PageFaultErrorCode) -> bool {
        // For now, we just return false to indicate we didn't handle it.
        // In a full implementation, we would:
        // 1. Find the memory region that contains the address.
        // 2. Check if the fault is due to a missing page or a protection violation.
        // 3. If missing, allocate a frame and zero-fill it (or load from file).
        // 4. If protection violation and it's a write fault on a copy-on-write page, 
        //    allocate a new frame, copy the page, and map it writable.
        false
    }
}
