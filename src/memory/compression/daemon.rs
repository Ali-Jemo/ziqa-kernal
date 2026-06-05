use alloc::vec::Vec;
use x86_64::VirtAddr;
use x86_64::structures::paging::PageTableFlags;
use crate::memory::PAGE_SIZE;
use crate::memory::compression::classifier;
use crate::memory::compression::fault::COMPRESSED_BIT;
use crate::memory::compression::COMPRESSION_ENGINE;
use crate::memory::compression::PAGE_STORE;

/// Run one cycle of the background compression daemon.
///
/// Scans pages across all active processes, compressing those that are
/// present and classified as compressible.  At most `max_pages` pages
/// are processed per call so the caller can rate-limit by invoking this
/// periodically from the idle loop or a shell command.
///
/// Returns the number of pages actually compressed.
pub fn run_daemon_cycle(max_pages: usize) -> usize {
    let pids = crate::process::scheduler::list_pids();
    let mut total_compressed = 0;

    'pids: for pid in &pids {
        if total_compressed >= max_pages {
            break;
        }

        // Snapshot the data we need from under the process lock.
        let (pt_frame, vmas) = match crate::process::scheduler::with_process(*pid, |proc| {
            let is_alive = !matches!(proc.state, crate::process::ProcessState::Exited(_));
            if !is_alive {
                (None, Vec::new())
            } else {
                (proc.page_table_frame, proc.vmas.clone())
            }
        }) {
            Some(data) => data,
            None => continue,
        };

        for vma in &vmas {
            if total_compressed >= max_pages {
                break 'pids;
            }

            let start = vma.start.as_u64();
            let end = vma.end.as_u64();

            let mut page_addr_val = start;
            while page_addr_val < end && total_compressed < max_pages {
                let page_addr = VirtAddr::new(page_addr_val);

                // Walk the process's page table to find the leaf PTE.
                let entry = match pt_frame {
                    Some(frame) => crate::memory::paging::get_leaf_entry_mut_in(frame, page_addr),
                    None => crate::memory::paging::get_leaf_entry_mut(page_addr),
                };

                if let Some(entry) = entry {
                    let flags = entry.flags();

                    // Only compress present pages that haven't been compressed yet.
                    if flags.contains(PageTableFlags::PRESENT) && !flags.contains(COMPRESSED_BIT) {
                        // Read page content from physical memory.
                        let phys_addr = entry.addr();
                        let virt = crate::memory::paging::phys_offset() + phys_addr.as_u64();
                        let page_data = unsafe {
                            core::slice::from_raw_parts(virt.as_ptr::<u8>(), PAGE_SIZE)
                        };

                        // Classify – skip incompressible pages early.
                        let classification = classifier::classify(page_data);
                        if classification.should_compress {
                            // Compress.
                            if let Some(compressed) = COMPRESSION_ENGINE.compress(page_data) {
                                // Store in the compressed-page store.
                                if PAGE_STORE.store(page_addr, &compressed, classification.recommended_tier) {
                                    // Mark the PTE: clear PRESENT, set COMPRESSED_BIT.
                                    let mut new_flags = flags;
                                    new_flags.remove(PageTableFlags::PRESENT);
                                    new_flags.insert(COMPRESSED_BIT);
                                    entry.set_addr(phys_addr, new_flags);

                                    crate::memory::paging::smp_tlb_flush(page_addr);
                                    total_compressed += 1;
                                }
                            }
                        }
                    }
                }

                page_addr_val += PAGE_SIZE as u64;
            }
        }
    }

    total_compressed
}

/// Return a summary of the current compression state for diagnostics.
pub fn daemon_status() -> alloc::string::String {
    use alloc::format;
    let pids = crate::process::scheduler::list_pids();
    let mut total_pages = 0usize;

    for pid in &pids {
        let vmas = crate::process::scheduler::with_process(*pid, |proc| {
            proc.vmas.clone()
        });
        if let Some(vmas) = vmas {
            for vma in &vmas {
                let size = (vma.end.as_u64() - vma.start.as_u64()) as usize;
                total_pages += size / PAGE_SIZE;
            }
        }
    }

    format!(
        "compression-daemon: {} processes, {} total pages",
        pids.len(),
        total_pages,
    )
}
