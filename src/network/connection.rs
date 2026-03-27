//! Connection management for P2P networking

use crate::crypto::DeviceId;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// Connection state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    /// Connecting to peer
    Connecting,
    /// Connected and ready
    Connected,
    /// Disconnecting
    Disconnecting,
    /// Disconnected
    Disconnected,
}

/// Connection statistics
#[derive(Debug, Clone, Default)]
pub struct ConnectionStats {
    /// Bytes sent
    pub bytes_sent: u64,
    /// Bytes received
    pub bytes_received: u64,
    /// Messages sent
    pub messages_sent: u64,
    /// Messages received
    pub messages_received: u64,
    /// Round-trip time estimate
    pub rtt: Option<Duration>,
}

/// A connection to a peer
#[derive(Debug)]
pub struct Connection {
    /// Remote device ID
    device_id: DeviceId,
    /// Remote address
    address: SocketAddr,
    /// Connection state
    state: ConnectionState,
    /// When connection was established
    connected_at: Option<Instant>,
    /// Last activity time
    last_activity: Instant,
    /// Connection statistics
    stats: ConnectionStats,
}

impl Connection {
    /// Create a new connection
    pub fn new(device_id: DeviceId, address: SocketAddr) -> Self {
        Self {
            device_id,
            address,
            state: ConnectionState::Connecting,
            connected_at: None,
            last_activity: Instant::now(),
            stats: ConnectionStats::default(),
        }
    }
    
    /// Get the device ID
    pub fn device_id(&self) -> &DeviceId {
        &self.device_id
    }
    
    /// Get the remote address
    pub fn address(&self) -> SocketAddr {
        self.address
    }
    
    /// Get current state
    pub fn state(&self) -> ConnectionState {
        self.state
    }
    
    /// Check if connected
    pub fn is_connected(&self) -> bool {
        self.state == ConnectionState::Connected
    }
    
    /// Get connection duration
    pub fn connected_duration(&self) -> Option<Duration> {
        self.connected_at.map(|t| t.elapsed())
    }
    
    /// Get time since last activity
    pub fn idle_duration(&self) -> Duration {
        self.last_activity.elapsed()
    }
    
    /// Get statistics
    pub fn stats(&self) -> &ConnectionStats {
        &self.stats
    }
    
    /// Mark as connected
    pub fn set_connected(&mut self) {
        self.state = ConnectionState::Connected;
        self.connected_at = Some(Instant::now());
        self.last_activity = Instant::now();
    }
    
    /// Mark as disconnected
    pub fn set_disconnected(&mut self) {
        self.state = ConnectionState::Disconnected;
    }
    
    /// Record sent data
    pub fn record_sent(&mut self, bytes: u64) {
        self.stats.bytes_sent += bytes;
        self.stats.messages_sent += 1;
        self.last_activity = Instant::now();
    }
    
    /// Record received data
    pub fn record_received(&mut self, bytes: u64) {
        self.stats.bytes_received += bytes;
        self.stats.messages_received += 1;
        self.last_activity = Instant::now();
    }
    
    /// Update RTT estimate
    pub fn update_rtt(&mut self, rtt: Duration) {
        self.stats.rtt = Some(rtt);
    }
}

/// Manages multiple connections
pub struct ConnectionManager {
    /// Active connections by device ID
    connections: HashMap<DeviceId, Connection>,
    /// Maximum concurrent connections
    max_connections: usize,
    /// Connection timeout
    timeout: Duration,
    /// Next request ID
    next_request_id: AtomicU64,
}

impl ConnectionManager {
    /// Create a new connection manager
    pub fn new() -> Self {
        Self {
            connections: HashMap::new(),
            max_connections: 100,
            timeout: Duration::from_secs(60),
            next_request_id: AtomicU64::new(1),
        }
    }
    
    /// Create with custom limits
    pub fn with_limits(max_connections: usize, timeout: Duration) -> Self {
        Self {
            connections: HashMap::new(),
            max_connections,
            timeout,
            next_request_id: AtomicU64::new(1),
        }
    }
    
    /// Get next request ID
    pub fn next_request_id(&self) -> u64 {
        self.next_request_id.fetch_add(1, Ordering::Relaxed)
    }
    
    /// Add a new connection
    pub fn add(&mut self, conn: Connection) -> bool {
        if self.connections.len() >= self.max_connections {
            return false;
        }
        
        let device_id = conn.device_id().clone();
        self.connections.insert(device_id, conn);
        true
    }
    
    /// Get a connection by device ID
    pub fn get(&self, device_id: &DeviceId) -> Option<&Connection> {
        self.connections.get(device_id)
    }
    
