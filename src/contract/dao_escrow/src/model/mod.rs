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

//! DAO-Escrow contract data structures
//!
//! ## Three Operating Modes
//!
//! DAO-Escrow supports three configuration modes via the `mode` field:
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────┐
//! │                         DAO-Escrow Modes                              │
//! ├─────────────────────────────────────────────────────────────────────┤
//! │                                                                       │
//! │  MODE_ESCROW: Escrow-Only (Insurance Pool)                          │
//! │  ┌─────────────────────────────────────────────────────────────┐     │
//! │  │  - Members pay premiums → endowment grows                   │     │
//! │  │  - No treasury (operational funds)                         │     │
//! │  │  - Endowment pays out claims                                │     │
//! │  │  - For: Pure insurance, no overhead                        │     │
//! │  └─────────────────────────────────────────────────────────────┘     │
//! │                                                                       │
//! │  MODE_TREASURY: Treasury-Only (Same as DarkWow DAO)                  │
//! │  ┌─────────────────────────────────────────────────────────────┐     │
//! │  │  - Members pay fees → treasury grows                        │     │
//! │  │  - DAO votes on treasury spending                           │     │
//! │  │  - No endowment/insurance                                   │     │
//! │  │  - For: Protocol treasury, grants, development             │     │
//! │  └─────────────────────────────────────────────────────────────┘     │
//! │                                                                       │
//! │  MODE_TREASURY_ENDOWMENT: Treasury + Endowment (Combined)           │
//! │  ┌─────────────────────────────────────────────────────────────┐     │
//! │  │  Treasury:  │  Endowment:                                    │     │
//! │  │  - Operational │  - Insurance reserve                       │     │
//! │  │  - DAO votes   │  - Emergency only                          │     │
//! │  │  - Grants      │  - Cannot fund treasury                    │     │
//! │  │  - Dev costs   │  - Refund protection                      │     │
//! │  └─────────────────────────────────────────────────────────────┘     │
//! │  For: Full-featured DAO with insurance backing                     │
//! │                                                                       │
//! └─────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Fee Split (MODE_TREASURY_ENDOWMENT only)
//!
//! When a member pays a premium:
//! - `treasury_share` → Treasury (operational funds)
//! - `endowment_share` → Endowment (insurance reserve)
//!
//! The split is enforced in the circuit. In other modes, all funds go
//! to the single pool.

use dwow_sdk::{
    blockchain::SerializedLen,
    crypto::{constants::DRK_POSEIDON_DOMAIN_COMMITMENT, pasta_prelude::PrimeField, poseidon_hash, BaseBlind, IntentNullifier, PublicKey, ScalarBlind, AssetId},
    error::ContractError,
    pasta::{group::GroupEncoding, pallas},
};

/// DAO-Escrow unique identifier (hash of parameters)
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct DaoEscrowBulla(pub pallas::Base);
impl DaoEscrowBulla {
    pub fn inner(&self) -> pallas::Base { self.0 }
    pub fn to_bytes(&self) -> [u8; 32] { self.0.to_repr() }
    pub fn from_bytes(x: [u8; 32]) -> Option<Self> {
        Option::<pallas::Base>::from(pallas::Base::from_repr(x)).map(Self)
    }
    pub fn is_zero(&self) -> bool { self.0 == pallas::Base::zero() }
    pub fn zero() -> Self { Self(pallas::Base::zero()) }
    pub fn encode(&self) -> Vec<u8> { self.to_bytes().to_vec() }
    #[expect(clippy::unwrap_used, reason = "internally-consistent serialized data")]
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 32 { return Err(ContractError::IoError(format!("expected 32 got {}", data.len()))); }
        Self::from_bytes(data.try_into().unwrap()).ok_or_else(|| ContractError::IoError("invalid field element".into()))
    }
}

/// Membership note identifier
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct MembershipNote(pub pallas::Base);
impl MembershipNote {
    pub fn inner(&self) -> pallas::Base { self.0 }
    pub fn to_bytes(&self) -> [u8; 32] { self.0.to_repr() }
    pub fn from_bytes(x: [u8; 32]) -> Option<Self> {
        Option::<pallas::Base>::from(pallas::Base::from_repr(x)).map(Self)
    }
    pub fn is_zero(&self) -> bool { self.0 == pallas::Base::zero() }
    pub fn zero() -> Self { Self(pallas::Base::zero()) }
    pub fn encode(&self) -> Vec<u8> { self.to_bytes().to_vec() }
    #[expect(clippy::unwrap_used, reason = "internally-consistent serialized data")]
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 32 { return Err(ContractError::IoError(format!("expected 32 got {}", data.len()))); }
        Self::from_bytes(data.try_into().unwrap()).ok_or_else(|| ContractError::IoError("invalid field element".into()))
    }
}

// ============================================================================
// DAO-ESCROW MODES
// ============================================================================

/// Operating mode of the DAO-Escrow
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DaoEscrowMode {
    /// Escrow-only: Pure insurance pool, no treasury
    Escrow = 0,
    /// Treasury-only: Same as DarkWow DAO, no endowment
    Treasury = 1,
    /// Treasury + Endowment: Full-featured with insurance backing
    TreasuryEndowment = 2,
}

impl TryFrom<u8> for DaoEscrowMode {
    type Error = dwow_sdk::error::ContractError;

    fn try_from(b: u8) -> Result<Self, Self::Error> {
        match b {
            0 => Ok(Self::Escrow),
            1 => Ok(Self::Treasury),
            2 => Ok(Self::TreasuryEndowment),
            _ => Err(dwow_sdk::error::ContractError::InvalidFunction),
        }
    }
}

impl DaoEscrowMode { pub fn encode(&self) -> Vec<u8> { vec![*self as u8] } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.is_empty() { return Err(ContractError::IoError("DaoEscrowMode: empty".into())); } Self::try_from(data[0]) } }

// `FeeConfig` lived here — a treasury/endowment split ratio, written `None` at init and never set by any
// caller. It was removed with the record field that held it, because no contract in this tree splits an
// incoming payment between two pools: the split it configured was never implementable, which is why
// `pay_premium_v1`'s own comment said "simplified - all to endowment".

/// Represents a DAO-Escrow instance
///
/// **Four fields, and every one of them is read.** The record previously carried nineteen, of which
/// fourteen were written and never consulted — verified by grepping the entrypoint for reads of each,
/// which found exactly `mode`, `owner_pubkey`, `multisig_group_id` and the `member_count` increment. The
/// removed ones were: `version`, `instance_seed`, `bulla` (the endowment tree is keyed by the bulla; the
/// record's copy was never consulted), `pool_asset_id`, the three purse ids, `member_count`, `fee_config`,
/// `max_members`, `created_at`, `bulla_blind`, `paused`, and the two drain-protection fields.
///
/// Three of those removals are worth naming because each was a *claim* the tree told about itself: the
/// purse ids said this contract tracked balances in Purses (it never addressed the Purse contract);
/// `fee_config` said a premium could be split between two pools (nothing in this tree splits a payment);
/// and `max_members` said the membership roll was capped (nothing counted members).
#[derive(Debug, Clone)]
pub struct DaoEscrow {
    /// Operating mode. Which endpoints are legitimate for this endowment — **not** a claim about which
    /// pool holds value, because this contract holds no balance. Set at init and immutable after.
    pub mode: DaoEscrowMode,
    /// Owner/creator public key. Read by `update_v1`'s ownership check.
    pub owner_pubkey: PublicKey,
    /// MultiSig group ID for governance (replaces `GovernanceConfig`). Zero means no group is installed,
    /// and every governance gate refuses. Written once, by `update_v1`, and never rotated.
    pub multisig_group_id: pallas::Base,
    /// Minimum premium. Read by `pay_premium_v1`, so a premium below the floor is refused.
    pub min_premium: u64,
}

impl DaoEscrow {
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(73);
        b.push(self.mode as u8);
        b.extend_from_slice(&self.owner_pubkey.to_bytes());
        b.extend_from_slice(&self.multisig_group_id.to_repr());
        b.extend_from_slice(&self.min_premium.to_le_bytes());
        b
    }
    #[expect(clippy::unwrap_used, reason = "internally-consistent serialized data")]
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        // An equality, not a minimum: this record has no variable-length part, so any other length is a
        // record this contract did not write. A minimum would let a longer buffer decode as a shorter
        // record and silently drop the tail — `OBL-C150`'s class, which this campaign has now found four
        // times.
        if data.len() != 73 { return Err(ContractError::IoError(format!("DaoEscrow: expected 73 bytes, got {}", data.len()))); }
        let mode = DaoEscrowMode::try_from(data[0])?;
        let owner_pubkey = PublicKey::from_bytes(data[1..33].try_into().unwrap())?;
        let multisig_group_id = Option::<pallas::Base>::from(pallas::Base::from_repr(data[33..65].try_into().unwrap())).ok_or_else(|| ContractError::IoError("DaoEscrow: invalid multisig_group_id".into()))?;
        let min_premium = u64::from_le_bytes(data[65..73].try_into().unwrap());
        Ok(DaoEscrow { mode, owner_pubkey, multisig_group_id, min_premium })
    }
    /// Derive the DAO-Escrow bulla from parameters.
    ///
    /// **This must be the same derivation `proof/init.zk` constrains.** The circuit computes
    /// `endowment_bulla = poseidon_hash(DOMAIN_COMMITMENT, dao_bulla, owner_pub_x, owner_pub_y,
    /// endowment_asset_id, bulla_blind)` and publishes it as instance 4; `client/init.rs` and
    /// `initialize_get_metadata` build the same six elements, so prover and host agreed and the proof
    /// always verified.
    ///
    /// This function — the value the endowment is actually **stored under** — hashed five elements with
    /// **no domain constant**. Nothing compared the two, so the instance the circuit attested was a value
    /// the chain never used, in two disjoint worlds that no test could tell apart (`OBL-C156`). The
    /// observable consequence was interoperability rather than authorisation: a client deriving the bulla
    /// the way the circuit and the documentation state it computed a value the contract had never stored,
    /// and every call it built failed `DaoEscrowNotFound`. The fixture only escaped that by discovering
    /// the derived value empirically.
    pub fn derive_bulla(
        dao_bulla: DaoEscrowBulla,
        owner_pubkey: &PublicKey,
        pool_asset_id: AssetId,
        bulla_blind: BaseBlind,
    ) -> DaoEscrowBulla {
        #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy()/x()/y() is always Some")]
        let (ox, oy) = owner_pubkey.xy().expect("pk not identity");
        DaoEscrowBulla(poseidon_hash([
            DRK_POSEIDON_DOMAIN_COMMITMENT,
            dao_bulla.inner(),
            ox,
            oy,
            pool_asset_id.inner(),
            bulla_blind.inner(),
        ]))
    }
}

