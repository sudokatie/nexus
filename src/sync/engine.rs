//! Sync engine - orchestrates file synchronization

use super::conflict::{Conflict, ConflictManager, ConflictStrategy, is_conflict};
use super::progress::{SyncProgress, SyncState};
use super::transfer::{TransferQueue, TransferRequest, TransferStats, Priority, RateLimiter};
use crate::crypto::DeviceId;
use crate::index::{diff_indexes, ChangeType, FileEntry, FolderIndex, IndexDiff};
use crate::storage::BlockHash;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Sync engine configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncConfig {
    /// Maximum concurrent block transfers
    pub max_concurrent: usize,
    /// Block request timeout
    pub request_timeout: Duration,
    /// Maximum retries per block
    pub max_retries: u32,
    /// Rate limit (bytes/sec, 0 = unlimited)
    pub rate_limit: u64,
    /// Conflict resolution strategy
    pub conflict_strategy: ConflictStrategy,
    /// Rescan interval
    pub rescan_interval: Duration,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            max_concurrent: 4,
            request_timeout: Duration::from_secs(30),
            max_retries: 3,
            rate_limit: 0,
            conflict_strategy: ConflictStrategy::NewestWins,
            rescan_interval: Duration::from_secs(60),
        }
    }
}

/// Peer sync state
#[derive(Debug, Clone)]
pub struct PeerState {
    /// Peer device ID
    pub device_id: DeviceId,
    /// Last received index sequence
    pub remote_sequence: u64,
    /// Our sequence when we last synced
    pub local_sequence: u64,
    /// Pending changes from this peer
    pub pending_changes: usize,
    /// Is peer currently connected
    pub connected: bool,
}

impl PeerState {
    /// Create new peer state
    pub fn new(device_id: DeviceId) -> Self {
        Self {
            device_id,
            remote_sequence: 0,
            local_sequence: 0,
            pending_changes: 0,
            connected: false,
        }
    }
}

/// Folder sync state
#[derive(Debug)]
pub struct FolderSync {
    /// Folder ID
    pub folder_id: String,
    /// Local path
    pub path: PathBuf,
    /// Local index
    pub index: FolderIndex,
    /// Peer states
    pub peers: HashMap<DeviceId, PeerState>,
    /// Transfer queue
    pub queue: TransferQueue,
    /// Transfer stats
    pub stats: TransferStats,
    /// Sync progress
    pub progress: SyncProgress,
    /// Pending conflicts
    pub conflicts: ConflictManager,
}

impl FolderSync {
    /// Create new folder sync state
    pub fn new(folder_id: impl Into<String>, path: impl Into<PathBuf>, device_name: &str) -> Self {
        let folder_id = folder_id.into();
        Self {
            index: FolderIndex::with_id(&folder_id),
            folder_id: folder_id.clone(),
            path: path.into(),
            peers: HashMap::new(),
            queue: TransferQueue::new(4),
            stats: TransferStats::new(),
            progress: SyncProgress::new(folder_id),
            conflicts: ConflictManager::new(device_name),
        }
    }
    
    /// Add a peer
    pub fn add_peer(&mut self, device_id: DeviceId) {
        self.peers.entry(device_id.clone())
            .or_insert_with(|| PeerState::new(device_id));
    }
    
    /// Process incoming index from peer
    pub fn process_remote_index(
        &mut self,
        peer_id: &DeviceId,
        remote_index: &FolderIndex,
        config: &SyncConfig,
    ) -> IndexDiff {
        let diff = diff_indexes(&self.index, remote_index);
        
        // Update peer state
        if let Some(peer) = self.peers.get_mut(peer_id) {
            peer.remote_sequence = remote_index.sequence();
            peer.pending_changes = diff.len();
        }
        
        // Check for conflicts
        for change in diff.changes() {
            if change.change_type == ChangeType::Modified {
                if let (Some(local), Some(remote)) = (
                    self.index.get(&change.path),
                    change.entry.as_ref()
                ) {
                    if is_conflict(local, remote) {
                        self.conflicts.add(Conflict::new(
                            &change.path,
                            local.clone(),
                            remote.clone(),
                        ));
                    }
                }
            }
        }
        
        // Resolve conflicts
        if self.conflicts.has_conflicts() {
            self.conflicts.set_strategy(config.conflict_strategy);
            self.conflicts.resolve_all();
        }
        
        // Queue block requests
        for block in diff.all_needed_blocks() {
            self.queue.enqueue(TransferRequest::new(block));
        }
        
        // Update progress
        self.progress.set_syncing(
            diff.len() as u64,
            diff.total_bytes(),
        );
        
        diff
    }
    
