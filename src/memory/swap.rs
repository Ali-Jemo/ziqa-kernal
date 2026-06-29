//! Swap-to-disk backend for ZiqaKernel.
//!
//! Provides a unified swap interface that can store evicted user pages either
//! in a reserved RAM region (always available, used as a fallback and during
//! early boot) or on a block device (production path). Pages are addressed
//! by a `SwapSlot` index.
//!
//! The backend keeps a free-slot bitmap so `alloc_slot` is O(1) amortised and
//! `free_slot` is O(1). Pages are always written/read as 4 KiB units — the
//! minimum page size the rest of the kernel uses.
//!
//! This is intentionally small and dependency-free: it does not perform any
//! page-table walks. Callers (the page-fault path and the eviction policy)
//! own those concerns and pass a `PhysFrame` plus its `phys_offset` mapping
//! to the backend.

use crate::drivers::block::{BlockDevice, SECTOR_SIZE};
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use lazy_static::lazy_static;
use spin::Mutex;
use x86_64::VirtAddr;

/// Number of 4 KiB pages per swap area when using the RAM backend.
const RAM_SWAP_PAGES: usize = 64; // 256 KiB by default

/// Number of 4 KiB pages per swap area when using a block device.
/// Sized to consume ~32 MiB of the disk's free space; the user can grow
/// this at format time.
pub const DISK_SWAP_PAGES: usize = 8192;

/// Opaque handle for a single 4 KiB page slot inside a swap area.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SwapSlot {
    /// Area index (0 for RAM, 1+ for disk areas).
    pub area: u16,
    /// Page index inside the area.
    pub index: u32,
}

impl SwapSlot {
    pub const fn invalid() -> Self {
        Self {
            area: u16::MAX,
            index: u32::MAX,
        }
    }

    pub fn is_valid(self) -> bool {
        self.area != u16::MAX
    }
}

/// Storage backing a single swap area.
enum SwapStorage {
    /// RAM-backed: a plain `Vec<u8>` large enough to hold `pages` pages.
    Ram(Vec<u8>),
    /// Disk-backed: a reference to a block device plus a starting sector.
    Disk {
        device: Arc<dyn BlockDevice>,
        start_sector: u64,
        sectors_per_page: u32,
    },
}

/// A simple packed bit-set: 1 bit per slot. We store one byte per 8 pages.
struct FreeBitmap {
    bytes: Vec<u8>,
    /// Number of slots tracked.
    len: usize,
}

impl FreeBitmap {
    fn new(len: usize, fill: bool) -> Self {
        let nbytes = (len + 7) / 8;
        let mut bytes = vec![0u8; nbytes];
        if fill {
            // Set every bit in the used range.
            for byte in bytes.iter_mut() {
                *byte = 0xFF;
            }
            // Zero out the tail bits in the last byte that don't represent
            // a real slot.
            let tail = len % 8;
            if tail != 0 {
                let mask = (1u8 << tail) - 1;
                if let Some(last) = bytes.last_mut() {
                    *last &= mask;
                }
            }
        }
        Self { bytes, len }
    }

    /// Set the bit at `idx`: true means free, false means used.
    fn set(&mut self, idx: usize, val: bool) {
        if idx >= self.len {
            return;
        }
        let byte = idx / 8;
        let bit = idx % 8;
        let mask = 1u8 << bit;
        if val {
            self.bytes[byte] |= mask;
        } else {
            self.bytes[byte] &= !mask;
        }
    }

    fn get(&self, idx: usize) -> bool {
        if idx >= self.len {
            return false;
        }
        let byte = idx / 8;
        let bit = idx % 8;
        (self.bytes[byte] >> bit) & 1 == 1
    }

    /// Find the lowest index whose bit is set. Returns `None` if no bit
    /// is set.
    fn find_first_set(&self) -> Option<usize> {
        for (byte_idx, byte) in self.bytes.iter().enumerate() {
            if *byte == 0 {
                continue;
            }
            // Find the lowest set bit.
            let bit = byte.trailing_zeros() as usize;
            let idx = byte_idx * 8 + bit;
            if idx < self.len {
                return Some(idx);
            }
        }
        None
    }
}

/// A single contiguous swap area.
struct SwapArea {
    storage: SwapStorage,
    /// Bitmap of which pages are free. 1 = free, 0 = used.
    free_bitmap: FreeBitmap,
    /// How many free pages remain.
    free_count: usize,
}

