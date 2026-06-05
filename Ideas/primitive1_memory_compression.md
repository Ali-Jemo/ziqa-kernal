# Primitive 1: Aggressive Kernel-Level Memory Compression

## ZiqaKernel Component: `mm::compression`

**Target Impact:** 4GB RAM → effective 12-16GB, transparent to userspace
**Primary Use Case:** Rejuvenation of old hardware
**Publishable As:** "Tier-Aware Aggressive Memory Compression with Capability Isolation"

---

## 1. Architecture Overview

### Memory Tier Model

```
┌─────────────────────────────────────────────────────────┐
│  Tier 0 (HOT)   - Uncompressed, in real RAM            │
│                    Recent access, active pages          │
├─────────────────────────────────────────────────────────┤
│  Tier 1 (WARM)  - LZ4 compressed, in RAM                │
│                    Accessed within 60s, fast decompress │
├─────────────────────────────────────────────────────────┤
│  Tier 2 (COLD)  - Zstd compressed, in RAM               │
│                    Not accessed in minutes, dense       │
├─────────────────────────────────────────────────────────┤
│  Tier 3 (FROZEN) - Compressed + serialized to SSD      │
│                    Long-term inactive, offloaded         │
└─────────────────────────────────────────────────────────┘
```

### Component Map

```
userspace
   ↓
   ↓ (no API change - transparent)
   ↓
┌──────────────────────────────────────────────────┐
│  Kernel mm::compression subsystem                │
│  ┌────────────────────────────────────────────┐  │
│  │  Page Classifier (hot/cold prediction)     │  │
│  └────────────────────────────────────────────┘  │
│  ┌────────────────────────────────────────────┐  │
│  │  Compression Engine                        │  │
│  │   - Zstd (cold) - ratio 2.8-3.5x          │  │
│  │   - LZ4  (warm) - ratio 2.0-2.5x, fast    │  │
│  │   - Skip (entropy > 0.9 - already packed) │  │
│  └────────────────────────────────────────────┘  │
│  ┌────────────────────────────────────────────┐  │
│  │  Compressed Page Store (slab allocator)    │  │
│  │   - Per-CPU pools (SMP-aware)              │  │
│  │   - Capability-tagged entries              │  │
│  └────────────────────────────────────────────┘  │
│  ┌────────────────────────────────────────────┐  │
│  │  Page Fault Handler (decompression path)   │  │
│  │   - Fast path: in-place swap (LZ4)        │  │
│  │   - Slow path: full decompress (Zstd)     │  │
│  └────────────────────────────────────────────┘  │
│  ┌────────────────────────────────────────────┐  │
│  │  eBPF Hook Points                          │  │
│  │   - Override classification                │  │
│  │   - Set compression policy                 │  │
│  │   - Subscribe to events                    │  │
│  └────────────────────────────────────────────┘  │
│  ┌────────────────────────────────────────────┐  │
│  │  Capability Integration                    │  │
│  │   - Per-cap memory budget                  │  │
│  │   - Per-cap compression ratio              │  │
│  │   - Instant Revocation → drop compressed   │  │
│  └────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────┘
```

---

## 2. Core Components — Detailed Design

### 2.1 Page Classifier

**Responsibility:** Decide which tier each page belongs to.

**Phase 1 (Initial) — LRU with PTE bits:**

```rust
// src/mm/compression/classifier.rs

pub struct PageScore {
    pub pfn: PhysFrame,           // physical frame number
    pub score: i64,                // hot/cold score
    pub last_access: u64,          // timestamp
    pub access_count_1s: u32,      // accesses in last 1s
    pub tier: Tier,
}

pub enum Tier { T0, T1, T2, T3 }

pub fn classify_page(score: &PageScore) -> Tier {
    // Hot: accessed 5+ times in last second
    if score.access_count_1s >= 5 { return Tier::T0; }

    // Warm: accessed 1+ time in last 60s
    let age_ns = current_ns() - score.last_access;
    if age_ns < 60_000_000_000 {
        return Tier::T1;
    }

    // Cold: not accessed in minutes
    if age_ns < 600_000_000_000 {
        return Tier::T2;
    }

    // Frozen: long-term inactive
    Tier::T3
}
```

**Hot path (every page access):**
- PTE Accessed bit is set by hardware automatically
- Page fault handler reads it, increments score, clears bit
- O(1) update using a per-frame metadata array

**Phase 2 (ML-Enhanced):**
- Logistic regression model: features = [time_of_day, process_id_hash, page_type, recent_pattern]
- 50KB model stored in kernel
- Online learning: per-user feedback loop
- Skip in v1, add in Phase 2 of implementation

### 2.2 Compression Engine

**Algorithm Selection:**

