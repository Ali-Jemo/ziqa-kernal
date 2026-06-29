use alloc::vec::Vec;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionTier {
    Uncompressed,
    Lz4,
}

pub trait Compressor {
    fn compress(&self, data: &[u8]) -> Option<Vec<u8>>;
    fn decompress(&self, compressed: &[u8], original_size: usize) -> Option<Vec<u8>>;
}
