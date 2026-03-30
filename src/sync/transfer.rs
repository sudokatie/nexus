//! Block transfer for sync operations

use crate::storage::{Block, BlockHash, BlockStore};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Transfer priority
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    /// Low priority (background)
    Low = 0,
    /// Normal priority
    Normal = 1,
    /// High priority (user requested)
    High = 2,
}

impl Default for Priority {
    fn default() -> Self {
        Self::Normal
    }
}

/// A pending block transfer request
#[derive(Debug, Clone)]
pub struct TransferRequest {
    /// Block hash to transfer
    pub hash: BlockHash,
    /// Priority level
    pub priority: Priority,
    /// When the request was made
    pub requested_at: Instant,
    /// Number of retry attempts
    pub attempts: u32,
}

impl TransferRequest {
    /// Create a new transfer request
    pub fn new(hash: BlockHash) -> Self {
        Self {
            hash,
            priority: Priority::Normal,
            requested_at: Instant::now(),
            attempts: 0,
        }
    }
    
    /// Create with priority
    pub fn with_priority(hash: BlockHash, priority: Priority) -> Self {
        Self {
            hash,
            priority,
            requested_at: Instant::now(),
            attempts: 0,
        }
    }
    
    /// Age of this request
    pub fn age(&self) -> Duration {
        self.requested_at.elapsed()
    }
}

/// Transfer queue for block requests
#[derive(Debug)]
pub struct TransferQueue {
    /// High priority queue
    high: VecDeque<TransferRequest>,
    /// Normal priority queue
    normal: VecDeque<TransferRequest>,
    /// Low priority queue
    low: VecDeque<TransferRequest>,
    /// In-flight requests (hash -> request)
    in_flight: HashMap<BlockHash, TransferRequest>,
    /// Maximum concurrent requests
    max_concurrent: usize,
}

impl TransferQueue {
    /// Create a new transfer queue
    pub fn new(max_concurrent: usize) -> Self {
        Self {
            high: VecDeque::new(),
            normal: VecDeque::new(),
            low: VecDeque::new(),
            in_flight: HashMap::new(),
            max_concurrent,
        }
    }
    
    /// Enqueue a transfer request
    pub fn enqueue(&mut self, request: TransferRequest) {
        // Skip if already queued or in flight
        if self.in_flight.contains_key(&request.hash) {
            return;
        }
        
        match request.priority {
            Priority::High => self.high.push_back(request),
            Priority::Normal => self.normal.push_back(request),
            Priority::Low => self.low.push_back(request),
        }
    }
    
    /// Enqueue multiple requests
    pub fn enqueue_all(&mut self, requests: impl IntoIterator<Item = TransferRequest>) {
        for req in requests {
            self.enqueue(req);
        }
    }
    
    /// Get next request to process (if under concurrency limit)
    pub fn next(&mut self) -> Option<TransferRequest> {
        if self.in_flight.len() >= self.max_concurrent {
            return None;
        }
        
        let request = self.high.pop_front()
            .or_else(|| self.normal.pop_front())
            .or_else(|| self.low.pop_front())?;
        
        self.in_flight.insert(request.hash.clone(), request.clone());
        Some(request)
    }
    
    /// Mark a transfer as complete
    pub fn complete(&mut self, hash: &BlockHash) -> Option<TransferRequest> {
        self.in_flight.remove(hash)
    }
    
    /// Mark a transfer as failed (will be retried)
    pub fn failed(&mut self, hash: &BlockHash, max_retries: u32) {
        if let Some(mut request) = self.in_flight.remove(hash) {
            request.attempts += 1;
            if request.attempts < max_retries {
                // Re-enqueue at lower priority
                request.priority = Priority::Low;
                self.enqueue(request);
            }
        }
    }
    
    /// Total pending (queued + in-flight)
    pub fn pending_count(&self) -> usize {
        self.high.len() + self.normal.len() + self.low.len() + self.in_flight.len()
    }
    
    /// Number queued (not in flight)
    pub fn queued_count(&self) -> usize {
        self.high.len() + self.normal.len() + self.low.len()
    }
    
    /// Number in flight
    pub fn in_flight_count(&self) -> usize {
        self.in_flight.len()
    }
    
    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.pending_count() == 0
    }
    
    /// Clear all queues
    pub fn clear(&mut self) {
        self.high.clear();
        self.normal.clear();
        self.low.clear();
        self.in_flight.clear();
    }
}

/// Transfer statistics
#[derive(Debug, Clone, Default)]
pub struct TransferStats {
    /// Blocks transferred
    pub blocks_transferred: u64,
    /// Bytes transferred
    pub bytes_transferred: u64,
    /// Blocks failed
    pub blocks_failed: u64,
    /// Transfer start time
    pub started_at: Option<Instant>,
    /// Transfer end time
    pub ended_at: Option<Instant>,
}

impl TransferStats {
    /// Create new stats
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Start tracking
    pub fn start(&mut self) {
        self.started_at = Some(Instant::now());
    }
    
