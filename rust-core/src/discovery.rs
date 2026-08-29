//! Local-network discovery.
//!
//! Two complementary mechanisms:
//!  * **UDP multicast** (primary, cross-platform) — announce + search on a
//!    fixed group/port. Works everywhere and is what the QUIC engines on all
//!    three shells speak.
//!  * **mDNS** (secondary) — advertised via `mdns-sd` so OS-native browsers and
//!    smart devices can also see the service.
//!
//! BLE advertisement is implemented natively per-OS (see Android `BluetoothLeAdvertiser`)
//! because BLE is a platform facility; the Rust core surfaces the *state* hook.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::net::UdpSocket;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::config::{DISCOVERY_MULTICAST_GROUP, DISCOVERY_MULTICAST_PORT};
#[cfg(not(target_os = "android"))]
use crate::config::MDNS_SERVICE_TYPE;
use crate::error::{EngineError, Result};

/// An advertised peer (the payload of a discovery packet).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerAd {
    pub protocol_version: String,
    pub device_name: String,
    pub peer_id: String,
    /// QUIC UDP port the peer is accepting transfers on.
    pub port: u16,
    /// One-hop LAN addresses (may be empty on some radios).
    pub addresses: Vec<String>,
}

/// A peer that has been seen recently on the network.
#[derive(Debug, Clone)]
pub struct DiscoveredPeer {
    pub peer_id: String,
    pub device_name: String,
    pub addr: SocketAddr,
    pub last_seen: std::time::Instant,
}

#[derive(Debug, Clone)]
pub struct DiscoveryConfig {
    pub group: String,
    pub port: u16,
    /// How often we broadcast our announcement.
    pub announce_interval: Duration,
    /// After this long without a packet a peer is considered gone.
    pub forget_after: Duration,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            group: DISCOVERY_MULTICAST_GROUP.to_string(),
            port: DISCOVERY_MULTICAST_PORT,
            announce_interval: Duration::from_secs(2),
            forget_after: Duration::from_secs(30),
        }
    }
}

use tokio::task::JoinHandle;

#[derive(Clone)]
pub struct DiscoveryHandle {
    config: DiscoveryConfig,
    peers: Arc<RwLock<Vec<DiscoveredPeer>>>,
    _task: Arc<JoinHandle<()>>,
}

impl DiscoveryHandle {
    /// Instantiate the service, bind the socket and spawn the background loop.
    pub fn start(config: DiscoveryConfig, ad: PeerAd) -> Result<Self> {
        let peers = Arc::new(RwLock::new(Vec::new()));
        let handle = tokio::spawn(run_loop(config.clone(), ad, peers.clone()));
        Ok(Self {
            config,
            peers,
            _task: Arc::new(handle),
        })
    }

    /// Snapshot of currently-live peers (deduplicated by peer_id).
    pub async fn live_peers(&self) -> Vec<DiscoveredPeer> {
        let now = std::time::Instant::now();
        let mut guard = self.peers.write().await;
        guard.retain(|p| now.duration_since(p.last_seen) < self.config.forget_after);
        let mut out = guard.clone();
        out.sort_by(|a, b| a.device_name.cmp(&b.device_name));
        out
    }
}

async fn run_loop(config: DiscoveryConfig, ad: PeerAd, peers: Arc<RwLock<Vec<DiscoveredPeer>>>) {
    if let Err(e) = run_loop_inner(config.clone(), ad, peers.clone()).await {
        warn!("discovery loop exited: {e}");
    }
}

async fn run_loop_inner(
    config: DiscoveryConfig,
    ad: PeerAd,
    peers: Arc<RwLock<Vec<DiscoveredPeer>>>,
) -> Result<()> {
    // Bind the socket and join the multicast group so we can both send and
    // receive on the well-known group/port.
    let bind_addr: SocketAddr = format!("0.0.0.0:{}", config.port)
        .parse()
        .map_err(|e| EngineError::Discovery(format!("bind: {e}")))?;
    let socket = UdpSocket::bind(bind_addr).await?;
    let group_ip: std::net::Ipv4Addr = config.group.parse().map_err(|e| {
        EngineError::Discovery(format!("bad multicast group {}: {e}", config.group))
    })?;
    if let Err(e) = socket.join_multicast_v4(group_ip, std::net::Ipv4Addr::UNSPECIFIED) {
        warn!("could not join multicast group: {e}");
    }
    socket.set_multicast_loop_v4(true).ok();
    socket.set_multicast_ttl_v4(1).ok();
    info!("discovery listening on group {}:{}", config.group, config.port);

    let announce_payload: Vec<u8> = serde_json::to_vec(&ad)
        .map_err(|e| EngineError::Discovery(format!("serialize ad: {e}")))?;
    let mut interval = tokio::time::interval(config.announce_interval);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    // Reusable receive buffer (bound to the loop so the read bytes are kept).
    let mut buf = [0u8; 2048];

    loop {
        // Announce.
        let _ = socket
            .send_to(&announce_payload, format!("{}:{}", config.group, config.port))
            .await;

        tokio::select! {
            _ = interval.tick() => {}
            // Poll for inbound packets; a single datagram holds one PeerAd.
            recv = socket.recv_from(&mut buf) => {
                match recv {
                    Ok((n, from)) => parse_and_track(&peers, &buf[..n], from).await,
                    Err(e) => debug!("recv error: {e}"),
                }
            }
        }
    }
}