    /// Apply a received block
    pub fn apply_block(
        &mut self,
        hash: &BlockHash,
        _data: &[u8],
    ) -> bool {
        // Mark transfer complete
        if self.queue.complete(hash).is_some() {
            // Block would be stored here
            // For now, assume block is already stored
            true
        } else {
            false
        }
    }
    
    /// Check if sync is complete
    pub fn is_complete(&self) -> bool {
        self.queue.is_empty() && !self.conflicts.has_conflicts()
    }
    
    /// Get next block to request
    pub fn next_request(&mut self) -> Option<TransferRequest> {
        self.queue.next()
    }
}

/// The main sync engine
pub struct SyncEngine {
    /// Our device ID
    device_id: DeviceId,
    /// Our device name
    device_name: String,
    /// Configuration
    config: SyncConfig,
    /// Folders being synced
    folders: HashMap<String, FolderSync>,
    /// Rate limiter
    rate_limiter: Option<RateLimiter>,
    /// Running state
    running: bool,
}

impl SyncEngine {
    /// Create a new sync engine
    pub fn new(device_id: DeviceId, device_name: impl Into<String>) -> Self {
        Self {
            device_id,
            device_name: device_name.into(),
            config: SyncConfig::default(),
            folders: HashMap::new(),
            rate_limiter: None,
            running: false,
        }
    }
    
    /// Create with config
    pub fn with_config(device_id: DeviceId, device_name: impl Into<String>, config: SyncConfig) -> Self {
        let rate_limiter = if config.rate_limit > 0 {
            Some(RateLimiter::new(config.rate_limit))
        } else {
            None
        };
        
        Self {
            device_id,
            device_name: device_name.into(),
            config,
            folders: HashMap::new(),
            rate_limiter,
            running: false,
        }
    }
    
    /// Add a folder to sync
    pub fn add_folder(&mut self, folder_id: impl Into<String>, path: impl Into<PathBuf>) {
        let folder_id = folder_id.into();
        let folder_sync = FolderSync::new(&folder_id, path, &self.device_name);
        self.folders.insert(folder_id, folder_sync);
    }
    
    /// Add a peer for a folder
    pub fn add_peer(&mut self, folder_id: &str, peer_id: DeviceId) {
        if let Some(folder) = self.folders.get_mut(folder_id) {
            folder.add_peer(peer_id);
        }
    }
    
    /// Get folder sync state
    pub fn get_folder(&self, folder_id: &str) -> Option<&FolderSync> {
        self.folders.get(folder_id)
    }
    
    /// Get folder sync state mutably
    pub fn get_folder_mut(&mut self, folder_id: &str) -> Option<&mut FolderSync> {
        self.folders.get_mut(folder_id)
    }
    
    /// Process an incoming index update
    pub fn process_index(
        &mut self,
        folder_id: &str,
        peer_id: &DeviceId,
        remote_index: &FolderIndex,
    ) -> Option<IndexDiff> {
        let config = self.config.clone();
        self.folders.get_mut(folder_id)
            .map(|f| f.process_remote_index(peer_id, remote_index, &config))
    }
    
    /// Start the sync engine
    pub fn start(&mut self) {
        self.running = true;
        for folder in self.folders.values_mut() {
            folder.progress.start();
        }
    }
    
    /// Stop the sync engine
    pub fn stop(&mut self) {
        self.running = false;
        for folder in self.folders.values_mut() {
            folder.progress.pause();
        }
    }
    
    /// Check if running
    pub fn is_running(&self) -> bool {
        self.running
    }
    
    /// Get all folder IDs
    pub fn folder_ids(&self) -> Vec<String> {
        self.folders.keys().cloned().collect()
    }
    
    /// Get overall sync status
    pub fn status(&self) -> SyncStatus {
        let folders: Vec<_> = self.folders.values()
            .map(|f| (f.folder_id.clone(), f.progress.clone()))
            .collect();
        
        let all_complete = self.folders.values().all(|f| f.is_complete());
        let any_syncing = self.folders.values().any(|f| f.progress.state == SyncState::Syncing);
        
        SyncStatus {
            running: self.running,
            folders,
            state: if all_complete {
                SyncState::Complete
            } else if any_syncing {
                SyncState::Syncing
            } else {
                SyncState::Idle
            },
        }
    }
    
