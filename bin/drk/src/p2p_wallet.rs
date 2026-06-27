/* This file is part of DarkWow
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

//! Wallet-owned P2P networking — TCP+TLS client for block sync.
//!
//! Two-layer transport architecture:
//!   Layer 0 (always on): Built-in TCP+TLS — the critical path, unchanged.
//!   Layer 1 (optional):   External transports via `dwow_transport` crate
//!                         (Tor, SOCKS5, QUIC, Nym) — additive, feature-gated.
//!
//! The wallet is a pure P2P client: outbound connections only. It never
//! listens for inbound connections. Wire protocol:
//!
//!   varint(msg_name_len) + msg_name + varint(payload_len) + payload
//!
//! No structopt, no SettingsOpt, no config merging. Config is TOML-direct.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use futures_rustls::{
    rustls::{
        self,
        client::danger::{ServerCertVerified, ServerCertVerifier},
        pki_types::{CertificateDer, ServerName, UnixTime},
        version::TLS13,
        ClientConfig, DigitallySignedStruct, SignatureScheme,
    },
    TlsConnector,
};
use serde::{Deserialize, Serialize};
use smol::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use smol::net::TcpStream;
use smol::prelude::*;
use url::Url;
use x509_parser::{
    parse_x509_certificate,
    prelude::{GeneralName, ParsedExtension},
};

#[cfg(feature = "transport")]
use dwow_transport;

use crate::wallet_error::{Error, Result};

/// Marker trait for type-erased async streams — same pattern as
/// dwow_transport::PtStream. Produced by both Layer 0 (built-in TCP)
/// and Layer 1 (dwow_transport).
pub trait WalletStream: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> WalletStream for T {}

// ============================================================================
// Config — direct TOML deserialization, no SettingsOpt, no structopt
// ============================================================================

/// Seed node address parsed from TOML.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeedAddr {
    pub url: String,
}

/// P2P configuration for the wallet. Parsed directly from TOML `[net]` section.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct P2pWalletConfig {
    #[serde(default)]
    pub seeds: Vec<SeedAddr>,
    #[serde(default = "default_magic_bytes")]
    pub magic_bytes: [u8; 4],
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_max_peers")]
    pub max_peers: usize,
    #[serde(default = "default_connect_timeout")]
    pub connect_timeout_secs: u64,
    #[serde(default = "default_request_timeout")]
    pub request_timeout_secs: u64,
    #[serde(default)]
    pub localnet: bool,
    /// Datastore path for Tor arti data/cache directories. Expanded by caller.
    #[serde(default)]
    pub datastore: Option<String>,
}

fn default_magic_bytes() -> [u8; 4] { [0xd9, 0xef, 0xb6, 0x7d] }
fn default_port() -> u16 { 31340 }
fn default_max_peers() -> usize { 8 }
fn default_connect_timeout() -> u64 { 10 }
fn default_request_timeout() -> u64 { 30 }

// ============================================================================
// TLS — same logic as src/net/transport/tls.rs, no dwow_core dep
// ============================================================================

/// TLS certificate verifier. Ported from dwow_core::net::transport::tls.
#[derive(Debug)]
struct WalletCertVerifier {
    localnet: bool,
}

impl ServerCertVerifier for WalletCertVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> std::result::Result<ServerCertVerified, rustls::Error> {
        if self.localnet {
            return Ok(ServerCertVerified::assertion());
        }

        let buf: Vec<u8> = end_entity.iter().copied().collect();
        let Ok((_, cert)) = parse_x509_certificate(&buf) else {
            return Err(rustls::CertificateError::BadEncoding.into())
        };

        let oid = x509_parser::oid_registry::asn1_rs::oid!(2.5.29.17);
        let Ok(Some(extension)) = cert.get_extension_unique(&oid) else {
            return Err(rustls::CertificateError::BadEncoding.into())
        };

        let dns_name = match extension.parsed_extension() {
            ParsedExtension::SubjectAlternativeName(altname) => {
                if altname.general_names.len() != 1 {
                    return Err(rustls::CertificateError::BadEncoding.into())
                }
                match altname.general_names[0] {
                    GeneralName::DNSName(dns_name) => dns_name,
                    _ => return Err(rustls::CertificateError::BadEncoding.into()),
                }
            }
            _ => return Err(rustls::CertificateError::BadEncoding.into()),
        };

        if dns_name != "dark.fi" {
            return Err(rustls::CertificateError::BadEncoding.into())
        }

        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        if dss.scheme != SignatureScheme::ED25519 {
            return Err(rustls::CertificateError::BadSignature.into())
        }
        let buf: Vec<u8> = cert.iter().copied().collect();
        let Ok((_, cert)) = parse_x509_certificate(&buf) else {
            return Err(rustls::CertificateError::BadEncoding.into())
        };
        let Ok(public_key) = ed25519_compact::PublicKey::from_der(cert.public_key().raw) else {
            return Err(rustls::CertificateError::BadEncoding.into())
        };
        let Ok(signature) = ed25519_compact::Signature::from_slice(dss.signature()) else {
            return Err(rustls::CertificateError::BadSignature.into())
        };
        if public_key.verify(message, &signature).is_err() {
            return Err(rustls::CertificateError::BadSignature.into())
        }
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        self.verify_tls12_signature(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![SignatureScheme::ED25519]
    }
}

fn make_tls_config(localnet: bool) -> Arc<ClientConfig> {
    let verifier = Arc::new(WalletCertVerifier { localnet });
    let config = ClientConfig::builder_with_protocol_versions(&[&TLS13])
        .dangerous()
        .with_custom_certificate_verifier(verifier.clone())
        .with_no_client_auth();
    Arc::new(config)
}

// ============================================================================
// Varint encoding — ported from sync_task.rs (P2P framing, not sync logic)
// ============================================================================

fn varint_encode(mut value: usize) -> Vec<u8> {
    let mut buf = Vec::with_capacity(8);
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        buf.push(byte);
        if value == 0 {
            break;
        }
    }
    buf
}

fn varint_decode(bytes: &[u8]) -> Option<(usize, &[u8])> {
    let mut result: usize = 0;
    let mut shift = 0;
    for (i, &byte) in bytes.iter().enumerate() {
        result |= ((byte & 0x7f) as usize) << shift;
        if byte & 0x80 == 0 {
            return Some((result, &bytes[i + 1..]));
        }
        shift += 7;
        if shift >= 64 {
            return None;
        }
    }
    None
}

// ============================================================================
// Wire protocol: version handshake
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Version {
    pub version: u32,
    pub services: u64,
    pub timestamp: u64,
    pub recv_addr: String,
    pub send_addr: String,
    pub nonce: u64,
    pub user_agent: String,
    pub start_height: u64,
}

// Hostlist + HostAddr REMOVED — dead code, never constructed (HAZOP round 2 dead code sweep)

// ============================================================================
// PeerConnection — TCP+TLS with varint framing
// ============================================================================

/// A framed connection to a single peer. Stream is type-erased (works with
/// both Layer 0 built-in TCP and Layer 1 external transports).
pub struct PeerConnection {
    addr: String,
    stream: Box<dyn WalletStream>,
    magic_bytes: [u8; 4],
}

impl PeerConnection {
    // ========================================================================
    // Layer 0: Built-in TCP+TLS — ALWAYS compiled, critical path.
    // Never touches dwow_transport.
    // ========================================================================

    /// Connect via built-in TCP (or TCP+TLS). This is the existing code path,
    /// with TLS now wired up (was previously unused `_tls_config`).
    pub async fn connect_tcp(
        addr: &str,
        tls_config: &Arc<ClientConfig>,
        magic_bytes: [u8; 4],
        local_height: u64,
        connect_timeout_secs: u64,
    ) -> Result<Self> {
        let (host, port) = parse_host_port(addr)?;
        let tcp = smol::future::or(
            async {
                smol::net::TcpStream::connect(format!("{host}:{port}")).await
                    .map_err(|e| Error::Custom(format!("TCP connect {addr}: {e}")))
            },
            async {
                smol::Timer::after(std::time::Duration::from_secs(connect_timeout_secs)).await;
                Err(Error::Custom(format!("TCP connect {addr}: timed out after {connect_timeout_secs}s")))
            },
        )
        .await?;

        // Wire up TLS that was previously unused
        let stream: Box<dyn WalletStream> = if addr.starts_with("tcp+tls://") {
            let server_name = ServerName::try_from(host)
                .map_err(|e| Error::Custom(format!("TLS SNI: {e}")))?;
            let connector = TlsConnector::from(tls_config.clone());
            let tls_stream = connector.connect(server_name, tcp).await
                .map_err(|e| Error::Custom(format!("TLS handshake {addr}: {e}")))?;
            Box::new(tls_stream)
        } else {
            Box::new(tcp)
        };

        let mut peer = PeerConnection { addr: addr.to_string(), stream, magic_bytes };
        peer.send_version(local_height).await?;
        Ok(peer)
    }

    // ========================================================================
    // Layer 1: External transports — ONLY compiled when `transport` feature
    // is enabled. Entirely additive.
    // ========================================================================

    /// Connect via external transport (Tor, SOCKS5, QUIC, etc.).
    #[cfg(feature = "transport")]
    pub async fn connect_external(
        endpoint_url: &str,
        magic_bytes: [u8; 4],
        local_height: u64,
        datastore: Option<PathBuf>,
        localnet: bool,
    ) -> Result<Self> {
        let url = Url::parse(endpoint_url)
            .map_err(|e| Error::Custom(format!("invalid transport URL '{endpoint_url}': {e}")))?;

        let dialer = dwow_transport::Dialer::new(url, datastore, localnet)
            .await
            .map_err(|e| Error::Custom(format!("transport setup: {e}")))?;

        let pt_stream: Box<dyn dwow_transport::PtStream> = dialer
            .dial(Some(Duration::from_secs(10)))
            .await
            .map_err(|e| Error::Custom(format!("dial {endpoint_url}: {e}")))?;

        // Convert Box<dyn PtStream> → Box<dyn WalletStream>.
        // PtStream and WalletStream have identical trait bounds (AsyncRead +
        // AsyncWrite + Unpin + Send), so their vtable layouts are compatible.
        // This is the same pattern used by erased-serde.
        let stream: Box<dyn WalletStream> = unsafe {
            let raw: *mut dyn dwow_transport::PtStream = Box::into_raw(pt_stream);
            let raw: *mut dyn WalletStream = std::mem::transmute(raw);
            Box::from_raw(raw)
        };

        let mut peer = PeerConnection { addr: endpoint_url.to_string(), stream, magic_bytes };
        peer.send_version(local_height).await?;
        Ok(peer)
    }

    // ========================================================================
    // Shared: version handshake + wire protocol I/O
    // ========================================================================

    /// Send version handshake using dwow_core's binary VersionMessage.
    /// Replaces the old JSON Version — must be wire-compatible with lilith's
    /// ProtocolVersion handshake (magic bytes already sent by send_raw).
    async fn send_version(&mut self, local_height: u64) -> Result<()> {
        use dwow_core::net::message::VersionMessage;
        use dwow_serial::Encodable;

        let version_msg = VersionMessage {
            node_id: String::new(),
            app_name: "dwow-wallet".to_string(),
            version: semver::Version::new(0, 5, 0),
            timestamp: SystemTime::now().duration_since(UNIX_EPOCH)
                .unwrap_or_default().as_secs(),
            connect_recv_addr: url::Url::parse(&format!("tcp+tls://{}", self.addr))
                .unwrap_or_else(|_| url::Url::parse("tcp+tls://0.0.0.0:0").unwrap()),
            resolve_recv_addr: None,
            ext_send_addr: vec![],
            features: vec![],
        };

        let mut payload = Vec::new();
        dwow_serial::Encodable::encode(&version_msg, &mut payload)
            .map_err(|e| Error::Custom(format!("version encode: {e}")))?;
        self.send_raw("version", &payload).await
    }

    /// Send framed message: magic_bytes(4) + varint(name_len) + name_bytes + varint(payload_len) + payload.
    /// Matches dwow_core::net::Channel wire format — magic bytes prefix for protocol compatibility.
    async fn send_raw(&mut self, name: &str, payload: &[u8]) -> Result<()> {
        let name_bytes = name.as_bytes();
        let mut frame = Vec::with_capacity(4 + 10 + name_bytes.len() + 10 + payload.len());
        frame.extend_from_slice(&self.magic_bytes);
        frame.extend_from_slice(&varint_encode(name_bytes.len()));
        frame.extend_from_slice(name_bytes);
        frame.extend_from_slice(&varint_encode(payload.len()));
        frame.extend_from_slice(payload);

        self.stream.write_all(&frame).await
            .map_err(|e| Error::Custom(format!("send {name}: {e}")))?;
        self.stream.flush().await
            .map_err(|e| Error::Custom(format!("flush {name}: {e}")))?;
        Ok(())
    }

    /// Receive a framed message, returning (name_bytes, payload_bytes).
    /// Reads magic_bytes(4) + varint(name_len) + name + varint(payload_len) + payload.
    /// Matches dwow_core::net::Channel wire format.
    async fn recv_raw(&mut self) -> Result<(Vec<u8>, Vec<u8>)> {
        // Read and verify 4 magic bytes first
        let mut magic = [0u8; 4];
        self.stream.read_exact(&mut magic).await
            .map_err(|e| Error::Custom(format!("recv magic bytes: {e}")))?;
        if magic != self.magic_bytes {
            return Err(Error::Custom(format!(
                "magic bytes mismatch: expected {:?}, got {magic:?}", self.magic_bytes
            )));
        }

        let mut header = vec![0u8; 16];
        let mut header_len = 0;
        // Read varint for message name length
        loop {
            let n = self.stream.read(&mut header[header_len..header_len + 1]).await
                .map_err(|e| Error::Custom(format!("recv header: {e}")))?;
            if n == 0 {
                return Err(Error::Custom("connection closed".into()));
            }
            header_len += 1;
            if header[header_len - 1] & 0x80 == 0 {
                break;
            }
            if header_len >= 10 {
                return Err(Error::Custom("name varint too long".into()));
            }
        }
        let name_len = varint_decode(&header[..header_len])
            .map(|(v, _)| v)
            .unwrap_or(0);
        if name_len == 0 || name_len > 256 {
            return Err(Error::Custom(format!("invalid name len: {name_len}")));
        }

        let mut name_buf = vec![0u8; name_len];
        self.stream.read_exact(&mut name_buf).await
            .map_err(|e| Error::Custom(format!("recv name: {e}")))?;

        // Read payload varint
        let mut payload_header = vec![0u8; 16];
        let mut payload_header_len = 0;
        loop {
            let n = self.stream.read(&mut payload_header[payload_header_len..payload_header_len + 1]).await
                .map_err(|e| Error::Custom(format!("recv payload header: {e}")))?;
            if n == 0 {
                return Err(Error::Custom("connection closed".into()));
            }
            payload_header_len += 1;
            if payload_header[payload_header_len - 1] & 0x80 == 0 {
                break;
            }
            if payload_header_len >= 10 {
                return Err(Error::Custom("payload varint too long".into()));
            }
        }
        let payload_len = varint_decode(&payload_header[..payload_header_len])
            .map(|(v, _)| v)
            .unwrap_or(0);

        // Cap at 10MB — prevents OOM from malicious peers claiming huge payloads
        if payload_len > 10 * 1024 * 1024 {
            return Err(Error::Custom(format!(
                "peer sent excessive payload length: {} bytes (max 10MB)",
                payload_len
            )));
        }

        let mut payload = vec![0u8; payload_len];
        if payload_len > 0 {
            self.stream.read_exact(&mut payload).await
                .map_err(|e| Error::Custom(format!("recv payload: {e}")))?;
        }

        Ok((name_buf, payload))
    }

    /// Send a typed message. name = wire protocol name like "lineargetblocks".
    pub async fn send<T: Serialize>(&mut self, name: &str, msg: &T) -> Result<()> {
        let payload = serde_json::to_vec(msg)
            .map_err(|e| Error::Custom(format!("serialize {name}: {e}")))?;
        self.send_raw(name, &payload).await
    }

    /// Receive the next message, returning (wire_name, payload_bytes).
    pub async fn recv(&mut self) -> Result<(String, Vec<u8>)> {
        let (name_bytes, payload) = self.recv_raw().await?;
        let name = String::from_utf8(name_bytes)
            .map_err(|e| Error::Custom(format!("invalid name utf8: {e}")))?;
        Ok((name, payload))
    }
}

fn parse_host_port(addr: &str) -> Result<(String, u16)> {
    if let Ok(url) = Url::parse(addr) {
        let host = url.host_str().unwrap_or("").to_string();
        let port = url.port().unwrap_or(52666);
        Ok((host, port))
    } else {
        // Parse as host:port
        let parts: Vec<&str> = addr.rsplitn(2, ':').collect();
        if parts.len() == 2 {
            let port: u16 = parts[0].parse()
                .map_err(|_| Error::Custom(format!("invalid port in {addr}")))?;
            Ok((parts[1].to_string(), port))
        } else {
            Ok((addr.to_string(), 52666))
        }
    }
}

// ============================================================================
// connect_peer — composition boundary between Layer 0 and Layer 1
// ============================================================================

/// Connect to a peer using the appropriate transport layer based on URL scheme.
/// Called by both P2pWallet::connect() and sync_task.
pub(crate) async fn connect_peer(
    addr: &str,
    tls_config: &Arc<ClientConfig>,
    magic_bytes: [u8; 4],
    local_height: u64,
    datastore: Option<PathBuf>,
    localnet: bool,
    connect_timeout_secs: u64,
) -> Result<PeerConnection> {
    let url = Url::parse(addr)
        .or_else(|_| Url::parse(&format!("tcp+tls://{addr}")))
        .map_err(|e| Error::Custom(format!("invalid peer URL '{}': {}", addr, e)))?;

    match url.scheme() {
        // Layer 0: Built-in TCP/TLS — always available, critical path
        "tcp" | "tcp+tls" => {
            PeerConnection::connect_tcp(addr, tls_config, magic_bytes, local_height, connect_timeout_secs).await
        }

        // Layer 1: External transports — only when feature enabled
        #[cfg(feature = "transport")]
        _ => {
            PeerConnection::connect_external(addr, magic_bytes, local_height, datastore, localnet).await
        }

        // Layer 1 absent: unsupported scheme → clear error
        #[cfg(not(feature = "transport"))]
        other => Err(Error::Custom(format!(
            "unsupported transport scheme '{other}'. Rebuild with transport feature enabled."
        ))),
    }
}

// ============================================================================
// PeerHandle — per-peer typed message dispatch via smol channels
// ============================================================================

/// Handle to a connected peer.
pub struct PeerHandle {
    addr: String,
    tx: smol::channel::Sender<(String, Vec<u8>)>,
}

impl PeerHandle {
    // PeerHandle::addr and PeerHandle::send removed — dead code (HAZOP round 2).
    // broadcast_to() uses handle.tx.send() directly.
}

// ============================================================================
// P2pWallet — main P2P instance
// ============================================================================

pub type P2pWalletPtr = Arc<RwLock<P2pWallet>>;

pub struct P2pWallet {
    pub(crate) config: P2pWalletConfig,
    peers: HashMap<String, Arc<PeerHandle>>,
    pub(crate) tls_config: Arc<rustls::ClientConfig>,
    pub(crate) magic_bytes: [u8; 4],
    pub local_height: Arc<AtomicU64>,
}

impl P2pWallet {
    pub async fn new(config: P2pWalletConfig) -> Result<P2pWalletPtr> {
        let tls_config = make_tls_config(config.localnet);
        let magic_bytes = config.magic_bytes;
        Ok(Arc::new(RwLock::new(P2pWallet {
            config,
            peers: HashMap::new(),
            tls_config,
            magic_bytes,
            local_height: Arc::new(AtomicU64::new(0)),
        })))
    }

    /// Connect to seed nodes, perform handshake.
    pub async fn seed(&mut self) -> Result<()> {
        for seed in &self.config.seeds.clone() {
            match self.connect(&seed.url).await {
                Ok(_) => {}
                Err(e) => {
                    tracing::warn!("seed connect {}: {e}", seed.url);
                }
            }
        }
        Ok(())
    }

    /// Connect to a peer by address. Dispatches to Layer 0 (built-in TCP)
    /// or Layer 1 (external transports) based on URL scheme.
    pub async fn connect(&mut self, addr: &str) -> Result<Arc<PeerHandle>> {
        if let Some(existing) = self.peers.get(addr) {
            return Ok(existing.clone());
        }

        let height = self.local_height.load(Ordering::Relaxed);
        let datastore = self.config.datastore.as_ref().map(|s| PathBuf::from(s));

        let mut conn = connect_peer(
            addr,
            &self.tls_config,
            self.config.magic_bytes,
            height,
            datastore,
            self.config.localnet,
            self.config.connect_timeout_secs,
        ).await?;

        // Create channel for write task
        let (tx, rx) = smol::channel::bounded::<(String, Vec<u8>)>(64);
        let handle = Arc::new(PeerHandle { addr: addr.to_string(), tx });

        // Spawn write task
        let write_handle = handle.clone();
        let write_addr = addr.to_string();
        smol::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok((name, payload)) => {
                        if let Err(e) = conn.send_raw(&name, &payload).await {
                            tracing::debug!("peer {write_addr} write: {e}");
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        }).detach();

        self.peers.insert(addr.to_string(), handle.clone());
        Ok(handle)
    }

    /// Get all connected peer addresses.
    pub fn peers(&self) -> Vec<String> {
        self.peers.keys().cloned().collect()
    }

    /// Number of connected peers.
    pub fn peer_count(&self) -> usize {
        self.peers.len()
    }

    /// Configured seed URLs (for diagnostic reporting).
    pub fn seed_urls(&self) -> Vec<String> {
        self.config.seeds.iter().map(|s| s.url.clone()).collect()
    }

    // get_peer + broadcast_tx REMOVED — dead code (HAZOP round 2).
    // collect_peers() + broadcast_to() are the live replacements.

    /// Collect peer handles (cheap — just clone Arcs). Caller must
    /// hold the read lock. Returns handles to iterate outside the lock.
    pub fn collect_peers(&self) -> Vec<Arc<PeerHandle>> {
        self.peers.values().cloned().collect()
    }

    /// Broadcast pre-serialized tx bytes to a list of peer handles.
    /// Free function — no &self, no lock. Caller collects handles under
    /// the read lock, drops it, then calls this. Keeps the future
    /// Send-safe for #[async_trait] RPC handlers.
    pub async fn broadcast_to(handles: &[Arc<PeerHandle>], name: &str, data: &[u8]) {
        for handle in handles {
            let _ = handle.tx.send((name.to_string(), data.to_vec())).await;
        }
    }

    // set_local_height + get_local_height REMOVED — never called (HAZOP round 2).
    // TODO: wire set_local_height into sync loop to send correct Version handshake.
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn varint_roundtrip() {
        let values = [0, 1, 127, 128, 255, 256, 16383, 16384, 1_000_000, u32::MAX as usize];
        for &v in &values {
            let encoded = varint_encode(v);
            let (decoded, remaining) = varint_decode(&encoded).expect("decode failed");
            assert_eq!(decoded, v, "varint {v}");
            assert!(remaining.is_empty());
        }
    }

}
