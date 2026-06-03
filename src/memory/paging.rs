use crate::memory::{BootInfoFrameAllocator, FRAME_ALLOCATOR};
use alloc::vec::Vec;
use lazy_static::lazy_static;
use spin::Mutex;
use x86_64::{
    registers::control::{Cr3, Cr3Flags},
    structures::paging::{
        mapper::{Translate, Mapper},
        page_table::PageTableEntry,
        FrameAllocator, OffsetPageTable, Page, PageTable, PageTableFlags, PhysFrame,
    },
    PhysAddr, VirtAddr,
};

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
        Self {
            readable: false,
            writable: false,
            executable: false,
            user_accessible: false,
            copy_on_write: false,
        }
    }
    pub const fn read_write() -> Self {
        Self {
            readable: true,
            writable: true,
            executable: false,
            user_accessible: true,
            copy_on_write: false,
        }
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
        Self {
            root_page_table: PageTable::new(),
            regions: Vec::new(),
        }
    }
    pub unsafe fn activate(&self) {
        let frame =
            PhysFrame::containing_address(PhysAddr::new(&self.root_page_table as *const _ as u64));
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

// ── COW Fork helpers ──────────────────────────────────────────────────────────

/// Get the physical memory offset from the boot info.
pub fn phys_offset() -> VirtAddr {
    let boot_info = crate::BOOT_INFO.lock();
    let bi = boot_info.as_ref().expect("BOOT_INFO not initialized");
    VirtAddr::new(bi.physical_memory_offset)
}

/// Map a physical frame as a mutable page table reference.
pub unsafe fn frame_as_page_table_mut(frame: PhysFrame) -> &'static mut PageTable {
    let po = phys_offset();
    let virt = po + frame.start_address().as_u64();
    &mut *(virt.as_mut_ptr())
}

/// Map a physical frame as a read-only page table reference.
pub unsafe fn frame_as_page_table(frame: PhysFrame) -> &'static PageTable {
    let po = phys_offset();
    let virt = po + frame.start_address().as_u64();
    &*(virt.as_ptr())
}

/// Walk the active page table (from CR3) to find the leaf (L1) entry for `vaddr`,
/// returning a mutable reference if present.
pub fn get_leaf_entry_mut(vaddr: VirtAddr) -> Option<&'static mut PageTableEntry> {
    let (l4_frame, _) = Cr3::read();

    let l4 = unsafe { frame_as_page_table(l4_frame) };
    let l4_idx = (vaddr.as_u64() >> 39) & 0x1FF;
    if !l4[l4_idx as usize]
        .flags()
        .contains(PageTableFlags::PRESENT)
    {
        return None;
    }
    let l3_frame = l4[l4_idx as usize].frame().ok()?;

    let l3 = unsafe { frame_as_page_table(l3_frame) };
    let l3_idx = (vaddr.as_u64() >> 30) & 0x1FF;
    if !l3[l3_idx as usize]
        .flags()
        .contains(PageTableFlags::PRESENT)
    {
        return None;
    }
    let l2_frame = l3[l3_idx as usize].frame().ok()?;

    let l2 = unsafe { frame_as_page_table(l2_frame) };
    let l2_idx = (vaddr.as_u64() >> 21) & 0x1FF;
    if !l2[l2_idx as usize]
        .flags()
        .contains(PageTableFlags::PRESENT)
    {
        return None;
    }
    if l2[l2_idx as usize]
        .flags()
        .contains(PageTableFlags::HUGE_PAGE)
    {
        // 2MB huge page — not handled
        return None;
    }
    let l1_frame = l2[l2_idx as usize].frame().ok()?;

    let l1 = unsafe { frame_as_page_table_mut(l1_frame) };
    let l1_idx = (vaddr.as_u64() >> 12) & 0x1FF;
    if !l1[l1_idx as usize]
        .flags()
        .contains(PageTableFlags::PRESENT)
    {
        return None;
    }
    Some(&mut l1[l1_idx as usize])
}

