/// Kernel tracepoint infrastructure for performance profiling
/// Provides low-overhead timestamped events for COW fork, page table, and TLB shootdown analysis
use core::sync::atomic::{AtomicUsize, Ordering};
use alloc::vec::Vec;
use spin::Mutex;

/// Unique tracepoint identifiers
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum TracepointId {
    // Page table allocation
    PgdAlloc = 0,
    PudAlloc = 1,
    PmdAlloc = 2,
    PteAlloc = 3,
    // COW fork path
    CowForkStart = 4,
    CowForkEnd = 5,
    CowMakeReadonlyStart = 6,
    CowMakeReadonlyEnd = 7,
    CowCloneStart = 8,
    CowCloneEnd = 9,
    // TLB shootdown
    TlbShootdownStart = 10,
    TlbShootdownEnd = 11,
    TlbShootdownSend = 12,
    TlbShootdownComplete = 13,
    // Page fault handling
    PageFaultCow = 14,
    PageFaultCowComplete = 15,
}

/// A single trace event
#[derive(Debug, Clone, Copy)]
pub struct TraceEvent {
    pub id: TracepointId,
    pub timestamp: u64,
    pub cpu_id: u32,
    pub data: u64, // Optional data (e.g., address, page count)
}

/// Ring buffer for trace events - uses Vec for each CPU
pub struct TraceBufferInner {
    buffer: [Vec<TraceEvent>; 32], // One Vec per CPU
    buffer_size: usize,
    initialized: AtomicUsize,
}

/// Static trace buffer wrapped in a Mutex for safe access
static TRACE_BUFFER: Mutex<TraceBufferInner> = Mutex::new(TraceBufferInner {
    buffer: [
        Vec::new(), Vec::new(), Vec::new(), Vec::new(),
        Vec::new(), Vec::new(), Vec::new(), Vec::new(),
        Vec::new(), Vec::new(), Vec::new(), Vec::new(),
        Vec::new(), Vec::new(), Vec::new(), Vec::new(),
        Vec::new(), Vec::new(), Vec::new(), Vec::new(),
        Vec::new(), Vec::new(), Vec::new(), Vec::new(),
        Vec::new(), Vec::new(), Vec::new(), Vec::new(),
        Vec::new(), Vec::new(), Vec::new(), Vec::new(),
    ],
    buffer_size: 65536, // 64K events per CPU
    initialized: AtomicUsize::new(0),
});

/// Fast cycle counter read
#[inline]
fn read_cycles() -> u64 {
    unsafe { core::arch::x86_64::_rdtsc() }
}

/// Initialize the trace buffer with memory for the specified number of CPUs
pub fn init_tracepoints(cpu_count: usize) {
    let mut inner = TRACE_BUFFER.lock();
    
    use x86_64::structures::paging::FrameAllocator;
    use x86_64::VirtAddr;

    let mut fa_guard = crate::memory::FRAME_ALLOCATOR.lock();
    let fa = fa_guard.as_mut().expect("FRAME_ALLOCATOR not initialized");

    for cpu in 0..cpu_count.min(32) {
        let frame = fa.allocate_frame().expect("OOM: trace buffer");
        let _virt = VirtAddr::new(crate::memory::paging::phys_offset().as_u64() + frame.start_address().as_u64());
        
        // Initialize the Vec with pre-allocated capacity
        let mut v = Vec::new();
        v.try_reserve(65536).ok(); // Try to reserve space
        
        // Copy the buffer pointer - we'll use raw writes
        inner.buffer[cpu] = v;
        // Actually extend with uninit space to avoid allocation overhead
        // But Vec doesn't support uninit, so we'll just append as needed
    }

    drop(fa_guard);
    inner.initialized.store(1, Ordering::Release);
}

/// Write a trace event
#[inline]
pub fn write_tracepoint(id: TracepointId, data: u64) {
    let mut inner = TRACE_BUFFER.lock();
    
    if inner.initialized.load(Ordering::Acquire) == 0 {
        return;
    }
    
    let cpu_id = crate::arch::x86_64::per_cpu::current_cpu().cpu_id as usize;
    if cpu_id >= 32 {
        return;
    }

    let event = TraceEvent {
        id,
        timestamp: read_cycles(),
        cpu_id: cpu_id as u32,
        data,
    };
    
    if inner.buffer[cpu_id].len() < inner.buffer_size {
        inner.buffer[cpu_id].push(event);
    }
}

/// Collect all trace events from all CPUs
pub fn collect_trace_data() -> Vec<TraceEvent> {
    let inner = TRACE_BUFFER.lock();
    let mut events = Vec::new();
    
    for cpu in 0..32 {
        events.extend(inner.buffer[cpu].iter().copied());
    }
    
    events
}

// Public helper functions for tracepoints
pub fn trace_pgd_alloc(frame: u64) { write_tracepoint(TracepointId::PgdAlloc, frame); }
pub fn trace_pud_alloc(frame: u64) { write_tracepoint(TracepointId::PudAlloc, frame); }
pub fn trace_pmd_alloc(frame: u64) { write_tracepoint(TracepointId::PmdAlloc, frame); }
pub fn trace_pte_alloc(frame: u64) { write_tracepoint(TracepointId::PteAlloc, frame); }

pub fn trace_cow_fork_start() { write_tracepoint(TracepointId::CowForkStart, read_cycles()); }
pub fn trace_cow_fork_end() { write_tracepoint(TracepointId::CowForkEnd, read_cycles()); }
pub fn trace_cow_make_readonly_start(pages: usize) { write_tracepoint(TracepointId::CowMakeReadonlyStart, pages as u64); }
pub fn trace_cow_make_readonly_end() { write_tracepoint(TracepointId::CowMakeReadonlyEnd, read_cycles()); }
pub fn trace_cow_clone_start() { write_tracepoint(TracepointId::CowCloneStart, read_cycles()); }
pub fn trace_cow_clone_end() { write_tracepoint(TracepointId::CowCloneEnd, read_cycles()); }

pub fn trace_tlb_shootdown_start(addr: u64, target_cpu_count: u32) { 
    write_tracepoint(TracepointId::TlbShootdownStart, addr | ((target_cpu_count as u64) << 32)); 
}
pub fn trace_tlb_shootdown_end() { write_tracepoint(TracepointId::TlbShootdownEnd, read_cycles()); }
pub fn trace_tlb_shootdown_send(cpu_id: u32) { write_tracepoint(TracepointId::TlbShootdownSend, cpu_id as u64); }
pub fn trace_tlb_shootdown_complete() { write_tracepoint(TracepointId::TlbShootdownComplete, read_cycles()); }

pub fn trace_page_fault_cow(addr: u64) { write_tracepoint(TracepointId::PageFaultCow, addr); }
pub fn trace_page_fault_cow_complete() { write_tracepoint(TracepointId::PageFaultCowComplete, read_cycles()); }