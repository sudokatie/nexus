//! Folder management commands

use crate::config::{Config, FolderConfig, FolderType};
use std::path::PathBuf;

/// Add a folder to sync
pub fn add(
    path: String,
    id: Option<String>,
    folder_type: Option<String>,
) -> Result<(), FolderError> {
    let config_path = Config::default_path();
    
    // Load existing config
    let mut config = Config::load(&config_path)
        .map_err(|e| FolderError::Config(e.to_string()))?;
    
    // Resolve path
    let abs_path = std::fs::canonicalize(&path)
        .map_err(|e| FolderError::InvalidPath(e.to_string()))?;
    
    // Generate folder ID if not provided
    let folder_id = id.unwrap_or_else(|| {
        abs_path.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "sync".to_string())
    });
    
    // Check if folder ID already exists
    if config.get_folder(&folder_id).is_some() {
        return Err(FolderError::AlreadyExists(folder_id));
    }
    
    // Parse folder type
    let ft = match folder_type.as_deref() {
        Some("send-only") => FolderType::SendOnly,
        Some("receive-only") => FolderType::ReceiveOnly,
        Some("send-receive") | None => FolderType::SendReceive,
        Some(other) => return Err(FolderError::InvalidType(other.to_string())),
    };
    
    // Create folder config
    let folder = FolderConfig::new(&folder_id, &abs_path)
        .with_type(ft);
    
    config.add_folder(folder);
    
    // Save config
    config.save(&config_path)
        .map_err(|e| FolderError::Config(e.to_string()))?;
    
    println!("Added folder:");
    println!("  ID: {}", folder_id);
    println!("  Path: {}", abs_path.display());
    println!("  Type: {:?}", ft);
    
    Ok(())
}

/// Remove a folder from sync
pub fn remove(folder_id: String) -> Result<(), FolderError> {
    let config_path = Config::default_path();
    
    // Load existing config
    let mut config = Config::load(&config_path)
        .map_err(|e| FolderError::Config(e.to_string()))?;
    
    // Remove folder
    if !config.remove_folder(&folder_id) {
        return Err(FolderError::NotFound(folder_id));
    }
    
    // Save config
    config.save(&config_path)
        .map_err(|e| FolderError::Config(e.to_string()))?;
    
    println!("Removed folder: {}", folder_id);
    
    Ok(())
}

/// List all folders
pub fn list() -> Result<(), FolderError> {
    let config_path = Config::default_path();
    
    let config = Config::load(&config_path)
        .map_err(|e| FolderError::Config(e.to_string()))?;
    
    if config.folders.is_empty() {
        println!("No folders configured");
        return Ok(());
    }
    
    println!("Configured folders:");
    for folder in &config.folders {
        let status = if folder.path_exists() { "ok" } else { "missing" };
        println!();
        println!("  {} [{}]", folder.display_name(), status);
        println!("    ID: {}", folder.id);
        println!("    Path: {}", folder.path.display());
        println!("    Type: {:?}", folder.folder_type);
        if folder.paused {
            println!("    Status: paused");
        }
        if !folder.devices.is_empty() {
            println!("    Shared with: {} device(s)", folder.devices.len());
        }
    }
    
    Ok(())
}

/// Folder error
#[derive(Debug)]
pub enum FolderError {
    /// Config error
    Config(String),
    /// Invalid path
    InvalidPath(String),
    /// Folder already exists
    AlreadyExists(String),
    /// Folder not found
    NotFound(String),
    /// Invalid folder type
    InvalidType(String),
}

impl std::fmt::Display for FolderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Config(e) => write!(f, "config error: {}", e),
            Self::InvalidPath(e) => write!(f, "invalid path: {}", e),
            Self::AlreadyExists(id) => write!(f, "folder '{}' already exists", id),
            Self::NotFound(id) => write!(f, "folder '{}' not found", id),
            Self::InvalidType(t) => write!(f, "invalid folder type: {} (use send-only, receive-only, or send-receive)", t),
        }
    }
}

impl std::error::Error for FolderError {}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_folder_error_display() {
        let err = FolderError::NotFound("test".to_string());
        assert!(err.to_string().contains("not found"));
        
        let err = FolderError::AlreadyExists("docs".to_string());
        assert!(err.to_string().contains("already exists"));
    }
}
