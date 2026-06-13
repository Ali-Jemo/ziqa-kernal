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

/// Alias for Grant used by ELF loader and Vma conversion.
pub type MemoryRegion = Grant;

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

/// A memory grant (VMA) within an address space.
#[derive(Debug, Clone)]
pub struct Grant {
    pub start: VirtAddr,
    pub size: usize,
    pub flags: MemoryRegionFlags,
    pub is_file_backed: bool,
    pub file_path: Option<alloc::string::String>,
    pub file_offset: u64,
    pub file_size: u64,
    /// Optional eBPF program ID for behavioral monitoring
    pub bco_hook: Option<u32>,
}

impl Grant {
    pub fn contains(&self, addr: VirtAddr) -> bool {
        let start = self.start.as_u64();
        let end = start + self.size as u64;
        addr.as_u64() >= start && addr.as_u64() < end
    }
}

/// A per-process address space.
pub struct AddressSpace {
    pub root_page_table: PageTable,
    pub grants: Vec<Grant>,
}

impl AddressSpace {
    pub fn new() -> Self {
        Self {
            root_page_table: PageTable::new(),
            grants: Vec::new(),
        }
    }
    pub unsafe fn activate(&self) {
        let frame =
            PhysFrame::containing_address(PhysAddr::new(&self.root_page_table as *const _ as u64));
        Cr3::write(frame, Cr3Flags::empty());
    }

    pub fn find_grant(&self, addr: VirtAddr) -> Option<&Grant> {
        self.grants.iter().find(|g| g.contains(addr))
    }
}

// ── COW Fork helpers ──────────────────────────────────────────────────────────

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

/// Handle a page fault by querying the AddressSpace grants.
pub fn handle_page_fault(addr_space: &AddressSpace, fault_addr: VirtAddr, access_flags: PageTableFlags) -> bool {
    let grant = match addr_space.find_grant(fault_addr) {
        Some(g) => g,
        None => return false, // Invalid access
    };

    // Verify permissions
    if access_flags.contains(PageTableFlags::WRITABLE) && !grant.flags.writable {
        return false; // Access violation
    }

    // If it's a COW grant and write access is requested, handle COW fault
    if grant.flags.copy_on_write && access_flags.contains(PageTableFlags::WRITABLE) {
        return false; // COW not yet implemented
    }

    // Otherwise, perform demand paging (map physical frame)
    demand_page(addr_space, fault_addr)
}

/// Demand page by mapping a new frame into the page table based on grant.
pub fn demand_page(addr_space: &AddressSpace, fault_addr: VirtAddr) -> bool {
    let grant = match addr_space.find_grant(fault_addr) {
        Some(g) => g,
        None => return false,
    };

    let frame = {
        let mut fa_guard = FRAME_ALLOCATOR.lock();
        let fa = fa_guard.as_mut().expect("FRAME_ALLOCATOR not initialized");
        fa.allocate_frame()
    };
    let frame = match frame {
        Some(f) => f,
        None => return false,
    };

    let po = phys_offset();
    let l4 = unsafe { frame_as_page_table_mut(PhysFrame::containing_address(PhysAddr::new(&addr_space.root_page_table as *const _ as u64))) };
    let mut mapper = unsafe { OffsetPageTable::new(l4, po) };
    let page = Page::containing_address(fault_addr);
    
    let page_flags = region_flags_to_page_flags(&grant.flags);

    let mut fa_guard = FRAME_ALLOCATOR.lock();
    let fa = fa_guard.as_mut().expect("FRAME_ALLOCATOR not initialized");
    unsafe {
        mapper
            .map_to(page, frame, page_flags, fa)
            .is_ok()
    }
}

/// Handle a copy-on-write page fault by allocating a new frame, copying the
/// faulted page content, and mapping the new frame as writable.
pub fn handle_cow_fault(pt_frame: PhysFrame, fault_addr: VirtAddr) -> bool {
    let new_frame = {
        let mut fa_guard = FRAME_ALLOCATOR.lock();
        let fa = fa_guard.as_mut().expect("FRAME_ALLOCATOR not initialized");
        fa.allocate_frame()
    };
    let new_frame = match new_frame {
        Some(f) => f,
        None => return false,
    };

    let po = phys_offset();

    // Copy content from the faulting (read-only COW) page to the new frame
    let src = po + fault_addr.as_u64();
    let dst = po + new_frame.start_address().as_u64();
    unsafe {
        core::ptr::copy_nonoverlapping(src.as_u64() as *const u8, dst.as_u64() as *mut u8, 0x1000);
    }

    // Map the new frame as writable in the process's page table
    let l4 = unsafe { frame_as_page_table_mut(pt_frame) };
    let mut mapper = unsafe { OffsetPageTable::new(l4, po) };
    let page = Page::containing_address(fault_addr);

    let mut fa_guard = FRAME_ALLOCATOR.lock();
    let fa = fa_guard.as_mut().expect("FRAME_ALLOCATOR not initialized");

    unsafe {
        mapper
            .map_to(
                page,
                new_frame,
                PageTableFlags::PRESENT
                    | PageTableFlags::WRITABLE
                    | PageTableFlags::USER_ACCESSIBLE,
                fa,
            )
            .is_ok()
    }
}

