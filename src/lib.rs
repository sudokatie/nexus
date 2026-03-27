//! Nexus - Peer-to-peer file synchronization
//!
//! A decentralized file sync tool with content-addressed storage.
//!
//! # Features
//!
//! - Content-addressed storage with deduplication
//! - Block-level chunking for efficient delta sync
//! - Encrypted transport (TLS 1.3)
//! - NAT traversal (STUN/TURN)
//! - Local and global device discovery
//! - Conflict resolution
//!
//! # Quick Start
//!
//! ```ignore
//! // Initialize device
//! nexus init
//!
//! // Add folder to sync
//! nexus add ~/Documents
//!
//! // Add peer device
//! nexus device add XXXXX-XXXXX-XXXXX
//!
//! // Start syncing
//! nexus serve
//! ```

pub mod cli;
pub mod config;
pub mod crypto;
pub mod discovery;
pub mod index;
pub mod network;
pub mod storage;
pub mod sync;

/// Error types
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Storage error: {0}")]
    Storage(String),
    
    #[error("Index error: {0}")]
    Index(String),
    
    #[error("Crypto error: {0}")]
    Crypto(String),
    
    #[error("Network error: {0}")]
    Network(String),
    
    #[error("Config error: {0}")]
    Config(String),
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Sync error: {0}")]
    Sync(String),
}

/// Result type alias
pub type Result<T> = std::result::Result<T, Error>;

/// Prelude with common types
pub mod prelude {
    pub use crate::storage::{Block, BlockHash};
    pub use crate::index::FileEntry;
    pub use crate::crypto::DeviceId;
    pub use crate::config::Config;
    pub use crate::{Error, Result};
}
