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
use dwow_sdk::{crypto::pasta_prelude::PrimeField, error::ContractResult, error::ContractError, pasta::pallas};

use crate::light_client::{MerkleProof, FinalityProof};

/// Chain identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChainId {
    Ethereum = 0,
    Monero = 1,
    Zcash = 2,
    Aztec = 3,
    Litecoin = 4,
}

impl ChainId {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Ethereum), 1 => Some(Self::Monero), 2 => Some(Self::Zcash),
            3 => Some(Self::Aztec), 4 => Some(Self::Litecoin), _ => None,
        }
    }
    pub fn as_u8(&self) -> u8 { *self as u8 }
    pub fn encode(&self) -> Vec<u8> { vec![self.as_u8()] }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.is_empty() { return Err(ContractError::IoError("ChainId: empty".into())); }
        Self::from_u8(data[0]).ok_or_else(|| ContractError::IoError(format!("ChainId: unknown {}", data[0])))
    }
}

/// Transaction hash type
#[derive(Debug, Clone)]
pub struct TxHash { pub chain: ChainId, pub hash: [u8; 32], }

#[expect(clippy::unwrap_used, reason = "slice length checked above")]
impl TxHash { pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(33); b.push(self.chain.as_u8()); b.extend_from_slice(&self.hash); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 33 { return Err(ContractError::IoError(format!("TxHash: expected 33 bytes, got {}", data.len()))); } Ok(TxHash { chain: ChainId::decode(&data[0..1])?, hash: data[1..33].try_into().unwrap() }) } }

/// Deposit parameters from external chain
#[derive(Debug, Clone)]
pub struct ExternalDeposit {
    pub chain: ChainId, pub amount: u64, pub recipient_cap: [u8; 32],
    pub block_hash: [u8; 32], pub merkle_proof: MerkleProof,
    pub finality_proof: Option<FinalityProof>, pub chain_data: ChainData,
}

/// Chain-specific deposit data
#[derive(Debug, Clone)]
pub enum ChainData {
    Ethereum { contract: [u8; 20], log_index: u64 },
    Monero { tx_hash: [u8; 32], output_index: u64, amount: u64 },
    Zcash { nullifier: [u8; 32], commitment: [u8; 32], anchor: [u8; 32] },
    Aztec { nullifier: [u8; 32], commitment: [u8; 32], proof_bytes: Vec<u8> },
    Litecoin { tx_hash: [u8; 32], output_index: u64, is_confidential: bool },
}

