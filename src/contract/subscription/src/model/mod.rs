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

//! Subscription contract data structures
//!
//! ## Composability with DAO-Escrow
//!
//! This contract integrates with DAO-Escrow for tiered membership benefits:
//! - DAO-Escrow members get discounted subscription rates
//! - Membership note verified via Merkle proof
//! - Nullifier check prevents double-spending of membership
//!
//! ## State Machine
//!
//! ```text
//! Active ──[Cancel]──> Cancelled ──[Expiry]──> Expired
//!    │                                          │
//!    └──[Renew]──> Active                       │
//! ```
//!
//! - **Active**: Subscription is valid, lock_until_block > current_block
//! - **Cancelled**: User cancelled, refund available
//! - **Expired**: Time lock expired, refund available

use dwow_sdk::{
    crypto::{pasta_prelude::PrimeField, poseidon_hash, PublicKey},
    error::ContractError,
    pasta::pallas,
    pasta::group::GroupEncoding,
};

// Bridge impls: dwow_serial::Encodable/Decodable delegate to rho-calculus
// encode/decode so param structs can derive SerialEncodable/SerialDecodable
// without losing the deterministic byte layout.

use std::io::{Read, Write};

impl dwow_serial::Encodable for SubscriptionId {
    fn encode<W: Write>(&self, w: &mut W) -> Result<usize, std::io::Error> {
        let bytes = self.to_bytes();
        w.write_all(&bytes)?;
        Ok(32)
    }
}

impl dwow_serial::Decodable for SubscriptionId {
    fn decode<D: Read>(d: &mut D) -> Result<Self, std::io::Error> {
        let mut buf = [0u8; 32];
        d.read_exact(&mut buf)?;
        Self::decode(&buf).map_err(|e| std::io::Error::other(format!("{e}")))
    }
}

#[cfg(feature = "client")]
#[dwow_serial::async_trait]
impl dwow_serial::AsyncEncodable for SubscriptionId {
    async fn encode_async<W: dwow_serial::AsyncWrite + Unpin + Send>(&self, w: &mut W) -> Result<usize, std::io::Error> {
        let bytes = self.to_bytes();
        use dwow_serial::AsyncWriteExt;
        w.write_slice_async(&bytes).await?;
        Ok(32)
    }
}

#[cfg(feature = "client")]
#[dwow_serial::async_trait]
impl dwow_serial::AsyncDecodable for SubscriptionId {
    async fn decode_async<D: dwow_serial::AsyncRead + Unpin + Send>(d: &mut D) -> Result<Self, std::io::Error> {
        let mut buf = [0u8; 32];
        use dwow_serial::AsyncReadExt;
        d.read_slice_async(&mut buf).await?;
        Self::decode(&buf).map_err(|e| std::io::Error::other(format!("{e}")))
    }
}

impl dwow_serial::Encodable for Plan {
    fn encode<W: Write>(&self, w: &mut W) -> Result<usize, std::io::Error> {
        let mut len = 0;
        w.write_all(&[self.version])?;
        len += 1;
        w.write_all(&self.id.to_le_bytes())?;
        len += 4;
        w.write_all(&self.name_hash.to_repr())?;
        len += 32;
        w.write_all(&self.price.to_le_bytes())?;
        len += 8;
        w.write_all(&self.token_id.to_repr())?;
        len += 32;
        w.write_all(&self.duration_blocks.to_le_bytes())?;
        len += 8;
        w.write_all(&self.treasury_share.to_le_bytes())?;
        len += 4;
        w.write_all(&self.endowment_share.to_le_bytes())?;
        len += 4;
        w.write_all(&[self.active as u8])?;
        len += 1;
        w.write_all(&self.dao_escrow_discount.to_le_bytes())?;
        len += 4;
        w.write_all(&[self.required_dao_escrow.is_some() as u8])?;
        len += 1;
        if let Some(ref v) = self.required_dao_escrow {
            w.write_all(&v.to_repr())?;
            len += 32;
        }
        Ok(len)
    }
}

impl dwow_serial::Decodable for Plan {
    fn decode<D: Read>(d: &mut D) -> Result<Self, std::io::Error> {
        use dwow_sdk::pasta::group::GroupEncoding;

        let mut buf1 = [0u8; 1];
        d.read_exact(&mut buf1)?;
        let version = buf1[0];

        let mut buf4 = [0u8; 4];
        d.read_exact(&mut buf4)?;
        let id = u32::from_le_bytes(buf4);

        let mut buf32 = [0u8; 32];
        d.read_exact(&mut buf32)?;
        let name_hash = Option::<pallas::Base>::from(pallas::Base::from_repr(buf32))
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "Plan: invalid name_hash"))?;

        let mut buf8 = [0u8; 8];
        d.read_exact(&mut buf8)?;
        let price = u64::from_le_bytes(buf8);

        d.read_exact(&mut buf32)?;
        let token_id = Option::<pallas::Base>::from(pallas::Base::from_repr(buf32))
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "Plan: invalid token_id"))?;

        d.read_exact(&mut buf8)?;
        let duration_blocks = u64::from_le_bytes(buf8);

        d.read_exact(&mut buf4)?;
        let treasury_share = u32::from_le_bytes(buf4);

        d.read_exact(&mut buf4)?;
        let endowment_share = u32::from_le_bytes(buf4);

        d.read_exact(&mut buf1)?;
        let active = buf1[0] != 0;

        d.read_exact(&mut buf4)?;
        let dao_escrow_discount = u32::from_le_bytes(buf4);

        d.read_exact(&mut buf1)?;
        let required_dao_escrow = if buf1[0] != 0 {
            d.read_exact(&mut buf32)?;
            Some(Option::<pallas::Base>::from(pallas::Base::from_repr(buf32))
                .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "Plan: invalid required_dao_escrow"))?)
        } else {
            None
        };

        Ok(Plan {
            version,
            id,
            name_hash,
            price,
            token_id,
            duration_blocks,
            treasury_share,
            endowment_share,
            active,
            dao_escrow_discount,
            required_dao_escrow,
        })
    }
}

#[cfg(feature = "client")]
#[dwow_serial::async_trait]
impl dwow_serial::AsyncEncodable for Plan {
    async fn encode_async<W: dwow_serial::AsyncWrite + Unpin + Send>(&self, w: &mut W) -> Result<usize, std::io::Error> {
        let mut len = 0;
        use dwow_serial::AsyncWriteExt;
        w.write_slice_async(&[self.version]).await?;
        len += 1;
        w.write_slice_async(&self.id.to_le_bytes()).await?;
        len += 4;
        w.write_slice_async(&self.name_hash.to_repr()).await?;
        len += 32;
        w.write_slice_async(&self.price.to_le_bytes()).await?;
        len += 8;
        w.write_slice_async(&self.token_id.to_repr()).await?;
        len += 32;
        w.write_slice_async(&self.duration_blocks.to_le_bytes()).await?;
        len += 8;
        w.write_slice_async(&self.treasury_share.to_le_bytes()).await?;
        len += 4;
        w.write_slice_async(&self.endowment_share.to_le_bytes()).await?;
        len += 4;
        w.write_slice_async(&[self.active as u8]).await?;
        len += 1;
        w.write_slice_async(&self.dao_escrow_discount.to_le_bytes()).await?;
        len += 4;
        w.write_slice_async(&[self.required_dao_escrow.is_some() as u8]).await?;
        len += 1;
        if let Some(ref v) = self.required_dao_escrow {
            w.write_slice_async(&v.to_repr()).await?;
            len += 32;
        }
        Ok(len)
    }
}

