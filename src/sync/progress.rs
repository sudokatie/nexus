//! Progress tracking for sync operations

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Sync state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SyncState {
    /// Not started
    #[default]
    Idle,
    /// Scanning local files
    Scanning,
    /// Exchanging index with peer
    Indexing,
    /// Transferring blocks
    Syncing,
    /// Sync complete
    Complete,
    /// Sync failed
    Failed,
    /// Sync paused
    Paused,
}

impl SyncState {
    /// Check if sync is active
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Scanning | Self::Indexing | Self::Syncing)
    }
    
    /// Check if sync is done (success or failure)
    pub fn is_done(&self) -> bool {
        matches!(self, Self::Complete | Self::Failed)
    }
}

/// Progress for a single sync operation
#[derive(Debug, Clone)]
pub struct SyncProgress {
    /// Current state
    pub state: SyncState,
    /// Folder being synced
    pub folder: String,
    /// Current file being processed
    pub current_file: Option<String>,
    /// Files processed
    pub files_done: u64,
    /// Total files to process
    pub files_total: u64,
    /// Bytes transferred
    pub bytes_done: u64,
    /// Total bytes to transfer
    pub bytes_total: u64,
    /// Start time
    pub started_at: Option<Instant>,
    /// End time
    pub ended_at: Option<Instant>,
    /// Error message if failed
    pub error: Option<String>,
}

impl Default for SyncProgress {
    fn default() -> Self {
        Self::new("")
    }
}

impl SyncProgress {
    /// Create new progress tracker for a folder
    pub fn new(folder: impl Into<String>) -> Self {
        Self {
            state: SyncState::Idle,
            folder: folder.into(),
            current_file: None,
            files_done: 0,
            files_total: 0,
            bytes_done: 0,
            bytes_total: 0,
            started_at: None,
            ended_at: None,
            error: None,
        }
    }
    
    /// Start sync
    pub fn start(&mut self) {
        self.state = SyncState::Scanning;
        self.started_at = Some(Instant::now());
        self.ended_at = None;
        self.error = None;
    }
    
    /// Set scanning state
    pub fn set_scanning(&mut self) {
        self.state = SyncState::Scanning;
    }
    
    /// Set indexing state
    pub fn set_indexing(&mut self) {
        self.state = SyncState::Indexing;
    }
    
    /// Set syncing state with totals
    pub fn set_syncing(&mut self, files_total: u64, bytes_total: u64) {
        self.state = SyncState::Syncing;
        self.files_total = files_total;
        self.bytes_total = bytes_total;
    }
    
    /// Update current file
    pub fn set_current_file(&mut self, file: impl Into<String>) {
        self.current_file = Some(file.into());
    }
    
    /// Record file completion
    pub fn complete_file(&mut self, bytes: u64) {
        self.files_done += 1;
        self.bytes_done += bytes;
    }
    
    /// Mark as complete
    pub fn complete(&mut self) {
        self.state = SyncState::Complete;
        self.ended_at = Some(Instant::now());
        self.current_file = None;
    }
    
    /// Mark as failed
    pub fn fail(&mut self, error: impl Into<String>) {
        self.state = SyncState::Failed;
        self.ended_at = Some(Instant::now());
        self.error = Some(error.into());
    }
    
    /// Pause sync
    pub fn pause(&mut self) {
        self.state = SyncState::Paused;
    }
    
    /// Resume sync
    pub fn resume(&mut self) {
        if self.state == SyncState::Paused {
            self.state = SyncState::Syncing;
        }
    }
    
    /// Get completion percentage (0-100)
    pub fn percent(&self) -> f64 {
        if self.bytes_total == 0 {
            if self.state == SyncState::Complete {
                100.0
            } else {
                0.0
            }
        } else {
            (self.bytes_done as f64 / self.bytes_total as f64) * 100.0
        }
    }
    
    /// Elapsed time
    pub fn elapsed(&self) -> Duration {
        match (self.started_at, self.ended_at) {
            (Some(start), Some(end)) => end.duration_since(start),
            (Some(start), None) => start.elapsed(),
            _ => Duration::ZERO,
        }
    }
    
    /// Bytes per second
    pub fn bytes_per_second(&self) -> f64 {
        let secs = self.elapsed().as_secs_f64();
        if secs > 0.0 {
            self.bytes_done as f64 / secs
        } else {
            0.0
        }
    }
    
    /// Estimated time remaining
    pub fn eta(&self) -> Option<Duration> {
        if self.bytes_done == 0 || self.bytes_total == 0 {
            return None;
        }
        
        let bps = self.bytes_per_second();
        if bps <= 0.0 {
            return None;
        }
        
        let remaining_bytes = self.bytes_total.saturating_sub(self.bytes_done);
        let remaining_secs = remaining_bytes as f64 / bps;
        
        Some(Duration::from_secs_f64(remaining_secs))
    }
    
