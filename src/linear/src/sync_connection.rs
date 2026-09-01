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

//! Unified sync connection (SyncPeer/SyncServer).
//!
//! Spec: sync-protocol.md §8 (unified connection), §9 (inherent safety), §11 (reuse), §13 (async logic).
//!
//! One minimal, self-contained sync transport used identically by the wallet,
//! observer, and mining node: `dial → handshake → GetTip/GetBlocks`. It replaces
//! the divergent session/hostlist/seed/refine/ban slice of `dwow_core::net` that
//! the wallet and node previously rode on separately, and — unlike that path —
//! every failure is logged.
//!
//! Framing: `magic(4) | command(varint+bytes) | payload(varint+json)`.
//! Reuses `dwow_core::net::transport` (TCP+TLS) and `crate::sync_types` codecs.

use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use dwow_core::net::transport::{Dialer, Listener};
use dwow_serial::{FutAsyncReadExt, FutAsyncWriteExt};
use dwow_sdk::blockchain::BlockHeight;
use futures_rustls::rustls::crypto::{ring, CryptoProvider};

use crate::sync_types::{
    varint_decode, varint_encode, BlockHash, Blocks, BroadcastTx, BroadcastTxAck, GetBlocks,
    GetTip, Tip,
};

/// Install the rustls crypto provider (idempotent). `dwow_core::net::P2p::new`
/// does this, but the unified sync connection bypasses `P2p` and drives the
/// TCP+TLS transport directly, so it must install the provider itself.
fn install_crypto_provider() {
    let _ = CryptoProvider::install_default(ring::default_provider());
}

/// Sync protocol version exchanged during the handshake. A mismatch SHALL be
/// rejected with a logged error (unlike the old wallet path, which was silent).
pub const SYNC_PROTOCOL_VERSION: (u64, u64) = (1, 0);

/// Port offset for the dedicated sync listener, relative to the node's
/// tx/broadcast inbound port. The node serves sync on `inbound + OFFSET`; the
/// wallet dials `peer + OFFSET` for sync. Keeps sync and tx/broadcast on
/// distinct listeners (two listeners cannot share a port).
pub const SYNC_PORT_OFFSET: u16 = 2;

/// Tip request timeout.
pub const TIP_TIMEOUT: Duration = Duration::from_secs(5);
/// Handshake (Hello/Ack) read timeout — M7.3: the client handshake read
/// previously had no timeout, so a peer that accepted the TCP/TLS dial but
/// never replied to `Hello` would hang the sync pass indefinitely.
pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(15);
/// Block request timeout.
pub const BLOCKS_TIMEOUT: Duration = Duration::from_secs(30);
/// Max blocks served in a single response.
pub const LINEAR_SYNC_BATCH: usize = 20;
/// Max cumulative encoded size of a single `Blocks` response, under the 16 MiB
/// `Blocks` wire cap (sync-protocol.md §4/§8.6.2). A batch of 20 large blocks
/// could otherwise exceed the cap and be dropped at the wire.
pub const MAX_BATCH_BYTES: usize = 12 * 1024 * 1024;
/// Upper bound on any single sync-frame payload. Matches the `Blocks` wire cap
/// (16 MiB, sync-protocol.md §4). A peer-controlled length above this is rejected
/// before allocation — prevents an unbounded `vec![0u8; payload_len]` OOM.
pub const MAX_FRAME_PAYLOAD: usize = 16 * 1024 * 1024;

const CMD_GET_TIP: &str = "lineargettip";
const CMD_TIP: &str = "lineartip";
const CMD_GET_BLOCKS: &str = "lineargetblocks";
const CMD_BLOCKS: &str = "linearblocks";
const CMD_HELLO: &str = "synchello";
const CMD_HELLO_ACK: &str = "synchelloack";
const CMD_BROADCAST_TX: &str = "broadcasttx";
const CMD_BROADCAST_TX_ACK: &str = "broadcasttxack";

