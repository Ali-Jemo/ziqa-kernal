use alloc::vec::Vec;
use lz4_flex::{compress_prepend_size, decompress_size_prepended};
use super::tier::Compressor;

pub struct Lz4Compressor;

impl Lz4Compressor {
    pub const fn new() -> Self { Self }
}

impl Compressor for Lz4Compressor {
    fn compress(&self, data: &[u8]) -> Option<Vec<u8>> {
        Some(compress_prepend_size(data))
    }
    
    fn decompress(&self, compressed: &[u8], original_size: usize) -> Option<Vec<u8>> {
        if compressed.len() < 4 { return None; }
        if original_size > 0 {
            let data_len = u32::from_le_bytes([compressed[0], compressed[1], compressed[2], compressed[3]]) as usize;
            if data_len != original_size { return None; }
        }
        decompress_size_prepended(compressed).ok()
    }
}

/// High-entropy threshold: pages above this are likely encrypted/compressed
/// and won't benefit from our compression — skip them.
const ENTROPY_SKIP_THRESHOLD: f64 = 7.5;

pub struct AdaptiveCompressor {
    lz4: Lz4Compressor,
}

impl AdaptiveCompressor {
    pub const fn new() -> Self {
        Self { lz4: Lz4Compressor::new() }
    }
    
    /// Calculate Shannon entropy of a byte slice (bits per byte, 0.0–8.0).
    /// Used by benchmarks and the compression daemon to decide whether
    /// a page is worth compressing.
    pub fn shannon_entropy(data: &[u8]) -> f64 {
        if data.is_empty() {
            return 0.0;
        }
        let mut counts = [0u64; 256];
        for &b in data {
            counts[b as usize] += 1;
        }
        let len = data.len() as f64;
        let mut entropy = 0.0f64;
        for &count in &counts {
            if count == 0 {
                continue;
            }
            let p = count as f64 / len;
            entropy -= p * libm::log2(p);
        }
        entropy
    }
    
    pub fn compress(&self, data: &[u8]) -> Option<Vec<u8>> {
        // Skip incompressible (high-entropy) pages
        if Self::shannon_entropy(data) > ENTROPY_SKIP_THRESHOLD {
            return None;
        }
        self.lz4.compress(data)
    }
    
    pub fn decompress(&self, compressed: &[u8], original_size: usize) -> Option<Vec<u8>> {
        self.lz4.decompress(compressed, original_size)
    }
}
