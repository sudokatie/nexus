//! Nexus CLI - P2P file synchronization

use clap::{Parser, Subcommand};
use nexus::cli;

#[derive(Parser)]
#[command(name = "nexus")]
#[command(about = "Peer-to-peer file synchronization")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize device with new identity
    Init {
        /// Device name
        #[arg(long)]
        name: Option<String>,
    },
    
    /// Add folder to sync
    Add {
        /// Path to folder
        path: String,
        
        /// Folder ID (generated if not provided)
        #[arg(long)]
        id: Option<String>,
        
        /// Folder type (send-receive, send-only, receive-only)
        #[arg(long, value_name = "TYPE")]
        folder_type: Option<String>,
    },
    
    /// Remove folder from sync
    Remove {
        /// Folder ID
        folder_id: String,
    },
    
    /// List folders
    Folders,
    
    /// Device management
    Device {
        #[command(subcommand)]
        action: DeviceAction,
    },
    
    /// Show sync status
    Status,
    
    /// Force sync now
    Sync,
    
    /// Run sync daemon
    Serve {
        /// Listen address
        #[arg(long, default_value = "0.0.0.0:22000")]
        listen: String,
    },
}

#[derive(Subcommand)]
enum DeviceAction {
    /// List known devices
    List,
    
    /// Add device by ID
    Add {
        /// Device ID
        device_id: String,
        
        /// Device name
        #[arg(long)]
        name: Option<String>,
    },
    
    /// Remove device
    Remove {
        /// Device ID
        device_id: String,
    },
    
    /// Show this device's ID
    Id,
}

fn main() {
    let cli = Cli::parse();
    
    let result = match cli.command {
        Commands::Init { name } => {
            cli::init::run(name, None)
                .map_err(|e| e.to_string())
        }
        
        Commands::Add { path, id, folder_type } => {
            cli::folder::add(path, id, folder_type)
                .map_err(|e| e.to_string())
        }
        
        Commands::Remove { folder_id } => {
            cli::folder::remove(folder_id)
                .map_err(|e| e.to_string())
        }
        
        Commands::Folders => {
            cli::folder::list()
                .map_err(|e| e.to_string())
        }
        
        Commands::Device { action } => match action {
            DeviceAction::List => {
                cli::device::list()
                    .map_err(|e| e.to_string())
            }
            DeviceAction::Add { device_id, name } => {
                cli::device::add(device_id, name)
                    .map_err(|e| e.to_string())
            }
            DeviceAction::Remove { device_id } => {
                cli::device::remove(device_id)
                    .map_err(|e| e.to_string())
            }
            DeviceAction::Id => {
                cli::device::show_id()
                    .map_err(|e| e.to_string())
            }
        },
        
        Commands::Status => {
            cli::status::show()
                .map_err(|e| e.to_string())
        }
        
        Commands::Sync => {
            println!("Forcing sync...");
            println!("Full sync requires daemon to be running.");
            Ok(())
        }
        
        Commands::Serve { listen } => {
            cli::serve::run(listen)
                .map_err(|e| e.to_string())
        }
    };
    
    if let Err(e) = result {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}
