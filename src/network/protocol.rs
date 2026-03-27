//! Wire protocol messages for P2P sync

use crate::crypto::DeviceId;
use crate::index::FileEntry;
use crate::storage::BlockHash;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Protocol version
pub const PROTOCOL_VERSION: u32 = 1;

/// Maximum message size (16 MB)
pub const MAX_MESSAGE_SIZE: usize = 16 * 1024 * 1024;

/// Protocol message types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Message {
    /// Device configuration and capabilities
    ClusterConfig(ClusterConfig),
    /// Full file index for a folder
    Index(IndexMessage),
    /// Incremental index update
    IndexUpdate(IndexUpdate),
    /// Request blocks
    Request(BlockRequest),
    /// Block data response
    Response(BlockResponse),
    /// Keepalive ping
    Ping,
    /// Keepalive pong
    Pong,
    /// Close connection
    Close(CloseReason),
}

impl Message {
    /// Get the message type id
    pub fn type_id(&self) -> u8 {
        match self {
            Message::ClusterConfig(_) => 0,
            Message::Index(_) => 1,
            Message::IndexUpdate(_) => 2,
            Message::Request(_) => 3,
            Message::Response(_) => 4,
            Message::Ping => 5,
            Message::Pong => 6,
            Message::Close(_) => 7,
        }
    }
    
    /// Get human-readable type name
    pub fn type_name(&self) -> &'static str {
        match self {
            Message::ClusterConfig(_) => "ClusterConfig",
            Message::Index(_) => "Index",
            Message::IndexUpdate(_) => "IndexUpdate",
            Message::Request(_) => "Request",
            Message::Response(_) => "Response",
            Message::Ping => "Ping",
            Message::Pong => "Pong",
            Message::Close(_) => "Close",
        }
    }
}

/// Device configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClusterConfig {
    /// Device ID
    pub device_id: DeviceId,
    /// Device name
    pub device_name: String,
    /// Protocol version
    pub version: u32,
    /// Folders this device shares
    pub folders: Vec<FolderConfig>,
}

/// Folder configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FolderConfig {
    /// Folder ID
    pub id: String,
    /// Human-readable label
    pub label: String,
    /// Read-only flag
    pub read_only: bool,
}

/// Full index message
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IndexMessage {
    /// Folder ID
    pub folder_id: String,
    /// Current sequence number
    pub sequence: u64,
    /// All file entries
    pub files: Vec<FileEntry>,
}

/// Incremental index update
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IndexUpdate {
    /// Folder ID
    pub folder_id: String,
    /// Sequence number of this update
    pub sequence: u64,
    /// Updated or new files
    pub updated: Vec<FileEntry>,
    /// Deleted file paths
    pub deleted: Vec<PathBuf>,
}

/// Block request
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BlockRequest {
    /// Request ID for matching responses
    pub request_id: u64,
    /// Folder ID
    pub folder_id: String,
    /// Block hashes to request
    pub blocks: Vec<BlockHash>,
}

/// Block response
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BlockResponse {
    /// Request ID this responds to
    pub request_id: u64,
    /// Block data (hash, data pairs)
    pub blocks: Vec<BlockData>,
    /// Blocks that weren't found
    pub not_found: Vec<BlockHash>,
}

/// Single block with data
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BlockData {
    pub hash: BlockHash,
    pub data: Vec<u8>,
}

/// Close reason
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CloseReason {
    /// Normal shutdown
    Normal,
    /// Protocol error
    ProtocolError(String),
    /// Internal error
    InternalError,
    /// Duplicate connection
    Duplicate,
}

/// Encode a message to bytes
pub fn encode(msg: &Message) -> Result<Vec<u8>, bincode::Error> {
    bincode::serialize(msg)
}

/// Decode a message from bytes
pub fn decode(data: &[u8]) -> Result<Message, bincode::Error> {
    bincode::deserialize(data)
}

/// Frame a message with length prefix
pub fn frame(msg: &Message) -> Result<Vec<u8>, bincode::Error> {
    let payload = encode(msg)?;
    let len = payload.len() as u32;
    
    let mut framed = Vec::with_capacity(4 + payload.len());
    framed.extend_from_slice(&len.to_be_bytes());
    framed.extend_from_slice(&payload);
    
    Ok(framed)
}

/// Read length prefix from framed data
pub fn read_frame_length(data: &[u8]) -> Option<usize> {
    if data.len() < 4 {
        return None;
    }
    let len = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
    Some(len as usize)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_message_type_id() {
        assert_eq!(Message::Ping.type_id(), 5);
        assert_eq!(Message::Pong.type_id(), 6);
        assert_eq!(Message::Close(CloseReason::Normal).type_id(), 7);
    }
    
    #[test]
    fn test_encode_decode_ping() {
        let msg = Message::Ping;
        let encoded = encode(&msg).unwrap();
        let decoded = decode(&encoded).unwrap();
        assert_eq!(msg, decoded);
    }
    
    #[test]
    fn test_encode_decode_cluster_config() {
        let config = ClusterConfig {
            device_id: DeviceId::from_hex(
                "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
            ).unwrap(),
            device_name: "My Device".to_string(),
            version: PROTOCOL_VERSION,
            folders: vec![
                FolderConfig {
                    id: "default".to_string(),
                    label: "Default Folder".to_string(),
                    read_only: false,
                },
            ],
        };
        let msg = Message::ClusterConfig(config.clone());
        
        let encoded = encode(&msg).unwrap();
        let decoded = decode(&encoded).unwrap();
        
        assert_eq!(msg, decoded);
    }
    
    #[test]
    fn test_encode_decode_block_request() {
        let hash = BlockHash::from_hex(
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        ).unwrap();
        
        let req = BlockRequest {
            request_id: 42,
            folder_id: "test".to_string(),
            blocks: vec![hash],
        };
        let msg = Message::Request(req);
        
        let encoded = encode(&msg).unwrap();
        let decoded = decode(&encoded).unwrap();
        
        assert_eq!(msg, decoded);
    }
    
    #[test]
    fn test_frame_message() {
        let msg = Message::Ping;
        let framed = frame(&msg).unwrap();
        
        // Should have 4-byte length prefix
        assert!(framed.len() > 4);
        
        let len = read_frame_length(&framed).unwrap();
        assert_eq!(len, framed.len() - 4);
    }
    
    #[test]
    fn test_close_reasons() {
        let reasons = vec![
            CloseReason::Normal,
            CloseReason::ProtocolError("bad message".to_string()),
            CloseReason::InternalError,
            CloseReason::Duplicate,
        ];
        
        for reason in reasons {
            let msg = Message::Close(reason.clone());
            let encoded = encode(&msg).unwrap();
            let decoded = decode(&encoded).unwrap();
            assert_eq!(Message::Close(reason), decoded);
        }
    }
}
