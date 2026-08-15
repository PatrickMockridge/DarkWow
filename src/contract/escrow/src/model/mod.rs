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

//! Escrow contract data structures
//!
//! ## State Machine
//!
//! ```text
//! Created ──[Fund]──> Funded ──[Claim]──> Claimed
//!                   │                │
//!                   │                └──[Refund]──> Refunded
//!                   │
//!                   └──[Cancel]──> Cancelled
//! ```
//!
//! - **Created**: Escrow created but not yet funded
//! - **Funded**: Funds locked in commitment, awaiting release condition
//! - **Claimed**: Seller proved knowledge of seller_secret, funds released
//! - **Refunded**: Buyer proved timeout reached, funds returned
//! - **Cancelled**: Buyer cancelled before funding

use dwow_sdk::{
    crypto::{pasta_prelude::PrimeField, poseidon_hash, MerkleNode, PublicKey},
    error::ContractError,
    pasta::{group::GroupEncoding, pallas},
};

/// Escrow unique identifier (hash of escrow data)
#[derive(Debug, Clone, Copy, Eq, PartialEq,)]
pub struct EscrowId(pub pallas::Base);
impl EscrowId {
    pub const ENCODED_SIZE: usize = 32;
    pub fn inner(&self) -> pallas::Base { self.0 }
    pub fn to_bytes(&self) -> [u8; 32] { self.0.to_repr() }
    pub fn from_bytes(x: [u8; 32]) -> Option<Self> {
        pallas::Base::from_repr(x).into_option().map(Self)
    }
    pub fn is_zero(&self) -> bool { self.0 == pallas::Base::zero() }
    pub fn zero() -> Self { Self(pallas::Base::zero()) }
    pub fn encode(&self) -> Vec<u8> { self.to_bytes().to_vec() }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 32 { return Err(ContractError::IoError(format!("EscrowId: expected 32 bytes, got {}", data.len()))); }
        Self::from_bytes(data[0..32].try_into().unwrap()).ok_or_else(|| ContractError::IoError("EscrowId: invalid".into()))
    }
}

/// Represents the current state of an escrow
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EscrowState {
    /// Escrow created but not yet funded
    Created = 0,
    /// Funds locked, awaiting release condition
    Funded = 1,
    /// Seller claimed funds
    Claimed = 2,
    /// Buyer refunded after timeout
    Refunded = 3,
    /// Cancelled by buyer before funding
    Cancelled = 4,
}

impl TryFrom<u8> for EscrowState {
    type Error = dwow_sdk::error::ContractError;

    fn try_from(b: u8) -> Result<Self, Self::Error> {
        match b {
            0 => Ok(Self::Created),
            1 => Ok(Self::Funded),
            2 => Ok(Self::Claimed),
            3 => Ok(Self::Refunded),
            4 => Ok(Self::Cancelled),
            _ => Err(dwow_sdk::error::ContractError::InvalidFunction),
        }
    }
}

/// Core escrow data stored on-chain.
///
/// ## Box + Purse Integration (Tier 1 Refactor)
///
/// The escrow uses two genesis O-Cap primitives as **child calls**, not as
/// replacement fields. The existing model stays intact. The integration is
/// at the entrypoint level:
/// - Fund creates a Purse::DepositV1 child call to lock funds
/// - Claim calls Box::TakeV1 to consume the seller's claim capability
/// - Refund calls Box::TakeV1 to consume the buyer's refund capability
///
/// The Purse and Box contracts handle balance tracking, nullifier replay,
/// and ZK proof verification — the escrow contract validates only that
/// the child call targets the correct genesis contract.
#[derive(Debug, Clone)]
pub struct Escrow {
    pub version: u8,
    /// Escrow identifier (commitment)
    pub id: EscrowId,
    /// Buyer's public key
    pub buyer_pubkey: PublicKey,
    /// Seller's public key (derived from seller_secret)
    pub seller_pubkey: PublicKey,
    /// Value locked in escrow
    pub value: u64,
    /// Token ID
    pub token_id: pallas::Base,
    /// Timeout block height for refund
    pub timeout: u64,
    /// Current state
    pub state: EscrowState,
    /// Pedersen commitment for the locked value
    pub value_commit: pallas::Point,
    /// Blinding factor used in commitment
    pub value_blind: pallas::Scalar,
    /// Nullifier for the escrow (prevents double-claim/refund)
    pub spent_nullifier: pallas::Base,
    /// Block height when escrow was created
    pub created_at: u64,
    /// Block height when escrow was funded
    pub funded_at: Option<u64>,
    /// Per-instance seed for deriving capability-scoped keys.
    /// Generated by the buyer at creation, shared with the seller
    /// off-chain so both parties derive the same instance key.
    pub instance_seed: [u8; 32],
}