// ============================================================================
// GOVERNANCE APPROVALS (`OBL-C151`)
// ============================================================================

/// Domain constant for the message a governance group approves.
///
/// Eleven is the next free value: `src/sdk/src/crypto/constants.rs` registers 1..=10 (`NULLIFIER` 1 …
/// `ATTRIBUTE` 10). It lives here rather than in that registry for a blast-radius reason — a change under
/// `src/sdk/**` stales every artifact — and it does not need to be there for correctness: the domain only
/// has to be distinct from every other `poseidon_hash` over this contract's values.
pub const DAO_ESCROW_DOMAIN_GOVERNANCE_APPROVAL: pallas::Base =
    pallas::Base::from_raw([11, 0, 0, 0]);

/// Role tags, one per governance-gated action.
///
/// **These are load-bearing rather than decorative.** A MultiSig approval is spend-once: `FinalizeV1`
/// consumes the nullifiers it names, and each is `H(1, member_secret, group_id, message_hash)`. The
/// endpoints do not all key on distinct ids — `propose_claim_v1` and `vote_claim_v1` both take a
/// `claim_id` — so with an untagged message the vote's approval would name nullifiers the proposal had
/// already spent, the child would fail, and the parent with it.
///
/// Four tags retired with their endpoints and their numbers are left unassigned: `RESOLVE_DISPUTE` (3),
/// `ENABLE_DRAIN_PROTECTION` (7), `REGISTER_CAPABILITY_REQUIREMENT` (8) and
/// `DEACTIVATE_CAPABILITY_REQUIREMENT` (9). A gap here is the same rule the function selectors follow:
/// a number that meant something keeps meaning it, so what a recorded approval names cannot change.
pub mod governance_role {
    /// `ProposeClaimV1` — the action id is the claim id.
    pub const PROPOSE_CLAIM: u8 = 1;
    /// `VoteClaimV1` — the action id is `(claim_id, voter_x, voter_y, direction)` (`OBL-C160`).
    pub const VOTE_CLAIM: u8 = 2;
    /// `EndowmentWithdrawV1` — the action id is `(bulla, value, recipient_x)`.
    pub const ENDOWMENT_WITHDRAW: u8 = 4;
    /// `TreasurySpendV1` — the action id is `(bulla, value, recipient_x)`.
    pub const TREASURY_SPEND: u8 = 5;
    /// `WithdrawV1` — the action id is `(bulla, value, recipient_x)`.
    pub const WITHDRAW: u8 = 6;
    /// `CancelClaimV1` — the action id is the claim id.
    pub const CANCEL_CLAIM: u8 = 10;
}

/// What a governance group signs to authorise one action: `H(domain, role, action_id)`.
///
/// The contract and the fixture both call this rather than re-implementing it, so the message the group
/// signs and the message the contract checks cannot drift — the same reason `MultiSigHarness::group_id`
/// delegates to the multisig contract's own `derive_group_id`.
///
/// **Why the two spend endpoints use a triple and not an id**: `treasury_spend_v1` reaches its gate
/// exactly when `proposal_id == 0`, so an id-based message there would be zero for every call and one
/// approval of zero would authorise every spend forever. `(bulla, value, recipient_x)` binds the
/// instance, the amount and the payee, and cannot be zero because `recipient_x` comes from a `PublicKey`
/// that cannot be the identity.
pub fn governance_message(role: u8, action_id: pallas::Base) -> pallas::Base {
    poseidon_hash([
        DAO_ESCROW_DOMAIN_GOVERNANCE_APPROVAL,
        pallas::Base::from(role as u64),
        action_id,
    ])
}

// ============================================================================
// MEMBERSHIP NOTE
// ============================================================================

/// Represents a membership note (time-limited)
#[derive(Debug, Clone)]
pub struct Membership {
    pub version: u8,
    /// Membership note (unique identifier)
    pub note: MembershipNote,
    /// DAO-Escrow bulla this membership belongs to
    pub dao_escrow_bulla: DaoEscrowBulla,
    /// Member's public key
    pub member_pubkey: PublicKey,
    /// Value/maturity of membership
    pub value: u64,
    /// Token ID
    pub asset_id: AssetId,
    /// Expiry block (membership valid until this block)
    pub expiry: u64,
    /// Created at block
    pub created_at: u64,
}

impl Membership {
    /// Derive the membership note from parameters
    pub fn derive_note(
        dao_escrow_bulla: DaoEscrowBulla,
        member_pubkey: &PublicKey,
        value: u64,
        asset_id: pallas::Base,
        expiry: u64,
        blind: BaseBlind,
    ) -> MembershipNote {
        #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy()/x()/y() is always Some")]
        let (mx, my) = member_pubkey.xy().expect("pk not identity");
        MembershipNote(poseidon_hash([
            dao_escrow_bulla.inner(),
            mx,
            my,
            pallas::Base::from(value),
            asset_id,
            pallas::Base::from(expiry),
            blind.inner(),
        ]))
    }
}

// ============================================================================
// PARAMETERS (for contract calls)
// ============================================================================

/// Parameters for `DaoEscrow::InitializeV1`
#[derive(Debug, Clone, )]
pub struct InitializeParamsV1 {
    /// The controlling DAO's bulla
    pub dao_bulla: DaoEscrowBulla,
    /// Owner's public key
    pub owner_pubkey: PublicKey,
    /// Endowment token ID
    pub endowment_asset_id: AssetId,
    /// Bulla blind factor
    pub bulla_blind: BaseBlind,
    /// Operating mode, chosen by the creator. **This is what makes the mode real**: the record has always
    /// carried a `DaoEscrowMode`, and `initialize_apply_v1` wrote `Escrow` as a constant, so no caller
    /// could ever produce the other two variants and `treasury_spend_v1`'s gate on them could not pass
    /// for any call (`OBL-C154`).
    pub mode: DaoEscrowMode,
    /// Minimum premium, enforced by `pay_premium_v1`. Read, where it used to be a field nothing set.
    pub min_premium: u64,
}

/// State update for `DaoEscrow::InitializeV1`
#[derive(Debug, Clone)]
pub struct InitializeUpdateV1 {
    /// The created endowment bulla — the key the record is stored under
    pub bulla: DaoEscrowBulla,
    /// Owner public key (for withdrawal authorization)
    pub owner_pubkey: PublicKey,
    /// Operating mode, carried from the params so apply writes what the caller chose
    pub mode: DaoEscrowMode,
    /// Minimum premium, carried from the params
    pub min_premium: u64,
}

/// Parameters for `DaoEscrow::UpdateV1`
#[derive(Debug, Clone, )]
pub struct UpdateParamsV1 {
    /// DAO-Escrow bulla
    pub bulla: DaoEscrowBulla,
    /// The governance group this call registers, if it registers one (`OBL-C151`). `None` writes
    /// nothing; the record refuses a second write, so setting it is one-shot.
    pub multisig_group_id: Option<pallas::Base>,
    /// The endowment's owner. Its coordinates are what the accompanying proof exposes and constrains to
    /// knowledge of `owner_secret` (`set_governance_config.zk`), so the handler compares the *exposed*
    /// value rather than trusting this field — a public key on its own proves nothing (see `OBL-C152`).
    pub owner_pubkey: PublicKey,
    /// `poseidon_hash(DOMAIN_NULLIFIER, owner_pub_x, owner_pub_y, owner_secret, dao_escrow_bulla)`, which
    /// the circuit binds to the secret and this contract records to make the ownership proof one-shot.
    /// It travels in params because it is witness-derived: `get_metadata` publishes the expected instances
    /// from params alone, so a value only the witness knew could not be published to compare against.
    pub owner_nullifier: pallas::Base,
}

/// State update for `DaoEscrow::UpdateV1`
#[derive(Debug, Clone)]
pub struct UpdateUpdateV1 {
    /// Updated DAO-Escrow bulla
    pub bulla: DaoEscrowBulla,
    /// The ownership proof's nullifier, which apply records so the proof is spend-once (`OBL-C151`).
    ///
    /// **It is carried because exec may not write and apply may not read** (`OBL-C72`): the *check* is
    /// exec's, and the *write* has to be apply's, so the value has to travel between them. Without it
    /// the check had nothing to find — a guard that could not fire, which the `UpdateV1_ReplaysTheProof`
    /// negative control caught by passing a second time when it should have been refused.
    pub owner_nullifier: pallas::Base,
    /// `DaoEscrow::encode()`, as exec left it — carried so apply re-stores it without reading
    /// (register OBL-C72). Before `OBL-C151` this struct carried only the bulla and its apply was a
    /// no-op, so nothing an `UpdateV1` call could say ever reached the record.
    pub endowment_bytes: Vec<u8>,
}

