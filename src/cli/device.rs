//! Device management commands

use crate::config::{Config, PeerDevice};

/// List known devices
pub fn list() -> Result<(), DeviceError> {
    let config_path = Config::default_path();
    
    let config = Config::load(&config_path)
        .map_err(|e| DeviceError::Config(e.to_string()))?;
    
    if config.devices.is_empty() {
        println!("No devices configured");
        return Ok(());
    }
    
    println!("Known devices:");
    for device in &config.devices {
        println!();
        println!("  {} [{}]", device.name, if device.auto_accept { "auto" } else { "manual" });
        println!("    ID: {}", device.id);
        if !device.addresses.is_empty() {
            println!("    Addresses: {}", device.addresses.join(", "));
        }
        if device.introducer {
            println!("    Introducer: yes");
        }
    }
    
    Ok(())
}

/// Add a device
pub fn add(device_id: String, name: Option<String>) -> Result<(), DeviceError> {
    let config_path = Config::default_path();
    
    // Load existing config
    let mut config = Config::load(&config_path)
        .map_err(|e| DeviceError::Config(e.to_string()))?;
    
    // Check if already exists
    if config.get_device(&device_id).is_some() {
        return Err(DeviceError::AlreadyExists(device_id));
    }
    
    // Create device entry
    let device_name = name.unwrap_or_else(|| format!("Device {}", &device_id[..7]));
    let device = PeerDevice::new(&device_id, &device_name);
    
    config.add_device(device);
    
    // Save config
    config.save(&config_path)
        .map_err(|e| DeviceError::Config(e.to_string()))?;
    
    println!("Added device:");
    println!("  ID: {}", device_id);
    println!("  Name: {}", device_name);
    
    Ok(())
}

/// Remove a device
pub fn remove(device_id: String) -> Result<(), DeviceError> {
    let config_path = Config::default_path();
    
    // Load existing config
    let mut config = Config::load(&config_path)
        .map_err(|e| DeviceError::Config(e.to_string()))?;
    
    // Remove device
    if !config.remove_device(&device_id) {
        return Err(DeviceError::NotFound(device_id));
    }
    
    // Also remove from any folder sharing lists
    for folder in &mut config.folders {
        folder.devices.retain(|d| d != &device_id);
    }
    
    // Save config
    config.save(&config_path)
        .map_err(|e| DeviceError::Config(e.to_string()))?;
    
    println!("Removed device: {}", device_id);
    
    Ok(())
}

/// Show this device's ID
pub fn show_id() -> Result<(), DeviceError> {
    let config_path = Config::default_path();
    
    let config = Config::load(&config_path)
        .map_err(|e| DeviceError::Config(e.to_string()))?;
    
    match &config.device.id {
        Some(id) => {
            println!("Device ID: {}", id);
            println!("Device name: {}", config.device.name);
        }
        None => {
            return Err(DeviceError::NotInitialized);
        }
    }
    
    Ok(())
}

/// Device error
#[derive(Debug)]
pub enum DeviceError {
    /// Config error
    Config(String),
    /// Device not initialized
    NotInitialized,
    /// Device already exists
    AlreadyExists(String),
    /// Device not found
    NotFound(String),
}

impl std::fmt::Display for DeviceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Config(e) => write!(f, "config error: {}", e),
            Self::NotInitialized => write!(f, "device not initialized (run 'nexus init' first)"),
            Self::AlreadyExists(id) => write!(f, "device '{}' already exists", id),
            Self::NotFound(id) => write!(f, "device '{}' not found", id),
        }
    }
}

impl std::error::Error for DeviceError {}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_device_error_display() {
        let err = DeviceError::NotFound("ABC123".to_string());
        assert!(err.to_string().contains("not found"));
        
        let err = DeviceError::NotInitialized;
        assert!(err.to_string().contains("not initialized"));
    }
}