/// Tx broadcast timeout — shorter than block fetch since a tx is a single frame.
pub const BROADCAST_TIMEOUT: Duration = Duration::from_secs(10);

/// Fire-and-forget sink the server invokes on receipt of a `BroadcastTx`.
/// The caller (node) owns admission into the mempool; the sync server only
/// forwards the decoded transaction and acks the deterministic txid.
pub type TxSink = Arc<dyn Fn(dwow_core::tx::Transaction) + Send + Sync>;

// ── Handshake ──────────────────────────────────────────────────────────

/// Client → server: version + genesis identity.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct SyncHello {
    major: u64,
    minor: u64,
    genesis_hash: Option<BlockHash>,
}

/// Server → client: accept or reject.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct SyncHelloAck {
    ok: bool,
}

// ── Framing ────────────────────────────────────────────────────────────

/// Write magic + command + a JSON payload (varint-length-prefixed) to `w`.
async fn write_json_frame(
    w: &mut (impl dwow_serial::AsyncWrite + Unpin + Send),
    magic: &[u8; 4],
    command: &str,
    payload: &[u8],
) -> std::io::Result<()> {
    FutAsyncWriteExt::write_all(w, magic).await?;
    varint_encode(command.len(), w).await?;
    FutAsyncWriteExt::write_all(w, command.as_bytes()).await?;
    varint_encode(payload.len(), w).await?;
    FutAsyncWriteExt::write_all(w, payload).await?;
    FutAsyncWriteExt::flush(w).await?;
    Ok(())
}

/// Read a command name and its raw JSON payload (varint-length-prefixed),
/// after validating the 4 magic bytes. Returns (command, payload_bytes).
async fn read_frame(
    r: &mut (impl dwow_serial::AsyncRead + Unpin + Send),
    magic: &[u8; 4],
) -> std::io::Result<(String, Vec<u8>)> {
    let mut got_magic = [0u8; 4];
    FutAsyncReadExt::read_exact(r, &mut got_magic).await?;
    if &got_magic != magic {
        warn!(target: "dwow_chain::sync_connection", "magic bytes mismatch: expected {magic:?} got {got_magic:?}");
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "magic bytes mismatch"));
    }
    let cmd_len = varint_decode(r).await?;
    if cmd_len > 255 {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "command too long"));
    }
    let mut cmd = vec![0u8; cmd_len];
    FutAsyncReadExt::read_exact(r, &mut cmd).await?;
    let command = String::from_utf8(cmd)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let payload_len = varint_decode(r).await?;
    if payload_len > MAX_FRAME_PAYLOAD {
        warn!(target: "dwow_chain::sync_connection", "oversized sync frame payload: {payload_len} bytes (max {MAX_FRAME_PAYLOAD})");
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "payload too large"));
    }
    let mut payload = vec![0u8; payload_len];
    FutAsyncReadExt::read_exact(r, &mut payload).await?;
    Ok((command, payload))
}

