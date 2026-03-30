//! Device discovery and NAT traversal

pub mod global;
pub mod local;
pub mod stun;

pub use global::{GlobalDiscovery, DiscoveryError, parse_addresses};
pub use local::{LocalDiscovery, Announcement, DiscoveredPeer, get_local_addresses};
pub use stun::{ExternalAddress, StunClient, DEFAULT_STUN_SERVERS};
