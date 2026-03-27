//! Symmetric encryption using ChaCha20-Poly1305

use ring::aead::{self, Aad, BoundKey, Nonce, NonceSequence, OpeningKey, SealingKey, UnboundKey};
use ring::rand::{SecureRandom, SystemRandom};

use super::session::SessionKey;

/// Nonce size for ChaCha20-Poly1305
pub const NONCE_SIZE: usize = 12;

/// Tag size for ChaCha20-Poly1305
pub const TAG_SIZE: usize = 16;

/// Errors from encryption/decryption
#[derive(Debug)]
pub enum CipherError {
    /// Failed to create key
    KeyCreation,
    /// Failed to encrypt
    Encryption,
    /// Failed to decrypt (authentication failed)
    Decryption,
    /// Invalid nonce
    InvalidNonce,
}

impl std::fmt::Display for CipherError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CipherError::KeyCreation => write!(f, "Failed to create encryption key"),
            CipherError::Encryption => write!(f, "Encryption failed"),
            CipherError::Decryption => write!(f, "Decryption failed (authentication error)"),
            CipherError::InvalidNonce => write!(f, "Invalid nonce"),
        }
    }
}

impl std::error::Error for CipherError {}

/// Counter-based nonce sequence
struct CounterNonceSequence {
    counter: u64,
    prefix: [u8; 4],
}

impl CounterNonceSequence {
    fn new() -> Self {
        let rng = SystemRandom::new();
        let mut prefix = [0u8; 4];
        rng.fill(&mut prefix).expect("Failed to generate nonce prefix");
        
        Self {
            counter: 0,
            prefix,
        }
    }
    
    fn with_nonce(nonce: [u8; NONCE_SIZE]) -> Self {
        let mut prefix = [0u8; 4];
        prefix.copy_from_slice(&nonce[0..4]);
        let counter = u64::from_le_bytes(nonce[4..12].try_into().unwrap());
        
        Self { counter, prefix }
    }
    
    fn current_nonce(&self) -> [u8; NONCE_SIZE] {
        let mut nonce = [0u8; NONCE_SIZE];
        nonce[0..4].copy_from_slice(&self.prefix);
        nonce[4..12].copy_from_slice(&self.counter.to_le_bytes());
        nonce
    }
}

impl NonceSequence for CounterNonceSequence {
    fn advance(&mut self) -> Result<Nonce, ring::error::Unspecified> {
        let nonce_bytes = self.current_nonce();
        self.counter = self.counter.wrapping_add(1);
        Nonce::try_assume_unique_for_key(&nonce_bytes)
    }
}

/// Encrypt data using ChaCha20-Poly1305
/// 
/// Returns nonce || ciphertext || tag
pub fn encrypt(key: &SessionKey, plaintext: &[u8]) -> Result<Vec<u8>, CipherError> {
    encrypt_with_aad(key, plaintext, &[])
}

/// Encrypt data with additional authenticated data
pub fn encrypt_with_aad(
    key: &SessionKey,
    plaintext: &[u8],
    aad: &[u8],
) -> Result<Vec<u8>, CipherError> {
    let unbound_key = UnboundKey::new(&aead::CHACHA20_POLY1305, key.as_bytes())
        .map_err(|_| CipherError::KeyCreation)?;
    
    let mut nonce_seq = CounterNonceSequence::new();
    let nonce_bytes = nonce_seq.current_nonce();
    
    let mut sealing_key = SealingKey::new(unbound_key, nonce_seq);
    
    // Encrypt in place - start with plaintext, tag will be appended
    let mut in_out = plaintext.to_vec();
    
    let aad = Aad::from(aad);
    sealing_key
        .seal_in_place_append_tag(aad, &mut in_out)
        .map_err(|_| CipherError::Encryption)?;
    
    // Prepend nonce to result: nonce || ciphertext || tag
    let mut output = Vec::with_capacity(NONCE_SIZE + in_out.len());
    output.extend_from_slice(&nonce_bytes);
    output.extend_from_slice(&in_out);
    
    Ok(output)
}

/// Decrypt data using ChaCha20-Poly1305
/// 
/// Expects nonce || ciphertext || tag
pub fn decrypt(key: &SessionKey, ciphertext: &[u8]) -> Result<Vec<u8>, CipherError> {
    decrypt_with_aad(key, ciphertext, &[])
}