| Algorithm | Ratio | Speed (MB/s) | Use Case |
|-----------|-------|--------------|----------|
| LZ4 | 2.0-2.5x | 750+ | Tier 1 (warm) - fast decompress |
| Zstd level 3 | 2.8-3.5x | 400+ | Tier 2 (cold) - good ratio |
| Skip | 1.0x | N/A | Already-compressed (entropy > 0.9) |

**Entropy detection** (skip already-compressed pages):
- Sample first 4KB
- If Shannon entropy > 0.9 → skip compression, store as-is
- Saves CPU on JPEG, MP4, encrypted data, etc.

**Implementation:**
```rust
// src/mm/compression/engine.rs

use lz4_flex::{compress_prepend_size, decompress_size_prepended};
use zstd::block::{compress, decompress};

pub fn compress_tier1(page: &[u8; PAGE_SIZE]) -> Vec<u8> {
    if estimate_entropy(page) > 0.9 {
        return page.to_vec(); // skip
    }
    compress_prepend_size(page)
}

pub fn compress_tier2(page: &[u8; PAGE_SIZE]) -> Vec<u8> {
    if estimate_entropy(page) > 0.9 {
        return page.to_vec();
    }
    zstd::block::compress(page, 3, true).unwrap()
}
```

### 2.3 Compressed Page Store

**Data structure:**
```rust
// src/mm/compression/store.rs

pub struct CompressedStore {
    // Per-CPU pool for lock-free access
    pools: Vec<PerCpuPool>,

    // Global lookup: PFN → CompressedEntry
    table: RwLock<HashMap<PhysFrameNum, CompressedEntry>>,

    // Slab allocator for compressed chunks
    slab: SlabAllocator,
}

pub struct CompressedEntry {
    pub pfn: PhysFrameNum,           // original page
    pub location: CompressedLocation, // where it lives now
    pub cap_id: CapabilityId,         // owning capability
    pub algorithm: CompressionAlgo,
    pub compressed_size: u32,
    pub original_size: u32,
    pub compressed_at: u64,
}

pub enum CompressedLocation {
    Ram { pool_idx: usize, offset: usize },
    Disk { block_id: u64 },
    Both { pool_idx: usize, block_id: u64 },
}
```

**SMP consideration:**
- Per-CPU pool for hot pages (lock-free)
- Global pool for cold pages (rwlock with sharding)
- Migration daemon runs on dedicated core

### 2.4 Page Fault Handler Integration

**The critical path - decompress on fault:**

```rust
// src/mm/compression/fault.rs

pub fn handle_page_fault(addr: VirtAddr) -> Result<(), FaultError> {
    let pte = current_pte(addr);

    if !pte.is_compressed() {
        return handle_normal_fault(addr); // not our path
    }

    // Fast path: in-place swap
    if let Some(frame) = try_acquire_free_frame_fast() {
        decompress_into(pte.compressed_ref(), frame);
        pte.replace_with_frame(frame);
        pte.clear_compressed_flag();
        return Ok(());
    }

    // Slow path: full eviction dance
    evict_cold_page();
    let frame = acquire_free_frame();
    decompress_into(pte.compressed_ref(), frame);
    pte.replace_with_frame(frame);
    pte.clear_compressed_flag();
    Ok(())
}
```

**Latency target:** < 5μs for T1 (LZ4 decompress), < 20μs for T2 (Zstd decompress)

### 2.5 eBPF Integration

**Hook points exposed to eBPF programs:**

```rust
// src/mm/compression/ebpf_hooks.rs

pub enum CompressionHook {
    OnClassify { pfn: PhysFrameNum, default_tier: Tier },
    OnCompress { pfn: PhysFrameNum, algo: CompressionAlgo },
    OnEvict { pfn: PhysFrameNum, cap_id: CapabilityId },
}

pub fn fire_hook(hook: CompressionHook) -> Decision {
    // Run attached eBPF program
    // Return: Allow, Deny, Modify(decision)
}
```

**Use case — game engine tells kernel:**
> "These 100MB of texture pages are mine, never compress them during gameplay"

```c
// eBPF program (C)
SEC("mm_compression/on_classify")
int game_texture_protect(struct classify_ctx *ctx) {
    if (ctx->pfn >= game_texture_start &&
        ctx->pfn < game_texture_end) {
        return KEEP_HOT; // never compress
    }
    return ALLOW_DEFAULT;
}
```

### 2.6 Capability Integration

**Per-capability memory budgets:**

```rust
// src/mm/compression/capability.rs

pub struct CapMemoryPolicy {
    pub cap_id: CapabilityId,
    pub max_compressed_bytes: u64,    // budget
    pub min_compression_ratio: f32,   // quality
    pub eviction_priority: u8,        // 0=keep, 255=evict first
    pub pin_tier: Option<Tier>,       // if Some, never migrate below
}

pub fn apply_cap_policy(cap_id: CapabilityId, decision: &mut EvictionDecision) {
    let policy = get_cap_policy(cap_id);
    decision.score += (255 - policy.eviction_priority) as i32;
    if let Some(tier) = policy.pin_tier {
        decision.min_tier = tier;
    }
}
```

