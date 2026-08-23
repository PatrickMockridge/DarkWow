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

//! Shared sync protocol message types.
//!
//! ## Protocol Invariants (sync-protocol.md §1-§7)
//!
//! §1 — **Message Type Authority.** These types are the single source of truth
//! for the linear sync wire protocol. All nodes (wallet, mining, observer)
//! import from this module. No node defines its own copy.
//!
//! §2 — **Nominal Types on the Wire.** Every consensus scalar uses its nominal
//! newtype (`BlockHeight`, not `u64`). Serde is transparent (JSON number).
//!
//! §3 — **genesis_hash Validation.** Every `Tip` carries `genesis_hash:
//! Option<String>`. Receivers MUST compare against local genesis and skip
//! mismatched peers.
//!
//! §4 — **Unified MAX_BYTES.** Canonical values: GetTip=256, Tip=512,
//! GetBlocks=256, Blocks=16MiB.

use serde::{Deserialize, Serialize};

use crate::Block;
use dwow_sdk::blockchain::BlockHeight;
use dwow_serial::{AsyncRead, AsyncWrite, FutAsyncReadExt, FutAsyncWriteExt};
// Note: AsyncEncodable/AsyncDecodable codec impls for these types live in
// each node crate (bin/dww, bin/dwowd) since they require the async_trait
// dependency and local varint functions. The struct definitions and varint
// encoding ARE shared — only the trait impls are per-crate.

// ============================================================================
// Message Types
// ============================================================================

/// Request blocks starting from a given height.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GetBlocks {
    pub start_height: BlockHeight,
    pub count: u64,
}

/// Response containing blocks.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Blocks {
    pub blocks: Vec<Block>,
}

/// Request to get the current chain tip.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GetTip;

/// §8.2.1: BlockHash — nominal blake3 hash for P2P boundary types.
/// Serializes as hex string on the wire for backward compatibility;
/// re-lifts through validating `from_hex_str` constructor.
#[repr(transparent)]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct BlockHash(pub(crate) blake3::Hash);

// blake3::Hash doesn't implement Ord — compare by bytes.
impl PartialOrd for BlockHash {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for BlockHash {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.as_bytes().cmp(other.0.as_bytes())
    }
}

impl BlockHash {
    /// Construct from a blake3::Hash.
    pub fn from_hash(h: blake3::Hash) -> Self {
        Self(h)
    }

    /// Construct from a hex string (the wire format for Tip.hash).
    /// Returns None if the string is empty (genesis sentinel) or not valid hex.
    pub fn from_hex_str(s: &str) -> Option<Self> {
        if s.is_empty() {
            return None;
        }
        let bytes = hex::decode(s).ok()?;
        if bytes.len() != 32 {
            return None;
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        Some(Self(blake3::Hash::from_bytes(arr)))
    }

    /// Zero-hash sentinel for height-0 peers with no blocks.
    pub fn zero() -> Self {
        Self(blake3::Hash::from_bytes([0u8; 32]))
    }

    /// Hex representation for display/comparison.
    pub fn to_hex(&self) -> String {
        hex::encode(self.0.as_bytes())
    }

    /// Whether this is the zero sentinel (height-0 empty hash).
    pub fn is_zero(&self) -> bool {
        self.0 == blake3::Hash::from_bytes([0u8; 32])
    }
}

impl std::fmt::Display for BlockHash {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(&self.to_hex())
    }
}

// §8.5: serde serialization as hex string — wire-compatible with the
// existing String format. Deserialization validates through from_hex_str.
impl Serialize for BlockHash {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.to_hex().serialize(s)
    }
}

impl<'de> Deserialize<'de> for BlockHash {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let hex_str = String::deserialize(d)?;
        BlockHash::from_hex_str(&hex_str)
            .ok_or_else(|| serde::de::Error::custom(format!(
                "invalid BlockHash hex string: '{}'", hex_str
            )))
    }
}

// §8.2.1: BlockHash exhibits ↓verify — a process holding a BlockHash can
// prove it knows a specific chain position.
impl dwow_core::barb::ExhibitsBarb for BlockHash {
    fn exhibited_barbs() -> &'static [dwow_core::barb::BarbId] {
        &[dwow_core::barb::BarbId::Verify]
    }
}

/// Response containing chain tip info.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Tip {
    pub height: BlockHeight,
    /// §8.2.1: block hash on the wire SHALL be BlockHash, never bare String.
    pub hash: BlockHash,
    /// Genesis block hash — allows peers to detect incompatible chains
    /// before downloading blocks (defense-in-depth, HAZID F1/F7).
    /// `Option` + `#[serde(default)]` is forward/backward compatible:
    /// old nodes ignore this field, new nodes treat None as unverified.
    #[serde(default)]
    pub genesis_hash: Option<BlockHash>,
}