/// Parameters for `DaoEscrow::PayPremiumV1`
#[derive(Debug, Clone, )]
pub struct PayPremiumParamsV1 {
    /// DAO-Escrow bulla
    pub dao_escrow_bulla: DaoEscrowBulla,
    /// Membership note commitment
    pub membership_note: MembershipNote,
    /// Member's value commitment (Pedersen)
    pub value_commit: pallas::Point,
    /// Premium amount being paid
    pub value: u64,
    /// Token ID
    pub asset_id: AssetId,
    /// Membership expiry block
    pub expiry: u64,
    /// Membership blind factor
    pub membership_blind: BaseBlind,
    /// Value blind factor
    pub value_blind: ScalarBlind,
    /// Member public key (verified in ZK proof)
    pub member_pubkey: PublicKey,
}

/// State update for `DaoEscrow::PayPremiumV1`
#[derive(Debug, Clone)]
pub struct PayPremiumUpdateV1 {
    /// DAO-Escrow bulla
    pub dao_escrow_bulla: DaoEscrowBulla,
    /// Created membership note
    pub membership_note: MembershipNote,
    /// Updated total endowment
    pub amount: u64,
    /// Member public key
    pub member_pubkey: PublicKey,
    /// Token ID
    pub asset_id: AssetId,
    /// Membership expiry block
    pub expiry: u64,
    /// Block height the membership is stamped with, read in exec.
    pub created_at: u64,
    /// `DaoEscrow::encode()` for the endowment, as exec left it. Carried so apply re-stores it
    /// instead of reading it back (register OBL-C72). `DaoEscrow::encode` is variable-length.
    pub endowment_bytes: Vec<u8>,
}

/// Parameters for `DaoEscrow::WithdrawV1`
#[derive(Debug, Clone, )]
pub struct WithdrawParamsV1 {
    /// DAO-Escrow bulla
    pub dao_escrow_bulla: DaoEscrowBulla,
    /// Amount to withdraw
    pub value: u64,
    /// Recipient
    pub recipient_pubkey: PublicKey,
}

/// State update for `DaoEscrow::WithdrawV1`
#[derive(Debug, Clone)]
pub struct WithdrawUpdateV1 {
    /// DAO-Escrow bulla
    pub dao_escrow_bulla: DaoEscrowBulla,
    /// Withdrawn amount
    pub value: u64,
    /// Updated total endowment
    pub amount: u64,
    /// `DaoEscrow::encode()`, as exec left it — carried so apply re-stores it without reading
    /// (register OBL-C72).
    pub endowment_bytes: Vec<u8>,
}

// ============================================================================
// CLAIM / ENDOWMENT WITHDRAWAL TYPES (for EndowmentWithdrawV1)
// ============================================================================

/// Claim identifier
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct ClaimId(pub pallas::Base);
impl ClaimId {
    pub fn inner(&self) -> pallas::Base { self.0 }
    pub fn to_bytes(&self) -> [u8; 32] { self.0.to_repr() }
    pub fn from_bytes(x: [u8; 32]) -> Option<Self> {
        Option::<pallas::Base>::from(pallas::Base::from_repr(x)).map(Self)
    }
    pub fn is_zero(&self) -> bool { self.0 == pallas::Base::zero() }
    pub fn encode(&self) -> Vec<u8> { self.to_bytes().to_vec() }
    #[expect(clippy::unwrap_used, reason = "internally-consistent serialized data")]
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 32 { return Err(ContractError::IoError(format!("expected 32 got {}", data.len()))); }
        Self::from_bytes(data.try_into().unwrap()).ok_or_else(|| ContractError::IoError("invalid field element".into()))
    }
}

/// Vote type for claims
#[derive(Debug, Clone, Copy, PartialEq, Eq, )]
pub enum VoteType {
    /// Yes vote
    Yes = 0,
    /// No vote
    No = 1,
}

impl TryFrom<u8> for VoteType { type Error = dwow_sdk::error::ContractError; fn try_from(b: u8) -> Result<Self, Self::Error> { match b { 0=>Ok(Self::Yes),1=>Ok(Self::No),_=>Err(dwow_sdk::error::ContractError::InvalidFunction) } } }
impl VoteType { pub fn encode(&self) -> Vec<u8> { vec![*self as u8] } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.is_empty() { return Err(ContractError::IoError("VoteType: empty".into())); } Self::try_from(data[0]) } }

// `ClaimType` — `Endowment` / `Treasury` / `Dispute` — retired here with its `TryFrom` and its
// `encode`/`decode`. It separated an endowment claim from a treasury one, which `DaoEscrow::mode` already
// separates, and its third variant named a dispute path that never had an implementation. The field it
// rode on (`Proposal::claim_type`) was written by `propose_claim_apply_v1` and read by nothing.

/// Parameters for proposing a claim
///
/// `description_hash` and `proposer_pubkey` used to sit here. Both were carried into the stored
/// `Proposal` and read by nothing there either: the description was never re-published to any reader,
/// and the proposer's key was read only by the cancellation check that compared two public values and
/// therefore admitted anyone (`OBL-C152`, retired in favour of the group's approval).
#[derive(Debug, Clone, )]
pub struct ProposeClaimParamsV1 {
    /// DAO-Escrow bulla
    pub dao_escrow_bulla: DaoEscrowBulla,
    /// Claim identifier
    pub claim_id: ClaimId,
    /// Amount being claimed
    pub value: u64,
    /// Recipient public key
    pub recipient_pubkey: PublicKey,
    /// The blind of the claim commitment the proposer's proof carries (`OBL-C153`).
    ///
    /// **This field is why the endpoint's proof could never verify.** The contract derived
    /// `claim_commit = poseidon_hash(4, claim_id, value, blind)` and had no blind to use, so it
    /// substituted `capability_proof.capability_secret` — its own comment called that a "claim_blind
    /// placeholder (needs dedicated field in params)". The client derives the same commitment from the
    /// blind it *does* hold, so the proof's instance vector and the published one disagreed and the proof
    /// was rejected as invalid. Nothing noticed for as long as `propose_claim_v1` refused earlier at
    /// `GovernanceNotActive`, because **exec runs before proof verification** (`execution.rs:566`
    /// precedes the loop at `:650+`): the governance fix walked the call far enough for its proof to be
    /// examined for the first time.
    pub claim_blind: pallas::Base,
}

/// State update for `ProposeClaimV1`
#[derive(Debug, Clone)]
pub struct ProposeClaimUpdateV1 {
    /// DAO-Escrow bulla
    pub dao_escrow_bulla: DaoEscrowBulla,
    /// Claim identifier
    pub claim_id: ClaimId,
    /// Amount being claimed
    pub value: u64,
    /// Voting deadline
    pub voting_ends_at: u64,
    /// Execution deadline
    pub execution_deadline: u64,
    /// Recipient public key
    pub recipient_pubkey: PublicKey,
}

/// Parameters for voting on a claim
#[derive(Debug, Clone, )]
pub struct VoteClaimParamsV1 {
    /// DAO-Escrow bulla
    pub dao_escrow_bulla: DaoEscrowBulla,
    /// Claim identifier
    pub claim_id: ClaimId,
    /// Vote type
    pub vote: VoteType,
    /// Voter's public key
    pub voter_pubkey: PublicKey,
    /// Capability proof for member_vote
    pub capability_proof: CapabilityProof,
}

/// State update for `VoteClaimV1`
#[derive(Debug, Clone)]
pub struct VoteClaimUpdateV1 {
    /// DAO-Escrow bulla
    pub dao_escrow_bulla: DaoEscrowBulla,
    /// Claim identifier
    pub claim_id: ClaimId,
    /// The state the vote decided: `Approved` or `Rejected`, or `Expired` on the window-elapsed path.
    ///
    /// This replaces the `yes_votes`/`no_votes`/`passed`/`expired` quartet. The tally had no reader —
    /// nothing compared it to a quorum — and the group's own `FinalizeV1` is the quorum, so the vote's
    /// outcome *is* the state (`OBL-C159`, `OBL-C160`).
    pub state: ProposalState,
    /// The voter's nullifier, carried here so **apply** marks it spent.
    ///
    /// Exec used to call `db_mark_spent` directly, which writes state during verification —
    /// `contract-standards.md` §8: *"An exec-phase write that persists when the transaction fails in
    /// another contract call creates orphaned state."* `db_contains_key` stays in exec (that is
    /// `↓nullify`'s entrypoint per `contract-wasm-type-system.md` §A.2.1); the write moves here.
    ///
    /// On the `Expired` path no vote was cast, so this is zero and apply does not spend it.
    pub vote_nullifier: pallas::Base,
    /// `Proposal::encode()` with the new state applied, as exec produced it. Carried so apply
    /// re-stores it rather than reading it back (register OBL-C72).
    pub proposal_bytes: Vec<u8>,
}

// ============================================================================
// ENDOWMENT WITHDRAWAL (Execute approved claim)
// ============================================================================

/// Parameters for executing an approved endowment withdrawal (claim)
///
/// The authority is the endowment's group, and only the group: `capability_proof` and `proposal_id`
/// used to sit here as two mutually-exclusive *path selectors*, neither of which is a thing the handler
/// reads. `proposal_id` named a proposal the endpoint loaded separately (`verify_proposal_approved`) —
/// a second executor for the lifecycle `ExecuteClaimV1` already executes — and `capability_proof` was
/// tested with a bare `Option::is_some()` whose contents nothing touched, so a field named for a
/// capability proof was carrying one bit of routing (`OBL-C152`'s class, and the note at
/// `endowment_withdraw_v1` recorded its removal as its own unit).
#[derive(Debug, Clone, )]
pub struct EndowmentWithdrawParamsV1 {
    /// DAO-Escrow bulla
    pub dao_escrow_bulla: DaoEscrowBulla,
    /// Claim identifier, for the record this call is about
    pub claim_id: ClaimId,
    /// Recipient of the funds
    pub recipient_pubkey: PublicKey,
    /// Amount to withdraw
    pub value: u64,
}

