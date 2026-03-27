//! Content-addressed storage

pub mod block;
pub mod cache;
pub mod chunker;
pub mod store;

pub use block::{Block, BlockHash, compute_hash};
pub use cache::BlockCache;
pub use chunker::{Chunker, ChunkerConfig, RollingHash};
pub use store::BlockStore;