**Instant Revocation integration:**

When a capability is revoked (existing ZiqaKernel feature):
```rust
// src/mm/compression/revocation.rs

pub fn on_capability_revoked(cap_id: CapabilityId) {
    // Find all compressed pages owned by this cap
    let entries = store.find_by_cap(cap_id);

    for entry in entries {
        match entry.location {
            CompressedLocation::Ram { .. } => {
                slab.free(entry.location);
            }
            CompressedLocation::Disk { block_id } => {
                disk.free(block_id);
            }
            _ => {}
        }
        table.remove(entry.pfn);
    }

    // No decompression needed - just drop the bytes
    // Revocation is now O(1) for compressed pages
}
```

**This is a feature no other kernel has:** Instant revocation includes compressed memory.

---

## 3. Implementation Phases

### Phase 1: Foundation (Weeks 1-4)

**Week 1-2: Skeleton & Build System**
- Create `src/mm/compression/` module tree
- Add to `src/mm/mod.rs`
- Cargo.toml updates (lz4_flex, zstd dependencies)
- Basic unit tests

**Week 3-4: Page Classifier + Compression Engine**
- LRU-based classifier with PTE Accessed bit
- LZ4 + Zstd compression functions
- Entropy detection
- Unit tests for round-trip correctness

**Deliverable:** Module compiles, basic compression works in isolation.

### Phase 2: Store & Page Fault (Weeks 5-8)

**Week 5-6: Compressed Page Store**
- Slab allocator for compressed chunks
- Per-CPU pools
- HashMap lookup table
- Lock-free fast paths

**Week 7-8: Page Fault Handler**
- Detect compressed PTEs
- Decompression path
- Frame acquisition
- Replace PTE in-place

**Deliverable:** Pages can be compressed and decompressed live.

### Phase 3: SMP & Locking (Weeks 9-10)

**Week 9-10: Multi-core Optimization**
- Per-CPU pools fully lock-free
- Migration daemon on dedicated core
- Lock contention benchmarks
- RCU-style updates for the lookup table

**Deliverable:** Scales linearly with core count.

### Phase 4: eBPF & Capability (Weeks 11-14)

**Week 11-12: eBPF Hooks**
- Define hook points
- Implement eBPF program loading
- Test with sample programs

**Week 13-14: Capability Integration**
- Per-capability policies
- Revocation integration
- Isolation tests

**Deliverable:** Full integration with ZiqaKernel subsystems.

### Phase 5: ML & Polish (Weeks 15-18)

**Week 15-16: ML-Enhanced Classification**
- Simple logistic regression model
- Feature extraction
- Online training loop

**Week 17-18: Optimization, Documentation, Paper Draft**
- Profile and optimize
- Write user documentation
- Draft research paper

**Deliverable:** Production-ready + paper draft.

---

## 4. Testing Plan

### 4.1 Correctness Tests

```rust
#[test]
fn round_trip_correctness() {
    let original = random_page();
    let compressed = compress_tier2(&original);
    let decompressed = decompress(&compressed);
    assert_eq!(original, decompressed);
}

#[test]
fn entropy_detection_skips() {
    let random_data = random_bytes(); // high entropy
    let compressed = compress_tier2(&random_data);
    assert_eq!(compressed.len(), random_data.len()); // skipped
}

#[test]
fn capability_isolation() {
    let cap_a = create_cap();
    let cap_b = create_cap();

    let page_a = alloc_page_for_cap(cap_a);
    let page_b = alloc_page_for_cap(cap_b);

    compress_page(page_a);

    // Verify: page_b cannot read page_a's compressed data
    assert!(!store.can_access(page_b, page_a));
}
```

### 4.2 Performance Tests

| Test | Target | How |
|------|--------|-----|
| Memory bandwidth | < 10% regression | Stream benchmark |
| Hot page access latency | < 5% increase | lmbench-style |
| Cold page decompression | < 20μs T2, < 5μs T1 | Microbenchmark |
| Compression ratio | 2.5-3.5x average | Real workload replay |
| CPU overhead | < 5% in steady state | perf + workloads |
| Lock contention | Scales to 16 cores | Synthetic multi-thread |

### 4.3 Real-World Workloads

1. **Browser stress test**: 50 tabs open, measure effective memory
2. **Game workload**: Run game, measure texture memory
3. **Compile test**: Build kernel, measure peak memory
4. **Docker test**: Run 5 containers, measure pressure
5. **AI inference test**: Load 7B model in INT4 on 4GB RAM

### 4.4 Stress Tests