/// State update for `EndowmentWithdrawV1`
#[derive(Debug, Clone)]
pub struct EndowmentWithdrawUpdateV1 {
    /// DAO-Escrow bulla
    pub dao_escrow_bulla: DaoEscrowBulla,
    /// Claim identifier
    pub claim_id: ClaimId,
    /// Amount withdrawn
    pub value: u64,
    /// Updated total endowment
    pub amount: u64,
    /// `DaoEscrow::encode()`, as exec left it (OBL-C72).
    pub endowment_bytes: Vec<u8>,
}

// ============================================================================
// TREASURY SPEND (Execute approved treasury proposal)
// ============================================================================

/// Parameters for executing an approved treasury spend
///
/// The group's approval is the only authority here, as for `EndowmentWithdrawV1` — see the note on that
/// struct for what `proposal_id` and `capability_proof` were and why neither is a handler input.
#[derive(Debug, Clone, )]
pub struct TreasurySpendParamsV1 {
    /// DAO-Escrow bulla
    pub dao_escrow_bulla: DaoEscrowBulla,
    /// Recipient of the funds
    pub recipient_pubkey: PublicKey,
    /// Amount to spend
    pub value: u64,
}

/// State update for `TreasurySpendV1`
#[derive(Debug, Clone)]
pub struct TreasurySpendUpdateV1 {
    /// DAO-Escrow bulla
    pub dao_escrow_bulla: DaoEscrowBulla,
    /// Amount spent
    pub value: u64,
    /// Updated total treasury
    pub amount: u64,
    /// `DaoEscrow::encode()`, as exec left it (OBL-C72).
    pub endowment_bytes: Vec<u8>,
}

// ============================================================================
// CAPABILITY PROOF (cross-contract reference to Identity contract)
// ============================================================================

/// A capability proof from the Identity contract.
///
/// **One field of this struct is load-bearing, and it is not a capability proof's.** `VoteClaimV1`
/// reads `capability_secret` to derive the `vote_nullifier` that `vote_claim_get_metadata` publishes,
/// so the value the contract records in its nullifiers tree is the value the `VoteClaimV2` proof
/// constrains (`entrypoint.rs`, `vote_claim_v1`). The rest — `capability_id`, `nullifier`, `issuer_pub`,
/// `predicate_result`, `proof` — is read by nothing, and `capability_secret` is public call data: the
/// name promises a secret that the struct carrying it publishes. That is `OBL-C160`, recorded and not
/// yet fixed; the field is kept because removing it moves `VoteClaimParamsV1`'s codec and the circuit's
/// instance set together, which is a unit of its own.
#[derive(Debug, Clone, )]
pub struct CapabilityProof {
    /// Capability identifier (from Identity contract)
    pub capability_id: [u8; 32],
    /// Holder's capability secret
    pub capability_secret: [u8; 32],
    /// Nullifier to prevent replay
    pub nullifier: IntentNullifier,
    /// Issuer's public key
    pub issuer_pub: [u8; 32],
    /// Predicate result from ZK circuit
    pub predicate_result: [u8; 32],
    /// ZK proof bytes
    pub proof: Vec<u8>,
}

// `CapabilityRequirement` retired here: it mapped a DAO role name to an Identity capability id, and the
// endpoint that wrote it (`RegisterCapabilityRequirementV1`, 0x0a) and the one that read it
// (`VerifyMemberCapabilityV1`, 0x0b) both retired with the OCap model. Nothing registered a requirement,
// which is why every governance call would have failed `CapabilityRequirementNotRegistered` even once its
// gates were reachable (`OBL-C151`).

// ============================================================================
// PROPOSAL STATE
// ============================================================================

/// Proposal lifecycle states
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProposalState {
    /// Voting is open
    Pending = 0,
    /// Vote passed
    Approved = 1,
    /// Vote failed
    Rejected = 2,
    /// Claim has been executed
    Executed = 3,
    /// Proposer cancelled
    Cancelled = 4,
    /// Voting/execution window expired
    Expired = 5,
}

impl TryFrom<u8> for ProposalState {
    type Error = dwow_sdk::error::ContractError;

    fn try_from(b: u8) -> Result<Self, Self::Error> {
        match b {
            0 => Ok(Self::Pending),
            1 => Ok(Self::Approved),
            2 => Ok(Self::Rejected),
            3 => Ok(Self::Executed),
            4 => Ok(Self::Cancelled),
            5 => Ok(Self::Expired),
            _ => Err(dwow_sdk::error::ContractError::InvalidFunction),
        }
    }
}

// ============================================================================
// PROPOSAL
// ============================================================================

/// Proposal identifier type
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct ProposalId(pub pallas::Base);
impl ProposalId {
    pub fn inner(&self) -> pallas::Base { self.0 }
    pub fn to_bytes(&self) -> [u8; 32] { self.0.to_repr() }
    pub fn from_bytes(x: [u8; 32]) -> Option<Self> {
        Option::<pallas::Base>::from(pallas::Base::from_repr(x)).map(Self)
    }
    pub fn is_zero(&self) -> bool { self.0 == pallas::Base::zero() }
    pub fn encode(&self) -> Vec<u8> { self.to_bytes().to_vec() }
    #[expect(clippy::unwrap_used, reason = "internally-consistent serialized data")]
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 32 { return Err(ContractError::IoError(format!("expected 32 got {}", data.len()))); }
        Self::from_bytes(data.try_into().unwrap()).ok_or_else(|| ContractError::IoError("invalid field element".into()))
    }
}

/// A governance claim, as stored under its claim id in the `proposals` tree.
///
/// **Reduced to the fields something reads.** The record used to carry `version`, `id`, `proposer_pubkey`,
/// `claim_type`, `description_hash`, `created_at`, `yes_votes` and `no_votes` as well — eight fields that
/// `propose_claim_apply_v1` wrote and no path in the crate ever read. `id` restated the key the record is
/// stored under; `proposer_pubkey` was read only by a cancellation check that compared two public values
/// and therefore admitted anyone (`OBL-C152`, retired); `claim_type` separated an endowment claim from a
/// treasury one, which `DaoEscrow::mode` already separates; and the tally's reader was a quorum comparison
/// that was never written — `vote_claim_v1` now lets the group's own `FinalizeV1` be the quorum
/// (`OBL-C159`, `OBL-C160`).
#[derive(Debug, Clone)]
pub struct Proposal {
    /// The endowment this claim draws on
    pub dao_escrow_bulla: DaoEscrowBulla,
    /// Amount claimed
    pub value: u64,
    /// Recipient
    pub recipient_pubkey: PublicKey,
    /// Pending, Approved, Rejected, Executed, Cancelled or Expired
    pub state: ProposalState,
    /// Voting deadline
    pub voting_ends_at: u64,
    /// Execution deadline
    pub execution_deadline: u64,
}

