//! UniFFI (proc-macro) layer exposing the MorseLink engine to native shells
//! (Android/Kotlin, Tauri desktop). Compiled only when the `ffi` feature is on;
//! the CLI/host build uses the core modules directly.
//!
//! The exported types (`EngineConfig`, `EngineError`, `TransferProgress`,
//! `TransferObserver`, `TransferEngine`) and functions (`protocol_version`)
//! become the Kotlin/Swift/Python bindings via `uniffi-bindgen`.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tokio::sync::Mutex as AsyncMutex;

use crate::discovery::{DiscoveryConfig, DiscoveryHandle, PeerAd};
use crate::error::EngineError as CoreError;
use crate::transfer::{ProgressSink, ServeContext, connect_and_send, serve};
use crate::transport;

/// Configuration dictionary for a new engine (mirrors `crate::config::EngineConfig`).
#[derive(uniffi::Record)]
pub struct EngineConfig {
    pub listen_addr: String,
    pub port: u16,
    pub server_name: String,
    pub device_name: String,
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

/// Progress snapshot returned immediately from `send_file`.
#[derive(uniffi::Record)]
pub struct TransferProgress {
    pub stream_id: u64,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub fraction: f64,
    pub status: String,
}

/// FFI error type (converted from the core's `EngineError`).
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum EngineError {
    #[error("transport: {0}")]
    Transport(String),
    #[error("discovery: {0}")]
    Discovery(String),
    #[error("transfer: {0}")]
    Transfer(String),
    #[error("io: {0}")]
    Io(String),
}

impl From<CoreError> for EngineError {
    fn from(e: CoreError) -> Self {
        use crate::error::EngineError as C;
        match e {
            C::Transport(m) => EngineError::Transport(m),
            C::Discovery(m) => EngineError::Discovery(m),
            C::Transfer(m) => EngineError::Transfer(m),
            C::Io(m) => EngineError::Io(m),
        }
    }
}

/// Callback interface implemented on the native (Kotlin/Swift/JS) side and
/// invoked from the engine's worker threads.
///
/// UniFFI 0.28 uses `#[uniffi::export(with_foreign)]` for foreign-implemented
/// traits (the older `callback_interface` name does not emit the
/// `FfiConverterArc` impl needed to pass `Arc<dyn Trait>` into exported fns).
#[uniffi::export(with_foreign)]
pub trait TransferObserver: Send + Sync {
    fn on_progress(&self, stream_id: u64, bytes_done: u64, bytes_total: u64);
    fn on_peer_discovered(&self, peer_id: String, peer_name: String, addr: String);
    fn on_transfer_complete(&self, file_name: String, total_bytes: u64);
}

/// Bridges Rust `ProgressSink` events into the callback interface.
struct SinkBridge {
    observer: Mutex<Option<Arc<dyn TransferObserver>>>,
}

impl ProgressSink for SinkBridge {
    fn on_progress(&self, stream_id: u64, bytes_done: u64, bytes_total: u64) {
        if let Some(obs) = self.observer.lock().unwrap().clone() {
            obs.on_progress(stream_id, bytes_done, bytes_total);
        }
    }
    fn on_peer_discovered(&self, peer_id: &str, peer_name: &str, addr: &str) {
        if let Some(obs) = self.observer.lock().unwrap().clone() {
            obs.on_peer_discovered(peer_id.to_string(), peer_name.to_string(), addr.to_string());
        }
    }
    fn on_transfer_complete(&self, file_name: &str, total_bytes: u64) {
        if let Some(obs) = self.observer.lock().unwrap().clone() {
            obs.on_transfer_complete(file_name.to_string(), total_bytes);
        }
    }
}

/// The engine object handed across the FFI boundary.
#[derive(uniffi::Object)]
pub struct TransferEngine {
    runtime: tokio::runtime::Runtime,
    config: crate::config::EngineConfig,
    node_id: String,
    sink: Arc<SinkBridge>,
    endpoint: Option<quinn::Endpoint>,
    serve_handle: AsyncMutex<Option<tokio::task::JoinHandle<()>>>,
    discovery: AsyncMutex<Option<DiscoveryHandle>>,
    receive_dir: PathBuf,
    send_dir: PathBuf,
}

#[uniffi::export]
impl TransferEngine {
    /// Construct a new engine instance.
    #[uniffi::constructor]
    pub fn new(config: EngineConfig) -> Result<Self, EngineError> {
        let core = crate::config::EngineConfig {
            listen_addr: config.listen_addr.clone(),
            port: config.port,
            server_name: config.server_name.clone(),
            device_name: config.device_name.clone(),
            enable_discovery: config.enable_discovery,
        };
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .map_err(|e| EngineError::Io(e.to_string()))?;

        let node_id = crate::make_node_id();
        let sink = Arc::new(SinkBridge {
            observer: Mutex::new(None),
        });

        Ok(Self {
            runtime,
            config: core,
            node_id,
            sink,
            endpoint: None,
            serve_handle: AsyncMutex::new(None),
            discovery: AsyncMutex::new(None),
            receive_dir: default_receive_dir(),
            send_dir: default_send_dir(),
        })
    }

    /// Register (or clear) the observer that receives progress/discovery events.
    pub fn set_observer(&self, observer: Option<Arc<dyn TransferObserver>>) {
        *self.sink.observer.lock().unwrap() = observer;
    }

