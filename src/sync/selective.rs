//! Selective synchronization - choose which folders/files to sync.
//!
//! Provides gitignore-style pattern matching and a TUI for folder selection.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Pattern matching mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PatternKind {
    /// Include files matching this pattern.
    Include,
    /// Exclude files matching this pattern.
    Exclude,
}

/// A single ignore/include pattern.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncPattern {
    /// The pattern string.
    pub pattern: String,
    /// Whether to include or exclude.
    pub kind: PatternKind,
    /// Match directories only (trailing /).
    pub directory_only: bool,
    /// Anchored to root (starts with /).
    pub anchored: bool,
}

impl SyncPattern {
    /// Create a new exclude pattern.
    pub fn exclude(pattern: impl Into<String>) -> Self {
        Self::new(pattern.into(), PatternKind::Exclude)
    }

    /// Create a new include pattern.
    pub fn include(pattern: impl Into<String>) -> Self {
        Self::new(pattern.into(), PatternKind::Include)
    }

    fn new(mut pattern: String, kind: PatternKind) -> Self {
        let anchored = pattern.starts_with('/');
        if anchored {
            pattern = pattern[1..].to_string();
        }

        let directory_only = pattern.ends_with('/');
        if directory_only {
            pattern = pattern[..pattern.len() - 1].to_string();
        }

        Self {
            pattern,
            kind,
            directory_only,
            anchored,
        }
    }

    /// Check if a path matches this pattern.
    pub fn matches(&self, path: &Path, is_dir: bool) -> bool {
        // Directory-only patterns don't match files
        if self.directory_only && !is_dir {
            return false;
        }

        let path_str = path.to_string_lossy();
        let name = path.file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();

        if self.anchored {
            // Must match from root
            self.glob_match(&self.pattern, &path_str)
        } else {
            // Can match any component
            self.glob_match(&self.pattern, &name) ||
            self.glob_match(&self.pattern, &path_str)
        }
    }

    /// Simple glob matching (supports * and **).
    fn glob_match(&self, pattern: &str, text: &str) -> bool {
        if pattern == "**" {
            return true;
        }

        if pattern.contains("**") {
            // Double star matches any path
            let parts: Vec<&str> = pattern.split("**").collect();
            if parts.len() == 2 {
                let (prefix, suffix) = (parts[0], parts[1]);
                let prefix = prefix.trim_end_matches('/');
                let suffix = suffix.trim_start_matches('/');
                
                if !prefix.is_empty() && !text.starts_with(prefix) {
                    return false;
                }
                if !suffix.is_empty() && !text.ends_with(suffix) {
                    return false;
                }
                return true;
            }
        }

        // Single star matching
        if pattern.contains('*') {
            let parts: Vec<&str> = pattern.split('*').collect();
            let mut pos = 0;
            
            for (i, part) in parts.iter().enumerate() {
                if part.is_empty() {
                    continue;
                }
                
                if i == 0 {
                    // First part must be at start
                    if !text.starts_with(part) {
                        return false;
                    }
                    pos = part.len();
                } else if i == parts.len() - 1 {
                    // Last part must be at end
                    if !text.ends_with(part) {
                        return false;
                    }
                } else {
                    // Middle parts must appear in order
                    if let Some(idx) = text[pos..].find(part) {
                        pos += idx + part.len();
                    } else {
                        return false;
                    }
                }
            }
            return true;
        }

        // Exact match
        text == pattern
    }
}

/// Selective sync configuration for a folder.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SelectiveConfig {
    /// Patterns (processed in order, last match wins).
    pub patterns: Vec<SyncPattern>,
    /// Explicitly selected subfolders (empty = all).
    pub selected_folders: HashSet<PathBuf>,
    /// Whether selective sync is enabled.
    pub enabled: bool,
}

impl SelectiveConfig {
    /// Create a new config with default patterns.
    pub fn new() -> Self {
        Self {
            patterns: Self::default_patterns(),
            selected_folders: HashSet::new(),
            enabled: false,
        }
    }

    /// Default ignore patterns.
    pub fn default_patterns() -> Vec<SyncPattern> {
        vec![
            SyncPattern::exclude(".git/"),
            SyncPattern::exclude(".DS_Store"),
            SyncPattern::exclude("Thumbs.db"),
            SyncPattern::exclude("*.tmp"),
            SyncPattern::exclude("*.swp"),
            SyncPattern::exclude("~*"),
            SyncPattern::exclude(".nexus/"),
        ]
    }

    /// Add an exclude pattern.
    pub fn exclude(&mut self, pattern: impl Into<String>) {
        self.patterns.push(SyncPattern::exclude(pattern));
    }

    /// Add an include pattern.
    pub fn include(&mut self, pattern: impl Into<String>) {
        self.patterns.push(SyncPattern::include(pattern));
    }

