//! Synchronization engine

pub mod conflict;
pub mod engine;
pub mod progress;
pub mod transfer;

pub use conflict::{Conflict, ConflictManager, ConflictResolution, ConflictStrategy, is_conflict};
pub use engine::{FolderSync, PeerState, SyncConfig, SyncEngine, SyncStatus};
pub use progress::{SyncProgress, SyncState, ProgressTracker};
pub use transfer::{Priority, RateLimiter, TransferQueue, TransferRequest, TransferStats};
