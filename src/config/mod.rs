//! Configuration management

pub mod device;
pub mod folder;
pub mod settings;

pub use device::{DeviceConfig, DeviceKeyManager, DeviceKeyError, PeerDevice};
pub use folder::{FolderConfig, FolderType, VersioningConfig, VersioningType};
pub use settings::{Config, ConfigError, Options};