impl Proposal {
    /// 32 (bulla) + 8 (value) + 32 (recipient) + 1 (state) + 8 + 8 (deadlines)
    pub const ENCODED_SIZE: usize = 89;
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(Self::ENCODED_SIZE);
        b.extend_from_slice(&self.dao_escrow_bulla.to_bytes());
        b.extend_from_slice(&self.value.to_le_bytes());
        b.extend_from_slice(&self.recipient_pubkey.to_bytes());
        b.push(self.state as u8);
        b.extend_from_slice(&self.voting_ends_at.to_le_bytes());
        b.extend_from_slice(&self.execution_deadline.to_le_bytes());
        b
    }
    #[expect(clippy::unwrap_used, reason = "internally-consistent serialized data")]
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE { return Err(ContractError::IoError(format!("Proposal: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len()))); }
        Ok(Proposal {
            dao_escrow_bulla: DaoEscrowBulla(Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("Proposal: invalid dao_escrow_bulla".into()))?),
            value: u64::from_le_bytes(data[32..40].try_into().unwrap()),
            recipient_pubkey: PublicKey::from_bytes(data[40..72].try_into().unwrap())?,
            state: ProposalState::try_from(data[72])?,
            voting_ends_at: u64::from_le_bytes(data[73..81].try_into().unwrap()),
            execution_deadline: u64::from_le_bytes(data[81..89].try_into().unwrap()),
        })
    }
}

// `VoteRecord`, `OracleAttestationRef` and `DisputeResolution` retired here. `VoteRecord` was a struct
// with no writer and no reader — the vote's double-spend guard is the nullifiers tree's, and the record it
// described was never stored. The other two belonged to `ResolveDisputeV1` (0x0c), whose oracle-threshold
// resolution this contract never evaluated.

// ============================================================================
// EXECUTE CLAIM V1
// ============================================================================

/// Parameters for `ExecuteClaimV1`
#[derive(Debug, Clone, )]
pub struct ExecuteClaimParamsV1 {
    /// DAO-Escrow bulla
    pub dao_escrow_bulla: DaoEscrowBulla,
    /// Proposal ID to execute
    pub proposal_id: ProposalId,
    /// Recipient of the funds
    pub recipient_pubkey: PublicKey,
    /// Amount to transfer
    pub value: u64,
}

/// State update for `ExecuteClaimV1`
#[derive(Debug, Clone)]
pub struct ExecuteClaimUpdateV1 {
    /// DAO-Escrow bulla
    pub dao_escrow_bulla: DaoEscrowBulla,
    /// Proposal ID
    pub proposal_id: ProposalId,
    /// Amount executed
    pub value: u64,
    /// Updated state
    pub state: ProposalState,
    /// `Proposal::encode()` with the new state applied, as exec produced it (OBL-C72).
    pub proposal_bytes: Vec<u8>,
}

// `RegisterCapabilityRequirementV1`, `VerifyMemberCapabilityV1` and `ResolveDisputeV1` — their params and
// their state updates — retired here with their endpoints (0x0a, 0x0b, 0x0c), their two circuits and their
// ZKAS namespaces. `CapabilityProof` survives: `VoteClaimV1` reads its `capability_secret` and derives the
// circuit's nullifier from it.

// ============================================================================
// CANCEL CLAIM V1
// ============================================================================

/// Parameters for `CancelClaimV1`
#[derive(Debug, Clone, )]
pub struct CancelClaimParamsV1 {
    /// DAO-Escrow bulla
    pub dao_escrow_bulla: DaoEscrowBulla,
    /// Claim identifier
    pub claim_id: ClaimId,
}

/// State update for `CancelClaimV1`
#[derive(Debug, Clone)]
pub struct CancelClaimUpdateV1 {
    /// DAO-Escrow bulla
    pub dao_escrow_bulla: DaoEscrowBulla,
    /// Claim identifier
    pub claim_id: ClaimId,
    /// Updated state
    pub state: ProposalState,
    /// `Proposal::encode()` with the new state applied, as exec produced it (OBL-C72).
    pub proposal_bytes: Vec<u8>,
}

// ============================================================================
// SET GOVERNANCE CONFIG V1
// ============================================================================

// SetGovernanceConfigV1 (0x0e), SetGovernanceActiveV1 (0x0f) and
// DeactivateCapabilityRequirementV1's params and update (0x10) are all removed — MultiSig groups manage
// governance configuration and activation, and `update_v1` (0x01) is the `SetGovernanceConfigV2` caller.

// ============================================================================
// RHO-CALCULUS EXPLICIT ENCODE/DECODE
// ============================================================================

// --- Parameter structs ---

impl dwow_serial::Encodable for InitializeParamsV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for InitializeParamsV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
#[expect(clippy::unwrap_used, reason = "internally-consistent serialized data")]
impl InitializeParamsV1 { pub const ENCODED_SIZE: usize = 137; pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(Self::ENCODED_SIZE); b.extend_from_slice(&self.dao_bulla.to_bytes()); b.extend_from_slice(&self.owner_pubkey.to_bytes()); b.extend_from_slice(&self.endowment_asset_id.to_bytes()); b.extend_from_slice(&self.bulla_blind.inner().to_repr()); b.push(self.mode as u8); b.extend_from_slice(&self.min_premium.to_le_bytes()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != Self::ENCODED_SIZE { return Err(ContractError::IoError(format!("InitializeParamsV1: expected {} got {}", Self::ENCODED_SIZE, data.len()))); } Ok(InitializeParamsV1 { dao_bulla: DaoEscrowBulla(Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("InitializeParamsV1: invalid dao_bulla".into()))?), owner_pubkey: PublicKey::from_bytes(data[32..64].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("InitializeParamsV1: invalid owner_pubkey: {}", e)))?, endowment_asset_id: AssetId::from_bytes(data[64..96].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("InitializeParamsV1: invalid endowment_asset_id: {}", e)))?, bulla_blind: dwow_sdk::crypto::Blind(Option::<pallas::Base>::from(pallas::Base::from_repr(data[96..128].try_into().unwrap())).ok_or_else(|| ContractError::IoError("InitializeParamsV1: invalid bulla_blind".into()))?), mode: DaoEscrowMode::try_from(data[128])?, min_premium: u64::from_le_bytes(data[129..137].try_into().unwrap()) }) } }

impl dwow_serial::Encodable for UpdateParamsV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for UpdateParamsV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
#[expect(clippy::unwrap_used, reason = "internally-consistent serialized data")]
impl UpdateParamsV1 { pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(129); b.extend_from_slice(&self.bulla.to_bytes()); b.push(self.multisig_group_id.is_some() as u8); if let Some(gid) = self.multisig_group_id { b.extend_from_slice(&gid.to_repr()); } b.extend_from_slice(&self.owner_pubkey.to_bytes()); b.extend_from_slice(&self.owner_nullifier.to_repr()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() < 97 { return Err(ContractError::IoError(format!("UpdateParamsV1: too short, got {}", data.len()))); } let bulla = DaoEscrowBulla(Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("UpdateParamsV1: invalid bulla".into()))?); let has_gid = data[32] != 0; let expected = if has_gid { 129 } else { 97 }; if data.len() != expected { return Err(ContractError::IoError(format!("UpdateParamsV1: expected {} got {}", expected, data.len()))); } let multisig_group_id = if has_gid { Some(Option::<pallas::Base>::from(pallas::Base::from_repr(data[33..65].try_into().unwrap())).ok_or_else(|| ContractError::IoError("UpdateParamsV1: invalid multisig_group_id".into()))?) } else { None }; let owner_pubkey = PublicKey::from_bytes(data[expected-64..expected-32].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("UpdateParamsV1: invalid owner_pubkey: {}", e)))?; let owner_nullifier = Option::<pallas::Base>::from(pallas::Base::from_repr(data[expected-32..expected].try_into().unwrap())).ok_or_else(|| ContractError::IoError("UpdateParamsV1: invalid owner_nullifier".into()))?; Ok(UpdateParamsV1 { bulla, multisig_group_id, owner_pubkey, owner_nullifier }) } }

impl dwow_serial::Encodable for PayPremiumParamsV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for PayPremiumParamsV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
#[expect(clippy::unwrap_used, reason = "internally-consistent serialized data")]
impl PayPremiumParamsV1 { pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(240); b.extend_from_slice(&self.dao_escrow_bulla.to_bytes()); b.extend_from_slice(&self.membership_note.to_bytes()); b.extend_from_slice(&self.value_commit.to_bytes()); b.extend_from_slice(&self.value.to_le_bytes()); b.extend_from_slice(&self.asset_id.to_bytes()); b.extend_from_slice(&self.expiry.to_le_bytes()); b.extend_from_slice(&self.membership_blind.inner().to_repr()); b.extend_from_slice(&self.value_blind.inner().to_repr()); b.extend_from_slice(&self.member_pubkey.to_bytes()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 240 { return Err(ContractError::IoError(format!("PayPremiumParamsV1: expected 240 got {}", data.len()))); } Ok(PayPremiumParamsV1 { dao_escrow_bulla: DaoEscrowBulla(Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("PayPremiumParamsV1: invalid dao_escrow_bulla".into()))?), membership_note: MembershipNote(Option::<pallas::Base>::from(pallas::Base::from_repr(data[32..64].try_into().unwrap())).ok_or_else(|| ContractError::IoError("PayPremiumParamsV1: invalid membership_note".into()))?), value_commit: Option::<pallas::Point>::from(pallas::Point::from_bytes(data[64..96].try_into().unwrap())).ok_or_else(|| ContractError::IoError("PayPremiumParamsV1: invalid value_commit".into()))?, value: u64::from_le_bytes(data[96..104].try_into().unwrap()), asset_id: AssetId::from_bytes(data[104..136].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("PayPremiumParamsV1: invalid asset_id: {}", e)))?, expiry: u64::from_le_bytes(data[136..144].try_into().unwrap()), membership_blind: dwow_sdk::crypto::Blind(Option::<pallas::Base>::from(pallas::Base::from_repr(data[144..176].try_into().unwrap())).ok_or_else(|| ContractError::IoError("PayPremiumParamsV1: invalid membership_blind".into()))?), value_blind: dwow_sdk::crypto::Blind(Option::<pallas::Scalar>::from(pallas::Scalar::from_repr(data[176..208].try_into().unwrap())).ok_or_else(|| ContractError::IoError("PayPremiumParamsV1: invalid value_blind".into()))?), member_pubkey: PublicKey::from_bytes(data[208..240].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("PayPremiumParamsV1: invalid member_pubkey: {}", e)))? }) } }

impl dwow_serial::Encodable for WithdrawParamsV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for WithdrawParamsV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
#[expect(clippy::unwrap_used, reason = "internally-consistent serialized data")]
impl WithdrawParamsV1 { pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(72); b.extend_from_slice(&self.dao_escrow_bulla.to_bytes()); b.extend_from_slice(&self.value.to_le_bytes()); b.extend_from_slice(&self.recipient_pubkey.to_bytes()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 72 { return Err(ContractError::IoError(format!("WithdrawParamsV1: expected 72 got {}", data.len()))); } Ok(WithdrawParamsV1 { dao_escrow_bulla: DaoEscrowBulla(Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("WithdrawParamsV1: invalid dao_escrow_bulla".into()))?), value: u64::from_le_bytes(data[32..40].try_into().unwrap()), recipient_pubkey: PublicKey::from_bytes(data[40..72].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("WithdrawParamsV1: invalid recipient_pubkey: {}", e)))? }) } }

