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

//! Sync serialization for linear blockchain types — deterministic sled storage.
//!
//! This module is unconditionally compiled (unlike `serial.rs` which is gated
//! behind `#[cfg(feature = "async")]`). These impls are required by node AND
//! wallet code paths for block storage, dedup hashing, and chain sync.

use dwow_serial::{Decodable, Encodable};
use std::io::Result;

use super::{Block, BlockHeader, ContractCall, PowSource, Transaction, TxInput, TxOutput, UncleBlock};
use crate::fee_window::FeeWindowFlags;
use crate::monero::MoneroPowData;

impl Encodable for TxInput {
    fn encode<W: std::io::Write>(&self, s: &mut W) -> Result<usize> {
        let mut len = 0;
        len += self.previous_output.encode(s)?;
        len += self.script.encode(s)?;
        len += self.sequence.encode(s)?;
        Ok(len)
    }
}

impl Decodable for TxInput {
    fn decode<D: std::io::Read>(d: &mut D) -> Result<Self> {
        let previous_output = Decodable::decode(d)?;
        let script = Decodable::decode(d)?;
        let sequence = Decodable::decode(d)?;
        Ok(Self { previous_output, script, sequence })
    }
}

impl Encodable for TxOutput {
    fn encode<W: std::io::Write>(&self, s: &mut W) -> Result<usize> {
        let mut len = 0;
        len += self.value.encode(s)?;
        len += self.script.encode(s)?;
        Ok(len)
    }
}

impl Decodable for TxOutput {
    fn decode<D: std::io::Read>(d: &mut D) -> Result<Self> {
        let value = Decodable::decode(d)?;
        let script = Decodable::decode(d)?;
        Ok(Self { value, script })
    }
}

impl Encodable for ContractCall {
    fn encode<W: std::io::Write>(&self, s: &mut W) -> Result<usize> {
        let mut len = 0;
        len += self.contract_id.encode(s)?;
        len += self.data.encode(s)?;
        Ok(len)
    }
}

impl Decodable for ContractCall {
    fn decode<D: std::io::Read>(d: &mut D) -> Result<Self> {
        let contract_id = Decodable::decode(d)?;
        let data = Decodable::decode(d)?;
        Ok(Self { contract_id, data })
    }
}

impl Encodable for Transaction {
    fn encode<W: std::io::Write>(&self, s: &mut W) -> Result<usize> {
        let mut len = 0;
        len += self.version.encode(s)?;
        len += self.inputs.encode(s)?;
        len += self.outputs.encode(s)?;
        len += self.contract_calls.encode(s)?;
        len += self.lock_time.encode(s)?;
        len += self.nullifiers.encode(s)?;
        len += self.witness.encode(s)?;
        Ok(len)
    }
}

impl Decodable for Transaction {
    fn decode<D: std::io::Read>(d: &mut D) -> Result<Self> {
        let version = Decodable::decode(d)?;
        let inputs = Decodable::decode(d)?;
        let outputs = Decodable::decode(d)?;
        let contract_calls = Decodable::decode(d)?;
        let lock_time = Decodable::decode(d)?;
        let nullifiers = Decodable::decode(d)?;
        let witness = Decodable::decode(d)?;
        Ok(Self { version, inputs, outputs, contract_calls, lock_time, nullifiers, witness })
    }
}

impl Encodable for BlockHeader {
    fn encode<W: std::io::Write>(&self, s: &mut W) -> Result<usize> {
        let mut len = 0;
        len += self.version.encode(s)?;
        len += self.previous.encode(s)?;
        len += self.merkle_root.encode(s)?;
        len += self.timestamp.encode(s)?;
        len += self.target.encode(s)?;
        len += self.nonce.encode(s)?;
        len += self.height.encode(s)?;
        len += self.uncle_merkle_root.encode(s)?;
        len += self.total_reward.encode(s)?;
        len += self.randomx_key.encode(s)?;
        len += self.miner.encode(s)?;
        len += self.commitment_merkle_root.encode(s)?;
        len += self.nullifier_root.encode(s)?;
        len += self.anchor_tx_id.encode(s)?;
        len += self.anchor_monero_height.encode(s)?;
        len += self.anchor_monero_hash.encode(s)?;
        len += self.finality_flags.encode(s)?;
        len += self.pow_source.encode(s)?;
        Ok(len)
    }
}