/// Walk a specific page table hierarchy (by root frame) to find the leaf entry for `vaddr`.
pub fn get_leaf_entry_mut_in(
    root_frame: PhysFrame,
    vaddr: VirtAddr,
) -> Option<&'static mut PageTableEntry> {
    let l4 = unsafe { frame_as_page_table(root_frame) };
    let l4_idx = (vaddr.as_u64() >> 39) & 0x1FF;
    if !l4[l4_idx as usize]
        .flags()
        .contains(PageTableFlags::PRESENT)
    {
        return None;
    }
    let l3_frame = l4[l4_idx as usize].frame().ok()?;

    let l3 = unsafe { frame_as_page_table(l3_frame) };
    let l3_idx = (vaddr.as_u64() >> 30) & 0x1FF;
    if !l3[l3_idx as usize]
        .flags()
        .contains(PageTableFlags::PRESENT)
    {
        return None;
    }
    let l2_frame = l3[l3_idx as usize].frame().ok()?;

    let l2 = unsafe { frame_as_page_table(l2_frame) };
    let l2_idx = (vaddr.as_u64() >> 21) & 0x1FF;
    if !l2[l2_idx as usize]
        .flags()
        .contains(PageTableFlags::PRESENT)
    {
        return None;
    }
    if l2[l2_idx as usize]
        .flags()
        .contains(PageTableFlags::HUGE_PAGE)
    {
        return None;
    }
    let l1_frame = l2[l2_idx as usize].frame().ok()?;

    let l1 = unsafe { frame_as_page_table_mut(l1_frame) };
    let l1_idx = (vaddr.as_u64() >> 12) & 0x1FF;
    if !l1[l1_idx as usize]
        .flags()
        .contains(PageTableFlags::PRESENT)
    {
        return None;
    }
    Some(&mut l1[l1_idx as usize])
}

/// Get the physical frame mapped by a leaf entry for a given virtual address.
pub fn get_phys_frame(root_frame: PhysFrame, vaddr: VirtAddr) -> Option<PhysFrame> {
    let l4 = unsafe { frame_as_page_table(root_frame) };
    let l4_idx = (vaddr.as_u64() >> 39) & 0x1FF;
    if !l4[l4_idx as usize]
        .flags()
        .contains(PageTableFlags::PRESENT)
    {
        return None;
    }
    let l3_frame = l4[l4_idx as usize].frame().ok()?;

    let l3 = unsafe { frame_as_page_table(l3_frame) };
    let l3_idx = (vaddr.as_u64() >> 30) & 0x1FF;
    if !l3[l3_idx as usize]
        .flags()
        .contains(PageTableFlags::PRESENT)
    {
        return None;
    }
    let l2_frame = l3[l3_idx as usize].frame().ok()?;

    let l2 = unsafe { frame_as_page_table(l2_frame) };
    let l2_idx = (vaddr.as_u64() >> 21) & 0x1FF;
    if !l2[l2_idx as usize]
        .flags()
        .contains(PageTableFlags::PRESENT)
    {
        return None;
    }
    if l2[l2_idx as usize]
        .flags()
        .contains(PageTableFlags::HUGE_PAGE)
    {
        return None;
    }
    let l1_frame = l2[l2_idx as usize].frame().ok()?;

    let l1 = unsafe { frame_as_page_table(l1_frame) };
    let l1_idx = (vaddr.as_u64() >> 12) & 0x1FF;
    if !l1[l1_idx as usize]
        .flags()
        .contains(PageTableFlags::PRESENT)
    {
        return None;
    }
    l1[l1_idx as usize].frame().ok()
}

