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
        len += self.coin_merkle_root.encode(s)?;
        len += self.nullifier_root.encode(s)?;
        len += self.anchor_tx_id.encode(s)?;
        len += self.anchor_monero_height.encode(s)?;
        len += self.anchor_monero_hash.encode(s)?;
        len += self.finality_flags.encode(s)?;
        match &self.pow_source {
            PowSource::Native => {
                len += 0u8.encode(s)?;
            }
            PowSource::Monero(data) => {
                len += 1u8.encode(s)?;
                len += data.encode(s)?;
            }
        }
        Ok(len)
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
        let coin_merkle_root = Decodable::decode(d)?;
        let nullifier_root = Decodable::decode(d)?;
        let anchor_tx_id = Decodable::decode(d)?;
        let anchor_monero_height = Decodable::decode(d)?;
        let anchor_monero_hash = Decodable::decode(d)?;
        let finality_flags = Decodable::decode(d)?;
        let disc: u8 = Decodable::decode(d)?;
        let pow_source = match disc {
            0 => PowSource::Native,
            1 => {
                let data = crate::monero::MoneroPowData::decode(d)?;
                PowSource::Monero(data)
            }
            _ => PowSource::Native,
        };
        Ok(Self {
            version, previous, merkle_root, timestamp, target, nonce, height,
            uncle_merkle_root, total_reward, randomx_key, coin_merkle_root,
            nullifier_root, anchor_tx_id, anchor_monero_height, anchor_monero_hash,
            finality_flags, pow_source,
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

impl Encodable for UncleBlock {
    fn encode<W: std::io::Write>(&self, s: &mut W) -> Result<usize> {
        let mut len = 0;
        len += self.header.encode(s)?;
        len += self.transactions.encode(s)?;
        len += self.depth.encode(s)?;
        len += self.pin_offered.encode(s)?;
        len += self.pin_accepted.encode(s)?;
        len += self.pin_confirmed.encode(s)?;
        Ok(len)
    }
}

impl Decodable for UncleBlock {
    fn decode<D: std::io::Read>(d: &mut D) -> Result<Self> {
        let header = Decodable::decode(d)?;
        let transactions = Decodable::decode(d)?;
        let depth = Decodable::decode(d)?;
        let pin_offered = Decodable::decode(d)?;
        let pin_accepted = Decodable::decode(d)?;
        let pin_confirmed = Decodable::decode(d)?;
        Ok(Self { header, transactions, depth, pin_offered, pin_accepted, pin_confirmed })
    }
}
