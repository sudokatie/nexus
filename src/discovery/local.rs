//! Local network discovery using mDNS-like UDP multicast

use crate::crypto::DeviceId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Multicast group for local discovery
pub const MULTICAST_GROUP: Ipv4Addr = Ipv4Addr::new(224, 0, 0, 251);

/// Default discovery port
pub const DISCOVERY_PORT: u16 = 21027;

/// Announcement interval
pub const ANNOUNCE_INTERVAL: Duration = Duration::from_secs(30);

/// Peer expiry time (no announcement heard)
pub const PEER_EXPIRY: Duration = Duration::from_secs(120);

/// Service name for nexus
pub const SERVICE_NAME: &str = "_nexus._udp.local";

/// Announcement message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Announcement {
    /// Device ID
    pub device_id: DeviceId,
    /// Device name
    pub name: String,
    /// Addresses where the device can be reached
    pub addresses: Vec<SocketAddr>,
    /// Protocol version
    pub version: u8,
}

impl Announcement {
    /// Create a new announcement
    pub fn new(device_id: DeviceId, name: String, addresses: Vec<SocketAddr>) -> Self {
        Self {
            device_id,
            name,
            addresses,
            version: 1,
        }
    }
    
    /// Serialize to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        bincode::serialize(self).unwrap_or_default()
    }
    
    /// Deserialize from bytes
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        bincode::deserialize(data).ok()
    }
}

/// Discovered peer info
#[derive(Debug, Clone)]
pub struct DiscoveredPeer {
    /// Device ID
    pub device_id: DeviceId,
    /// Device name
    pub name: String,
    /// Addresses
    pub addresses: Vec<SocketAddr>,
    /// Source address where we received the announcement
    pub source: SocketAddr,
    /// Last seen time
    pub last_seen: Instant,
}

impl DiscoveredPeer {
    /// Check if peer has expired
    pub fn is_expired(&self) -> bool {
        self.last_seen.elapsed() > PEER_EXPIRY
    }
}

/// Local discovery service
pub struct LocalDiscovery {
    /// Our device ID
    device_id: DeviceId,
    /// Our device name
    name: String,
    /// Addresses we advertise
    addresses: Vec<SocketAddr>,
    /// Discovered peers
    peers: Arc<Mutex<HashMap<DeviceId, DiscoveredPeer>>>,
    /// Socket for sending/receiving
    socket: Option<UdpSocket>,
}

impl LocalDiscovery {
    /// Create a new local discovery instance
    pub fn new(device_id: DeviceId, name: String, addresses: Vec<SocketAddr>) -> Self {
        Self {
            device_id,
            name,
            addresses,
            peers: Arc::new(Mutex::new(HashMap::new())),
            socket: None,
        }
    }
    
