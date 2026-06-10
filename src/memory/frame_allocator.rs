use spin::Mutex;
// ═══════════════════════════════════════════════════════════════════════════════
// rmm integration status
//
// Tier 1 COMPLETE:
//   - third_party/rmm/ copied from redox-os/kernel
//   - Cargo.toml has `rmm = { path = "third_party/rmm" }` and `bitflags = "2"`
//   - `cargo check -p rmm --target x86_64-unknown-none` → OK
//
// Tier 2 BLOCKED:
//   Full BuddyAllocator/PageMapper swap requires refactoring:
//     - src/init.rs boot-sequence ownership (mapper + FA captured simultaneously)
//     - src/memory/paging.rs KERNEL_MAPPER type
//     - src/memory/heap.rs init_heap signature
//   rmm::PageMapper is self-referential (&mut F in its type), incompatible with
//   current mutable-passing pattern.
//
// This file is intentionally minimal until the boot-sequence refactor lands.
// ═══════════════════════════════════════════════════════════════════════════════

use rmm::FrameAllocator as RmmFrameAllocator;
use x86_64::{
    structures::paging::{FrameAllocator, PhysFrame, Size4KiB},
    PhysAddr,
};

pub struct BootInfoFrameAllocator;

static RMM_ALLOC: Mutex<
    Option<rmm::BuddyAllocator<rmm::x86_64::X8664Arch>>,
> = Mutex::new(None);

unsafe impl FrameAllocator<Size4KiB> for BootInfoFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
        let phys = RMM_ALLOC.lock().as_mut()?.allocate(rmm::FrameCount::new(1))?;
        Some(PhysFrame::containing_address(PhysAddr::new(
            phys.data() as u64,
        )))
    }
}

#[allow(dead_code)]
/// Init hook: provided by boot.rs once the frame allocator is live.
pub fn _rmm_set_allocator(alloc: rmm::BuddyAllocator<rmm::x86_64::X8664Arch>) {
    *RMM_ALLOC.lock() = Some(alloc);
}

/// Init hook: initialize RMM from boot info
pub unsafe fn rmm_init_from_bootinfo(boot_info: &'static bootloader::BootInfo) {
    use rmm::{BumpAllocator, BuddyAllocator, MemoryArea};
    use bootloader::bootinfo::MemoryRegionType;

    // Debug: print all memory regions
    crate::println!("[RMM] Memory map regions:");
    for region in boot_info.memory_map.iter() {
        crate::println!(
            "[RMM]   {:#x}-{:#x} type={:?}",
            region.range.start_addr(),
            region.range.end_addr(),
            region.region_type
        );
    }

    static mut RMM_AREAS: [MemoryArea; 32] = [MemoryArea {
        base: rmm::PhysicalAddress::new(0),
        size: 0,
    }; 32];

    let mut area_count = 0;
    for region in boot_info.memory_map.iter() {
        if region.region_type == MemoryRegionType::Usable {
            if area_count < 32 {
                RMM_AREAS[area_count] = MemoryArea {
                    base: rmm::PhysicalAddress::new(region.range.start_addr() as usize),
                    size: (region.range.end_addr() - region.range.start_addr()) as usize,
                };
                area_count += 1;
            } else {
                crate::println!("[RMM] Warning: more than 32 usable memory regions, some ignored.");
            }
        }
    }
    let areas = &mut RMM_AREAS[..area_count];

    crate::println!("[RMM] Usable areas count: {}", areas.len());
    for (i, area) in areas.iter().enumerate() {
        crate::println!("[RMM]   Area {}: base={:#x} size={:#x}", i, area.base.data(), area.size);
    }

    if areas.is_empty() {
        panic!("No usable memory regions found in bootloader memory map");
    }

    let bump = BumpAllocator::new(areas, 0);
    let buddy = BuddyAllocator::new(bump).expect("Buddy init");

    *RMM_ALLOC.lock() = Some(buddy);
    crate::println!("[RMM] Buddy allocator initialized successfully");
}
