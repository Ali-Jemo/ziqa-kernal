use crate::memory::compression::engine::AdaptiveCompressor;
use crate::memory::compression::tier::CompressionTier;
use crate::memory::compression::PAGE_STORE;
use crate::memory::PAGE_SIZE;
use x86_64::VirtAddr;
use alloc::vec::Vec;
use crate::println;

pub fn run_tests() {
    println!("[TEST]   Running memory compression tests...");
    
    test_compression_roundtrip();
    test_entropy_detection();
    test_page_store();
    test_run_benchmarks();
}

fn test_compression_roundtrip() {
    let engine = AdaptiveCompressor::new();
    
    // Patterned data (highly compressible)
    let original_data: Vec<u8> = (0..PAGE_SIZE).map(|i| (i % 255) as u8).collect();
    
    let compressed = engine.compress(&original_data).expect("Compression failed for patterned data");
    assert!(compressed.len() < PAGE_SIZE);
    
    let decompressed = engine.decompress(&compressed, PAGE_SIZE).expect("Decompression failed");
    assert_eq!(original_data, decompressed);
    
    println!("[TEST]     PASS  compression roundtrip");
}

fn test_entropy_detection() {
    // Low entropy
    let zeros = [0u8; PAGE_SIZE];
    assert!(AdaptiveCompressor::shannon_entropy(&zeros) < 1.0);
    
    // High entropy (pseudo-random)
    let mut pseudo_random = Vec::with_capacity(PAGE_SIZE);
    let mut state: u32 = 1;
    for _ in 0..PAGE_SIZE {
        state = state.wrapping_mul(1664525).wrapping_add(1013904223);
        pseudo_random.push((state >> 24) as u8);
    }
    
    assert!(AdaptiveCompressor::shannon_entropy(&pseudo_random) > 7.0);
    
    println!("[TEST]     PASS  entropy detection");
}

fn test_page_store() {
    let vaddr = VirtAddr::new(0xDEAD_BEEF_000);
    let data = [0xAAu8; 100];
    
    // Store
    let success = PAGE_STORE.store(vaddr, &data, CompressionTier::Lz4);
    assert!(success);
    assert!(PAGE_STORE.is_compressed(vaddr));
    
    // Retrieve
    let retrieved = PAGE_STORE.retrieve(vaddr).expect("Failed to retrieve from store");
    assert_eq!(&retrieved, &data);
    
    // Release
    PAGE_STORE.release(vaddr);
    assert!(!PAGE_STORE.is_compressed(vaddr));
    
    println!("[TEST]     PASS  page store operations");
}

fn test_run_benchmarks() {
    crate::memory::compression::bench::run_benchmarks();
    println!("[TEST]     PASS  benchmarks executed");
}