/// Decrypt data with additional authenticated data
pub fn decrypt_with_aad(
    key: &SessionKey,
    ciphertext: &[u8],
    aad: &[u8],
) -> Result<Vec<u8>, CipherError> {
    if ciphertext.len() < NONCE_SIZE + TAG_SIZE {
        return Err(CipherError::InvalidNonce);
    }
    
    let mut nonce_bytes = [0u8; NONCE_SIZE];
    nonce_bytes.copy_from_slice(&ciphertext[..NONCE_SIZE]);
    
    let unbound_key = UnboundKey::new(&aead::CHACHA20_POLY1305, key.as_bytes())
        .map_err(|_| CipherError::KeyCreation)?;
    
    let nonce_seq = CounterNonceSequence::with_nonce(nonce_bytes);
    let mut opening_key = OpeningKey::new(unbound_key, nonce_seq);
    
    // Copy ciphertext (without nonce prefix) for in-place decryption
    let mut data = ciphertext[NONCE_SIZE..].to_vec();
    
    let aad = Aad::from(aad);
    let plaintext = opening_key
        .open_in_place(aad, &mut data)
        .map_err(|_| CipherError::Decryption)?;
    
    Ok(plaintext.to_vec())
}

/// Cipher for streaming encryption with a session key
pub struct Cipher {
    key: SessionKey,
}

impl Cipher {
    /// Create a new cipher from a session key
    pub fn new(key: SessionKey) -> Self {
        Self { key }
    }
    
    /// Encrypt data
    pub fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, CipherError> {
        encrypt(&self.key, plaintext)
    }
    
    /// Decrypt data
    pub fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>, CipherError> {
        decrypt(&self.key, ciphertext)
    }
    
    /// Encrypt with AAD
    pub fn encrypt_aad(&self, plaintext: &[u8], aad: &[u8]) -> Result<Vec<u8>, CipherError> {
        encrypt_with_aad(&self.key, plaintext, aad)
    }
    
    /// Decrypt with AAD
    pub fn decrypt_aad(&self, ciphertext: &[u8], aad: &[u8]) -> Result<Vec<u8>, CipherError> {
        decrypt_with_aad(&self.key, ciphertext, aad)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    fn test_key() -> SessionKey {
        SessionKey::from_bytes([42u8; 32])
    }
    
    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let key = test_key();
        let plaintext = b"Hello, World!";
        
        let ciphertext = encrypt(&key, plaintext).unwrap();
        let decrypted = decrypt(&key, &ciphertext).unwrap();
        
        assert_eq!(decrypted, plaintext);
    }
    
    #[test]
    fn test_ciphertext_has_overhead() {
        let key = test_key();
        let plaintext = b"test";
        
        let ciphertext = encrypt(&key, plaintext).unwrap();
        
        // Ciphertext should be: nonce (12) + plaintext (4) + tag (16) = 32
        assert_eq!(ciphertext.len(), plaintext.len() + NONCE_SIZE + TAG_SIZE);
    }
    
    #[test]
    fn test_decrypt_wrong_key_fails() {
        let key1 = SessionKey::from_bytes([1u8; 32]);
        let key2 = SessionKey::from_bytes([2u8; 32]);
        
        let ciphertext = encrypt(&key1, b"secret").unwrap();
        let result = decrypt(&key2, &ciphertext);
        
        assert!(result.is_err());
    }
    
    #[test]
    fn test_decrypt_tampered_fails() {
        let key = test_key();
        let mut ciphertext = encrypt(&key, b"secret").unwrap();
        
        // Tamper with ciphertext
        let last = ciphertext.len() - 1;
        ciphertext[last] ^= 0xFF;
        
        let result = decrypt(&key, &ciphertext);
        assert!(result.is_err());
    }
    
    #[test]
    fn test_encrypt_with_aad() {
        let key = test_key();
        let plaintext = b"message";
        let aad = b"header";
        
        let ciphertext = encrypt_with_aad(&key, plaintext, aad).unwrap();
        
        // Decrypt with correct AAD
        let decrypted = decrypt_with_aad(&key, &ciphertext, aad).unwrap();
        assert_eq!(decrypted, plaintext);
        
        // Decrypt with wrong AAD should fail
        let result = decrypt_with_aad(&key, &ciphertext, b"wrong");
        assert!(result.is_err());
    }
    
    #[test]
    fn test_cipher_struct() {
        let cipher = Cipher::new(test_key());
        let plaintext = b"test message";
        
        let encrypted = cipher.encrypt(plaintext).unwrap();
        let decrypted = cipher.decrypt(&encrypted).unwrap();
        
        assert_eq!(decrypted, plaintext);
    }
}
