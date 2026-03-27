//! Directory scanning for file indexing

use super::entry::FileEntry;
use super::folder::FolderIndex;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Configuration for directory scanning
#[derive(Debug, Clone)]
pub struct ScanConfig {
    /// Patterns to ignore (glob-style)
    pub ignore_patterns: Vec<String>,
    /// Follow symbolic links
    pub follow_symlinks: bool,
    /// Maximum depth (None = unlimited)
    pub max_depth: Option<usize>,
}

impl Default for ScanConfig {
    fn default() -> Self {
        Self {
            ignore_patterns: vec![
                ".git".to_string(),
                ".nexus".to_string(),
                ".DS_Store".to_string(),
                "*.tmp".to_string(),
                "*.swp".to_string(),
            ],
            follow_symlinks: false,
            max_depth: None,
        }
    }
}

/// Directory scanner for building file indexes
#[derive(Debug)]
pub struct Scanner {
    config: ScanConfig,
}

impl Scanner {
    /// Create a new scanner with default config
    pub fn new() -> Self {
        Self {
            config: ScanConfig::default(),
        }
    }
    
    /// Create with custom config
    pub fn with_config(config: ScanConfig) -> Self {
        Self { config }
    }
    
    /// Get config reference
    pub fn config(&self) -> &ScanConfig {
        &self.config
    }
    
    /// Scan a directory and return a folder index
    pub fn scan(&self, root: impl AsRef<Path>, folder_id: impl Into<String>) -> std::io::Result<FolderIndex> {
        let root = root.as_ref();
        let mut index = FolderIndex::with_id(folder_id);
        
        self.scan_dir(root, root, &mut index, 0)?;
        
        Ok(index)
    }
    
    /// Scan a single file and return its entry
    pub fn scan_file(&self, path: impl AsRef<Path>, root: impl AsRef<Path>) -> std::io::Result<FileEntry> {
        let path = path.as_ref();
        let root = root.as_ref();
        
        let metadata = if self.config.follow_symlinks {
            fs::metadata(path)?
        } else {
            fs::symlink_metadata(path)?
        };
        
        if !metadata.is_file() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Path is not a file",
            ));
        }
        
        let relative = path.strip_prefix(root)
            .map_err(|_| std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Path not under root",
            ))?;
        
        Ok(FileEntry::from_metadata(relative, &metadata))
    }
    
    /// Incremental scan - only scan files that changed
    pub fn scan_incremental(
        &self,
        root: impl AsRef<Path>,
        existing: &FolderIndex,
    ) -> std::io::Result<ScanResult> {
        let root = root.as_ref();
        let mut result = ScanResult::default();
        let mut seen_paths: HashSet<PathBuf> = HashSet::new();
        
        self.scan_dir_incremental(root, root, existing, &mut result, &mut seen_paths, 0)?;
        
        // Find deleted files
        for entry in existing.files() {
            if !seen_paths.contains(entry.path()) {
                result.deleted.push(entry.path().to_path_buf());
            }
        }
        
        Ok(result)
    }
    
    fn scan_dir(
        &self,
        dir: &Path,
        root: &Path,
        index: &mut FolderIndex,
        depth: usize,
    ) -> std::io::Result<()> {
        if let Some(max) = self.config.max_depth {
            if depth > max {
                return Ok(());
            }
        }
        
        let entries = fs::read_dir(dir)?;
        
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            
            if self.should_ignore(&name_str) {
                continue;
            }
            
            let metadata = if self.config.follow_symlinks {
                fs::metadata(&path)?
            } else {
                entry.metadata()?
            };
            
            if metadata.is_dir() {
                self.scan_dir(&path, root, index, depth + 1)?;
            } else if metadata.is_file() {
                let relative = path.strip_prefix(root)
                    .map_err(|_| std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "Path not under root",
                    ))?;
                
                let file_entry = FileEntry::from_metadata(relative, &metadata);
                index.put(file_entry);
            }
        }
        
        Ok(())
    }
    
    fn scan_dir_incremental(
        &self,
        dir: &Path,
        root: &Path,
        existing: &FolderIndex,
        result: &mut ScanResult,
        seen: &mut HashSet<PathBuf>,
        depth: usize,
    ) -> std::io::Result<()> {
        if let Some(max) = self.config.max_depth {
            if depth > max {
                return Ok(());
            }
        }
        
        let entries = fs::read_dir(dir)?;
        
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            
            if self.should_ignore(&name_str) {
                continue;
            }
            
            let metadata = if self.config.follow_symlinks {
                fs::metadata(&path)?
            } else {
                entry.metadata()?
            };
            
            if metadata.is_dir() {
                self.scan_dir_incremental(&path, root, existing, result, seen, depth + 1)?;
            } else if metadata.is_file() {
                let relative = path.strip_prefix(root)
                    .map_err(|_| std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "Path not under root",
                    ))?
                    .to_path_buf();
                
                seen.insert(relative.clone());
                
                let file_entry = FileEntry::from_metadata(&relative, &metadata);
                
                if let Some(existing_entry) = existing.get(&relative) {
                    if existing_entry.needs_rechunk(&metadata) {
                        result.modified.push(file_entry);
                    }
                } else {
                    result.added.push(file_entry);
                }
            }
        }
        
        Ok(())
    }
    
    fn should_ignore(&self, name: &str) -> bool {
        for pattern in &self.config.ignore_patterns {
            if pattern.starts_with('*') {
                // Suffix match
                let suffix = &pattern[1..];
                if name.ends_with(suffix) {
                    return true;
                }
            } else if pattern.ends_with('*') {
                // Prefix match
                let prefix = &pattern[..pattern.len() - 1];
                if name.starts_with(prefix) {
                    return true;
                }
            } else if name == pattern {
                // Exact match
                return true;
            }
        }
        false
    }
    
    /// Load ignore patterns from .nexusignore file
    pub fn load_ignore_file(&mut self, path: impl AsRef<Path>) -> std::io::Result<()> {
        let content = fs::read_to_string(path)?;
        for line in content.lines() {
            let line = line.trim();
            if !line.is_empty() && !line.starts_with('#') {
                self.config.ignore_patterns.push(line.to_string());
            }
        }
        Ok(())
    }
}