#[expect(clippy::unwrap_used, reason = "internally-consistent serialized data")]
impl ProposeClaimParamsV1 { pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(136); b.extend_from_slice(&self.dao_escrow_bulla.to_bytes()); b.extend_from_slice(&self.claim_id.to_bytes()); b.extend_from_slice(&self.value.to_le_bytes()); b.extend_from_slice(&self.recipient_pubkey.to_bytes()); b.extend_from_slice(&self.claim_blind.to_repr()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 136 { return Err(ContractError::IoError(format!("ProposeClaimParamsV1: expected 136 got {}", data.len()))); } Ok(ProposeClaimParamsV1 { dao_escrow_bulla: DaoEscrowBulla(Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("ProposeClaimParamsV1: invalid dao_escrow_bulla".into()))?), claim_id: ClaimId(Option::<pallas::Base>::from(pallas::Base::from_repr(data[32..64].try_into().unwrap())).ok_or_else(|| ContractError::IoError("ProposeClaimParamsV1: invalid claim_id".into()))?), value: u64::from_le_bytes(data[64..72].try_into().unwrap()), recipient_pubkey: PublicKey::from_bytes(data[72..104].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("ProposeClaimParamsV1: invalid recipient_pubkey: {}", e)))?, claim_blind: Option::<pallas::Base>::from(pallas::Base::from_repr(data[104..136].try_into().unwrap())).ok_or_else(|| ContractError::IoError("ProposeClaimParamsV1: invalid claim_blind".into()))? }) } }

#[expect(clippy::unwrap_used, reason = "internally-consistent serialized data")]
impl VoteClaimParamsV1 { pub fn encode(&self) -> Result<Vec<u8>, ContractError> { let cp = self.capability_proof.encode()?; let mut b = Vec::with_capacity(98+cp.len()); b.extend_from_slice(&self.dao_escrow_bulla.to_bytes()); b.extend_from_slice(&self.claim_id.to_bytes()); b.push(self.vote as u8); b.extend_from_slice(&self.voter_pubkey.to_bytes()); b.extend_from_slice(&cp); Ok(b) } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() < 98 { return Err(ContractError::IoError("VoteClaimParamsV1: too short".into())); } Ok(VoteClaimParamsV1 { dao_escrow_bulla: DaoEscrowBulla(Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("VoteClaimParamsV1: invalid dao_escrow_bulla".into()))?), claim_id: ClaimId(Option::<pallas::Base>::from(pallas::Base::from_repr(data[32..64].try_into().unwrap())).ok_or_else(|| ContractError::IoError("VoteClaimParamsV1: invalid claim_id".into()))?), vote: VoteType::decode(&data[64..65])?, voter_pubkey: PublicKey::from_bytes(data[65..97].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("VoteClaimParamsV1: invalid voter_pubkey: {}", e)))?, capability_proof: CapabilityProof::decode(&data[97..])? }) } }

#[expect(clippy::unwrap_used, reason = "internally-consistent serialized data")]
impl EndowmentWithdrawParamsV1 { pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(104); b.extend_from_slice(&self.dao_escrow_bulla.to_bytes()); b.extend_from_slice(&self.claim_id.to_bytes()); b.extend_from_slice(&self.recipient_pubkey.to_bytes()); b.extend_from_slice(&self.value.to_le_bytes()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 104 { return Err(ContractError::IoError(format!("EndowmentWithdrawParamsV1: expected 104 got {}", data.len()))); } Ok(EndowmentWithdrawParamsV1 { dao_escrow_bulla: DaoEscrowBulla(Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("EndowmentWithdrawParamsV1: invalid dao_escrow_bulla".into()))?), claim_id: ClaimId(Option::<pallas::Base>::from(pallas::Base::from_repr(data[32..64].try_into().unwrap())).ok_or_else(|| ContractError::IoError("EndowmentWithdrawParamsV1: invalid claim_id".into()))?), recipient_pubkey: PublicKey::from_bytes(data[64..96].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("EndowmentWithdrawParamsV1: invalid recipient_pubkey: {}", e)))?, value: u64::from_le_bytes(data[96..104].try_into().unwrap()) }) } }

#[expect(clippy::unwrap_used, reason = "internally-consistent serialized data")]
impl TreasurySpendParamsV1 { pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(72); b.extend_from_slice(&self.dao_escrow_bulla.to_bytes()); b.extend_from_slice(&self.recipient_pubkey.to_bytes()); b.extend_from_slice(&self.value.to_le_bytes()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 72 { return Err(ContractError::IoError(format!("TreasurySpendParamsV1: expected 72 got {}", data.len()))); } Ok(TreasurySpendParamsV1 { dao_escrow_bulla: DaoEscrowBulla(Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("TreasurySpendParamsV1: invalid dao_escrow_bulla".into()))?), recipient_pubkey: PublicKey::from_bytes(data[32..64].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("TreasurySpendParamsV1: invalid recipient_pubkey: {}", e)))?, value: u64::from_le_bytes(data[64..72].try_into().unwrap()) }) } }

#[expect(clippy::unwrap_used, reason = "internally-consistent serialized data")]
impl ExecuteClaimParamsV1 { pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(104); b.extend_from_slice(&self.dao_escrow_bulla.to_bytes()); b.extend_from_slice(&self.proposal_id.to_bytes()); b.extend_from_slice(&self.recipient_pubkey.to_bytes()); b.extend_from_slice(&self.value.to_le_bytes()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 104 { return Err(ContractError::IoError(format!("ExecuteClaimParamsV1: expected 104 got {}", data.len()))); } Ok(ExecuteClaimParamsV1 { dao_escrow_bulla: DaoEscrowBulla(Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("ExecuteClaimParamsV1: invalid dao_escrow_bulla".into()))?), proposal_id: ProposalId(Option::<pallas::Base>::from(pallas::Base::from_repr(data[32..64].try_into().unwrap())).ok_or_else(|| ContractError::IoError("ExecuteClaimParamsV1: invalid proposal_id".into()))?), recipient_pubkey: PublicKey::from_bytes(data[64..96].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("ExecuteClaimParamsV1: invalid recipient_pubkey: {}", e)))?, value: u64::from_le_bytes(data[96..104].try_into().unwrap()) }) } }

#[expect(clippy::unwrap_used, reason = "internally-consistent serialized data")]
impl CancelClaimParamsV1 { pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(64); b.extend_from_slice(&self.dao_escrow_bulla.to_bytes()); b.extend_from_slice(&self.claim_id.to_bytes()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 64 { return Err(ContractError::IoError(format!("CancelClaimParamsV1: expected 64 got {}", data.len()))); } Ok(CancelClaimParamsV1 { dao_escrow_bulla: DaoEscrowBulla(Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("CancelClaimParamsV1: invalid dao_escrow_bulla".into()))?), claim_id: ClaimId(Option::<pallas::Base>::from(pallas::Base::from_repr(data[32..64].try_into().unwrap())).ok_or_else(|| ContractError::IoError("CancelClaimParamsV1: invalid claim_id".into()))?) }) } }

#[expect(clippy::unwrap_used, reason = "internally-consistent serialized data")]
impl CapabilityProof { pub fn encode(&self) -> Result<Vec<u8>, ContractError> { let pl = SerializedLen::try_from_len(self.proof.len())?; let mut b = Vec::with_capacity(164+self.proof.len()); b.extend_from_slice(&self.capability_id); b.extend_from_slice(&self.capability_secret); b.extend_from_slice(&self.nullifier.to_bytes()); b.extend_from_slice(&self.issuer_pub); b.extend_from_slice(&self.predicate_result); b.extend_from_slice(&pl.to_le_bytes()); b.extend_from_slice(&self.proof); Ok(b) } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() < 164 { return Err(ContractError::IoError("CapabilityProof: too short".into())); } let proof_len = SerializedLen::from_le_bytes(data[160..164].try_into().unwrap()).to_usize(); // An EQUALITY, and it is exact because the field is now last. It used to be a minimum, for a reason
// that no longer holds: `EndowmentWithdrawParamsV1` carried a trailing `has_pid` byte and an optional
// `proposal_id` *after* the proof, so the buffer handed here was not the end of the frame and demanding
// equality made every governance-path call undecodable — `OBL-C150`'s class (an encoder and its decoder
// disagreeing about the length they describe) in its third instance. `EndowmentWithdrawParamsV1` no
// longer carries a `CapabilityProof` at all, and the one container that still does
// (`VoteClaimParamsV1`) puts it last, so the remainder of the frame is exactly the proof and a minimum
// would accept trailing bytes the caller appended.
let expected = proof_len.saturating_add(164); if data.len() != expected { return Err(ContractError::IoError(format!("CapabilityProof: expected {} got {}", expected, data.len()))); } Ok(CapabilityProof { capability_id: data[0..32].try_into().unwrap(), capability_secret: data[32..64].try_into().unwrap(), nullifier: IntentNullifier::from_bytes(data[64..96].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("CapabilityProof: invalid nullifier: {}", e)))?, issuer_pub: data[96..128].try_into().unwrap(), predicate_result: data[128..160].try_into().unwrap(), proof: data[164..164+proof_len].to_vec() }) } }

// --- Bridge update structs ---

impl dwow_serial::Encodable for UpdateUpdateV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode().map_err(|e| std::io::Error::other(format!("{e}")))?; w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for UpdateUpdateV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
#[expect(clippy::unwrap_used, reason = "internally-consistent serialized data")]
impl UpdateUpdateV1 { pub const FIXED: usize = 68; pub fn encode(&self) -> Result<Vec<u8>, ContractError> { let n = dwow_sdk::blockchain::SerializedLen::try_from_len(self.endowment_bytes.len())?; let mut b = Vec::with_capacity(Self::FIXED + self.endowment_bytes.len()); b.extend_from_slice(&self.bulla.to_bytes()); b.extend_from_slice(&self.owner_nullifier.to_repr()); b.extend_from_slice(&n.to_le_bytes()); b.extend_from_slice(&self.endowment_bytes); Ok(b) } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() < Self::FIXED { return Err(ContractError::IoError(format!("UpdateUpdateV1: expected at least {} bytes, got {}", Self::FIXED, data.len()))); } let bulla = DaoEscrowBulla(Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("UpdateUpdateV1: invalid bulla".into()))?); let owner_nullifier = Option::<pallas::Base>::from(pallas::Base::from_repr(data[32..64].try_into().unwrap())).ok_or_else(|| ContractError::IoError("UpdateUpdateV1: invalid owner_nullifier".into()))?; let n = dwow_sdk::blockchain::SerializedLen::from_le_bytes(data[64..68].try_into().unwrap()).to_usize(); let expected = n.saturating_add(Self::FIXED); if data.len() != expected { return Err(ContractError::IoError(format!("UpdateUpdateV1: expected {} got {}", expected, data.len()))); } let endowment_bytes = data[68..expected].to_vec(); Ok(UpdateUpdateV1 { bulla, owner_nullifier, endowment_bytes }) } }

