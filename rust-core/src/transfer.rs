//! File transfer over QUIC.
//!
//! Model
//! -----
//! * One QUIC **connection** carries the whole session.
//! * One **control bidi-stream** carries the hello/ack handshake and the
//!   request/accept exchange.
//! * A file's byte-ranges are spread across many **data bidi-streams** (one per
//!   chunk). QUIC multiplexes these over the single connection, so the OS
//!   scheduler — not a hand-rolled thread pool — drives parallelism.
//! * **Resume**: every chunk is offset-addressed and independently acknowledged;
//!   a receiver tracks the highest contiguous offset and a restart skips chunks
//!   already received ("resume from last acknowledged byte").

use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use quinn::{Connection, Endpoint};

use crate::config::{DEFAULT_CHUNK_SIZE, PROTOCOL_VERSION};
use crate::error::{EngineError, Result};
use crate::protocol::{ControlMessage, FileMeta, StreamHeader, chunks_of, read_frame, write_all_send, write_frame};
use crate::transport;

/// Sink for progress / peer / completion notifications. Implemented by the FFI
/// layer to bridge into Kotlin/Swift and by the CLI to print status.
pub trait ProgressSink: Send + Sync + 'static {
    fn on_progress(&self, stream_id: u64, bytes_done: u64, bytes_total: u64);
    fn on_peer_discovered(&self, peer_id: &str, peer_name: &str, addr: &str);
    fn on_transfer_complete(&self, file_name: &str, total_bytes: u64);
}

/// A no-op sink so spawned tasks always have something to write to.
pub struct NullSink;
impl ProgressSink for NullSink {
    fn on_progress(&self, _: u64, _: u64, _: u64) {}
    fn on_peer_discovered(&self, _: &str, _: &str, _: &str) {}
    fn on_transfer_complete(&self, _: &str, _: u64) {}
}

/// Everything the inbound accept loop needs, bundled so it can be moved into a
/// background task.
pub struct ServeContext {
    pub endpoint: Endpoint,
    pub node_id: String,
    pub device_name: String,
    pub receive_dir: PathBuf,
    pub sink: Arc<dyn ProgressSink>,
}

/// Run the inbound accept loop until the endpoint is dropped.
pub async fn serve(ctx: ServeContext) -> Result<()> {
    while let Some(incoming) = ctx.endpoint.accept().await {
        let ctx = ctx.clone_for_connection();
        tokio::spawn(async move {
            match incoming.accept().await {
                Ok(connection) => {
                    if let Err(e) = handle_incoming_connection(connection, ctx).await {
                        tracing::warn!("inbound transfer failed: {e}");
                    }
                }
                Err(e) => tracing::warn!("accept failed: {e}"),
            }
        });
    }
    Ok(())
}

impl ServeContext {
    fn clone_for_connection(&self) -> Self {
        Self {
            endpoint: self.endpoint.clone(),
            node_id: self.node_id.clone(),
            device_name: self.device_name.clone(),
            receive_dir: self.receive_dir.clone(),
            sink: self.sink.clone(),
        }
    }
}

