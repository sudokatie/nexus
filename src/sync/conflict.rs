//! Conflict resolution for sync operations

use crate::index::FileEntry;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Conflict resolution strategy
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ConflictStrategy {
    /// Keep the newest version (by mtime)
    #[default]
    NewestWins,
    /// Keep the local version
    LocalWins,
    /// Keep the remote version
    RemoteWins,
    /// Create conflict copies
    CreateCopy,
}

/// A detected conflict
#[derive(Debug, Clone)]
pub struct Conflict {
    /// Path of the conflicting file
    pub path: PathBuf,
    /// Local version
    pub local: FileEntry,
    /// Remote version
    pub remote: FileEntry,
    /// Resolution applied
    pub resolution: Option<ConflictResolution>,
}

/// Resolution result
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictResolution {
    /// Keep local version
    KeepLocal,
    /// Keep remote version
    KeepRemote,
    /// Keep both (create conflict copy)
    KeepBoth,
}

impl Conflict {
    /// Create a new conflict
    pub fn new(path: impl Into<PathBuf>, local: FileEntry, remote: FileEntry) -> Self {
        Self {
            path: path.into(),
            local,
            remote,
            resolution: None,
        }
    }
    
    /// Resolve using a strategy
    pub fn resolve(&mut self, strategy: ConflictStrategy) -> ConflictResolution {
        let resolution = match strategy {
            ConflictStrategy::NewestWins => {
                if self.local.mtime() >= self.remote.mtime() {
                    ConflictResolution::KeepLocal
                } else {
                    ConflictResolution::KeepRemote
                }
            }
            ConflictStrategy::LocalWins => ConflictResolution::KeepLocal,
            ConflictStrategy::RemoteWins => ConflictResolution::KeepRemote,
            ConflictStrategy::CreateCopy => ConflictResolution::KeepBoth,
        };
        
        self.resolution = Some(resolution);
        resolution
    }
    
    /// Get conflict copy filename
    pub fn conflict_filename(&self, device_name: &str) -> PathBuf {
        let path = &self.path;
        let stem = path.file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let ext = path.extension()
            .map(|s| format!(".{}", s.to_string_lossy()))
            .unwrap_or_default();
        
        let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
        let conflict_name = format!("{}.sync-conflict-{}-{}{}", stem, timestamp, device_name, ext);
        
        path.parent()
            .map(|p| p.join(&conflict_name))
            .unwrap_or_else(|| PathBuf::from(&conflict_name))
    }
}

/// Conflict manager
#[derive(Debug)]
pub struct ConflictManager {
    /// Default strategy
    strategy: ConflictStrategy,
    /// Device name for conflict copies
    device_name: String,
    /// Detected conflicts
    conflicts: Vec<Conflict>,
}

impl ConflictManager {
    /// Create a new conflict manager
    pub fn new(device_name: impl Into<String>) -> Self {
        Self {
            strategy: ConflictStrategy::default(),
            device_name: device_name.into(),
            conflicts: Vec::new(),
        }
    }
    
    /// Set default strategy
    pub fn set_strategy(&mut self, strategy: ConflictStrategy) {
        self.strategy = strategy;
    }
    
    /// Get current strategy
    pub fn strategy(&self) -> ConflictStrategy {
        self.strategy
    }
    
    /// Add a conflict
    pub fn add(&mut self, conflict: Conflict) {
        self.conflicts.push(conflict);
    }
    
    /// Check if there are conflicts
    pub fn has_conflicts(&self) -> bool {
        !self.conflicts.is_empty()
    }
    
    /// Number of conflicts
    pub fn count(&self) -> usize {
        self.conflicts.len()
    }
    
    /// Get all conflicts
    pub fn conflicts(&self) -> &[Conflict] {
        &self.conflicts
    }
    
    /// Resolve all conflicts with the default strategy
    pub fn resolve_all(&mut self) -> Vec<ConflictResolution> {
        let strategy = self.strategy;
        self.conflicts.iter_mut()
            .map(|c| c.resolve(strategy))
            .collect()
    }
    
    /// Clear resolved conflicts
    pub fn clear(&mut self) {
        self.conflicts.clear();
    }
    
    /// Get conflict copy path for a conflict
    pub fn conflict_path(&self, conflict: &Conflict) -> PathBuf {
        conflict.conflict_filename(&self.device_name)
    }
}

/// Check if two entries are in conflict
pub fn is_conflict(local: &FileEntry, remote: &FileEntry) -> bool {
    // Conflict if both modified (different content, neither is ancestor)
    !local.content_matches(remote) && 
    local.mtime() != 0 && 
    remote.mtime() != 0
}

/// Determine winner using newest-wins
pub fn newest_wins(local: &FileEntry, remote: &FileEntry) -> ConflictResolution {
    if local.mtime() >= remote.mtime() {
        ConflictResolution::KeepLocal
    } else {
        ConflictResolution::KeepRemote
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::compute_hash;
    
    fn make_entry(path: &str, data: &[u8], mtime: u64) -> FileEntry {
        let blocks = vec![compute_hash(data)];
        FileEntry::new(path, data.len() as u64, mtime, 0o644, blocks)
    }
    
    #[test]
    fn test_conflict_resolution_newest_wins() {
        let local = make_entry("file.txt", b"local", 2000);
        let remote = make_entry("file.txt", b"remote", 1000);
        
        let mut conflict = Conflict::new("file.txt", local, remote);
        let resolution = conflict.resolve(ConflictStrategy::NewestWins);
        
        assert_eq!(resolution, ConflictResolution::KeepLocal);
    }
    
    #[test]
    fn test_conflict_resolution_remote_newer() {
        let local = make_entry("file.txt", b"local", 1000);
        let remote = make_entry("file.txt", b"remote", 2000);
        
        let mut conflict = Conflict::new("file.txt", local, remote);
        let resolution = conflict.resolve(ConflictStrategy::NewestWins);
        
        assert_eq!(resolution, ConflictResolution::KeepRemote);
    }
    
    #[test]
    fn test_conflict_filename() {
        let local = make_entry("docs/report.pdf", b"local", 1000);
        let remote = make_entry("docs/report.pdf", b"remote", 2000);
        
        let conflict = Conflict::new("docs/report.pdf", local, remote);
        let conflict_path = conflict.conflict_filename("my-laptop");
        
        let name = conflict_path.file_name().unwrap().to_string_lossy();
        assert!(name.contains("sync-conflict"));
        assert!(name.contains("my-laptop"));
        assert!(name.ends_with(".pdf"));
    }
    
    #[test]
    fn test_conflict_manager() {
        let local = make_entry("a.txt", b"local", 1000);
        let remote = make_entry("a.txt", b"remote", 2000);
        
        let mut manager = ConflictManager::new("device1");
        assert!(!manager.has_conflicts());
        
        manager.add(Conflict::new("a.txt", local, remote));
        assert!(manager.has_conflicts());
        assert_eq!(manager.count(), 1);
        
        let resolutions = manager.resolve_all();
        assert_eq!(resolutions.len(), 1);
        assert_eq!(resolutions[0], ConflictResolution::KeepRemote);
    }
    
    #[test]
    fn test_is_conflict() {
        let same1 = make_entry("f.txt", b"same", 1000);
        let same2 = make_entry("f.txt", b"same", 2000);
        let diff = make_entry("f.txt", b"different", 1500);
        
        // Same content = no conflict
        assert!(!is_conflict(&same1, &same2));
        
        // Different content = conflict
        assert!(is_conflict(&same1, &diff));
    }
}
