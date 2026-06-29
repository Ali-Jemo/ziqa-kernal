pub mod classifier;
pub mod daemon;
pub mod engine;
pub mod fault;
pub mod store;
pub mod tier;
pub mod tests;
pub mod bench;

use lazy_static::lazy_static;
use engine::AdaptiveCompressor;
use store::CompressedPageStore;

lazy_static! {
    pub static ref COMPRESSION_ENGINE: AdaptiveCompressor = AdaptiveCompressor::new();
    pub static ref PAGE_STORE: CompressedPageStore = CompressedPageStore::new();
}
