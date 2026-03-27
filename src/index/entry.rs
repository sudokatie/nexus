//! File entry representation for indexing

use crate::storage::BlockHash;
use serde::{Deserialize, Serialize};
use std::fs::Metadata;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Unix timestamp in seconds
pub type Timestamp = u64;

/// File permissions mode
pub type FileMode = u32;

/// A file entry in the index
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileEntry {
    /// Relative path within sync folder
    path: PathBuf,
    /// File size in bytes
    size: u64,
    /// Modification time (Unix timestamp)
    mtime: Timestamp,
    /// File mode/permissions
    mode: FileMode,
    /// Block hashes that compose this file
    blocks: Vec<BlockHash>,
}

impl FileEntry {
    /// Create a new file entry
    pub fn new(
        path: impl Into<PathBuf>,
        size: u64,
        mtime: Timestamp,
        mode: FileMode,
        blocks: Vec<BlockHash>,
    ) -> Self {
        Self {
            path: path.into(),
            size,
            mtime,
            mode,
            blocks,
        }
    }
    
    /// Create from filesystem metadata (without blocks)
    pub fn from_metadata(path: impl Into<PathBuf>, metadata: &Metadata) -> Self {
        let mtime = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        
        #[cfg(unix)]
        let mode = {
            use std::os::unix::fs::PermissionsExt;
            metadata.permissions().mode()
        };
        
        #[cfg(not(unix))]
        let mode = if metadata.permissions().readonly() { 0o444 } else { 0o644 };
        
        Self {
            path: path.into(),
            size: metadata.len(),
            mtime,
            mode,
            blocks: Vec::new(),
        }
    }
    
    /// Get the relative path
    pub fn path(&self) -> &Path {
        &self.path
    }
    
    /// Get file size
    pub fn size(&self) -> u64 {
        self.size
    }
    
    /// Get modification time
    pub fn mtime(&self) -> Timestamp {
        self.mtime
    }
    
    /// Get file mode
    pub fn mode(&self) -> FileMode {
        self.mode
    }
    
    /// Get block hashes
    pub fn blocks(&self) -> &[BlockHash] {
        &self.blocks
    }
    
    /// Set block hashes
    pub fn set_blocks(&mut self, blocks: Vec<BlockHash>) {
        self.blocks = blocks;
    }
    
    /// Number of blocks
    pub fn block_count(&self) -> usize {
        self.blocks.len()
    }
    
    /// Check if file has blocks
    pub fn has_blocks(&self) -> bool {
        !self.blocks.is_empty()
    }
    
    /// Generate manifest string (for debugging/display)
    pub fn to_manifest(&self) -> String {
        let mut manifest = format!(
            "path: {}\nsize: {}\nmtime: {}\nmode: {:o}\nblocks:\n",
            self.path.display(),
            self.size,
            self.mtime,
            self.mode
        );
        
        for (i, hash) in self.blocks.iter().enumerate() {
            manifest.push_str(&format!("  {}: {}\n", i, hash.short()));
        }
        
        manifest
    }
    
    /// Check if this entry is newer than another
    pub fn is_newer_than(&self, other: &FileEntry) -> bool {
        self.mtime > other.mtime
    }
    
    /// Check if content matches (by comparing blocks)
    pub fn content_matches(&self, other: &FileEntry) -> bool {
        self.blocks == other.blocks
    }
    
    /// Check if this entry needs re-chunking based on metadata change
    pub fn needs_rechunk(&self, metadata: &Metadata) -> bool {
        let current_mtime = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        
        self.size != metadata.len() || self.mtime != current_mtime
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::compute_hash;
    use tempfile::NamedTempFile;
    use std::io::Write;
    
    fn make_block_hashes(data: &[&[u8]]) -> Vec<BlockHash> {
        data.iter().map(|d| compute_hash(d)).collect()
    }
    
    #[test]
    fn test_file_entry_new() {
        let blocks = make_block_hashes(&[b"chunk1", b"chunk2"]);
        let entry = FileEntry::new("test.txt", 1024, 1700000000, 0o644, blocks.clone());
        
        assert_eq!(entry.path(), Path::new("test.txt"));
        assert_eq!(entry.size(), 1024);
        assert_eq!(entry.mtime(), 1700000000);
        assert_eq!(entry.mode(), 0o644);
        assert_eq!(entry.blocks(), &blocks);
        assert_eq!(entry.block_count(), 2);
    }
    
    #[test]
    fn test_file_entry_from_metadata() {
        let mut tmpfile = NamedTempFile::new().unwrap();
        tmpfile.write_all(b"test content").unwrap();
        tmpfile.flush().unwrap();
        
        let metadata = tmpfile.as_file().metadata().unwrap();
        let entry = FileEntry::from_metadata("test.txt", &metadata);
        
        assert_eq!(entry.path(), Path::new("test.txt"));
        assert_eq!(entry.size(), 12); // "test content" length
        assert!(entry.mtime() > 0);
        assert!(!entry.has_blocks());
    }
    
    #[test]
    fn test_file_entry_set_blocks() {
        let mut entry = FileEntry::new("test.txt", 100, 0, 0o644, vec![]);
        assert!(!entry.has_blocks());
        
        let blocks = make_block_hashes(&[b"data"]);
        entry.set_blocks(blocks.clone());
        
        assert!(entry.has_blocks());
        assert_eq!(entry.blocks(), &blocks);
    }
    
    #[test]
    fn test_file_entry_manifest() {
        let blocks = make_block_hashes(&[b"chunk1"]);
        let entry = FileEntry::new("dir/file.txt", 100, 1700000000, 0o755, blocks);
        
        let manifest = entry.to_manifest();
        assert!(manifest.contains("path: dir/file.txt"));
        assert!(manifest.contains("size: 100"));
        assert!(manifest.contains("mtime: 1700000000"));
        assert!(manifest.contains("mode: 755"));
        assert!(manifest.contains("blocks:"));
    }
    
    #[test]
    fn test_file_entry_comparison() {
        let blocks1 = make_block_hashes(&[b"data1"]);
        let blocks2 = make_block_hashes(&[b"data2"]);
        
        let entry1 = FileEntry::new("file.txt", 100, 1000, 0o644, blocks1.clone());
        let entry2 = FileEntry::new("file.txt", 100, 2000, 0o644, blocks1.clone());
        let entry3 = FileEntry::new("file.txt", 100, 1000, 0o644, blocks2);
        
        assert!(entry2.is_newer_than(&entry1));
        assert!(!entry1.is_newer_than(&entry2));
        assert!(entry1.content_matches(&entry2)); // Same blocks
        assert!(!entry1.content_matches(&entry3)); // Different blocks
    }
    
    #[test]
    fn test_file_entry_serialization() {
        let blocks = make_block_hashes(&[b"chunk"]);
        let entry = FileEntry::new("test.txt", 500, 1700000000, 0o644, blocks);
        
        let json = serde_json::to_string(&entry).unwrap();
        let restored: FileEntry = serde_json::from_str(&json).unwrap();
        
        assert_eq!(entry, restored);
    }
}
