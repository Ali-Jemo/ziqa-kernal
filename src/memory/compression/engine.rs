use alloc::vec::Vec;
use lz4_flex::{compress_prepend_size, decompress_size_prepended};
use super::tier::{CompressionTier, Compressor};

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
        let data_len = u32::from_le_bytes([compressed[0], compressed[1], compressed[2], compressed[3]]) as usize;
        if data_len != original_size { return None; }
        decompress_size_prepended(compressed).ok()
    }
}

pub struct AdaptiveCompressor {
    lz4: Lz4Compressor,
}

impl AdaptiveCompressor {
    pub const fn new() -> Self {
        Self { lz4: Lz4Compressor::new() }
    }
    
    pub fn compress(&self, data: &[u8]) -> Option<Vec<u8>> {
        self.lz4.compress(data)
    }
    
    pub fn decompress(&self, compressed: &[u8], original_size: usize) -> Option<Vec<u8>> {
        self.lz4.decompress(compressed, original_size)
    }
}