impl SwapArea {
    fn new_ram() -> Self {
        let bytes = vec![0u8; RAM_SWAP_PAGES * 4096];
        let bm = FreeBitmap::new(RAM_SWAP_PAGES, true);
        Self {
            storage: SwapStorage::Ram(bytes),
            free_bitmap: bm,
            free_count: RAM_SWAP_PAGES,
        }
    }

    fn new_disk(device: Arc<dyn BlockDevice>, start_sector: u64, pages: usize) -> Option<Self> {
        let sectors_per_page = (4096 / SECTOR_SIZE) as u32;
        let bm = FreeBitmap::new(pages, true);
        Some(Self {
            storage: SwapStorage::Disk {
                device,
                start_sector,
                sectors_per_page,
            },
            free_bitmap: bm,
            free_count: pages,
        })
    }

    fn write_page(&mut self, index: u32, page: &[u8; 4096]) -> Result<(), &'static str> {
        if (index as usize) >= self.free_bitmap.len {
            return Err("swap slot out of range");
        }
        match &mut self.storage {
            SwapStorage::Ram(buf) => {
                let off = index as usize * 4096;
                buf[off..off + 4096].copy_from_slice(page);
                Ok(())
            }
            SwapStorage::Disk {
                device,
                start_sector,
                sectors_per_page,
            } => {
                let sector = *start_sector + (index as u64) * (*sectors_per_page as u64);
                device
                    .write_sectors(sector, *sectors_per_page, page)
                    .map_err(|_| "swap disk write failed")
            }
        }
    }

    fn read_page(&self, index: u32, page: &mut [u8; 4096]) -> Result<(), &'static str> {
        if (index as usize) >= self.free_bitmap.len {
            return Err("swap slot out of range");
        }
        match &self.storage {
            SwapStorage::Ram(buf) => {
                let off = index as usize * 4096;
                page.copy_from_slice(&buf[off..off + 4096]);
                Ok(())
            }
            SwapStorage::Disk {
                device,
                start_sector,
                sectors_per_page,
            } => {
                let sector = *start_sector + (index as u64) * (*sectors_per_page as u64);
                device
                    .read_sectors(sector, *sectors_per_page, page)
                    .map_err(|_| "swap disk read failed")
            }
        }
    }
}

/// The global swap backend. Initialised in `init_services`. Falls back to
/// RAM-only if no block device is registered.
struct SwapBackendInner {
    areas: Vec<SwapArea>,
    /// Total number of pages successfully written to swap (for stats).
    total_writes: u64,
    /// Total number of pages read back from swap.
    total_reads: u64,
    /// Total number of evictions that failed because the backend was full.
    failed_evictions: u64,
}

impl SwapBackendInner {
    const fn new() -> Self {
        Self {
            areas: Vec::new(),
            total_writes: 0,
            total_reads: 0,
            failed_evictions: 0,
        }
    }

    /// Allocate a slot from any area. RAM is preferred (cheaper), then disk
    /// in area-index order.
    fn alloc_slot(&mut self) -> Option<SwapSlot> {
        for (area_idx, area) in self.areas.iter_mut().enumerate() {
            if area.free_count == 0 {
                continue;
            }
            if let Some(pos) = area.free_bitmap.find_first_set() {
                area.free_bitmap.set(pos, false);
                area.free_count -= 1;
                return Some(SwapSlot {
                    area: area_idx as u16,
                    index: pos as u32,
                });
            }
        }
        None
    }

    fn free_slot(&mut self, slot: SwapSlot) {
        if let Some(area) = self.areas.get_mut(slot.area as usize) {
            let idx = slot.index as usize;
            if idx < area.free_bitmap.len && !area.free_bitmap.get(idx) {
                area.free_bitmap.set(idx, true);
                area.free_count += 1;
            }
        }
    }

    fn write_page(&mut self, slot: SwapSlot, page: &[u8; 4096]) -> Result<(), &'static str> {
        let area = self
            .areas
            .get_mut(slot.area as usize)
            .ok_or("invalid swap area")?;
        area.write_page(slot.index, page)?;
        self.total_writes += 1;
        Ok(())
    }

    fn read_page(&mut self, slot: SwapSlot, page: &mut [u8; 4096]) -> Result<(), &'static str> {
        let area = self
            .areas
            .get(slot.area as usize)
            .ok_or("invalid swap area")?;
        area.read_page(slot.index, page)?;
        self.total_reads += 1;
        Ok(())
    }
}

lazy_static! {
    static ref SWAP_BACKEND: Mutex<SwapBackendInner> = Mutex::new(SwapBackendInner::new());
}

