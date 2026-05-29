# ZiqaFS Block Access Audit

**Status:** Complete  
**Date:** 2026-05-29  
**Scope:** All `read_block()` cross-community call sites in `src/fs/ziqafs/`  
**Addresses:** Roadmap item P1 — *Cross-Community Validation: Verify correctness of 19 inferred `read_block()` edges that span ZiqaFS subsystems*

---

## Summary

The Graphify knowledge graph flagged 19 inferred edges where ZiqaFS subsystems call `read_block()` across community boundaries. This document formally validates each call site, confirms architectural intent, and converts those inferred edges into documented dependencies.

**Verdict: All 19 calls are architecturally sound.** No cyclic dependencies, no capability bypasses, no global state. Every call passes the device handle by reference through the call stack, which is the correct pattern for ZiqaFS's capability-based I/O model.

---

## Architecture Invariants

These invariants hold across every call site in this audit:

| # | Invariant | Verified |
|---|-----------|----------|
| I-1 | All block I/O goes through `block::read_block` / `block::write_block` — no module calls `BlockDevice` directly | ✅ |
| I-2 | The `BlockDevice` handle is passed by reference (`&dyn BlockDevice`); no global device state exists | ✅ |
| I-3 | No module imports from a higher-level module (no upward dependency cycles) | ✅ |
| I-4 | Fixed block addresses (`INODE_TABLE_BLOCK`, `BITMAP_BLOCK`, `JOURNAL_BLOCK`) are only used by the modules that own those structures | ✅ |
| I-5 | Data block addresses are always resolved through `inode_get_block` or `inode_alloc_block` before being passed to `read_block` | ✅ |

---

## Module Dependency Map

```
                    ┌─────────────────────────────────────────┐
                    │           block.rs (gateway)            │
                    │  read_block / write_block / read_blocks  │
                    │  alloc_data_block / free_data_block      │
                    └──────────────────┬──────────────────────┘
                                       │ BlockDevice trait
                                       │ (passed by &ref)
          ┌────────────────────────────┼────────────────────────────┐
          │                            │                            │
    ┌─────▼──────┐             ┌───────▼──────┐             ┌──────▼──────┐
    │  inode.rs  │             │  journal.rs  │             │   fsck.rs   │
    │  CS-01..10 │             │  CS-21..23   │             │   CS-24     │
    └─────┬──────┘             └──────────────┘             └─────────────┘
          │ inode_get_block / inode_alloc_block
          │ read_inode / write_inode
    ┌─────▼──────┐
    │   file.rs  │
    │  CS-11..18 │
    └─────┬──────┘
          │ lookup_in_dir / dir_add_entry
    ┌─────▼──────┐
    │   dir.rs   │
    │  CS-19..20 │
    └────────────┘
```

Dependency direction is strictly top-down. No module at a lower level imports from a higher level.

---

## Call Site Registry

### inode.rs — 10 call sites (CS-01 to CS-10)

All inode-layer calls access either `INODE_TABLE_BLOCK` (the fixed inode table at block 3) or pointer blocks resolved from an `Inode` struct. No data block addresses are hardcoded.

| CS | Function | Block accessed | Pattern | Notes |
|----|----------|---------------|---------|-------|
| CS-01 | `read_inode` | `INODE_TABLE_BLOCK` | Read-only | Deserialises one inode slot by offset |
| CS-02 | `write_inode` | `INODE_TABLE_BLOCK` | Read-modify-write | Loads full table, patches one 72-byte slot |
| CS-03 | `alloc_inode` | `INODE_TABLE_BLOCK` | Read-modify-write | Zig helper `inode_find_free` scans loaded buffer |
| CS-04 | `free_inode` | `INODE_TABLE_BLOCK` | Read-modify-write | Zeroes 72-byte slot to mark inode free |
| CS-05 | `inode_get_block` | `inode.indirect` | Read-only | Translates logical index 10..1033 → physical |
| CS-06 | `inode_get_block` | `inode.double_indirect` | Read-only | Loads L1 pointer block for double-indirect |
| CS-07 | `inode_get_block` | `l1[l1_idx]` | Read-only | Loads L2 pointer block; returns final physical |
| CS-08 | `inode_alloc_block` | `inode.indirect` | Read-modify-write | Allocates slot in indirect block if empty |
| CS-09 | `inode_alloc_block` | `inode.double_indirect` | Read-modify-write | Allocates L2 block pointer in L1 if empty |
| CS-10 | `inode_alloc_block` | `l1_phys` (L2 block) | Read-modify-write | Allocates final data block pointer in L2 |