#[expect(clippy::unwrap_used, reason = "slice length checked above")]
impl ChainData { pub fn encode(&self) -> Vec<u8> { match self { Self::Ethereum{contract,log_index}=>{let mut b=Vec::with_capacity(29);b.push(0);b.extend_from_slice(contract);b.extend_from_slice(&log_index.to_le_bytes());b} Self::Monero{tx_hash,output_index,amount}=>{let mut b=Vec::with_capacity(49);b.push(1);b.extend_from_slice(tx_hash);b.extend_from_slice(&output_index.to_le_bytes());b.extend_from_slice(&amount.to_le_bytes());b} Self::Zcash{nullifier,commitment,anchor}=>{let mut b=Vec::with_capacity(97);b.push(2);b.extend_from_slice(nullifier);b.extend_from_slice(commitment);b.extend_from_slice(anchor);b} Self::Aztec{nullifier,commitment,proof_bytes}=>{let mut b=Vec::with_capacity(66+proof_bytes.len());b.push(3);b.extend_from_slice(nullifier);b.extend_from_slice(commitment);b.push(proof_bytes.len() as u8);b.extend_from_slice(proof_bytes);b} Self::Litecoin{tx_hash,output_index,is_confidential}=>{let mut b=Vec::with_capacity(42);b.push(4);b.extend_from_slice(tx_hash);b.extend_from_slice(&output_index.to_le_bytes());b.push(*is_confidential as u8);b} } } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.is_empty() { return Err(ContractError::IoError("ChainData: empty".into())); } match data[0] { 0=>{ if data.len()!=29 { return Err(ContractError::IoError("ChainData::Ethereum: bad len".into())); } Ok(Self::Ethereum{contract:data[1..21].try_into().unwrap(),log_index:u64::from_le_bytes(data[21..29].try_into().unwrap())}) } 1=>{ if data.len()!=49 { return Err(ContractError::IoError("ChainData::Monero: bad len".into())); } Ok(Self::Monero{tx_hash:data[1..33].try_into().unwrap(),output_index:u64::from_le_bytes(data[33..41].try_into().unwrap()),amount:u64::from_le_bytes(data[41..49].try_into().unwrap())}) } 2=>{ if data.len()!=97 { return Err(ContractError::IoError("ChainData::Zcash: bad len".into())); } Ok(Self::Zcash{nullifier:data[1..33].try_into().unwrap(),commitment:data[33..65].try_into().unwrap(),anchor:data[65..97].try_into().unwrap()}) } 3=>{ if data.len()<66 { return Err(ContractError::IoError("ChainData::Aztec: too short".into())); } let pb_len=data[65] as usize; if data.len()!=66+pb_len { return Err(ContractError::IoError("ChainData::Aztec: bad len".into())); } Ok(Self::Aztec{nullifier:data[1..33].try_into().unwrap(),commitment:data[33..65].try_into().unwrap(),proof_bytes:data[66..66+pb_len].to_vec()}) } 4=>{ if data.len()!=42 { return Err(ContractError::IoError("ChainData::Litecoin: bad len".into())); } Ok(Self::Litecoin{tx_hash:data[1..33].try_into().unwrap(),output_index:u64::from_le_bytes(data[33..41].try_into().unwrap()),is_confidential:data[41]!=0}) } _=>Err(ContractError::IoError(format!("ChainData: unknown variant {}",data[0]))) } } }

/// Deposit parameters from external chain
#[expect(clippy::unwrap_used, reason = "slice length checked above")]
impl ExternalDeposit { pub fn encode(&self) -> Vec<u8> { let mp = self.merkle_proof.encode(); let fp = self.finality_proof.as_ref().map(|f| f.encode()); let cd = self.chain_data.encode(); let mut b = Vec::with_capacity(74+mp.len()+fp.as_ref().map_or(1,|v| 1+v.len())+cd.len()); b.push(self.chain.as_u8()); b.extend_from_slice(&self.amount.to_le_bytes()); b.extend_from_slice(&self.recipient_cap); b.extend_from_slice(&self.block_hash); let mp_len: u32 = mp.len() as u32; b.extend_from_slice(&mp_len.to_le_bytes()); b.extend_from_slice(&mp); b.push(self.finality_proof.is_some() as u8); if let Some(ref f) = self.finality_proof { let fb = f.encode(); let fb_len: u32 = fb.len() as u32; b.extend_from_slice(&fb_len.to_le_bytes()); b.extend_from_slice(&fb); } let cd_len: u32 = cd.len() as u32; b.extend_from_slice(&cd_len.to_le_bytes()); b.extend_from_slice(&cd); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() < 78 { return Err(ContractError::IoError("ExternalDeposit: too short".into())); } let chain = ChainId::decode(&data[0..1])?; let amount = u64::from_le_bytes(data[1..9].try_into().unwrap()); let recipient_cap: [u8;32] = data[9..41].try_into().unwrap(); let block_hash: [u8;32] = data[41..73].try_into().unwrap(); let mp_len = u32::from_le_bytes(data[73..77].try_into().unwrap()) as usize; if data.len() < 77+mp_len { return Err(ContractError::IoError("ExternalDeposit: mp truncated".into())); } let merkle_proof = MerkleProof::decode(&data[77..77+mp_len])?; let mut pos = 77+mp_len; if data.len() < pos+1 { return Err(ContractError::IoError("ExternalDeposit: fp flag missing".into())); } let has_fp = data[pos] != 0; pos += 1; let finality_proof = if has_fp { if data.len() < pos+4 { return Err(ContractError::IoError("ExternalDeposit: fp len truncated".into())); } let fp_len = u32::from_le_bytes(data[pos..pos+4].try_into().unwrap()) as usize; pos += 4; if data.len() < pos+fp_len { return Err(ContractError::IoError("ExternalDeposit: fp truncated".into())); } let fp = FinalityProof::decode(&data[pos..pos+fp_len])?; pos += fp_len; Some(fp) } else { None }; if data.len() < pos+4 { return Err(ContractError::IoError("ExternalDeposit: cd len missing".into())); } let cd_len = u32::from_le_bytes(data[pos..pos+4].try_into().unwrap()) as usize; pos += 4; let chain_data = ChainData::decode(&data[pos..pos+cd_len])?; Ok(ExternalDeposit { chain, amount, recipient_cap, block_hash, merkle_proof, finality_proof, chain_data }) } }