/// Broadcast a transaction to the sync peer for mempool admission.
///
/// The transaction is carried as hex-encoded `dwow_serial` bytes (the same
/// binary encoding the wallet already produces via `serialize`), matching the
/// hex-string convention used by `BlockHash` above. This lets tx broadcast ride
/// the SAME SyncPeer/SyncServer rail as block sync, eliminating the separate
/// `dwow_core::net::P2p` tx path (sync-protocol.md §12, harmonization).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BroadcastTx {
    pub tx_hex: String,
}

/// Acknowledgment carrying the admitted transaction id.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BroadcastTxAck {
    pub txid: String,
}

// ============================================================================
// Varint encoding (shared — byte-identical to both wallet and node)
// ============================================================================

/// Encode a usize as a varint into an async writer.
pub async fn varint_encode<W: AsyncWrite + Unpin + Send>(
    mut value: usize,
    s: &mut W,
) -> std::io::Result<usize> {
    let mut len = 0;
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        len += FutAsyncWriteExt::write(s, &[byte]).await?;
        if value == 0 {
            break;
        }
    }
    Ok(len)
}

/// Decode a varint from an async reader into a usize.
pub async fn varint_decode<R: AsyncRead + Unpin + Send>(
    d: &mut R,
) -> std::io::Result<usize> {
    let mut result = 0;
    let mut shift = 0;
    loop {
        let mut buf = [0u8; 1];
        FutAsyncReadExt::read_exact(d, &mut buf).await?;
        let byte = buf[0];
        result |= ((byte & 0x7f) as usize) << shift;
        if byte & 0x80 == 0 {
            break;
        }
        shift += 7;
    }
    Ok(result)
}

// ============================================================================
// Codec + P2P message registration (gated behind sync-p2p feature)
// ============================================================================
//
// Orphan rule: AsyncEncodable/AsyncDecodable (from dwow_serial) and Message
// (from dwow_core) are foreign traits. Their impls MUST live in the same crate
// as the type definitions. Gate behind sync-p2p so dwow_chain consumers that
// don't need P2P networking (tests, tools) aren't forced to compile the net stack.

#[cfg(feature = "sync-p2p")]
mod p2p_impls {
    use async_trait::async_trait;
    use dwow_core::{
        impl_p2p_message,
        net::{Message, metering::MeteringConfiguration},
        util::time::NanoTimestamp,
    };
    use dwow_serial::{AsyncDecodable, AsyncEncodable, AsyncRead, AsyncWrite, FutAsyncReadExt, FutAsyncWriteExt};

    use super::*;

    // ── Codec macro ──────────────────────────────────────────────────

    macro_rules! impl_sync_codec {
        ($ty:ty) => {
            #[async_trait]
            impl AsyncEncodable for $ty {
                async fn encode_async<S: AsyncWrite + Unpin + Send>(
                    &self, s: &mut S,
                ) -> std::io::Result<usize> {
                    let bytes = serde_json::to_vec(self)
                        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
                    let mut len = 0;
                    len += super::varint_encode(bytes.len(), s).await?;
                    len += FutAsyncWriteExt::write(s, &bytes).await?;
                    Ok(len)
                }
            }
            #[async_trait]
            impl AsyncDecodable for $ty {
                async fn decode_async<D: AsyncRead + Unpin + Send>(
                    d: &mut D,
                ) -> std::io::Result<Self> {
                    let len = super::varint_decode(d).await?;
                    let mut buf = vec![0u8; len];
                    FutAsyncReadExt::read_exact(d, &mut buf).await?;
                    serde_json::from_slice(&buf)
                        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
                }
            }
        };
    }

    impl_sync_codec!(GetBlocks);
    impl_sync_codec!(Blocks);
    impl_sync_codec!(GetTip);
    impl_sync_codec!(Tip);

    // ── P2P message registration ─────────────────────────────────────
    //
    // MAX_BYTES per sync-protocol.md §4:
    //   GetTip: 256, Tip: 512, GetBlocks: 256,
    //   Blocks: 0 (unlimited, consensus-level validation)

    const MAX_SMALL: u64 = 256;
    const MAX_TIP: u64 = 512;
    // §8.6.2: MAX_BYTES=0 (unlimited) SHALL NOT appear on any message type.
    // 16 MiB accommodates the genesis block (9 contract WASM deployments,
    // measured ~11.35 MiB) served ALONE by handle_get_blocks, plus headroom.
    // The normal MAX_BLOCK_SIZE (4 MiB) does NOT bound genesis — genesis is a
    // special multi-contract bootstrap block. Metering is not a substitute.
    const MAX_BLOCK_BATCH: u64 = 16 * 1024 * 1024; // 16 MiB

    const SYNC_METERING: MeteringConfiguration = MeteringConfiguration {
        threshold: 20,
        sleep_step: 500,
        expiry_time: NanoTimestamp::from_secs(5),
    };

