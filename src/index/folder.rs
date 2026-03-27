//! Folder index - tracks all files in a sync folder

use super::entry::FileEntry;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Unique identifier for a folder
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FolderId(String);

impl FolderId {
    /// Create a new folder ID
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
    
    /// Get the ID string
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for FolderId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Sequence number for ordering updates
pub type Sequence = u64;

/// Record of a deleted file
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DeletedFile {
    /// Path that was deleted
    pub path: PathBuf,
    /// When it was deleted (sequence number)
    pub deleted_at: Sequence,
}

/// Index of all files in a sync folder
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderIndex {
    /// Unique folder identifier
    folder_id: FolderId,
    /// Current sequence number
    sequence: Sequence,
    /// Active files (path -> entry)
    files: HashMap<PathBuf, FileEntry>,
    /// Recently deleted files
    deleted: Vec<DeletedFile>,
    /// Maximum deleted entries to retain
    #[serde(default = "default_max_deleted")]
    max_deleted: usize,
}

fn default_max_deleted() -> usize {
    1000
}

impl FolderIndex {
    /// Create a new empty folder index
    pub fn new(folder_id: impl Into<FolderId>) -> Self {
        Self {
            folder_id: folder_id.into(),
            sequence: 0,
            files: HashMap::new(),
            deleted: Vec::new(),
            max_deleted: default_max_deleted(),
        }
    }
    
    /// Create with a specific folder ID string
    pub fn with_id(id: impl Into<String>) -> Self {
        Self::new(FolderId::new(id))
    }
    
    /// Get folder ID
    pub fn folder_id(&self) -> &FolderId {
        &self.folder_id
    }
    
    /// Get current sequence number
    pub fn sequence(&self) -> Sequence {
        self.sequence
    }
    
    /// Get a file entry by path
    pub fn get(&self, path: impl AsRef<Path>) -> Option<&FileEntry> {
        self.files.get(path.as_ref())
    }
    
    /// Check if a file exists in the index
    pub fn contains(&self, path: impl AsRef<Path>) -> bool {
        self.files.contains_key(path.as_ref())
    }
    
    /// Add or update a file entry
    pub fn put(&mut self, entry: FileEntry) {
        self.sequence += 1;
        self.files.insert(entry.path().to_path_buf(), entry);
    }
    
    /// Remove a file and track as deleted
    pub fn remove(&mut self, path: impl AsRef<Path>) -> Option<FileEntry> {
        let path = path.as_ref();
        if let Some(entry) = self.files.remove(path) {
            self.sequence += 1;
            self.deleted.push(DeletedFile {
                path: path.to_path_buf(),
                deleted_at: self.sequence,
            });
            self.trim_deleted();
            Some(entry)
        } else {
            None
        }
    }
    
    /// Get all file entries
    pub fn files(&self) -> impl Iterator<Item = &FileEntry> {
        self.files.values()
    }
    
    /// Get all file paths
    pub fn paths(&self) -> impl Iterator<Item = &Path> {
        self.files.keys().map(|p| p.as_path())
    }
    
    /// Number of files in index
    pub fn file_count(&self) -> usize {
        self.files.len()
    }
    
    /// Total size of all files
    pub fn total_size(&self) -> u64 {
        self.files.values().map(|e| e.size()).sum()
    }
    
    /// Get deleted files
    pub fn deleted(&self) -> &[DeletedFile] {
        &self.deleted
    }
    
    /// Get deleted files since a sequence number
    pub fn deleted_since(&self, seq: Sequence) -> Vec<&DeletedFile> {
        self.deleted.iter().filter(|d| d.deleted_at > seq).collect()
    }
    
    /// Check if a path was recently deleted
    pub fn is_deleted(&self, path: impl AsRef<Path>) -> bool {
        let path = path.as_ref();
        self.deleted.iter().any(|d| d.path == path)
    }
    
