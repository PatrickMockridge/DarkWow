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

//! Bearer Bond data models — Profit-Share Staking Model
//!
//! A stake coin is a tradeable capital position. The holder provides capital
//! to the issuer, the issuer does work, and profits are shared pro-rata.
//! If there are no profits, there are no payouts — risk is shared, riba is
//! avoided, and liquidity crises are prevented by tying distributions to
//! actual revenue.
//!
//! ## Lifecycle
//!
//! - IssueStakeV1 (0x00): Issuer creates staking pool, sets terms, receives
//!   capital, mints stake coins to the staker.
//! - TransferStakeV1 (0x01): Holder transfers stake position to new holder.
//!   Unclaimed profit distributions travel with the coin — the new coin
//!   preserves `last_claim_block`.
//! - DeclareProfitsV1 (0x02): Issuer declares a profit amount for the series
//!   (amount + block range).
//! - ClaimProfitsV1 (0x03): Holder claims pro-rata share of declared but
//!   unclaimed profits. Stake coin persists (not consumed).
//! - UnstakeV1 (0x04): Burn stake coin, receive principal plus any unclaimed
//!   profits back.
//! - BurnStakeV1 (0x05): Issuer retires staking pool.

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
/// `poseidon_hash([public_key, value, token_id, spend_hook, user_data, blind])`
///
/// Bond metadata (principal, last_claim_block, maturity_block, issuer_contract)
/// is NOT included in the coin commitment — it lives as plaintext in `BondCoin`.
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
// PROFIT DECLARATION
// ============================================================================

/// A profit declaration by the issuer.
///
/// Issuer declares: "between start_block and end_block, this series earned
/// `profit_amount` in revenue." Stakers claim their pro-rata share.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ProfitDeclaration {
    /// Token ID of the staking pool series
    pub series_token_id: pallas::Base,
    /// Total profit amount declared
    pub profit_amount: u64,
    /// Start block of the earning period
    pub start_block: u64,
    /// End block of the earning period
    pub end_block: u64,
}

// ============================================================================
// STAKE COIN
// ============================================================================

/// An on-chain stake coin.
///
/// Stake coins are tracked in a Merkle tree. Each coin carries staking
/// metadata: principal, last_claim_block, maturity_block, and issuer_contract.
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
    /// Principal value (staked amount in smallest units)
    pub principal: u64,
    /// Block height of last profit claim
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
            principal: 0,
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
    /// Block height of last profit claim
    pub last_claim_block: u64,
    /// Block height when stake matures
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

/// Parameters for IssueStakeV1 — create a new staking pool.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct IssueStakeParamsV1 {
    /// Stake principal
    pub principal: u64,
    /// Block height when stake matures
    pub maturity_block: u64,
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
// DECLARE PROFITS
// ============================================================================

/// Parameters for DeclareProfitsV1 — issuer declares a profit distribution.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct DeclareProfitsParamsV1 {
    /// Token ID of the staking pool series
    pub series_token_id: pallas::Base,
    /// Total profit amount being declared
    pub profit_amount: u64,
    /// Start block of the earning period
    pub start_block: u64,
    /// End block of the earning period
    pub end_block: u64,
}

/// State update for DeclareProfitsV1.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct DeclareProfitsUpdateV1 {
    pub declaration: ProfitDeclaration,
}

// ============================================================================
// CLAIM PROFITS
// ============================================================================

/// Parameters for ClaimProfitsV1 — claim pro-rata share of declared profits.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ClaimProfitsParamsV1 {
    /// The stake coin being claimed against (not consumed)
    pub bond_input: BondInput,
    /// Current block height (public input, verified by host)
    pub claim_block: u64,
    /// Minimum claim threshold (dust protection)
    pub min_claim: u64,
    /// Profit share amount (computed on-chain from declarations)
    pub profit_share: u64,
}

/// State update for ClaimProfitsV1 — updates last_claim_block on the stake coin.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ClaimProfitsUpdateV1 {
    /// Updated stake coin with new last_claim_block
    pub updated_coin: BondCoin,
    /// Profit payout coin (minted to holder)
    pub profit_coin: BondCoin,
}

// ============================================================================
// UNSTAKE
// ============================================================================

/// Parameters for UnstakeV1 — withdraw principal at maturity.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct UnstakeParamsV1 {
    pub bond_input: BondInput,
    /// Total payout = principal + unclaimed profits
    pub payout: u64,
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
// PROFIT SHARE CALCULATION (host-side helper)
// ============================================================================

/// Calculate pro-rata profit share for a stake coin.
///
/// ```text
/// share = staked × declared_profit / total_staked
/// ```
///
/// Returns `None` on overflow or if `total_staked` is zero.
pub fn calculate_profit_share(
    staked: u64,
    total_staked: u64,
    declared_profit: u64,
) -> Option<u64> {
    if total_staked == 0 {
        return None;
    }
    let numerator = (staked as u128) * (declared_profit as u128);
    let result = numerator / (total_staked as u128);
    if result > u64::MAX as u128 {
        return None;
    }
    Some(result as u64)
}

// ============================================================================
// PROVE COVERAGE (GOVERNANCE)
// ============================================================================

/// Parameters for ProveCoverageV1 — issuer proves solvency.
///
/// The ZK circuit (ProveCoverage_V1) uses `base_div` to compute
/// `coverage_ratio_bps = reserve_amount / total_outstanding * 10000`
/// and constrains it against the submitted value. The entrypoint
/// independently verifies `reserve_amount >= total_outstanding`
/// (>= 100% coverage required).
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ProveCoverageParamsV1 {
    /// Staking pool series identifier
    pub series_token_id: pallas::Base,
    /// Total staked principal across all stake coins in the series
    pub total_outstanding: u64,
    /// Issuer's reserve balance (must be >= total_outstanding)
    pub reserve_amount: u64,
    /// coverage_ratio_bps = reserve_amount / total_outstanding * 10000
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
    /// Issuer's reserve balance at time of report
    pub reserve_amount: u64,
    /// Coverage ratio in basis points (10000 = 100%)
    pub coverage_ratio_bps: u64,
    /// Block height of this report
    pub report_block: u64,
}

/// State update for ProveCoverageV1.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ProveCoverageUpdateV1 {
    pub report: CoverageReport,
}