    /// Format progress as string
    pub fn format(&self) -> String {
        match self.state {
            SyncState::Idle => "Idle".to_string(),
            SyncState::Scanning => "Scanning...".to_string(),
            SyncState::Indexing => "Exchanging index...".to_string(),
            SyncState::Syncing => {
                let pct = self.percent();
                let bps = self.bytes_per_second();
                let eta = self.eta()
                    .map(|d| format!(" ETA: {}s", d.as_secs()))
                    .unwrap_or_default();
                format!(
                    "{:.1}% ({}/{} files, {}/{}){} @ {:.1} KB/s",
                    pct,
                    self.files_done,
                    self.files_total,
                    format_bytes(self.bytes_done),
                    format_bytes(self.bytes_total),
                    eta,
                    bps / 1024.0
                )
            }
            SyncState::Complete => {
                let elapsed = self.elapsed();
                format!(
                    "Complete: {} files, {} in {:.1}s",
                    self.files_done,
                    format_bytes(self.bytes_done),
                    elapsed.as_secs_f64()
                )
            }
            SyncState::Failed => {
                format!("Failed: {}", self.error.as_deref().unwrap_or("unknown error"))
            }
            SyncState::Paused => "Paused".to_string(),
        }
    }
}

/// Format bytes as human readable
fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    
    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Progress tracker for multiple folders
#[derive(Debug, Default)]
pub struct ProgressTracker {
    /// Progress per folder
    folders: Mutex<Vec<SyncProgress>>,
}

impl ProgressTracker {
    /// Create new tracker
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Add a folder to track
    pub fn add_folder(&self, folder: impl Into<String>) -> usize {
        let mut folders = self.folders.lock().unwrap();
        let id = folders.len();
        folders.push(SyncProgress::new(folder));
        id
    }
    
    /// Get progress for a folder by index
    pub fn get(&self, id: usize) -> Option<SyncProgress> {
        let folders = self.folders.lock().unwrap();
        folders.get(id).cloned()
    }
    
    /// Update progress for a folder
    pub fn update<F>(&self, id: usize, f: F)
    where
        F: FnOnce(&mut SyncProgress),
    {
        let mut folders = self.folders.lock().unwrap();
        if let Some(progress) = folders.get_mut(id) {
            f(progress);
        }
    }
    
    /// Get all progress
    pub fn all(&self) -> Vec<SyncProgress> {
        self.folders.lock().unwrap().clone()
    }
    
    /// Overall completion percentage
    pub fn overall_percent(&self) -> f64 {
        let folders = self.folders.lock().unwrap();
        if folders.is_empty() {
            return 0.0;
        }
        
        let total_bytes: u64 = folders.iter().map(|p| p.bytes_total).sum();
        let done_bytes: u64 = folders.iter().map(|p| p.bytes_done).sum();
        
        if total_bytes == 0 {
            0.0
        } else {
            (done_bytes as f64 / total_bytes as f64) * 100.0
        }
    }
    
    /// Check if all syncs are complete
    pub fn all_complete(&self) -> bool {
        let folders = self.folders.lock().unwrap();
        !folders.is_empty() && folders.iter().all(|p| p.state == SyncState::Complete)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_sync_state() {
        assert!(SyncState::Scanning.is_active());
        assert!(SyncState::Syncing.is_active());
        assert!(!SyncState::Idle.is_active());
        
        assert!(SyncState::Complete.is_done());
        assert!(SyncState::Failed.is_done());
        assert!(!SyncState::Syncing.is_done());
    }
    
    #[test]
    fn test_sync_progress_lifecycle() {
        let mut progress = SyncProgress::new("test-folder");
        assert_eq!(progress.state, SyncState::Idle);
        
        progress.start();
        assert_eq!(progress.state, SyncState::Scanning);
        
        progress.set_syncing(10, 10000);
        assert_eq!(progress.files_total, 10);
        assert_eq!(progress.bytes_total, 10000);
        
        progress.complete_file(1000);
        assert_eq!(progress.files_done, 1);
        assert_eq!(progress.bytes_done, 1000);
        assert_eq!(progress.percent(), 10.0);
        
        progress.complete();
        assert_eq!(progress.state, SyncState::Complete);
    }
    
    #[test]
    fn test_sync_progress_failure() {
        let mut progress = SyncProgress::new("test");
        progress.start();
        progress.fail("network error");
        
        assert_eq!(progress.state, SyncState::Failed);
        assert_eq!(progress.error, Some("network error".to_string()));
    }
    
    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(500), "500 B");
        assert_eq!(format_bytes(1500), "1.5 KB");
        assert_eq!(format_bytes(1500000), "1.4 MB");
        assert_eq!(format_bytes(1500000000), "1.4 GB");
    }
    
    #[test]
    fn test_progress_tracker() {
        let tracker = ProgressTracker::new();
        
        let id1 = tracker.add_folder("folder1");
        let id2 = tracker.add_folder("folder2");
        
        tracker.update(id1, |p| {
            p.start();
            p.set_syncing(5, 5000);
            p.complete_file(2500);
        });
        
        tracker.update(id2, |p| {
            p.start();
            p.set_syncing(5, 5000);
            p.complete_file(5000);
            p.complete();
        });
        
        let p1 = tracker.get(id1).unwrap();
        assert_eq!(p1.percent(), 50.0);
        
        let p2 = tracker.get(id2).unwrap();
        assert_eq!(p2.state, SyncState::Complete);
        
        // Overall: 7500 / 10000 = 75%
        assert_eq!(tracker.overall_percent(), 75.0);
    }
}
