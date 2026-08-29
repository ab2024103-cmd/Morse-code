//! MorseLink desktop backend.
//!
//! Thin Tauri shell over the shared Rust core. Emits the same QUIC/TLS-1.3
//! protocol as the Android app, so a phone and a PC talk directly over the LAN.
//! The UI is plain HTML/CSS/JS (no framework) served in Tauri's native webview.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;

use morselink_core::discovery::{DiscoveryConfig, DiscoveryHandle, PeerAd};
use morselink_core::protocol::FileMeta;
use morselink_core::transfer::{NullSink, ServeContext, connect_and_send, serve};
use morselink_core::transport;
use tauri::{Manager, State};

/// Backend state holding the receiver task and discovery handle.
struct AppState {
    runtime: tokio::runtime::Runtime,
    serve_handle: Mutex<Option<tokio::task::JoinHandle<()>>>,
    discovery: Mutex<Option<DiscoveryHandle>>,
    receive_dir: PathBuf,
}

#[tauri::command]
async fn protocol_version() -> String {
    morselink_core::protocol_version().to_string()
}

/// Start receiving + discovery in the background.
#[tauri::command]
async fn start_receive(
    state: State<'_, AppState>,
    port: u16,
    dir: String,
    device_name: String,
) -> Result<String, String> {
    let recv_dir = PathBuf::from(dir);
    std::fs::create_dir_all(&recv_dir).map_err(|e| e.to_string())?;

    let (cert, key) = transport::generate_self_signed("morselink.local").map_err(|e| e.to_string())?;
    let server_cfg = transport::server_config(cert, key).map_err(|e| e.to_string())?;
    let endpoint = transport::bind_endpoint("0.0.0.0", port, Some(server_cfg))
        .map_err(|e| e.to_string())?;
    let node_id = morselink_core::make_node_id();

    // Discovery.
    let local_port = endpoint.local_addr().map(|a| a.port()).unwrap_or(port);
    let ad = PeerAd {
        protocol_version: morselink_core::protocol_version().to_string(),
        device_name: device_name.clone(),
        peer_id: node_id.clone(),
        port: local_port,
        addresses: vec![],
    };
    let dh = DiscoveryHandle::start(DiscoveryConfig::default(), ad).map_err(|e| e.to_string())?;
    *state.discovery.lock().unwrap() = Some(dh);

    let ctx = ServeContext {
        endpoint,
        node_id,
        device_name,
        receive_dir: recv_dir.clone(),
        sink: Arc::new(NullSink),
    };
    let handle = state.runtime.spawn(async move {
        let _ = serve(ctx).await;
    });
    *state.serve_handle.lock().unwrap() = Some(handle);

    Ok(format!("Receiving on UDP {port}, saving to {}", recv_dir.display()))
}

/// Discover currently-live peers.
#[tauri::command]
async fn list_peers(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    // Clone the handle out of the lock so we don't hold a std MutexGuard across
    // the `.await` (which would make the command future non-Send).
    let handle = { state.discovery.lock().unwrap().clone() };
    match handle {
        Some(dh) => {
            let peers = dh.live_peers().await;
            Ok(peers.iter().map(|p| format!("{} @ {}", p.device_name, p.addr)).collect())
        }
        None => Ok(vec![]),
    }
}

/// Send files to a peer `ip:port`.
#[tauri::command]
async fn send_files(
    state: State<'_, AppState>,
    port: String,
    files: Vec<String>,
) -> Result<String, String> {
    let addr = port.parse::<std::net::SocketAddr>().map_err(|e| format!("bad addr: {e}"))?;
    let endpoint = transport::bind_endpoint("0.0.0.0", 0, None).map_err(|e| e.to_string())?;

    let mut metas = Vec::new();
    let mut paths = Vec::new();
    for (i, f) in files.iter().enumerate() {
        let meta = std::fs::metadata(f).map_err(|e| format!("{}: {e}", f))?;
        metas.push(FileMeta {
            name: PathBuf::from(f).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
            size: meta.len(),
            mime: mime_for(PathBuf::from(f)),
            file_index: (i + 1) as u32,
        });
        paths.push(PathBuf::from(f));
    }

    let node_id = morselink_core::make_node_id();
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let sink = Arc::new(CliSink { events: Mutex::new(Some(tx)) });
    state.runtime.spawn(async move {
        let _ = connect_and_send(&endpoint, addr, metas, paths, node_id, "MorseLink PC".into(), None, sink).await;
    });

    Ok(format!("Sending {} file(s) to {addr}", files.len()))
}

/// Trivial progress sink so the UI keeps working (events would be piped back
/// to the frontend via Tauri events in a full implementation).
struct CliSink {
    events: Mutex<Option<std::sync::mpsc::Sender<String>>>,
}
impl morselink_core::transfer::ProgressSink for CliSink {
    fn on_progress(&self, stream_id: u64, done: u64, total: u64) {
        if let Some(tx) = self.events.lock().unwrap().as_ref() {
            let _ = tx.send(format!("stream {stream_id}: {done}/{total}"));
        }
    }
    fn on_peer_discovered(&self, peer_id: &str, peer_name: &str, addr: &str) {
        let _ = (peer_id, peer_name, addr);
    }
    fn on_transfer_complete(&self, name: &str, total: u64) {
        if let Some(tx) = self.events.lock().unwrap().as_ref() {
            let _ = tx.send(format!("complete: {name} ({total} bytes)"));
        }
    }
}

fn mime_for(path: PathBuf) -> String {
    let ext = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "jpg" | "jpeg" => "image/jpeg".into(),
        "png" => "image/png".into(),
        "mp4" | "mkv" => "video/mp4".into(),
        "mp3" | "wav" | "flac" => "audio/mpeg".into(),
        "pdf" => "application/pdf".into(),
        _ => "application/octet-stream".into(),
    }
}

#[tauri::command]
async fn stop_receive(state: State<'_, AppState>) -> Result<(), String> {
    let mut g = state.serve_handle.lock().unwrap();
    if let Some(h) = g.take() {
        h.abort();
    }
    let mut d = state.discovery.lock().unwrap();
    d.take();
    Ok(())
}

pub fn run() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("tokio runtime");

    tauri::Builder::default()
        .setup(|app| {
            let recv_dir = std::env::temp_dir().join("morselink-desktop");
            std::fs::create_dir_all(&recv_dir).ok();
            app.manage(AppState {
                runtime,
                serve_handle: Mutex::new(None),
                discovery: Mutex::new(None),
                receive_dir: recv_dir,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            protocol_version,
            start_receive,
            list_peers,
            send_files,
            stop_receive
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
