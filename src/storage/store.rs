//! Block store - persistent content-addressed storage

use std::path::Path;
use sled::Db;
use crate::Result;
use crate::Error;
use super::block::{Block, BlockHash};

/// Persistent block store backed by sled
pub struct BlockStore {
    db: Db,
}

impl BlockStore {
    /// Open or create a block store at the given path
    pub fn open(path: &Path) -> Result<Self> {
        let db = sled::open(path)
            .map_err(|e| Error::Storage(format!("Failed to open store: {}", e)))?;
        Ok(Self { db })
    }
    
    /// Open an in-memory store (for testing)
    pub fn open_temp() -> Result<Self> {
        let db = sled::Config::new()
            .temporary(true)
            .open()
            .map_err(|e| Error::Storage(format!("Failed to open temp store: {}", e)))?;
        Ok(Self { db })
    }
    
    /// Store a block
    pub fn put(&self, block: &Block) -> Result<()> {
        let key = block.hash().as_bytes();
        let value = block.data();
        self.db.insert(key, value)
            .map_err(|e| Error::Storage(format!("Failed to store block: {}", e)))?;
        Ok(())
    }
    
    /// Retrieve a block by hash
    pub fn get(&self, hash: &BlockHash) -> Result<Option<Block>> {
        let key = hash.as_bytes();
        match self.db.get(key) {
            Ok(Some(data)) => {
                let block = Block::with_hash(*hash, data.to_vec());
                Ok(Some(block))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(Error::Storage(format!("Failed to get block: {}", e))),
        }
    }
    
    /// Check if a block exists
    pub fn has(&self, hash: &BlockHash) -> Result<bool> {
        let key = hash.as_bytes();
        self.db.contains_key(key)
            .map_err(|e| Error::Storage(format!("Failed to check block: {}", e)))
    }
    
    /// Delete a block
    pub fn delete(&self, hash: &BlockHash) -> Result<bool> {
        let key = hash.as_bytes();
        match self.db.remove(key) {
            Ok(Some(_)) => Ok(true),
            Ok(None) => Ok(false),
            Err(e) => Err(Error::Storage(format!("Failed to delete block: {}", e))),
        }
    }
    
    /// List all block hashes
    pub fn list(&self) -> Result<Vec<BlockHash>> {
        let mut hashes = Vec::new();
        for item in self.db.iter() {
            let (key, _) = item
                .map_err(|e| Error::Storage(format!("Failed to iterate: {}", e)))?;
            if key.len() == 32 {
                let mut bytes = [0u8; 32];
                bytes.copy_from_slice(&key);
                hashes.push(BlockHash::from_bytes(bytes));
            }
        }
        Ok(hashes)
    }
    
    /// Count blocks
    pub fn count(&self) -> Result<usize> {
        Ok(self.db.len())
    }
    
    /// Total size in bytes
    pub fn size(&self) -> Result<u64> {
        let mut total = 0u64;
        for item in self.db.iter() {
            let (_, value) = item
                .map_err(|e| Error::Storage(format!("Failed to iterate: {}", e)))?;
            total += value.len() as u64;
        }
        Ok(total)
    }
    
    /// Flush to disk
    pub fn flush(&self) -> Result<()> {
        self.db.flush()
            .map_err(|e| Error::Storage(format!("Failed to flush: {}", e)))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_store_open_temp() {
        let store = BlockStore::open_temp().unwrap();
        assert_eq!(store.count().unwrap(), 0);
    }
    
    #[test]
    fn test_store_put_get() {
        let store = BlockStore::open_temp().unwrap();
        let block = Block::new(b"test data".to_vec());
        let hash = *block.hash();
        
        store.put(&block).unwrap();
        
        let retrieved = store.get(&hash).unwrap().unwrap();
        assert_eq!(retrieved.data(), block.data());
    }
    
    #[test]
    fn test_store_has() {
        let store = BlockStore::open_temp().unwrap();
        let block = Block::new(b"test".to_vec());
        let hash = *block.hash();
        
        assert!(!store.has(&hash).unwrap());
        store.put(&block).unwrap();
        assert!(store.has(&hash).unwrap());
    }
    
    #[test]
    fn test_store_delete() {
        let store = BlockStore::open_temp().unwrap();
        let block = Block::new(b"to delete".to_vec());
        let hash = *block.hash();
        
        store.put(&block).unwrap();
        assert!(store.has(&hash).unwrap());
        
        let deleted = store.delete(&hash).unwrap();
        assert!(deleted);
        assert!(!store.has(&hash).unwrap());
    }
    
    #[test]
    fn test_store_list() {
        let store = BlockStore::open_temp().unwrap();
        let block1 = Block::new(b"block 1".to_vec());
        let block2 = Block::new(b"block 2".to_vec());
        
        store.put(&block1).unwrap();
        store.put(&block2).unwrap();
        
        let hashes = store.list().unwrap();
        assert_eq!(hashes.len(), 2);
        assert!(hashes.contains(block1.hash()));
        assert!(hashes.contains(block2.hash()));
    }
    
    #[test]
    fn test_store_count() {
        let store = BlockStore::open_temp().unwrap();
        
        assert_eq!(store.count().unwrap(), 0);
        
        store.put(&Block::new(b"1".to_vec())).unwrap();
        assert_eq!(store.count().unwrap(), 1);
        
        store.put(&Block::new(b"2".to_vec())).unwrap();
        assert_eq!(store.count().unwrap(), 2);
    }
    
    #[test]
    fn test_store_size() {
        let store = BlockStore::open_temp().unwrap();
        
        store.put(&Block::new(b"hello".to_vec())).unwrap();
        store.put(&Block::new(b"world".to_vec())).unwrap();
        
        let size = store.size().unwrap();
        assert_eq!(size, 10); // 5 + 5
    }
}
