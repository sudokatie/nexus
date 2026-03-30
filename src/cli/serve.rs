//! Daemon/serve command

use crate::config::Config;
use crate::crypto::{DeviceKey, DeviceId};
use crate::sync::{SyncConfig, SyncEngine};
use std::path::PathBuf;

/// Run the sync daemon
pub fn run(listen: String) -> Result<(), ServeError> {
    let config_path = Config::default_path();
    let data_dir = Config::default_data_dir();
    let key_path = data_dir.join("device.key");
    
    // Load config
    let config = Config::load(&config_path)
        .map_err(|e| ServeError::Config(e.to_string()))?;
    
    // Load device key
    if !key_path.exists() {
        return Err(ServeError::NotInitialized);
    }
    
    let device_key = DeviceKey::load(&key_path)
        .map_err(|e| ServeError::Key(e.to_string()))?;
    
    let device_id = device_key.device_id();
    
    println!("Starting Nexus daemon");
    println!("  Device: {} ({})", config.device.name, device_id.short());
    println!("  Listen: {}", listen);
    println!();
    
    // Create sync engine
    let sync_config = SyncConfig {
        rate_limit: config.options.rate_limit,
        ..Default::default()
    };
    
    let mut engine = SyncEngine::with_config(
        device_id,
        &config.device.name,
        sync_config,
    );
    
    // Add configured folders
    for folder in &config.folders {
        if folder.paused {
            println!("  Skipping paused folder: {}", folder.id);
            continue;
        }
        
        if !folder.path_exists() {
            println!("  Warning: folder path missing: {}", folder.path.display());
            continue;
        }
        
        engine.add_folder(&folder.id, &folder.path);
        println!("  Added folder: {} ({})", folder.display_name(), folder.path.display());
        
        // Add peer devices for this folder
        for device_id_str in &folder.devices {
            if let Some(peer) = config.get_device(device_id_str) {
                if let Some(peer_id) = peer.device_id() {
                    engine.add_peer(&folder.id, peer_id);
                }
            }
        }
    }
    
    println!();
    println!("Daemon running. Press Ctrl+C to stop.");
    
    // Start engine
    engine.start();
    
    // In a real implementation, we would:
    // 1. Start listening on the specified address
    // 2. Start discovery services
    // 3. Connect to known peers
    // 4. Run the sync loop
    
    // For now, just indicate we would run
    println!();
    println!("Note: Full daemon implementation requires async runtime.");
    println!("This is a placeholder showing configuration was loaded.");
    
    Ok(())
}

/// Serve error
#[derive(Debug)]
pub enum ServeError {
    /// Config error
    Config(String),
    /// Key error
    Key(String),
    /// Not initialized
    NotInitialized,
    /// Network error
    Network(String),
}

impl std::fmt::Display for ServeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Config(e) => write!(f, "config error: {}", e),
            Self::Key(e) => write!(f, "key error: {}", e),
            Self::NotInitialized => write!(f, "device not initialized (run 'nexus init' first)"),
            Self::Network(e) => write!(f, "network error: {}", e),
        }
    }
}

impl std::error::Error for ServeError {}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_serve_error_display() {
        let err = ServeError::NotInitialized;
        assert!(err.to_string().contains("not initialized"));
    }
}
