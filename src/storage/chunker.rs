//! Content-defined chunking using Rabin fingerprinting

use super::block::{Block, compute_hash};

/// Chunker configuration
#[derive(Debug, Clone)]
pub struct ChunkerConfig {
    /// Minimum chunk size in bytes
    pub min_size: usize,
    /// Maximum chunk size in bytes
    pub max_size: usize,
    /// Target average chunk size in bytes
    pub avg_size: usize,
    /// Mask for boundary detection (determines avg size)
    mask: u64,
}

impl ChunkerConfig {
    /// Create configuration with target average size
    pub fn new(min_size: usize, max_size: usize, avg_size: usize) -> Self {
        // Mask is (avg_size - 1) for power-of-2 avg sizes
        // This gives ~avg_size average chunk size
        let mask = (avg_size - 1) as u64;
        Self {
            min_size,
            max_size,
            avg_size,
            mask,
        }
    }
    
    /// Default configuration (16KB average)
    pub fn default_16k() -> Self {
        Self::new(4 * 1024, 64 * 1024, 16 * 1024)
    }
    
    /// Small chunks (4KB average, for testing)
    pub fn small() -> Self {
        Self::new(1024, 8 * 1024, 4 * 1024)
    }
}

impl Default for ChunkerConfig {
    fn default() -> Self {
        Self::default_16k()
    }
}

/// Rabin fingerprint polynomial
/// Using a common 64-bit polynomial
const RABIN_POLY: u64 = 0xbfe6b8a5bf378d83;

/// Window size for rolling hash
const WINDOW_SIZE: usize = 48;

/// Precomputed table for Rabin fingerprinting
struct RabinTable {
    out_table: [u64; 256],
    mod_table: [u64; 256],
}

impl RabinTable {
    fn new() -> Self {
        let mut out_table = [0u64; 256];
        let mut mod_table = [0u64; 256];
        
        // Compute tables
        for i in 0..256 {
            let mut hash = (i as u64) << 56;
            for _ in 0..8 {
                if hash & (1 << 63) != 0 {
                    hash = (hash << 1) ^ RABIN_POLY;
                } else {
                    hash <<= 1;
                }
            }
            mod_table[i] = hash;
        }
        
        // Out table for sliding window
        let mut k = 1u64;
        for _ in 0..WINDOW_SIZE {
            k = rabin_shift(k, 0, &mod_table);
        }
        for i in 0..256 {
            out_table[i] = rabin_shift(k, i as u8, &mod_table);
        }
        
        Self { out_table, mod_table }
    }
}

fn rabin_shift(hash: u64, byte: u8, mod_table: &[u64; 256]) -> u64 {
    let out = (hash >> 56) as usize;
    ((hash << 8) | byte as u64) ^ mod_table[out]
}

lazy_static::lazy_static! {
    static ref RABIN: RabinTable = RabinTable::new();
}

/// Rolling hash state
pub struct RollingHash {
    hash: u64,
    window: [u8; WINDOW_SIZE],
    window_pos: usize,
    window_filled: bool,
}

impl RollingHash {
    pub fn new() -> Self {
        Self {
            hash: 0,
            window: [0; WINDOW_SIZE],
            window_pos: 0,
            window_filled: false,
        }
    }
    
    /// Add a byte and return new hash
    pub fn roll(&mut self, byte: u8) -> u64 {
        let out_byte = self.window[self.window_pos];
        self.window[self.window_pos] = byte;
        self.window_pos = (self.window_pos + 1) % WINDOW_SIZE;
        
        if self.window_filled {
            // Slide out old byte, slide in new byte
            self.hash ^= RABIN.out_table[out_byte as usize];
            self.hash = rabin_shift(self.hash, byte, &RABIN.mod_table);
        } else {
            self.hash = rabin_shift(self.hash, byte, &RABIN.mod_table);
            if self.window_pos == 0 {
                self.window_filled = true;
            }
        }
        
        self.hash
    }
    
    /// Reset state
    pub fn reset(&mut self) {
        self.hash = 0;
        self.window = [0; WINDOW_SIZE];
        self.window_pos = 0;
        self.window_filled = false;
    }
    
    /// Get current hash
    pub fn hash(&self) -> u64 {
        self.hash
    }
}

impl Default for RollingHash {
    fn default() -> Self {
        Self::new()
    }
}

