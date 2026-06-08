/// Memory subsystem for ZiqaKernel
/// Re-exports existing frame allocator and heap, plus adds
/// types needed by the process manager and ELF loader.
pub mod frame_allocator;
pub mod heap;
pub mod heapstats;
pub mod paging;
pub mod compression;

pub use frame_allocator::BootInfoFrameAllocator;
pub use paging::{AddressSpace, MemoryRegion, MemoryRegionFlags};
pub use x86_64::VirtAddr;
use x86_64::structures::paging::FrameAllocator;

use spin::Mutex;
pub static FRAME_ALLOCATOR: Mutex<Option<BootInfoFrameAllocator>> = Mutex::new(None);

/// Allocate a 4KB page for per-CPU data.
pub fn allocate_percpu_area() -> u64 {
    let mut fa_guard = FRAME_ALLOCATOR.lock();
    let fa = fa_guard.as_mut().expect("FRAME_ALLOCATOR not initialized");
    let frame = fa.allocate_frame().expect("out of memory for per-CPU area");
    drop(fa_guard);
    let po = paging::phys_offset();
    let vaddr = po + frame.start_address().as_u64();
    unsafe {
        let ptr: *mut u8 = vaddr.as_mut_ptr();
        core::ptr::write_bytes(ptr, 0, PAGE_SIZE);
    }
    frame.start_address().as_u64()
}

/// Page size on x86_64
pub const PAGE_SIZE: usize = 4096;

// ── User-memory access ────────────────────────────────────────────────────────

/// Canonical user-space address ceiling on x86_64 (256 TiB).
pub const USER_ADDR_MAX: u64 = 0x0000_8000_0000_0000;

/// Return `true` if the entire byte range `[addr, addr+len)` lies within
/// user-space and every page in the range is present and user-accessible.
pub fn verify_user_region(addr: u64, len: usize) -> bool {
    if len == 0 {
        return true;
    }
    let end = match addr.checked_add(len as u64) {
        Some(e) => e,
        None => return false, // overflow
    };
    if end > USER_ADDR_MAX {
        return false; // touches kernel space
    }
    // Walk every page in the range.
    let start_page = addr & !(PAGE_SIZE as u64 - 1);
    let mut page = start_page;
    while page < end {
        let vaddr = x86_64::VirtAddr::new(page);
        match paging::get_leaf_entry_mut(vaddr) {
            Some(entry) => {
                use x86_64::structures::paging::PageTableFlags;
                let flags = entry.flags();
                if !flags.contains(PageTableFlags::PRESENT)
                    || !flags.contains(PageTableFlags::USER_ACCESSIBLE)
                {
                    return false;
                }
            }
            None => return false,
        }
        page += PAGE_SIZE as u64;
    }
    true
}

/// Copy `len` bytes from user-space address `src` into `dst`.
///
/// Validates the source range via page-table walk, then performs the copy
/// inside a STAC/CLAC bracket so SMAP does not fault on the access.
///
/// Returns `Ok(())` on success, `Err(())` if the range is invalid.
pub fn copy_from_user(dst: &mut [u8], src: u64) -> Result<(), ()> {
    let len = dst.len();
    if !verify_user_region(src, len) {
        return Err(());
    }
    unsafe {
        crate::arch::x86_64::cpu_features::with_user_access(|| {
            core::ptr::copy_nonoverlapping(src as *const u8, dst.as_mut_ptr(), len);
        });
    }
    Ok(())
}

/// Copy `len` bytes from `src` into user-space address `dst`.
///
/// Validates the destination range via page-table walk, then performs the copy
/// inside a STAC/CLAC bracket.
///
/// Returns `Ok(())` on success, `Err(())` if the range is invalid or read-only.
pub fn copy_to_user(dst: u64, src: &[u8]) -> Result<(), ()> {
    let len = src.len();
    if !verify_user_region(dst, len) {
        return Err(());
    }
    // Also verify the destination pages are writable.
    let start_page = dst & !(PAGE_SIZE as u64 - 1);
    let end = dst + len as u64;
    let mut page = start_page;
    while page < end {
        let vaddr = x86_64::VirtAddr::new(page);
        match paging::get_leaf_entry_mut(vaddr) {
            Some(entry) => {
                use x86_64::structures::paging::PageTableFlags;
                if !entry.flags().contains(PageTableFlags::WRITABLE) {
                    return Err(());
                }
            }
            None => return Err(()),
        }
        page += PAGE_SIZE as u64;
    }
    unsafe {
        crate::arch::x86_64::cpu_features::with_user_access(|| {
            core::ptr::copy_nonoverlapping(src.as_ptr(), dst as *mut u8, len);
        });
    }
    Ok(())
}
