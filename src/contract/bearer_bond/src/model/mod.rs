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

//! Bearer Bond data models — Fixed-Interest Staking Model
//!
//! A stake coin is a tradeable capital position. The holder provides capital
//! to the issuer and earns a fixed interest rate set at series creation.
//! Interest is computed deterministically from on-chain state — no issuer
//! reporting is needed and the holder's privacy is preserved.
//!
//! Maturity is ZK-committed in the coin commitment, making it a
//! cryptographically bound property of the bond token.
//!
//! ## Lifecycle
//!
//! - IssueStakeV1 (0x00): Issuer creates staking pool, sets terms, receives
//!   capital, mints stake coins to the staker.
//! - TransferStakeV1 (0x01): Holder transfers stake position to new holder.
//!   Unclaimed interest travels with the coin — the new coin
//!   preserves `last_claim_block`.
//! - RequestInterestV1 (0x02): Holder requests interest payment (prove ownership).
//! - PayInterestV1 (0x08): Issuer pays a pending interest claim.
//!   Stake coin persists (not consumed).
//! - EmergencyUnstakeV1 (0x03): Holder exits before maturity when coverage
//!   falls below the minimum threshold.
//! - UnstakeV1 (0x04): Burn stake coin, receive principal plus any unclaimed
//!   interest back. Enforced at or after maturity.
//! - BurnStakeV1 (0x05): Issuer retires staking pool.

use dwow_sdk::crypto::poseidon_hash;
use dwow_sdk::{
    crypto::{pasta_prelude::Group, ContractId, MerkleNode},
    pasta::pallas,
};
use dwow_serial::{SerialDecodable, SerialEncodable};

// ============================================================================
// CONSTANTS
// ============================================================================

/// Minimum claim value (1 unit — prevents dust claims)
pub const DEFAULT_MIN_CLAIM: u64 = 1;

/// Maximum principal value (prevent overflow)
pub const MAX_PRINCIPAL: u64 = 1_000_000_000_000;

// ============================================================================
// COIN ATTRIBUTES (for ZK circuit coin commitment)
// ============================================================================

/// Coin attributes that the ZK circuits (Burn_V1, BlindOutput_V1, Redeem_V1)
/// commit to. The coin commitment is:
/// `poseidon_hash([public_key, value, token_id, spend_hook, user_data, blind, maturity_block])`
///
/// Maturity is ZK-committed so it becomes a cryptographically bound property
/// of the bond token — the issuer cannot alter it after issuance.
///
/// Principal, last_claim_block, and issuer_contract remain as plaintext on
/// `BondCoin` since they don't need cryptographic binding for security.
#[derive(Debug, Clone)]
pub struct CoinAttributes {
    /// Poseidon hash of the owner's secret
    pub public_key: pallas::Base,
    /// Coin value
    pub value: u64,
    /// Token ID
    pub token_id: pallas::Base,
    /// Spend hook
    pub spend_hook: pallas::Base,
    /// User data
    pub user_data: pallas::Base,
    /// Coin blinding factor
    pub blind: pallas::Base,
    /// Block height when stake matures (ZK-committed)
    pub maturity_block: u64,
}

impl CoinAttributes {
    /// Compute the coin commitment (Poseidon hash of all attributes).
    pub fn to_coin(&self) -> pallas::Base {
        dwow_sdk::crypto::poseidon_hash([
            self.public_key,
            pallas::Base::from(self.value),
            self.token_id,
            self.spend_hook,
            self.user_data,
            self.blind,
            pallas::Base::from(self.maturity_block),
        ])
    }
}

// ============================================================================
// NULLIFIER
// ============================================================================

/// Nullifier for double-spend prevention.
#[derive(Debug, Clone, Copy, Eq, PartialEq, SerialEncodable, SerialDecodable)]
pub struct Nullifier(pallas::Base);

impl Nullifier {
    pub fn new(secret: pallas::Base, coin: pallas::Base) -> Self {
        Nullifier(dwow_sdk::crypto::poseidon_hash([secret, coin]))
    }

    pub fn inner(&self) -> pallas::Base {
        self.0
    }

    pub fn from_base(base: pallas::Base) -> Self {
        Nullifier(base)
    }
}

// ============================================================================
// BOND SERIES INFO
// ============================================================================

/// Status of a bond series.
#[derive(Debug, Clone, Copy, PartialEq, Eq, SerialEncodable, SerialDecodable)]
#[repr(u8)]
pub enum SeriesStatus {
    /// Series is active — stakes, transfers, and interest claims are allowed
    Active = 0,
    /// Series has been voided due to coverage failure — only emergency unstake allowed
    Voided = 1,
    /// Series has reached maturity — only unstake allowed
    Matured = 2,
}