impl Escrow {
    /// Derive the escrow ID from buyer_pubkey, seller_pubkey, value, token_id, and timeout
    #[allow(dead_code)]
    pub fn derive_id(
        buyer_pubkey: &PublicKey,
        seller_pubkey: &PublicKey,
        value: u64,
        token_id: pallas::Base,
        timeout: u64,
        buyer_secret: pallas::Base,
        seller_secret: pallas::Base,
    ) -> EscrowId {
        let (bx, by) = buyer_pubkey.xy().expect("pk not identity");
        let (sx, sy) = seller_pubkey.xy().expect("pk not identity");
        EscrowId(poseidon_hash([
            bx, by, sx, sy,
            pallas::Base::from(value),
            token_id,
            pallas::Base::from(timeout),
            buyer_secret,
            seller_secret,
        ]))
    }

    /// Compute the nullifier that prevents double-claim or double-refund
    #[allow(dead_code)]
    pub fn compute_nullifier(&self, secret: pallas::Base) -> pallas::Base {
        poseidon_hash([self.id.0, secret])
    }
}

impl dwow_serial::Encodable for Escrow { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for Escrow { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl Escrow {
    /// Fixed canonical byte size:
    ///   version(1) + id(32) + buyer_pubkey(32) + seller_pubkey(32) +
    ///   value(8) + token_id(32) + timeout(8) + state(1) +
    ///   value_commit(32) + value_blind(32) + spent_nullifier(32) +
    ///   created_at(8) + funded_at(9) + instance_seed(32)
    pub const ENCODED_SIZE: usize = 291;

    /// Encode to canonical bytes (rho-calculus: quote).
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(Self::ENCODED_SIZE);
        b.push(self.version);
        b.extend_from_slice(&self.id.to_bytes());
        b.extend_from_slice(&self.buyer_pubkey.to_bytes());
        b.extend_from_slice(&self.seller_pubkey.to_bytes());
        b.extend_from_slice(&self.value.to_le_bytes());
        b.extend_from_slice(&self.token_id.to_repr());
        b.extend_from_slice(&self.timeout.to_le_bytes());
        b.push(self.state as u8);
        b.extend_from_slice(&self.value_commit.to_bytes());
        b.extend_from_slice(&self.value_blind.to_repr());
        b.extend_from_slice(&self.spent_nullifier.to_repr());
        b.extend_from_slice(&self.created_at.to_le_bytes());
        b.push(self.funded_at.is_some() as u8);
        if let Some(fa) = self.funded_at {
            b.extend_from_slice(&fa.to_le_bytes());
        } else {
            b.extend_from_slice(&[0u8; 8]);
        }
        b.extend_from_slice(&self.instance_seed);
        b
    }

