//! Block storage types

use sha2::{Sha256, Digest};
use serde::{Deserialize, Serialize};
use std::fmt;

/// SHA-256 hash of block content
#[derive(Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BlockHash([u8; 32]);

impl BlockHash {
    /// Create from raw bytes
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
    
    /// Get raw bytes
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
    
    /// Create from hex string
    pub fn from_hex(s: &str) -> Result<Self, hex::FromHexError> {
        let mut bytes = [0u8; 32];
        hex::decode_to_slice(s, &mut bytes)?;
        Ok(Self(bytes))
    }
    
    /// Convert to hex string
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }
    
    /// Short display (first 8 chars)
    pub fn short(&self) -> String {
        self.to_hex()[..8].to_string()
    }
}

impl fmt::Debug for BlockHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "BlockHash({})", self.short())
    }
}

impl fmt::Display for BlockHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

/// Compute SHA-256 hash of data
pub fn compute_hash(data: &[u8]) -> BlockHash {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let result = hasher.finalize();
    let mut bytes = [0u8; 32];
    bytes.copy_from_slice(&result);
    BlockHash(bytes)
}

/// A content-addressed block
#[derive(Clone, Serialize, Deserialize)]
pub struct Block {
    /// SHA-256 hash of content
    hash: BlockHash,
    /// Raw block data
    data: Vec<u8>,
}

impl Block {
    /// Create a new block from data
    pub fn new(data: Vec<u8>) -> Self {
        let hash = compute_hash(&data);
        Self { hash, data }
    }
    
    /// Create from data with precomputed hash
    pub fn with_hash(hash: BlockHash, data: Vec<u8>) -> Self {
        Self { hash, data }
    }
    
    /// Get block hash
    pub fn hash(&self) -> &BlockHash {
        &self.hash
    }
    
    /// Get block data
    pub fn data(&self) -> &[u8] {
        &self.data
    }
    
    /// Get block size
    pub fn size(&self) -> usize {
        self.data.len()
    }
    
    /// Verify block integrity
    pub fn verify(&self) -> bool {
        compute_hash(&self.data) == self.hash
    }
    
    /// Consume block and return data
    pub fn into_data(self) -> Vec<u8> {
        self.data
    }
}

impl fmt::Debug for Block {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Block")
            .field("hash", &self.hash.short())
            .field("size", &self.data.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_compute_hash() {
        let data = b"hello world";
        let hash = compute_hash(data);
        // Known SHA-256 of "hello world"
        assert_eq!(
            hash.to_hex(),
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }
    
    #[test]
    fn test_block_hash_from_hex() {
        let hex = "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9";
        let hash = BlockHash::from_hex(hex).unwrap();
        assert_eq!(hash.to_hex(), hex);
    }
    
    #[test]
    fn test_block_hash_short() {
        let data = b"test";
        let hash = compute_hash(data);
        assert_eq!(hash.short().len(), 8);
    }
    
    #[test]
    fn test_block_new() {
        let data = b"test data".to_vec();
        let block = Block::new(data.clone());
        assert_eq!(block.data(), data.as_slice());
        assert_eq!(block.size(), data.len());
    }
    
    #[test]
    fn test_block_verify_valid() {
        let block = Block::new(b"some content".to_vec());
        assert!(block.verify());
    }
    
    #[test]
    fn test_block_verify_invalid() {
        let hash = compute_hash(b"original");
        let block = Block::with_hash(hash, b"tampered".to_vec());
        assert!(!block.verify());
    }
}
