//! P2P networking and protocol

pub mod connection;
pub mod protocol;

pub use connection::{Connection, ConnectionManager, ConnectionState, ConnectionStats};
pub use protocol::{
    decode, encode, frame, read_frame_length,
    BlockData, BlockRequest, BlockResponse, CloseReason, ClusterConfig,
    FolderConfig, IndexMessage, IndexUpdate, Message, PROTOCOL_VERSION,
};