#[cfg(feature = "client")]
#[dwow_serial::async_trait]
impl dwow_serial::AsyncDecodable for Plan {
    async fn decode_async<D: dwow_serial::AsyncRead + Unpin + Send>(d: &mut D) -> Result<Self, std::io::Error> {
        use dwow_sdk::pasta::group::GroupEncoding;
        use dwow_serial::AsyncReadExt;

        let mut buf1 = [0u8; 1];
        d.read_slice_async(&mut buf1).await?;
        let version = buf1[0];

        let mut buf4 = [0u8; 4];
        d.read_slice_async(&mut buf4).await?;
        let id = u32::from_le_bytes(buf4);

        let mut buf32 = [0u8; 32];
        d.read_slice_async(&mut buf32).await?;
        let name_hash = Option::<pallas::Base>::from(pallas::Base::from_repr(buf32))
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "Plan: invalid name_hash"))?;

        let mut buf8 = [0u8; 8];
        d.read_slice_async(&mut buf8).await?;
        let price = u64::from_le_bytes(buf8);

        d.read_slice_async(&mut buf32).await?;
        let token_id = Option::<pallas::Base>::from(pallas::Base::from_repr(buf32))
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "Plan: invalid token_id"))?;

        d.read_slice_async(&mut buf8).await?;
        let duration_blocks = u64::from_le_bytes(buf8);

        d.read_slice_async(&mut buf4).await?;
        let treasury_share = u32::from_le_bytes(buf4);

        d.read_slice_async(&mut buf4).await?;
        let endowment_share = u32::from_le_bytes(buf4);

        d.read_slice_async(&mut buf1).await?;
        let active = buf1[0] != 0;

        d.read_slice_async(&mut buf4).await?;
        let dao_escrow_discount = u32::from_le_bytes(buf4);

        d.read_slice_async(&mut buf1).await?;
        let required_dao_escrow = if buf1[0] != 0 {
            d.read_slice_async(&mut buf32).await?;
            Some(Option::<pallas::Base>::from(pallas::Base::from_repr(buf32))
                .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "Plan: invalid required_dao_escrow"))?)
        } else {
            None
        };

        Ok(Plan {
            version,
            id,
            name_hash,
            price,
            token_id,
            duration_blocks,
            treasury_share,
            endowment_share,
            active,
            dao_escrow_discount,
            required_dao_escrow,
        })
    }
}

// ============================================================================
// STATE TYPES
// ============================================================================

/// Subscription unique identifier (Poseidon hash of subscription data)
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct SubscriptionId(pub pallas::Base);
impl SubscriptionId {
    pub fn inner(&self) -> pallas::Base { self.0 }
    pub fn to_bytes(&self) -> [u8; 32] { self.0.to_repr() }
    pub fn from_bytes(x: [u8; 32]) -> Option<Self> {
        pallas::Base::from_repr(x).into_option().map(Self)
    }
    pub fn is_zero(&self) -> bool { self.0 == pallas::Base::zero() }
    pub fn zero() -> Self { Self(pallas::Base::zero()) }
}

/// Represents the current state of a subscription
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubscriptionState {
    /// Subscription is active
    Active = 0,
    /// User cancelled, refund available at lock_until_block
    Cancelled = 1,
    /// Subscription expired (lock_until_block reached), refund available
    Expired = 2,
}

impl TryFrom<u8> for SubscriptionState {
    type Error = dwow_sdk::error::ContractError;

    fn try_from(b: u8) -> Result<Self, Self::Error> {
        match b {
            0 => Ok(Self::Active),
            1 => Ok(Self::Cancelled),
            2 => Ok(Self::Expired),
            _ => Err(dwow_sdk::error::ContractError::InvalidFunction),
        }
    }
}

// ============================================================================
// CORE DATA STRUCTURES
// ============================================================================

/// Core subscription data stored on-chain
#[derive(Debug, Clone)]
pub struct Subscription {
    pub version: u8,
    /// Subscription identifier (commitment)
    pub id: SubscriptionId,
    /// Subscriber's public key
    pub subscriber_pubkey: PublicKey,
    /// Plan ID
    pub plan_id: u32,
    /// Block height when subscription expires (time lock)
    pub lock_until_block: u64,
    /// Deposit amount (held in escrow)
    pub deposit: u64,
    /// Token ID
    pub token_id: pallas::Base,
    /// Value commitment (Pedersen)
    pub value_commit: pallas::Point,
    /// Current state
    pub state: SubscriptionState,
    /// Nullifier for the subscription (prevents double-cancel/renew)
    pub spent_nullifier: pallas::Base,
    /// Block height when subscription was created
    pub created_at: u64,
    /// DAO-Escrow bulla (if member of insurance pool)
    pub dao_escrow_bulla: Option<pallas::Base>,
    /// DAO-Escrow membership note (verifies insurance membership)
    pub dao_membership_note: Option<pallas::Base>,
    // ========================================================================
    // Rate Limiting Fields
    // ========================================================================
    /// Total uses allowed in a period (for rate-limited subscriptions)
    pub uses_allowed: u64,
    /// Rate period in blocks (resets after this many blocks)
    pub rate_period: u64,
    /// Accumulated uses in current period
    pub period_uses: u64,
    /// Block height of last access (for rate limiting)
    pub last_access_block: u64,
    /// Remaining uses in current period
    pub uses_remaining: u64,
    pub instance_seed: [u8; 32],
}

impl Subscription {
    /// Derive the subscription ID from subscriber, plan, deposit, token, and lock
    #[allow(dead_code)]
    pub fn derive_id(
        subscriber_pubkey: &PublicKey,
        plan_id: u32,
        deposit: u64,
        token_id: pallas::Base,
        lock_until_block: u64,
        subscriber_secret: pallas::Base,
        plan_nonce: pallas::Base,
    ) -> SubscriptionId {
        let (bx, by) = subscriber_pubkey.xy().expect("pk not identity");
        SubscriptionId(poseidon_hash([
            bx,
            by,
            pallas::Base::from(plan_id as u64),
            pallas::Base::from(deposit),
            token_id,
            pallas::Base::from(lock_until_block),
            subscriber_secret,
            plan_nonce,
        ]))
    }

    /// Compute the nullifier that prevents double-cancel or double-renew
    pub fn compute_nullifier(&self, secret: pallas::Base) -> pallas::Base {
        poseidon_hash([self.id.inner(), secret])
    }
}

/// Subscription plan definition
#[derive(Debug, Clone)]
pub struct Plan {
    pub version: u8,
    /// Plan unique identifier
    pub id: u32,
    /// Plan name (Poseidon hash for privacy)
    pub name_hash: pallas::Base,
    /// Price per period (subscription fee)
    pub price: u64,
    /// Token ID for payment
    pub token_id: pallas::Base,
    /// Duration in blocks
    pub duration_blocks: u64,
    /// DAO treasury share (percentage * 10000, e.g., 1000 = 10%)
    pub treasury_share: u32,
    /// Endowment share (percentage * 10000)
    pub endowment_share: u32,
    /// Whether the plan is active
    pub active: bool,
    /// DAO-Escrow discount (percentage * 10000, e.g., 2000 = 20% off for members)
    pub dao_escrow_discount: u32,
    /// Required DAO-Escrow bulla for discount (optional)
    pub required_dao_escrow: Option<pallas::Base>,
}

