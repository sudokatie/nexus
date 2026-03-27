//! Content-addressed storage

pub mod block;
pub mod chunker;

pub use block::{Block, BlockHash, compute_hash};
pub use chunker::{Chunker, ChunkerConfig, RollingHash};