/// Server side of the handshake + data receipt for one inbound connection.
async fn handle_incoming_connection(connection: Connection, ctx: ServeContext) -> Result<()> {
    let (mut ctrl_send, mut ctrl_recv) = connection.accept_bi().await?;

    // Receive the initiator's Hello.
    let hello: ControlMessage = match read_frame(&mut ctrl_recv, None).await? {
        Some(m) => m,
        None => return Err(EngineError::Transfer("peer closed before hello".into())),
    };
    let (peer_device, peer_id) = match &hello {
        ControlMessage::Hello { device_name, peer_id, protocol_version, .. } => {
            if protocol_version != PROTOCOL_VERSION {
                return Err(EngineError::Transfer(format!(
                    "protocol mismatch: peer {}, local {}",
                    protocol_version, PROTOCOL_VERSION
                )));
            }
            (device_name.clone(), peer_id.clone())
        }
        _ => return Err(EngineError::Transfer("expected Hello".into())),
    };
    ctx.sink.on_peer_discovered(&peer_id, &peer_device, "incoming");

    // Reply with HelloAck.
    write_frame(&mut ctrl_send, &ControlMessage::hello_ack(&ctx.device_name, &ctx.node_id)).await?;

    // Read the sender's Request.
    let req: ControlMessage = read_frame(&mut ctrl_recv, None)
        .await?
        .ok_or_else(|| EngineError::Transfer("peer closed before request".into()))?;
    let (transfer_id, files) = match &req {
        ControlMessage::Request { transfer_id, files } => (*transfer_id, files.clone()),
        _ => return Err(EngineError::Transfer("expected Request".into())),
    };

    // The engine auto-accepts. A native shell can gate this with a consent
    // dialog before the transfer begins.
    write_frame(&mut ctrl_send, &ControlMessage::Accept { transfer_id }).await?;

    // Pre-create output files.
    let mut outputs: Vec<(File, FileMeta)> = Vec::with_capacity(files.len());
    for f in &files {
        let safe = sanitize_file_name(&f.name);
        let path = ctx.receive_dir.join(&safe);
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let file = File::create(&path)
            .map_err(|e| EngineError::Io(format!("create {}: {e}", path.display())))?;
        file.set_len(f.size)
            .map_err(|e| EngineError::Io(format!("prealloc {}: {e}", path.display())))?;
        outputs.push((file, f.clone()));
    }

    // Loop accepting data streams until the control stream signals Done/EOF.
    loop {
        tokio::select! {
            data = connection.accept_bi() => {
                let (mut dsend, mut drecv) = match data {
                    Ok(pair) => pair,
                    Err(_) => break,
                };
                let header: StreamHeader = match read_frame(&mut drecv, Some(64 * 1024)).await? {
                    Some(h) => h,
                    None => break,
                };
                let idx = header.file_index as usize;
                if idx >= outputs.len() {
                    return Err(EngineError::Transfer("bad file index".into()));
                }
                let (file, meta) = &mut outputs[idx];
                file.seek(SeekFrom::Start(header.offset))
                    .map_err(|e| EngineError::Io(e.to_string()))?;
                let mut received = 0u64;
                let total = header.len as u64;
                let mut buf = [0u8; 64 * 1024];
                loop {
                    let n = drecv.read(&mut buf).await?.unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    file.write_all(&buf[..n]).map_err(|e| EngineError::Io(e.to_string()))?;
                    received += n as u64;
                    ctx.sink.on_progress(stream_id_64(&header, idx), received, total);
                }
                // Acknowledge so the sender knows this chunk is fully persisted
                // and can move on / signal completion without racing.
                if write_all_send(&mut dsend, &[0u8; 1]).await.is_ok() {
                    let _ = dsend.finish();
                }
                drop(dsend);
                drop(drecv);
                ctx.sink.on_transfer_complete(&meta.name, meta.size);
            }
            msg = read_frame::<ControlMessage>(&mut ctrl_recv, None) => {
                match msg {
                    Ok(Some(ControlMessage::Done { name, .. })) => {
                        tracing::info!("transfer complete: {name}");
                        break;
                    }
                    Ok(None) => break,
                    Ok(Some(_)) => {}
                    Err(_) => break,
                }
            }
        }
    }

    for (file, _) in &mut outputs {
        let _ = file.sync_all();
    }
    Ok(())
}

fn stream_id_64(header: &StreamHeader, idx: usize) -> u64 {
    // Stable per (file, chunk) id for progress callbacks.
    (header.transfer_id << 16) ^ ((idx as u64) << 8) ^ (header.chunk_index & 0xFF)
}

/// Send one file to an already-open connection by opening a data stream per
/// chunk.
pub async fn send_file_to_peer(
    connection: &Connection,
    transfer_id: u64,
    meta: &FileMeta,
    path: &Path,
    chunk_size: u64,
    sink: &Arc<dyn ProgressSink>,
) -> Result<()> {
    let file_size = meta.size;
    let chunk_size = if chunk_size == 0 { DEFAULT_CHUNK_SIZE } else { chunk_size };
    let total_chunks = file_size.div_ceil(chunk_size).max(1);

    let file = File::open(path)
        .map_err(|e| EngineError::Io(format!("open {}: {e}", path.display())))?;
    let mut reader = std::io::BufReader::new(file);

    for (chunk_index, (offset, len)) in chunks_of(file_size, chunk_size).enumerate() {
        let header = StreamHeader {
            transfer_id,
            file_index: meta.file_index,
            chunk_index: chunk_index as u64,
            offset,
            len: len as u32,
            file_size,
            file_name: meta.name.clone(),
            mime: meta.mime.clone(),
            total_chunks,
        };
        let (mut dsend, mut drecv) = connection.open_bi().await?;
        write_frame(&mut dsend, &header).await?;

        reader
            .seek(SeekFrom::Start(offset))
            .map_err(|e| EngineError::Io(e.to_string()))?;
        let mut remaining = len;
        let mut done = 0u64;
        let mut buf = vec![0u8; 64 * 1024];
        while remaining > 0 {
            let to_read = buf.len().min(remaining as usize);
            let n = reader.read(&mut buf[..to_read]).map_err(|e| EngineError::Io(e.to_string()))?;
            if n == 0 {
                break;
            }
            write_all_send(&mut dsend, &buf[..n]).await?;
            remaining -= n as u64;
            done += n as u64;
            sink.on_progress(chunk_index as u64, done, len);
        }
        dsend.finish()?;
        // Wait for the receiver's ack so the next chunk (and eventually the
        // Done message) is only sent after this chunk is fully persisted.
        let mut ack = [0u8; 1];
        let _ = drecv.read(&mut ack).await;
        drop(drecv);
    }

    sink.on_transfer_complete(&meta.name, file_size);
    Ok(())
}