    /// End tracking
    pub fn end(&mut self) {
        self.ended_at = Some(Instant::now());
    }
    
    /// Record a successful transfer
    pub fn record_success(&mut self, bytes: u64) {
        self.blocks_transferred += 1;
        self.bytes_transferred += bytes;
    }
    
    /// Record a failed transfer
    pub fn record_failure(&mut self) {
        self.blocks_failed += 1;
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
            self.bytes_transferred as f64 / secs
        } else {
            0.0
        }
    }
}

/// Rate limiter for transfers
pub struct RateLimiter {
    /// Bytes per second limit
    limit: u64,
    /// Bytes transferred in current window
    current_bytes: u64,
    /// Window start
    window_start: Instant,
    /// Window duration
    window: Duration,
}

impl RateLimiter {
    /// Create a new rate limiter (bytes per second)
    pub fn new(bytes_per_second: u64) -> Self {
        Self {
            limit: bytes_per_second,
            current_bytes: 0,
            window_start: Instant::now(),
            window: Duration::from_secs(1),
        }
    }
    
    /// Check if transfer of given size is allowed
    pub fn check(&mut self, bytes: u64) -> bool {
        self.maybe_reset_window();
        
        if self.current_bytes + bytes <= self.limit {
            self.current_bytes += bytes;
            true
        } else {
            false
        }
    }
    
    /// Get delay needed before next transfer
    pub fn delay_needed(&self) -> Duration {
        let elapsed = self.window_start.elapsed();
        if elapsed < self.window {
            self.window - elapsed
        } else {
            Duration::ZERO
        }
    }
    
    /// Reset window if expired
    fn maybe_reset_window(&mut self) {
        if self.window_start.elapsed() >= self.window {
            self.window_start = Instant::now();
            self.current_bytes = 0;
        }
    }
    
    /// Get current limit
    pub fn limit(&self) -> u64 {
        self.limit
    }
    
    /// Set limit
    pub fn set_limit(&mut self, bytes_per_second: u64) {
        self.limit = bytes_per_second;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::compute_hash;
    
    fn test_hash(data: &[u8]) -> BlockHash {
        compute_hash(data)
    }
    
    #[test]
    fn test_transfer_request() {
        let hash = test_hash(b"data");
        let request = TransferRequest::new(hash.clone());
        
        assert_eq!(request.hash, hash);
        assert_eq!(request.priority, Priority::Normal);
        assert_eq!(request.attempts, 0);
    }
    
    #[test]
    fn test_transfer_queue_priority() {
        let mut queue = TransferQueue::new(10);
        
        queue.enqueue(TransferRequest::with_priority(test_hash(b"low"), Priority::Low));
        queue.enqueue(TransferRequest::with_priority(test_hash(b"high"), Priority::High));
        queue.enqueue(TransferRequest::with_priority(test_hash(b"normal"), Priority::Normal));
        
        // Should get high first, then normal, then low
        let first = queue.next().unwrap();
        assert_eq!(first.priority, Priority::High);
        
        let second = queue.next().unwrap();
        assert_eq!(second.priority, Priority::Normal);
        
        let third = queue.next().unwrap();
        assert_eq!(third.priority, Priority::Low);
    }
    
    #[test]
    fn test_transfer_queue_concurrency() {
        let mut queue = TransferQueue::new(2);
        
        queue.enqueue(TransferRequest::new(test_hash(b"a")));
        queue.enqueue(TransferRequest::new(test_hash(b"b")));
        queue.enqueue(TransferRequest::new(test_hash(b"c")));
        
        assert!(queue.next().is_some());
        assert!(queue.next().is_some());
        // Third should be blocked by concurrency limit
        assert!(queue.next().is_none());
        
        // Complete one
        queue.complete(&test_hash(b"a"));
        // Now we can get another
        assert!(queue.next().is_some());
    }
    
    #[test]
    fn test_transfer_queue_retry() {
        let mut queue = TransferQueue::new(10);
        let hash = test_hash(b"fail");
        
        queue.enqueue(TransferRequest::new(hash.clone()));
        queue.next(); // Move to in-flight
        
        queue.failed(&hash, 3); // Max 3 retries
        
        // Should be re-queued at low priority
        let retry = queue.next().unwrap();
        assert_eq!(retry.attempts, 1);
        assert_eq!(retry.priority, Priority::Low);
    }
    
    #[test]
    fn test_transfer_stats() {
        let mut stats = TransferStats::new();
        stats.start();
        
        stats.record_success(1024);
        stats.record_success(2048);
        stats.record_failure();
        
        assert_eq!(stats.blocks_transferred, 2);
        assert_eq!(stats.bytes_transferred, 3072);
        assert_eq!(stats.blocks_failed, 1);
    }
    
    #[test]
    fn test_rate_limiter() {
        let mut limiter = RateLimiter::new(1000); // 1000 bytes/sec
        
        assert!(limiter.check(500));
        assert!(limiter.check(400));
        // Should be blocked now (900 used, 200 would exceed 1000)
        assert!(!limiter.check(200));
    }
}
