//! Index diffing for sync operations

use super::entry::FileEntry;
use super::folder::{FolderIndex, Sequence};
use crate::storage::BlockHash;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Type of change detected
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChangeType {
    /// File was added
    Added,
    /// File was modified
    Modified,
    /// File was deleted
    Deleted,
}

/// A single file change
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChange {
    /// Path of the changed file
    pub path: PathBuf,
    /// Type of change
    pub change_type: ChangeType,
    /// New file entry (for Added/Modified)
    pub entry: Option<FileEntry>,
    /// Blocks needed (for Modified - only changed blocks)
    pub needed_blocks: Vec<BlockHash>,
}

impl FileChange {
    /// Create an Add change
    pub fn added(entry: FileEntry) -> Self {
        let needed_blocks = entry.blocks().to_vec();
        Self {
            path: entry.path().to_path_buf(),
            change_type: ChangeType::Added,
            entry: Some(entry),
            needed_blocks,
        }
    }
    
    /// Create a Modified change with delta blocks
    pub fn modified(entry: FileEntry, needed_blocks: Vec<BlockHash>) -> Self {
        Self {
            path: entry.path().to_path_buf(),
            change_type: ChangeType::Modified,
            entry: Some(entry),
            needed_blocks,
        }
    }
    
    /// Create a Delete change
    pub fn deleted(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            change_type: ChangeType::Deleted,
            entry: None,
            needed_blocks: Vec::new(),
        }
    }
    
    /// Check if this is a delete
    pub fn is_deleted(&self) -> bool {
        self.change_type == ChangeType::Deleted
    }
    
    /// Get total bytes needed for this change
    pub fn bytes_needed(&self) -> u64 {
        self.entry.as_ref().map(|e| e.size()).unwrap_or(0)
    }
}

/// Result of comparing two indexes
#[derive(Debug, Clone, Default)]
pub struct IndexDiff {
    /// All changes
    changes: Vec<FileChange>,
    /// Starting sequence (local)
    local_sequence: Sequence,
    /// Ending sequence (remote)
    remote_sequence: Sequence,
}

impl IndexDiff {
    /// Create an empty diff
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Create with sequences
    pub fn with_sequences(local_seq: Sequence, remote_seq: Sequence) -> Self {
        Self {
            changes: Vec::new(),
            local_sequence: local_seq,
            remote_sequence: remote_seq,
        }
    }
    
    /// Add a change
    pub fn push(&mut self, change: FileChange) {
        self.changes.push(change);
    }
    
    /// Get all changes
    pub fn changes(&self) -> &[FileChange] {
        &self.changes
    }
    
    /// Number of changes
    pub fn len(&self) -> usize {
        self.changes.len()
    }
    
    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }
    
    /// Get added files
    pub fn added(&self) -> impl Iterator<Item = &FileChange> {
        self.changes.iter().filter(|c| c.change_type == ChangeType::Added)
    }
    
    /// Get modified files
    pub fn modified(&self) -> impl Iterator<Item = &FileChange> {
        self.changes.iter().filter(|c| c.change_type == ChangeType::Modified)
    }
    
    /// Get deleted files
    pub fn deleted(&self) -> impl Iterator<Item = &FileChange> {
        self.changes.iter().filter(|c| c.change_type == ChangeType::Deleted)
    }
    
    /// Count by type
    pub fn count_added(&self) -> usize {
        self.added().count()
    }
    
    pub fn count_modified(&self) -> usize {
        self.modified().count()
    }
    
    pub fn count_deleted(&self) -> usize {
        self.deleted().count()
    }
    
    /// Total bytes needed to apply this diff
    pub fn total_bytes(&self) -> u64 {
        self.changes.iter().map(|c| c.bytes_needed()).sum()
    }
    
    /// All blocks needed (deduped)
    pub fn all_needed_blocks(&self) -> Vec<BlockHash> {
        let mut seen = HashSet::new();
        let mut blocks = Vec::new();
        
        for change in &self.changes {
            for block in &change.needed_blocks {
                if seen.insert(block.clone()) {
                    blocks.push(block.clone());
                }
            }
        }
        
        blocks
    }
    
    /// Local sequence
    pub fn local_sequence(&self) -> Sequence {
        self.local_sequence
    }
    
    /// Remote sequence
    pub fn remote_sequence(&self) -> Sequence {
        self.remote_sequence
    }
}

/// Compare two folder indexes to find what needs to sync
pub fn diff_indexes(local: &FolderIndex, remote: &FolderIndex) -> IndexDiff {
    let mut diff = IndexDiff::with_sequences(local.sequence(), remote.sequence());
    
    // Build a set of local paths
    let local_paths: HashSet<&Path> = local.paths().collect();
    
    // Check remote files against local
    for remote_entry in remote.files() {
        let path = remote_entry.path();
        
        if let Some(local_entry) = local.get(path) {
            // File exists locally - check if modified
            if !remote_entry.content_matches(local_entry) {
                // Content changed - compute delta blocks
                let needed = delta_blocks(local_entry, remote_entry);
                diff.push(FileChange::modified(remote_entry.clone(), needed));
            }
            // else: same content, nothing to do
        } else {
            // File doesn't exist locally - it's new
            diff.push(FileChange::added(remote_entry.clone()));
        }
    }
    
    // Check for local files not in remote (deleted remotely)
    let remote_paths: HashSet<&Path> = remote.paths().collect();
    for local_path in local_paths {
        if !remote_paths.contains(local_path) {
            // Check if it was explicitly deleted vs just not synced yet
            if remote.is_deleted(local_path) {
                diff.push(FileChange::deleted(local_path));
            }
        }
    }
    
    diff
}

