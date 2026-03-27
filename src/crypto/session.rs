//! Session key derivation using X25519 key exchange

use ring::agreement::{self, EphemeralPrivateKey, PublicKey, UnparsedPublicKey, X25519};
use ring::hkdf::{self, Salt, HKDF_SHA256};
use ring::rand::SystemRandom;
use serde::{Deserialize, Serialize};

/// Session key for symmetric encryption
#[derive(Clone, Serialize, Deserialize)]
pub struct SessionKey([u8; 32]);

impl SessionKey {
    /// Create from raw bytes
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
    
    /// Get raw bytes
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl std::fmt::Debug for SessionKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SessionKey([redacted])")
    }
}

/// Ephemeral key pair for key exchange
pub struct EphemeralKeyPair {
    private_key: EphemeralPrivateKey,
    public_key_bytes: Vec<u8>,
}

impl EphemeralKeyPair {
    /// Generate a new ephemeral key pair
    pub fn generate() -> Result<Self, ring::error::Unspecified> {
        let rng = SystemRandom::new();
        let private_key = EphemeralPrivateKey::generate(&X25519, &rng)?;
        let public_key = private_key.compute_public_key()?;
        
        Ok(Self {
            private_key,
            public_key_bytes: public_key.as_ref().to_vec(),
        })
    }
    
    /// Get the public key bytes
    pub fn public_key(&self) -> &[u8] {
        &self.public_key_bytes
    }
    
    /// Perform key agreement and derive session key
    pub fn derive_session_key(
        self,
        peer_public_key: &[u8],
        context: &[u8],
    ) -> Result<SessionKey, ring::error::Unspecified> {
        let peer_key = UnparsedPublicKey::new(&X25519, peer_public_key);
        
        agreement::agree_ephemeral(
            self.private_key,
            &peer_key,
            |shared_secret| {
                derive_key_hkdf(shared_secret, context)
            },
        )
    }
}

/// Derive a key using HKDF
fn derive_key_hkdf(input: &[u8], info: &[u8]) -> SessionKey {
    let salt = Salt::new(HKDF_SHA256, b"nexus-session-v1");
    let prk = salt.extract(input);
    
    let mut key_bytes = [0u8; 32];
    let info_refs = [info];
    let okm = prk.expand(&info_refs, MyKeyType(32)).unwrap();
    okm.fill(&mut key_bytes).unwrap();
    
    SessionKey(key_bytes)
}

// Helper type for HKDF output length
struct MyKeyType(usize);

impl hkdf::KeyType for MyKeyType {
    fn len(&self) -> usize {
        self.0
    }
}

/// Key exchange helper for both parties
pub struct KeyExchange {
    our_keypair: EphemeralKeyPair,
}

impl KeyExchange {
    /// Start a new key exchange
    pub fn new() -> Result<Self, ring::error::Unspecified> {
        Ok(Self {
            our_keypair: EphemeralKeyPair::generate()?,
        })
    }
    
    /// Get our public key to send to peer
    pub fn public_key(&self) -> &[u8] {
        self.our_keypair.public_key()
    }
    
    /// Complete the exchange with peer's public key
    pub fn complete(self, peer_public_key: &[u8]) -> Result<SessionKey, ring::error::Unspecified> {
        self.our_keypair.derive_session_key(peer_public_key, b"session")
    }
}

impl Default for KeyExchange {
    fn default() -> Self {
        Self::new().expect("Failed to generate key exchange")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_session_key_from_bytes() {
        let bytes = [42u8; 32];
        let key = SessionKey::from_bytes(bytes);
        assert_eq!(key.as_bytes(), &bytes);
    }
    
    #[test]
    fn test_ephemeral_keypair_generate() {
        let kp = EphemeralKeyPair::generate().unwrap();
        assert_eq!(kp.public_key().len(), 32);
    }
    
    #[test]
    fn test_key_exchange_both_parties() {
        // Alice starts
        let alice = KeyExchange::new().unwrap();
        let alice_public = alice.public_key().to_vec();
        
        // Bob starts
        let bob = KeyExchange::new().unwrap();
        let bob_public = bob.public_key().to_vec();
        
        // Both derive session key
        let alice_key = alice.complete(&bob_public).unwrap();
        let bob_key = bob.complete(&alice_public).unwrap();
        
        // Both should have the same key
        assert_eq!(alice_key.as_bytes(), bob_key.as_bytes());
    }
    
    #[test]
    fn test_different_peers_different_keys() {
        let alice = KeyExchange::new().unwrap();
        let bob = KeyExchange::new().unwrap();
        let charlie = KeyExchange::new().unwrap();
        
        let bob_public = bob.public_key().to_vec();
        let charlie_public = charlie.public_key().to_vec();
        
        // Alice with Bob
        let alice_bob = alice.complete(&bob_public).unwrap();
        
        // New Alice with Charlie
        let alice2 = KeyExchange::new().unwrap();
        let alice_charlie = alice2.complete(&charlie_public).unwrap();
        
        // Keys should be different
        assert_ne!(alice_bob.as_bytes(), alice_charlie.as_bytes());
    }
}