/// Capability derived from subscription for access control
/// This implements the Object Capability pattern
#[derive(Debug, Clone)]
pub struct SubscriptionCapability {
    /// Subscriber's public key
    pub subscriber: PublicKey,
    /// Plan ID
    pub plan_id: u32,
    /// Subscription ID (commitment)
    pub subscription_id: SubscriptionId,
    /// Permissions bitmask
    pub permissions: u8,
    /// Expiry block
    pub expires_at: u64,
    /// Nonce for unpredictability
    pub nonce: pallas::Base,
}

impl SubscriptionCapability {
    /// Derive the capability digest for access control
    #[allow(dead_code)]
    pub fn derive_capability(
        subscriber: &PublicKey,
        plan_id: u32,
        subscription_id: SubscriptionId,
        permissions: u8,
        expires_at: u64,
        nonce: pallas::Base,
    ) -> pallas::Base {
        let (bx, by) = subscriber.xy().expect("pk not identity");
        poseidon_hash([
            bx,
            by,
            pallas::Base::from(plan_id as u64),
            subscription_id.inner(),
            pallas::Base::from(permissions as u64),
            pallas::Base::from(expires_at),
            nonce,
        ])
    }
}

// ============================================================================
// PARAMETER TYPES (for contract calls)
// ============================================================================

fn read_base(data: &[u8]) -> Result<pallas::Base, ContractError> { Option::<pallas::Base>::from(pallas::Base::from_repr(data.try_into().unwrap())).ok_or_else(|| ContractError::IoError("invalid base".into())) }

/// Parameters for `Subscription::SubscribeV1`
#[derive(Debug, Clone,)]
pub struct SubscribeParamsV1 {
    /// Plan ID to subscribe to
    pub plan_id: u32,
    /// Subscriber's public key
    pub subscriber_pubkey: PublicKey,
    /// Commitment to subscription parameters
    pub commitment: SubscriptionId,
    /// Value commitment (Pedersen)
    pub value_commit: pallas::Point,
    /// Merkle proof of the commitment
    pub merkle_proof: Vec<pallas::Base>,
    /// Merkle root for the plan
    pub merkle_root: pallas::Base,
    /// DAO-Escrow bulla (optional - for insurance tier discount)
    pub dao_escrow_bulla: Option<pallas::Base>,
    /// DAO-Escrow membership note (verifies valid insurance member)
    pub dao_membership_note: Option<pallas::Base>,
    /// DAO-Escrow Merkle root (for verifying membership note)
    pub dao_escrow_merkle_root: Option<pallas::Base>,
    /// DAO-Escrow membership Merkle proof
    pub dao_merkle_proof: Option<Vec<pallas::Base>>,
    /// DAO-Escrow membership leaf position
    pub dao_leaf_pos: Option<u32>,
    pub instance_seed: [u8; 32],
}

impl dwow_serial::Encodable for SubscribeParamsV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for SubscribeParamsV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }

impl SubscribeParamsV1 { pub fn encode(&self) -> Vec<u8> { let mp_cap = self.merkle_proof.len()*32; let dmp_cap = self.dao_merkle_proof.as_ref().map_or(0,|v| 1+v.len()*32); let mut b = Vec::with_capacity(170+mp_cap+dmp_cap); b.extend_from_slice(&self.plan_id.to_le_bytes()); b.extend_from_slice(&self.subscriber_pubkey.to_bytes()); b.extend_from_slice(&self.commitment.inner().to_repr()); b.extend_from_slice(&self.value_commit.to_bytes()); b.push(self.merkle_proof.len() as u8); for p in &self.merkle_proof { b.extend_from_slice(&p.to_repr()); } b.extend_from_slice(&self.merkle_root.to_repr()); b.push(self.dao_escrow_bulla.is_some() as u8); if let Some(v) = self.dao_escrow_bulla { b.extend_from_slice(&v.to_repr()); } b.push(self.dao_membership_note.is_some() as u8); if let Some(v) = self.dao_membership_note { b.extend_from_slice(&v.to_repr()); } b.push(self.dao_escrow_merkle_root.is_some() as u8); if let Some(v) = self.dao_escrow_merkle_root { b.extend_from_slice(&v.to_repr()); } b.push(self.dao_merkle_proof.is_some() as u8); if let Some(ref v) = self.dao_merkle_proof { b.push(v.len() as u8); for p in v { b.extend_from_slice(&p.to_repr()); } } b.push(self.dao_leaf_pos.is_some() as u8); if let Some(v) = self.dao_leaf_pos { b.extend_from_slice(&v.to_le_bytes()); } b.extend_from_slice(&self.instance_seed); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() < 170 { return Err(ContractError::IoError("SubscribeParamsV1: too short".into())); } let plan_id = u32::from_le_bytes(data[0..4].try_into().unwrap()); let subscriber_pubkey = PublicKey::from_bytes(data[4..36].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("SubscribeParamsV1: invalid subscriber_pubkey: {}", e)))?; let commitment = SubscriptionId(read_base(&data[36..68])?); let value_commit = Option::<pallas::Point>::from(pallas::Point::from_bytes(data[68..100].try_into().unwrap())).ok_or_else(|| ContractError::IoError("SubscribeParamsV1: invalid value_commit".into()))?; let mp_count = data[100] as usize; let mut pos = 101+mp_count*32; if data.len() < pos { return Err(ContractError::IoError("SubscribeParamsV1: merkle_proof truncated".into())); } let mut merkle_proof = Vec::with_capacity(mp_count); for i in 0..mp_count { merkle_proof.push(read_base(&data[101+i*32..101+(i+1)*32])?); } let merkle_root = read_base(&data[pos-32..pos])?; let mut read_opt = |pos: &mut usize| -> Option<pallas::Base> { if data[*pos] != 0 { *pos += 1; let v = read_base(&data[*pos..*pos+32]).ok()?; *pos += 32; Some(v) } else { *pos += 1; None } }; let mut read_opt_u32 = |pos: &mut usize| -> Option<u32> { if data[*pos] != 0 { let v = u32::from_le_bytes(data[*pos+1..*pos+5].try_into().unwrap()); *pos += 5; Some(v) } else { *pos += 1; None } }; let has_dao_mp = data[pos] != 0; pos += 1; let dao_merkle_proof = if has_dao_mp { let c = data[pos] as usize; pos += 1; let mut v = Vec::with_capacity(c); for i in 0..c { v.push(read_base(&data[pos..pos+32])?); pos += 32; } Some(v) } else { None }; Ok(SubscribeParamsV1 { plan_id, subscriber_pubkey, commitment, value_commit, merkle_proof, merkle_root, dao_escrow_bulla: read_opt(&mut pos), dao_membership_note: read_opt(&mut pos), dao_escrow_merkle_root: read_opt(&mut pos), dao_merkle_proof, dao_leaf_pos: read_opt_u32(&mut pos), instance_seed: data[pos..pos+32].try_into().unwrap() }) } }