/// The canonical wire encoding of `PowSource`: a discriminator, then the Monero proof if present.
///
/// Extracted from `BlockHeader`'s impl so that `PowSource` has a codec of its own — which serde then
/// uses as its transport (see the `Serialize`/`Deserialize` impls below). Splitting it out is what
/// closes `OBL-C69`: `BlockHeader`'s serde derive had `pow_source` declared `#[serde(skip)]`, so the
/// canonical codec preserved the merge-mining proof and the serde one silently dropped it, and the
/// P2P block wire is serde. Two serializations of one consensus field existed and only one worked.
impl Encodable for PowSource {
    fn encode<S: std::io::Write>(&self, s: &mut S) -> Result<usize> {
        match self {
            PowSource::Native => 0u8.encode(s),
            PowSource::Monero(data) => {
                let mut len = 1u8.encode(s)?;
                len += data.encode(s)?;
                Ok(len)
            }
        }
    }
}

impl Decodable for PowSource {
    fn decode<D: std::io::Read>(d: &mut D) -> Result<Self> {
        let disc: u8 = Decodable::decode(d)?;
        match disc {
            0 => Ok(PowSource::Native),
            1 => Ok(PowSource::Monero(MoneroPowData::decode(d)?)),
            // Fail closed. This arm used to return `PowSource::Native`, which silently reclassified a
            // block whose discriminator was corrupt or unknown as native — the same downgrade
            // `OBL-C69` records, on the consensus path rather than the wire. A merge-mined block
            // decoded this way then claims native PoW, which it cannot satisfy.
            other => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("unknown PowSource discriminator {other}"),
            )),
        }
    }
}

/// `PowSource` in serde, as a transport for the canonical codec above.
///
/// A hex string rather than a byte sequence, because the serde consumer here is JSON (the block wire
/// and the block RPC) and serde_json renders `serialize_bytes` as an array of integers — several
/// times larger, and unreadable in a log. The point of these impls is that they *cannot* disagree with
/// the codec: they carry its bytes verbatim rather than restating the layout.
impl serde::Serialize for PowSource {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        let bytes = dwow_serial::serialize(self);
        s.serialize_str(&hex::encode(bytes))
    }
}

impl<'de> serde::Deserialize<'de> for PowSource {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        struct HexVisitor;

        impl<'de> serde::de::Visitor<'de> for HexVisitor {
            type Value = PowSource;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a hex-encoded PowSource")
            }

            fn visit_str<E: serde::de::Error>(self, v: &str) -> std::result::Result<PowSource, E> {
                let bytes = hex::decode(v).map_err(E::custom)?;
                dwow_serial::deserialize(&bytes).map_err(E::custom)
            }
        }

        d.deserialize_str(HexVisitor)
    }
}

impl Decodable for BlockHeader {
    fn decode<D: std::io::Read>(d: &mut D) -> Result<Self> {
        let version = Decodable::decode(d)?;
        let previous = Decodable::decode(d)?;
        let merkle_root = Decodable::decode(d)?;
        let timestamp = Decodable::decode(d)?;
        let target = Decodable::decode(d)?;
        let nonce = Decodable::decode(d)?;
        let height = Decodable::decode(d)?;
        let uncle_merkle_root = Decodable::decode(d)?;
        let total_reward = Decodable::decode(d)?;
        let randomx_key = Decodable::decode(d)?;
        let miner = Decodable::decode(d)?;
        let commitment_merkle_root = Decodable::decode(d)?;
        let nullifier_root = Decodable::decode(d)?;
        let anchor_tx_id = Decodable::decode(d)?;
        let anchor_monero_height = Decodable::decode(d)?;
        let anchor_monero_hash = Decodable::decode(d)?;
        let finality_flags = Decodable::decode(d)?;
        let pow_source = PowSource::decode(d)?;
        Ok(Self {
            version, previous, merkle_root, timestamp, target, nonce, height,
            uncle_merkle_root, total_reward, randomx_key, miner, commitment_merkle_root,
            nullifier_root, anchor_tx_id, anchor_monero_height, anchor_monero_hash,
            finality_flags,
            fee_window_flags: FeeWindowFlags::default(),
            pow_source,
        })
    }
}