impl dwow_serial::Encodable for InitializeUpdateV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for InitializeUpdateV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
#[expect(clippy::unwrap_used, reason = "internally-consistent serialized data")]
impl InitializeUpdateV1 { pub const ENCODED_SIZE: usize = 73; pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(Self::ENCODED_SIZE); b.extend_from_slice(&self.bulla.to_bytes()); b.extend_from_slice(&self.owner_pubkey.to_bytes()); b.push(self.mode as u8); b.extend_from_slice(&self.min_premium.to_le_bytes()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != Self::ENCODED_SIZE { return Err(ContractError::IoError(format!("InitializeUpdateV1: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len()))); } Ok(InitializeUpdateV1 { bulla: DaoEscrowBulla(Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("InitializeUpdateV1: invalid bulla".into()))?), owner_pubkey: PublicKey::from_bytes(data[32..64].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("InitializeUpdateV1: invalid owner_pubkey: {}", e)))?, mode: DaoEscrowMode::try_from(data[64])?, min_premium: u64::from_le_bytes(data[65..73].try_into().unwrap()) }) } }

impl dwow_serial::Encodable for PayPremiumUpdateV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode().map_err(|e| std::io::Error::other(format!("{e}")))?; w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for PayPremiumUpdateV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
#[expect(clippy::unwrap_used, reason = "internally-consistent serialized data")]
impl PayPremiumUpdateV1 { pub const FIXED: usize = 156; pub fn encode(&self) -> Result<Vec<u8>, ContractError> { let n = dwow_sdk::blockchain::SerializedLen::try_from_len(self.endowment_bytes.len())?; let mut b = Vec::with_capacity(Self::FIXED + self.endowment_bytes.len()); b.extend_from_slice(&self.dao_escrow_bulla.to_bytes()); b.extend_from_slice(&self.membership_note.to_bytes()); b.extend_from_slice(&self.amount.to_le_bytes()); b.extend_from_slice(&self.member_pubkey.to_bytes()); b.extend_from_slice(&self.asset_id.to_bytes()); b.extend_from_slice(&self.expiry.to_le_bytes()); b.extend_from_slice(&self.created_at.to_le_bytes()); b.extend_from_slice(&n.to_le_bytes()); b.extend_from_slice(&self.endowment_bytes); Ok(b) } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() < Self::FIXED { return Err(ContractError::IoError(format!("PayPremiumUpdateV1: expected at least {} bytes, got {}", Self::FIXED, data.len()))); } let n = dwow_sdk::blockchain::SerializedLen::from_le_bytes(data[152..156].try_into().unwrap()).to_usize(); if data.len() != Self::FIXED.saturating_add(n) { return Err(ContractError::IoError(format!("PayPremiumUpdateV1: {} endowment bytes do not fit {} total", n, data.len()))); } Ok(PayPremiumUpdateV1 { dao_escrow_bulla: DaoEscrowBulla(Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("PayPremiumUpdateV1: invalid dao_escrow_bulla".into()))?), membership_note: MembershipNote(Option::<pallas::Base>::from(pallas::Base::from_repr(data[32..64].try_into().unwrap())).ok_or_else(|| ContractError::IoError("PayPremiumUpdateV1: invalid membership_note".into()))?), amount: u64::from_le_bytes(data[64..72].try_into().unwrap()), member_pubkey: PublicKey::from_bytes(data[72..104].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("PayPremiumUpdateV1: invalid member_pubkey: {}", e)))?, asset_id: AssetId::from_bytes(data[104..136].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("PayPremiumUpdateV1: invalid asset_id: {}", e)))?, expiry: u64::from_le_bytes(data[136..144].try_into().unwrap()), created_at: u64::from_le_bytes(data[144..152].try_into().unwrap()), endowment_bytes: data[Self::FIXED..].to_vec() }) } }

impl dwow_serial::Encodable for WithdrawUpdateV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode().map_err(|e| std::io::Error::other(format!("{e}")))?; w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for WithdrawUpdateV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
#[expect(clippy::unwrap_used, reason = "internally-consistent serialized data")]
impl WithdrawUpdateV1 { pub const FIXED: usize = 52; pub fn encode(&self) -> Result<Vec<u8>, ContractError> { let n = dwow_sdk::blockchain::SerializedLen::try_from_len(self.endowment_bytes.len())?; let mut b = Vec::with_capacity(Self::FIXED + self.endowment_bytes.len()); b.extend_from_slice(&self.dao_escrow_bulla.to_bytes()); b.extend_from_slice(&self.value.to_le_bytes()); b.extend_from_slice(&self.amount.to_le_bytes()); b.extend_from_slice(&n.to_le_bytes()); b.extend_from_slice(&self.endowment_bytes); Ok(b) } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() < Self::FIXED { return Err(ContractError::IoError(format!("WithdrawUpdateV1: expected at least {} bytes, got {}", Self::FIXED, data.len()))); } let n = dwow_sdk::blockchain::SerializedLen::from_le_bytes(data[48..52].try_into().unwrap()).to_usize(); if data.len() != Self::FIXED.saturating_add(n) { return Err(ContractError::IoError(format!("WithdrawUpdateV1: {} endowment bytes do not fit {} total", n, data.len()))); } Ok(WithdrawUpdateV1 { dao_escrow_bulla: DaoEscrowBulla(Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("WithdrawUpdateV1: invalid dao_escrow_bulla".into()))?), value: u64::from_le_bytes(data[32..40].try_into().unwrap()), amount: u64::from_le_bytes(data[40..48].try_into().unwrap()), endowment_bytes: data[Self::FIXED..].to_vec() }) } }

#[expect(clippy::unwrap_used, reason = "internally-consistent serialized data")]
impl EndowmentWithdrawUpdateV1 { pub const FIXED: usize = 84; pub fn encode(&self) -> Result<Vec<u8>, ContractError> { let n = dwow_sdk::blockchain::SerializedLen::try_from_len(self.endowment_bytes.len())?; let mut b = Vec::with_capacity(Self::FIXED + self.endowment_bytes.len()); b.extend_from_slice(&self.dao_escrow_bulla.to_bytes()); b.extend_from_slice(&self.claim_id.to_bytes()); b.extend_from_slice(&self.value.to_le_bytes()); b.extend_from_slice(&self.amount.to_le_bytes()); b.extend_from_slice(&n.to_le_bytes()); b.extend_from_slice(&self.endowment_bytes); Ok(b) } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() < Self::FIXED { return Err(ContractError::IoError(format!("EndowmentWithdrawUpdateV1: expected at least {} bytes, got {}", Self::FIXED, data.len()))); } let n = dwow_sdk::blockchain::SerializedLen::from_le_bytes(data[80..84].try_into().unwrap()).to_usize(); if data.len() != Self::FIXED.saturating_add(n) { return Err(ContractError::IoError(format!("EndowmentWithdrawUpdateV1: {} endowment bytes do not fit {} total", n, data.len()))); } Ok(EndowmentWithdrawUpdateV1 { dao_escrow_bulla: DaoEscrowBulla(Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("EndowmentWithdrawUpdateV1: invalid dao_escrow_bulla".into()))?), claim_id: ClaimId(Option::<pallas::Base>::from(pallas::Base::from_repr(data[32..64].try_into().unwrap())).ok_or_else(|| ContractError::IoError("EndowmentWithdrawUpdateV1: invalid claim_id".into()))?), value: u64::from_le_bytes(data[64..72].try_into().unwrap()), amount: u64::from_le_bytes(data[72..80].try_into().unwrap()), endowment_bytes: data[Self::FIXED..].to_vec() }) } }

#[expect(clippy::unwrap_used, reason = "internally-consistent serialized data")]
impl TreasurySpendUpdateV1 { pub const FIXED: usize = 52; pub fn encode(&self) -> Result<Vec<u8>, ContractError> { let n = dwow_sdk::blockchain::SerializedLen::try_from_len(self.endowment_bytes.len())?; let mut b = Vec::with_capacity(Self::FIXED + self.endowment_bytes.len()); b.extend_from_slice(&self.dao_escrow_bulla.to_bytes()); b.extend_from_slice(&self.value.to_le_bytes()); b.extend_from_slice(&self.amount.to_le_bytes()); b.extend_from_slice(&n.to_le_bytes()); b.extend_from_slice(&self.endowment_bytes); Ok(b) } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() < Self::FIXED { return Err(ContractError::IoError(format!("TreasurySpendUpdateV1: expected at least {} bytes, got {}", Self::FIXED, data.len()))); } let n = dwow_sdk::blockchain::SerializedLen::from_le_bytes(data[48..52].try_into().unwrap()).to_usize(); if data.len() != Self::FIXED.saturating_add(n) { return Err(ContractError::IoError(format!("TreasurySpendUpdateV1: {} endowment bytes do not fit {} total", n, data.len()))); } Ok(TreasurySpendUpdateV1 { dao_escrow_bulla: DaoEscrowBulla(Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("TreasurySpendUpdateV1: invalid dao_escrow_bulla".into()))?), value: u64::from_le_bytes(data[32..40].try_into().unwrap()), amount: u64::from_le_bytes(data[40..48].try_into().unwrap()), endowment_bytes: data[Self::FIXED..].to_vec() }) } }

