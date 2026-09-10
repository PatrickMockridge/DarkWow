/* This file is part of DarkWow
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * DarkWow is a tool for people and nations to establish sovereignty
 * according to human rights law. See the UN Declaration on the Rights
 * of Indigenous Peoples and associated documents:
 * https://documents.un.org/doc/undoc/gen/g26/031/70/pdf/g2603170.pdf
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

//! Wire-format regression test for the uncle-pin trim (P2-9-3).
//!
//! `BlockBroadcast` encodes as JSON (`Encodable` in proto/linear_broadcast.rs
//! is `serde_json` — this IS the "linearlblock" P2P wire format). After
//! dropping `UncleBlock.depth` / `pin_offered`, the JSON must carry neither
//! key, must still carry `pin_confirmed`, and must roundtrip through the
//! codec. Spec: uncle_merkle.md §"Uncle Generation".

use dwow_chain::{Block, BlockHeader, PowSource, compute_merkle_root, create_uncle};
use dwow_sdk::blockchain::{
    BlockHeight, BlockTimestamp, BlockVersion, MoneroBlockHeight, expected_reward,
};

use crate::proto::linear_broadcast::BlockBroadcast;

/// Serialize a BlockBroadcast with one create_uncle uncle; assert the JSON
/// contains no `depth`/`pin_offered`, contains `pin_confirmed`, and roundtrips.
/// UNVERIFIED(P2-9-3): needs cargo test -p dwowd --lib -- wire_format
#[test]
fn block_broadcast_wire_golden() {
    // Minimal block (chain_state.rs test pattern) as the uncle at height 2.
    let h_uncle = BlockHeight::new(2);
    let uncle_block = Block {
        header: BlockHeader {
            version: BlockVersion::CURRENT,
            previous: blake3::hash(b"genesis"),
            merkle_root: compute_merkle_root(&[]),
            timestamp: BlockTimestamp::new(1),
            target: dwow_sdk::blockchain::BlockTarget::MAX,
            nonce: 0,
            height: h_uncle,
            uncle_merkle_root: [0u8; 32],
            total_reward: expected_reward(h_uncle),
            randomx_key: [0u8; 32],
            miner: [0u8; 32],
            commitment_merkle_root: [0u8; 32],
            nullifier_root: [0u8; 32],
            anchor_tx_id: [0u8; 32],
            anchor_monero_height: MoneroBlockHeight::new(0),
            anchor_monero_hash: [0u8; 32],
            finality_flags: 0,
            pow_source: PowSource::Native,
            fee_window_flags: dwow_chain::fee_window::FeeWindowFlags::default(),
        },
        transactions: vec![],
    };

    // Canonical block at height 3 referencing the uncle at depth 1.
    let h_canonical = BlockHeight::new(3);
    let mut canonical = uncle_block.clone();
    canonical.header.height = h_canonical;
    canonical.header.nonce = 1;

    let uncle = create_uncle(uncle_block, 1, expected_reward(h_canonical));
    let msg = BlockBroadcast { block: canonical, uncles: vec![uncle] };

    // BlockBroadcast's Encodable IS serde_json — this is the P2P wire format.
    let bytes = dwow_serial::serialize(&msg);
    let json = String::from_utf8(bytes.clone()).expect("JSON wire format");

    assert!(json.contains("\"pin_confirmed\""), "wire must carry pin_confirmed: {json}");
    assert!(!json.contains("\"depth\""), "trimmed depth still on wire: {json}");
    assert!(!json.contains("\"pin_offered\""), "trimmed pin_offered still on wire: {json}");

    // Roundtrip through the real codec (no PartialEq on BlockBroadcast — compare fields).
    let decoded: BlockBroadcast = dwow_serial::deserialize(&bytes).expect("wire roundtrip");
    assert_eq!(decoded.block.header.height, h_canonical);
    assert_eq!(decoded.block.header.nonce, 1);
    assert_eq!(decoded.uncles.len(), 1, "exactly one uncle roundtrips");
    assert_eq!(decoded.uncles[0].header.height, h_uncle);
    assert!(!decoded.uncles[0].pin_accepted, "pin_accepted must survive as false");
    assert_eq!(decoded.uncles[0].pin_confirmed, msg.uncles[0].pin_confirmed,
        "pin_confirmed value must roundtrip unchanged");
}