/// Encode a sync message as its raw JSON body (the frame adds the varint length).
fn encode_msg<M: Serialize>(msg: &M) -> std::io::Result<Vec<u8>> {
    serde_json::to_vec(msg).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

/// Decode a sync message from its raw JSON body.
fn decode_msg<M: for<'de> Deserialize<'de>>(payload: &[u8]) -> std::io::Result<M> {
    serde_json::from_slice(payload).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

// ── SyncPeer (client) ──────────────────────────────────────────────────

/// A connected, handshaken sync peer. Owns the framed read/write halves.
pub struct SyncPeer {
    writer: smol::io::WriteHalf<Box<dyn dwow_core::net::transport::PtStream>>,
    reader: smol::io::ReadHalf<Box<dyn dwow_core::net::transport::PtStream>>,
    magic: [u8; 4],
    /// Dialed peer URL — stable identity for per-peer scoring/punishment.
    url: url::Url,
}

impl SyncPeer {
    /// Dial a peer over TCP+TLS, then perform the version/genesis handshake.
    pub async fn dial(
        url: url::Url,
        magic: [u8; 4],
        genesis_hash: Option<BlockHash>,
        timeout: Duration,
    ) -> dwow_core::Result<SyncPeer> {
        install_crypto_provider();
        let dialer = Dialer::new(url.clone(), None, None, true).await.map_err(|e| {
            warn!(target: "dwow_chain::sync_connection", "Dialer::new({url}) failed: {e}");
            dwow_core::Error::Custom(format!("dial {url}: {e}"))
        })?;
        let stream = dialer.dial(Some(timeout)).await.map_err(|e| {
            warn!(target: "dwow_chain::sync_connection", "dial {url} failed: {e}");
            dwow_core::Error::Custom(format!("dial {url}: {e}"))
        })?;
        let (reader, writer) = smol::io::split(stream);
        let mut peer = SyncPeer { writer, reader, magic, url: url.clone() };

        // Handshake: send Hello, await Ack.
        let hello = SyncHello {
            major: SYNC_PROTOCOL_VERSION.0,
            minor: SYNC_PROTOCOL_VERSION.1,
            genesis_hash,
        };
        let hello_bytes = serde_json::to_vec(&hello)
            .map_err(|e| dwow_core::Error::Custom(format!("encode hello: {e}")))?;
        write_json_frame(&mut peer.writer, &magic, CMD_HELLO, &hello_bytes).await
            .map_err(|e| dwow_core::Error::Custom(format!("send hello to {url}: {e}")))?;

        let (cmd, ack_bytes) = smol::future::or(
            async { read_frame(&mut peer.reader, &magic).await },
            async {
                smol::Timer::after(HANDSHAKE_TIMEOUT).await;
                Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "handshake ack timed out"))
            },
        )
        .await
        .map_err(|e| dwow_core::Error::Custom(format!("read hello ack from {url}: {e}")))?;
        if cmd != CMD_HELLO_ACK {
            return Err(dwow_core::Error::Custom(format!("unexpected handshake reply {cmd} from {url}")));
        }
        let ack: SyncHelloAck = serde_json::from_slice(&ack_bytes)
            .map_err(|e| dwow_core::Error::Custom(format!("decode hello ack: {e}")))?;
        if !ack.ok {
            warn!(target: "dwow_chain::sync_connection", "peer {url} rejected handshake (version or genesis mismatch)");
            return Err(dwow_core::Error::Custom(format!("peer {url} rejected handshake")));
        }
        info!(target: "dwow_chain::sync_connection", "synced peer connected: {url}");
        Ok(peer)
    }

    /// The dialed peer URL — stable identity for per-peer scoring/punishment.
    pub fn url(&self) -> &url::Url {
        &self.url
    }

    /// Request the chain tip.
    pub async fn request_tip(&mut self) -> dwow_core::Result<Tip> {
        let payload = encode_msg(&GetTip)
            .map_err(|e| dwow_core::Error::Custom(format!("encode GetTip: {e}")))?;
        write_json_frame(&mut self.writer, &self.magic, CMD_GET_TIP, &payload).await
            .map_err(|e| dwow_core::Error::Custom(format!("send GetTip: {e}")))?;

        let (cmd, payload) = smol::future::or(
            async { read_frame(&mut self.reader, &self.magic).await },
            async {
                smol::Timer::after(TIP_TIMEOUT).await;
                Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "GetTip timed out"))
            },
        ).await
            .map_err(|e| dwow_core::Error::Custom(format!("read Tip: {e}")))?;

        if cmd != CMD_TIP {
            return Err(dwow_core::Error::Custom(format!("unexpected reply {cmd} to GetTip")));
        }
        decode_msg::<Tip>(&payload)
            .map_err(|e| dwow_core::Error::Custom(format!("decode Tip: {e}")))
    }

    /// Request a batch of blocks starting at `start_height`.
    pub async fn request_blocks(
        &mut self,
        start_height: BlockHeight,
        count: u64,
    ) -> dwow_core::Result<Vec<crate::Block>> {
        let request = GetBlocks { start_height, count };
        let payload = encode_msg(&request)
            .map_err(|e| dwow_core::Error::Custom(format!("encode GetBlocks: {e}")))?;
        write_json_frame(&mut self.writer, &self.magic, CMD_GET_BLOCKS, &payload).await
            .map_err(|e| dwow_core::Error::Custom(format!("send GetBlocks: {e}")))?;

        let (cmd, payload) = smol::future::or(
            async { read_frame(&mut self.reader, &self.magic).await },
            async {
                smol::Timer::after(BLOCKS_TIMEOUT).await;
                Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "GetBlocks timed out"))
            },
        ).await
            .map_err(|e| dwow_core::Error::Custom(format!("read Blocks: {e}")))?;

        if cmd != CMD_BLOCKS {
            return Err(dwow_core::Error::Custom(format!("unexpected reply {cmd} to GetBlocks")));
        }
        let blocks: Blocks = decode_msg(&payload)
            .map_err(|e| dwow_core::Error::Custom(format!("decode Blocks: {e}")))?;
        Ok(blocks.blocks)
    }

    /// Broadcast a transaction over the sync connection and return its txid.
    ///
    /// Serializes the tx with `dwow_serial` (the same binary encoding the wallet
    /// already produces), hex-encodes it for the JSON frame, and awaits the
    /// server's `BroadcastTxAck`.
    pub async fn broadcast_tx(&mut self, tx: &dwow_core::tx::Transaction) -> dwow_core::Result<String> {
        let tx_hex = hex::encode(dwow_serial::serialize(tx));
        let req = BroadcastTx { tx_hex };
        let payload = encode_msg(&req)
            .map_err(|e| dwow_core::Error::Custom(format!("encode BroadcastTx: {e}")))?;
        write_json_frame(&mut self.writer, &self.magic, CMD_BROADCAST_TX, &payload).await
            .map_err(|e| dwow_core::Error::Custom(format!("send BroadcastTx: {e}")))?;

        let (cmd, payload) = smol::future::or(
            async { read_frame(&mut self.reader, &self.magic).await },
            async {
                smol::Timer::after(BROADCAST_TIMEOUT).await;
                Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "BroadcastTx timed out"))
            },
        ).await
            .map_err(|e| dwow_core::Error::Custom(format!("read BroadcastTxAck: {e}")))?;

        if cmd != CMD_BROADCAST_TX_ACK {
            return Err(dwow_core::Error::Custom(format!("unexpected reply {cmd} to BroadcastTx")));
        }
        let ack: BroadcastTxAck = decode_msg(&payload)
            .map_err(|e| dwow_core::Error::Custom(format!("decode BroadcastTxAck: {e}")))?;
        Ok(ack.txid)
    }
}