impl Encodable for Block {
    fn encode<W: std::io::Write>(&self, s: &mut W) -> Result<usize> {
        let mut len = 0;
        len += self.header.encode(s)?;
        len += self.transactions.encode(s)?;
        Ok(len)
    }
}

impl Decodable for Block {
    fn decode<D: std::io::Read>(d: &mut D) -> Result<Self> {
        let header = Decodable::decode(d)?;
        let transactions = Decodable::decode(d)?;
        Ok(Self { header, transactions })
    }
}

// P2-9-3: UncleBlock's binary codec (sled storage format) no longer encodes
// `depth`/`pin_offered` — the fields were removed from the struct. Existing
// sled `uncles` trees hold the old 6-field shape and will fail to decode:
// devnet wipe required (mainnet TBD).
impl Encodable for UncleBlock {
    fn encode<W: std::io::Write>(&self, s: &mut W) -> Result<usize> {
        let mut len = 0;
        len += self.header.encode(s)?;
        len += self.transactions.encode(s)?;
        len += self.pin_accepted.encode(s)?;
        len += self.pin_confirmed.encode(s)?;
        Ok(len)
    }
}

impl Decodable for UncleBlock {
    fn decode<D: std::io::Read>(d: &mut D) -> Result<Self> {
        let header = Decodable::decode(d)?;
        let transactions = Decodable::decode(d)?;
        let pin_accepted = Decodable::decode(d)?;
        let pin_confirmed = Decodable::decode(d)?;
        Ok(Self { header, transactions, pin_accepted, pin_confirmed })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dwow_sdk::blockchain::{
        BlockHeight, BlockReward, BlockTarget, BlockTimestamp, BlockVersion,
        MoneroBlockHeight,
    };

    /// P2-9-3: pin the sled binary shape of UncleBlock (4 fields — no depth,
    /// no pin_offered) through an encode/decode roundtrip.
    #[test]
    fn uncle_block_binary_roundtrip_pins_sled_shape() {
        let header = BlockHeader {
            version: BlockVersion::CURRENT,
            previous: blake3::hash(b"parent"),
            merkle_root: blake3::hash(b"txs"),
            timestamp: BlockTimestamp::new(1),
            target: BlockTarget::new(0xFFFF_FFFF),
            nonce: 7,
            height: BlockHeight::new(5),
            uncle_merkle_root: [0u8; 32],
            total_reward: BlockReward::ZERO,
            randomx_key: [9u8; 32],
            miner: [1u8; 32],
            commitment_merkle_root: [0u8; 32],
            nullifier_root: [0u8; 32],
            anchor_tx_id: [0u8; 32],
            anchor_monero_height: MoneroBlockHeight::new(0),
            anchor_monero_hash: [0u8; 32],
            finality_flags: 0,
            fee_window_flags: FeeWindowFlags::default(),
            pow_source: PowSource::Native,
        };
        let uncle = UncleBlock {
            header,
            transactions: vec![],
            pin_accepted: true,
            pin_confirmed: BlockReward::new(50_000_000),
        };
        let encoded = dwow_serial::serialize(&uncle);
        let decoded: UncleBlock = dwow_serial::deserialize(&encoded).expect("decode");
        assert_eq!(decoded.header.height, BlockHeight::new(5));
        assert_eq!(decoded.header.nonce, 7);
        assert!(decoded.pin_accepted);
        assert_eq!(decoded.pin_confirmed, BlockReward::new(50_000_000));
        assert!(decoded.transactions.is_empty());
    }
}
