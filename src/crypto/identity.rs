//! Device identity using Ed25519 keys

use ring::rand::SystemRandom;
use ring::signature::{Ed25519KeyPair, KeyPair};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs;
use std::path::Path;

/// Device identifier derived from public key
#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DeviceId([u8; 32]);

impl DeviceId {
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
    
    /// Display format (XXXXX-XXXXX-XXXXX-XXXXX-XXXXX-XXXXX-X)
    pub fn to_display(&self) -> String {
        let hex = self.to_hex();
        let mut result = String::with_capacity(71);
        for (i, chunk) in hex.as_bytes().chunks(5).enumerate() {
            if i > 0 {
                result.push('-');
            }
            result.push_str(std::str::from_utf8(chunk).unwrap_or(""));
        }
        result.to_uppercase()
    }
    
    /// Parse from display format
    pub fn from_display(s: &str) -> Result<Self, hex::FromHexError> {
        let clean: String = s.chars()
            .filter(|c| c.is_ascii_hexdigit())
            .collect();
        Self::from_hex(&clean.to_lowercase())
    }
    
    /// Short display (first 7 chars)
    pub fn short(&self) -> String {
        self.to_display()[..7].to_string()
    }
}

impl fmt::Debug for DeviceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "DeviceId({})", self.short())
    }
}

impl fmt::Display for DeviceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_display())
    }
}

/// Device key pair for signing and identity
pub struct DeviceKey {
    /// Ed25519 key pair
    keypair: Ed25519KeyPair,
    /// PKCS#8 encoded secret key
    pkcs8_bytes: Vec<u8>,
}

impl DeviceKey {
    /// Generate a new random device key
    pub fn generate() -> Result<Self, ring::error::Unspecified> {
        let rng = SystemRandom::new();
        let pkcs8_bytes = Ed25519KeyPair::generate_pkcs8(&rng)?;
        let keypair = Ed25519KeyPair::from_pkcs8(pkcs8_bytes.as_ref())?;
        
        Ok(Self {
            keypair,
            pkcs8_bytes: pkcs8_bytes.as_ref().to_vec(),
        })
    }
    
    /// Load from PKCS#8 bytes
    pub fn from_pkcs8(pkcs8_bytes: &[u8]) -> Result<Self, ring::error::KeyRejected> {
        let keypair = Ed25519KeyPair::from_pkcs8(pkcs8_bytes)?;
        
        Ok(Self {
            keypair,
            pkcs8_bytes: pkcs8_bytes.to_vec(),
        })
    }
    
    /// Get the device ID (derived from public key)
    pub fn device_id(&self) -> DeviceId {
        let public = self.keypair.public_key().as_ref();
        let mut bytes = [0u8; 32];
        bytes.copy_from_slice(public);
        DeviceId(bytes)
    }
    
    /// Get the public key bytes
    pub fn public_key(&self) -> &[u8] {
        self.keypair.public_key().as_ref()
    }
    
    /// Get the PKCS#8 encoded key bytes
    pub fn to_pkcs8(&self) -> &[u8] {
        &self.pkcs8_bytes
    }
    
    /// Sign data with this key
    pub fn sign(&self, data: &[u8]) -> Vec<u8> {
        self.keypair.sign(data).as_ref().to_vec()
    }
    
    /// Save key to file
    pub fn save(&self, path: impl AsRef<Path>) -> std::io::Result<()> {
        fs::write(path, &self.pkcs8_bytes)
    }
    
    /// Load key from file
    pub fn load(path: impl AsRef<Path>) -> Result<Self, DeviceKeyError> {
        let bytes = fs::read(path)?;
        Self::from_pkcs8(&bytes).map_err(DeviceKeyError::from)
    }
}

/// Error loading device key
#[derive(Debug)]
pub enum DeviceKeyError {
    Io(std::io::Error),
    Key(ring::error::KeyRejected),
}

impl From<std::io::Error> for DeviceKeyError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<ring::error::KeyRejected> for DeviceKeyError {
    fn from(e: ring::error::KeyRejected) -> Self {
        Self::Key(e)
    }
}

impl std::fmt::Display for DeviceKeyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "IO error: {}", e),
            Self::Key(e) => write!(f, "Key error: {}", e),
        }
    }
}

impl std::error::Error for DeviceKeyError {}

/// Verify a signature from a device ID
pub fn verify_signature(device_id: &DeviceId, data: &[u8], signature: &[u8]) -> bool {
    use ring::signature::{UnparsedPublicKey, ED25519};
    
    let public_key = UnparsedPublicKey::new(&ED25519, device_id.as_bytes());
    public_key.verify(data, signature).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    
    #[test]
    fn test_device_id_from_hex() {
        let hex = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let id = DeviceId::from_hex(hex).unwrap();
        assert_eq!(id.to_hex(), hex);
    }
    
    #[test]
    fn test_device_id_display_format() {
        let hex = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let id = DeviceId::from_hex(hex).unwrap();
        let display = id.to_display();
        
        assert!(display.contains('-'));
        // 64 hex chars / 5 = 12 groups of 5 + 1 group of 4 = 13 groups, 12 dashes
        // Total: 64 + 12 = 76
        assert_eq!(display.len(), 76);
        
        // Round-trip
        let parsed = DeviceId::from_display(&display).unwrap();
        assert_eq!(parsed, id);
    }
    
    #[test]
    fn test_device_key_generate() {
        let key = DeviceKey::generate().unwrap();
        let id = key.device_id();
        
        assert_eq!(id.as_bytes().len(), 32);
        assert_eq!(key.public_key().len(), 32);
    }
    
    #[test]
    fn test_device_key_sign_verify() {
        let key = DeviceKey::generate().unwrap();
        let device_id = key.device_id();
        let data = b"hello world";
        
        let signature = key.sign(data);
        assert!(verify_signature(&device_id, data, &signature));
        
        // Verify fails with wrong data
        assert!(!verify_signature(&device_id, b"wrong data", &signature));
    }
    
    #[test]
    fn test_device_key_save_load() {
        let tmp = TempDir::new().unwrap();
        let key_path = tmp.path().join("device.key");
        
        let key1 = DeviceKey::generate().unwrap();
        key1.save(&key_path).unwrap();
        
        let key2 = DeviceKey::load(&key_path).unwrap();
        assert_eq!(key1.device_id(), key2.device_id());
    }
    
    #[test]
    fn test_device_key_pkcs8_roundtrip() {
        let key1 = DeviceKey::generate().unwrap();
        let pkcs8 = key1.to_pkcs8();
        
        let key2 = DeviceKey::from_pkcs8(pkcs8).unwrap();
        assert_eq!(key1.device_id(), key2.device_id());
    }
}
