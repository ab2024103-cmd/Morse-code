//! MorseLink shared core engine.
//!
//! A single Rust library implements QUIC/TLS-1.3 peer-to-peer file transfer,
//! local-network discovery, and chunked multiplexed transfer. The same code is
//! consumed by:
//!   * Android (via UniFFI/JNI -> Kotlin)
//!   * Windows / macOS / Linux (Tauri, via the CLI or UniFFI)
//!   * a headless CLI for testing / scripting
//!
//! The media viewers are intentionally NOT part of this crate — they are
//! platform-native modules that only read finished files, per the architecture
//! "non-negotiable rule".

pub mod config;
pub mod discovery;
pub mod error;
pub mod protocol;
pub mod transfer;
pub mod transport;

pub use config::{DEFAULT_CHUNK_SIZE, PROTOCOL_VERSION};
pub use error::Result;
pub use transfer::{NullSink, ProgressSink};

/// The wire protocol version, as a plain string (used by scripts/logs).
pub fn protocol_version() -> &'static str {
    crate::config::PROTOCOL_VERSION
}

/// Short, stable device identifier for discovery identity.
pub fn make_node_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("morse-{:08x}", (nanos as u64) & 0xFFFF_FFFF)
}

// UniFFI (proc-macro) surface for native shells. Only compiled with `--features ffi`.
#[cfg(feature = "ffi")]
mod ffi;

#[cfg(feature = "ffi")]
uniffi::setup_scaffolding!();
