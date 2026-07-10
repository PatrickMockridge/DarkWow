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

//! Async serialization implementations for linear blockchain types

use dwow_serial::{async_trait, AsyncDecodable, AsyncEncodable, AsyncRead, AsyncWrite};
use std::io::Result;

use super::{Block, BlockHeader, ContractCall, Input, Output, PowSource, Transaction, UncleBlock, UncleProof};

#[async_trait]
impl AsyncEncodable for Input {
    async fn encode_async<S: AsyncWrite + Unpin + Send>(&self, s: &mut S) -> Result<usize> {
        let mut len = 0;
        len += self.previous_output.encode_async(s).await?;
        len += self.script.encode_async(s).await?;
        len += self.sequence.encode_async(s).await?;
        Ok(len)
    }
}

#[async_trait]
impl AsyncDecodable for Input {
    async fn decode_async<D: AsyncRead + Unpin + Send>(d: &mut D) -> Result<Self> {
        let previous_output = AsyncDecodable::decode_async(d).await?;
        let script = AsyncDecodable::decode_async(d).await?;
        let sequence = AsyncDecodable::decode_async(d).await?;
        Ok(Self { previous_output, script, sequence })
    }
}

#[async_trait]
impl AsyncEncodable for Output {
    async fn encode_async<S: AsyncWrite + Unpin + Send>(&self, s: &mut S) -> Result<usize> {
        let mut len = 0;
        len += self.value.encode_async(s).await?;
        len += self.script.encode_async(s).await?;
        Ok(len)
    }
}

#[async_trait]
impl AsyncDecodable for Output {
    async fn decode_async<D: AsyncRead + Unpin + Send>(d: &mut D) -> Result<Self> {
        let value = AsyncDecodable::decode_async(d).await?;
        let script = AsyncDecodable::decode_async(d).await?;
        Ok(Self { value, script })
    }
}

#[async_trait]
impl AsyncEncodable for ContractCall {
    async fn encode_async<S: AsyncWrite + Unpin + Send>(&self, s: &mut S) -> Result<usize> {
        let mut len = 0;
        len += self.contract_id.encode_async(s).await?;
        len += self.data.encode_async(s).await?;
        Ok(len)
    }
}

#[async_trait]
impl AsyncDecodable for ContractCall {
    async fn decode_async<D: AsyncRead + Unpin + Send>(d: &mut D) -> Result<Self> {
        let contract_id = AsyncDecodable::decode_async(d).await?;
        let data = AsyncDecodable::decode_async(d).await?;
        Ok(Self { contract_id, data })
    }
}

#[async_trait]
impl AsyncEncodable for Transaction {
    async fn encode_async<S: AsyncWrite + Unpin + Send>(&self, s: &mut S) -> Result<usize> {
        let mut len = 0;
        len += self.version.encode_async(s).await?;
        len += self.inputs.encode_async(s).await?;
        len += self.outputs.encode_async(s).await?;
        len += self.contract_calls.encode_async(s).await?;
        len += self.lock_time.encode_async(s).await?;
        Ok(len)
    }
}

#[async_trait]
impl AsyncDecodable for Transaction {
    async fn decode_async<D: AsyncRead + Unpin + Send>(d: &mut D) -> Result<Self> {
        let version = AsyncDecodable::decode_async(d).await?;
        let inputs = AsyncDecodable::decode_async(d).await?;
        let outputs = AsyncDecodable::decode_async(d).await?;
        let contract_calls = AsyncDecodable::decode_async(d).await?;
        let lock_time = AsyncDecodable::decode_async(d).await?;
        Ok(Self { version, inputs, outputs, contract_calls, lock_time, nullifiers: vec![] })
    }
}

#[async_trait]
impl AsyncEncodable for BlockHeader {
    async fn encode_async<S: AsyncWrite + Unpin + Send>(&self, s: &mut S) -> Result<usize> {
        let mut len = 0;
        len += self.version.encode_async(s).await?;
        len += self.previous.encode_async(s).await?;
        len += self.merkle_root.encode_async(s).await?;
        len += self.timestamp.encode_async(s).await?;
        len += self.target.encode_async(s).await?;
        len += self.nonce.encode_async(s).await?;
        len += self.height.encode_async(s).await?;
        len += self.uncle_merkle_root.encode_async(s).await?;
        len += self.total_reward.encode_async(s).await?;
        len += self.randomx_key.encode_async(s).await?;
        len += self.coin_merkle_root.encode_async(s).await?;
        len += self.nullifier_root.encode_async(s).await?;
        len += self.anchor_tx_id.encode_async(s).await?;
        len += self.anchor_monero_height.encode_async(s).await?;
        len += self.anchor_monero_hash.encode_async(s).await?;
        len += self.finality_flags.encode_async(s).await?;
        match &self.pow_source {
            PowSource::Native => {
                len += 0u8.encode_async(s).await?;
            }
            PowSource::Monero(data) => {
                len += 1u8.encode_async(s).await?;
                use dwow_serial::AsyncEncodable;
                len += data.encode_async(s).await?;
            }
        }
        Ok(len)
    }
}

