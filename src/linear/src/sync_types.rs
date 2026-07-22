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
//! GetBlocks=256, GetBlock=256, BlockResponse=0, Blocks=0 (unlimited).

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

/// Request a single block by height.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GetBlock {
    pub height: BlockHeight,
}

/// Response containing a single block.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BlockResponse {
    pub block: Option<Block>,
}

/// Request to get the current chain tip.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GetTip;

/// Response containing chain tip info.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Tip {
    pub height: BlockHeight,
    pub hash: String,
    /// Genesis block hash — allows peers to detect incompatible chains
    /// before downloading blocks (defense-in-depth, HAZID F1/F7).
    /// `Option` + `#[serde(default)]` is forward/backward compatible:
    /// old nodes ignore this field, new nodes treat None as unverified.
    #[serde(default)]
    pub genesis_hash: Option<String>,
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
    impl_sync_codec!(GetBlock);
    impl_sync_codec!(BlockResponse);
    impl_sync_codec!(GetTip);
    impl_sync_codec!(Tip);

    // ── P2P message registration ─────────────────────────────────────
    //
    // MAX_BYTES per sync-protocol.md §4:
    //   GetTip: 256, Tip: 512, GetBlocks: 256, GetBlock: 256,
    //   Blocks: 0 (unlimited, consensus-level validation)
    //   BlockResponse: 0 (unlimited)

    const MAX_SMALL: u64 = 256;
    const MAX_TIP: u64 = 512;
    const MAX_UNLIMITED: u64 = 0;

    const SYNC_METERING: MeteringConfiguration = MeteringConfiguration {
        threshold: 20,
        sleep_step: 500,
        expiry_time: NanoTimestamp::from_secs(5),
    };

    macro_rules! sync_barbs {
        () => { &[
            dwow_core::net::barb_trait::BarbId::Verify,
            dwow_core::net::barb_trait::BarbId::SyncBarrier,
            dwow_core::net::barb_trait::BarbId::GossipForward,
        ] };
    }

    impl_p2p_message!(GetBlocks, "lineargetblocks", MAX_SMALL, 1, SYNC_METERING, sync_barbs!());
    impl_p2p_message!(Blocks, "linearblocks", MAX_UNLIMITED, 1, SYNC_METERING, sync_barbs!());
    impl_p2p_message!(GetBlock, "lineargetblock", MAX_SMALL, 1, SYNC_METERING, sync_barbs!());
    impl_p2p_message!(BlockResponse, "linearblockresponse", MAX_UNLIMITED, 1, SYNC_METERING, sync_barbs!());
    impl_p2p_message!(GetTip, "lineargettip", MAX_SMALL, 1, SYNC_METERING, sync_barbs!());
    impl_p2p_message!(Tip, "lineartip", MAX_TIP, 1, SYNC_METERING, sync_barbs!());
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
            hash: "abcdef0123456789".to_string(),
            genesis_hash: Some("0000000000000000".to_string()),
        };
        let json = serde_json::to_string(&tip).expect("Tip serialize");
        // Round-trip: deserialize must produce identical struct
        let tip2: Tip = serde_json::from_str(&json).expect("Tip deserialize");
        assert_eq!(tip2.height, BlockHeight::new(42));
        assert_eq!(tip2.hash, "abcdef0123456789");
        assert_eq!(tip2.genesis_hash, Some("0000000000000000".to_string()));

        // GetBlocks with BlockHeight
        let gb = GetBlocks { start_height: BlockHeight::new(1), count: 20 };
        let json = serde_json::to_string(&gb).expect("GetBlocks serialize");
        let gb2: GetBlocks = serde_json::from_str(&json).expect("GetBlocks deserialize");
        assert_eq!(gb2.start_height, BlockHeight::new(1));
        assert_eq!(gb2.count, 20);

        // Tip without genesis_hash (backward compat)
        let tip_no_genesis = Tip {
            height: BlockHeight::new(42),
            hash: "abcdef".to_string(),
            genesis_hash: None,
        };
        let json = serde_json::to_string(&tip_no_genesis).expect("Tip serialize");
        // Must deserialize correctly with null genesis_hash
        let tip3: Tip = serde_json::from_str(&json).expect("Tip deserialize null genesis_hash");
        assert_eq!(tip3.genesis_hash, None);

        // Backward compat: JSON with missing genesis_hash field
        let old_json = r#"{"height":42,"hash":"abcdef"}"#;
        let tip4: Tip = serde_json::from_str(old_json).expect("Tip deserialize old format");
        assert_eq!(tip4.height, BlockHeight::new(42));
        assert_eq!(tip4.genesis_hash, None, "Missing genesis_hash must deserialize as None");
    }
}
