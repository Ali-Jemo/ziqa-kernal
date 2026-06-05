# Performance Optimization Path (Research Path)

This document describes the kernel profiling infrastructure for analyzing COW fork, page table cloning, and TLB shootdown performance under multi-core workloads.

## Overview

The performance optimization stack provides:

1. **Kernel Tracepoints** - Low-overhead, in-kernel event logging
2. **Fork Benchmark** - Measures COW fork latency and overhead
3. **TLB Shootdown Benchmark** - Measures inter-CPU TLB invalidation latency
4. **Statistics Collection** - Aggregates and reports performance metrics

## Building with Performance Benchmarks

Enable the `perf-benchmarks` feature to include the benchmarking code:

```bash
cargo build --features perf-benchmarks
```

## Running the Benchmarks

### From the Shell (when built with `perf-benchmarks`)

The benchmarks can be invoked from the kernel shell once initialized. The kernel automatically initializes tracepoints during SMP initialization.

### From Code

Call the benchmark entry points directly:

```rust
// Fork benchmark - measures COW fork overhead
crate::bench_fork::bench_fork_entry();

// TLB shootdown benchmark - measures TLB invalidation latency
crate::bench_tlb::bench_tlb_entry();
```

## Tracepoints

The kernel tracepoint infrastructure is defined in `src/tracepoints.rs` and provides the following events:

| Event | Description |
|-------|-------------|
| `PgdAlloc` | L4 (PGD) page table allocation |
| `PudAlloc` | L3 (PUD) page table allocation |
| `PmdAlloc` | L2 (PMD) page table allocation |
| `PteAlloc` | L1 (PTE) page table allocation |
| `CowForkStart` | Start of COW fork operation |
| `CowForkEnd` | End of COW fork operation |
| `TlbShootdownStart` | Start of TLB shootdown (address in data, CPU count in upper bits) |
| `TlbShootdownSend` | Send IPI to a CPU |
| `TlbShootdownComplete` | Complete TLB shootdown |
| `PageFaultCow` | COW page fault triggered |
| `PageFaultCowComplete` | COW page fault completed |

## Using External Profiling Tools

If the kernel is running under a hypervisor that supports external profiling, you can use standard Linux tools:

### Using perf

```bash
perf record -e cpu-cycles,instructions,cache-misses -a ./run-qemu.sh
perf report
```

### Using bpftrace

```bash
bpftrace -e '
tracepoint:sched:sched_fork { @forks[tid] = nsecs; }
tracepoint:sched:sched_fork /@forks[tid]/ {
    @latency = hist((nsecs - @forks[tid])/1000);
    delete(@forks[tid]);
}
'
```

## Interpreting Results

### Fork Benchmark Output

```
━━━ Fork Benchmark Results ━━━
  Iterations:         20000
  Total Cycles:       12345678
  Average Cycles:     617
  Min Cycles:         400
  Max Cycles:         2000
  Estimated Avg:      ~308 ns
  Page Table Allocs:   80000
  TLB Shootdowns:     20000
  Avg Shootdowns/Fork: 1.00
```

### TLB Shootdown Benchmark Output

```
━━━ TLB Shootdown Benchmark Results ━━━
  Iterations:           8000
  Total Cycles:         3200000
  Average Cycles:       400
  Min Cycles:           200
  Max Cycles:           1200
  Estimated Avg:        ~200 ns
  TLB Shootdowns:       8000
  Avg Shootdown Latency: ~2 us
```

## Optimization Guidelines

### Page Table Allocation Overhead

If page table allocations dominate cow fork time (>70%):

1. Consider pre-allocating page table pools
2. Use batched allocation strategies
3. Implement page table caching for frequent forks

### TLB Shootdown Latency

If TLB shootdown latency scales poorly with CPU count:

1. Consider using `INVPCID` for targeted invalidations
2. Implement lazy TLB flush strategies
3. Use per-CPU memory pools to reduce cross-CPU invalidations

### COW Page Fault Overhead

If COW page fault latency is high:

1. Check for excessive page copying
2. Consider huge page support for large anonymous mappings
3. Optimize frame allocation for COW pages

## Adding New Tracepoints

To add a new tracepoint:

1. Add the event to the `TracepointId` enum in `src/tracepoints.rs`
2. Create a `trace_<event_name>()` function
3. Call the function at the appropriate location in your code

```rust
// In tracepoints.rs
pub fn trace_my_event(data: u64) {
    write_tracepoint(TracepointId::MyEvent, data);
}

// In your code
crate::tracepoints::trace_my_event(some_value);
```

## Files Modified/Created

- `src/tracepoints.rs` - Tracepoint infrastructure
- `src/bench_fork.rs` - Fork benchmark (perf-benchmarks feature)
- `src/bench_tlb.rs` - TLB shootdown benchmark (perf-benchmarks feature)
- `src/memory/paging.rs` - Added tracepoints to COW fork and page table code
- `src/arch/x86_64/smp.rs` - Added tracepoints to TLB shootdown code
- `Cargo.toml` - Added `perf-benchmarks` feature