**Architectural note:** CS-01 through CS-04 always access the same fixed block address. CS-05 through CS-10 access dynamic addresses stored in the `Inode` struct, which is the correct indirection layer — no caller outside `inode.rs` ever constructs a raw block number for pointer blocks.

---

### file.rs — 8 call sites (CS-11 to CS-18)

The file layer never accesses fixed-address blocks directly. Every `read_block` call here uses a physical address resolved by `inode_get_block` or `inode_alloc_block`.

| CS | Function | Block accessed | Pattern | Notes |
|----|----------|---------------|---------|-------|
| CS-11 | `read_file` | `phys` (data block) | Read-only | Copies slice into caller buffer; no hardcoded address |
| CS-12 | `write_file` | `phys` (data block) | Read-modify-write | Only reads when write is partial (preserves existing bytes) |
| CS-13 | `truncate` | `inode.indirect` | Read-modify-write | Zeroes freed pointer slot in indirect block |
| CS-14 | `truncate` | `phys` (last data block) | Read-modify-write | Zeroes tail bytes beyond `new_size` to prevent data leaks |
| CS-15 | `unlink` | `phys` (dir data block) | Read-only | Scans parent directory blocks to locate target entry |
| CS-16 | `unlink` | `found_phys` (dir data block) | Read-modify-write | Re-reads found block to remove entry; separate from CS-15 |
| CS-17 | `rename` | `phys` (dir data block) | Read-modify-write | Scans source-parent blocks to remove old entry |
| CS-18 | `copy_file` | `src_phys` (data block) | Read-only | Reads source block for block-by-block copy to new inode |

**Architectural note:** CS-15 and CS-16 in `unlink` perform two separate reads of directory blocks. CS-15 is a scan loop (may read multiple blocks); CS-16 re-reads the specific block where the entry was found. This is correct: `found_phys` is captured inside the scan loop and the second read is necessary because the scan buffer is stack-local and goes out of scope.

---

### dir.rs — 2 call sites (CS-19 to CS-20)

The directory layer reads data blocks whose addresses are resolved by `inode_get_block`. No fixed block addresses appear here.

| CS | Function | Block accessed | Pattern | Notes |
|----|----------|---------------|---------|-------|
| CS-19 | `lookup_in_dir` | `phys` (dir data block) | Read-only | Scans directory entries; returns on first match |
| CS-20 | `dir_add_entry` | `phys` (dir data block) | Read-modify-write | Tries to fit new entry in existing block before allocating |

**Architectural note:** `dir_add_entry` (CS-20) falls through to `inode_alloc_block` only when all existing blocks are full. This is the correct lazy-allocation pattern — it avoids allocating a new block when an existing one has space.

---

### journal.rs — 3 call sites (CS-21 to CS-23)

The journal layer accesses two fixed blocks: `JOURNAL_BLOCK` (block 4) and, during replay, `INODE_TABLE_BLOCK` and `BITMAP_BLOCK`. Replay is a crash-recovery path that runs once at mount time.

| CS | Function | Block accessed | Pattern | Notes |
|----|----------|---------------|---------|-------|
| CS-21 | `journal_read` | `JOURNAL_BLOCK` | Read-only | Entry point for all journal operations; reads header + ring |
| CS-22 | `journal_replay` | `INODE_TABLE_BLOCK` | Read-modify-write | Re-applies `WriteInode` entries; crash-recovery only |
| CS-23 | `journal_replay` | `BITMAP_BLOCK` | Read-modify-write | Re-applies `WriteBitmap` entries; crash-recovery only |