impl Default for Scanner {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of an incremental scan
#[derive(Debug, Default)]
pub struct ScanResult {
    /// Newly added files
    pub added: Vec<FileEntry>,
    /// Modified files
    pub modified: Vec<FileEntry>,
    /// Deleted file paths
    pub deleted: Vec<PathBuf>,
}

impl ScanResult {
    /// Check if there are any changes
    pub fn has_changes(&self) -> bool {
        !self.added.is_empty() || !self.modified.is_empty() || !self.deleted.is_empty()
    }
    
    /// Total number of changes
    pub fn change_count(&self) -> usize {
        self.added.len() + self.modified.len() + self.deleted.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;
    
    fn create_test_tree(dir: &Path) {
        fs::create_dir_all(dir.join("subdir")).unwrap();
        fs::write(dir.join("file1.txt"), "hello").unwrap();
        fs::write(dir.join("file2.txt"), "world").unwrap();
        fs::write(dir.join("subdir/nested.txt"), "nested").unwrap();
    }
    
    #[test]
    fn test_scanner_scan() {
        let tmp = TempDir::new().unwrap();
        create_test_tree(tmp.path());
        
        let scanner = Scanner::new();
        let index = scanner.scan(tmp.path(), "test").unwrap();
        
        assert_eq!(index.file_count(), 3);
        assert!(index.contains("file1.txt"));
        assert!(index.contains("file2.txt"));
        assert!(index.contains("subdir/nested.txt"));
    }
    
    #[test]
    fn test_scanner_scan_file() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("test.txt"), "content").unwrap();
        
        let scanner = Scanner::new();
        let entry = scanner.scan_file(
            tmp.path().join("test.txt"),
            tmp.path(),
        ).unwrap();
        
        assert_eq!(entry.path(), Path::new("test.txt"));
        assert_eq!(entry.size(), 7);
    }
    
    #[test]
    fn test_scanner_ignore_patterns() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("file.txt"), "keep").unwrap();
        fs::write(tmp.path().join("file.tmp"), "ignore").unwrap();
        fs::write(tmp.path().join(".DS_Store"), "ignore").unwrap();
        fs::create_dir(tmp.path().join(".git")).unwrap();
        fs::write(tmp.path().join(".git/config"), "ignored").unwrap();
        
        let scanner = Scanner::new();
        let index = scanner.scan(tmp.path(), "test").unwrap();
        
        assert_eq!(index.file_count(), 1);
        assert!(index.contains("file.txt"));
    }
    
    #[test]
    fn test_scanner_max_depth() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join("a/b/c")).unwrap();
        fs::write(tmp.path().join("root.txt"), "").unwrap();
        fs::write(tmp.path().join("a/level1.txt"), "").unwrap();
        fs::write(tmp.path().join("a/b/level2.txt"), "").unwrap();
        fs::write(tmp.path().join("a/b/c/level3.txt"), "").unwrap();
        
        let config = ScanConfig {
            max_depth: Some(1),
            ..Default::default()
        };
        let scanner = Scanner::with_config(config);
        let index = scanner.scan(tmp.path(), "test").unwrap();
        
        assert!(index.contains("root.txt"));
        assert!(index.contains("a/level1.txt"));
        assert!(!index.contains("a/b/level2.txt"));
    }
    
    #[test]
    fn test_scanner_incremental() {
        let tmp = TempDir::new().unwrap();
        create_test_tree(tmp.path());
        
        let scanner = Scanner::new();
        let initial = scanner.scan(tmp.path(), "test").unwrap();
        
        // Add a new file
        fs::write(tmp.path().join("new.txt"), "new").unwrap();
        
        // Modify an existing file
        std::thread::sleep(std::time::Duration::from_millis(10));
        fs::write(tmp.path().join("file1.txt"), "modified").unwrap();
        
        // Delete a file
        fs::remove_file(tmp.path().join("file2.txt")).unwrap();
        
        let result = scanner.scan_incremental(tmp.path(), &initial).unwrap();
        
        assert!(result.has_changes());
        assert_eq!(result.added.len(), 1);
        assert_eq!(result.modified.len(), 1);
        assert_eq!(result.deleted.len(), 1);
    }
    
    #[test]
    fn test_scanner_ignore_file() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join(".nexusignore"), "*.log\nsecret/\n# comment\n").unwrap();
        fs::write(tmp.path().join("app.log"), "log").unwrap();
        fs::write(tmp.path().join("keep.txt"), "keep").unwrap();
        
        let mut scanner = Scanner::new();
        scanner.load_ignore_file(tmp.path().join(".nexusignore")).unwrap();
        
        let index = scanner.scan(tmp.path(), "test").unwrap();
        
        assert!(index.contains("keep.txt"));
        assert!(!index.contains("app.log"));
    }
}
