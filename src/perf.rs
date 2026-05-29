use crate::memory::heapstats;
/// Performance benchmarking for ZiqaKernel
///
/// Provides utilities to measure scheduler, memory, and I/O performance
use spin::Mutex;

#[derive(Debug, Clone, Copy)]
pub struct BenchmarkResult {
    pub name: &'static str,
    pub iterations: u64,
    pub total_cycles: u64,
    pub avg_cycles_per_op: u64,
    pub min_cycles: u64,
    pub max_cycles: u64,
}

impl BenchmarkResult {
    pub fn avg_ns(&self) -> u64 {
        // Approximate: 1 cycle ≈ 0.5 ns on modern x86_64 @ 2GHz
        self.avg_cycles_per_op / 2
    }
}

pub struct Benchmark {
    name: &'static str,
    iterations: u64,
    total_cycles: u64,
    min_cycles: u64,
    max_cycles: u64,
}

impl Benchmark {
    pub fn new(name: &'static str, iterations: u64) -> Self {
        Benchmark {
            name,
            iterations,
            total_cycles: 0,
            min_cycles: u64::MAX,
            max_cycles: 0,
        }
    }

    /// Record a measurement in CPU cycles
    pub fn record_cycles(&mut self, cycles: u64) {
        self.total_cycles += cycles;
        self.min_cycles = self.min_cycles.min(cycles);
        self.max_cycles = self.max_cycles.max(cycles);
    }

    pub fn result(&self) -> BenchmarkResult {
        BenchmarkResult {
            name: self.name,
            iterations: self.iterations,
            total_cycles: self.total_cycles,
            avg_cycles_per_op: if self.iterations > 0 {
                self.total_cycles / self.iterations
            } else {
                0
            },
            min_cycles: self.min_cycles,
            max_cycles: self.max_cycles,
        }
    }
}

pub struct PerformanceSuite {
    results: [Option<BenchmarkResult>; 8],
    count: usize,
}

impl PerformanceSuite {
    pub const fn new() -> Self {
        const NONE: Option<BenchmarkResult> = None;
        PerformanceSuite {
            results: [NONE; 8],
            count: 0,
        }
    }

    pub fn add_result(&mut self, result: BenchmarkResult) {
        if self.count < 8 {
            self.results[self.count] = Some(result);
            self.count += 1;
        }
    }

    pub fn report(&self) {
        crate::println!("\n━━━ Performance Benchmark Report ━━━");
        crate::println!("Heap Statistics:");
        let heap_stats = heapstats::get_stats();
        crate::println!("  Total Allocations: {}", heap_stats.total_allocations);
        crate::println!("  Total Frees: {}", heap_stats.total_frees);
        crate::println!(
            "  Current Usage: {} bytes",
            heap_stats.current_usage_bytes()
        );
        crate::println!("  Peak Usage: {} bytes", heap_stats.peak_usage_bytes);
        crate::println!("  Current Blocks: {}", heap_stats.current_blocks);

        crate::println!("\nBenchmark Results:");
        for i in 0..self.count {
            if let Some(result) = self.results[i] {
                crate::println!("  {}", result.name);
                crate::println!("    Iterations: {}", result.iterations);
                crate::println!("    Avg Cycles/Op: {}", result.avg_cycles_per_op);
                crate::println!("    Min Cycles: {}", result.min_cycles);
                crate::println!("    Max Cycles: {}", result.max_cycles);
                crate::println!("    Estimated Avg: ~{} ns", result.avg_ns());
            }
        }
    }
}

pub static PERF_SUITE: Mutex<PerformanceSuite> = Mutex::new(PerformanceSuite::new());

/// Helper to read CPU cycles (x86_64 RDTSC)
#[inline]
pub fn read_cycles() -> u64 {
    unsafe { core::arch::x86_64::_rdtsc() }
}

/// Quick benchmark: measure time for a closure
pub fn benchmark<F>(name: &'static str, iterations: u64, mut f: F) -> BenchmarkResult
where
    F: FnMut(),
{
    let mut bench = Benchmark::new(name, iterations);

    for _ in 0..iterations {
        let start = read_cycles();
        f();
        let end = read_cycles();
        bench.record_cycles(end.saturating_sub(start));
    }

    bench.result()
}

/// Benchmark page cache performance
pub fn benchmark_page_cache() {
    crate::println!("\n━━━ Page Cache Benchmark ━━━");

    use crate::fs::pagecache::{cache_page, get_cached_page, PageKey};

    let test_data = [42u8; 4096];
    let key = PageKey {
        file_id: 1,
        page_num: 0,
    };

    // Warm up
    let _ = cache_page(key, &test_data);

    // Benchmark cache hits
    let hits = benchmark("PageCache Hit", 1000, || {
        let _ = get_cached_page(key);
    });

    // Benchmark cache misses + inserts
    let misses = benchmark("PageCache Miss+Insert", 100, || {
        let key2 = PageKey {
            file_id: 2,
            page_num: 100,
        };
        let _ = cache_page(key2, &test_data);
    });

    crate::println!(
        "  Cache Hit: {} cycles/op (~{} ns)",
        hits.avg_cycles_per_op,
        hits.avg_ns()
    );
    crate::println!(
        "  Cache Miss: {} cycles/op (~{} ns)",
        misses.avg_cycles_per_op,
        misses.avg_ns()
    );
}
