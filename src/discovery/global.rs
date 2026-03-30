//! Global discovery server client

use crate::crypto::DeviceId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

/// Default global discovery server
pub const DEFAULT_DISCOVERY_SERVER: &str = "https://discovery.syncthing.net/v2";

/// Registration interval
pub const REGISTER_INTERVAL: Duration = Duration::from_secs(1800); // 30 minutes

/// Lookup cache TTL
pub const CACHE_TTL: Duration = Duration::from_secs(300); // 5 minutes

/// Device registration request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistrationRequest {
    /// Device ID (hex-encoded)
    pub device_id: String,
    /// Addresses where device can be reached
    pub addresses: Vec<String>,
}

/// Device lookup response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LookupResponse {
    /// Addresses for the device
    pub addresses: Vec<String>,
    /// When the registration was seen
    #[serde(default)]
    pub seen: Option<String>,
}

/// Cached lookup result
#[derive(Debug, Clone)]
pub struct CachedLookup {
    /// Addresses
    pub addresses: Vec<SocketAddr>,
    /// When cached
    pub cached_at: Instant,
}

impl CachedLookup {
    /// Check if cache entry is still valid
    pub fn is_valid(&self) -> bool {
        self.cached_at.elapsed() < CACHE_TTL
    }
}

/// Global discovery client
pub struct GlobalDiscovery {
    /// Discovery server URL
    server_url: String,
    /// HTTP client
    client: reqwest::blocking::Client,
    /// Lookup cache
    cache: HashMap<DeviceId, CachedLookup>,
    /// Last registration time
    last_register: Option<Instant>,
}

impl GlobalDiscovery {
    /// Create a new global discovery client
    pub fn new() -> Self {
        Self::with_server(DEFAULT_DISCOVERY_SERVER.to_string())
    }
    
    /// Create with custom server URL
    pub fn with_server(server_url: String) -> Self {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap_or_else(|_| reqwest::blocking::Client::new());
        
        Self {
            server_url,
            client,
            cache: HashMap::new(),
            last_register: None,
        }
    }
    
    /// Register this device with the global discovery server
    pub fn register(&mut self, device_id: &DeviceId, addresses: &[SocketAddr]) -> Result<(), DiscoveryError> {
        let addr_strings: Vec<String> = addresses.iter()
            .map(|a| format!("tcp://{}", a))
            .collect();
        
        let url = format!("{}/{}", self.server_url, device_id.to_display());
        
        let response = self.client
            .post(&url)
            .json(&addr_strings)
            .send()
            .map_err(|e| DiscoveryError::Network(e.to_string()))?;
        
        if response.status().is_success() {
            self.last_register = Some(Instant::now());
            Ok(())
        } else {
            Err(DiscoveryError::Server(format!("status {}", response.status())))
        }
    }
    
    /// Check if registration is needed
    pub fn needs_register(&self) -> bool {
        match self.last_register {
            None => true,
            Some(t) => t.elapsed() > REGISTER_INTERVAL,
        }
    }
    
    /// Lookup a device by ID
    pub fn lookup(&mut self, device_id: &DeviceId) -> Result<Vec<SocketAddr>, DiscoveryError> {
        // Check cache first
        if let Some(cached) = self.cache.get(device_id) {
            if cached.is_valid() {
                return Ok(cached.addresses.clone());
            }
        }
        
        let url = format!("{}/{}", self.server_url, device_id.to_display());
        
        let response = self.client
            .get(&url)
            .send()
            .map_err(|e| DiscoveryError::Network(e.to_string()))?;
        
        if response.status().is_success() {
            let lookup: LookupResponse = response.json()
                .map_err(|e| DiscoveryError::Parse(e.to_string()))?;
            
            let addresses = parse_addresses(&lookup.addresses);
            
            // Cache the result
            self.cache.insert(device_id.clone(), CachedLookup {
                addresses: addresses.clone(),
                cached_at: Instant::now(),
            });
            
            Ok(addresses)
        } else if response.status().as_u16() == 404 {
            Err(DiscoveryError::NotFound)
        } else {
            Err(DiscoveryError::Server(format!("status {}", response.status())))
        }
    }
    
    /// Clear the lookup cache
    pub fn clear_cache(&mut self) {
        self.cache.clear();
    }
    
    /// Get cached lookup if available and valid
    pub fn get_cached(&self, device_id: &DeviceId) -> Option<Vec<SocketAddr>> {
        self.cache.get(device_id)
            .filter(|c| c.is_valid())
            .map(|c| c.addresses.clone())
    }
    
    /// Get the server URL
    pub fn server_url(&self) -> &str {
        &self.server_url
    }
}

impl Default for GlobalDiscovery {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse address strings (e.g., "tcp://192.168.1.1:22000") into SocketAddr
pub fn parse_addresses(addr_strings: &[String]) -> Vec<SocketAddr> {
    addr_strings.iter()
        .filter_map(|s| {
            let cleaned = s.strip_prefix("tcp://")
                .or_else(|| s.strip_prefix("quic://"))
                .unwrap_or(s);
            cleaned.parse().ok()
        })
        .collect()
}

/// Discovery error
#[derive(Debug, Clone)]
pub enum DiscoveryError {
    /// Network error
    Network(String),
    /// Server error
    Server(String),
    /// Parse error
    Parse(String),
    /// Device not found
    NotFound,
}

impl std::fmt::Display for DiscoveryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Network(e) => write!(f, "network error: {}", e),
            Self::Server(e) => write!(f, "server error: {}", e),
            Self::Parse(e) => write!(f, "parse error: {}", e),
            Self::NotFound => write!(f, "device not found"),
        }
    }
}

impl std::error::Error for DiscoveryError {}

#[cfg(test)]
mod tests {
    use super::*;
    
    fn test_device_id() -> DeviceId {
        DeviceId::from_bytes([2u8; 32])
    }
    
    #[test]
    fn test_parse_addresses() {
        let addr_strings = vec![
            "tcp://192.168.1.1:22000".to_string(),
            "quic://10.0.0.1:22001".to_string(),
            "127.0.0.1:22002".to_string(),
        ];
        
        let addrs = parse_addresses(&addr_strings);
        assert_eq!(addrs.len(), 3);
        assert_eq!(addrs[0].port(), 22000);
        assert_eq!(addrs[1].port(), 22001);
        assert_eq!(addrs[2].port(), 22002);
    }
    
    #[test]
    fn test_cached_lookup_validity() {
        let cached = CachedLookup {
            addresses: vec!["192.168.1.1:22000".parse().unwrap()],
            cached_at: Instant::now(),
        };
        
        assert!(cached.is_valid());
    }
    
    #[test]
    fn test_global_discovery_new() {
        let discovery = GlobalDiscovery::new();
        assert!(discovery.server_url().starts_with("https://"));
        assert!(discovery.needs_register());
    }
    
    #[test]
    fn test_global_discovery_cache() {
        let mut discovery = GlobalDiscovery::new();
        
        // Cache should be empty initially
        assert!(discovery.get_cached(&test_device_id()).is_none());
        
        // After clearing, still empty
        discovery.clear_cache();
        assert!(discovery.get_cached(&test_device_id()).is_none());
    }
}
