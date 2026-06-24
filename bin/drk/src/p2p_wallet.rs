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
//! Replaces the entire `dwow_core::net` dependency (~13,000 lines of daemon
//! P2P infrastructure) with ~400 lines of wallet-owned code. The wallet is a
//! pure P2P client: outbound TCP+TLS connections only. It never listens for
//! inbound connections. It speaks the same wire protocol as dwowd:
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
use smol::io::{AsyncReadExt, AsyncWriteExt};
use smol::net::TcpStream;
use smol::prelude::*;
use url::Url;
use x509_parser::{
    parse_x509_certificate,
    prelude::{GeneralName, ParsedExtension},
};

use crate::wallet_error::{Error, Result};

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

// ============================================================================
// Hostlist
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostAddr {
    pub url: String,
    pub last_seen: u64,
    pub services: u64,
}

struct Hostlist {
    addrs: Vec<HostAddr>,
    path: Option<PathBuf>,
}

impl Hostlist {
    fn new(path: Option<PathBuf>) -> Self {
        Hostlist { addrs: Vec::new(), path }
    }

    fn load(path: &PathBuf) -> Result<Self> {
        if path.exists() {
            let data = std::fs::read_to_string(path)
                .map_err(|e| Error::Custom(format!("hostlist read: {e}")))?;
            let addrs: Vec<HostAddr> = serde_json::from_str(&data)
                .map_err(|e| Error::Custom(format!("hostlist parse: {e}")))?;
            Ok(Hostlist { addrs, path: Some(path.clone()) })
        } else {
            Ok(Hostlist { addrs: Vec::new(), path: Some(path.clone()) })
        }
    }

    fn save(&self) -> Result<()> {
        if let Some(ref path) = self.path {
            let data = serde_json::to_string(&self.addrs)
                .map_err(|e| Error::Custom(format!("hostlist serialize: {e}")))?;
            std::fs::write(path, &data)
                .map_err(|e| Error::Custom(format!("hostlist write: {e}")))?;
        }
        Ok(())
    }

    fn add(&mut self, url: &str) {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
        // Update existing or push new
        if let Some(existing) = self.addrs.iter_mut().find(|a| a.url == url) {
            existing.last_seen = now;
        } else {
            self.addrs.push(HostAddr { url: url.to_string(), last_seen: now, services: 0 });
        }
    }
}

// ============================================================================
// PeerConnection — TCP+TLS with varint framing
// ============================================================================

/// A framed connection to a single peer. Plain TCP for now (TLS added in follow-up).
pub struct PeerConnection {
    addr: String,
    tcp: smol::net::TcpStream,
}

impl PeerConnection {
    /// Connect to addr, send version handshake.
    pub async fn connect(
        addr: &str,
        _tls_config: &Arc<ClientConfig>,
        _magic_bytes: [u8; 4],
        local_height: u64,
    ) -> Result<Self> {
        // Parse addr as "host:port"
        let (host, port) = parse_host_port(addr)?;
        let tcp = smol::net::TcpStream::connect(format!("{host}:{port}"))
            .await
            .map_err(|e| Error::Custom(format!("TCP connect {addr}: {e}")))?;

        let mut peer = PeerConnection { addr: addr.to_string(), tcp };

        // Send version handshake
        let version = Version {
            version: 1,
            services: 0,
            timestamp: SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs(),
            recv_addr: format!("tcp+tls://{addr}"),
            send_addr: format!("tcp+tls://{addr}"),
            nonce: rand::random(),
            user_agent: "dwow-wallet".to_string(),
            start_height: local_height,
        };

        peer.send_raw("version", &serde_json::to_vec(&version)
            .map_err(|e| Error::Custom(format!("version serialize: {e}")))?).await?;

        Ok(peer)
    }

