//! Filesystem watcher for real-time change detection

use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher as NotifyWatcher};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

/// Filesystem events
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FsEvent {
    /// File or directory created
    Create(PathBuf),
    /// File or directory modified
    Modify(PathBuf),
    /// File or directory deleted
    Delete(PathBuf),
    /// File or directory renamed (from, to)
    Rename(PathBuf, PathBuf),
}

impl FsEvent {
    /// Get the primary path affected
    pub fn path(&self) -> &Path {
        match self {
            FsEvent::Create(p) | FsEvent::Modify(p) | FsEvent::Delete(p) => p,
            FsEvent::Rename(_, to) => to,
        }
    }
    
    /// Check if this event affects the given path
    pub fn affects(&self, path: &Path) -> bool {
        match self {
            FsEvent::Create(p) | FsEvent::Modify(p) | FsEvent::Delete(p) => p == path,
            FsEvent::Rename(from, to) => from == path || to == path,
        }
    }
}

/// Configuration for the watcher
#[derive(Debug, Clone)]
pub struct WatcherConfig {
    /// Debounce duration for coalescing events
    pub debounce_ms: u64,
    /// Watch subdirectories recursively
    pub recursive: bool,
}

impl Default for WatcherConfig {
    fn default() -> Self {
        Self {
            debounce_ms: 100,
            recursive: true,
        }
    }
}

/// Filesystem watcher with debouncing
pub struct FileWatcher {
    /// The underlying notify watcher
    _watcher: RecommendedWatcher,
    /// Receiver for raw events
    raw_rx: Receiver<Result<Event, notify::Error>>,
    /// Pending events for debouncing
    pending: HashMap<PathBuf, (FsEvent, Instant)>,
    /// Configuration
    config: WatcherConfig,
}

impl FileWatcher {
    /// Create a new watcher for the given path
    pub fn new(path: impl AsRef<Path>) -> Result<Self, notify::Error> {
        Self::with_config(path, WatcherConfig::default())
    }
    
    /// Create with custom configuration
    pub fn with_config(path: impl AsRef<Path>, config: WatcherConfig) -> Result<Self, notify::Error> {
        let (tx, rx) = mpsc::channel();
        
        let mut watcher = RecommendedWatcher::new(
            move |res| {
                let _ = tx.send(res);
            },
            Config::default(),
        )?;
        
        let mode = if config.recursive {
            RecursiveMode::Recursive
        } else {
            RecursiveMode::NonRecursive
        };
        
        watcher.watch(path.as_ref(), mode)?;
        
        Ok(Self {
            _watcher: watcher,
            raw_rx: rx,
            pending: HashMap::new(),
            config,
        })
    }
    
    /// Poll for the next event (non-blocking)
    pub fn poll(&mut self) -> Option<FsEvent> {
        // Process any raw events
        while let Ok(result) = self.raw_rx.try_recv() {
            if let Ok(event) = result {
                self.process_raw_event(event);
            }
        }
        
        // Check for debounced events ready to emit
        let debounce = Duration::from_millis(self.config.debounce_ms);
        let now = Instant::now();
        
        let ready: Vec<_> = self.pending
            .iter()
            .filter(|(_, (_, time))| now.duration_since(*time) >= debounce)
            .map(|(path, _)| path.clone())
            .collect();
        
        if let Some(path) = ready.into_iter().next() {
            return self.pending.remove(&path).map(|(event, _)| event);
        }
        
        None
    }
    
