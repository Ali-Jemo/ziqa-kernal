/// Per-frame reference counting and CoW/Shared flags.
///
/// Inspired by Redox's `PageInfo` in `src/memory/mod.rs`.
/// Every allocatable physical frame has one entry in a static array
/// (indexed by `frame_phys_addr / PAGE_SIZE`). The buddy allocator
/// still owns frame *allocation*; this module owns the *semantics*.
use core::sync::atomic::{AtomicUsize, Ordering};

use super::PAGE_SIZE;

// ── Bit layout of `refcount` field ──────────────────────────────────────────
//
//   bit 63  → RC_USED_NOT_FREE  (1 = used, 0 = free/untracked)
//   bit 62  → RC_SHARED_NOT_COW (1 = shared, 0 = CoW)  — only valid when used
//   bits 0..61 → actual reference count
//
const RC_USED_NOT_FREE: usize = 1 << (usize::BITS - 1);
const RC_SHARED_NOT_COW: usize = 1 << (usize::BITS - 2);
const RC_COUNT_MASK: usize = !(RC_USED_NOT_FREE | RC_SHARED_NOT_COW);

/// Mode in which a frame is shared among mappings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefKind {
    /// Copy-on-write: mapped read-only; on write fault, copy and remap.
    Cow,
    /// Shared: mapped writable in all owners.
    Shared,
}

/// A single per-frame info cell.
///
/// Zero-initialised by default (free, refcount 0).
#[derive(Debug)]
pub struct PageInfo {
    /// Packed: RC_USED_NOT_FREE | RC_SHARED_NOT_COW | count
    pub refcount: AtomicUsize,
}

impl Default for PageInfo {
    fn default() -> Self {
        Self { refcount: AtomicUsize::new(0) }
    }
}

impl PageInfo {
    /// Mark frame as "used" with an initial refcount of 1.
    pub fn init_used(&self, kind: RefKind) {
        let bits = RC_USED_NOT_FREE
            | if kind == RefKind::Shared { RC_SHARED_NOT_COW } else { 0 }
            | 1;
        self.refcount.store(bits, Ordering::Release);
    }

    /// Increment the refcount. Returns the new count, or `None` on overflow.
    pub fn add_ref(&self) -> Option<usize> {
        let prev = self.refcount.fetch_add(1, Ordering::AcqRel);
        let count = (prev & RC_COUNT_MASK) + 1;
        if count >= RC_COUNT_MASK {
            // overflow guard: undo and fail
            self.refcount.fetch_sub(1, Ordering::AcqRel);
            return None;
        }
        Some(count)
    }

    /// Decrement the refcount.  
    /// Returns `Some(remaining)` on success, `None` when the count was already 0.
    pub fn remove_ref(&self) -> Option<usize> {
        let prev = self.refcount.fetch_sub(1, Ordering::AcqRel);
        let count = prev & RC_COUNT_MASK;
        if count == 0 {
            self.refcount.fetch_add(1, Ordering::AcqRel); // undo
            return None;
        }
        Some(count - 1)
    }

    /// How many references currently exist.
    pub fn ref_count(&self) -> usize {
        self.refcount.load(Ordering::Acquire) & RC_COUNT_MASK
    }

    /// Whether the frame is tracked as "used" (vs free/untracked).
    pub fn is_used(&self) -> bool {
        self.refcount.load(Ordering::Acquire) & RC_USED_NOT_FREE != 0
    }

    /// Whether the frame is in shared (not CoW) mode.
    pub fn is_shared(&self) -> bool {
        self.refcount.load(Ordering::Acquire) & RC_SHARED_NOT_COW != 0
    }

    /// Upgrade a CoW frame to Shared (e.g. after `mprotect(PROT_WRITE)` on a
    /// shared-memory region).  Has no effect if already shared.
    pub fn upgrade_to_shared(&self) {
        self.refcount.fetch_or(RC_SHARED_NOT_COW, Ordering::AcqRel);
    }

    /// Reset to free state (refcount 0, no flags).
    pub fn mark_free(&self) {
        self.refcount.store(0, Ordering::Release);
    }
}

// ── Global PageInfo table ────────────────────────────────────────────────────

/// Maximum tracked physical memory: 4 GiB → 1 M frames.
/// Increase `MAX_TRACKED_FRAMES` if your target has more RAM.
pub const MAX_TRACKED_FRAMES: usize = 1 << 20; // 1 048 576

static PAGE_INFO_TABLE: [PageInfo; MAX_TRACKED_FRAMES] = {
    // const-init: all zero → free, refcount 0
    //
    // Rust does not yet support `[T::default(); N]` for non-Copy types in
    // const context, so we use a transmute of a zeroed array.
    //
    // SAFETY: `PageInfo` is a newtype around `AtomicUsize` which has the same
    // layout as `usize`; a zero-bit pattern is a valid `AtomicUsize(0)`.
    unsafe {
        core::mem::transmute::<[usize; MAX_TRACKED_FRAMES], [PageInfo; MAX_TRACKED_FRAMES]>(
            [0usize; MAX_TRACKED_FRAMES],
        )
    }
};

/// Look up the `PageInfo` for a physical frame address.
///
/// Returns `None` if the address is above `MAX_TRACKED_FRAMES * PAGE_SIZE`.
#[inline]
pub fn get_page_info(phys_addr: u64) -> Option<&'static PageInfo> {
    let idx = (phys_addr as usize) / PAGE_SIZE;
    PAGE_INFO_TABLE.get(idx)
}