#[expect(clippy::unwrap_used, reason = "internally-consistent serialized data")]
impl ProposeClaimUpdateV1 { pub const ENCODED_SIZE: usize = 120; pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(Self::ENCODED_SIZE); b.extend_from_slice(&self.dao_escrow_bulla.to_bytes()); b.extend_from_slice(&self.claim_id.to_bytes()); b.extend_from_slice(&self.value.to_le_bytes()); b.extend_from_slice(&self.voting_ends_at.to_le_bytes()); b.extend_from_slice(&self.execution_deadline.to_le_bytes()); b.extend_from_slice(&self.recipient_pubkey.to_bytes()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != Self::ENCODED_SIZE { return Err(ContractError::IoError(format!("ProposeClaimUpdateV1: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len()))); } Ok(ProposeClaimUpdateV1 { dao_escrow_bulla: DaoEscrowBulla(Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("ProposeClaimUpdateV1: invalid dao_escrow_bulla".into()))?), claim_id: ClaimId(Option::<pallas::Base>::from(pallas::Base::from_repr(data[32..64].try_into().unwrap())).ok_or_else(|| ContractError::IoError("ProposeClaimUpdateV1: invalid claim_id".into()))?), value: u64::from_le_bytes(data[64..72].try_into().unwrap()), voting_ends_at: u64::from_le_bytes(data[72..80].try_into().unwrap()), execution_deadline: u64::from_le_bytes(data[80..88].try_into().unwrap()), recipient_pubkey: PublicKey::from_bytes(data[88..120].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("ProposeClaimUpdateV1: invalid recipient_pubkey: {}", e)))? }) } }

#[expect(clippy::unwrap_used, reason = "internally-consistent serialized data")]
impl VoteClaimUpdateV1 { pub const FIXED: usize = 101; pub fn encode(&self) -> Result<Vec<u8>, ContractError> { let n = dwow_sdk::blockchain::SerializedLen::try_from_len(self.proposal_bytes.len())?; let mut b = Vec::with_capacity(Self::FIXED + self.proposal_bytes.len()); b.extend_from_slice(&self.dao_escrow_bulla.to_bytes()); b.extend_from_slice(&self.claim_id.to_bytes()); b.push(self.state as u8); b.extend_from_slice(&self.vote_nullifier.to_repr()); b.extend_from_slice(&n.to_le_bytes()); b.extend_from_slice(&self.proposal_bytes); Ok(b) } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() < Self::FIXED { return Err(ContractError::IoError(format!("VoteClaimUpdateV1: expected at least {} bytes, got {}", Self::FIXED, data.len()))); } let n = dwow_sdk::blockchain::SerializedLen::from_le_bytes(data[97..101].try_into().unwrap()).to_usize(); if data.len() != Self::FIXED.saturating_add(n) { return Err(ContractError::IoError(format!("VoteClaimUpdateV1: {} proposal bytes do not fit {} total", n, data.len()))); } Ok(VoteClaimUpdateV1 { dao_escrow_bulla: DaoEscrowBulla(Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("VoteClaimUpdateV1: invalid dao_escrow_bulla".into()))?), claim_id: ClaimId(Option::<pallas::Base>::from(pallas::Base::from_repr(data[32..64].try_into().unwrap())).ok_or_else(|| ContractError::IoError("VoteClaimUpdateV1: invalid claim_id".into()))?), state: ProposalState::try_from(data[64])?, vote_nullifier: Option::<pallas::Base>::from(pallas::Base::from_repr(data[65..97].try_into().unwrap())).ok_or_else(|| ContractError::IoError("VoteClaimUpdateV1: invalid vote_nullifier".into()))?, proposal_bytes: data[Self::FIXED..].to_vec() }) } }

#[expect(clippy::unwrap_used, reason = "internally-consistent serialized data")]
impl ExecuteClaimUpdateV1 { pub const FIXED: usize = 77; pub fn encode(&self) -> Result<Vec<u8>, ContractError> { let n = dwow_sdk::blockchain::SerializedLen::try_from_len(self.proposal_bytes.len())?; let mut b = Vec::with_capacity(Self::FIXED + self.proposal_bytes.len()); b.extend_from_slice(&self.dao_escrow_bulla.to_bytes()); b.extend_from_slice(&self.proposal_id.to_bytes()); b.extend_from_slice(&self.value.to_le_bytes()); b.push(self.state as u8); b.extend_from_slice(&n.to_le_bytes()); b.extend_from_slice(&self.proposal_bytes); Ok(b) } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() < Self::FIXED { return Err(ContractError::IoError(format!("ExecuteClaimUpdateV1: expected at least {} bytes, got {}", Self::FIXED, data.len()))); } let n = dwow_sdk::blockchain::SerializedLen::from_le_bytes(data[73..77].try_into().unwrap()).to_usize(); if data.len() != Self::FIXED.saturating_add(n) { return Err(ContractError::IoError(format!("ExecuteClaimUpdateV1: {} proposal bytes do not fit {} total", n, data.len()))); } Ok(ExecuteClaimUpdateV1 { dao_escrow_bulla: DaoEscrowBulla(Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("ExecuteClaimUpdateV1: invalid dao_escrow_bulla".into()))?), proposal_id: ProposalId(Option::<pallas::Base>::from(pallas::Base::from_repr(data[32..64].try_into().unwrap())).ok_or_else(|| ContractError::IoError("ExecuteClaimUpdateV1: invalid proposal_id".into()))?), value: u64::from_le_bytes(data[64..72].try_into().unwrap()), state: ProposalState::try_from(data[72])?, proposal_bytes: data[Self::FIXED..].to_vec() }) } }

#[expect(clippy::unwrap_used, reason = "internally-consistent serialized data")]
impl CancelClaimUpdateV1 { pub const FIXED: usize = 69; pub fn encode(&self) -> Result<Vec<u8>, ContractError> { let n = dwow_sdk::blockchain::SerializedLen::try_from_len(self.proposal_bytes.len())?; let mut b = Vec::with_capacity(Self::FIXED + self.proposal_bytes.len()); b.extend_from_slice(&self.dao_escrow_bulla.to_bytes()); b.extend_from_slice(&self.claim_id.to_bytes()); b.push(self.state as u8); b.extend_from_slice(&n.to_le_bytes()); b.extend_from_slice(&self.proposal_bytes); Ok(b) } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() < Self::FIXED { return Err(ContractError::IoError(format!("CancelClaimUpdateV1: expected at least {} bytes, got {}", Self::FIXED, data.len()))); } let n = dwow_sdk::blockchain::SerializedLen::from_le_bytes(data[65..69].try_into().unwrap()).to_usize(); if data.len() != Self::FIXED.saturating_add(n) { return Err(ContractError::IoError(format!("CancelClaimUpdateV1: {} proposal bytes do not fit {} total", n, data.len()))); } Ok(CancelClaimUpdateV1 { dao_escrow_bulla: DaoEscrowBulla(Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("CancelClaimUpdateV1: invalid dao_escrow_bulla".into()))?), claim_id: ClaimId(Option::<pallas::Base>::from(pallas::Base::from_repr(data[32..64].try_into().unwrap())).ok_or_else(|| ContractError::IoError("CancelClaimUpdateV1: invalid claim_id".into()))?), state: ProposalState::try_from(data[64])?, proposal_bytes: data[Self::FIXED..].to_vec() }) } }

impl Membership {
    pub const ENCODED_SIZE: usize = 153;
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(153);
        b.push(self.version);
        b.extend_from_slice(&self.note.to_bytes());
        b.extend_from_slice(&self.dao_escrow_bulla.to_bytes());
        b.extend_from_slice(&self.member_pubkey.to_bytes());
        b.extend_from_slice(&self.value.to_le_bytes());
        b.extend_from_slice(&self.asset_id.to_bytes());
        b.extend_from_slice(&self.expiry.to_le_bytes());
        b.extend_from_slice(&self.created_at.to_le_bytes());
        b
    }
    #[expect(clippy::unwrap_used, reason = "internally-consistent serialized data")]
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 153 { return Err(ContractError::IoError(format!("Membership: expected 153 bytes, got {}", data.len()))); }
        Ok(Membership { version: data[0], note: MembershipNote(Option::<pallas::Base>::from(pallas::Base::from_repr(data[1..33].try_into().unwrap())).ok_or_else(|| ContractError::IoError("Membership: invalid note".into()))?), dao_escrow_bulla: DaoEscrowBulla(Option::<pallas::Base>::from(pallas::Base::from_repr(data[33..65].try_into().unwrap())).ok_or_else(|| ContractError::IoError("Membership: invalid dao_escrow_bulla".into()))?), member_pubkey: PublicKey::from_bytes(data[65..97].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("Membership: invalid member_pubkey: {}", e)))?, value: u64::from_le_bytes(data[97..105].try_into().unwrap()), asset_id: AssetId::from_bytes(data[105..137].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("Membership: invalid asset_id: {}", e)))?, expiry: u64::from_le_bytes(data[137..145].try_into().unwrap()), created_at: u64::from_le_bytes(data[145..153].try_into().unwrap()) })
    }
}

// The `CapabilityRequirement`, `RegisterCapabilityRequirementUpdateV1`, `ResolveDisputeUpdateV1` and
// `DeactivateCapabilityRequirementUpdateV1` codecs lived here. All four described records for endpoints
// that no longer exist, and `CapabilityRequirement` had no `Encodable`/`Decodable` impl at all, so it
// could not have travelled through a transaction even when its endpoint did.