    /// Get a mutable connection by device ID
    pub fn get_mut(&mut self, device_id: &DeviceId) -> Option<&mut Connection> {
        self.connections.get_mut(device_id)
    }
    
    /// Remove a connection
    pub fn remove(&mut self, device_id: &DeviceId) -> Option<Connection> {
        self.connections.remove(device_id)
    }
    
    /// Check if connected to a device
    pub fn is_connected(&self, device_id: &DeviceId) -> bool {
        self.connections
            .get(device_id)
            .map(|c| c.is_connected())
            .unwrap_or(false)
    }
    
    /// Get all connected device IDs
    pub fn connected_devices(&self) -> Vec<DeviceId> {
        self.connections
            .iter()
            .filter(|(_, c)| c.is_connected())
            .map(|(id, _)| id.clone())
            .collect()
    }
    
    /// Number of connections
    pub fn connection_count(&self) -> usize {
        self.connections.len()
    }
    
    /// Number of connected peers
    pub fn connected_count(&self) -> usize {
        self.connections
            .values()
            .filter(|c| c.is_connected())
            .count()
    }
    
    /// Remove idle connections
    pub fn cleanup_idle(&mut self) -> Vec<DeviceId> {
        let timeout = self.timeout;
        let idle: Vec<DeviceId> = self.connections
            .iter()
            .filter(|(_, c)| c.idle_duration() > timeout)
            .map(|(id, _)| id.clone())
            .collect();
        
        for id in &idle {
            self.connections.remove(id);
        }
        
        idle
    }
    
    /// Iterate over all connections
    pub fn iter(&self) -> impl Iterator<Item = (&DeviceId, &Connection)> {
        self.connections.iter()
    }
}

impl Default for ConnectionManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};
    
    fn test_device_id(n: u8) -> DeviceId {
        DeviceId::from_bytes([n; 32])
    }
    
    fn test_addr(port: u16) -> SocketAddr {
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), port)
    }
    
    #[test]
    fn test_connection_new() {
        let conn = Connection::new(test_device_id(1), test_addr(8000));
        
        assert_eq!(conn.state(), ConnectionState::Connecting);
        assert!(!conn.is_connected());
        assert!(conn.connected_duration().is_none());
    }
    
    #[test]
    fn test_connection_lifecycle() {
        let mut conn = Connection::new(test_device_id(1), test_addr(8000));
        
        assert_eq!(conn.state(), ConnectionState::Connecting);
        
        conn.set_connected();
        assert_eq!(conn.state(), ConnectionState::Connected);
        assert!(conn.is_connected());
        assert!(conn.connected_duration().is_some());
        
        conn.set_disconnected();
        assert_eq!(conn.state(), ConnectionState::Disconnected);
        assert!(!conn.is_connected());
    }
    
    #[test]
    fn test_connection_stats() {
        let mut conn = Connection::new(test_device_id(1), test_addr(8000));
        conn.set_connected();
        
        conn.record_sent(100);
        conn.record_received(200);
        conn.record_sent(50);
        
        assert_eq!(conn.stats().bytes_sent, 150);
        assert_eq!(conn.stats().bytes_received, 200);
        assert_eq!(conn.stats().messages_sent, 2);
        assert_eq!(conn.stats().messages_received, 1);
    }
    
    #[test]
    fn test_connection_manager_add_get() {
        let mut mgr = ConnectionManager::new();
        
        let conn = Connection::new(test_device_id(1), test_addr(8000));
        assert!(mgr.add(conn));
        
        assert_eq!(mgr.connection_count(), 1);
        assert!(mgr.get(&test_device_id(1)).is_some());
        assert!(mgr.get(&test_device_id(2)).is_none());
    }
    
    #[test]
    fn test_connection_manager_max_connections() {
        let mut mgr = ConnectionManager::with_limits(2, Duration::from_secs(60));
        
        assert!(mgr.add(Connection::new(test_device_id(1), test_addr(8001))));
        assert!(mgr.add(Connection::new(test_device_id(2), test_addr(8002))));
        assert!(!mgr.add(Connection::new(test_device_id(3), test_addr(8003))));
        
        assert_eq!(mgr.connection_count(), 2);
    }
    
    #[test]
    fn test_connection_manager_connected_devices() {
        let mut mgr = ConnectionManager::new();
        
        let mut conn1 = Connection::new(test_device_id(1), test_addr(8001));
        conn1.set_connected();
        
        let conn2 = Connection::new(test_device_id(2), test_addr(8002));
        // conn2 stays in Connecting state
        
        mgr.add(conn1);
        mgr.add(conn2);
        
        let connected = mgr.connected_devices();
        assert_eq!(connected.len(), 1);
        assert_eq!(connected[0], test_device_id(1));
    }
}
