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

//! ChainHandler trait
//!
//! This trait defines the interface for chain-specific bridge handlers.
//! Each external chain implements this trait to provide:
//! - Deposit verification via light client
//! - Withdrawal execution
//! - Chain-specific address encoding
//!
//! ## Adding a New Chain
//!
//! To add support for a new chain:
//! 1. Implement `ChainHandler` for your chain
//! 2. Register in `ChainRegistry`
//! 3. NO changes to bridge core contract needed

use async_trait::async_trait;
use darkfi_sdk::{error::ContractResult, pasta::pallas};
use darkfi_serial::{SerialDecodable, SerialEncodable};

use crate::light_client::{MerkleProof, FinalityProof};

/// Chain identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, SerialEncodable, SerialDecodable)]
pub enum ChainId {
    Ethereum = 0,
    Monero = 1,
    Zcash = 2,
    Aztec = 3,
    Litecoin = 4,
}

impl ChainId {
    /// Convert from u8 to ChainId
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Ethereum),
            1 => Some(Self::Monero),
            2 => Some(Self::Zcash),
            3 => Some(Self::Aztec),
            4 => Some(Self::Litecoin),
            _ => None,
        }
    }

    /// Convert to u8
    pub fn as_u8(&self) -> u8 {
        *self as u8
    }
}

/// Transaction hash type (generic, chain-specific implementation)
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct TxHash {
    /// Chain identifier
    pub chain: ChainId,
    /// Raw transaction hash bytes
    pub hash: [u8; 32],
}

/// Deposit parameters from external chain
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ExternalDeposit {
    /// Source chain
    pub chain: ChainId,
    /// Deposit amount in smallest unit
    pub amount: u64,
    /// Recipient capability on destination chain
    pub recipient_cap: [u8; 32],
    /// Block hash containing the deposit
    pub block_hash: [u8; 32],
    /// Merkle proof of deposit inclusion
    pub merkle_proof: MerkleProof,
    /// Finality proof (if chain has finality)
    pub finality_proof: Option<FinalityProof>,
    /// Chain-specific deposit data
    pub chain_data: ChainData,
}

/// Chain-specific deposit data
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub enum ChainData {
    /// Ethereum: contract address and log index
    Ethereum {
        contract: [u8; 20],
        log_index: u64,
    },
    /// Monero: transaction hash and output index
    Monero {
        tx_hash: [u8; 32],
        output_index: u64,
        amount: u64,
    },
    /// Zcash: nullifier and commitment
    Zcash {
        nullifier: [u8; 32],
        commitment: [u8; 32],
        anchor: [u8; 32],
    },
    /// Aztec: note data
    Aztec {
        nullifier: [u8; 32],
        commitment: [u8; 32],
        proof_bytes: Vec<u8>,
    },
    /// Litecoin: transaction data
    Litecoin {
        tx_hash: [u8; 32],
        output_index: u64,
        is_confidential: bool,
    },
}

/// Verified deposit after light client verification
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct VerifiedDeposit {
    /// Source chain
    pub chain: ChainId,
    /// Verified amount
    pub amount: u64,
    /// Recipient capability
    pub recipient_cap: [u8; 32],
    /// Deposit commitment for bridge state
    pub commitment: [u8; 32],
}

/// Withdrawal request parameters
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct WithdrawalRequest {
    /// Source chain (where withdrawal executes)
    pub chain: ChainId,
    /// Nullifier proving deposit ownership
    pub nullifier: [u8; 32],
    /// Recipient address on external chain (hashed)
    pub recipient_hash: [u8; 32],
    /// Amount to withdraw
    pub amount: u64,
    /// Relayer fee
    pub fee: u64,
}

/// Verified withdrawal ready for execution
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct VerifiedWithdrawal {
    /// Chain where withdrawal executes
    pub chain: ChainId,
    /// Nullifier
    pub nullifier: [u8; 32],
    /// Decoded recipient address
    pub recipient_address: Vec<u8>,
    /// Amount in smallest unit
    pub amount: u64,
    /// Transaction fee
    pub fee: u64,
}

// ============================================================================
// HTLC Types (for Cross-Chain Atomic Swaps)
// ============================================================================

/// HTLC state for cross-chain coordination
#[derive(Debug, Clone, Copy, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub enum HtlcState {
    /// HTLC created, waiting for counterparty
    Pending = 0,
    /// Counterparty deposited, funds can be claimed
    Claimable = 1,
    /// Funds claimed by recipient
    Claimed = 2,
    /// Refunded after timelock expiration
    Refunded = 3,
}

impl TryFrom<u8> for HtlcState {
    type Error = darkfi_sdk::error::ContractError;

