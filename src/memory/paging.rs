use x86_64::{
    VirtAddr,
    structures::paging::{
        OffsetPageTable, PageTable, PhysFrame,
        mapper::Translate,
    },
    PhysAddr,
    registers::control::{Cr3, Cr3Flags},
};
use alloc::vec::Vec;
use spin::Mutex;
use lazy_static::lazy_static;

/// A wrapper around a mapper that also tracks the frame allocator.
pub struct MemoryMapper {
    pub mapper: OffsetPageTable<'static>,
}

impl MemoryMapper {
    pub unsafe fn new(physical_memory_offset: VirtAddr) -> Self {
        let level_4_table = Self::active_level_4_table(physical_memory_offset);
        let mapper = OffsetPageTable::new(level_4_table, physical_memory_offset);
        Self { mapper }
    }

    unsafe fn active_level_4_table(physical_memory_offset: VirtAddr) -> &'static mut PageTable {
        let (level_4_table_frame, _) = Cr3::read();
        let phys = level_4_table_frame.start_address();
        let virt = physical_memory_offset + phys.as_u64();
        &mut *(virt.as_mut_ptr())
    }

    pub fn translate_addr(&self, addr: VirtAddr) -> Option<PhysAddr> {
        self.mapper.translate_addr(addr)
    }
}

lazy_static! {
    pub static ref KERNEL_MAPPER: Mutex<Option<MemoryMapper>> = Mutex::new(None);
}

pub fn init_kernel_mapper(physical_memory_offset: VirtAddr) {
    let mapper = unsafe { MemoryMapper::new(physical_memory_offset) };
    *KERNEL_MAPPER.lock() = Some(mapper);
}

/// A set of flags that describe the properties of a memory region.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryRegionFlags {
    pub readable: bool,
    pub writable: bool,
    pub executable: bool,
    pub user_accessible: bool,
    pub copy_on_write: bool,
}

impl MemoryRegionFlags {
    pub const fn empty() -> Self {
        Self { readable: false, writable: false, executable: false, user_accessible: false, copy_on_write: false }
    }
    pub const fn read_write() -> Self {
        Self { readable: true, writable: true, executable: false, user_accessible: true, copy_on_write: false }
    }
}

/// A memory region within an address space.
#[derive(Debug, Clone)]
pub struct MemoryRegion {
    pub start: VirtAddr,
    pub size: usize,
    pub flags: MemoryRegionFlags,
    pub is_file_backed: bool,
    pub file_offset: u64,
}

/// A per-process address space.
pub struct AddressSpace {
    pub root_page_table: PageTable,
    pub regions: Vec<MemoryRegion>,
}

impl AddressSpace {
    pub fn new() -> Self {
        Self { root_page_table: PageTable::new(), regions: Vec::new() }
    }
    pub unsafe fn activate(&self) {
        let frame = PhysFrame::containing_address(PhysAddr::new(&self.root_page_table as *const _ as u64));
        Cr3::write(frame, Cr3Flags::empty());
    }

    pub fn find_region(&self, addr: VirtAddr) -> Option<&MemoryRegion> {
        self.regions.iter().find(|r| {
            let start = r.start.as_u64();
            let end = start + r.size as u64;
            addr.as_u64() >= start && addr.as_u64() < end
        })
    }
}