/// State update for `Subscription::SubscribeV1`
#[derive(Debug, Clone)]
pub struct SubscribeUpdateV1 {
    /// The full subscription object
    pub subscription: Subscription,
}

/// Parameters for `Subscription::CancelV1`
#[derive(Debug, Clone,)]
pub struct CancelParamsV1 {
    /// Subscription ID
    pub subscription_id: SubscriptionId,
    /// Subscriber's secret (proves ownership)
    pub subscriber_secret: pallas::Base,
    /// Nullifier revealing the subscription is cancelled
    pub spent_nullifier: pallas::Base,
    /// Current block height
    pub current_block: u64,
    /// Recipient public key for refund
    pub recipient_pubkey: PublicKey,
}

impl dwow_serial::Encodable for CancelParamsV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for CancelParamsV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }

impl CancelParamsV1 { pub const ENCODED_SIZE: usize = 136; pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(136); b.extend_from_slice(&self.subscription_id.inner().to_repr()); b.extend_from_slice(&self.subscriber_secret.to_repr()); b.extend_from_slice(&self.spent_nullifier.to_repr()); b.extend_from_slice(&self.current_block.to_le_bytes()); b.extend_from_slice(&self.recipient_pubkey.to_bytes()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 136 { return Err(ContractError::IoError(format!("CancelParamsV1: expected 136 bytes, got {}", data.len()))); } Ok(CancelParamsV1 { subscription_id: SubscriptionId(read_base(&data[0..32])?), subscriber_secret: read_base(&data[32..64])?, spent_nullifier: read_base(&data[64..96])?, current_block: u64::from_le_bytes(data[96..104].try_into().unwrap()), recipient_pubkey: PublicKey::from_bytes(data[104..136].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("CancelParamsV1: invalid recipient_pubkey: {}", e)))? }) } }

/// State update for `Subscription::CancelV1`
#[derive(Debug, Clone)]
pub struct CancelUpdateV1 {
    /// The cancelled subscription ID
    pub subscription_id: SubscriptionId,
    /// Nullifier for the cancelled subscription
    pub spent_nullifier: pallas::Base,
    /// The updated subscription with Cancelled state
    pub updated_subscription: Subscription,
}

/// Parameters for `Subscription::RenewV1`
#[derive(Debug, Clone,)]
pub struct RenewParamsV1 {
    /// Existing subscription ID
    pub subscription_id: SubscriptionId,
    /// Subscriber's secret (proves ownership)
    pub subscriber_secret: pallas::Base,
    /// New lock_until_block
    pub new_lock_until_block: u64,
    /// Nullifier for the old subscription
    pub spent_nullifier: pallas::Base,
    /// Value commitment for renewal payment
    pub value_commit: pallas::Point,
    /// Merkle proof of the commitment
    pub merkle_proof: Vec<pallas::Base>,
}

impl dwow_serial::Encodable for RenewParamsV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for RenewParamsV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }

impl RenewParamsV1 { pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(137+self.merkle_proof.len()*32); b.extend_from_slice(&self.subscription_id.inner().to_repr()); b.extend_from_slice(&self.subscriber_secret.to_repr()); b.extend_from_slice(&self.new_lock_until_block.to_le_bytes()); b.extend_from_slice(&self.spent_nullifier.to_repr()); b.extend_from_slice(&self.value_commit.to_bytes()); b.push(self.merkle_proof.len() as u8); for p in &self.merkle_proof { b.extend_from_slice(&p.to_repr()); } b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() < 137 { return Err(ContractError::IoError("RenewParamsV1: too short".into())); } let subscription_id = SubscriptionId(read_base(&data[0..32])?); let subscriber_secret = read_base(&data[32..64])?; let new_lock_until_block = u64::from_le_bytes(data[64..72].try_into().unwrap()); let spent_nullifier = read_base(&data[72..104])?; let value_commit = Option::<pallas::Point>::from(pallas::Point::from_bytes(data[104..136].try_into().unwrap())).ok_or_else(|| ContractError::IoError("RenewParamsV1: invalid value_commit".into()))?; let mp_count = data[136] as usize; if data.len() != 137+mp_count*32 { return Err(ContractError::IoError(format!("RenewParamsV1: expected {} bytes, got {}", 137+mp_count*32, data.len()))); } let mut merkle_proof = Vec::with_capacity(mp_count); for i in 0..mp_count { merkle_proof.push(read_base(&data[137+i*32..137+(i+1)*32])?); } Ok(RenewParamsV1 { subscription_id, subscriber_secret, new_lock_until_block, spent_nullifier, value_commit, merkle_proof }) } }

/// State update for `Subscription::RenewV1`
#[derive(Debug, Clone)]
pub struct RenewUpdateV1 {
    /// The renewed subscription ID (new commitment)
    pub subscription_id: SubscriptionId,
    /// Nullifier for the old subscription
    pub spent_nullifier: pallas::Base,
    /// The new subscription object
    pub new_subscription: Subscription,
}

/// Parameters for `Subscription::VerifyAccessV1`
#[derive(Debug, Clone,)]
pub struct VerifyAccessParamsV1 {
    /// Subscription ID being verified
    pub subscription_id: SubscriptionId,
    /// Capability derived from subscription
    pub capability: pallas::Base,
    /// Nonce for the proof
    pub nonce: pallas::Base,
}

impl dwow_serial::Encodable for VerifyAccessParamsV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for VerifyAccessParamsV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }

impl VerifyAccessParamsV1 { pub const ENCODED_SIZE: usize = 96; pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(96); b.extend_from_slice(&self.subscription_id.inner().to_repr()); b.extend_from_slice(&self.capability.to_repr()); b.extend_from_slice(&self.nonce.to_repr()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 96 { return Err(ContractError::IoError(format!("VerifyAccessParamsV1: expected 96 bytes, got {}", data.len()))); } Ok(VerifyAccessParamsV1 { subscription_id: SubscriptionId(read_base(&data[0..32])?), capability: read_base(&data[32..64])?, nonce: read_base(&data[64..96])? }) } }

/// Parameters for `Subscription::UpdateUsageV1`
#[derive(Debug, Clone,)]
pub struct UpdateUsageParamsV1 {
    /// Subscription ID
    pub subscription_id: SubscriptionId,
    /// Subscriber's public key x-coordinate
    pub subscriber_pub_x: pallas::Base,
    /// Subscriber's public key y-coordinate
    pub subscriber_pub_y: pallas::Base,
    /// Subscriber's secret (proves ownership)
    pub subscriber_secret: pallas::Base,
    /// Current block height (usage_timestamp in ZK circuit)
    pub current_block: u64,
    /// Nonce for ZK circuit witness
    pub nonce: pallas::Base,
    /// Spent nullifier for this subscription
    pub spent_nullifier: pallas::Base,
    /// Merkle proof of the subscription state
    pub merkle_proof: Vec<pallas::Base>,
}

impl dwow_serial::Encodable for UpdateUsageParamsV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for UpdateUsageParamsV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }

impl UpdateUsageParamsV1 { pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(201+self.merkle_proof.len()*32); b.extend_from_slice(&self.subscription_id.inner().to_repr()); b.extend_from_slice(&self.subscriber_pub_x.to_repr()); b.extend_from_slice(&self.subscriber_pub_y.to_repr()); b.extend_from_slice(&self.subscriber_secret.to_repr()); b.extend_from_slice(&self.current_block.to_le_bytes()); b.extend_from_slice(&self.nonce.to_repr()); b.extend_from_slice(&self.spent_nullifier.to_repr()); b.push(self.merkle_proof.len() as u8); for p in &self.merkle_proof { b.extend_from_slice(&p.to_repr()); } b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() < 201 { return Err(ContractError::IoError("UpdateUsageParamsV1: too short".into())); } let subscription_id = SubscriptionId(read_base(&data[0..32])?); let subscriber_pub_x = read_base(&data[32..64])?; let subscriber_pub_y = read_base(&data[64..96])?; let subscriber_secret = read_base(&data[96..128])?; let current_block = u64::from_le_bytes(data[128..136].try_into().unwrap()); let nonce = read_base(&data[136..168])?; let spent_nullifier = read_base(&data[168..200])?; let mp_count = data[200] as usize; if data.len() != 201+mp_count*32 { return Err(ContractError::IoError(format!("UpdateUsageParamsV1: expected {} bytes, got {}", 201+mp_count*32, data.len()))); } let mut merkle_proof = Vec::with_capacity(mp_count); for i in 0..mp_count { merkle_proof.push(read_base(&data[201+i*32..201+(i+1)*32])?); } Ok(UpdateUsageParamsV1 { subscription_id, subscriber_pub_x, subscriber_pub_y, subscriber_secret, current_block, nonce, spent_nullifier, merkle_proof }) } }

/// State update for `Subscription::UpdateUsageV1`
#[derive(Debug, Clone)]
pub struct UpdateUsageUpdateV1 {
    /// The subscription ID
    pub subscription_id: SubscriptionId,
    /// Updated period uses
    pub period_uses: u64,
    /// Updated last access block
    pub last_access_block: u64,
    /// Updated uses remaining
    pub uses_remaining: u64,
    /// Whether this was a period reset
    pub is_new_period: bool,
}

/// Parameters for `Subscription::DaoControlV1`
#[derive(Debug, Clone,)]
pub enum DaoControlParamsV1 {
    /// Update plan parameters
    UpdatePlan(Plan),
    /// Activate/deactivate a plan
    SetPlanActive { plan_id: u32, active: bool },
    /// Emergency pause all subscriptions
    EmergencyPause { pause: bool, reason: pallas::Base },
    /// Withdraw from endowment fund
    EndowmentWithdraw { amount: u64, recipient: PublicKey },
    /// Slash a subscription (governance enforcement)
    Slash { subscription_id: SubscriptionId, reason: pallas::Base },
}

impl dwow_serial::Encodable for DaoControlParamsV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for DaoControlParamsV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }

impl DaoControlParamsV1 {
    pub fn encode(&self) -> Vec<u8> {
        match self {
            Self::UpdatePlan(plan) => { let inner = plan.encode(); let mut b = Vec::with_capacity(1+inner.len()); b.push(0u8); b.extend_from_slice(&inner); b }
            Self::SetPlanActive { plan_id, active } => { let mut b = Vec::with_capacity(6); b.push(1u8); b.extend_from_slice(&plan_id.to_le_bytes()); b.push(*active as u8); b }
            Self::EmergencyPause { pause, reason } => { let mut b = Vec::with_capacity(34); b.push(2u8); b.push(*pause as u8); b.extend_from_slice(&reason.to_repr()); b }
            Self::EndowmentWithdraw { amount, recipient } => { let mut b = Vec::with_capacity(41); b.push(3u8); b.extend_from_slice(&amount.to_le_bytes()); b.extend_from_slice(&recipient.to_bytes()); b }
            Self::Slash { subscription_id, reason } => { let mut b = Vec::with_capacity(65); b.push(4u8); b.extend_from_slice(&subscription_id.inner().to_repr()); b.extend_from_slice(&reason.to_repr()); b }
        }
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.is_empty() { return Err(ContractError::IoError("DaoControlParamsV1: empty".into())); }
        match data[0] {
            0 => { let plan = Plan::decode(&data[1..])?; Ok(Self::UpdatePlan(plan)) }
            1 => { if data.len() < 6 { return Err(ContractError::IoError("DaoControlParamsV1: SetPlanActive truncated".into())); } Ok(Self::SetPlanActive { plan_id: u32::from_le_bytes(data[1..5].try_into().unwrap()), active: data[5] != 0 }) }
            2 => { if data.len() < 34 { return Err(ContractError::IoError("DaoControlParamsV1: EmergencyPause truncated".into())); } Ok(Self::EmergencyPause { pause: data[1] != 0, reason: Option::<pallas::Base>::from(pallas::Base::from_repr(data[2..34].try_into().unwrap())).ok_or_else(|| ContractError::IoError("DaoControlParamsV1: invalid reason".into()))? }) }
            3 => { if data.len() < 41 { return Err(ContractError::IoError("DaoControlParamsV1: EndowmentWithdraw truncated".into())); } Ok(Self::EndowmentWithdraw { amount: u64::from_le_bytes(data[1..9].try_into().unwrap()), recipient: PublicKey::from_bytes(data[9..41].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("DaoControlParamsV1: invalid recipient: {}", e)))? }) }
            4 => { if data.len() < 65 { return Err(ContractError::IoError("DaoControlParamsV1: Slash truncated".into())); } Ok(Self::Slash { subscription_id: SubscriptionId(Option::<pallas::Base>::from(pallas::Base::from_repr(data[1..33].try_into().unwrap())).ok_or_else(|| ContractError::IoError("DaoControlParamsV1: invalid subscription_id".into()))?), reason: Option::<pallas::Base>::from(pallas::Base::from_repr(data[33..65].try_into().unwrap())).ok_or_else(|| ContractError::IoError("DaoControlParamsV1: invalid reason".into()))? }) }
            _ => Err(ContractError::InvalidFunction),
        }
    }
}

/// State update for `Subscription::DaoControlV1`
#[derive(Debug, Clone)]
pub struct DaoControlUpdateV1 {
    /// The DAO action performed
    pub action: DaoControlAction,
}

/// Actions resulting from DAO control
#[derive(Debug, Clone)]
pub enum DaoControlAction {
    /// Plan was updated
    PlanUpdated(u32),
    /// Plan active status changed
    PlanStatusChanged { plan_id: u32, active: bool },
    /// Emergency pause toggled
    EmergencyPauseToggled(bool),
    /// Endowment withdrawn
    EndowmentWithdrawn { amount: u64, recipient: PublicKey },
    /// Subscription slashed
    SubscriptionSlashed(SubscriptionId),
}

// ============================================================================
// RHO-CALCULUS EXPLICIT ENCODE/DECODE
//
// Replace SerialEncodable/SerialDecodable derives with explicit deterministic
// encoding. This eliminates the VarInt length-prefix anti-pattern and gives
// full control over byte layout.
// ============================================================================

impl SubscriptionId {
    /// Decode from canonical bytes (ρ-calculus: eval).
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 32 {
            return Err(ContractError::IoError(format!(
                "SubscriptionId: expected 32 bytes, got {}",
                data.len()
            )));
        }
        Self::from_bytes(data.try_into().unwrap())
            .ok_or_else(|| {
                ContractError::IoError(
                    "SubscriptionId: invalid field element".into(),
                )
            })
    }

    /// Encode to canonical bytes (ρ-calculus: quote).
    pub fn encode(&self) -> Vec<u8> { self.to_bytes().to_vec() }
}

