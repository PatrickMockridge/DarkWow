/* This file is part of DarkFi (https://dark.fi)
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
use darkfi_serial::{SerialDecodable, SerialEncodable};

/// Confirmation level for deposits
#[derive(Debug, Clone, Copy, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub enum ConfirmationLevel {
    /// Transaction in mempool (unconfirmed)
    Mempool,
    /// N blocks deep
    Confirmed(u64),
    /// Finalized (consensus guaranteed)
    Finalized,
}

/// Merkle proof for transaction inclusion
///
/// Contains the authentication path from a leaf
/// to the Merkle root.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct MerkleProof {
    /// Position of the leaf in the tree (0-indexed)
    pub position: u32,
    /// Authentication path (sibling hashes)
    pub path: Vec<[u8; 32]>,
}

/// Finality proof for chains with consensus finality
///
/// Proves that a block has been finalized according
/// to the chain's consensus rules.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct FinalityProof {
    /// Hash of the finalized block
    pub block_hash: [u8; 32],
    /// Height of the finalized block
    pub block_height: u64,
    /// Finality checkpoint hash
    pub checkpoint_hash: [u8; 32],
    /// Proof path from block to checkpoint
    pub proof: Vec<[u8; 32]>,
}

/// Block header data
///
/// Lightweight header verification without full block data.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct BlockHeader {
    /// Block hash
    pub hash: [u8; 32],
    /// Block height
    pub height: u64,
    /// Timestamp
    pub timestamp: u64,
    /// Merkle root
    pub merkle_root: [u8; 32],
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