    /// Select a subfolder for sync.
    pub fn select_folder(&mut self, path: impl Into<PathBuf>) {
        self.selected_folders.insert(path.into());
        self.enabled = true;
    }

    /// Deselect a subfolder.
    pub fn deselect_folder(&mut self, path: &Path) {
        self.selected_folders.remove(path);
    }

    /// Check if a path should be synced.
    pub fn should_sync(&self, path: &Path, is_dir: bool) -> bool {
        // Check selected folders first
        if self.enabled && !self.selected_folders.is_empty() {
            let in_selected = self.selected_folders.iter().any(|selected| {
                path.starts_with(selected) || selected.starts_with(path)
            });
            if !in_selected {
                return false;
            }
        }

        // Apply patterns (last match wins, default include)
        let mut result = true;
        for pattern in &self.patterns {
            if pattern.matches(path, is_dir) {
                result = pattern.kind == PatternKind::Include;
            }
        }
        result
    }

    /// Load patterns from a .nexusignore file.
    pub fn load_ignore_file(&mut self, path: impl AsRef<Path>) -> std::io::Result<()> {
        let content = fs::read_to_string(path)?;
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if let Some(pattern) = line.strip_prefix('!') {
                // Negation = include
                self.patterns.push(SyncPattern::include(pattern));
            } else {
                self.patterns.push(SyncPattern::exclude(line));
            }
        }
        Ok(())
    }

    /// Save patterns to a .nexusignore file.
    pub fn save_ignore_file(&self, path: impl AsRef<Path>) -> std::io::Result<()> {
        let mut content = String::new();
        content.push_str("# Nexus ignore patterns\n");
        content.push_str("# Lines starting with ! are include patterns\n\n");

        for pattern in &self.patterns {
            let prefix = if pattern.kind == PatternKind::Include { "!" } else { "" };
            let anchor = if pattern.anchored { "/" } else { "" };
            let dir_suffix = if pattern.directory_only { "/" } else { "" };
            content.push_str(&format!("{}{}{}{}\n", prefix, anchor, pattern.pattern, dir_suffix));
        }

        fs::write(path, content)
    }
}

/// Folder selection entry for TUI.
#[derive(Debug, Clone)]
pub struct FolderEntry {
    /// Path relative to sync root.
    pub path: PathBuf,
    /// Display name.
    pub name: String,
    /// Depth in tree.
    pub depth: usize,
    /// Whether currently selected.
    pub selected: bool,
    /// Whether expanded in tree view.
    pub expanded: bool,
    /// Whether this is a directory.
    pub is_dir: bool,
    /// Size in bytes (files only).
    pub size: u64,
}

impl FolderEntry {
    /// Create from a path.
    pub fn from_path(root: &Path, path: &Path, selected: bool) -> std::io::Result<Self> {
        let relative = path.strip_prefix(root).unwrap_or(path);
        let metadata = fs::metadata(path)?;
        let depth = relative.components().count();

        Ok(Self {
            path: relative.to_path_buf(),
            name: path.file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| ".".to_string()),
            depth,
            selected,
            expanded: depth < 2,
            is_dir: metadata.is_dir(),
            size: if metadata.is_file() { metadata.len() } else { 0 },
        })
    }

    /// Format size for display.
    pub fn size_display(&self) -> String {
        if !self.is_dir {
            format_size(self.size)
        } else {
            String::new()
        }
    }
}

/// Format bytes as human-readable size.
fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Build a list of folder entries for TUI display.
pub fn list_folders(root: &Path, config: &SelectiveConfig, max_depth: usize) -> std::io::Result<Vec<FolderEntry>> {
    let mut entries = Vec::new();
    list_folders_recursive(root, root, config, &mut entries, 0, max_depth)?;
    Ok(entries)
}