    /// Clear deleted entries older than a sequence
    pub fn clear_deleted_before(&mut self, seq: Sequence) {
        self.deleted.retain(|d| d.deleted_at >= seq);
    }
    
    /// Trim deleted list to max size
    fn trim_deleted(&mut self) {
        if self.deleted.len() > self.max_deleted {
            let excess = self.deleted.len() - self.max_deleted;
            self.deleted.drain(0..excess);
        }
    }
    
    /// Serialize to JSON
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
    
    /// Deserialize from JSON
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
    
    /// Save to file
    pub fn save(&self, path: impl AsRef<Path>) -> std::io::Result<()> {
        let json = self.to_json().map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, e)
        })?;
        std::fs::write(path, json)
    }
    
    /// Load from file
    pub fn load(path: impl AsRef<Path>) -> std::io::Result<Self> {
        let json = std::fs::read_to_string(path)?;
        Self::from_json(&json).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, e)
        })
    }
}

impl From<String> for FolderId {
    fn from(s: String) -> Self {
        Self::new(s)
    }
}

impl From<&str> for FolderId {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    fn make_entry(path: &str, size: u64) -> FileEntry {
        FileEntry::new(path, size, 1700000000, 0o644, vec![])
    }
    
    #[test]
    fn test_folder_index_new() {
        let index = FolderIndex::with_id("test-folder");
        
        assert_eq!(index.folder_id().as_str(), "test-folder");
        assert_eq!(index.sequence(), 0);
        assert_eq!(index.file_count(), 0);
    }
    
    #[test]
    fn test_folder_index_put_get() {
        let mut index = FolderIndex::with_id("test");
        let entry = make_entry("file.txt", 100);
        
        index.put(entry.clone());
        
        assert!(index.contains("file.txt"));
        assert_eq!(index.file_count(), 1);
        assert_eq!(index.sequence(), 1);
        
        let retrieved = index.get("file.txt").unwrap();
        assert_eq!(retrieved.size(), 100);
    }
    
    #[test]
    fn test_folder_index_remove() {
        let mut index = FolderIndex::with_id("test");
        index.put(make_entry("file1.txt", 100));
        index.put(make_entry("file2.txt", 200));
        
        let removed = index.remove("file1.txt");
        assert!(removed.is_some());
        assert!(!index.contains("file1.txt"));
        assert!(index.contains("file2.txt"));
        assert!(index.is_deleted("file1.txt"));
        assert_eq!(index.deleted().len(), 1);
    }
    
    #[test]
    fn test_folder_index_total_size() {
        let mut index = FolderIndex::with_id("test");
        index.put(make_entry("a.txt", 100));
        index.put(make_entry("b.txt", 200));
        index.put(make_entry("c.txt", 300));
        
        assert_eq!(index.total_size(), 600);
    }
    
    #[test]
    fn test_folder_index_deleted_since() {
        let mut index = FolderIndex::with_id("test");
        index.put(make_entry("a.txt", 100)); // seq 1
        index.put(make_entry("b.txt", 200)); // seq 2
        
        let checkpoint = index.sequence();
        
        index.remove("a.txt"); // seq 3
        index.put(make_entry("c.txt", 300)); // seq 4
        index.remove("b.txt"); // seq 5
        
        let deleted = index.deleted_since(checkpoint);
        assert_eq!(deleted.len(), 2);
    }
    
    #[test]
    fn test_folder_index_serialization() {
        let mut index = FolderIndex::with_id("my-folder");
        index.put(make_entry("doc.txt", 500));
        index.put(make_entry("img.png", 1000));
        index.remove("doc.txt");
        
        let json = index.to_json().unwrap();
        let restored = FolderIndex::from_json(&json).unwrap();
        
        assert_eq!(restored.folder_id().as_str(), "my-folder");
        assert_eq!(restored.file_count(), 1);
        assert!(restored.is_deleted("doc.txt"));
    }
}