    fn try_from(b: u8) -> Result<Self, Self::Error> {
        match b {
            0 => Ok(Self::Pending),
            1 => Ok(Self::Claimable),
            2 => Ok(Self::Claimed),
            3 => Ok(Self::Refunded),
            _ => Err(darkfi_sdk::error::ContractError::InvalidFunction),
        }
    }
}

/// HTLC swap data for cross-chain atomic swap coordination
///
/// This struct tracks the state of an HTLC that coordinates with
/// the DarkWow atomic swap contract. The bridge executes claims/refunds
/// on external chains when the atomic swap state changes.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct HtlcSwap {
    /// Swap ID (matches atomic_swap SwapId)
    pub swap_id: [u8; 32],
    /// Hash that locks the HTLC (poseidon_hash(secret))
    pub hash: pallas::Base,
    /// Timelock block height (after which refund is allowed)
    pub timelock: u64,
    /// Amount locked
    pub amount: u64,
    /// Sender's address/representation on external chain
    pub external_sender: Vec<u8>,
    /// Recipient's address/representation on external chain
    pub external_recipient: Vec<u8>,
    /// Current state of the HTLC
    pub state: HtlcState,
    /// Block height when created
    pub created_at: u64,
}

/// HTLC deposit parameters for verification
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct HtlcDeposit {
    /// Swap ID to verify against
    pub swap_id: [u8; 32],
    /// Expected hash (should match HTLC.hash)
    pub expected_hash: pallas::Base,
    /// Expected timelock
    pub timelock: u64,
    /// External chain deposit data
    pub deposit: ExternalDeposit,
}

/// ChainHandler trait - implemented by each external chain
///
/// This trait is the plugin interface for chain-specific bridge operations.
/// Each chain implements this trait to provide deposit verification and
/// withdrawal execution.
///
/// ## Implementations
///
/// - `EthereumHandler`: ETH and ERC-20 withdrawals
/// - `MoneroHandler`: XMR withdrawals
/// - `ZcashHandler`: ZEC withdrawals
/// - `AztecHandler`: AZT withdrawals
/// - `LitecoinHandler`: LTC withdrawals
#[async_trait]
pub trait ChainHandler: Send + Sync {
    /// Get the chain this handler supports
    fn chain_id(&self) -> ChainId;

    /// Check if this handler is enabled
    fn is_enabled(&self) -> bool;

    /// Verify a deposit on the external chain
    ///
    /// Uses the light client to verify:
    /// - Block header validity
    /// - Merkle proof of deposit inclusion
    /// - Confirmation level (if required)
    async fn verify_deposit(&self, deposit: &ExternalDeposit) -> ContractResult;

    /// Verify a withdrawal request can be executed
    ///
    /// This verifies:
    /// - The withdrawal is well-formed
    /// - The recipient address is valid for this chain
    async fn verify_withdrawal(&self, withdrawal: &WithdrawalRequest) -> ContractResult;

    /// Execute a verified withdrawal on the external chain
    ///
    /// This should:
    /// - Sign the transaction
    /// - Broadcast to the network
    /// - Return the transaction hash
    async fn execute(&self, verified: &VerifiedWithdrawal) -> ContractResult;

    /// Estimate the fee for executing a withdrawal
    async fn estimate_fee(&self, withdrawal: &WithdrawalRequest) -> ContractResult;

    /// Verify a transaction confirmation
    async fn verify_confirmation(&self, tx_hash: &TxHash) -> ContractResult;

    // =========================================================================
    // HTLC Methods (for Cross-Chain Atomic Swaps)
    // =========================================================================

    /// Verify an HTLC deposit exists on the external chain
    ///
    /// This verifies:
    /// - The deposit matches the expected swap_id, hash, timelock
    /// - Sufficient confirmations have occurred
    /// - The deposit is locked correctly (matches HTLC terms)
    async fn verify_htlc_deposit(&self, htlc_deposit: &HtlcDeposit) -> ContractResult;

    /// Execute an HTLC claim on the external chain
    ///
    /// This should:
    /// - Build a transaction that reveals the secret and claims funds
    /// - Sign and broadcast the transaction
    async fn execute_htlc_claim(
        &self,
        swap_id: &[u8; 32],
        secret: pallas::Base,
        recipient: &[u8],
    ) -> ContractResult;

    /// Execute an HTLC refund on the external chain
    ///
    /// This should:
    /// - Verify the timelock has expired
    /// - Build a refund transaction returning funds to sender
    /// - Sign and broadcast the transaction
    async fn execute_htlc_refund(
        &self,
        swap_id: &[u8; 32],
        sender: &[u8],
    ) -> ContractResult;

    /// Get the current status of an HTLC on the external chain
    ///
    /// Returns the current state and any relevant block info.
    async fn get_htlc_status(&self, swap_id: &[u8; 32]) -> ContractResult;
}