    /// Decode from canonical bytes (rho-calculus: eval).
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "Escrow: expected {} bytes, got {}",
                Self::ENCODED_SIZE, data.len()
            )));
        }
        let version = data[0];
        let id = EscrowId::from_bytes(data[1..33].try_into().unwrap())
            .ok_or_else(|| ContractError::IoError("Escrow: invalid id".into()))?;
        let buyer_pubkey = PublicKey::from_bytes(data[33..65].try_into().unwrap())
            .map_err(|e| ContractError::IoError(format!("Escrow: invalid buyer_pubkey: {}", e)))?;
        let seller_pubkey = PublicKey::from_bytes(data[65..97].try_into().unwrap())
            .map_err(|e| ContractError::IoError(format!("Escrow: invalid seller_pubkey: {}", e)))?;
        let value = u64::from_le_bytes(data[97..105].try_into().unwrap());
        let token_id = Option::<pallas::Base>::from(
            pallas::Base::from_repr(data[105..137].try_into().unwrap())
        ).ok_or_else(|| ContractError::IoError("Escrow: invalid token_id".into()))?;
        let timeout = u64::from_le_bytes(data[137..145].try_into().unwrap());
        let state = EscrowState::try_from(data[145])?;
        let value_commit = Option::<pallas::Point>::from(
            pallas::Point::from_bytes(data[146..178].try_into().unwrap())
        ).ok_or_else(|| ContractError::IoError("Escrow: invalid value_commit".into()))?;
        let value_blind = Option::<pallas::Scalar>::from(
            pallas::Scalar::from_repr(data[178..210].try_into().unwrap())
        ).ok_or_else(|| ContractError::IoError("Escrow: invalid value_blind".into()))?;
        let spent_nullifier = Option::<pallas::Base>::from(
            pallas::Base::from_repr(data[210..242].try_into().unwrap())
        ).ok_or_else(|| ContractError::IoError("Escrow: invalid spent_nullifier".into()))?;
        let created_at = u64::from_le_bytes(data[242..250].try_into().unwrap());
        let funded_at = if data[250] != 0 {
            Some(u64::from_le_bytes(data[251..259].try_into().unwrap()))
        } else {
            None
        };
        let instance_seed: [u8; 32] = data[259..291].try_into().unwrap();
        Ok(Escrow {
            version,
            id,
            buyer_pubkey,
            seller_pubkey,
            value,
            token_id,
            timeout,
            state,
            value_commit,
            value_blind,
            spent_nullifier,
            created_at,
            funded_at,
            instance_seed,
        })
    }
}

/// Parameters for `Escrow::CreateEscrowV1`
#[derive(Debug, Clone,)]
pub struct CreateEscrowParamsV1 {
    /// Buyer's public key
    pub buyer_pubkey: PublicKey,
    /// Seller's public key
    pub seller_pubkey: PublicKey,
    /// Value to be locked in escrow
    pub value: u64,
    /// Token ID
    pub token_id: pallas::Base,
    /// Timeout block height (after which buyer can refund)
    pub timeout: u64,
    /// Commitment to the escrow parameters
    pub commitment: EscrowId,
    /// ZK proof public inputs:
    pub merkle_root: MerkleNode,
    /// Per-instance seed for deriving capability-scoped keys.
    /// Both parties use this seed with derive_instance to produce
    /// distinct pubkeys per escrow instance.
    pub instance_seed: [u8; 32],
}

impl dwow_serial::Encodable for CreateEscrowParamsV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for CreateEscrowParamsV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl CreateEscrowParamsV1 { pub const ENCODED_SIZE: usize = 208; pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(208); b.extend_from_slice(&self.buyer_pubkey.to_bytes()); b.extend_from_slice(&self.seller_pubkey.to_bytes()); b.extend_from_slice(&self.value.to_le_bytes()); b.extend_from_slice(&self.token_id.to_repr()); b.extend_from_slice(&self.timeout.to_le_bytes()); b.extend_from_slice(&self.commitment.encode()); b.extend_from_slice(&self.merkle_root.to_bytes()); b.extend_from_slice(&self.instance_seed); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 208 { return Err(ContractError::IoError(format!("CreateEscrowParamsV1: expected 208 bytes, got {}", data.len()))); } Ok(CreateEscrowParamsV1 { buyer_pubkey: PublicKey::from_bytes(data[0..32].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("CreateEscrowParamsV1: invalid buyer_pubkey: {}", e)))?, seller_pubkey: PublicKey::from_bytes(data[32..64].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("CreateEscrowParamsV1: invalid seller_pubkey: {}", e)))?, value: u64::from_le_bytes(data[64..72].try_into().unwrap()), token_id: Option::<pallas::Base>::from(pallas::Base::from_repr(data[72..104].try_into().unwrap())).ok_or_else(|| ContractError::IoError("CreateEscrowParamsV1: invalid token_id".into()))?, timeout: u64::from_le_bytes(data[104..112].try_into().unwrap()), commitment: EscrowId::decode(&data[112..144])?, merkle_root: MerkleNode::from_bytes(data[144..176].try_into().unwrap()).ok_or_else(|| ContractError::IoError("CreateEscrowParamsV1: invalid merkle_root".into()))?, instance_seed: data[176..208].try_into().unwrap() }) } }

/// State update for `Escrow::CreateEscrowV1`
#[derive(Debug, Clone)]
pub struct CreateEscrowUpdateV1 {
    /// The created escrow record
    pub escrow: Escrow,
}