/// Per-series configuration stored in the `bonds_info` tree.
///
/// Keyed by `poseidon_hash(series_token_id)`.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct BondSeriesInfo {
    /// Token ID of the staking pool series
    pub series_token_id: pallas::Base,
    /// Annual interest rate in basis points (e.g. 500 = 5%)
    pub interest_rate_bps: u64,
    /// Block height when the series matures
    pub maturity_block: u64,
    /// Current status of the series
    pub status: SeriesStatus,
    /// Issuer contract ID
    pub issuer_contract: ContractId,
    /// Total staked principal across all coins in this series
    pub total_staked: u64,
}

// ============================================================================
// STAKE COIN
// ============================================================================

/// An on-chain stake coin.
///
/// Stake coins are tracked in a Merkle tree. Each coin carries staking
/// metadata: value_commit (Pedersen), last_claim_block, maturity_block, and issuer_contract.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct BondCoin {
    /// Pedersen commitment of the principal value (additively homomorphic)
    pub value_commit: pallas::Point,
    /// Commitment of the stake pool series token_id (Poseidon hash)
    pub token_commit: pallas::Base,
    /// Nullifier — proves the coin has not been spent
    pub nullifier: Nullifier,
    /// Merkle root at the time the coin was created
    pub merkle_root: MerkleNode,
    /// Encrypted user data field
    pub user_data_enc: pallas::Base,
    /// Spend hook — set to the BondContract itself to prevent raw PN transfers
    pub spend_hook: pallas::Base,
    /// Signature public key (Poseidon hash of secret, as field element)
    pub signature_public: pallas::Base,
    /// Block height of last interest claim
    pub last_claim_block: u64,
    /// Block height when stake matures (can be unstaked)
    pub maturity_block: u64,
    /// Issuer contract ID
    pub issuer_contract: ContractId,
}

impl Default for BondCoin {
    fn default() -> Self {
        BondCoin {
            value_commit: pallas::Point::identity(),
            token_commit: pallas::Base::zero(),
            nullifier: Nullifier::from_base(pallas::Base::zero()),
            merkle_root: MerkleNode::from(pallas::Base::zero()),
            user_data_enc: pallas::Base::zero(),
            spend_hook: pallas::Base::zero(),
            signature_public: pallas::Base::zero(),
            last_claim_block: 0,
            maturity_block: 0,
            issuer_contract: ContractId::from(pallas::Base::zero()),
        }
    }
}

/// Client-side witness data for ZK proof generation.
///
/// These fields are NEVER serialized on-chain. They are passed from the
/// client to the ZK prover alongside the on-chain coin data.
#[derive(Debug, Clone)]
pub struct BondCoinWitness {
    /// Principal value
    pub principal: u64,
    /// Token ID
    pub token_id: pallas::Base,
    /// Block height of last interest claim
    pub last_claim_block: u64,
    /// Block height when stake matures (ZK-committed via CoinAttributes)
    pub maturity_block: u64,
    /// Issuer contract ID
    pub issuer_contract: ContractId,
    /// User data
    pub user_data: pallas::Base,
    /// Coin blind
    pub coin_blind: pallas::Base,
    /// Value blind (for Pedersen value commitment)
    pub value_blind: pallas::Scalar,
    /// Token blind (for Poseidon token commitment)
    pub token_blind: pallas::Base,
}

// ============================================================================
// ISSUE STAKE
// ============================================================================

/// Parameters for IssueStakeV1 — create a new staking position.
///
/// Maturity is derived from the bond series (BondSeriesInfo), not set by
/// the wallet at issuance time.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct IssueStakeParamsV1 {
    /// Minimum claim value (dust protection)
    pub min_claim: u64,
    /// Issuer contract ID
    pub issuer_contract: ContractId,
    /// Token ID for the stake pool series
    pub token_id: pallas::Base,
    /// Initial stake coin
    pub coin: BondCoin,
}

/// State update for IssueStakeV1.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct IssueStakeUpdateV1 {
    pub coins: Vec<BondCoin>,
}

// ============================================================================
// TRANSFER STAKE
// ============================================================================

/// On-chain input for TransferStakeV1 — proves ownership of an existing stake.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct BondInput {
    /// Pedersen commitment of the principal
    pub value_commit: pallas::Point,
    /// Token commitment
    pub token_commit: pallas::Base,
    /// Nullifier proving coin is not double-spent
    pub nullifier: Nullifier,
    /// Merkle root proving coin existed
    pub merkle_root: MerkleNode,
    /// Encrypted user data
    pub user_data_enc: pallas::Base,
    /// Spend hook
    pub spend_hook: pallas::Base,
    /// Signature public key
    pub signature_public: pallas::Base,
}

