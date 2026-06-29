use crate::memory::compression::engine::AdaptiveCompressor;
use crate::memory::PAGE_SIZE;
use crate::println;

pub fn run_benchmarks() {
    println!("\n[BENCH] Starting Memory Compression Benchmarks...");
    println!("[BENCH] Target: 2.5x - 3.5x average ratio on compressible workloads\n");

    let engine = AdaptiveCompressor::new();

    // 1. Zero Page (Most common in idle systems)
    let zero_page = [0u8; PAGE_SIZE];
    bench_data(&engine, "Zero Page", &zero_page);

    // 2. Patterned Page (e.g., heap metadata, repetitive structures)
    let mut patterned_page = [0u8; PAGE_SIZE];
    for i in 0..PAGE_SIZE {
        patterned_page[i] = (i % 16) as u8;
    }
    bench_data(&engine, "Patterned Page (16-byte repeat)", &patterned_page);

    // 3. Text/Code (Simulating application memory)
    let text_page = simulate_text_page();
    bench_data(&engine, "Simulated Text/Code Page", &text_page);

    // 4. Mixed Data (Simulating a real heap page)
    let mixed_page = simulate_mixed_page();
    bench_data(&engine, "Simulated Mixed Heap Page", &mixed_page);

    // 5. High Entropy (Encrypted/Compressed - Should be skipped)
    let mut random_page = [0u8; PAGE_SIZE];
    let mut state: u32 = 0x12345678;
    for i in 0..PAGE_SIZE {
        state = state.wrapping_mul(1664525).wrapping_add(1013904223);
        random_page[i] = (state >> 24) as u8;
    }
    bench_data(&engine, "Random Data (Entropy Test)", &random_page);

    println!("[BENCH] Benchmarks Completed.\n");
}

fn bench_data(engine: &AdaptiveCompressor, name: &str, data: &[u8]) {
    let entropy = AdaptiveCompressor::shannon_entropy(data);
    
    let result = engine.compress(data);
    
    match result {
        Some(compressed) => {
            let ratio = PAGE_SIZE as f32 / compressed.len() as f32;
            let saved = PAGE_SIZE - compressed.len();
            println!(
                "[BENCH] {:<30} | Entropy: {:.2} | Ratio: {:.2}x | Saved: {:4} bytes",
                name, entropy, ratio, saved
            );
        }
        None => {
            println!(
                "[BENCH] {:<30} | Entropy: {:.2} | SKIPPED (Incompressible)",
                name, entropy
            );
        }
    }
}

fn simulate_text_page() -> [u8; PAGE_SIZE] {
    let mut page = [0u8; PAGE_SIZE];
    let code = b"fn main() { println!(\"Hello, ZiqaKernel!\"); let x = 5 + 10; for i in 0..x { do_something(i); } } ";
    for i in 0..PAGE_SIZE {
        page[i] = code[i % code.len()];
    }
    page
}

fn simulate_mixed_page() -> [u8; PAGE_SIZE] {
    let mut page = [0u8; PAGE_SIZE];
    // First 1KB is pointers/metadata (repetitive)
    for i in 0..1024 {
        page[i] = if i % 8 == 0 { 0x40 } else { 0x00 };
    }
    // Next 1KB is text
    let text = b"This is some heap data containing strings and configuration flags.";
    for i in 1024..2048 {
        page[i] = text[i % text.len()];
    }
    // Rest is zeroes
    page
}