/// Clear the writable bit on all present user-level leaf (L1) pages
/// reachable from `root_frame`. This is called on the parent during COW fork
/// so that both parent and child initially have read-only mappings.
pub fn make_user_leaf_readonly(root_frame: PhysFrame) {
    let l4 = unsafe { frame_as_page_table_mut(root_frame) };
    for l4_idx in 0..256 {
        if !l4[l4_idx].flags().contains(PageTableFlags::PRESENT) {
            continue;
        }
        // Skip non-user L4 entries — these are kernel identity mappings
        if !l4[l4_idx].flags().contains(PageTableFlags::USER_ACCESSIBLE) {
            continue;
        }
        if l4[l4_idx].flags().contains(PageTableFlags::HUGE_PAGE) {
            continue;
        }
        let l3_frame = match l4[l4_idx].frame() {
            Ok(f) => f,
            _ => continue,
        };
        let l3 = unsafe { frame_as_page_table_mut(l3_frame) };
        for l3_idx in 0..512 {
            if !l3[l3_idx].flags().contains(PageTableFlags::PRESENT) {
                continue;
            }
            // Skip non-user L3 entries
            if !l3[l3_idx].flags().contains(PageTableFlags::USER_ACCESSIBLE) {
                continue;
            }
            let l2_frame = match l3[l3_idx].frame() {
                Ok(f) => f,
                _ => continue,
            };
            let l2 = unsafe { frame_as_page_table_mut(l2_frame) };
            for l2_idx in 0..512 {
                if !l2[l2_idx].flags().contains(PageTableFlags::PRESENT) {
                    continue;
                }
                // Skip non-user L2 entries
                if !l2[l2_idx].flags().contains(PageTableFlags::USER_ACCESSIBLE) {
                    continue;
                }
                if l2[l2_idx].flags().contains(PageTableFlags::HUGE_PAGE) {
                    continue;
                }
                let l1_frame = match l2[l2_idx].frame() {
                    Ok(f) => f,
                    _ => continue,
                };
                let l1 = unsafe { frame_as_page_table_mut(l1_frame) };
                for l1_idx in 0..512 {
                    let entry = &mut l1[l1_idx];
                    if !entry.flags().contains(PageTableFlags::PRESENT) {
                        continue;
                    }
                    // Skip non-user leaf entries
                    if !entry.flags().contains(PageTableFlags::USER_ACCESSIBLE) {
                        continue;
                    }
                    let flags = entry.flags();
                    if flags.contains(PageTableFlags::WRITABLE) {
                        let addr = entry.addr();
                        let new_flags = flags & !PageTableFlags::WRITABLE;
                        entry.set_addr(addr, new_flags);
                    }
                }
            }
        }
    }
}

/// Recursively clone the user page table hierarchy (L4 entries 0–255).
/// Kernel entries (256–511) are copied by pointer (shared).
/// All leaf pages in the clone retain the same flags (already read-only
/// because the parent's pages were made read-only first).
pub fn clone_user_table_tree(
    src_frame: PhysFrame,
    level: u8,
    fa: &mut BootInfoFrameAllocator,
) -> Option<PhysFrame> {
    let src = unsafe { frame_as_page_table(src_frame) };
    let new_frame = fa.allocate_frame()?;
    let new = unsafe { frame_as_page_table_mut(new_frame) };

    let is_l4 = level == 4;
    for i in 0..512 {
        // Kernel entries: copy by pointer (share lower-level tables)
        if is_l4 && i >= 256 {
            new[i] = src[i].clone();
            continue;
        }
        if !src[i].flags().contains(PageTableFlags::PRESENT) {
            continue;
        }

        // Skip non-user entries to avoid cloning kernel identity-mapped pages
        if !src[i].flags().contains(PageTableFlags::USER_ACCESSIBLE) {
            continue;
        }

        // Handle huge pages at intermediate levels — copy as-is (leaf-like)
        if level > 1 && src[i].flags().contains(PageTableFlags::HUGE_PAGE) {
            let cow_flags = if src[i].flags().contains(PageTableFlags::WRITABLE) {
                src[i].flags() & !PageTableFlags::WRITABLE
            } else {
                src[i].flags()
            };
            new[i].set_addr(src[i].addr(), cow_flags);
            continue;
        }

        if level == 1 {
            // Leaf page: copy entry as-is (already read-only from parent fixup)
            let flags = src[i].flags();
            // Mark writable pages as COW (read-only)
            let cow_flags = if flags.contains(PageTableFlags::WRITABLE) {
                flags & !PageTableFlags::WRITABLE
            } else {
                flags
            };
            new[i].set_addr(src[i].addr(), cow_flags);
        } else {
            // Table: recursively clone
            let child_frame = match src[i].frame() {
                Ok(f) => f,
                _ => continue,
            };
            if let Some(cloned) = clone_user_table_tree(child_frame, level - 1, fa) {
                new[i].set_addr(cloned.start_address(), src[i].flags());
            } else {
                // Allocation failure — copy entry as-is (best-effort)
                new[i] = src[i].clone();
            }
        }
    }
    Some(new_frame)
}

/// Create an `OffsetPageTable` for the currently active (CR3) page table.
/// This must be used instead of `KERNEL_MAPPER` after per-process page tables
/// are in use, since `KERNEL_MAPPER` is initialized once at boot.
pub unsafe fn current_mapper() -> OffsetPageTable<'static> {
    let po = phys_offset();
    let (l4_frame, _) = Cr3::read();
    let l4_virt = po + l4_frame.start_address().as_u64();
    let l4 = &mut *(l4_virt.as_mut_ptr());
    OffsetPageTable::new(l4, po)
}