impl dwow_serial::Encodable for CreateEscrowUpdateV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for CreateEscrowUpdateV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl CreateEscrowUpdateV1 {
    pub fn encode(&self) -> Vec<u8> { self.escrow.encode() }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        Ok(CreateEscrowUpdateV1 { escrow: Escrow::decode(data)? })
    }
}

/// Parameters for `Escrow::FundV1`
#[derive(Debug, Clone,)]
pub struct FundEscrowParamsV1 {
    /// Escrow ID
    pub escrow_id: EscrowId,
    /// Value commitment (Pedersen)
    pub value_commit: pallas::Point,
    /// Merkle proof of the commitment
    pub merkle_proof: Vec<pallas::Base>,
    /// ZK proof public inputs
    pub merkle_root: MerkleNode,
}

impl dwow_serial::Encodable for FundEscrowParamsV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for FundEscrowParamsV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl FundEscrowParamsV1 { pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(65+self.merkle_proof.len()*32); b.extend_from_slice(&self.escrow_id.encode()); b.extend_from_slice(&self.value_commit.to_bytes()); b.push(self.merkle_proof.len() as u8); for p in &self.merkle_proof { b.extend_from_slice(&p.to_repr()); } b.extend_from_slice(&self.merkle_root.to_bytes()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() < 65 { return Err(ContractError::IoError("FundEscrowParamsV1: too short".into())); } let escrow_id = EscrowId::decode(&data[0..32])?; let value_commit = Option::<pallas::Point>::from(pallas::Point::from_bytes(data[32..64].try_into().unwrap())).ok_or_else(|| ContractError::IoError("FundEscrowParamsV1: invalid value_commit".into()))?; let mp_count = data[64] as usize; let mp_end = 65+mp_count*32; if data.len() < mp_end+32 { return Err(ContractError::IoError("FundEscrowParamsV1: merkle_proof truncated".into())); } let mut merkle_proof = Vec::with_capacity(mp_count); for i in 0..mp_count { merkle_proof.push(Option::<pallas::Base>::from(pallas::Base::from_repr(data[65+i*32..65+(i+1)*32].try_into().unwrap())).ok_or_else(|| ContractError::IoError(format!("FundEscrowParamsV1: invalid merkle_proof[{}]", i)))?); } let merkle_root = MerkleNode::from_bytes(data[mp_end..mp_end+32].try_into().unwrap()).ok_or_else(|| ContractError::IoError("FundEscrowParamsV1: invalid merkle_root".into()))?; Ok(FundEscrowParamsV1 { escrow_id, value_commit, merkle_proof, merkle_root }) } }

/// State update for `Escrow::FundV1`
#[derive(Debug, Clone)]
pub struct FundEscrowUpdateV1 {
    /// The funded escrow record (state already Funded)
    pub escrow: Escrow,
}

impl dwow_serial::Encodable for FundEscrowUpdateV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for FundEscrowUpdateV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl FundEscrowUpdateV1 {
    pub fn encode(&self) -> Vec<u8> { self.escrow.encode() }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        Ok(FundEscrowUpdateV1 { escrow: Escrow::decode(data)? })
    }
}

/// Parameters for `Escrow::ClaimV1`
#[derive(Debug, Clone,)]
pub struct ClaimEscrowParamsV1 {
    /// Escrow ID
    pub escrow_id: EscrowId,
    /// Seller's secret (proves ownership)
    pub seller_secret: pallas::Base,
    /// Nullifier revealing the escrow is spent
    pub spent_nullifier: pallas::Base,
    /// Recipient public key for the funds
    pub recipient_pubkey: PublicKey,
}