impl SubscriptionState {
    /// Encode to canonical bytes (ρ-calculus: quote).
    pub fn encode(&self) -> Vec<u8> {
        vec![*self as u8]
    }

    /// Decode from canonical bytes (ρ-calculus: eval).
    /// Uses Pattern 5: TryFrom<u8> for enum discriminant.
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.is_empty() {
            return Err(ContractError::IoError(
                "SubscriptionState: empty data".into(),
            ));
        }
        Self::try_from(data[0])
    }
}

impl Subscription {
    /// Minimum canonical byte size (without optional fields present).
    pub const MIN_ENCODED_SIZE: usize = 264;

    /// Encode to canonical bytes (ρ-calculus: quote).
    pub fn encode(&self) -> Vec<u8> {
        let cap = Self::MIN_ENCODED_SIZE
            + if self.dao_escrow_bulla.is_some() { 32 } else { 0 }
            + if self.dao_membership_note.is_some() { 32 } else { 0 };
        let mut b = Vec::with_capacity(cap);
        b.push(self.version);
        b.extend_from_slice(&self.id.to_bytes());
        b.extend_from_slice(&self.subscriber_pubkey.to_bytes());
        b.extend_from_slice(&self.plan_id.to_le_bytes());
        b.extend_from_slice(&self.lock_until_block.to_le_bytes());
        b.extend_from_slice(&self.deposit.to_le_bytes());
        b.extend_from_slice(&self.token_id.to_repr());
        b.extend_from_slice(&self.value_commit.to_bytes());
        b.push(self.state as u8);
        b.extend_from_slice(&self.spent_nullifier.to_repr());
        b.extend_from_slice(&self.created_at.to_le_bytes());
        // Option fields (Pattern 4: presence byte + conditional value)
        b.push(self.dao_escrow_bulla.is_some() as u8);
        if let Some(ref v) = self.dao_escrow_bulla {
            b.extend_from_slice(&v.to_repr());
        }
        b.push(self.dao_membership_note.is_some() as u8);
        if let Some(ref v) = self.dao_membership_note {
            b.extend_from_slice(&v.to_repr());
        }
        // Rate limiting fields
        b.extend_from_slice(&self.uses_allowed.to_le_bytes());
        b.extend_from_slice(&self.rate_period.to_le_bytes());
        b.extend_from_slice(&self.period_uses.to_le_bytes());
        b.extend_from_slice(&self.last_access_block.to_le_bytes());
        b.extend_from_slice(&self.uses_remaining.to_le_bytes());
        b.extend_from_slice(&self.instance_seed);
        b
    }

    /// Decode from canonical bytes (ρ-calculus: eval).
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < Self::MIN_ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "Subscription: expected at least {} bytes, got {}",
                Self::MIN_ENCODED_SIZE,
                data.len()
            )));
        }
        let mut pos = 0;
        let version = data[pos];
        pos += 1;
        let id = SubscriptionId::decode(&data[pos..pos + 32])?;
        pos += 32;
        let subscriber_pubkey =
            PublicKey::from_bytes(data[pos..pos + 32].try_into().unwrap())
                .map_err(|e| {
                    ContractError::IoError(format!(
                        "Subscription: invalid subscriber_pubkey: {}",
                        e
                    ))
                })?;
        pos += 32;
        let plan_id =
            u32::from_le_bytes(data[pos..pos + 4].try_into().unwrap());
        pos += 4;
        let lock_until_block =
            u64::from_le_bytes(data[pos..pos + 8].try_into().unwrap());
        pos += 8;
        let deposit =
            u64::from_le_bytes(data[pos..pos + 8].try_into().unwrap());
        pos += 8;
        let token_id =
            Option::<pallas::Base>::from(pallas::Base::from_repr(
                data[pos..pos + 32].try_into().unwrap(),
            ))
            .ok_or_else(|| {
                ContractError::IoError(
                    "Subscription: invalid token_id".into(),
                )
            })?;
        pos += 32;
        let value_commit =
            Option::<pallas::Point>::from(pallas::Point::from_bytes(
                &data[pos..pos + 32].try_into().unwrap(),
            ))
            .ok_or_else(|| {
                ContractError::IoError(
                    "Subscription: invalid value_commit".into(),
                )
            })?;
        pos += 32;
        let state = SubscriptionState::try_from(data[pos])?;
        pos += 1;
        let spent_nullifier =
            Option::<pallas::Base>::from(pallas::Base::from_repr(
                data[pos..pos + 32].try_into().unwrap(),
            ))
            .ok_or_else(|| {
                ContractError::IoError(
                    "Subscription: invalid spent_nullifier".into(),
                )
            })?;
        pos += 32;
        let created_at =
            u64::from_le_bytes(data[pos..pos + 8].try_into().unwrap());
        pos += 8;
        // Option fields (Pattern 4: presence byte + conditional position tracking)
        let has_bulla = data[pos] != 0;
        pos += 1;
        let dao_escrow_bulla = if has_bulla {
            let v = Option::<pallas::Base>::from(pallas::Base::from_repr(
                data[pos..pos + 32].try_into().unwrap(),
            ))
            .ok_or_else(|| {
                ContractError::IoError(
                    "Subscription: invalid dao_escrow_bulla".into(),
                )
            })?;
            pos += 32;
            Some(v)
        } else {
            None
        };
        let has_note = data[pos] != 0;
        pos += 1;
        let dao_membership_note = if has_note {
            let v = Option::<pallas::Base>::from(pallas::Base::from_repr(
                data[pos..pos + 32].try_into().unwrap(),
            ))
            .ok_or_else(|| {
                ContractError::IoError(
                    "Subscription: invalid dao_membership_note".into(),
                )
            })?;
            pos += 32;
            Some(v)
        } else {
            None
        };
        // Rate limiting fields
        let uses_allowed =
            u64::from_le_bytes(data[pos..pos + 8].try_into().unwrap());
        pos += 8;
        let rate_period =
            u64::from_le_bytes(data[pos..pos + 8].try_into().unwrap());
        pos += 8;
        let period_uses =
            u64::from_le_bytes(data[pos..pos + 8].try_into().unwrap());
        pos += 8;
        let last_access_block =
            u64::from_le_bytes(data[pos..pos + 8].try_into().unwrap());
        pos += 8;
        let uses_remaining =
            u64::from_le_bytes(data[pos..pos + 8].try_into().unwrap());
        pos += 8;
        let instance_seed: [u8; 32] =
            data[pos..pos + 32].try_into().unwrap();

        Ok(Subscription {
            version,
            id,
            subscriber_pubkey,
            plan_id,
            lock_until_block,
            deposit,
            token_id,
            value_commit,
            state,
            spent_nullifier,
            created_at,
            dao_escrow_bulla,
            dao_membership_note,
            uses_allowed,
            rate_period,
            period_uses,
            last_access_block,
            uses_remaining,
            instance_seed,
        })
    }
}

impl Plan {
    /// Minimum canonical byte size (without optional fields present).
    pub const MIN_ENCODED_SIZE: usize = 99;

