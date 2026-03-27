//! STUN client for NAT traversal

use std::io;
use std::net::{SocketAddr, UdpSocket};
use std::time::Duration;

/// Default STUN servers
pub const DEFAULT_STUN_SERVERS: &[&str] = &[
    "stun.l.google.com:19302",
    "stun1.l.google.com:19302",
    "stun.cloudflare.com:3478",
];

/// STUN message type
const BINDING_REQUEST: u16 = 0x0001;
const BINDING_RESPONSE: u16 = 0x0101;

/// STUN attribute types
const ATTR_MAPPED_ADDRESS: u16 = 0x0001;
const ATTR_XOR_MAPPED_ADDRESS: u16 = 0x0020;

/// STUN magic cookie
const MAGIC_COOKIE: u32 = 0x2112A442;

/// External address result
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalAddress {
    /// Our external IP and port
    pub address: SocketAddr,
    /// STUN server used
    pub server: String,
}

/// STUN client for discovering external address
pub struct StunClient {
    /// Timeout for STUN requests
    timeout: Duration,
    /// STUN servers to try
    servers: Vec<String>,
}

impl StunClient {
    /// Create with default settings
    pub fn new() -> Self {
        Self {
            timeout: Duration::from_secs(3),
            servers: DEFAULT_STUN_SERVERS.iter().map(|s| s.to_string()).collect(),
        }
    }
    
    /// Create with custom servers
    pub fn with_servers(servers: Vec<String>) -> Self {
        Self {
            timeout: Duration::from_secs(3),
            servers,
        }
    }
    
    /// Set timeout
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
    
    /// Discover external address using STUN
    pub fn discover(&self) -> io::Result<ExternalAddress> {
        let socket = UdpSocket::bind("0.0.0.0:0")?;
        socket.set_read_timeout(Some(self.timeout))?;
        
        for server in &self.servers {
            if let Ok(addr) = self.query_server(&socket, server) {
                return Ok(ExternalAddress {
                    address: addr,
                    server: server.clone(),
                });
            }
        }
        
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Failed to discover external address from any STUN server",
        ))
    }
    
    /// Query a single STUN server
    fn query_server(&self, socket: &UdpSocket, server: &str) -> io::Result<SocketAddr> {
        use std::net::ToSocketAddrs;
        
        let server_addr = server.to_socket_addrs()?.next().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "Invalid STUN server address")
        })?;
        
        // Build STUN binding request
        let transaction_id: [u8; 12] = rand_transaction_id();
        let request = build_binding_request(&transaction_id);
        
        // Send request
        socket.send_to(&request, server_addr)?;
        
        // Receive response
        let mut buf = [0u8; 1024];
        let (len, _from) = socket.recv_from(&mut buf)?;
        
        // Parse response
        parse_binding_response(&buf[..len], &transaction_id)
    }
}

impl Default for StunClient {
    fn default() -> Self {
        Self::new()
    }
}

/// Build a STUN binding request
fn build_binding_request(transaction_id: &[u8; 12]) -> Vec<u8> {
    let mut msg = Vec::with_capacity(20);
    
    // Message type: Binding Request
    msg.extend_from_slice(&BINDING_REQUEST.to_be_bytes());
    
    // Message length (no attributes)
    msg.extend_from_slice(&0u16.to_be_bytes());
    
    // Magic cookie
    msg.extend_from_slice(&MAGIC_COOKIE.to_be_bytes());
    
    // Transaction ID (12 bytes)
    msg.extend_from_slice(transaction_id);
    
    msg
}

/// Parse a STUN binding response
fn parse_binding_response(data: &[u8], expected_txn: &[u8; 12]) -> io::Result<SocketAddr> {
    if data.len() < 20 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "Response too short"));
    }
    
    // Check message type
    let msg_type = u16::from_be_bytes([data[0], data[1]]);
    if msg_type != BINDING_RESPONSE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Unexpected message type: {:#06x}", msg_type),
        ));
    }
    
    // Check magic cookie
    let cookie = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
    if cookie != MAGIC_COOKIE {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "Invalid magic cookie"));
    }
    
    // Check transaction ID
    if &data[8..20] != expected_txn {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "Transaction ID mismatch"));
    }
    
    // Parse attributes
    let msg_len = u16::from_be_bytes([data[2], data[3]]) as usize;
    let attrs_end = 20 + msg_len.min(data.len() - 20);
    let mut pos = 20;
    
    while pos + 4 <= attrs_end {
        let attr_type = u16::from_be_bytes([data[pos], data[pos + 1]]);
        let attr_len = u16::from_be_bytes([data[pos + 2], data[pos + 3]]) as usize;
        pos += 4;
        
        if pos + attr_len > attrs_end {
            break;
        }
        
        match attr_type {
            ATTR_XOR_MAPPED_ADDRESS => {
                if let Some(addr) = parse_xor_mapped_address(&data[pos..pos + attr_len]) {
                    return Ok(addr);
                }
            }
            ATTR_MAPPED_ADDRESS => {
                if let Some(addr) = parse_mapped_address(&data[pos..pos + attr_len]) {
                    return Ok(addr);
                }
            }
            _ => {}
        }
        
        // Move to next attribute (4-byte aligned)
        pos += (attr_len + 3) & !3;
    }
    
    Err(io::Error::new(io::ErrorKind::NotFound, "No mapped address in response"))
}

