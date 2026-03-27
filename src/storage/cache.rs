//! LRU block cache

use std::collections::HashMap;
use super::block::{Block, BlockHash};

/// LRU cache entry
struct CacheEntry {
    block: Block,
    access_order: u64,
}

/// LRU block cache for frequently accessed blocks
pub struct BlockCache {
    entries: HashMap<BlockHash, CacheEntry>,
    max_size: usize,
    current_size: usize,
    access_counter: u64,
}

impl BlockCache {
    /// Create a new cache with maximum size in bytes
    pub fn new(max_size: usize) -> Self {
        Self {
            entries: HashMap::new(),
            max_size,
            current_size: 0,
            access_counter: 0,
        }
    }
    
    /// Get a block from cache
    pub fn get(&mut self, hash: &BlockHash) -> Option<&Block> {
        if let Some(entry) = self.entries.get_mut(hash) {
            self.access_counter += 1;
            entry.access_order = self.access_counter;
            Some(&entry.block)
        } else {
            None
        }
    }
    
    /// Put a block in cache
    pub fn put(&mut self, block: Block) {
        let hash = *block.hash();
        let size = block.size();
        
        // Don't cache blocks larger than max_size
        if size > self.max_size {
            return;
        }
        
        // Remove existing entry if present
        if let Some(old) = self.entries.remove(&hash) {
            self.current_size -= old.block.size();
        }
        
        // Evict until we have room
        while self.current_size + size > self.max_size && !self.entries.is_empty() {
            self.evict_lru();
        }
        
        // Insert new entry
        self.access_counter += 1;
        self.entries.insert(hash, CacheEntry {
            block,
            access_order: self.access_counter,
        });
        self.current_size += size;
    }
    
    /// Check if block is in cache
    pub fn has(&self, hash: &BlockHash) -> bool {
        self.entries.contains_key(hash)
    }
    
    /// Remove a block from cache
    pub fn remove(&mut self, hash: &BlockHash) -> Option<Block> {
        if let Some(entry) = self.entries.remove(hash) {
            self.current_size -= entry.block.size();
            Some(entry.block)
        } else {
            None
        }
    }
    
    /// Evict the least recently used entry
    fn evict_lru(&mut self) {
        let lru_hash = self.entries
            .iter()
            .min_by_key(|(_, e)| e.access_order)
            .map(|(h, _)| *h);
        
        if let Some(hash) = lru_hash {
            if let Some(entry) = self.entries.remove(&hash) {
                self.current_size -= entry.block.size();
            }
        }
    }
    
    /// Clear the cache
    pub fn clear(&mut self) {
        self.entries.clear();
        self.current_size = 0;
    }
    
    /// Current cache size in bytes
    pub fn size(&self) -> usize {
        self.current_size
    }
    
    /// Number of cached blocks
    pub fn count(&self) -> usize {
        self.entries.len()
    }
    
    /// Cache hit statistics (would need more tracking)
    pub fn utilization(&self) -> f64 {
        if self.max_size == 0 {
            0.0
        } else {
            self.current_size as f64 / self.max_size as f64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_cache_new() {
        let cache = BlockCache::new(1024);
        assert_eq!(cache.size(), 0);
        assert_eq!(cache.count(), 0);
    }
    
    #[test]
    fn test_cache_put_get() {
        let mut cache = BlockCache::new(1024);
        let block = Block::new(b"test".to_vec());
        let hash = *block.hash();
        
        cache.put(block);
        
        let retrieved = cache.get(&hash).unwrap();
        assert_eq!(retrieved.data(), b"test");
    }
    
    #[test]
    fn test_cache_has() {
        let mut cache = BlockCache::new(1024);
        let block = Block::new(b"test".to_vec());
        let hash = *block.hash();
        
        assert!(!cache.has(&hash));
        cache.put(block);
        assert!(cache.has(&hash));
    }
    
    #[test]
    fn test_cache_remove() {
        let mut cache = BlockCache::new(1024);
        let block = Block::new(b"test".to_vec());
        let hash = *block.hash();
        
        cache.put(block);
        assert!(cache.has(&hash));
        
        let removed = cache.remove(&hash).unwrap();
        assert_eq!(removed.data(), b"test");
        assert!(!cache.has(&hash));
    }
    
    #[test]
    fn test_cache_eviction() {
        // Cache with 10 bytes max
        let mut cache = BlockCache::new(10);
        
        // Add 5-byte block
        let b1 = Block::new(b"aaaaa".to_vec());
        let h1 = *b1.hash();
        cache.put(b1);
        
        // Add another 5-byte block
        let b2 = Block::new(b"bbbbb".to_vec());
        let h2 = *b2.hash();
        cache.put(b2);
        
        // Both should be cached (10 bytes total)
        assert!(cache.has(&h1));
        assert!(cache.has(&h2));
        
        // Add 6-byte block - should evict LRU (h1)
        let b3 = Block::new(b"cccccc".to_vec());
        let h3 = *b3.hash();
        cache.put(b3);
        
        assert!(!cache.has(&h1)); // evicted
        assert!(!cache.has(&h2)); // evicted to make room
        assert!(cache.has(&h3));
    }
    
    #[test]
    fn test_cache_lru_order() {
        let mut cache = BlockCache::new(15);
        
        let b1 = Block::new(b"aaaa".to_vec()); // 4 bytes
        let h1 = *b1.hash();
        cache.put(b1);
        
        let b2 = Block::new(b"bbbb".to_vec()); // 4 bytes
        let h2 = *b2.hash();
        cache.put(b2);
        
        let b3 = Block::new(b"cccc".to_vec()); // 4 bytes
        let h3 = *b3.hash();
        cache.put(b3);
        
        // Access h1 to make it recently used
        cache.get(&h1);
        
        // Add block that requires eviction
        let b4 = Block::new(b"dddd".to_vec()); // 4 bytes
        let h4 = *b4.hash();
        cache.put(b4);
        
        // h2 should be evicted (least recently used), not h1
        assert!(cache.has(&h1));
        assert!(!cache.has(&h2));
        assert!(cache.has(&h3));
        assert!(cache.has(&h4));
    }
    
    #[test]
    fn test_cache_clear() {
        let mut cache = BlockCache::new(1024);
        cache.put(Block::new(b"a".to_vec()));
        cache.put(Block::new(b"b".to_vec()));
        
        assert_eq!(cache.count(), 2);
        cache.clear();
        assert_eq!(cache.count(), 0);
        assert_eq!(cache.size(), 0);
    }
}