- Memory pressure: fill RAM, watch eviction behavior
- Concurrent access: 16 threads accessing compressed pages
- Power loss: kill -9 during compression, verify no corruption
- OOM scenarios: what happens when even compressed store is full?

---

## 5. Risks & Mitigations

| Risk | Impact | Mitigation |
|------|--------|-----------|
| Compression CPU overhead | Slow system | Adaptive algorithm selection, skip already-compressed |
| Latency spikes on fault | Stuttering apps | Pre-decompress hot pages, dedicated decompress thread |
| Memory corruption from bugs | System crash | Formal verification of critical paths, extensive fuzzing |
| Compatibility (DMA, MMIO) | Crashes | Skip list, capability flag for non-compressible |
| Capability leak in compressed data | Security issue | Encrypted compressed storage, strict isolation |
| Lock contention on SMP | Poor multi-core scaling | Per-CPU pools, RCU for table updates |
| ML model adds overhead | Slow classification | Start with simple heuristics, ML optional |

---

## 6. Success Metrics

**Technical:**
- 2.5-3.0x effective memory on diverse workloads
- < 5% CPU overhead in steady state
- < 5% latency increase for hot page access
- < 20μs T2 decompression
- 100% functional compatibility (existing programs work)

**User-Visible:**
- 4GB laptop opens 30-tab Chrome + VS Code without swap thrashing
- Game with 8GB requirement runs on 4GB hardware
- Compile jobs don't crash on memory pressure
- Boot to interactive in < 1s (when paired with Primitive 4)

**Research:**
- 1 SOSP/OSDI/NSDI paper
- 3-5 follow-up workshop papers
- Open-source release with benchmarks

---

## 7. Code Structure (Rust)

```
src/mm/
├── mod.rs                       # existing - add pub mod compression
├── compression/
│   ├── mod.rs                   # public API
│   ├── tier.rs                  # Tier enum, transitions
│   ├── classifier.rs            # hot/cold classification
│   ├── engine.rs                # LZ4 + Zstd wrappers
│   ├── store.rs                 # CompressedPageStore
│   ├── fault.rs                 # page fault integration
│   ├── ebpf_hooks.rs            # eBPF integration
│   ├── capability.rs            # per-cap policies
│   ├── revocation.rs            # instant revocation
│   ├── daemon.rs                # background migration thread
│   ├── ml/
│   │   ├── mod.rs
│   │   ├── model.rs             # logistic regression
│   │   ├── features.rs          # feature extraction
│   │   └── train.rs             # online learning
│   └── tests/
│       ├── correctness.rs
│       ├── performance.rs
│       └── isolation.rs
```

**Estimated Lines of Code:** ~3,500-4,500 (kernel Rust)
**Estimated Test Code:** ~1,500-2,000

---

## 8. First Steps (Today)

If you want to start now, here's the order:

**Step 1: Add dependencies to Cargo.toml**
```toml
[dependencies]
lz4_flex = "0.11"
zstd = "0.13"
```

**Step 2: Create the skeleton**
```bash
mkdir -p src/mm/compression
touch src/mm/compression/{mod,tier,classifier,engine,store,fault,daemon}.rs
```

**Step 3: Add to mm/mod.rs**
```rust
pub mod compression;
```

**Step 4: Write first test (round-trip compression)**
```rust
// src/mm/compression/tests/correctness.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_lz4() {
        let page = [42u8; 4096];
        let compressed = engine::compress_tier1(&page);
        let decompressed = engine::decompress_tier1(&compressed).unwrap();
        assert_eq!(page.to_vec(), decompressed);
    }
}
```

**Step 5: Verify build**
```bash
cargo build --target x86_64-unknown-none
cargo test --lib compression::
```

**Step 6: Run benchmark**
```bash
cargo bench --bench compression_bench
```

After Step 6, you'll have a working compression engine to build on.

---

## 9. Timeline Summary

| Phase | Weeks | Output |
|-------|-------|--------|
| 1. Foundation | 1-4 | Compression engine works |
| 2. Store & Fault | 5-8 | Live compression/decompression |
| 3. SMP & Locking | 9-10 | Scales to 16+ cores |
| 4. eBPF & Capability | 11-14 | Full ZiqaKernel integration |
| 5. ML & Polish | 15-18 | Production + paper draft |

**Total: ~4-5 months for full implementation + research paper**

---

## 10. Why This Is Publishable + Rejuvenating

**Publishable angle:**
- First kernel with capability-aware aggressive compression
- First system to use ML-driven hot/cold classification in kernel
- First to integrate instant revocation with compressed memory

**Rejuvenation angle:**
- 4GB laptop from 2015 → runs 2026 software
- Old PCs in developing countries → become "weapons"
- E-waste reduction (extend hardware lifespan by 5+ years)

**This is your first primitive. Build it right, and the next 6 follow naturally.**