/// Perform the COW fork page table operations:
/// 1. Make all writable user leaf pages read-only in the parent
/// 2. Clone the user page table hierarchy for the child
/// 3. Return the child's L4 physical frame
pub fn cow_fork_parent() -> Option<PhysFrame> {
    let (parent_l4_frame, _) = Cr3::read();

    // Step 1: make parent's writable user pages read-only
    make_user_leaf_readonly(parent_l4_frame);

    // Step 2: clone the user table tree for the child
    let mut fa_guard = FRAME_ALLOCATOR.lock();
    let fa = fa_guard.as_mut().expect("FRAME_ALLOCATOR not initialized");
    let child_l4 = clone_user_table_tree(parent_l4_frame, 4, fa)?;
    drop(fa_guard);

    smp_tlb_flush_all();

    Some(child_l4)
}

pub fn smp_tlb_flush_all() {
    x86_64::instructions::tlb::flush_all();
    crate::arch::x86_64::smp::tlb_shootdown_all(0);
}

pub fn smp_tlb_flush(vaddr: VirtAddr) {
    x86_64::instructions::tlb::flush(vaddr);
    crate::arch::x86_64::smp::tlb_shootdown_all(vaddr.as_u64());
}

/// Handle a copy-on-write page fault at `fault_addr`.
/// Allocates a new physical frame, copies the content from the shared read-only page,
/// and remaps it as writable in the current (faulting) process's page table.
pub fn handle_cow_fault(fault_addr: VirtAddr) -> bool {
    // Read the current mapping to find the old physical frame
    let old_frame = match get_phys_frame(Cr3::read().0, fault_addr) {
        Some(f) => f,
        None => {
            return false;
        }
    };

    // Allocate a new frame
    let mut fa_guard = FRAME_ALLOCATOR.lock();
    let fa = fa_guard.as_mut().expect("FRAME_ALLOCATOR not initialized");
    let new_frame = match fa.allocate_frame() {
        Some(f) => f,
        None => {
            return false;
        }
    };
    drop(fa_guard);

    let po = phys_offset();

    // Copy data from old frame to new frame
    unsafe {
        let src_ptr = (po + old_frame.start_address().as_u64()).as_ptr();
        let dst_ptr = (po + new_frame.start_address().as_u64()).as_mut_ptr();
        core::ptr::copy_nonoverlapping::<u8>(src_ptr, dst_ptr, 4096);
    }

    // Update the leaf entry to point to the new frame with writable permission
    let leaf = match get_leaf_entry_mut(fault_addr) {
        Some(e) => e,
        None => {
            return false;
        }
    };

    let mut flags = leaf.flags();
    flags |= PageTableFlags::WRITABLE;
    leaf.set_addr(new_frame.start_address(), flags);

    // Flush TLB for this page
    x86_64::instructions::tlb::flush(fault_addr);

    crate::println!(
        "[COW] Copied page at {:?} to new frame {:?}",
        fault_addr,
        new_frame
    );
    true
}

/// Map all VMA regions of a process into its page table.
/// Copies data from `binary` for file-backed regions (identified by the
/// file_offset field and the process's stored `binary_data`).
pub fn map_process_regions(process: &mut crate::process::Process) -> Result<(), &'static str> {
    let root_frame = match process.page_table_frame {
        Some(f) => f,
        None => return Err("no per-process page table"),
    };

    // Build a mapper from the process's root page table
    let po = phys_offset();
    let l4_virt = po + root_frame.start_address().as_u64();
    let l4 = unsafe { &mut *(l4_virt.as_mut_ptr()) };
    let mut mapper = unsafe { OffsetPageTable::new(l4, po) };

    let mut fa_guard = FRAME_ALLOCATOR.lock();
    let fa = fa_guard.as_mut().expect("FRAME_ALLOCATOR not initialized");

    let binary_data = &process.binary_data;

    for vma in &process.vmas {
        let start_addr = vma.start.as_u64();
        let end_addr = vma.end.as_u64();
        let page_flags = region_flags_to_page_flags(&vma.flags);

        for addr in (start_addr..end_addr).step_by(4096) {
            let page = x86_64::structures::paging::Page::containing_address(VirtAddr::new(addr));
            let frame = fa.allocate_frame().ok_or("OOM mapping process region")?;

            unsafe {
                mapper
                    .map_to(page, frame, page_flags, fa)
                    .map_err(|_| "map_to failed in map_process_regions")?
                    .flush();
            }

            // Copy data from binary if this VMA has file_offset == vaddr
            let vma_start = vma.start.as_u64();
            let page_in_region_offset = addr - vma_start;
            let binary_offset = vma.file_offset as usize + page_in_region_offset as usize;
            let copy_size = 4096usize.min(binary_data.len().saturating_sub(binary_offset));

            if copy_size > 0 {
                let dst = (po + frame.start_address().as_u64()).as_mut_ptr();
                unsafe {
                    core::ptr::copy_nonoverlapping(binary_data.as_ptr().add(binary_offset), dst, copy_size);
                }
            }
        }
    }

    Ok(())
}