    /// Bind to multicast socket
    pub fn bind(&mut self) -> std::io::Result<()> {
        let socket = UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, DISCOVERY_PORT))?;
        socket.set_nonblocking(true)?;
        socket.join_multicast_v4(&MULTICAST_GROUP, &Ipv4Addr::UNSPECIFIED)?;
        socket.set_multicast_loop_v4(false)?;
        self.socket = Some(socket);
        Ok(())
    }
    
    /// Send announcement to local network
    pub fn announce(&self) -> std::io::Result<()> {
        let socket = self.socket.as_ref()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotConnected, "not bound"))?;
        
        let announcement = Announcement::new(
            self.device_id.clone(),
            self.name.clone(),
            self.addresses.clone(),
        );
        
        let data = announcement.to_bytes();
        let target = SocketAddrV4::new(MULTICAST_GROUP, DISCOVERY_PORT);
        socket.send_to(&data, target)?;
        
        Ok(())
    }
    
    /// Receive and process announcements
    pub fn receive(&self) -> std::io::Result<Option<DiscoveredPeer>> {
        let socket = self.socket.as_ref()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotConnected, "not bound"))?;
        
        let mut buf = [0u8; 4096];
        match socket.recv_from(&mut buf) {
            Ok((len, source)) => {
                if let Some(announcement) = Announcement::from_bytes(&buf[..len]) {
                    // Ignore our own announcements
                    if announcement.device_id == self.device_id {
                        return Ok(None);
                    }
                    
                    let peer = DiscoveredPeer {
                        device_id: announcement.device_id.clone(),
                        name: announcement.name,
                        addresses: announcement.addresses,
                        source,
                        last_seen: Instant::now(),
                    };
                    
                    // Update peer map
                    self.peers.lock().unwrap().insert(announcement.device_id, peer.clone());
                    
                    return Ok(Some(peer));
                }
                Ok(None)
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
            Err(e) => Err(e),
        }
    }
    
    /// Get all discovered peers
    pub fn peers(&self) -> Vec<DiscoveredPeer> {
        let mut peers = self.peers.lock().unwrap();
        
        // Remove expired peers
        peers.retain(|_, peer| !peer.is_expired());
        
        peers.values().cloned().collect()
    }
    
    /// Get a specific peer by device ID
    pub fn get_peer(&self, device_id: &DeviceId) -> Option<DiscoveredPeer> {
        let peers = self.peers.lock().unwrap();
        peers.get(device_id).filter(|p| !p.is_expired()).cloned()
    }
    
    /// Clear all discovered peers
    pub fn clear(&self) {
        self.peers.lock().unwrap().clear();
    }
    
    /// Get number of known peers
    pub fn peer_count(&self) -> usize {
        let mut peers = self.peers.lock().unwrap();
        peers.retain(|_, peer| !peer.is_expired());
        peers.len()
    }
}

/// Parse local IP addresses from the system
pub fn get_local_addresses(port: u16) -> Vec<SocketAddr> {
    let mut addresses = Vec::new();
    
    // Try to get local addresses by connecting to a public address
    // This gives us the interface that would be used for routing
    if let Ok(socket) = UdpSocket::bind("0.0.0.0:0") {
        if socket.connect("8.8.8.8:80").is_ok() {
            if let Ok(addr) = socket.local_addr() {
                if let SocketAddr::V4(v4) = addr {
                    addresses.push(SocketAddr::V4(SocketAddrV4::new(*v4.ip(), port)));
                }
            }
        }
    }
    
    addresses
}

#[cfg(test)]
mod tests {
    use super::*;
    
    fn test_device_id() -> DeviceId {
        DeviceId::from_bytes([1u8; 32])
    }
    
    #[test]
    fn test_announcement_serialize() {
        let ann = Announcement::new(
            test_device_id(),
            "test-device".to_string(),
            vec!["192.168.1.100:22000".parse().unwrap()],
        );
        
        let bytes = ann.to_bytes();
        assert!(!bytes.is_empty());
        
        let parsed = Announcement::from_bytes(&bytes).unwrap();
        assert_eq!(parsed.device_id, ann.device_id);
        assert_eq!(parsed.name, ann.name);
        assert_eq!(parsed.addresses.len(), 1);
    }
    
    #[test]
    fn test_discovered_peer_expiry() {
        let peer = DiscoveredPeer {
            device_id: test_device_id(),
            name: "test".to_string(),
            addresses: vec![],
            source: "127.0.0.1:22000".parse().unwrap(),
            last_seen: Instant::now(),
        };
        
        assert!(!peer.is_expired());
    }
    
    #[test]
    fn test_local_discovery_new() {
        let discovery = LocalDiscovery::new(
            test_device_id(),
            "my-device".to_string(),
            vec!["192.168.1.50:22000".parse().unwrap()],
        );
        
        assert_eq!(discovery.peer_count(), 0);
    }
    
    #[test]
    fn test_get_local_addresses() {
        let addrs = get_local_addresses(22000);
        // May or may not return addresses depending on network config
        // Just verify it doesn't panic
        for addr in &addrs {
            assert_eq!(addr.port(), 22000);
        }
    }
}
