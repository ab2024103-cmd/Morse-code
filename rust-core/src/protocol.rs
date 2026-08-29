//! Wire protocol: framing and message types.
//!
//! Layout
//! ------
//! After a QUIC connection is established, the initiating peer opens a single
//! **control bidi-stream** and performs a hello/ack handshake, then a
//! request/accept exchange. Each file *byte-range* is then carried on its own
//! **data bidi-stream** so QUIC can multiplex many ranges natively — this is
//! what allows a single socket to saturate the wifi link without hand-rolled
//! parallelism.
//!
//! Every stream is framed as:
//!   [u32 BE length][payload bytes]
//! where the payload is either a JSON control message or a split header+data.

use serde::{Deserialize, Serialize};

use crate::config::PROTOCOL_VERSION;
use crate::error::{EngineError, Result};

/// Upper bound on a single framed control message (metadata only).
const MAX_CTRL_FRAME: usize = 1 << 20; // 1 MiB
/// Upper bound on a per-stream metadata header.
const MAX_HEADER_FRAME: usize = 64 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMeta {
    pub name: String,
    pub size: u64,
    #[serde(default)]
    pub mime: String,
    /// 1-based index within the transfer batch.
    pub file_index: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ControlMessage {
    Hello {
        protocol_version: String,
        device_name: String,
        peer_id: String,
    },
    HelloAck {
        protocol_version: String,
        device_name: String,
        peer_id: String,
    },
    Request {
        transfer_id: u64,
        files: Vec<FileMeta>,
    },
    Accept {
        transfer_id: u64,
    },
    Reject {
        transfer_id: u64,
        reason: String,
    },
    Done {
        transfer_id: u64,
        name: String,
    },
}

/// Header written at the start of every data stream (one per chunk).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamHeader {
    pub transfer_id: u64,
    pub file_index: u32,
    pub chunk_index: u64,
    /// Byte offset of this chunk within the file (bytes [offset, offset+len)).
    pub offset: u64,
    /// Length of this chunk's payload (`u32`-sized so it fits the frame).
    pub len: u32,
    pub file_size: u64,
    pub file_name: String,
    #[serde(default)]
    pub mime: String,
    pub total_chunks: u64,
}

impl ControlMessage {
    pub fn hello(device_name: &str, peer_id: &str) -> Self {
        Self::Hello {
            protocol_version: PROTOCOL_VERSION.into(),
            device_name: device_name.into(),
            peer_id: peer_id.into(),
        }
    }

    pub fn hello_ack(device_name: &str, peer_id: &str) -> Self {
        Self::HelloAck {
            protocol_version: PROTOCOL_VERSION.into(),
            device_name: device_name.into(),
            peer_id: peer_id.into(),
        }
    }
}

/// Write a length-prefixed JSON frame to a send stream.
pub async fn write_frame<T: Serialize, S: quinn::SendStream + Unpin>(
    send: &mut S,
    value: &T,
) -> Result<()> {
    let bytes = serde_json::to_vec(value)?;
    if bytes.len() > MAX_CTRL_FRAME {
        return Err(EngineError::Transfer("frame exceeds max size".into()));
    }
    let len = (bytes.len() as u32).to_be_bytes();
    write_all_send(send, &len).await?;
    write_all_send(send, &bytes).await?;
    Ok(())
}

/// Write `buf` in full using `SendStream`'s inherent `write`.
pub async fn write_all_send<S: quinn::SendStream + Unpin>(
    send: &mut S,
    mut buf: &[u8],
) -> Result<()> {
    while !buf.is_empty() {
        let n = send.write(buf).await?;
        if n == 0 {
            return Err(EngineError::Transfer("stream stalled while writing".into()));
        }
        buf = &buf[n..];
    }
    Ok(())
}

/// Read a length-prefixed JSON frame from a recv stream.
pub async fn read_frame<T: for<'de> Deserialize<'de>, R: quinn::RecvStream + Unpin>(
    recv: &mut R,
    max: Option<usize>,
) -> Result<Option<T>> {
    let mut len_buf = [0u8; 4];
    // A clean EOF before any byte means the stream finished with no message.
    match read_exact(recv, &mut len_buf).await {
        Ok(n) if n == 0 => return Ok(None),
        Ok(_) => {}
        Err(e) => return Err(e),
    }
    let len = u32::from_be_bytes(len_buf) as usize;
    let cap = max.unwrap_or(MAX_CTRL_FRAME);
    if len == 0 || len > cap {
        return Err(EngineError::Transfer("bad frame length".into()));
    }
    let mut buf = vec![0u8; len];
    read_exact(recv, &mut buf).await?;
    let value = serde_json::from_slice(&buf)?;
    Ok(Some(value))
}

/// Read exactly `buf.len()` bytes; returns the number of bytes read (short only
/// at clean EOF).
async fn read_exact<R: quinn::RecvStream + Unpin>(recv: &mut R, mut buf: &mut [u8]) -> Result<usize> {
    let mut read = 0;
    while !buf.is_empty() {
        match recv.read(&mut buf).await {
            Ok(Some(n)) => {
                buf = &mut buf[n..];
                read += n;
            }
            Ok(None) => break,
            Err(e) => return Err(EngineError::from(e)),
        }
    }
    Ok(read)
}

/// Compute chunk boundaries for a file, matching clients on both platforms.
pub fn chunks_of(file_size: u64, chunk_size: u64) -> impl Iterator<Item = (u64, u64)> {
    let cs = chunk_size.max(1);
    (0..file_size.div_ceil(cs)).map(move |i| {
        let offset = i * cs;
        let len = (file_size - offset).min(cs);
        (offset, len)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunking_covers_whole_file() {
        // odd file size: 10 bytes with 4-byte chunks -> 4,4,2
        let chunks: Vec<_> = chunks_of(10, 4).collect();
        assert_eq!(chunks, vec![(0, 4), (4, 4), (8, 2)]);
        // empty file -> zero chunks
        assert_eq!(chunks_of(0, 4).count(), 0);
        // exactly aligned
        assert_eq!(chunks_of(8, 4).collect::<Vec<_>>(), vec![(0, 4), (4, 4)]);
    }

    #[test]
    fn header_len_fits_u32() {
        let h = StreamHeader {
            transfer_id: 1,
            file_index: 1,
            chunk_index: 0,
            offset: 0,
            len: u32::MAX,
            file_size: u64::MAX,
            file_name: "x".into(),
            mime: String::new(),
            total_chunks: 1,
        };
        assert!(h.len as u64 <= u32::MAX as u64);
    }
}