impl dwow_serial::Encodable for ClaimEscrowParamsV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for ClaimEscrowParamsV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl ClaimEscrowParamsV1 { pub const ENCODED_SIZE: usize = 128; pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(128); b.extend_from_slice(&self.escrow_id.encode()); b.extend_from_slice(&self.seller_secret.to_repr()); b.extend_from_slice(&self.spent_nullifier.to_repr()); b.extend_from_slice(&self.recipient_pubkey.to_bytes()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 128 { return Err(ContractError::IoError(format!("ClaimEscrowParamsV1: expected 128 bytes, got {}", data.len()))); } Ok(ClaimEscrowParamsV1 { escrow_id: EscrowId::decode(&data[0..32])?, seller_secret: Option::<pallas::Base>::from(pallas::Base::from_repr(data[32..64].try_into().unwrap())).ok_or_else(|| ContractError::IoError("ClaimEscrowParamsV1: invalid seller_secret".into()))?, spent_nullifier: Option::<pallas::Base>::from(pallas::Base::from_repr(data[64..96].try_into().unwrap())).ok_or_else(|| ContractError::IoError("ClaimEscrowParamsV1: invalid spent_nullifier".into()))?, recipient_pubkey: PublicKey::from_bytes(data[96..128].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("ClaimEscrowParamsV1: invalid recipient_pubkey: {}", e)))? }) } }

/// State update for `Escrow::ClaimEscrowV1`
#[derive(Debug, Clone)]
pub struct ClaimEscrowUpdateV1 {
    /// The claimed escrow record (state already Claimed)
    pub escrow: Escrow,
    /// Nullifier for the spent escrow
    pub spent_nullifier: pallas::Base,
}

impl dwow_serial::Encodable for ClaimEscrowUpdateV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for ClaimEscrowUpdateV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl ClaimEscrowUpdateV1 {
    pub fn encode(&self) -> Vec<u8> {
        let mut b = self.escrow.encode();
        b.extend_from_slice(&self.spent_nullifier.to_repr());
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Escrow::ENCODED_SIZE + 32 {
            return Err(ContractError::IoError(format!("ClaimEscrowUpdateV1: expected {} bytes, got {}", Escrow::ENCODED_SIZE + 32, data.len())));
        }
        let escrow = Escrow::decode(&data[0..Escrow::ENCODED_SIZE])?;
        let spent_nullifier = Option::<pallas::Base>::from(
            pallas::Base::from_repr(data[Escrow::ENCODED_SIZE..].try_into().unwrap())
        ).ok_or_else(|| ContractError::IoError("ClaimEscrowUpdateV1: invalid spent_nullifier".into()))?;
        Ok(ClaimEscrowUpdateV1 { escrow, spent_nullifier })
    }
}

/// Parameters for `Escrow::RefundV1`
#[derive(Debug, Clone,)]
pub struct RefundEscrowParamsV1 {
    /// Escrow ID
    pub escrow_id: EscrowId,
    /// Buyer's secret (proves ownership)
    pub buyer_secret: pallas::Base,
    /// Nullifier revealing the escrow is spent
    pub spent_nullifier: pallas::Base,
    /// Current block height (proves timeout reached)
    pub current_block: u64,
    /// Timeout block height (must match escrow timeout for ZK proof)
    pub timeout: u64,
    /// Recipient public key for the refunded funds
    pub recipient_pubkey: PublicKey,
}

impl dwow_serial::Encodable for RefundEscrowParamsV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for RefundEscrowParamsV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl RefundEscrowParamsV1 { pub const ENCODED_SIZE: usize = 144; pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(144); b.extend_from_slice(&self.escrow_id.encode()); b.extend_from_slice(&self.buyer_secret.to_repr()); b.extend_from_slice(&self.spent_nullifier.to_repr()); b.extend_from_slice(&self.current_block.to_le_bytes()); b.extend_from_slice(&self.timeout.to_le_bytes()); b.extend_from_slice(&self.recipient_pubkey.to_bytes()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 144 { return Err(ContractError::IoError(format!("RefundEscrowParamsV1: expected 144 bytes, got {}", data.len()))); } Ok(RefundEscrowParamsV1 { escrow_id: EscrowId::decode(&data[0..32])?, buyer_secret: Option::<pallas::Base>::from(pallas::Base::from_repr(data[32..64].try_into().unwrap())).ok_or_else(|| ContractError::IoError("RefundEscrowParamsV1: invalid buyer_secret".into()))?, spent_nullifier: Option::<pallas::Base>::from(pallas::Base::from_repr(data[64..96].try_into().unwrap())).ok_or_else(|| ContractError::IoError("RefundEscrowParamsV1: invalid spent_nullifier".into()))?, current_block: u64::from_le_bytes(data[96..104].try_into().unwrap()), timeout: u64::from_le_bytes(data[104..112].try_into().unwrap()), recipient_pubkey: PublicKey::from_bytes(data[112..144].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("RefundEscrowParamsV1: invalid recipient_pubkey: {}", e)))? }) } }

