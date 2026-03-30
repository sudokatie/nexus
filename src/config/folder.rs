//! Folder configuration

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Folder sync configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderConfig {
    /// Unique folder ID
    pub id: String,
    /// Local path
    pub path: PathBuf,
    /// Label (display name)
    #[serde(default)]
    pub label: Option<String>,
    /// Folder type
    #[serde(default)]
    pub folder_type: FolderType,
    /// Devices that share this folder
    #[serde(default)]
    pub devices: Vec<String>,
    /// Ignore patterns
    #[serde(default)]
    pub ignore: Vec<String>,
    /// Rescan interval in seconds (0 = filesystem watch only)
    #[serde(default = "default_rescan")]
    pub rescan_interval_secs: u64,
    /// File versioning
    #[serde(default)]
    pub versioning: Option<VersioningConfig>,
    /// Minimum free space percentage
    #[serde(default)]
    pub min_free_percent: f32,
    /// Pause sync
    #[serde(default)]
    pub paused: bool,
}

fn default_rescan() -> u64 {
    60
}

impl FolderConfig {
    /// Create a new folder config
    pub fn new(id: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        Self {
            id: id.into(),
            path: path.into(),
            label: None,
            folder_type: FolderType::SendReceive,
            devices: Vec::new(),
            ignore: Vec::new(),
            rescan_interval_secs: default_rescan(),
            versioning: None,
            min_free_percent: 1.0,
            paused: false,
        }
    }
    
    /// Set label
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }
    
    /// Add a device
    pub fn with_device(mut self, device_id: impl Into<String>) -> Self {
        self.devices.push(device_id.into());
        self
    }
    
    /// Add ignore pattern
    pub fn with_ignore(mut self, pattern: impl Into<String>) -> Self {
        self.ignore.push(pattern.into());
        self
    }
    
    /// Set folder type
    pub fn with_type(mut self, folder_type: FolderType) -> Self {
        self.folder_type = folder_type;
        self
    }
    
    /// Get display name
    pub fn display_name(&self) -> &str {
        self.label.as_deref().unwrap_or(&self.id)
    }
    
    /// Check if path exists
    pub fn path_exists(&self) -> bool {
        self.path.exists()
    }
    
    /// Get default ignore patterns
    pub fn default_ignores() -> Vec<String> {
        vec![
            ".nexusignore".to_string(),
            ".DS_Store".to_string(),
            "Thumbs.db".to_string(),
            "~*".to_string(),
            "*.tmp".to_string(),
            ".git".to_string(),
            "node_modules".to_string(),
        ]
    }
}

/// Folder sync type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum FolderType {
    /// Send and receive changes
    #[default]
    SendReceive,
    /// Send only (read-only for others)
    SendOnly,
    /// Receive only (read-only locally)
    ReceiveOnly,
}

impl FolderType {
    /// Can we send changes
    pub fn can_send(&self) -> bool {
        matches!(self, Self::SendReceive | Self::SendOnly)
    }
    
    /// Can we receive changes
    pub fn can_receive(&self) -> bool {
        matches!(self, Self::SendReceive | Self::ReceiveOnly)
    }
}

/// File versioning configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersioningConfig {
    /// Versioning type
    #[serde(rename = "type")]
    pub versioning_type: VersioningType,
    /// Parameters
    #[serde(default)]
    pub params: VersioningParams,
}

/// Versioning type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum VersioningType {
    /// No versioning
    #[default]
    None,
    /// Trash can (simple delete protection)
    TrashCan,
    /// Simple versioning (keep N versions)
    Simple,
    /// Staggered versioning (time-based retention)
    Staggered,
}

/// Versioning parameters
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VersioningParams {
    /// Clean out period (seconds)
    #[serde(default)]
    pub clean_out_days: u32,
    /// Maximum age (seconds)
    #[serde(default)]
    pub max_age_days: u32,
    /// Versions to keep
    #[serde(default)]
    pub keep: u32,
    /// Versions path
    #[serde(default)]
    pub versions_path: Option<PathBuf>,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_folder_config_new() {
        let config = FolderConfig::new("default", "/home/user/Sync");
        
        assert_eq!(config.id, "default");
        assert_eq!(config.path, PathBuf::from("/home/user/Sync"));
        assert_eq!(config.folder_type, FolderType::SendReceive);
        assert!(!config.paused);
    }
    
    #[test]
    fn test_folder_config_builder() {
        let config = FolderConfig::new("docs", "/docs")
            .with_label("Documents")
            .with_device("DEVICE-ID-1")
            .with_device("DEVICE-ID-2")
            .with_ignore(".git")
            .with_type(FolderType::SendOnly);
        
        assert_eq!(config.display_name(), "Documents");
        assert_eq!(config.devices.len(), 2);
        assert_eq!(config.ignore.len(), 1);
        assert_eq!(config.folder_type, FolderType::SendOnly);
    }
    
    #[test]
    fn test_folder_type() {
        assert!(FolderType::SendReceive.can_send());
        assert!(FolderType::SendReceive.can_receive());
        
        assert!(FolderType::SendOnly.can_send());
        assert!(!FolderType::SendOnly.can_receive());
        
        assert!(!FolderType::ReceiveOnly.can_send());
        assert!(FolderType::ReceiveOnly.can_receive());
    }
    
    #[test]
    fn test_versioning_config() {
        let versioning = VersioningConfig {
            versioning_type: VersioningType::Simple,
            params: VersioningParams {
                keep: 5,
                ..Default::default()
            },
        };
        
        assert_eq!(versioning.versioning_type, VersioningType::Simple);
        assert_eq!(versioning.params.keep, 5);
    }
}
