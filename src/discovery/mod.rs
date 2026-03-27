//! Device discovery and NAT traversal

pub mod stun;

pub use stun::{ExternalAddress, StunClient, DEFAULT_STUN_SERVERS};