/// Client-side witness for transfer input.
#[derive(Debug, Clone)]
pub struct BondInputWitness {
    pub principal: u64,
    pub token_id: pallas::Base,
    pub last_claim_block: u64,
    pub maturity_block: u64,
    pub issuer_contract: ContractId,
    pub user_data: pallas::Base,
    pub coin_blind: pallas::Base,
    pub value_blind: pallas::Scalar,
    pub token_blind: pallas::Base,
    pub leaf_position: u64,
    pub merkle_path: Vec<MerkleNode>,
    pub secret: pallas::Base,
    pub ephemeral_signature_secret: pallas::Base,
}

/// Parameters for TransferStakeV1 — burn old stake, create new with same metadata.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct TransferStakeParamsV1 {
    pub inputs: Vec<BondInput>,
    pub outputs: Vec<BondCoin>,
}

/// State update for TransferStakeV1.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct TransferStakeUpdateV1 {
    pub nullifiers: Vec<Nullifier>,
    pub coins: Vec<BondCoin>,
}

// ============================================================================
// REQUEST INTEREST
// ============================================================================

/// Status of an interest claim request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, SerialEncodable, SerialDecodable)]
#[repr(u8)]
pub enum ClaimStatus {
    /// Claim is awaiting payment from the issuer
    Pending = 0,
    /// Claim has been paid
    Paid = 1,
}

/// An on-chain record of a holder's interest claim request.
///
/// Like a physical bond coupon — the holder presents it, the issuer pays
/// against it. Stored in the `bonds_info` tree keyed by
/// `(token_commit, claim_block)`.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct RequestedClaim {
    /// Interest amount owed (computed deterministically)
    pub interest_amount: u64,
    /// Holder's one-time key for receiving payment
    pub payment_key: pallas::Base,
    /// Claim status
    pub status: ClaimStatus,
}

/// Parameters for RequestInterestV1 — holder requests interest payment.
///
/// The holder proves bond ownership (via Burn_V1 ZK proof) and provides
/// a fresh one-time key for the issuer to pay to. This is like presenting
/// a physical bond coupon — the burden is on the holder to ask.
///
/// Interest is computed deterministically from on-chain state:
/// ```text
/// interest = principal * interest_rate_bps * blocks_elapsed / (BP_PRECISION * BLOCKS_PER_YEAR)
/// ```
/// where `blocks_elapsed = current_block - last_claim_block`.
///
/// `last_claim_block` is NOT updated yet — only when the issuer pays.
/// The pending claim record blocks duplicate claims for the same period.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct RequestInterestParamsV1 {
    /// The stake coin being claimed against (not consumed)
    pub bond_input: BondInput,
    /// Current block height
    pub claim_block: u64,
    /// Fresh one-time key for the issuer to pay to
    pub payment_key: pallas::Base,
    /// Minimum claim threshold (dust protection)
    pub min_claim: u64,
}

/// State update for RequestInterestV1 — stores the claim record on-chain.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct RequestInterestUpdateV1 {
    /// Token commit of the bond being claimed against
    pub bond_token_commit: pallas::Base,
    /// Block height of the claim
    pub claim_block: u64,
    /// The claim record to store
    pub claim: RequestedClaim,
}

// ============================================================================
// PAY INTEREST
// ============================================================================

/// Parameters for PayInterestV1 — issuer pays a pending interest claim.
///
/// The issuer reads the claim record, verifies reserves are sufficient
/// (via latest CoverageReport), and creates a fresh payment coin
/// (BlindOutput_V1) addressed to the holder's one-time `payment_key`.
/// Updates `last_claim_block` on the stake coin and marks the claim Paid.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct PayInterestParamsV1 {
    /// Token commit identifying the bond
    pub bond_token_commit: pallas::Base,
    /// Block height of the claim being paid
    pub claim_block: u64,
    /// Payment coin (BlindOutput_V1 to holder's payment_key)
    pub interest_coin: BondCoin,
}

/// State update for PayInterestV1 — updates stake coin and stores payment.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct PayInterestUpdateV1 {
    /// Stake coin with updated last_claim_block
    pub updated_coin: BondCoin,
    /// Payment coin (BlindOutput_V1)
    pub interest_coin: BondCoin,
    /// Token commit of the bond
    pub bond_token_commit: pallas::Base,
    /// Block height of the claim
    pub claim_block: u64,
}

// ============================================================================
// EMERGENCY UNSTAKE
// ============================================================================