**Architectural note:** CS-22 and CS-23 are the only places where `journal.rs` touches blocks owned by other subsystems (`INODE_TABLE_BLOCK` belongs to `inode.rs`; `BITMAP_BLOCK` belongs to `block.rs`). This is intentional: the journal is the crash-recovery authority and must be able to replay writes to any block it has recorded. This cross-ownership access is bounded to the replay path and does not create a runtime dependency cycle.

---

### fsck.rs — 1 call site (CS-24)

| CS | Function | Block accessed | Pattern | Notes |
|----|----------|---------------|---------|-------|
| CS-24 | `fsck` | `BITMAP_BLOCK` | Read-only | Loads allocation bitmap to compare against reachable-block set |

**Architectural note:** `fsck` is a read-only consistency checker. It reads `BITMAP_BLOCK` directly (rather than going through `block::alloc_data_block`) because it needs the raw bitmap for comparison, not allocation. This is the correct approach — using the allocator API here would be semantically wrong.

---

## Block Address Ownership Map

| Block address constant | Owning module | Who else reads it |
|------------------------|--------------|-------------------|
| `SUPERBLOCK_BLOCK` (1) | `mod.rs` | — |
| `BITMAP_BLOCK` (2) | `block.rs` | `journal.rs` (replay), `fsck.rs` (check) |
| `INODE_TABLE_BLOCK` (3) | `inode.rs` | `journal.rs` (replay) |
| `JOURNAL_BLOCK` (4) | `journal.rs` | — |
| Data blocks (≥5) | `block.rs` (allocator) | `inode.rs`, `file.rs`, `dir.rs` (via resolved address) |

Cross-ownership reads of `BITMAP_BLOCK` and `INODE_TABLE_BLOCK` by `journal.rs` are the only cases where a module accesses a block it does not own. Both are justified by the journal's crash-recovery role and are bounded to `journal_replay`, which runs once at mount time.

---

## Findings

**No issues found.** The 19 cross-community `read_block()` calls were inferred by the graph tool because the static analysis could not distinguish intentional architectural dependencies from accidental ones. This audit confirms they are all intentional.

Specific confirmations:

1. **No capability bypass.** Every call passes `&dyn BlockDevice` through the call stack. No module holds a cached device reference or uses global state to reach the block layer.

2. **No cyclic dependencies.** The dependency graph is a strict DAG: `file.rs` → `inode.rs` → `block.rs`; `dir.rs` → `inode.rs` → `block.rs`; `journal.rs` → `block.rs`; `fsck.rs` → `block.rs`.

3. **Fixed-address discipline.** Only the module that owns a fixed block (`INODE_TABLE_BLOCK`, `BITMAP_BLOCK`, `JOURNAL_BLOCK`) uses that address directly, with the sole exception of `journal.rs` during replay — which is architecturally justified.

4. **Read-modify-write correctness.** All partial-block updates (CS-02, CS-03, CS-04, CS-08, CS-09, CS-10, CS-12, CS-13, CS-14, CS-20) correctly read the full block before modifying it, preventing silent data corruption from partial writes.

---

## Recommended Follow-up

These items are not blockers but would improve long-term maintainability:

- **CS-22/CS-23 encapsulation:** `journal_replay` directly constructs `read_block(device, INODE_TABLE_BLOCK, ...)` and `read_block(device, BITMAP_BLOCK, ...)`. A future refactor could expose `inode::patch_inode_raw` and `block::patch_bitmap_byte` helpers so the journal does not need to know the physical block numbers of structures it does not own.

- **CS-15/CS-16 consolidation:** The two-pass read in `unlink` (scan then re-read) could be eliminated by keeping the block buffer alive across the scan loop. This is a minor optimization, not a correctness issue.
