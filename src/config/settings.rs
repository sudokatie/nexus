//! Global settings configuration

use super::device::{DeviceConfig, PeerDevice};
use super::folder::FolderConfig;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Main configuration file
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    /// This device
    #[serde(default)]
    pub device: DeviceConfig,
    /// Sync folders
    #[serde(default)]
    pub folders: Vec<FolderConfig>,
    /// Peer devices
    #[serde(default)]
    pub devices: Vec<PeerDevice>,
    /// Global options
    #[serde(default)]
    pub options: Options,
}

impl Default for DeviceConfig {
    fn default() -> Self {
        Self {
            name: hostname::get()
                .ok()
                .and_then(|s| s.into_string().ok())
                .unwrap_or_else(|| "unknown".to_string()),
            id: None,
        }
    }
}

/// Global options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Options {
    /// Listen addresses
    #[serde(default = "default_listen")]
    pub listen_addresses: Vec<String>,
    /// Global discovery enabled
    #[serde(default = "default_true")]
    pub global_discovery: bool,
    /// Local discovery enabled
    #[serde(default = "default_true")]
    pub local_discovery: bool,
    /// NAT traversal enabled
    #[serde(default = "default_true")]
    pub nat_traversal: bool,
    /// QUIC enabled (vs TCP)
    #[serde(default = "default_true")]
    pub quic_enabled: bool,
    /// Rate limit (bytes/sec, 0 = unlimited)
    #[serde(default)]
    pub rate_limit: u64,
    /// Max concurrent outgoing connections
    #[serde(default = "default_connections")]
    pub max_connections: usize,
    /// Reconnect interval (seconds)
    #[serde(default = "default_reconnect")]
    pub reconnect_interval_secs: u64,
    /// Start minimized to tray
    #[serde(default)]
    pub start_minimized: bool,
}

fn default_listen() -> Vec<String> {
    vec!["tcp://0.0.0.0:22000".to_string()]
}

fn default_true() -> bool {
    true
}

fn default_connections() -> usize {
    10
}

fn default_reconnect() -> u64 {
    60
}

impl Default for Options {
    fn default() -> Self {
        Self {
            listen_addresses: default_listen(),
            global_discovery: true,
            local_discovery: true,
            nat_traversal: true,
            quic_enabled: true,
            rate_limit: 0,
            max_connections: default_connections(),
            reconnect_interval_secs: default_reconnect(),
            start_minimized: false,
        }
    }
}

impl Config {
    /// Create a new empty config
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Load from file
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let content = fs::read_to_string(path.as_ref())
            .map_err(ConfigError::Io)?;
        
        toml::from_str(&content)
            .map_err(|e| ConfigError::Parse(e.to_string()))
    }
    
    /// Save to file
    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), ConfigError> {
        let content = toml::to_string_pretty(self)
            .map_err(|e| ConfigError::Serialize(e.to_string()))?;
        
        // Create parent directory if needed
        if let Some(parent) = path.as_ref().parent() {
            fs::create_dir_all(parent).map_err(ConfigError::Io)?;
        }
        
        fs::write(path, content).map_err(ConfigError::Io)
    }
    
    /// Get default config path
    pub fn default_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("nexus")
            .join("config.toml")
    }
    
    /// Get default data directory
    pub fn default_data_dir() -> PathBuf {
        dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("nexus")
    }
    
    /// Add a folder
    pub fn add_folder(&mut self, folder: FolderConfig) {
        self.folders.push(folder);
    }
    
    /// Remove a folder by ID
    pub fn remove_folder(&mut self, id: &str) -> bool {
        let len = self.folders.len();
        self.folders.retain(|f| f.id != id);
        self.folders.len() < len
    }
    
    /// Get a folder by ID
    pub fn get_folder(&self, id: &str) -> Option<&FolderConfig> {
        self.folders.iter().find(|f| f.id == id)
    }
    
    /// Add a device
    pub fn add_device(&mut self, device: PeerDevice) {
        self.devices.push(device);
    }
    
    /// Remove a device by ID
    pub fn remove_device(&mut self, id: &str) -> bool {
        let len = self.devices.len();
        self.devices.retain(|d| d.id != id);
        self.devices.len() < len
    }
    
    /// Get a device by ID
    pub fn get_device(&self, id: &str) -> Option<&PeerDevice> {
        self.devices.iter().find(|d| d.id == id)
    }
}

/// Configuration error
#[derive(Debug)]
pub enum ConfigError {
    /// IO error
    Io(std::io::Error),
    /// Parse error
    Parse(String),
    /// Serialize error
    Serialize(String),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "IO error: {}", e),
            Self::Parse(e) => write!(f, "parse error: {}", e),
            Self::Serialize(e) => write!(f, "serialize error: {}", e),
        }
    }
}

impl std::error::Error for ConfigError {}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    
    #[test]
    fn test_config_new() {
        let config = Config::new();
        assert!(config.folders.is_empty());
        assert!(config.devices.is_empty());
        assert!(config.options.global_discovery);
    }
    
    #[test]
    fn test_config_add_remove_folder() {
        let mut config = Config::new();
        
        config.add_folder(FolderConfig::new("default", "/sync"));
        assert_eq!(config.folders.len(), 1);
        
        config.add_folder(FolderConfig::new("photos", "/photos"));
        assert_eq!(config.folders.len(), 2);
        
        assert!(config.remove_folder("default"));
        assert_eq!(config.folders.len(), 1);
        
        assert!(!config.remove_folder("nonexistent"));
    }
    
    #[test]
    fn test_config_save_load() {
        let tmp = TempDir::new().unwrap();
        let config_path = tmp.path().join("config.toml");
        
        let mut config = Config::new();
        config.device.name = "test-device".to_string();
        config.add_folder(FolderConfig::new("default", "/sync"));
        config.add_device(PeerDevice::new("DEVICE-1", "phone"));
        
        config.save(&config_path).unwrap();
        
        let loaded = Config::load(&config_path).unwrap();
        assert_eq!(loaded.device.name, "test-device");
        assert_eq!(loaded.folders.len(), 1);
        assert_eq!(loaded.devices.len(), 1);
    }
    
    #[test]
    fn test_options_default() {
        let options = Options::default();
        
        assert!(!options.listen_addresses.is_empty());
        assert!(options.global_discovery);
        assert!(options.local_discovery);
        assert!(options.nat_traversal);
        assert_eq!(options.rate_limit, 0);
    }
    
    #[test]
    fn test_config_get_folder() {
        let mut config = Config::new();
        config.add_folder(FolderConfig::new("docs", "/docs").with_label("Documents"));
        
        let folder = config.get_folder("docs").unwrap();
        assert_eq!(folder.display_name(), "Documents");
        
        assert!(config.get_folder("nonexistent").is_none());
    }
}