/// Parameters for EmergencyUnstakeV1 — unstake before maturity when coverage fails.
///
/// Only valid when the latest coverage report shows
/// `coverage_ratio_bps < MIN_COVERAGE_RATIO_BPS` for the series.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct EmergencyUnstakeParamsV1 {
    pub bond_input: BondInput,
    /// Coverage report proving the series is under-collateralized
    pub coverage_report: CoverageReport,
}

/// State update for EmergencyUnstakeV1.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct EmergencyUnstakeUpdateV1 {
    pub nullifiers: Vec<Nullifier>,
    /// Receipt coin proving emergency unstake
    pub receipt_coin: BondCoin,
}

// ============================================================================
// UNSTAKE
// ============================================================================

/// Parameters for UnstakeV1 — withdraw principal at maturity.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct UnstakeParamsV1 {
    pub bond_input: BondInput,
    /// Current block height (public input, verified by host)
    pub current_block: u64,
}

/// State update for UnstakeV1.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct UnstakeUpdateV1 {
    pub nullifiers: Vec<Nullifier>,
    /// Receipt coin proving unstake
    pub receipt_coin: BondCoin,
}

// ============================================================================
// BURN STAKE
// ============================================================================

/// Parameters for BurnStakeV1 — issuer retires staking pool.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct BurnStakeParamsV1 {
    pub inputs: Vec<BondInput>,
}

/// State update for BurnStakeV1.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct BurnStakeUpdateV1 {
    pub nullifiers: Vec<Nullifier>,
}

// ============================================================================
// INTEREST CALCULATION (host-side helper)
// ============================================================================

/// Basis point precision (10000 = 100%).
pub const BP_PRECISION: u64 = 10000;

/// Approximate blocks per year (2-second block time: ~15_768_000 blocks/year).
pub const BLOCKS_PER_YEAR: u64 = 15_768_000;

/// Calculate deterministic interest accrued on a stake position.
///
/// ```text
/// interest = principal * interest_rate_bps * blocks_elapsed / (BP_PRECISION * BLOCKS_PER_YEAR)
/// ```
///
/// Returns `None` on overflow or if `blocks_elapsed` is zero.
pub fn calculate_interest(
    principal: u64,
    interest_rate_bps: u64,
    blocks_elapsed: u64,
) -> Option<u64> {
    if blocks_elapsed == 0 {
        return Some(0);
    }
    let numerator = (principal as u128) * (interest_rate_bps as u128) * (blocks_elapsed as u128);
    let denominator = (BP_PRECISION as u128) * (BLOCKS_PER_YEAR as u128);
    let result = numerator / denominator;
    if result > u64::MAX as u128 {
        return None;
    }
    Some(result as u64)
}

// ============================================================================
// PROVE COVERAGE (GOVERNANCE)
// ============================================================================

/// Parameters for ProveCoverageV1 — proves solvency (callable by issuer or holder).
///
/// The ZK circuit (ProveCoverage_V1) uses `base_div` to compute
/// `coverage_ratio_bps = reserve_amount / (total_outstanding + total_interest_obligation) * 10000`
/// and constrains it against the submitted value. The entrypoint
/// independently verifies `reserve_amount >= total_outstanding + total_interest_obligation`
/// (>= 100% coverage required for both principal and interest).
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ProveCoverageParamsV1 {
    /// Staking pool series identifier
    pub series_token_id: pallas::Base,
    /// Total staked principal across all stake coins in the series
    pub total_outstanding: u64,
    /// Total accrued interest obligation across all outstanding stakes
    pub total_interest_obligation: u64,
    /// Issuer's reserve balance (must be >= total_outstanding + total_interest_obligation)
    pub reserve_amount: u64,
    /// coverage_ratio_bps = reserve_amount / (total_outstanding + total_interest_obligation) * 10000
    pub coverage_ratio_bps: u64,
    /// Block height of this report
    pub report_block: u64,
    /// ZK proof (ProveCoverage_V1 circuit)
    pub proof: Vec<u8>,
}

/// On-chain record of a coverage report.
///
/// Stored in the `bonds_info` tree keyed by
/// `poseidon_hash(series_token_id, report_block)`.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CoverageReport {
    /// Staking pool series identifier
    pub series_token_id: pallas::Base,
    /// Total staked principal at time of report
    pub total_outstanding: u64,
    /// Total interest obligation across all outstanding stakes
    pub total_interest_obligation: u64,
    /// Issuer's reserve balance at time of report
    pub reserve_amount: u64,
    /// Coverage ratio in basis points (10000 = 100%)
    /// Computed as: reserve_amount / (total_outstanding + total_interest_obligation) * 10000
    pub coverage_ratio_bps: u64,
    /// Block height of this report
    pub report_block: u64,
}

/// State update for ProveCoverageV1.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ProveCoverageUpdateV1 {
    pub report: CoverageReport,
}