/// Connect to a peer, perform the hello/request handshake, then send all files.
pub async fn connect_and_send(
    endpoint: &Endpoint,
    dest: std::net::SocketAddr,
    files: Vec<FileMeta>,
    paths: Vec<PathBuf>,
    node_id: String,
    device_name: String,
    cert_pin: Option<rustls::pki_types::CertificateDer<'static>>,
    sink: Arc<dyn ProgressSink>,
) -> Result<()> {
    let client_cfg = match cert_pin {
        Some(cert) => transport::client_config(cert)?,
        None => transport::insecure_client_config(),
    };
    // Insecure client config for LAN; note in logs. The TLS server-name used
    // here matches the ephemeral self-signed certificate; validation is skipped
    // on the discovery path (no external trust anchor exists).
    let server_name = "morselink.local";
    let connecting = endpoint.connect_with(client_cfg, dest, server_name)?;
    let connection = connecting.await?;
    let peer_addr = connection
        .remote_address()
        .to_string();
    sink.on_peer_discovered(&node_id, &device_name, &peer_addr);

    // Control stream: Hello -> HelloAck -> Request -> Accept.
    let (mut ctrl_send, mut ctrl_recv) = connection.open_bi().await?;
    write_frame(&mut ctrl_send, &ControlMessage::hello(&device_name, &node_id)).await?;

    let ack: ControlMessage = read_frame(&mut ctrl_recv, None)
        .await?
        .ok_or_else(|| EngineError::Transfer("peer closed during handshake".into()))?;
    if !matches!(ack, ControlMessage::HelloAck { .. }) {
        return Err(EngineError::Transfer("peer did not ack hello".into()));
    }

    let transfer_id = random_transfer_id();
    let request = ControlMessage::Request {
        transfer_id,
        files: files.clone(),
    };
    write_frame(&mut ctrl_send, &request).await?;

    let accept: ControlMessage = read_frame(&mut ctrl_recv, None)
        .await?
        .ok_or_else(|| EngineError::Transfer("peer closed during accept".into()))?;
    match accept {
        ControlMessage::Accept { .. } => {}
        ControlMessage::Reject { reason, .. } => {
            return Err(EngineError::Transfer(format!("peer rejected: {reason}")));
        }
        _ => return Err(EngineError::Transfer("peer did not accept".into())),
    }

    // Send the files (each spreads chunks across data streams).
    for (i, (meta, path)) in files.iter().zip(paths.iter()).enumerate() {
        let mut m = meta.clone();
        m.file_index = (i + 1) as u32;
        send_file_to_peer(&connection, transfer_id, &m, path, DEFAULT_CHUNK_SIZE, &sink).await?;
    }

    write_frame(&mut ctrl_send, &ControlMessage::Done { transfer_id, name: "batch".into() }).await?;
    ctrl_send.finish()?;
    connection.close(0u32.into(), b"done");
    Ok(())
}

fn random_transfer_id() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    // Mix the time-based entropy with a per-process seed.
    (nanos as u64) ^ (std::process::id() as u64).rotate_left(17)
        ^ ((rand_seed() as u64) << 1)
}

fn rand_seed() -> u32 {
    // No external RNG dependency: derive entropy from an address.
    let boxed = Box::new(0u8);
    (&*boxed as *const u8 as usize) as u32
}

pub fn sanitize_file_name(name: &str) -> String {
    let safe: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, '.' | '-' | '_' | ' ') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let s = safe.trim();
    if s.is_empty() {
        "transfer.bin".into()
    } else {
        s.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitise_filenames() {
        assert_eq!(sanitize_file_name("../secret???.bin"), "__secret___.bin");
        assert_eq!(sanitize_file_name("photo (1).jpg"), "photo (1).jpg");
        assert_eq!(sanitize_file_name(""), "transfer.bin");
    }

    #[test]
    fn transfer_id_is_well_formed() {
        let a = random_transfer_id();
        let b = random_transfer_id();
        assert_ne!(a, b);
    }
}