/// Convert MemoryRegionFlags to PageTableFlags
pub fn region_flags_to_page_flags(rf: &crate::memory::paging::MemoryRegionFlags) -> PageTableFlags {
    let mut pf = PageTableFlags::PRESENT;
    if rf.writable {
        pf |= PageTableFlags::WRITABLE;
    }
    if rf.user_accessible {
        pf |= PageTableFlags::USER_ACCESSIBLE;
    }
    if !rf.executable {
        pf |= PageTableFlags::NO_EXECUTE;
    }
    pf
}

/// Map pages for a brk expansion from `old_brk` to `new_brk`.
/// `page_table_frame` is an optional per-process page table.
pub fn handle_brk(
    page_table_frame: Option<PhysFrame>,
    old_brk: u64,
    new_brk: u64,
) -> Result<(), &'static str> {
    let start_page = (old_brk + 0xFFF) & !0xFFF;
    let end_page = new_brk & !0xFFF;
    if start_page >= end_page {
        return Ok(());
    }

    let po = phys_offset();
    let mut fa_guard = FRAME_ALLOCATOR.lock();
    let fa = fa_guard.as_mut().expect("FRAME_ALLOCATOR not initialized");

    // Determine which mapper to use
    if let Some(root_frame) = page_table_frame {
        let l4_virt = po + root_frame.start_address().as_u64();
        let l4 = unsafe { &mut *(l4_virt.as_mut_ptr()) };
        let mut mapper = unsafe { OffsetPageTable::new(l4, po) };

        for addr in (start_page..end_page).step_by(4096) {
            let page = Page::containing_address(VirtAddr::new(addr));
            if mapper.translate_addr(page.start_address()).is_some() {
                continue;
            }
            let frame = fa.allocate_frame().ok_or("OOM in brk")?;
            let flags = PageTableFlags::PRESENT
                | PageTableFlags::WRITABLE
                | PageTableFlags::USER_ACCESSIBLE
                | PageTableFlags::NO_EXECUTE;
            unsafe {
                mapper
                    .map_to(page, frame, flags, fa)
                    .map_err(|_| "map_to failed in brk")?
                    .flush();
            }
        }
    } else {
        let mut mapper = unsafe { current_mapper() };
        for addr in (start_page..end_page).step_by(4096) {
            let page = Page::containing_address(VirtAddr::new(addr));
            if mapper.translate_addr(page.start_address()).is_some() {
                continue;
            }
            let frame = fa.allocate_frame().ok_or("OOM in brk")?;
            let flags = PageTableFlags::PRESENT
                | PageTableFlags::WRITABLE
                | PageTableFlags::USER_ACCESSIBLE
                | PageTableFlags::NO_EXECUTE;
            unsafe {
                mapper
                    .map_to(page, frame, flags, fa)
                    .map_err(|_| "map_to failed in brk")?
                    .flush();
            }
        }
    }

    Ok(())
}

/// Create a new page table for a process by copying kernel space L4 entries.
pub fn create_process_page_table() -> Option<PhysFrame> {
    let (parent_l4_frame, _) = Cr3::read();
    let mut fa_guard = FRAME_ALLOCATOR.lock();
    let fa = fa_guard.as_mut().expect("FRAME_ALLOCATOR not initialized");

    // Allocate a new frame for L4 table
    let new_frame = fa.allocate_frame()?;
    let new_table = unsafe { frame_as_page_table_mut(new_frame) };

    // Clear all entries first
    for i in 0..512 {
        new_table[i].set_unused();
    }

    // Copy the kernel mappings (L4 indices 256 to 511)
    let parent_table = unsafe { frame_as_page_table(parent_l4_frame) };
    for i in 256..512 {
        new_table[i] = parent_table[i].clone();
    }

    Some(new_frame)
}