/// Demand page using a raw page table frame directly (no AddressSpace grants lookup).
pub fn demand_page_for_frame(
    pt_frame: PhysFrame,
    fault_addr: VirtAddr,
    page_flags: PageTableFlags,
    init_data: Option<&[u8]>,
) -> bool {
    let new_frame = {
        let mut fa_guard = FRAME_ALLOCATOR.lock();
        let fa = fa_guard.as_mut().expect("FRAME_ALLOCATOR not initialized");
        fa.allocate_frame()
    };
    let new_frame = match new_frame {
        Some(f) => f,
        None => return false,
    };

    let po = phys_offset();
    let frame_ptr = (po + new_frame.start_address().as_u64()).as_mut_ptr::<u8>();
    unsafe {
        if let Some(data) = init_data {
            let len = data.len().min(4096);
            core::ptr::copy_nonoverlapping(data.as_ptr(), frame_ptr, len);
            if len < 4096 {
                core::ptr::write_bytes(frame_ptr.add(len), 0, 4096 - len);
            }
        } else {
            core::ptr::write_bytes(frame_ptr, 0, 4096);
        }
    }

    let l4 = unsafe { frame_as_page_table_mut(pt_frame) };
    let mut mapper = unsafe { OffsetPageTable::new(l4, po) };
    let page = Page::containing_address(fault_addr);

    let mut fa_guard = FRAME_ALLOCATOR.lock();
    let fa = fa_guard.as_mut().expect("FRAME_ALLOCATOR not initialized");

    unsafe { mapper.map_to(page, new_frame, page_flags, fa).is_ok() }
}

/// Map all Grant regions of a process into its page table.
pub fn map_process_regions(addr_space: &AddressSpace, binary_data: &[u8]) -> Result<(), &'static str> {
    // Build a mapper from the process's root page table
    let po = phys_offset();
    let l4 = unsafe { frame_as_page_table_mut(PhysFrame::containing_address(PhysAddr::new(&addr_space.root_page_table as *const _ as u64))) };
    let mut mapper = unsafe { OffsetPageTable::new(l4, po) };

    let mut fa_guard = FRAME_ALLOCATOR.lock();
    let fa = fa_guard.as_mut().expect("FRAME_ALLOCATOR not initialized");

    for grant in &addr_space.grants {
        let start_addr = grant.start.as_u64();
        let end_addr = start_addr + grant.size as u64;
        let page_flags = region_flags_to_page_flags(&grant.flags);

        for addr in (start_addr..end_addr).step_by(4096) {
            let page = x86_64::structures::paging::Page::containing_address(VirtAddr::new(addr));
            let frame = fa.allocate_frame().ok_or("OOM mapping process region")?;

            unsafe {
                mapper
                    .map_to(page, frame, page_flags, fa)
                    .map_err(|_| "map_to failed in map_process_regions")?
                    .flush();
            }

            // Copy data from binary if this grant has file_offset
            let grant_start = grant.start.as_u64();
            let page_in_grant_offset = addr - grant_start;
            let binary_offset = grant.file_offset as usize + page_in_grant_offset as usize;
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

    // Copy the kernel heap mapping (L4 index 136) since it is in the lower half
    // but houses the kernel stacks allocated via Vec.
    new_table[136] = parent_table[136].clone();

    // Allocate private Level 3 and Level 2 page tables for L4 index 0 (low-half)
    // so the kernel mappings stay present but don't conflict with private userspace.
    let new_l3_frame = fa.allocate_frame()?;
    let new_l3_table = unsafe { frame_as_page_table_mut(new_l3_frame) };
    for i in 0..512 {
        new_l3_table[i].set_unused();
    }

    let l3_flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE;
    new_table[0].set_addr(new_l3_frame.start_address(), l3_flags);

    let new_l2_frame = fa.allocate_frame()?;
    let new_l2_table = unsafe { frame_as_page_table_mut(new_l2_frame) };
    for i in 0..512 {
        new_l2_table[i].set_unused();
    }

    let l2_flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE;
    new_l3_table[0].set_addr(new_l2_frame.start_address(), l2_flags);

    // Do not copy parent low-half L2 entries into a process address space.
    // User mappings must start empty; copying the kernel's bootstrap low
    // mappings makes ordinary user VMAs collide with PageAlreadyMapped.

    Some(new_frame)
}