    /// Get configuration
    pub fn config(&self) -> &SyncConfig {
        &self.config
    }
    
    /// Update configuration
    pub fn set_config(&mut self, config: SyncConfig) {
        if config.rate_limit > 0 {
            self.rate_limiter = Some(RateLimiter::new(config.rate_limit));
        } else {
            self.rate_limiter = None;
        }
        self.config = config;
    }
}

/// Overall sync status
#[derive(Debug, Clone)]
pub struct SyncStatus {
    /// Is engine running
    pub running: bool,
    /// Per-folder progress
    pub folders: Vec<(String, SyncProgress)>,
    /// Overall state
    pub state: SyncState,
}

impl SyncStatus {
    /// Check if all folders are synced
    pub fn all_synced(&self) -> bool {
        self.state == SyncState::Complete
    }
    
    /// Get total bytes synced across all folders
    pub fn total_bytes_done(&self) -> u64 {
        self.folders.iter().map(|(_, p)| p.bytes_done).sum()
    }
    
    /// Get total bytes to sync across all folders
    pub fn total_bytes_total(&self) -> u64 {
        self.folders.iter().map(|(_, p)| p.bytes_total).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::compute_hash;
    
    fn test_device_id() -> DeviceId {
        DeviceId::from_bytes([3u8; 32])
    }
    
    fn make_entry(path: &str, data: &[u8]) -> FileEntry {
        let blocks = vec![compute_hash(data)];
        FileEntry::new(path, data.len() as u64, 1700000000, 0o644, blocks)
    }
    
    #[test]
    fn test_sync_config_default() {
        let config = SyncConfig::default();
        assert_eq!(config.max_concurrent, 4);
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.conflict_strategy, ConflictStrategy::NewestWins);
    }
    
    #[test]
    fn test_folder_sync_new() {
        let sync = FolderSync::new("test-folder", "/path/to/folder", "my-device");
        
        assert_eq!(sync.folder_id, "test-folder");
        assert!(sync.peers.is_empty());
        assert!(sync.is_complete());
    }
    
    #[test]
    fn test_folder_sync_add_peer() {
        let mut sync = FolderSync::new("test", "/test", "device");
        let peer_id = test_device_id();
        
        sync.add_peer(peer_id.clone());
        
        assert!(sync.peers.contains_key(&peer_id));
        let peer = sync.peers.get(&peer_id).unwrap();
        assert_eq!(peer.remote_sequence, 0);
    }
    
    #[test]
    fn test_sync_engine_new() {
        let engine = SyncEngine::new(test_device_id(), "my-laptop");
        
        assert!(!engine.is_running());
        assert!(engine.folder_ids().is_empty());
    }
    
    #[test]
    fn test_sync_engine_add_folder() {
        let mut engine = SyncEngine::new(test_device_id(), "device");
        
        engine.add_folder("default", "/home/user/Sync");
        engine.add_peer("default", DeviceId::from_bytes([4u8; 32]));
        
        let folder = engine.get_folder("default").unwrap();
        assert_eq!(folder.folder_id, "default");
        assert_eq!(folder.peers.len(), 1);
    }
    
    #[test]
    fn test_sync_engine_start_stop() {
        let mut engine = SyncEngine::new(test_device_id(), "device");
        engine.add_folder("test", "/test");
        
        assert!(!engine.is_running());
        
        engine.start();
        assert!(engine.is_running());
        
        engine.stop();
        assert!(!engine.is_running());
    }
    
    #[test]
    fn test_sync_engine_process_index() {
        let mut engine = SyncEngine::new(test_device_id(), "device");
        engine.add_folder("test", "/test");
        
        let peer_id = DeviceId::from_bytes([5u8; 32]);
        engine.add_peer("test", peer_id.clone());
        
        // Remote has a file we don't have
        let mut remote_index = FolderIndex::with_id("test");
        remote_index.put(make_entry("new.txt", b"content"));
        
        let diff = engine.process_index("test", &peer_id, &remote_index).unwrap();
        
        assert_eq!(diff.count_added(), 1);
    }
    
    #[test]
    fn test_sync_status() {
        let mut engine = SyncEngine::new(test_device_id(), "device");
        engine.add_folder("folder1", "/f1");
        engine.add_folder("folder2", "/f2");
        
        let status = engine.status();
        
        assert!(!status.running);
        assert_eq!(status.folders.len(), 2);
        assert_eq!(status.state, SyncState::Complete); // No pending changes
    }
}