// ── SyncServer (serve) ─────────────────────────────────────────────────

/// Serves `GetTip`/`GetBlocks`/`BroadcastTx` from a chain state on an inbound
/// TCP+TLS listener. Tx broadcast is forwarded to the optional `tx_sink`.
pub struct SyncServer {
    listener: Box<dyn dwow_core::net::transport::PtListener>,
    magic: [u8; 4],
    chain_state: Arc<crate::CChainState>,
    tx_sink: Option<TxSink>,
}

impl SyncServer {
    /// Bind an inbound TCP+TLS listener.
    pub async fn listen(
        url: url::Url,
        magic: [u8; 4],
        chain_state: Arc<crate::CChainState>,
        tx_sink: Option<TxSink>,
    ) -> dwow_core::Result<SyncServer> {
        install_crypto_provider();
        let listener = Listener::new(url.clone(), None, true).await.map_err(|e| {
            warn!(target: "dwow_chain::sync_connection", "Listener::new({url}) failed: {e}");
            dwow_core::Error::Custom(format!("listen {url}: {e}"))
        })?;
        let listener = listener.listen().await.map_err(|e| {
            warn!(target: "dwow_chain::sync_connection", "listen {url} failed: {e}");
            dwow_core::Error::Custom(format!("listen {url}: {e}"))
        })?;
        Ok(SyncServer { listener, magic, chain_state, tx_sink })
    }