/// Verified deposit
#[derive(Debug, Clone)]
pub struct VerifiedDeposit { pub chain: ChainId, pub amount: u64, pub recipient_cap: [u8; 32], pub commitment: [u8; 32], }
#[expect(clippy::unwrap_used, reason = "slice length checked above")]
impl VerifiedDeposit { pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(73); b.push(self.chain.as_u8()); b.extend_from_slice(&self.amount.to_le_bytes()); b.extend_from_slice(&self.recipient_cap); b.extend_from_slice(&self.commitment); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 73 { return Err(ContractError::IoError(format!("VerifiedDeposit: expected 73 bytes, got {}", data.len()))); } Ok(VerifiedDeposit { chain: ChainId::decode(&data[0..1])?, amount: u64::from_le_bytes(data[1..9].try_into().unwrap()), recipient_cap: data[9..41].try_into().unwrap(), commitment: data[41..73].try_into().unwrap() }) } }

/// Withdrawal request
#[derive(Debug, Clone)]
pub struct WithdrawalRequest { pub chain: ChainId, pub nullifier: [u8; 32], pub recipient_hash: [u8; 32], pub amount: u64, pub fee: u64, }
#[expect(clippy::unwrap_used, reason = "slice length checked above")]
impl WithdrawalRequest { pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(81); b.push(self.chain.as_u8()); b.extend_from_slice(&self.nullifier); b.extend_from_slice(&self.recipient_hash); b.extend_from_slice(&self.amount.to_le_bytes()); b.extend_from_slice(&self.fee.to_le_bytes()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 81 { return Err(ContractError::IoError(format!("WithdrawalRequest: expected 81 bytes, got {}", data.len()))); } Ok(WithdrawalRequest { chain: ChainId::decode(&data[0..1])?, nullifier: data[1..33].try_into().unwrap(), recipient_hash: data[33..65].try_into().unwrap(), amount: u64::from_le_bytes(data[65..73].try_into().unwrap()), fee: u64::from_le_bytes(data[73..81].try_into().unwrap()) }) } }

/// Verified withdrawal
#[derive(Debug, Clone)]
pub struct VerifiedWithdrawal { pub chain: ChainId, pub nullifier: [u8; 32], pub recipient_address: Vec<u8>, pub amount: u64, pub fee: u64, }
#[expect(clippy::unwrap_used, reason = "slice length checked above")]
impl VerifiedWithdrawal { pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(50+self.recipient_address.len()); b.push(self.chain.as_u8()); b.extend_from_slice(&self.nullifier); b.push(self.recipient_address.len() as u8); b.extend_from_slice(&self.recipient_address); b.extend_from_slice(&self.amount.to_le_bytes()); b.extend_from_slice(&self.fee.to_le_bytes()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() < 50 { return Err(ContractError::IoError("VerifiedWithdrawal: too short".into())); } let chain = ChainId::decode(&data[0..1])?; let nullifier: [u8;32] = data[1..33].try_into().unwrap(); let addr_len = data[33] as usize; let addr_end = 34+addr_len; if data.len() < addr_end+16 { return Err(ContractError::IoError("VerifiedWithdrawal: truncated".into())); } Ok(VerifiedWithdrawal { chain, nullifier, recipient_address: data[34..addr_end].to_vec(), amount: u64::from_le_bytes(data[addr_end..addr_end+8].try_into().unwrap()), fee: u64::from_le_bytes(data[addr_end+8..addr_end+16].try_into().unwrap()) }) } }

