//! End-to-end loopback transfer test: a real QUIC sender and receiver on
//! 127.0.0.1 verify that chunked multiplexed transfer produces byte-identical
//! output. This is the quickest way to validate the whole engine on a CI
//! runner without any devices.

use std::sync::Arc;

use morselink_core::protocol::FileMeta;
use morselink_core::transfer::{NullSink, ServeContext, connect_and_send, serve};
use morselink_core::transport;

const CHUNK: usize = 256 * 1024;

#[tokio::test]
async fn loopback_transfer_round_trips() {
    // 1. Build a server endpoint and learn its address.
    let (cert, key) = transport::generate_self_signed("morselink.local").unwrap();
    let server_cfg = transport::server_config(cert, key).unwrap();
    let server_endpoint = transport::bind_endpoint("127.0.0.1", 0, Some(server_cfg)).unwrap();
    let server_addr = server_endpoint.local_addr().unwrap();

    // 2. Create a temp source file and a receive dir.
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("source.bin");
    let data: Vec<u8> = (0..(3 * CHUNK + 173)).map(|i| (i % 251) as u8).collect();
    std::fs::write(&src, &data).unwrap();

    let receive_dir = tmp.path().join("received");
    std::fs::create_dir_all(&receive_dir).unwrap();

    // 3. Start the receiving side.
    let sink: Arc<dyn morselink_core::ProgressSink> = Arc::new(NullSink);
    let ctx = ServeContext {
        endpoint: server_endpoint,
        node_id: "server-node".into(),
        device_name: "Loopback receiver".into(),
        receive_dir: receive_dir.clone(),
        sink: sink.clone(),
    };
    let serve_task = tokio::spawn(serve(ctx));

    // 4. Send the file.
    let client_endpoint = transport::bind_endpoint("127.0.0.1", 0, None).unwrap();
    let meta = FileMeta {
        name: "source.bin".into(),
        size: data.len() as u64,
        mime: "application/octet-stream".into(),
        file_index: 1,
    };
    connect_and_send(
        &client_endpoint,
        server_addr,
        vec![meta],
        vec![src],
        "client-node".into(),
        "Loopback sender".into(),
        None,
        sink,
    )
    .await
    .unwrap();

    // 5. Verify the received file matches byte-for-byte.
    let received_path = receive_dir.join("source.bin");
    assert!(received_path.exists(), "received file must exist");
    let got = std::fs::read(&received_path).unwrap();
    assert_eq!(got, data, "received data must match source exactly");

    // 6. Stop the server.
    serve_task.abort();
    let _ = tmp;
}