    /// Bind the QUIC endpoint, start discovery and begin accepting transfers.
    pub fn start(&self) -> Result<(), EngineError> {
        let (cert, key) = transport::generate_self_signed(&self.config.server_name)
            .map_err(|e| EngineError::Transport(e.to_string()))?;
        let server_cfg = transport::server_config(cert, key)
            .map_err(|e| EngineError::Transport(e.to_string()))?;
        let endpoint = transport::bind_endpoint(&self.config.listen_addr, self.config.port, Some(server_cfg))
            .map_err(|e| EngineError::Transport(e.to_string()))?;

        let node_id = self.node_id.clone();
        let device_name = self.config.device_name.clone();
        let receive_dir = self.receive_dir.clone();
        let sink = self.sink.clone();

        let rt = &self.runtime;
        let endpoint_clone = endpoint.clone();
        let handle = rt.block_on(async move {
            let ctx = ServeContext {
                endpoint: endpoint_clone,
                node_id,
                device_name,
                receive_dir,
                sink,
            };
            // Wrap so the JoinHandle output type is `()`, matching
            // `serve_handle: AsyncMutex<Option<JoinHandle<()>>>`.
            tokio::spawn(async move {
                if let Err(e) = serve(ctx).await {
                    tracing::warn!("serve loop stopped: {e}");
                }
            })
        });
        let local_port = endpoint.local_addr().map(|a| a.port()).unwrap_or(0);
        self.endpoint = Some(endpoint);
        {
            let mut g = rt.block_on(self.serve_handle.lock());
            *g = Some(handle);
        }

        if self.config.enable_discovery {
            let ad = PeerAd {
                protocol_version: crate::PROTOCOL_VERSION.to_string(),
                device_name: self.config.device_name.clone(),
                peer_id: self.node_id.clone(),
                port: local_port,
                addresses: Vec::new(),
            };
            let dcfg = DiscoveryConfig::default();
            let dh = rt.block_on(async move {
                match DiscoveryHandle::start(dcfg, ad) {
                    Ok(h) => {
                        let _ =
                            crate::discovery::advertise_mdns(&device_name, local_port).await;
                        h
                    }
                    Err(e) => {
                        tracing::warn!("discovery disabled: {e}");
                        DiscoveryHandle::start(DiscoveryConfig::default(), PeerAd {
                            protocol_version: crate::PROTOCOL_VERSION.into(),
                            device_name: device_name.clone(),
                            peer_id: node_id.clone(),
                            port: 0,
                            addresses: vec![],
                        })
                        .unwrap()
                    }
                }
            });
            let mut g = rt.block_on(self.discovery.lock());
            *g = Some(dh);
        }

        Ok(())
    }

    /// Queue a file for sending to `ip:port`. Returns a progress snapshot
    /// immediately; progress events flow through the observer.
    pub fn send_file(&self, dest_addr: String, path: String) -> Result<TransferProgress, EngineError> {
        let socket = dest_addr
            .parse::<std::net::SocketAddr>()
            .map_err(|e| EngineError::Transport(format!("bad address: {e}")))?;

        let file_meta = match std::fs::metadata(&path) {
            Ok(m) => crate::protocol::FileMeta {
                name: PathBuf::from(&path)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "file".into()),
                size: m.len(),
                mime: guess_mime(&path),
                file_index: 1,
            },
            Err(e) => return Err(EngineError::Io(e.to_string())),
        };

        let node_id = self.node_id.clone();
        let device_name = self.config.device_name.clone();
        let sink = self.sink.clone();
        let files = vec![file_meta];
        let paths = vec![PathBuf::from(&path)];

        let runtime = &self.runtime;
        runtime.spawn(async move {
            let endpoint = transport::bind_endpoint("0.0.0.0", 0, None)
                .map_err(CoreError::transport)?;
            let _ = connect_and_send(&endpoint, socket, files, paths, node_id, device_name, None, sink).await;
            Ok::<_, CoreError>(())
        });

        Ok(TransferProgress {
            stream_id: 0,
            bytes_done: 0,
            bytes_total: file_meta.size,
            fraction: 0.0,
            status: "queued".into(),
        })
    }

    /// Stop accepting transfers and discovery.
    pub fn shutdown(&self) {
        let rt = &self.runtime;
        rt.block_on(async move {
            let mut g = self.serve_handle.lock().await;
            if let Some(h) = g.take() {
                h.abort();
            }
            let mut d = self.discovery.lock().await;
            d.take();
        });
    }
}

/// Exported namespace function returning the protocol version string.
#[uniffi::export]
pub fn protocol_version() -> String {
    crate::PROTOCOL_VERSION.to_string()
}

// --- helpers ---------------------------------------------------------------

fn guess_mime(path: &str) -> String {
    let ext = PathBuf::from(path)
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "heic" | "heif" => "image/heic",
        "mp4" | "mkv" | "webm" | "mov" => "video/mp4",
        "mp3" | "wav" | "flac" | "aac" | "ogg" | "m4a" => "audio/mpeg",
        "pdf" => "application/pdf",
        "docx" | "doc" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xlsx" | "xls" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "pptx" | "ppt" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        "txt" => "text/plain",
        _ => "application/octet-stream",
    }
    .to_string()
}

fn default_receive_dir() -> PathBuf {
    #[cfg(target_os = "android")]
    {
        // Android shells supply a ContentResolver-backed path via config in
        // production; here we fall back to the app's external files dir.
        PathBuf::from("/sdcard/Download/MorseLink")
    }
    #[cfg(not(target_os = "android"))]
    {
        let dir = std::env::temp_dir().join("morselink-received");
        let _ = std::fs::create_dir_all(&dir);
        dir
    }
}

fn default_send_dir() -> PathBuf {
    #[cfg(target_os = "android")]
    {
        PathBuf::from("/sdcard/Download/MorseLink")
    }
    #[cfg(not(target_os = "android"))]
    {
        std::env::temp_dir().join("morselink-send")
    }
}