// ============================================================================
// HTLC Types
// ============================================================================

/// HTLC state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HtlcState { Pending = 0, Claimable = 1, Claimed = 2, Refunded = 3, }

impl TryFrom<u8> for HtlcState {
    type Error = dwow_sdk::error::ContractError;
    fn try_from(b: u8) -> Result<Self, Self::Error> { match b { 0 => Ok(Self::Pending), 1 => Ok(Self::Claimable), 2 => Ok(Self::Claimed), 3 => Ok(Self::Refunded), _ => Err(dwow_sdk::error::ContractError::InvalidFunction), } }
}

impl HtlcState { pub fn encode(&self) -> Vec<u8> { vec![*self as u8] } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.is_empty() { return Err(ContractError::IoError("HtlcState: empty".into())); } Self::try_from(data[0]) } }

/// HTLC swap data
#[derive(Debug, Clone)]
pub struct HtlcSwap { pub swap_id: [u8; 32], pub hash: pallas::Base, pub timelock: u64, pub amount: u64, pub external_sender: Vec<u8>, pub external_recipient: Vec<u8>, pub state: HtlcState, pub created_at: u64, }
#[expect(clippy::unwrap_used, reason = "slice length checked above")]
impl HtlcSwap { pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(82+self.external_sender.len()+self.external_recipient.len()); b.extend_from_slice(&self.swap_id); b.extend_from_slice(&self.hash.to_repr()); b.extend_from_slice(&self.timelock.to_le_bytes()); b.extend_from_slice(&self.amount.to_le_bytes()); b.push(self.external_sender.len() as u8); b.extend_from_slice(&self.external_sender); b.push(self.external_recipient.len() as u8); b.extend_from_slice(&self.external_recipient); b.push(self.state as u8); b.extend_from_slice(&self.created_at.to_le_bytes()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() < 82 { return Err(ContractError::IoError("HtlcSwap: too short".into())); } let swap_id: [u8;32] = data[0..32].try_into().unwrap(); let hash = Option::<pallas::Base>::from(pallas::Base::from_repr(data[32..64].try_into().unwrap())).ok_or_else(|| ContractError::IoError("HtlcSwap: invalid hash".into()))?; let timelock = u64::from_le_bytes(data[64..72].try_into().unwrap()); let amount = u64::from_le_bytes(data[72..80].try_into().unwrap()); let sender_len = data[80] as usize; let mut pos = 81+sender_len; if data.len() < pos+1 { return Err(ContractError::IoError("HtlcSwap: sender truncated".into())); } let external_sender = data[81..pos].to_vec(); let recip_len = data[pos] as usize; pos += 1; if data.len() < pos+recip_len+1+8 { return Err(ContractError::IoError("HtlcSwap: recipient truncated".into())); } let external_recipient = data[pos..pos+recip_len].to_vec(); pos += recip_len; let state = HtlcState::decode(&data[pos..pos+1])?; pos += 1; let created_at = u64::from_le_bytes(data[pos..pos+8].try_into().unwrap()); Ok(HtlcSwap { swap_id, hash, timelock, amount, external_sender, external_recipient, state, created_at }) } }

/// HTLC deposit parameters
#[derive(Debug, Clone)]
pub struct HtlcDeposit { pub swap_id: [u8; 32], pub expected_hash: pallas::Base, pub timelock: u64, pub deposit: ExternalDeposit, }

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
