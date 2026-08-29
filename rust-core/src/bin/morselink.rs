//! MorseLink headless CLI.
//!
//! Exercises the shared QUIC engine without any UI. Useful for throughput
//! testing, scripting, and as a reference for the PC app.
//!
//!   # Receive files on 0.0.0.0:45843
//!   morselink serve --port 45843 --dir ./received
//!
//!   # Send a file to a peer (address shown by `serve`)
//!   morselink send-files 192.168.1.20:45843 ./photo.jpg

use std::path::PathBuf;
use std::sync::Arc;

use clap::{Parser, Subcommand};
use morselink_core::discovery::{DiscoveryConfig, DiscoveryHandle, PeerAd};
use morselink_core::error::EngineError;
use morselink_core::protocol::FileMeta;
use morselink_core::transfer::{ProgressSink, ServeContext, connect_and_send, serve};
use morselink_core::transport;

#[derive(Parser)]
#[command(name = "morselink", about = "MorseLink P2P transfer CLI", version)]
struct Cli {
    /// Host (0.0.0.0 or a specific interface) to bind.
    #[arg(global = true, long, default_value = "0.0.0.0")]
    bind: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run the receiver: bind a QUIC endpoint and accept files.
    Serve {
        /// UDP port to listen on.
        #[arg(long, default_value_t = 45843)]
        port: u16,
        /// Directory to write received files into.
        #[arg(long, default_value = "./received")]
        dir: PathBuf,
        /// Device name advertised over discovery.
        #[arg(long, default_value = "MorseLink device")]
        name: String,
    },
    /// Send one or more files to a peer.
    SendFiles {
        /// Peer address as `ip:port`.
        addr: String,
        /// One or more files to transfer.
        files: Vec<PathBuf>,
    },
}

struct CliSink;
impl ProgressSink for CliSink {
    fn on_progress(&self, stream_id: u64, done: u64, total: u64) {
        if total > 0 {
            let pct = (done as f64 / total as f64) * 100.0;
            println!("\r  stream {stream_id}: {done}/{total} bytes ({pct:.1}%)");
        } else {
            println!("\r  stream {stream_id}: {done} bytes done");
        }
    }
    fn on_peer_discovered(&self, peer_id: &str, peer_name: &str, addr: &str) {
        println!("  [discovery] {peer_name} ({peer_id}) at {addr}");
    }
    fn on_transfer_complete(&self, name: &str, total: u64) {
        println!("\r  [complete] {name}: {total} bytes");
    }
}

fn main() -> Result<(), EngineError> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_target(false)
        .compact()
        .init();

    let cli = Cli::parse();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .map_err(|e| EngineError::Io(e.to_string()))?;

    match cli.command {
        Commands::Serve { port, dir, name } => {
            runtime.block_on(serve_cmd(cli.bind, port, dir, name))
        }
        Commands::SendFiles { addr, files } => runtime.block_on(send_cmd(cli.bind, addr, files)),
    }
}

async fn serve_cmd(bind: String, port: u16, dir: PathBuf, name: String) -> Result<(), EngineError> {
    std::fs::create_dir_all(&dir).map_err(|e| EngineError::Io(e.to_string()))?;
    let (cert, key) = transport::generate_self_signed("morselink.local")?;
    let server_cfg = transport::server_config(cert, key)?;
    let endpoint = transport::bind_endpoint(&bind, port, Some(server_cfg))?;

    let node_id = morselink_core::make_node_id();
    let sink: Arc<dyn ProgressSink> = Arc::new(CliSink);
    let addr = endpoint
        .local_addr()
        .map(|a| a.to_string())
        .unwrap_or_else(|_| format!("{bind}:{port}"));
    println!("MorseLink receiving on {addr}");
    println!("  device name : {name}");
    println!("  node id     : {node_id}");
    println!("  save to     : {}", dir.display());

    // Discovery announces only on the bound port; enable multicast search too.
    let ad = PeerAd {
        protocol_version: morselink_core::protocol_version().to_string(),
        device_name: name.clone(),
        peer_id: node_id.clone(),
        port,
        addresses: vec![],
    };
    let _dh = DiscoveryHandle::start(DiscoveryConfig::default(), ad)?;
    let _ = morselink_core::discovery::advertise_mdns(&name, port).await;

    let ctx = ServeContext {
        endpoint,
        node_id,
        device_name: name,
        receive_dir: dir,
        sink,
    };
    serve(ctx).await
}

async fn send_cmd(bind: String, addr: String, files: Vec<PathBuf>) -> Result<(), EngineError> {
    let socket = addr
        .parse::<std::net::SocketAddr>()
        .map_err(|e| EngineError::Transport(format!("bad peer address '{addr}': {e}")))?;
    let endpoint = transport::bind_endpoint(&bind, 0, None)?;

    let mut metas = Vec::new();
    let mut paths = Vec::new();
    for (i, f) in files.iter().enumerate() {
        let meta = std::fs::metadata(f)
            .map_err(|e| EngineError::Io(format!("{}: {e}", f.display())))?;
        metas.push(FileMeta {
            name: f
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            size: meta.len(),
            mime: mime_for(f),
            file_index: (i + 1) as u32,
        });
        paths.push(f.clone());
    }

    let sink: Arc<dyn ProgressSink> = Arc::new(CliSink);
    connect_and_send(
        &endpoint,
        socket,
        metas,
        paths,
        morselink_core::make_node_id(),
        "CLI sender".into(),
        None,
        sink,
    )
    .await?;
    println!("Done.");
    Ok(())
}

fn mime_for(path: &PathBuf) -> String {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "jpg" | "jpeg" => "image/jpeg".into(),
        "png" => "image/png".into(),
        "webp" => "image/webp".into(),
        "mp4" | "mkv" | "webm" => "video/mp4".into(),
        "mp3" | "wav" | "flac" => "audio/mpeg".into(),
        "pdf" => "application/pdf".into(),
        _ => "application/octet-stream".into(),
    }
}