    /// Wait for the next event (blocking with timeout)
    pub fn wait(&mut self, timeout: Duration) -> Option<FsEvent> {
        let start = Instant::now();
        
        loop {
            if let Some(event) = self.poll() {
                return Some(event);
            }
            
            if start.elapsed() >= timeout {
                return None;
            }
            
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    
    /// Drain all pending events
    pub fn drain(&mut self) -> Vec<FsEvent> {
        // Process any remaining raw events
        while let Ok(result) = self.raw_rx.try_recv() {
            if let Ok(event) = result {
                self.process_raw_event(event);
            }
        }
        
        // Return all pending events
        self.pending.drain().map(|(_, (event, _))| event).collect()
    }
    
    fn process_raw_event(&mut self, event: Event) {
        let now = Instant::now();
        
        for path in event.paths {
            let fs_event = match event.kind {
                EventKind::Create(_) => FsEvent::Create(path.clone()),
                EventKind::Modify(_) => FsEvent::Modify(path.clone()),
                EventKind::Remove(_) => FsEvent::Delete(path.clone()),
                _ => continue,
            };
            
            // Debounce: update or insert the event
            self.pending.insert(path, (fs_event, now));
        }
    }
}

/// Simple event receiver without debouncing
pub struct EventReceiver {
    rx: Receiver<FsEvent>,
}

impl EventReceiver {
    /// Receive the next event (blocking)
    pub fn recv(&self) -> Result<FsEvent, mpsc::RecvError> {
        self.rx.recv()
    }
    
    /// Try to receive without blocking
    pub fn try_recv(&self) -> Option<FsEvent> {
        self.rx.try_recv().ok()
    }
}

/// Create a simple watcher that sends events to a channel
pub fn watch_channel(path: impl AsRef<Path>) -> Result<(RecommendedWatcher, EventReceiver), notify::Error> {
    let (tx, rx) = mpsc::channel();
    
    let sender: Sender<FsEvent> = tx;
    let mut watcher = RecommendedWatcher::new(
        move |res: Result<Event, notify::Error>| {
            if let Ok(event) = res {
                for path in event.paths {
                    let fs_event = match event.kind {
                        EventKind::Create(_) => FsEvent::Create(path),
                        EventKind::Modify(_) => FsEvent::Modify(path),
                        EventKind::Remove(_) => FsEvent::Delete(path),
                        _ => continue,
                    };
                    let _ = sender.send(fs_event);
                }
            }
        },
        Config::default(),
    )?;
    
    watcher.watch(path.as_ref(), RecursiveMode::Recursive)?;
    
    Ok((watcher, EventReceiver { rx }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;
    
    #[test]
    fn test_fs_event_path() {
        let create = FsEvent::Create(PathBuf::from("file.txt"));
        assert_eq!(create.path(), Path::new("file.txt"));
        
        let rename = FsEvent::Rename(
            PathBuf::from("old.txt"),
            PathBuf::from("new.txt"),
        );
        assert_eq!(rename.path(), Path::new("new.txt"));
    }
    
    #[test]
    fn test_fs_event_affects() {
        let event = FsEvent::Modify(PathBuf::from("test.txt"));
        assert!(event.affects(Path::new("test.txt")));
        assert!(!event.affects(Path::new("other.txt")));
        
        let rename = FsEvent::Rename(
            PathBuf::from("old.txt"),
            PathBuf::from("new.txt"),
        );
        assert!(rename.affects(Path::new("old.txt")));
        assert!(rename.affects(Path::new("new.txt")));
    }
    
    #[test]
    fn test_watcher_config_default() {
        let config = WatcherConfig::default();
        assert_eq!(config.debounce_ms, 100);
        assert!(config.recursive);
    }
    
    #[test]
    fn test_watcher_create_event() {
        let tmp = TempDir::new().unwrap();
        
        let config = WatcherConfig {
            debounce_ms: 10,
            recursive: true,
        };
        let mut watcher = FileWatcher::with_config(tmp.path(), config).unwrap();
        
        // Give watcher time to start
        std::thread::sleep(Duration::from_millis(50));
        
        // Create a file
        fs::write(tmp.path().join("test.txt"), "hello").unwrap();
        
        // Wait for event
        let event = watcher.wait(Duration::from_millis(500));
        assert!(event.is_some());
        
        match event.unwrap() {
            FsEvent::Create(p) | FsEvent::Modify(p) => {
                assert!(p.ends_with("test.txt"));
            }
            _ => panic!("Expected create or modify event"),
        }
    }
}
