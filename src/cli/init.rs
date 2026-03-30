//! Device initialization command

use crate::config::{Config, DeviceConfig, DeviceKeyManager};
use std::path::PathBuf;

/// Initialize a new device
pub fn run(name: Option<String>, config_path: Option<PathBuf>) -> Result<(), InitError> {
    let config_path = config_path.unwrap_or_else(Config::default_path);
    let data_dir = Config::default_data_dir();
    let key_path = data_dir.join("device.key");
    
    // Check if already initialized
    if key_path.exists() {
        return Err(InitError::AlreadyInitialized);
    }
    
    // Create data directory
    std::fs::create_dir_all(&data_dir)
        .map_err(|e| InitError::Io(e.to_string()))?;
    
    // Generate device key
    let mut key_manager = DeviceKeyManager::new(&key_path);
    key_manager.generate()
        .map_err(|e| InitError::KeyGeneration(e.to_string()))?;
    
    let device_id = key_manager.device_id().unwrap();
    
    // Determine device name
    let device_name = name.unwrap_or_else(|| {
        hostname::get()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|_| "nexus-device".to_string())
    });
    
    // Create config
    let config = Config {
        device: DeviceConfig::with_id(&device_name, &device_id),
        ..Default::default()
    };
    
    // Save config
    config.save(&config_path)
        .map_err(|e| InitError::Config(e.to_string()))?;
    
    println!("Device initialized successfully!");
    println!();
    println!("Device ID: {}", device_id);
    println!("Device name: {}", device_name);
    println!();
    println!("Share your Device ID with others to connect.");
    println!("Config saved to: {}", config_path.display());
    
    Ok(())
}

/// Get device ID from existing initialization
pub fn get_device_id() -> Result<String, InitError> {
    let data_dir = Config::default_data_dir();
    let key_path = data_dir.join("device.key");
    
    if !key_path.exists() {
        return Err(InitError::NotInitialized);
    }
    
    let mut key_manager = DeviceKeyManager::new(&key_path);
    key_manager.load()
        .map_err(|e| InitError::KeyGeneration(e.to_string()))?;
    
    Ok(key_manager.device_id().unwrap().to_display())
}

/// Initialization error
#[derive(Debug)]
pub enum InitError {
    /// Already initialized
    AlreadyInitialized,
    /// Not initialized
    NotInitialized,
    /// IO error
    Io(String),
    /// Key generation error
    KeyGeneration(String),
    /// Config error
    Config(String),
}

impl std::fmt::Display for InitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyInitialized => write!(f, "device already initialized"),
            Self::NotInitialized => write!(f, "device not initialized (run 'nexus init' first)"),
            Self::Io(e) => write!(f, "IO error: {}", e),
            Self::KeyGeneration(e) => write!(f, "key generation failed: {}", e),
            Self::Config(e) => write!(f, "config error: {}", e),
        }
    }
}

impl std::error::Error for InitError {}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    
    #[test]
    fn test_get_device_id_not_initialized() {
        // This will fail because no device is initialized in test environment
        let result = get_device_id();
        // Just verify it returns an error, not a panic
        assert!(result.is_err());
    }
}