    /// Decode from canonical bytes (ρ-calculus: eval).
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < Self::MIN_ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "Plan: expected at least {} bytes, got {}",
                Self::MIN_ENCODED_SIZE,
                data.len()
            )));
        }
        let mut pos = 0;
        let version = data[pos];
        pos += 1;
        let id =
            u32::from_le_bytes(data[pos..pos + 4].try_into().unwrap());
        pos += 4;
        let name_hash =
            Option::<pallas::Base>::from(pallas::Base::from_repr(
                data[pos..pos + 32].try_into().unwrap(),
            ))
            .ok_or_else(|| {
                ContractError::IoError("Plan: invalid name_hash".into())
            })?;
        pos += 32;
        let price =
            u64::from_le_bytes(data[pos..pos + 8].try_into().unwrap());
        pos += 8;
        let token_id =
            Option::<pallas::Base>::from(pallas::Base::from_repr(
                data[pos..pos + 32].try_into().unwrap(),
            ))
            .ok_or_else(|| {
                ContractError::IoError("Plan: invalid token_id".into())
            })?;
        pos += 32;
        let duration_blocks =
            u64::from_le_bytes(data[pos..pos + 8].try_into().unwrap());
        pos += 8;
        let treasury_share =
            u32::from_le_bytes(data[pos..pos + 4].try_into().unwrap());
        pos += 4;
        let endowment_share =
            u32::from_le_bytes(data[pos..pos + 4].try_into().unwrap());
        pos += 4;
        let active = data[pos] != 0;
        pos += 1;
        let dao_escrow_discount =
            u32::from_le_bytes(data[pos..pos + 4].try_into().unwrap());
        pos += 4;
        let has_req = data[pos] != 0;
        pos += 1;
        let required_dao_escrow = if has_req {
            let v =
                Option::<pallas::Base>::from(pallas::Base::from_repr(
                    data[pos..pos + 32].try_into().unwrap(),
                ))
                .ok_or_else(|| {
                    ContractError::IoError(
                        "Plan: invalid required_dao_escrow".into(),
                    )
                })?;
            pos += 32;
            Some(v)
        } else {
            None
        };

        Ok(Plan {
            version,
            id,
            name_hash,
            price,
            token_id,
            duration_blocks,
            treasury_share,
            endowment_share,
            active,
            dao_escrow_discount,
            required_dao_escrow,
        })
    }

    /// Encode to canonical bytes (ρ-calculus: quote).
    /// Fixed 99 + 32 optional = 131 max.
    pub fn encode(&self) -> Vec<u8> {
        let cap = if self.required_dao_escrow.is_some() { 131 } else { 99 };
        let mut b = Vec::with_capacity(cap);
        b.push(self.version);
        b.extend_from_slice(&self.id.to_le_bytes());
        b.extend_from_slice(&self.name_hash.to_repr());
        b.extend_from_slice(&self.price.to_le_bytes());
        b.extend_from_slice(&self.token_id.to_repr());
        b.extend_from_slice(&self.duration_blocks.to_le_bytes());
        b.extend_from_slice(&self.treasury_share.to_le_bytes());
        b.extend_from_slice(&self.endowment_share.to_le_bytes());
        b.push(self.active as u8);
        b.extend_from_slice(&self.dao_escrow_discount.to_le_bytes());
        b.push(self.required_dao_escrow.is_some() as u8);
        if let Some(ref v) = self.required_dao_escrow {
            b.extend_from_slice(&v.to_repr());
        }
        b
    }
}

impl SubscriptionCapability {
    /// Fixed canonical byte size: subscriber(32) + plan_id(4) +
    /// subscription_id(32) + permissions(1) + expires_at(8) + nonce(32) = 109
    pub const ENCODED_SIZE: usize = 109;

    /// Encode to canonical bytes (ρ-calculus: quote).
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(Self::ENCODED_SIZE);
        b.extend_from_slice(&self.subscriber.to_bytes());
        b.extend_from_slice(&self.plan_id.to_le_bytes());
        b.extend_from_slice(&self.subscription_id.to_bytes());
        b.push(self.permissions);
        b.extend_from_slice(&self.expires_at.to_le_bytes());
        b.extend_from_slice(&self.nonce.to_repr());
        b
    }

    /// Decode from canonical bytes (ρ-calculus: eval).
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "SubscriptionCapability: expected {} bytes, got {}",
                Self::ENCODED_SIZE,
                data.len()
            )));
        }
        let subscriber =
            PublicKey::from_bytes(data[0..32].try_into().unwrap())
                .map_err(|e| {
                    ContractError::IoError(format!(
                        "SubscriptionCapability: invalid subscriber: {}",
                        e
                    ))
                })?;
        let plan_id =
            u32::from_le_bytes(data[32..36].try_into().unwrap());
        let subscription_id = SubscriptionId::decode(&data[36..68])?;
        let permissions = data[68];
        let expires_at =
            u64::from_le_bytes(data[69..77].try_into().unwrap());
        let nonce =
            Option::<pallas::Base>::from(pallas::Base::from_repr(
                data[77..109].try_into().unwrap(),
            ))
            .ok_or_else(|| {
                ContractError::IoError(
                    "SubscriptionCapability: invalid nonce".into(),
                )
            })?;

        Ok(SubscriptionCapability {
            subscriber,
            plan_id,
            subscription_id,
            permissions,
            expires_at,
            nonce,
        })
    }
}

impl dwow_serial::Encodable for SubscribeUpdateV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for SubscribeUpdateV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }

impl SubscribeUpdateV1 {
    /// Encode to canonical bytes (ρ-calculus: quote).
    pub fn encode(&self) -> Vec<u8> {
        self.subscription.encode()
    }

    /// Decode from canonical bytes (ρ-calculus: eval).
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        Ok(SubscribeUpdateV1 {
            subscription: Subscription::decode(data)?,
        })
    }
}

impl dwow_serial::Encodable for CancelUpdateV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for CancelUpdateV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }

impl CancelUpdateV1 {
    /// Encode to canonical bytes (ρ-calculus: quote).
    pub fn encode(&self) -> Vec<u8> {
        let mut b =
            Vec::with_capacity(64 + Subscription::MIN_ENCODED_SIZE);
        b.extend_from_slice(&self.subscription_id.to_bytes());
        b.extend_from_slice(&self.spent_nullifier.to_repr());
        b.extend_from_slice(&self.updated_subscription.encode());
        b
    }

    /// Decode from canonical bytes (ρ-calculus: eval).
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 64 {
            return Err(ContractError::IoError(format!(
                "CancelUpdateV1: expected at least 64 bytes, got {}",
                data.len()
            )));
        }
        let subscription_id = SubscriptionId::decode(&data[0..32])?;
        let spent_nullifier =
            Option::<pallas::Base>::from(pallas::Base::from_repr(
                data[32..64].try_into().unwrap(),
            ))
            .ok_or_else(|| {
                ContractError::IoError(
                    "CancelUpdateV1: invalid spent_nullifier".into(),
                )
            })?;
        let updated_subscription = Subscription::decode(&data[64..])?;
        Ok(CancelUpdateV1 {
            subscription_id,
            spent_nullifier,
            updated_subscription,
        })
    }
}

impl dwow_serial::Encodable for RenewUpdateV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for RenewUpdateV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }

