/// Page cache layer for ZiqaKernel filesystem
///
/// Caches recently accessed filesystem pages to reduce disk I/O.
/// Uses an LRU (Least Recently Used) eviction policy.
extern crate alloc;

use crate::abi::AbiError;
use spin::Mutex;

const PAGE_SIZE: usize = 4096;
const CACHE_PAGES: usize = 16; // 64 KB cache

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PageKey {
    /// File identifier (inode-like)
    pub file_id: u32,
    /// Page offset within the file
    pub page_num: u32,
}

struct CacheEntry {
    key: PageKey,
    data: [u8; PAGE_SIZE],
    size: usize,
    access_count: u64,
    dirty: bool,
}

pub struct PageCache {
    entries: [Option<CacheEntry>; CACHE_PAGES],
    access_epoch: u64,
}

impl PageCache {
    pub const fn new() -> Self {
        const NONE: Option<CacheEntry> = None;
        PageCache {
            entries: [NONE; CACHE_PAGES],
            access_epoch: 0,
        }
    }

    /// Fetch a page from cache, or return None if not cached
    pub fn get(&mut self, key: PageKey) -> Option<&[u8]> {
        self.access_epoch += 1;
        for entry in self.entries.iter_mut() {
            if let Some(e) = entry {
                if e.key == key {
                    e.access_count = self.access_epoch;
                    return Some(&e.data[..e.size]);
                }
            }
        }
        None
    }

    /// Insert a page into the cache
    pub fn put(&mut self, key: PageKey, data: &[u8]) -> Result<(), AbiError> {
        if data.len() > PAGE_SIZE {
            return Err(AbiError::OutOfMemory);
        }

        // Check if already cached
        for entry in self.entries.iter_mut() {
            if let Some(e) = entry {
                if e.key == key {
                    e.data[..data.len()].copy_from_slice(data);
                    e.size = data.len();
                    e.access_count = self.access_epoch;
                    e.dirty = true;
                    return Ok(());
                }
            }
        }

        // Find an empty slot or evict LRU
        let mut min_access = u64::MAX;
        let mut lru_idx = None;

        for (i, entry) in self.entries.iter().enumerate() {
            if entry.is_none() {
                lru_idx = Some(i);
                break;
            }
            if let Some(e) = entry {
                if e.access_count < min_access {
                    min_access = e.access_count;
                    lru_idx = Some(i);
                }
            }
        }

        if let Some(idx) = lru_idx {
            let mut new_entry = CacheEntry {
                key,
                data: [0u8; PAGE_SIZE],
                size: data.len(),
                access_count: self.access_epoch,
                dirty: true,
            };
            new_entry.data[..data.len()].copy_from_slice(data);
            self.entries[idx] = Some(new_entry);
            Ok(())
        } else {
            Err(AbiError::OutOfMemory)
        }
    }

    /// Mark a cached page as dirty (modified)
    pub fn mark_dirty(&mut self, key: PageKey) {
        for entry in self.entries.iter_mut() {
            if let Some(e) = entry {
                if e.key == key {
                    e.dirty = true;
                    break;
                }
            }
        }
    }

    /// Flush dirty pages (in a real kernel, write to disk)
    pub fn flush(&mut self) -> usize {
        let mut flushed = 0;
        for entry in self.entries.iter_mut() {
            if let Some(e) = entry {
                if e.dirty {
                    // In a real kernel, write e.data to persistent storage
                    e.dirty = false;
                    flushed += 1;
                }
            }
        }
        flushed
    }

    /// Get cache stats for monitoring
    pub fn stats(&self) -> CacheStats {
        let mut total_entries = 0;
        let mut dirty_entries = 0;

        for entry in self.entries.iter() {
            if let Some(e) = entry {
                total_entries += 1;
                if e.dirty {
                    dirty_entries += 1;
                }
            }
        }

        CacheStats {
            total_entries,
            dirty_entries,
            capacity: CACHE_PAGES,
            epoch: self.access_epoch,
        }
    }

    /// Evict all entries (clear cache)
    pub fn clear(&mut self) {
        for entry in self.entries.iter_mut() {
            *entry = None;
        }
        self.access_epoch = 0;
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CacheStats {
    pub total_entries: usize,
    pub dirty_entries: usize,
    pub capacity: usize,
    pub epoch: u64,
}

/// Global page cache instance
pub static PAGE_CACHE: Mutex<PageCache> = Mutex::new(PageCache::new());

/// Helper to get from global cache
pub fn get_cached_page(key: PageKey) -> Option<alloc::vec::Vec<u8>> {
    let mut cache = PAGE_CACHE.lock();
    cache.get(key).map(|data| {
        let mut vec = alloc::vec::Vec::new();
        vec.extend_from_slice(data);
        vec
    })
}

/// Helper to put into global cache
pub fn cache_page(key: PageKey, data: &[u8]) -> Result<(), AbiError> {
    PAGE_CACHE.lock().put(key, data)
}
