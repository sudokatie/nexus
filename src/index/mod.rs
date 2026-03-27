//! Indexing and file tracking

pub mod entry;
pub mod folder;
pub mod scanner;

pub use entry::FileEntry;
pub use folder::{FolderId, FolderIndex};
pub use scanner::{ScanConfig, ScanResult, Scanner};
