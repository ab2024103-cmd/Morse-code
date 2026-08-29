//! Engine configuration and protocol version negotiation.

use serde::{Deserialize, Serialize};

/// Version of the MorseLink wire protocol. Bumped whenever the framing or
/// capabilities change; peers refuse to talk across incompatible versions.
pub const PROTOCOL_VERSION: &str = "morselink/1.0";

/// Namespace used for mDNS service discovery (matches the desktop client).
pub const MDNS_SERVICE_TYPE: &str = "_morselink._udp.local.";

/// Default UDP multicast group for IPv4 link-local discovery.
pub const DISCOVERY_MULTICAST_GROUP: &str = "239.255.60.42";
pub const DISCOVERY_MULTICAST_PORT: u16 = 45842;

/// Smallest sensible chunk we hand to a QUIC stream. The transfer engine maps
/// one byte-range of a file onto one QUIC stream so the OS/QUIC multiplexer
/// (not manual thread pools) drives parallelism. QUIC multiplexes many streams
/// over a single connection, so many small ranges parallelise natively.
pub const DEFAULT_CHUNK_SIZE: u64 = 4 * 1024 * 1024; // 4 MiB
/// Streams opened concurrently per connection (upper bound).
pub const MAX_CONCURRENT_STREAMS: u32 = 32;

/// Configuration consumed by the engine. Serialisable so it can also be
/// persisted/loaded by desktop shells.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineConfig {
    /// Address the QUIC endpoint binds to.
    pub listen_addr: String,
    /// UDP port. `0` means "pick an ephemeral port".
    pub port: u16,
    /// TLS SNI name used by both sides (self-signed cert names this).
    pub server_name: String,
    /// Human-friendly name advertised via discovery.
    pub device_name: String,
    /// Whether to run the local-network discovery loop.
    pub enable_discovery: bool,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            listen_addr: "0.0.0.0".into(),
            port: 0,
            server_name: "morselink.local".into(),
            device_name: "MorseLink device".into(),
            enable_discovery: true,
        }
    }
}

impl EngineConfig {
    pub fn new() -> Self {
        Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_round_trips_through_json() {
        let cfg = EngineConfig {
            device_name: "Test device".into(),
            ..Default::default()
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let back: EngineConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.device_name, "Test device");
        assert_eq!(back.listen_addr, "0.0.0.0");
    }
}
