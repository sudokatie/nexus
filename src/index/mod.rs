//! Indexing and file tracking

pub mod entry;
pub mod folder;
pub mod scanner;
pub mod watcher;

pub use entry::FileEntry;
pub use folder::{FolderId, FolderIndex};
pub use scanner::{ScanConfig, ScanResult, Scanner};
pub use watcher::{FileWatcher, FsEvent, WatcherConfig};
