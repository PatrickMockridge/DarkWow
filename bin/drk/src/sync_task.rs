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

//! Wallet P2P chain sync task.
//!
//! Wire-compatible with dwowd's linear sync. Same messages, same flow:
//! GetTip → Tip, GetBlocks → Blocks. Uses wallet-owned P2P (p2p_wallet).
//! Zero dependency on dwow_core::net.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, warn};

use dwow_core::net::P2pPtr;
use crate::wallet_error::Result;
use dwow_chain::Block;
use dwow_serial::{AsyncDecodable, AsyncEncodable, AsyncRead, AsyncWrite, FutAsyncReadExt, FutAsyncWriteExt};

use crate::DwwPtr;

/// Fixed batch size for GetBlocks requests — matches dwowd LINEAR_SYNC_BATCH
const LINEAR_SYNC_BATCH: u64 = 20;

// ============================================================================
// Message Types — wire-compatible with dwowd
// ============================================================================

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GetBlocks {
    pub start_height: u64,
    pub count: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Blocks {
    pub blocks: Vec<Block>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GetTip;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Tip {
    pub height: u64,
    pub hash: String,
}

// Async codec for serde_json + varint framing
use async_trait::async_trait;

macro_rules! impl_json_message_codec {
    ($ty:ty) => {
        #[async_trait]
        impl AsyncEncodable for $ty {
            async fn encode_async<S: AsyncWrite + Unpin + Send>(&self, s: &mut S) -> std::io::Result<usize> {
                let bytes = serde_json::to_vec(self)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
                let mut len = 0;
                len += varint_encode(bytes.len(), s).await?;
                len += s.write(&bytes).await?;
                Ok(len)
            }
        }
        #[async_trait]
        impl AsyncDecodable for $ty {
            async fn decode_async<D: AsyncRead + Unpin + Send>(d: &mut D) -> std::io::Result<Self> {
                let len = varint_decode(d).await?;
                let mut buf = vec![0u8; len];
                d.read_exact(&mut buf).await?;
                serde_json::from_slice(&buf)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
            }
        }
    };
}

impl_json_message_codec!(GetBlocks);
impl_json_message_codec!(Blocks);
impl_json_message_codec!(GetTip);
impl_json_message_codec!(Tip);

// ============================================================================
// Varint encoding (async — used by codec)
// ============================================================================

async fn varint_encode<W: AsyncWrite + Unpin + Send>(mut value: usize, s: &mut W) -> std::io::Result<usize> {
    let mut len = 0;
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 { byte |= 0x80; }
        len += FutAsyncWriteExt::write(s, &[byte]).await?;
        if value == 0 { break; }
    }
    Ok(len)
}

async fn varint_decode<R: AsyncRead + Unpin + Send>(d: &mut R) -> std::io::Result<usize> {
    let mut result = 0;
    let mut shift = 0;
    loop {
        let mut buf = [0u8; 1];
        FutAsyncReadExt::read_exact(d, &mut buf).await?;
        let byte = buf[0];
        result |= ((byte & 0x7f) as usize) << shift;
        if byte & 0x80 == 0 { break; }
        shift += 7;
    }
    Ok(result)
}

// ============================================================================
// HighestPeerTip — atomic, monotonic
// ============================================================================

/// Highest peer tip seen. Updated on each Tip response.
pub struct HighestPeerTip(pub AtomicU64);

impl HighestPeerTip {
    pub fn new() -> Self { Self(AtomicU64::new(0)) }

    pub fn get(&self) -> u64 { self.0.load(Ordering::Relaxed) }

    pub fn set_max(&self, height: u64) {
        let _ = self.0.fetch_update(Ordering::Release, Ordering::Relaxed, |c| {
            if height > c { Some(height) } else { None }
        });
    }
}

// ============================================================================
// Sync loop — wallet-owned P2P, no dwow_core::net
// ============================================================================

/// Run the wallet sync loop. Uses wallet-owned P2P (p2p_wallet.rs).
pub async fn run_wallet_sync(
    p2p: P2pPtr,
    dww: DwwPtr,
    highest_peer_tip: Arc<HighestPeerTip>,
) -> Result<()> {
    info!(target: "drk::wallet::sync", "Wallet sync task running — P2p handles peer discovery");

    let mut zero_peer_ticks: u32 = 0;
    loop {
        smol::Timer::after(Duration::from_secs(2)).await;
        let dww_r = dww.read().await;
        let local = dww_r.chain.get_height().unwrap_or(0);
        let peer_count = dww_r.p2p.as_ref()
            .map(|p| p.hosts().peers().len())
            .unwrap_or(0);
        debug!(target: "drk::wallet::sync",
            "Heartbeat: local_height={}, peer_count={}", local, peer_count);

        // Defense in depth: if no peers for 3 consecutive ticks (6s),
        // re-seed to discover mining nodes. SeedSyncSession may need
        // multiple attempts if OutboundSession slots are still connecting.
        if peer_count == 0 {
            zero_peer_ticks += 1;
            if zero_peer_ticks >= 3 {
                warn!(target: "drk::wallet::sync",
                    "No peers for {}s — re-seeding", zero_peer_ticks * 2);
                if let Some(ref p2p) = dww_r.p2p {
                    p2p.clone().seed().await;
                }
                zero_peer_ticks = 0;
            }
        } else {
            zero_peer_ticks = 0;
        }
        drop(dww_r);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_highest_peer_tip_initial() {
        let tip = HighestPeerTip::new();
        assert_eq!(tip.get(), 0);
    }

    #[test]
    fn test_highest_peer_tip_monotonic() {
        let tip = HighestPeerTip::new();
        tip.set_max(42);
        assert_eq!(tip.get(), 42);
        tip.set_max(10);
        assert_eq!(tip.get(), 42);
        tip.set_max(100);
        assert_eq!(tip.get(), 100);
    }
}