    /// Send framed message: varint(name_len) + name_bytes + varint(payload_len) + payload.
    async fn send_raw(&mut self, name: &str, payload: &[u8]) -> Result<()> {
        let name_bytes = name.as_bytes();
        let mut frame = Vec::with_capacity(10 + name_bytes.len() + 10 + payload.len());
        frame.extend_from_slice(&varint_encode(name_bytes.len()));
        frame.extend_from_slice(name_bytes);
        frame.extend_from_slice(&varint_encode(payload.len()));
        frame.extend_from_slice(payload);

        self.tcp.write_all(&frame).await
            .map_err(|e| Error::Custom(format!("send {name}: {e}")))?;
        self.tcp.flush().await
            .map_err(|e| Error::Custom(format!("flush {name}: {e}")))?;
        Ok(())
    }

    /// Receive a framed message, returning (name_bytes, payload_bytes).
    async fn recv_raw(&mut self) -> Result<(Vec<u8>, Vec<u8>)> {
        let mut header = vec![0u8; 16];
        let mut header_len = 0;
        // Read varint for message name length
        loop {
            let n = self.tcp.read(&mut header[header_len..header_len + 1]).await
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
        self.tcp.read_exact(&mut name_buf).await
            .map_err(|e| Error::Custom(format!("recv name: {e}")))?;

        // Read payload varint
        let mut payload_header = vec![0u8; 16];
        let mut payload_header_len = 0;
        loop {
            let n = self.tcp.read(&mut payload_header[payload_header_len..payload_header_len + 1]).await
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

        let mut payload = vec![0u8; payload_len];
        if payload_len > 0 {
            self.tcp.read_exact(&mut payload).await
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
// PeerHandle — per-peer typed message dispatch via smol channels
// ============================================================================

/// Handle to a connected peer.
pub struct PeerHandle {
    addr: String,
    tx: smol::channel::Sender<(String, Vec<u8>)>,
}

impl PeerHandle {
    pub fn addr(&self) -> &str { &self.addr }

    /// Send a typed message to this peer.
    pub async fn send<T: Serialize>(&self, msg_name: &str, msg: &T) -> Result<()> {
        let payload = serde_json::to_vec(msg)
            .map_err(|e| Error::Custom(format!("serialize {msg_name}: {e}")))?;
        self.tx.send((msg_name.to_string(), payload)).await
            .map_err(|_| Error::Custom(format!("peer {msg_name} send channel closed")))?;
        Ok(())
    }
}

// ============================================================================
// P2pWallet — main P2P instance
// ============================================================================

pub type P2pWalletPtr = Arc<RwLock<P2pWallet>>;

pub struct P2pWallet {
    config: P2pWalletConfig,
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

    /// Connect to a peer by address.
    pub async fn connect(&mut self, addr: &str) -> Result<Arc<PeerHandle>> {
        if let Some(existing) = self.peers.get(addr) {
            return Ok(existing.clone());
        }

        let height = self.local_height.load(Ordering::Relaxed);
        let mut conn = PeerConnection::connect(
            addr,
            &self.tls_config,
            self.config.magic_bytes,
            height,
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

    /// Get a handle to a specific connected peer.
    pub fn get_peer(&self, addr: &str) -> Option<Arc<PeerHandle>> {
        self.peers.get(addr).cloned()
    }

    /// Broadcast Transaction to all connected peers.
    pub async fn broadcast_tx(&self, tx: &dwow_core::tx::Transaction) -> Result<()> {
        let tx_bytes = dwow_serial::serialize_async(tx).await;
        let mut sent = 0usize;
        for (_, handle) in &self.peers {
            match handle.send("tx", &tx_bytes).await {
                Ok(()) => sent += 1,
                Err(e) => tracing::debug!("broadcast tx: {e}"),
            }
        }
        if sent > 0 {
            tracing::debug!("broadcast tx to {sent} peers");
        }
        Ok(())
    }

    pub fn set_local_height(&self, height: u64) {
        self.local_height.store(height, Ordering::Relaxed);
    }

    pub fn get_local_height(&self) -> u64 {
        self.local_height.load(Ordering::Relaxed)
    }
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

    #[test]
    fn hostlist_serde() {
        let addrs = vec![
            HostAddr { url: "tcp+tls://seed.dark.fi:52666".into(), last_seen: 1234, services: 0 },
        ];
        let json = serde_json::to_string(&addrs).unwrap();
        let back: Vec<HostAddr> = serde_json::from_str(&json).unwrap();
        assert_eq!(back[0].url, addrs[0].url);
    }
}
