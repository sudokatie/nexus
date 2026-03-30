//! Status display command

use crate::config::Config;
use crate::sync::SyncState;

/// Show sync status
pub fn show() -> Result<(), StatusError> {
    let config_path = Config::default_path();
    
    let config = Config::load(&config_path)
        .map_err(|e| StatusError::Config(e.to_string()))?;
    
    // Check device initialization
    if config.device.id.is_none() {
        println!("Status: Not initialized");
        println!();
        println!("Run 'nexus init' to set up this device.");
        return Ok(());
    }
    
    println!("Nexus Status");
    println!("=============");
    println!();
    
    // Device info
    println!("Device: {}", config.device.name);
    if let Some(id) = &config.device.id {
        println!("ID: {}", id);
    }
    println!();
    
    // Folders
    if config.folders.is_empty() {
        println!("Folders: none configured");
        println!("  Run 'nexus add <path>' to add a folder.");
    } else {
        println!("Folders:");
        for folder in &config.folders {
            let status = if folder.paused {
                "paused"
            } else if !folder.path_exists() {
                "missing"
            } else {
                "idle"
            };
            
            println!("  {} ({})", folder.display_name(), status);
            println!("    Path: {}", folder.path.display());
            
            if !folder.devices.is_empty() {
                println!("    Shared with: {} device(s)", folder.devices.len());
            } else {
                println!("    Not shared with any devices");
            }
        }
    }
    println!();
    
    // Devices
    if config.devices.is_empty() {
        println!("Devices: none configured");
        println!("  Run 'nexus device add <id>' to add a device.");
    } else {
        println!("Connected devices:");
        for device in &config.devices {
            // In a real implementation, we'd check actual connection status
            println!("  {} - disconnected", device.name);
        }
    }
    println!();
    
    // Options
    println!("Options:");
    println!("  Global discovery: {}", if config.options.global_discovery { "enabled" } else { "disabled" });
    println!("  Local discovery: {}", if config.options.local_discovery { "enabled" } else { "disabled" });
    println!("  NAT traversal: {}", if config.options.nat_traversal { "enabled" } else { "disabled" });
    if config.options.rate_limit > 0 {
        println!("  Rate limit: {} KB/s", config.options.rate_limit / 1024);
    }
    
    Ok(())
}

/// Status error
#[derive(Debug)]
pub enum StatusError {
    /// Config error
    Config(String),
}

impl std::fmt::Display for StatusError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Config(e) => write!(f, "config error: {}", e),
        }
    }
}

impl std::error::Error for StatusError {}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_status_error_display() {
        let err = StatusError::Config("file not found".to_string());
        assert!(err.to_string().contains("config error"));
    }
}