/// Initialise the swap subsystem. Always provisions a RAM area so the system
/// has a place to put pages even when no disk is registered. If a disk is
/// available and `use_disk` is true, a 32 MiB disk-backed area is also
/// added.
pub fn init(use_disk: bool) {
    let mut backend = SWAP_BACKEND.lock();
    // Area 0 is always RAM.
    if backend.areas.is_empty() {
        backend.areas.push(SwapArea::new_ram());
    }
    if use_disk {
        if let Some(entry) = crate::drivers::block_registry::first() {
            // Reserve a region near the end of the disk. The format-time
            // ZiqaFS code typically carves a partition table at the front;
            // we simply use a high-LBA area to avoid clobbering it.
            let total = entry.device.total_sectors();
            let sp = (4096 / SECTOR_SIZE) as u64;
            let required = (DISK_SWAP_PAGES as u64) * sp;
            if total > required + 2048 {
                let start = total - required;
                // Sanity: not overlapping with our RAM area index 0.
                if backend.areas.len() == 1 {
                    if let Some(area) =
                        SwapArea::new_disk(entry.device.clone(), start, DISK_SWAP_PAGES)
                    {
                        backend.areas.push(area);
                        crate::klog!(
                            crate::klog::Level::Info,
                            "swap: disk area ready at sector {} ({} pages)",
                            start,
                            DISK_SWAP_PAGES
                        );
                    }
                }
            }
        }
    }
    crate::klog!(
        crate::klog::Level::Info,
        "swap: backend online ({} area(s), {} KiB total)",
        backend.areas.len(),
        backend.areas.iter().map(|a| a.free_bitmap.len * 4).sum::<usize>()
    );
}

/// Write a physical 4 KiB page to swap. Returns the assigned slot or `None`
/// if the backend is exhausted. On success, the caller may reuse the frame
/// — the page contents now live in the swap area.
pub fn swap_out(frame_paddr: u64) -> Option<SwapSlot> {
    let po = crate::memory::paging::phys_offset();
    let src = VirtAddr::new(po.as_u64() + frame_paddr).as_ptr::<u8>();
    let mut page = [0u8; 4096];
    unsafe {
        core::ptr::copy_nonoverlapping(src, page.as_mut_ptr(), 4096);
    }

    let mut backend = SWAP_BACKEND.lock();
    let slot = match backend.alloc_slot() {
        Some(s) => s,
        None => {
            backend.failed_evictions += 1;
            return None;
        }
    };
    if backend.write_page(slot, &page).is_err() {
        backend.free_slot(slot);
        backend.failed_evictions += 1;
        return None;
    }
    Some(slot)
}

pub fn alloc_slot() -> Option<SwapSlot> {
    SWAP_BACKEND.lock().alloc_slot()
}

/// Read a 4 KiB page from swap back into a fresh frame. The destination
/// frame must already be allocated (this function does not allocate one).
///
/// The frame is then re-mapped by the caller (the page-fault or page-in
/// path) into the target address space.
pub fn swap_in(slot: SwapSlot, dst_frame_paddr: u64) -> Result<(), &'static str> {
    let po = crate::memory::paging::phys_offset();
    let dst = (po.as_u64() + dst_frame_paddr) as *mut u8;

    let mut backend = SWAP_BACKEND.lock();
    let mut page = [0u8; 4096];
    backend.read_page(slot, &mut page)?;
    // Drop the lock before the copy: we don't want the swap mutex held
    // across the memcpy. The page buffer is on our stack so the bytes are
    // safe to use.
    drop(backend);
    unsafe {
        core::ptr::copy_nonoverlapping(page.as_ptr(), dst, 4096);
    }
    Ok(())
}

/// Release a slot. Called after a page has been successfully brought back
/// from swap.
pub fn free_slot(slot: SwapSlot) {
    SWAP_BACKEND.lock().free_slot(slot);
}

/// Number of free pages across all swap areas.
pub fn free_page_count() -> usize {
    SWAP_BACKEND.lock().areas.iter().map(|a| a.free_count).sum()
}

/// Number of pages currently in use across all swap areas.
pub fn used_page_count() -> usize {
    SWAP_BACKEND
        .lock()
        .areas
        .iter()
        .map(|a| a.free_bitmap.len - a.free_count)
        .sum()
}

/// Return a tuple of (writes, reads, failed_evictions) since boot. Cheap
/// because we just lock and read three integers.
pub fn stats() -> (u64, u64, u64) {
    let b = SWAP_BACKEND.lock();
    (b.total_writes, b.total_reads, b.failed_evictions)
}

/// Total capacity in pages across all swap areas.
pub fn capacity_pages() -> usize {
    SWAP_BACKEND.lock().areas.iter().map(|a| a.free_bitmap.len).sum()
}