/// Compute which blocks are needed (not in local)
pub fn delta_blocks(local: &FileEntry, remote: &FileEntry) -> Vec<BlockHash> {
    let local_blocks: HashSet<&BlockHash> = local.blocks().iter().collect();
    
    remote.blocks()
        .iter()
        .filter(|b| !local_blocks.contains(b))
        .cloned()
        .collect()
}

/// Check if a file needs updating based on entries
pub fn needs_update(local: Option<&FileEntry>, remote: &FileEntry) -> bool {
    match local {
        None => true, // File doesn't exist locally
        Some(local_entry) => !remote.content_matches(local_entry),
    }
}

/// Compute blocks we have that remote needs
pub fn blocks_to_send(local: &FileEntry, remote: &FileEntry) -> Vec<BlockHash> {
    let remote_blocks: HashSet<&BlockHash> = remote.blocks().iter().collect();
    
    local.blocks()
        .iter()
        .filter(|b| !remote_blocks.contains(b))
        .cloned()
        .collect()
}

/// Summary statistics for a diff
#[derive(Debug, Clone, Default)]
pub struct DiffStats {
    pub added: usize,
    pub modified: usize,
    pub deleted: usize,
    pub bytes_needed: u64,
    pub blocks_needed: usize,
}

impl DiffStats {
    /// Compute stats from a diff
    pub fn from_diff(diff: &IndexDiff) -> Self {
        let blocks = diff.all_needed_blocks();
        Self {
            added: diff.count_added(),
            modified: diff.count_modified(),
            deleted: diff.count_deleted(),
            bytes_needed: diff.total_bytes(),
            blocks_needed: blocks.len(),
        }
    }
    
    /// Total changes
    pub fn total_changes(&self) -> usize {
        self.added + self.modified + self.deleted
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::compute_hash;
    
    fn make_block(data: &[u8]) -> BlockHash {
        compute_hash(data)
    }
    
    fn make_entry(path: &str, blocks: Vec<BlockHash>) -> FileEntry {
        let size = blocks.len() as u64 * 1024;
        FileEntry::new(path, size, 1700000000, 0o644, blocks)
    }
    
    #[test]
    fn test_file_change_added() {
        let blocks = vec![make_block(b"data")];
        let entry = make_entry("new.txt", blocks.clone());
        let change = FileChange::added(entry);
        
        assert_eq!(change.change_type, ChangeType::Added);
        assert!(!change.is_deleted());
        assert_eq!(change.needed_blocks.len(), 1);
    }
    
    #[test]
    fn test_file_change_deleted() {
        let change = FileChange::deleted("removed.txt");
        
        assert_eq!(change.change_type, ChangeType::Deleted);
        assert!(change.is_deleted());
        assert!(change.needed_blocks.is_empty());
    }
    
    #[test]
    fn test_diff_indexes_added() {
        let local = FolderIndex::with_id("test");
        let mut remote = FolderIndex::with_id("test");
        remote.put(make_entry("new.txt", vec![make_block(b"content")]));
        
        let diff = diff_indexes(&local, &remote);
        
        assert_eq!(diff.count_added(), 1);
        assert_eq!(diff.count_modified(), 0);
        assert_eq!(diff.count_deleted(), 0);
    }
    
    #[test]
    fn test_diff_indexes_modified() {
        let block1 = make_block(b"original");
        let block2 = make_block(b"updated");
        
        let mut local = FolderIndex::with_id("test");
        local.put(make_entry("file.txt", vec![block1.clone()]));
        
        let mut remote = FolderIndex::with_id("test");
        remote.put(make_entry("file.txt", vec![block2.clone()]));
        
        let diff = diff_indexes(&local, &remote);
        
        assert_eq!(diff.count_added(), 0);
        assert_eq!(diff.count_modified(), 1);
        assert_eq!(diff.count_deleted(), 0);
        
        // Should need only the new block
        let needed = diff.all_needed_blocks();
        assert_eq!(needed.len(), 1);
        assert_eq!(needed[0], block2);
    }
    
    #[test]
    fn test_diff_indexes_deleted() {
        let mut local = FolderIndex::with_id("test");
        local.put(make_entry("old.txt", vec![make_block(b"data")]));
        
        let mut remote = FolderIndex::with_id("test");
        // Add and remove to mark as deleted
        remote.put(make_entry("old.txt", vec![make_block(b"data")]));
        remote.remove("old.txt");
        
        let diff = diff_indexes(&local, &remote);
        
        assert_eq!(diff.count_deleted(), 1);
    }
    
    #[test]
    fn test_delta_blocks() {
        let shared = make_block(b"shared");
        let only_local = make_block(b"local");
        let only_remote = make_block(b"remote");
        
        let local = make_entry("file.txt", vec![shared.clone(), only_local]);
        let remote = make_entry("file.txt", vec![shared, only_remote.clone()]);
        
        let delta = delta_blocks(&local, &remote);
        
        assert_eq!(delta.len(), 1);
        assert_eq!(delta[0], only_remote);
    }
    
    #[test]
    fn test_diff_stats() {
        let mut diff = IndexDiff::new();
        diff.push(FileChange::added(make_entry("a.txt", vec![make_block(b"a")])));
        diff.push(FileChange::added(make_entry("b.txt", vec![make_block(b"b")])));
        diff.push(FileChange::deleted("c.txt"));
        
        let stats = DiffStats::from_diff(&diff);
        
        assert_eq!(stats.added, 2);
        assert_eq!(stats.deleted, 1);
        assert_eq!(stats.total_changes(), 3);
    }
}