/// Content-defined chunker
pub struct Chunker {
    config: ChunkerConfig,
}

impl Chunker {
    pub fn new(config: ChunkerConfig) -> Self {
        Self { config }
    }
    
    /// Chunk data into blocks
    pub fn chunk(&self, data: &[u8]) -> Vec<Block> {
        if data.is_empty() {
            return Vec::new();
        }
        
        let mut blocks = Vec::new();
        let mut rolling = RollingHash::new();
        let mut chunk_start = 0;
        
        for i in 0..data.len() {
            let hash = rolling.roll(data[i]);
            let chunk_len = i - chunk_start + 1;
            
            // Check for boundary
            let at_boundary = chunk_len >= self.config.min_size
                && (hash & self.config.mask == 0 || chunk_len >= self.config.max_size);
            
            let at_end = i == data.len() - 1;
            
            if at_boundary || at_end {
                let chunk_data = data[chunk_start..=i].to_vec();
                blocks.push(Block::new(chunk_data));
                chunk_start = i + 1;
                rolling.reset();
            }
        }
        
        blocks
    }
    
    /// Chunk data and return hashes only
    pub fn chunk_hashes(&self, data: &[u8]) -> Vec<super::BlockHash> {
        self.chunk(data).iter().map(|b| *b.hash()).collect()
    }
}

impl Default for Chunker {
    fn default() -> Self {
        Self::new(ChunkerConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_chunker_config_default() {
        let config = ChunkerConfig::default();
        assert_eq!(config.min_size, 4 * 1024);
        assert_eq!(config.max_size, 64 * 1024);
        assert_eq!(config.avg_size, 16 * 1024);
    }
    
    #[test]
    fn test_rolling_hash_deterministic() {
        let mut hash1 = RollingHash::new();
        let mut hash2 = RollingHash::new();
        
        for &byte in b"hello world" {
            let h1 = hash1.roll(byte);
            let h2 = hash2.roll(byte);
            assert_eq!(h1, h2);
        }
    }
    
    #[test]
    fn test_chunker_empty() {
        let chunker = Chunker::default();
        let blocks = chunker.chunk(&[]);
        assert!(blocks.is_empty());
    }
    
    #[test]
    fn test_chunker_small_data() {
        let chunker = Chunker::new(ChunkerConfig::small());
        let data = b"small data";
        let blocks = chunker.chunk(data);
        
        // Small data should produce one block
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].data(), data);
    }
    
    #[test]
    fn test_chunker_preserves_data() {
        let chunker = Chunker::new(ChunkerConfig::small());
        let data: Vec<u8> = (0..10000).map(|i| (i % 256) as u8).collect();
        let blocks = chunker.chunk(&data);
        
        // Reassemble
        let mut reassembled = Vec::new();
        for block in blocks {
            reassembled.extend_from_slice(block.data());
        }
        
        assert_eq!(reassembled, data);
    }
    
    #[test]
    fn test_chunker_respects_min_size() {
        let config = ChunkerConfig::new(100, 1000, 200);
        let chunker = Chunker::new(config);
        let data: Vec<u8> = (0..500).map(|i| (i % 256) as u8).collect();
        let blocks = chunker.chunk(&data);
        
        for block in &blocks[..blocks.len().saturating_sub(1)] {
            assert!(block.size() >= 100, "block size {} < min 100", block.size());
        }
    }
    
    #[test]
    fn test_chunker_respects_max_size() {
        let config = ChunkerConfig::new(10, 100, 50);
        let chunker = Chunker::new(config);
        let data: Vec<u8> = (0..1000).map(|i| (i % 256) as u8).collect();
        let blocks = chunker.chunk(&data);
        
        for block in &blocks {
            assert!(block.size() <= 100, "block size {} > max 100", block.size());
        }
    }
    
    #[test]
    fn test_chunker_content_defined() {
        // Same content should produce same chunks
        let chunker = Chunker::new(ChunkerConfig::small());
        
        let data: Vec<u8> = (0..5000).map(|i| (i % 256) as u8).collect();
        let blocks1 = chunker.chunk(&data);
        let blocks2 = chunker.chunk(&data);
        
        assert_eq!(blocks1.len(), blocks2.len());
        for (b1, b2) in blocks1.iter().zip(blocks2.iter()) {
            assert_eq!(b1.hash(), b2.hash());
        }
    }
}