/// State update for `Escrow::RefundEscrowV1`
#[derive(Debug, Clone)]
pub struct RefundEscrowUpdateV1 {
    /// The refunded escrow record (state already Refunded)
    pub escrow: Escrow,
    /// Nullifier for the spent escrow
    pub spent_nullifier: pallas::Base,
}

impl dwow_serial::Encodable for RefundEscrowUpdateV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for RefundEscrowUpdateV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl RefundEscrowUpdateV1 {
    pub fn encode(&self) -> Vec<u8> {
        let mut b = self.escrow.encode();
        b.extend_from_slice(&self.spent_nullifier.to_repr());
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Escrow::ENCODED_SIZE + 32 {
            return Err(ContractError::IoError(format!("RefundEscrowUpdateV1: expected {} bytes, got {}", Escrow::ENCODED_SIZE + 32, data.len())));
        }
        let escrow = Escrow::decode(&data[0..Escrow::ENCODED_SIZE])?;
        let spent_nullifier = Option::<pallas::Base>::from(
            pallas::Base::from_repr(data[Escrow::ENCODED_SIZE..].try_into().unwrap())
        ).ok_or_else(|| ContractError::IoError("RefundEscrowUpdateV1: invalid spent_nullifier".into()))?;
        Ok(RefundEscrowUpdateV1 { escrow, spent_nullifier })
    }
}

/// Parameters for `Escrow::CancelV1`
#[derive(Debug, Clone,)]
pub struct CancelEscrowParamsV1 {
    /// Escrow ID
    pub escrow_id: EscrowId,
    /// Buyer's public key (must match stored escrow)
    pub buyer_pubkey: PublicKey,
    /// Cancel nullifier = H(escrow_id, buyer_secret) — ZK circuit output
    pub cancel_nullifier: pallas::Base,
}

impl dwow_serial::Encodable for CancelEscrowParamsV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for CancelEscrowParamsV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl CancelEscrowParamsV1 { pub const ENCODED_SIZE: usize = 96; pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(96); b.extend_from_slice(&self.escrow_id.encode()); b.extend_from_slice(&self.buyer_pubkey.to_bytes()); b.extend_from_slice(&self.cancel_nullifier.to_repr()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 96 { return Err(ContractError::IoError(format!("CancelEscrowParamsV1: expected 96 bytes, got {}", data.len()))); } Ok(CancelEscrowParamsV1 { escrow_id: EscrowId::decode(&data[0..32])?, buyer_pubkey: PublicKey::from_bytes(data[32..64].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("CancelEscrowParamsV1: invalid buyer_pubkey: {}", e)))?, cancel_nullifier: Option::<pallas::Base>::from(pallas::Base::from_repr(data[64..96].try_into().unwrap())).ok_or_else(|| ContractError::IoError("CancelEscrowParamsV1: invalid cancel_nullifier".into()))? }) } }

/// State update for `Escrow::CancelV1`
#[derive(Debug, Clone)]
pub struct CancelEscrowUpdateV1 {
    /// The cancelled escrow record (state already Cancelled)
    pub escrow: Escrow,
    /// Cancel nullifier — recorded to prevent double-cancel
    pub cancel_nullifier: pallas::Base,
}

impl dwow_serial::Encodable for CancelEscrowUpdateV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for CancelEscrowUpdateV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl CancelEscrowUpdateV1 {
    pub fn encode(&self) -> Vec<u8> {
        let mut b = self.escrow.encode();
        b.extend_from_slice(&self.cancel_nullifier.to_repr());
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Escrow::ENCODED_SIZE + 32 {
            return Err(ContractError::IoError(format!("CancelEscrowUpdateV1: expected {} bytes, got {}", Escrow::ENCODED_SIZE + 32, data.len())));
        }
        let escrow = Escrow::decode(&data[0..Escrow::ENCODED_SIZE])?;
        let cancel_nullifier = Option::<pallas::Base>::from(
            pallas::Base::from_repr(data[Escrow::ENCODED_SIZE..].try_into().unwrap())
        ).ok_or_else(|| ContractError::IoError("CancelEscrowUpdateV1: invalid cancel_nullifier".into()))?;
        Ok(CancelEscrowUpdateV1 { escrow, cancel_nullifier })
    }
}
