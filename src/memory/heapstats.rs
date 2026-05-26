/// Heap profiling and statistics for ZiqaKernel
///
/// Tracks memory allocations, fragmentation, and usage patterns.

use spin::Mutex;

#[derive(Debug, Clone, Copy)]
pub struct HeapStats {
    pub total_allocations: u64,
    pub total_frees: u64,
    pub current_blocks: u32,
    pub total_allocated_bytes: u64,
    pub total_freed_bytes: u64,
    pub peak_usage_bytes: u64,
}

impl HeapStats {
    pub fn current_usage_bytes(&self) -> u64 {
        self.total_allocated_bytes - self.total_freed_bytes
    }

    pub fn allocation_rate(&self) -> f64 {
        if self.total_allocations == 0 {
            0.0
        } else {
            self.total_frees as f64 / self.total_allocations as f64
        }
    }

    pub fn fragmentation_ratio(&self) -> f64 {
        if self.current_blocks == 0 {
            return 0.0;
        }
        (self.total_allocated_bytes / self.current_blocks as u64) as f64
    }
}

pub struct HeapProfiler {
    stats: HeapStats,
}

impl HeapProfiler {
    pub const fn new() -> Self {
        HeapProfiler {
            stats: HeapStats {
                total_allocations: 0,
                total_frees: 0,
                current_blocks: 0,
                total_allocated_bytes: 0,
                total_freed_bytes: 0,
                peak_usage_bytes: 0,
            },
        }
    }

    pub fn record_allocation(&mut self, size: u64) {
        self.stats.total_allocations += 1;
        self.stats.current_blocks += 1;
        self.stats.total_allocated_bytes += size;

        let current = self.stats.current_usage_bytes();
        if current > self.stats.peak_usage_bytes {
            self.stats.peak_usage_bytes = current;
        }
    }

    pub fn record_deallocation(&mut self, size: u64) {
        self.stats.total_frees += 1;
        self.stats.current_blocks = self.stats.current_blocks.saturating_sub(1);
        self.stats.total_freed_bytes += size;
    }

    pub fn get_stats(&self) -> HeapStats {
        self.stats
    }

    pub fn reset(&mut self) {
        self.stats = HeapStats {
            total_allocations: 0,
            total_frees: 0,
            current_blocks: 0,
            total_allocated_bytes: 0,
            total_freed_bytes: 0,
            peak_usage_bytes: 0,
        };
    }
}

/// Global heap profiler
pub static HEAP_PROFILER: Mutex<HeapProfiler> = Mutex::new(HeapProfiler::new());

/// Record an allocation
pub fn record_alloc(size: u64) {
    HEAP_PROFILER.lock().record_allocation(size);
}

/// Record a deallocation
pub fn record_dealloc(size: u64) {
    HEAP_PROFILER.lock().record_deallocation(size);
}

/// Get current heap statistics
pub fn get_stats() -> HeapStats {
    HEAP_PROFILER.lock().get_stats()
}