/// Parse XOR-MAPPED-ADDRESS attribute
fn parse_xor_mapped_address(data: &[u8]) -> Option<SocketAddr> {
    if data.len() < 8 {
        return None;
    }
    
    let family = data[1];
    let xport = u16::from_be_bytes([data[2], data[3]]) ^ (MAGIC_COOKIE >> 16) as u16;
    
    match family {
        0x01 => {
            // IPv4
            let xaddr = u32::from_be_bytes([data[4], data[5], data[6], data[7]]) ^ MAGIC_COOKIE;
            let ip = std::net::Ipv4Addr::from(xaddr);
            Some(SocketAddr::new(ip.into(), xport))
        }
        0x02 => {
            // IPv6 (need 20 bytes)
            if data.len() < 20 {
                return None;
            }
            let mut addr_bytes = [0u8; 16];
            addr_bytes.copy_from_slice(&data[4..20]);
            // XOR with magic cookie + transaction ID (simplified)
            let cookie_bytes = MAGIC_COOKIE.to_be_bytes();
            for i in 0..4 {
                addr_bytes[i] ^= cookie_bytes[i];
            }
            let ip = std::net::Ipv6Addr::from(addr_bytes);
            Some(SocketAddr::new(ip.into(), xport))
        }
        _ => None,
    }
}

/// Parse MAPPED-ADDRESS attribute (non-XOR)
fn parse_mapped_address(data: &[u8]) -> Option<SocketAddr> {
    if data.len() < 8 {
        return None;
    }
    
    let family = data[1];
    let port = u16::from_be_bytes([data[2], data[3]]);
    
    match family {
        0x01 => {
            // IPv4
            let ip = std::net::Ipv4Addr::new(data[4], data[5], data[6], data[7]);
            Some(SocketAddr::new(ip.into(), port))
        }
        0x02 => {
            // IPv6
            if data.len() < 20 {
                return None;
            }
            let mut addr_bytes = [0u8; 16];
            addr_bytes.copy_from_slice(&data[4..20]);
            let ip = std::net::Ipv6Addr::from(addr_bytes);
            Some(SocketAddr::new(ip.into(), port))
        }
        _ => None,
    }
}

/// Generate random transaction ID
fn rand_transaction_id() -> [u8; 12] {
    use std::time::{SystemTime, UNIX_EPOCH};
    
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    
    let mut id = [0u8; 12];
    id[0..8].copy_from_slice(&now.to_le_bytes()[0..8]);
    id[8..12].copy_from_slice(&(std::process::id() as u32).to_le_bytes());
    id
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_build_binding_request() {
        let txn_id = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
        let request = build_binding_request(&txn_id);
        
        assert_eq!(request.len(), 20);
        // Message type
        assert_eq!(request[0..2], [0x00, 0x01]);
        // Length
        assert_eq!(request[2..4], [0x00, 0x00]);
        // Magic cookie
        assert_eq!(request[4..8], [0x21, 0x12, 0xA4, 0x42]);
        // Transaction ID
        assert_eq!(&request[8..20], &txn_id);
    }
    
    #[test]
    fn test_parse_mapped_address_v4() {
        // Family 0x01, port 0x1234, IP 192.168.1.1
        let data = [0x00, 0x01, 0x12, 0x34, 192, 168, 1, 1];
        let addr = parse_mapped_address(&data).unwrap();
        
        assert_eq!(addr.port(), 0x1234);
        assert!(addr.ip().is_ipv4());
    }
    
    #[test]
    fn test_parse_xor_mapped_address_v4() {
        // Port XOR'd with magic cookie high bytes, IP XOR'd with magic cookie
        let port: u16 = 12345 ^ (MAGIC_COOKIE >> 16) as u16;
        let ip: u32 = u32::from_be_bytes([93, 184, 216, 34]) ^ MAGIC_COOKIE;
        let ip_bytes = ip.to_be_bytes();
        
        let data = [0x00, 0x01, (port >> 8) as u8, port as u8, 
                    ip_bytes[0], ip_bytes[1], ip_bytes[2], ip_bytes[3]];
        
        let addr = parse_xor_mapped_address(&data).unwrap();
        assert_eq!(addr.port(), 12345);
    }
    
    #[test]
    fn test_stun_client_new() {
        let client = StunClient::new();
        assert_eq!(client.servers.len(), 3);
        assert_eq!(client.timeout, Duration::from_secs(3));
    }
}
