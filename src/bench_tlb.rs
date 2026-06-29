use crate::memory::VirtAddr;
use core::arch::x86_64::_rdtsc;

pub fn bench_tlb() {
    crate::println!("[BENCH] Starting cross-CPU TLB shootdown latency test...");
    
    let addr = VirtAddr::new(0xDEAD_BEEF_000);
    
    let start = unsafe { _rdtsc() };
    crate::memory::paging::smp_tlb_flush(addr);
    let end = unsafe { _rdtsc() };
    
    crate::println!("[BENCH]   TLB shootdown completed in {} cycles", end - start);
}