    const SYNC_BARBS: &'static [dwow_core::barb::BarbId] = &[
        dwow_core::barb::BarbId::Verify,
        dwow_core::barb::BarbId::SyncBarrier,
        dwow_core::barb::BarbId::GossipForward,
    ];

    impl_p2p_message!(GetBlocks, "lineargetblocks", MAX_SMALL, 1, SYNC_METERING, SYNC_BARBS);
    impl_p2p_message!(Blocks, "linearblocks", MAX_BLOCK_BATCH, 1, SYNC_METERING, SYNC_BARBS);
    impl_p2p_message!(GetTip, "lineargettip", MAX_SMALL, 1, SYNC_METERING, SYNC_BARBS);
    impl_p2p_message!(Tip, "lineartip", MAX_TIP, 1, SYNC_METERING, SYNC_BARBS);

    // ── BoundaryCodec with JSON encoding ───────────────────────────────
    //
    // These types use serde_json for wire encoding (matching the async
    // codec at impl_sync_codec! above). BoundaryCodec requires Encodable +
    // Decodable supertraits (§10.5), so we provide JSON-based impls instead
    // of deriving SerialEncodable/SerialDecodable (which would use
    // dwow_serial's binary format and break wire compatibility).
    macro_rules! impl_sync_boundary_codec {
        ($ty:ty, $max_bytes:expr, $metering_score:expr) => {
            impl dwow_serial::Encodable for $ty {
                fn encode<W: std::io::Write>(&self, e: &mut W) -> std::io::Result<usize> {
                    let json = serde_json::to_vec(self)
                        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
                    e.write(&json)
                }
            }
            impl dwow_serial::Decodable for $ty {
                fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> {
                    let mut buf = Vec::new();
                    d.read_to_end(&mut buf)?;
                    serde_json::from_slice(&buf)
                        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
                }
            }
            dwow_core::impl_boundary_codec!($ty, $max_bytes, $metering_score, SYNC_BARBS);
        };
    }

    impl_sync_boundary_codec!(GetBlocks, MAX_SMALL, 1);
    impl_sync_boundary_codec!(Blocks, MAX_BLOCK_BATCH, 1);
    impl_sync_boundary_codec!(GetTip, MAX_SMALL, 1);
    impl_sync_boundary_codec!(Tip, MAX_TIP, 1);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// §6: Wire Format Stability — golden-file test.
    /// Any change to a sync message type that alters the JSON shape breaks this test.
    #[test]
    fn wire_format_golden() {
        // GetTip is empty — serializes as "null" in JSON
        let get_tip = GetTip;
        let json = serde_json::to_string(&get_tip).expect("GetTip serialize");
        assert_eq!(json, "null", "GetTip JSON shape changed — wire format broken");

        // Tip with known values
        let tip = Tip {
            height: BlockHeight::new(42),
            hash: BlockHash::from_hash(blake3::Hash::from_bytes([0xAAu8; 32])),
            genesis_hash: Some(BlockHash::from_hash(blake3::Hash::from_bytes([0xBBu8; 32]))),
        };
        let json = serde_json::to_string(&tip).expect("Tip serialize");
        // Round-trip: deserialize must produce identical struct
        let tip2: Tip = serde_json::from_str(&json).expect("Tip deserialize");
        assert_eq!(tip2.height, BlockHeight::new(42));
        assert_eq!(tip2.hash, BlockHash::from_hash(blake3::Hash::from_bytes([0xAAu8; 32])));
        assert_eq!(tip2.genesis_hash, Some(BlockHash::from_hash(blake3::Hash::from_bytes([0xBBu8; 32]))));

        // GetBlocks with BlockHeight
        let gb = GetBlocks { start_height: BlockHeight::new(1), count: 20 };
        let json = serde_json::to_string(&gb).expect("GetBlocks serialize");
        let gb2: GetBlocks = serde_json::from_str(&json).expect("GetBlocks deserialize");
        assert_eq!(gb2.start_height, BlockHeight::new(1));
        assert_eq!(gb2.count, 20);

        // Tip without genesis_hash (backward compat)
        let tip_no_genesis = Tip {
            height: BlockHeight::new(42),
            hash: BlockHash::from_hash(blake3::Hash::from_bytes([0xCCu8; 32])),
            genesis_hash: None,
        };
        let json = serde_json::to_string(&tip_no_genesis).expect("Tip serialize");
        // Must deserialize correctly with null genesis_hash
        let tip3: Tip = serde_json::from_str(&json).expect("Tip deserialize null genesis_hash");
        assert_eq!(tip3.genesis_hash, None);

        // Backward compat: JSON with missing genesis_hash field
        let old_json = r#"{"height":42,"hash":"0101010101010101010101010101010101010101010101010101010101010101"}"#;
        let tip4: Tip = serde_json::from_str(old_json).expect("Tip deserialize old format");
        assert_eq!(tip4.height, BlockHeight::new(42));
        assert_eq!(tip4.genesis_hash, None, "Missing genesis_hash must deserialize as None");
    }
}
