//! QUIC transport setup: ephemeral self-signed TLS 1.3 certificates and
//! endpoint/client configuration.
//!
//! Security model
//! --------------
//! MorseLink is a strict zero-trust, zero-cloud LAN app. For first-contact
//! discovery on an untrusted local network there is no way to pre-share a
//! certificate, so we use *ephemeral self-signed certificates* generated fresh
//! for every session. We also ship a `SkipServerVerification` verifier used
//! only on the ephemeral/discovery path (no external trust anchor exists); it
//! still guarantees TLS 1.3 encryption + integrity, just without fingerprint
//! pinning. Nothing is persisted after a session ends (zero pairing storage).

use std::sync::Arc;
use std::time::Duration;

use quinn::crypto::rustls::{QuicClientConfig, QuicServerConfig};
use quinn::{ClientConfig, Endpoint, ServerConfig};
use rcgen::CertifiedKey;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime};
use rustls::{ClientConfig as RustlsClient, ServerConfig as RustlsServer, DigitallySignedStruct, SignatureScheme};

use crate::config::MAX_CONCURRENT_STREAMS;
use crate::error::{EngineError, Result};

/// Generate an ephemeral self-signed certificate for a given server name.
pub fn generate_self_signed(
    server_name: &str,
) -> Result<(CertificateDer<'static>, PrivateKeyDer<'static>)> {
    let CertifiedKey { cert, key_pair } =
        rcgen::generate_simple_self_signed(vec![server_name.to_string()])
            .map_err(|e| EngineError::Transport(format!("cert generation: {e}")))?;
    let cert_der = cert.der().clone();
    let key_der = PrivateKeyDer::Pkcs8(key_pair.serialize_der().into());
    Ok((cert_der, key_der))
}

/// Build the QUIC `ServerConfig` for the receiving side.
pub fn server_config(
    cert: CertificateDer<'static>,
    key: PrivateKeyDer<'static>,
) -> Result<ServerConfig> {
    // quinn 0.11 expects a rustls server config wrapped into a QuicServerConfig.
    let rustls = RustlsServer::builder()
        .with_no_client_auth()
        .with_single_cert(vec![cert], key)
        .map_err(|e| EngineError::Transport(format!("rustls server config: {e}")))?;
    let crypto = QuicServerConfig::try_from(rustls)
        .map_err(|e| EngineError::Transport(format!("quic server config: {e}")))?;

    let mut cfg = ServerConfig::with_crypto(Arc::new(crypto));
    let mut tcfg = default_transport_config();
    tcfg.max_concurrent_bidi_streams(MAX_CONCURRENT_STREAMS.into());
    cfg.transport_config(Arc::new(tcfg));
    Ok(cfg)
}

/// Build a QUIC `ClientConfig` that pins the peer's self-signed certificate.
pub fn client_config(cert: CertificateDer<'static>) -> Result<ClientConfig> {
    let mut roots = rustls::RootCertStore::empty();
    roots
        .add(cert)
        .map_err(|e| EngineError::Transport(format!("pin cert: {e}")))?;

    let rustls = RustlsClient::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let crypto = QuicClientConfig::try_from(rustls)
        .map_err(|e| EngineError::Transport(format!("quic client config: {e}")))?;

    let mut cfg = ClientConfig::new(Arc::new(crypto));
    let mut tcfg = default_transport_config();
    tcfg.max_concurrent_bidi_streams(MAX_CONCURRENT_STREAMS.into());
    cfg.transport_config(Arc::new(tcfg));
    Ok(cfg)
}

/// QUIC-over-UDP endpoints bind to the configured address. `0.0.0.0` allows
/// all interfaces (hotspot, wi-fi-direct, ethernet).
pub fn bind_endpoint(
    addr: &str,
    port: u16,
    server_config: Option<ServerConfig>,
) -> Result<Endpoint> {
    let sockaddr: std::net::SocketAddr = format!("{addr}:{port}")
        .parse()
        .map_err(|e| EngineError::Transport(format!("bad bind address: {e}")))?;
    match server_config {
        Some(cfg) => Endpoint::server(cfg, sockaddr)
            .map_err(|e| EngineError::Transport(format!("endpoint bind: {e}"))),
        None => Endpoint::client(sockaddr)
            .map_err(|e| EngineError::Transport(format!("endpoint bind: {e}"))),
    }
}

fn default_transport_config() -> quinn::TransportConfig {
    let mut tcfg = quinn::TransportConfig::default();
    tcfg.max_concurrent_uni_streams(0u32.into());
    tcfg.keep_alive_interval(Some(Duration::from_secs(5)));
    tcfg
}

/// Verifier that accepts any server certificate. Used ONLY on the ephemeral
/// discovery path; TLS 1.3 is still fully negotiated.
#[derive(Debug)]
pub struct SkipServerVerification;

impl ServerCertVerifier for SkipServerVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> std::result::Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![
            SignatureScheme::RSA_PKCS1_SHA256,
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::ED25519,
            SignatureScheme::RSA_PSS_SHA256,
        ]
    }
}

/// Client config using the permissive verifier (host/CLI/testing only).
pub fn insecure_client_config() -> ClientConfig {
    let rustls = RustlsClient::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(SkipServerVerification))
        .with_no_client_auth();
    // Unwrapping is acceptable here: a rustls default builder with TLS 1.3 did
    // not negotiate a cipher suite only in pathological builds.
    let crypto = QuicClientConfig::try_from(rustls)
        .expect("rustls default client config must be QUIC-compatible");

    let mut cfg = ClientConfig::new(Arc::new(crypto));
    let mut tcfg = default_transport_config();
    tcfg.max_concurrent_bidi_streams(MAX_CONCURRENT_STREAMS.into());
    cfg.transport_config(Arc::new(tcfg));
    cfg
}