#[async_trait]
impl AsyncDecodable for BlockHeader {
    async fn decode_async<D: AsyncRead + Unpin + Send>(d: &mut D) -> Result<Self> {
        let version = AsyncDecodable::decode_async(d).await?;
        let previous = AsyncDecodable::decode_async(d).await?;
        let merkle_root = AsyncDecodable::decode_async(d).await?;
        let timestamp = AsyncDecodable::decode_async(d).await?;
        let target = AsyncDecodable::decode_async(d).await?;
        let nonce = AsyncDecodable::decode_async(d).await?;
        let height = AsyncDecodable::decode_async(d).await?;
        let uncle_merkle_root = AsyncDecodable::decode_async(d).await?;
        let total_reward = AsyncDecodable::decode_async(d).await?;
        let randomx_key = AsyncDecodable::decode_async(d).await?;
        let coin_merkle_root = AsyncDecodable::decode_async(d).await?;
        let nullifier_root = AsyncDecodable::decode_async(d).await?;
        let anchor_tx_id = AsyncDecodable::decode_async(d).await?;
        let anchor_monero_height = AsyncDecodable::decode_async(d).await?;
        let anchor_monero_hash = AsyncDecodable::decode_async(d).await?;
        let finality_flags = AsyncDecodable::decode_async(d).await?;
        let disc: u8 = AsyncDecodable::decode_async(d).await?;
        let pow_source = match disc {
            0 => PowSource::Native,
            1 => {
                use dwow_serial::AsyncDecodable;
                let data = crate::monero::MoneroPowData::decode_async(d).await?;
                PowSource::Monero(data)
            }
            _ => PowSource::Native,
        };
        Ok(Self {
            version,
            previous,
            merkle_root,
            timestamp,
            target,
            nonce,
            height,
            uncle_merkle_root,
            total_reward,
            randomx_key,
            coin_merkle_root,
            nullifier_root,
            anchor_tx_id,
            anchor_monero_height,
            anchor_monero_hash,
            finality_flags,
            pow_source,
        })
    }
}

#[async_trait]
impl AsyncEncodable for Block {
    async fn encode_async<S: AsyncWrite + Unpin + Send>(&self, s: &mut S) -> Result<usize> {
        let mut len = 0;
        len += self.header.encode_async(s).await?;
        len += self.transactions.encode_async(s).await?;
        Ok(len)
    }
}

#[async_trait]
impl AsyncDecodable for Block {
    async fn decode_async<D: AsyncRead + Unpin + Send>(d: &mut D) -> Result<Self> {
        let header = AsyncDecodable::decode_async(d).await?;
        let transactions = AsyncDecodable::decode_async(d).await?;
        Ok(Self { header, transactions })
    }
}

#[async_trait]
impl AsyncEncodable for UncleBlock {
    async fn encode_async<S: AsyncWrite + Unpin + Send>(&self, s: &mut S) -> Result<usize> {
        let mut len = 0;
        len += self.header.encode_async(s).await?;
        len += self.transactions.encode_async(s).await?;
        len += self.depth.encode_async(s).await?;
        len += self.pin_offered.encode_async(s).await?;
        len += self.pin_accepted.encode_async(s).await?;
        len += self.pin_reward.encode_async(s).await?;
        Ok(len)
    }
}

#[async_trait]
impl AsyncDecodable for UncleBlock {
    async fn decode_async<D: AsyncRead + Unpin + Send>(d: &mut D) -> Result<Self> {
        let header = AsyncDecodable::decode_async(d).await?;
        let transactions = AsyncDecodable::decode_async(d).await?;
        let depth = AsyncDecodable::decode_async(d).await?;
        let pin_offered = AsyncDecodable::decode_async(d).await?;
        let pin_accepted = AsyncDecodable::decode_async(d).await?;
        let pin_reward = AsyncDecodable::decode_async(d).await?;
        Ok(Self { header, transactions, depth, pin_offered, pin_accepted, pin_reward })
    }
}

#[async_trait]
impl AsyncEncodable for UncleProof {
    async fn encode_async<S: AsyncWrite + Unpin + Send>(&self, s: &mut S) -> Result<usize> {
        let mut len = 0;
        len += self.header.encode_async(s).await?;
        len += self.pow_hash.encode_async(s).await?;
        len += self.merkle_path.encode_async(s).await?;
        len += self.position.encode_async(s).await?;
        len += self.depth.encode_async(s).await?;
        Ok(len)
    }
}

#[async_trait]
impl AsyncDecodable for UncleProof {
    async fn decode_async<D: AsyncRead + Unpin + Send>(d: &mut D) -> Result<Self> {
        let header = AsyncDecodable::decode_async(d).await?;
        let pow_hash = AsyncDecodable::decode_async(d).await?;
        let merkle_path = AsyncDecodable::decode_async(d).await?;
        let position = AsyncDecodable::decode_async(d).await?;
        let depth = AsyncDecodable::decode_async(d).await?;
        Ok(Self { header, pow_hash, merkle_path, position, depth })
    }
}