pub mod tier;
pub mod engine;
pub mod store;

use lazy_static::lazy_static;
use engine::AdaptiveCompressor;
use store::CompressedPageStore;

lazy_static! {
    pub static ref COMPRESSION_ENGINE: AdaptiveCompressor = AdaptiveCompressor::new();
    pub static ref PAGE_STORE: CompressedPageStore = CompressedPageStore::new();
}
