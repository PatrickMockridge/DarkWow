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

//! Light Client Trait
//!
//! Defines the interface for light client implementations that verify
//! external chain state without running a full node.
//!
//! ## Why Light Clients?
//!
//! Using oracles for bridge verification just moves trust to the oracle.
//! Light clients provide trustless verification by verifying proofs
//! against block headers.
//!
//! ## Implementations
//!
//! - `EthLightClient`: Ethereum SPV via eth_getProof
//! - `BtcLightClient`: Bitcoin SPV via BIP-157
//! - `XmrLightClient`: Monero view-key scanning
//! - `ZecLightClient`: Zcash lightwalletd

use async_trait::async_trait;
use dwow_sdk::error::ContractError;

/// Confirmation level for deposits
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmationLevel {
    Mempool,
    Confirmed(u64),
    Finalized,
}

impl ConfirmationLevel {
    pub fn encode(&self) -> Vec<u8> {
        match self {
            Self::Mempool => vec![0],
            Self::Confirmed(n) => { let mut b = vec![1]; b.extend_from_slice(&n.to_le_bytes()); b }
            Self::Finalized => vec![2],
        }
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.is_empty() { return Err(ContractError::IoError("ConfirmationLevel: empty".into())); }
        match data[0] {
            0 => Ok(Self::Mempool),
            1 => { if data.len() != 9 { return Err(ContractError::IoError("ConfirmationLevel: Confirmed needs 9 bytes".into())); } Ok(Self::Confirmed(u64::from_le_bytes(data[1..9].try_into().unwrap()))) }
            2 => Ok(Self::Finalized),
            _ => Err(ContractError::IoError(format!("ConfirmationLevel: unknown variant {}", data[0]))),
        }
    }
}

/// Merkle proof for transaction inclusion
#[derive(Debug, Clone)]
pub struct MerkleProof {
    pub position: u32,
    pub path: Vec<[u8; 32]>,
}

impl MerkleProof {
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(5 + self.path.len() * 32);
        b.extend_from_slice(&self.position.to_le_bytes());
        b.push(self.path.len() as u8);
        for h in &self.path { b.extend_from_slice(h); }
        b
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 5 { return Err(ContractError::IoError("MerkleProof: too short".into())); }
        let position = u32::from_le_bytes(data[0..4].try_into().unwrap());
        let count = data[4] as usize;
        if data.len() != 5 + count * 32 { return Err(ContractError::IoError(format!("MerkleProof: expected {} bytes, got {}", 5+count*32, data.len()))); }
        let mut path = Vec::with_capacity(count);
        for i in 0..count { path.push(data[5+i*32..5+(i+1)*32].try_into().unwrap()); }
        Ok(MerkleProof { position, path })
    }
}

/// Finality proof for chains with consensus finality
#[derive(Debug, Clone)]
pub struct FinalityProof {
    pub block_hash: [u8; 32],
    pub block_height: u64,
    pub checkpoint_hash: [u8; 32],
    pub proof: Vec<[u8; 32]>,
}

impl FinalityProof {
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(73 + self.proof.len() * 32);
        b.extend_from_slice(&self.block_hash);
        b.extend_from_slice(&self.block_height.to_le_bytes());
        b.extend_from_slice(&self.checkpoint_hash);
        b.push(self.proof.len() as u8);
        for h in &self.proof { b.extend_from_slice(h); }
        b
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 73 { return Err(ContractError::IoError("FinalityProof: too short".into())); }
        let block_hash: [u8;32] = data[0..32].try_into().unwrap();
        let block_height = u64::from_le_bytes(data[32..40].try_into().unwrap());
        let checkpoint_hash: [u8;32] = data[40..72].try_into().unwrap();
        let count = data[72] as usize;
        if data.len() != 73 + count * 32 { return Err(ContractError::IoError(format!("FinalityProof: expected {} bytes, got {}", 73+count*32, data.len()))); }
        let mut proof = Vec::with_capacity(count);
        for i in 0..count { proof.push(data[73+i*32..73+(i+1)*32].try_into().unwrap()); }
        Ok(FinalityProof { block_hash, block_height, checkpoint_hash, proof })
    }
}

/// Block header data
#[derive(Debug, Clone)]
pub struct BlockHeader {
    pub hash: [u8; 32],
    pub height: u64,
    pub timestamp: u64,
    pub merkle_root: [u8; 32],
}

impl BlockHeader {
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(80);
        b.extend_from_slice(&self.hash);
        b.extend_from_slice(&self.height.to_le_bytes());
        b.extend_from_slice(&self.timestamp.to_le_bytes());
        b.extend_from_slice(&self.merkle_root);
        b
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 80 { return Err(ContractError::IoError(format!("BlockHeader: expected 80 bytes, got {}", data.len()))); }
        Ok(BlockHeader {
            hash: data[0..32].try_into().unwrap(),
            height: u64::from_le_bytes(data[32..40].try_into().unwrap()),
            timestamp: u64::from_le_bytes(data[40..48].try_into().unwrap()),
            merkle_root: data[48..80].try_into().unwrap(),
        })
    }
}

/// Light client errors
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("RPC error: {0}")]
    Rpc(String),

    #[error("Invalid proof: {0}")]
    InvalidProof(String),

    #[error("Chain reorganization detected")]
    Reorg,

    #[error("Block not found: {0}")]
    BlockNotFound(String),

    #[error("Verification failed: {0}")]
    VerificationFailed(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Light client trait
///
/// Implemented by each external chain to provide
/// trustless state verification via SPV.
#[async_trait]
pub trait LightClient: Send + Sync {
    /// Get the chain identifier
    fn chain_id(&self) -> u8;

    /// Verify a block header is valid and at the given height
    ///
    /// This verifies:
    /// - The header hash matches the claimed height
    /// - The header is part of the canonical chain
    async fn verify_header(&self, block_hash: &[u8; 32], height: u64) -> Result<bool, Error>;

    /// Verify a Merkle proof for a transaction
    ///
    /// Proves that a transaction is included in a block
    /// at the given position with the given authentication path.
    async fn verify_merkle_proof(
        &self,
        tx_hash: &[u8; 32],
        block_hash: &[u8; 32],
        proof: &MerkleProof,
    ) -> Result<bool, Error>;

    /// Get the current chain tip
    ///
    /// Returns (height, block_hash) of the latest known block.
    async fn get_tip(&self) -> Result<(u64, [u8; 32]), Error>;

    /// Get confirmation level for a block
    ///
    /// Returns how many blocks deep the given block is,
    /// or Finalized if the chain has consensus finality.
    async fn get_confirmation_level(
        &self,
        block_hash: &[u8; 32],
    ) -> Result<ConfirmationLevel, Error>;

    /// Verify a finality proof
    ///
    /// For chains with consensus finality (Ethereum 2.0, etc),
    /// proves that a block has reached finality.
    async fn verify_finality_proof(&self, finality_proof: &FinalityProof) -> Result<bool, Error>;

    /// Get a block header
    async fn get_header(&self, block_hash: &[u8; 32]) -> Result<Option<BlockHeader>, Error>;
}