    /// Accept and serve connections forever.
    pub async fn run(self) -> dwow_core::Result<()> {
        info!(target: "dwow_chain::sync_connection", "sync server accepting");
        loop {
            let (stream, peer_url) = match self.listener.next().await {
                Ok(x) => x,
                Err(e) => {
                    warn!(target: "dwow_chain::sync_connection", "accept failed: {e}");
                    continue;
                }
            };
            let magic = self.magic;
            let chain_state = self.chain_state.clone();
            let tx_sink = self.tx_sink.clone();
            let url = peer_url.clone();
            smol::spawn(async move {
                if let Err(e) = serve_conn(stream, peer_url, magic, chain_state, tx_sink).await {
                    warn!(target: "dwow_chain::sync_connection", "serve {url}: {e}");
                }
            }).detach();
        }
    }
}

async fn serve_conn(
    stream: Box<dyn dwow_core::net::transport::PtStream>,
    peer_url: url::Url,
    magic: [u8; 4],
    chain_state: Arc<crate::CChainState>,
    tx_sink: Option<TxSink>,
) -> dwow_core::Result<()> {
    let (mut reader, mut writer) = smol::io::split(stream);

    // Handshake: read Hello, reply Ack.
    let (cmd, hello_bytes) = read_frame(&mut reader, &magic).await
        .map_err(|e| dwow_core::Error::Custom(format!("read hello from {peer_url}: {e}")))?;
    let ok = if cmd == CMD_HELLO {
        match serde_json::from_slice::<SyncHello>(&hello_bytes) {
            Ok(hello) => {
                let version_ok = (hello.major, hello.minor) == SYNC_PROTOCOL_VERSION;
                let genesis_ok = hello.genesis_hash.as_ref()
                    .map(|h| chain_state.genesis_hash().map(|g| BlockHash::from_hash(g) == *h).unwrap_or(true))
                    .unwrap_or(true);
                version_ok && genesis_ok
            }
            Err(_) => false,
        }
    } else {
        false
    };
    let ack = SyncHelloAck { ok };
    let ack_bytes = serde_json::to_vec(&ack)
        .map_err(|e| dwow_core::Error::Custom(format!("encode ack: {e}")))?;
    write_json_frame(&mut writer, &magic, CMD_HELLO_ACK, &ack_bytes).await
        .map_err(|e| dwow_core::Error::Custom(format!("send ack to {peer_url}: {e}")))?;
    if !ok {
        warn!(target: "dwow_chain::sync_connection", "rejected handshake from {peer_url}");
        return Ok(());
    }

    // Serve GetTip/GetBlocks. No server-side idle timeout: the client may be
    // legitimately busy applying a batch (genesis bootstrap can take minutes),
    // during which it sends no frames — an idle close here breaks the sync
    // connection with "Broken pipe" on the client's next GetBlocks.
    loop {
        let (cmd, payload) = match read_frame(&mut reader, &magic).await {
            Ok(x) => x,
            Err(e) => {
                if e.kind() == std::io::ErrorKind::UnexpectedEof {
                    return Ok(()); // normal disconnect
                }
                return Err(dwow_core::Error::Custom(format!("read from {peer_url}: {e}")));
            }
        };
        match cmd.as_str() {
            CMD_GET_TIP => {
                let (height, hash) = match chain_state.tip_hash() {
                    Some((h, hash)) => (h, BlockHash::from_hash(hash)),
                    None => {
                        // R4: a zero hash at a real height is a poison tip — the
                        // receiver's reorg/quorum vote must not count it. Log it
                        // so the failure is observable, and serve the sentinel.
                        let h = chain_state.get_height();
                        if !h.is_zero() {
                            tracing::warn!(
                                target: "dwow_chain::sync_connection",
                                "tip_hash failed at height {} — serving height with zero hash", h
                            );
                        }
                        (h, BlockHash::zero())
                    }
                };
                let genesis_hash = chain_state.genesis_hash().map(BlockHash::from_hash);
                let tip = Tip { height, hash, genesis_hash };
                let payload = encode_msg(&tip)
                    .map_err(|e| dwow_core::Error::Custom(format!("encode Tip: {e}")))?;
                write_json_frame(&mut writer, &magic, CMD_TIP, &payload).await
                    .map_err(|e| dwow_core::Error::Custom(format!("send Tip: {e}")))?;
            }
            CMD_GET_BLOCKS => {
                let request: GetBlocks = decode_msg(&payload)
                    .map_err(|e| dwow_core::Error::Custom(format!("decode GetBlocks: {e}")))?;
                let count = if request.start_height == BlockHeight::GENESIS {
                    1
                } else {
                    std::cmp::min(request.count as usize, LINEAR_SYNC_BATCH)
                };
                let mut blocks = Vec::with_capacity(count);
                // R5/B6: respect the wire cap. A batch of 20 large (4 MiB) blocks
                // would exceed the 16 MiB `Blocks` cap and be dropped at the wire.
                // Trim by cumulative encoded size (MAX_BATCH_BYTES budget, under cap).
                let mut bytes_used: usize = 0;
                let mut height = request.start_height;
                for _ in 0..count {
                    match chain_state.get_block(height) {
                        Ok(block) => {
                            let sz = dwow_serial::serialize(&block).len();
                            if !blocks.is_empty() && bytes_used + sz > MAX_BATCH_BYTES {
                                break;
                            }
                            bytes_used += sz;
                            blocks.push(block);
                        }
                        Err(_) => break,
                    }
                    height = height.succ();
                }
                let response = Blocks { blocks };
                let payload = encode_msg(&response)
                    .map_err(|e| dwow_core::Error::Custom(format!("encode Blocks: {e}")))?;
                write_json_frame(&mut writer, &magic, CMD_BLOCKS, &payload).await
                    .map_err(|e| dwow_core::Error::Custom(format!("send Blocks: {e}")))?;
            }
            CMD_BROADCAST_TX => {
                let req: BroadcastTx = decode_msg(&payload)
                    .map_err(|e| dwow_core::Error::Custom(format!("decode BroadcastTx: {e}")))?;
                let tx_bytes = hex::decode(&req.tx_hex)
                    .map_err(|e| dwow_core::Error::Custom(format!("hex-decode BroadcastTx: {e}")))?;
                let tx: dwow_core::tx::Transaction = dwow_serial::deserialize(&tx_bytes)
                    .map_err(|e| dwow_core::Error::Custom(format!("deserialize Transaction: {e}")))?;
                // Deterministic txid — acked regardless of admission outcome,
                // matching the fire-and-forget semantics of the old P2p path.
                let txid = tx.hash().to_string();
                if let Some(sink) = &tx_sink {
                    sink(tx);
                } else {
                    warn!(target: "dwow_chain::sync_connection",
                        "BroadcastTx received from {peer_url} but no tx sink is configured");
                }
                let ack = BroadcastTxAck { txid };
                let payload = encode_msg(&ack)
                    .map_err(|e| dwow_core::Error::Custom(format!("encode BroadcastTxAck: {e}")))?;
                write_json_frame(&mut writer, &magic, CMD_BROADCAST_TX_ACK, &payload).await
                    .map_err(|e| dwow_core::Error::Custom(format!("send BroadcastTxAck: {e}")))?;
            }
            _ => {
                warn!(target: "dwow_chain::sync_connection", "unknown sync command {cmd} from {peer_url}");
                return Ok(());
            }
        }
    }
}