async fn parse_and_track(
    peers: &Arc<RwLock<Vec<DiscoveredPeer>>>,
    buf: &[u8],
    from: SocketAddr,
) {
    if buf.is_empty() {
        return;
    }
    match serde_json::from_slice::<PeerAd>(buf) {
        Ok(ad) => {
            let mut guard = peers.write().await;
            if let Some(existing) = guard.iter_mut().find(|p| p.peer_id == ad.peer_id) {
                existing.last_seen = std::time::Instant::now();
                existing.addr = from;
            } else {
                debug!("discovered peer '{}' at {}", ad.device_name, from);
                guard.push(DiscoveredPeer {
                    peer_id: ad.peer_id,
                    device_name: ad.device_name,
                    addr: from,
                    last_seen: std::time::Instant::now(),
                });
            }
        }
        Err(e) => debug!("ignoring non-MorseLink multicast packet: {e}"),
    }
}

/// mDNS advertisement (secondary). Registers a `_morselink._udp.local` instance
/// so OS-native discovery tools can find the device too.
///
/// Only compiled on desktop/host targets. On Android the OS manages mDNS
/// natively (NsdManager), and `mdns-sd` pulls in `if-addrs`, which links against
/// `getifaddrs`/`freeifaddrs` symbols that are not available when targeting
/// Android — so Android gets a no-op instead.
#[cfg(not(target_os = "android"))]
pub async fn advertise_mdns(device_name: &str, port: u16) -> Result<()> {
    if !MDNS_SERVICE_TYPE.contains("_udp") {
        // defensive; never hit in practice
        return Ok(());
    }
    let service = mdns_sd::ServiceDaemon::new()
        .map_err(|e| EngineError::Discovery(format!("mdns daemon: {e}")))?;

    let host = format!("{}.local", sanitize_host(device_name));
    let props = [("peer_id", device_name), ("proto", "morselink/1.0")];
    let ty = MDNS_SERVICE_TYPE.to_string();
    let instance = format!("{}.{}", device_name.trim(), ty);

    match service.register(
        mdns_sd::ServiceInfo::new(&ty, &instance, &host, "", port, &props[..])
            .map_err(|e| EngineError::Discovery(format!("mdns service info: {e}")))?,
    ) {
        Ok(_) => info!("mDNS registered '{instance}' on port {port}"),
        Err(e) => warn!("mDNS registration failed: {e}"),
    }
    // Keep the daemon alive for the lifetime of the process; it is tied to our
    // runtime so returning here is safe (daemon runs on its own thread).
    std::mem::forget(service);
    Ok(())
}

/// Android no-op: the platform's NsdManager handles mDNS. Kept so callers
/// (`ffi.rs`, `bin/morselink.rs`) compile identically on all targets.
#[cfg(target_os = "android")]
pub async fn advertise_mdns(_device_name: &str, _port: u16) -> Result<()> {
    Ok(())
}

fn sanitize_host(name: &str) -> String {
    let clean: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '-' })
        .collect();
    if clean.is_empty() {
        "morselink".into()
    } else {
        clean
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peer_ad_serialises() {
        let ad = PeerAd {
            protocol_version: "morselink/1.0".into(),
            device_name: "Pixel".into(),
            peer_id: "abc123".into(),
            port: 5555,
            addresses: vec!["192.168.1.10".into()],
        };
        let v = serde_json::to_vec(&ad).unwrap();
        let back: PeerAd = serde_json::from_slice(&v).unwrap();
        assert_eq!(back.peer_id, "abc123");
    }

    #[test]
    fn host_sanitised() {
        assert_eq!(sanitize_host("John's Pixel 8"), "John-s-Pixel-8");
        assert_eq!(sanitize_host(""), "morselink");
    }
}
