//! Device configuration

use crate::crypto::{DeviceId, DeviceKey};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Device configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceConfig {
    /// Device name (human-readable)
    pub name: String,
    /// Device ID (derived from key, stored for reference)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

impl DeviceConfig {
    /// Create a new device config
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            id: None,
        }
    }
    
    /// Create with device ID
    pub fn with_id(name: impl Into<String>, device_id: &DeviceId) -> Self {
        Self {
            name: name.into(),
            id: Some(device_id.to_display()),
        }
    }
    
    /// Get device ID if set
    pub fn device_id(&self) -> Option<DeviceId> {
        self.id.as_ref().and_then(|s| DeviceId::from_display(s).ok())
    }
}

/// Peer device entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerDevice {
    /// Device ID
    pub id: String,
    /// Device name
    pub name: String,
    /// Addresses to connect to
    #[serde(default)]
    pub addresses: Vec<String>,
    /// Auto-accept connections
    #[serde(default = "default_true")]
    pub auto_accept: bool,
    /// Compression enabled
    #[serde(default = "default_true")]
    pub compression: bool,
    /// Introducer (share other devices)
    #[serde(default)]
    pub introducer: bool,
}

fn default_true() -> bool {
    true
}

impl PeerDevice {
    /// Create a new peer device entry
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            addresses: Vec::new(),
            auto_accept: true,
            compression: true,
            introducer: false,
        }
    }
    
    /// Add an address
    pub fn with_address(mut self, addr: impl Into<String>) -> Self {
        self.addresses.push(addr.into());
        self
    }
    
    /// Get device ID
    pub fn device_id(&self) -> Option<DeviceId> {
        DeviceId::from_display(&self.id).ok()
    }
}

/// Device key management
pub struct DeviceKeyManager {
    /// Key file path
    key_path: PathBuf,
    /// Loaded key
    key: Option<DeviceKey>,
}

impl DeviceKeyManager {
    /// Create a new key manager
    pub fn new(key_path: impl Into<PathBuf>) -> Self {
        Self {
            key_path: key_path.into(),
            key: None,
        }
    }
    
    /// Generate a new key
    pub fn generate(&mut self) -> Result<&DeviceKey, DeviceKeyError> {
        let key = DeviceKey::generate()
            .map_err(|_| DeviceKeyError::Generation)?;
        
        // Save to file
        key.save(&self.key_path)
            .map_err(DeviceKeyError::Io)?;
        
        self.key = Some(key);
        Ok(self.key.as_ref().unwrap())
    }
    
    /// Load existing key
    pub fn load(&mut self) -> Result<&DeviceKey, DeviceKeyError> {
        let key = DeviceKey::load(&self.key_path)
            .map_err(|e| match e {
                crate::crypto::DeviceKeyError::Io(io) => DeviceKeyError::Io(io),
                crate::crypto::DeviceKeyError::Key(_) => DeviceKeyError::Invalid,
            })?;
        
        self.key = Some(key);
        Ok(self.key.as_ref().unwrap())
    }
    
    /// Load or generate key
    pub fn load_or_generate(&mut self) -> Result<&DeviceKey, DeviceKeyError> {
        if self.key_path.exists() {
            self.load()
        } else {
            self.generate()
        }
    }
    
    /// Get the loaded key
    pub fn key(&self) -> Option<&DeviceKey> {
        self.key.as_ref()
    }
    
    /// Get device ID
    pub fn device_id(&self) -> Option<DeviceId> {
        self.key.as_ref().map(|k| k.device_id())
    }
    
    /// Check if key exists
    pub fn exists(&self) -> bool {
        self.key_path.exists()
    }
}

/// Device key error
#[derive(Debug)]
pub enum DeviceKeyError {
    /// IO error
    Io(std::io::Error),
    /// Key generation failed
    Generation,
    /// Invalid key format
    Invalid,
}

impl std::fmt::Display for DeviceKeyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "IO error: {}", e),
            Self::Generation => write!(f, "key generation failed"),
            Self::Invalid => write!(f, "invalid key format"),
        }
    }
}

impl std::error::Error for DeviceKeyError {}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    
    #[test]
    fn test_device_config_new() {
        let config = DeviceConfig::new("my-laptop");
        assert_eq!(config.name, "my-laptop");
        assert!(config.id.is_none());
    }
    
    #[test]
    fn test_device_config_with_id() {
        let device_id = DeviceId::from_bytes([1u8; 32]);
        let config = DeviceConfig::with_id("laptop", &device_id);
        
        assert_eq!(config.name, "laptop");
        assert!(config.id.is_some());
        assert_eq!(config.device_id().unwrap(), device_id);
    }
    
    #[test]
    fn test_peer_device() {
        let peer = PeerDevice::new(
            "ABCDEF1-ABCDEF1-ABCDEF1-ABCDEF1-ABCDEF1-ABCDEF1-ABCD",
            "friend-phone"
        ).with_address("tcp://192.168.1.50:22000");
        
        assert_eq!(peer.name, "friend-phone");
        assert_eq!(peer.addresses.len(), 1);
        assert!(peer.auto_accept);
        assert!(peer.compression);
    }
    
    #[test]
    fn test_device_key_manager_generate() {
        let tmp = TempDir::new().unwrap();
        let key_path = tmp.path().join("device.key");
        
        let mut manager = DeviceKeyManager::new(&key_path);
        assert!(!manager.exists());
        
        manager.generate().unwrap();
        assert!(manager.exists());
        assert!(manager.device_id().is_some());
    }
    
    #[test]
    fn test_device_key_manager_load() {
        let tmp = TempDir::new().unwrap();
        let key_path = tmp.path().join("device.key");
        
        // Generate first
        let mut manager1 = DeviceKeyManager::new(&key_path);
        manager1.generate().unwrap();
        let id1 = manager1.device_id().unwrap();
        
        // Load in new manager
        let mut manager2 = DeviceKeyManager::new(&key_path);
        manager2.load().unwrap();
        let id2 = manager2.device_id().unwrap();
        
        assert_eq!(id1, id2);
    }
}