impl RenewUpdateV1 {
    /// Encode to canonical bytes (ρ-calculus: quote).
    pub fn encode(&self) -> Vec<u8> {
        let mut b =
            Vec::with_capacity(64 + Subscription::MIN_ENCODED_SIZE);
        b.extend_from_slice(&self.subscription_id.to_bytes());
        b.extend_from_slice(&self.spent_nullifier.to_repr());
        b.extend_from_slice(&self.new_subscription.encode());
        b
    }

    /// Decode from canonical bytes (ρ-calculus: eval).
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 64 {
            return Err(ContractError::IoError(format!(
                "RenewUpdateV1: expected at least 64 bytes, got {}",
                data.len()
            )));
        }
        let subscription_id = SubscriptionId::decode(&data[0..32])?;
        let spent_nullifier =
            Option::<pallas::Base>::from(pallas::Base::from_repr(
                data[32..64].try_into().unwrap(),
            ))
            .ok_or_else(|| {
                ContractError::IoError(
                    "RenewUpdateV1: invalid spent_nullifier".into(),
                )
            })?;
        let new_subscription = Subscription::decode(&data[64..])?;
        Ok(RenewUpdateV1 {
            subscription_id,
            spent_nullifier,
            new_subscription,
        })
    }
}

impl dwow_serial::Encodable for UpdateUsageUpdateV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for UpdateUsageUpdateV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }

impl UpdateUsageUpdateV1 {
    /// Fixed canonical byte size: subscription_id(32) + period_uses(8) +
    /// last_access_block(8) + uses_remaining(8) + is_new_period(1) = 57
    pub const ENCODED_SIZE: usize = 57;

    /// Encode to canonical bytes (ρ-calculus: quote).
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(Self::ENCODED_SIZE);
        b.extend_from_slice(&self.subscription_id.to_bytes());
        b.extend_from_slice(&self.period_uses.to_le_bytes());
        b.extend_from_slice(&self.last_access_block.to_le_bytes());
        b.extend_from_slice(&self.uses_remaining.to_le_bytes());
        b.push(self.is_new_period as u8);
        b
    }

    /// Decode from canonical bytes (ρ-calculus: eval).
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "UpdateUsageUpdateV1: expected {} bytes, got {}",
                Self::ENCODED_SIZE,
                data.len()
            )));
        }
        let subscription_id = SubscriptionId::decode(&data[0..32])?;
        let period_uses =
            u64::from_le_bytes(data[32..40].try_into().unwrap());
        let last_access_block =
            u64::from_le_bytes(data[40..48].try_into().unwrap());
        let uses_remaining =
            u64::from_le_bytes(data[48..56].try_into().unwrap());
        let is_new_period = data[56] != 0;

        Ok(UpdateUsageUpdateV1 {
            subscription_id,
            period_uses,
            last_access_block,
            uses_remaining,
            is_new_period,
        })
    }
}

impl dwow_serial::Encodable for DaoControlUpdateV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for DaoControlUpdateV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }

impl DaoControlUpdateV1 {
    /// Encode to canonical bytes (ρ-calculus: quote).
    pub fn encode(&self) -> Vec<u8> {
        self.action.encode()
    }

    /// Decode from canonical bytes (ρ-calculus: eval).
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        Ok(DaoControlUpdateV1 {
            action: DaoControlAction::decode(data)?,
        })
    }
}

impl DaoControlAction {
    /// Encode to canonical bytes (ρ-calculus: quote).
    /// Pattern 5: discriminant byte + variant-specific data.
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::new();
        match self {
            Self::PlanUpdated(plan_id) => {
                b.push(0u8);
                b.extend_from_slice(&plan_id.to_le_bytes());
            }
            Self::PlanStatusChanged { plan_id, active } => {
                b.push(1u8);
                b.extend_from_slice(&plan_id.to_le_bytes());
                b.push(*active as u8);
            }
            Self::EmergencyPauseToggled(pause) => {
                b.push(2u8);
                b.push(*pause as u8);
            }
            Self::EndowmentWithdrawn { amount, recipient } => {
                b.push(3u8);
                b.extend_from_slice(&amount.to_le_bytes());
                b.extend_from_slice(&recipient.to_bytes());
            }
            Self::SubscriptionSlashed(subscription_id) => {
                b.push(4u8);
                b.extend_from_slice(&subscription_id.to_bytes());
            }
        }
        b
    }

    /// Decode from canonical bytes (ρ-calculus: eval).
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.is_empty() {
            return Err(ContractError::IoError(
                "DaoControlAction: empty data".into(),
            ));
        }
        let discriminant = data[0];
        match discriminant {
            0 => {
                if data.len() < 5 {
                    return Err(ContractError::IoError(
                        "DaoControlAction::PlanUpdated: expected 5 bytes"
                            .into(),
                    ));
                }
                let plan_id =
                    u32::from_le_bytes(data[1..5].try_into().unwrap());
                Ok(Self::PlanUpdated(plan_id))
            }
            1 => {
                if data.len() < 6 {
                    return Err(ContractError::IoError(
                        "DaoControlAction::PlanStatusChanged: expected 6 bytes"
                            .into(),
                    ));
                }
                let plan_id =
                    u32::from_le_bytes(data[1..5].try_into().unwrap());
                let active = data[5] != 0;
                Ok(Self::PlanStatusChanged { plan_id, active })
            }
            2 => {
                if data.len() < 2 {
                    return Err(ContractError::IoError(
                        "DaoControlAction::EmergencyPauseToggled: expected 2 bytes"
                            .into(),
                    ));
                }
                Ok(Self::EmergencyPauseToggled(data[1] != 0))
            }
            3 => {
                if data.len() < 41 {
                    return Err(ContractError::IoError(
                        "DaoControlAction::EndowmentWithdrawn: expected 41 bytes"
                            .into(),
                    ));
                }
                let amount =
                    u64::from_le_bytes(data[1..9].try_into().unwrap());
                let recipient = PublicKey::from_bytes(
                    data[9..41].try_into().unwrap(),
                )
                .map_err(|e| {
                    ContractError::IoError(format!(
                        "DaoControlAction: invalid recipient: {}",
                        e
                    ))
                })?;
                Ok(Self::EndowmentWithdrawn { amount, recipient })
            }
            4 => {
                if data.len() < 33 {
                    return Err(ContractError::IoError(
                        "DaoControlAction::SubscriptionSlashed: expected 33 bytes"
                            .into(),
                    ));
                }
                let subscription_id =
                    SubscriptionId::decode(&data[1..33])?;
                Ok(Self::SubscriptionSlashed(subscription_id))
            }
            _ => Err(ContractError::IoError(format!(
                "DaoControlAction: unknown discriminant {}",
                discriminant
            ))),
        }
    }
}

// ============================================================================
// ACCESS CONTROL CONSTANTS
// ============================================================================

/// Permission bitmask for subscription capabilities
pub mod permissions {
    /// Read access - can verify membership
    pub const READ: u8 = 0b0000_0001;
    /// Write access - can modify subscription
    pub const WRITE: u8 = 0b0000_0010;
    /// Cancel access - can cancel subscription
    pub const CANCEL: u8 = 0b0000_0100;
    /// Renew access - can renew subscription
    pub const RENEW: u8 = 0b0000_1000;
    /// Admin access - DAO governance
    pub const ADMIN: u8 = 0b1000_0000;
}