fn list_folders_recursive(
    root: &Path,
    path: &Path,
    config: &SelectiveConfig,
    entries: &mut Vec<FolderEntry>,
    depth: usize,
    max_depth: usize,
) -> std::io::Result<()> {
    if depth > max_depth {
        return Ok(());
    }

    let read_dir = match fs::read_dir(path) {
        Ok(rd) => rd,
        Err(_) => return Ok(()),
    };

    let mut items: Vec<_> = read_dir
        .filter_map(|e| e.ok())
        .collect();
    
    // Sort: directories first, then alphabetically
    items.sort_by(|a, b| {
        let a_dir = a.file_type().map(|t| t.is_dir()).unwrap_or(false);
        let b_dir = b.file_type().map(|t| t.is_dir()).unwrap_or(false);
        match (a_dir, b_dir) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.file_name().cmp(&b.file_name()),
        }
    });

    for item in items {
        let item_path = item.path();
        let relative = item_path.strip_prefix(root).unwrap_or(&item_path);
        let is_dir = item.file_type().map(|t| t.is_dir()).unwrap_or(false);

        // Skip ignored items
        if !config.should_sync(relative, is_dir) {
            continue;
        }

        let selected = config.selected_folders.is_empty() || 
            config.selected_folders.contains(relative);

        if let Ok(entry) = FolderEntry::from_path(root, &item_path, selected) {
            entries.push(entry);

            if is_dir {
                list_folders_recursive(root, &item_path, config, entries, depth + 1, max_depth)?;
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_pattern_exclude() {
        let pattern = SyncPattern::exclude("*.tmp");
        assert!(pattern.matches(Path::new("test.tmp"), false));
        assert!(!pattern.matches(Path::new("test.txt"), false));
    }

    #[test]
    fn test_pattern_include() {
        let pattern = SyncPattern::include("important.txt");
        assert_eq!(pattern.kind, PatternKind::Include);
        assert!(pattern.matches(Path::new("important.txt"), false));
    }

    #[test]
    fn test_pattern_directory_only() {
        let pattern = SyncPattern::exclude(".git/");
        assert!(pattern.directory_only);
        assert!(pattern.matches(Path::new(".git"), true));
        assert!(!pattern.matches(Path::new(".git"), false));
    }

    #[test]
    fn test_pattern_anchored() {
        let pattern = SyncPattern::exclude("/build");
        assert!(pattern.anchored);
        assert!(pattern.matches(Path::new("build"), true));
    }

    #[test]
    fn test_pattern_double_star() {
        let pattern = SyncPattern::exclude("**/node_modules");
        assert!(pattern.matches(Path::new("project/node_modules"), true));
        assert!(pattern.matches(Path::new("deep/nested/node_modules"), true));
    }

    #[test]
    fn test_selective_config_new() {
        let config = SelectiveConfig::new();
        assert!(!config.enabled);
        assert!(config.selected_folders.is_empty());
        assert!(!config.patterns.is_empty());
    }

    #[test]
    fn test_should_sync_default() {
        let config = SelectiveConfig::new();
        assert!(config.should_sync(Path::new("file.txt"), false));
        assert!(!config.should_sync(Path::new(".git"), true));
        assert!(!config.should_sync(Path::new("test.tmp"), false));
    }

    #[test]
    fn test_should_sync_selected_folders() {
        let mut config = SelectiveConfig::new();
        config.select_folder("src");
        
        assert!(config.should_sync(Path::new("src"), true));
        assert!(config.should_sync(Path::new("src/main.rs"), false));
        assert!(!config.should_sync(Path::new("docs"), true));
    }

    #[test]
    fn test_load_ignore_file() {
        let tmp = TempDir::new().unwrap();
        let ignore_path = tmp.path().join(".nexusignore");
        
        fs::write(&ignore_path, "# Comment\n*.log\n!important.log\nbuild/\n").unwrap();
        
        let mut config = SelectiveConfig::new();
        config.load_ignore_file(&ignore_path).unwrap();
        
        // Should have default + 3 new patterns
        assert!(config.patterns.len() >= 3);
    }

    #[test]
    fn test_save_ignore_file() {
        let tmp = TempDir::new().unwrap();
        let ignore_path = tmp.path().join(".nexusignore");
        
        let mut config = SelectiveConfig::new();
        config.exclude("*.log");
        config.include("important.log");
        config.save_ignore_file(&ignore_path).unwrap();
        
        let content = fs::read_to_string(&ignore_path).unwrap();
        assert!(content.contains("*.log"));
        assert!(content.contains("!important.log"));
    }

    #[test]
    fn test_format_size() {
        assert_eq!(format_size(500), "500 B");
        assert_eq!(format_size(1024), "1.0 KB");
        assert_eq!(format_size(1024 * 1024), "1.0 MB");
        assert_eq!(format_size(1024 * 1024 * 1024), "1.0 GB");
    }

    #[test]
    fn test_folder_entry() {
        let tmp = TempDir::new().unwrap();
        let file_path = tmp.path().join("test.txt");
        fs::write(&file_path, "hello").unwrap();
        
        let entry = FolderEntry::from_path(tmp.path(), &file_path, true).unwrap();
        assert_eq!(entry.name, "test.txt");
        assert!(!entry.is_dir);
        assert!(entry.selected);
        assert_eq!(entry.size, 5);
    }

    #[test]
    fn test_list_folders() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir(tmp.path().join("src")).unwrap();
        fs::write(tmp.path().join("src/main.rs"), "fn main() {}").unwrap();
        fs::write(tmp.path().join("README.md"), "# Hello").unwrap();
        
        let config = SelectiveConfig::new();
        let entries = list_folders(tmp.path(), &config, 3).unwrap();
        
        assert!(!entries.is_empty());
        let names: Vec<_> = entries.iter().map(|e| &e.name).collect();
        assert!(names.contains(&&"src".to_string()));
    }
}
