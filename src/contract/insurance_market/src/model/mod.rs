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

//! Insurance Market Contract Data Models

use darkfi_sdk::{
    crypto::{poseidon_hash, PublicKey},
    pasta::pallas,
};
use darkfi_serial::{SerialDecodable, SerialEncodable};

use crate::error::InsuranceMarketError;

// ============================================================================
// STATE TYPES
// ============================================================================

/// Unique risk type identifier (Poseidon hash of risk parameters)
pub type RiskTypeId = pallas::Base;

/// Unique insurance market identifier
pub type MarketId = pallas::Base;

/// Unique coverage identifier
pub type CoverageId = pallas::Base;

/// Unique claim identifier
pub type ClaimId = pallas::Base;

/// Unique underwriter identifier
pub type UnderwriterId = pallas::Base;

/// Risk categories
#[derive(Debug, Clone, Copy, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub enum RiskCategory {
    SmartContractHack = 0,
    OracleManipulation = 1,
    KeyManagementFailure = 2,
    ProtocolInsolvency = 3,
    StablecoinDepeg = 4,
    LiquidityCrunch = 5,
    GovernanceCapture = 6,
    RegulatoryClampdown = 7,
    Custom = 8,
}

impl TryFrom<u8> for RiskCategory {
    type Error = darkfi_sdk::error::ContractError;

    fn try_from(b: u8) -> Result<Self, Self::Error> {
        match b {
            0 => Ok(Self::SmartContractHack),
            1 => Ok(Self::OracleManipulation),
            2 => Ok(Self::KeyManagementFailure),
            3 => Ok(Self::ProtocolInsolvency),
            4 => Ok(Self::StablecoinDepeg),
            5 => Ok(Self::LiquidityCrunch),
            6 => Ok(Self::GovernanceCapture),
            7 => Ok(Self::RegulatoryClampdown),
            8 => Ok(Self::Custom),
            _ => Err(darkfi_sdk::error::ContractError::InvalidFunction),
        }
    }
}

/// Coverage state
#[derive(Debug, Clone, Copy, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub enum CoverageState {
    Active = 0,
    Expired = 1,
    Claimed = 2,
    Cancelled = 3,
}

impl TryFrom<u8> for CoverageState {
    type Error = darkfi_sdk::error::ContractError;

    fn try_from(b: u8) -> Result<Self, Self::Error> {
        match b {
            0 => Ok(Self::Active),
            1 => Ok(Self::Expired),
            2 => Ok(Self::Claimed),
            3 => Ok(Self::Cancelled),
            _ => Err(darkfi_sdk::error::ContractError::InvalidFunction),
        }
    }
}

/// Claim state
#[derive(Debug, Clone, Copy, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub enum ClaimState {
    Filed = 0,
    Resolved = 1,
    Rejected = 2,
    Paid = 3,
}

impl TryFrom<u8> for ClaimState {
    type Error = darkfi_sdk::error::ContractError;

    fn try_from(b: u8) -> Result<Self, Self::Error> {
        match b {
            0 => Ok(Self::Filed),
            1 => Ok(Self::Resolved),
            2 => Ok(Self::Rejected),
            3 => Ok(Self::Paid),
            _ => Err(darkfi_sdk::error::ContractError::InvalidFunction),
        }
    }
}

// ============================================================================
// CORE DATA STRUCTURES
// ============================================================================

/// Represents a registered risk type
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct RiskType {
    /// Unique risk type identifier
    pub id: RiskTypeId,
    /// Risk category
    pub category: RiskCategory,
    /// Human-readable description
    pub description: Vec<u8>,
    /// Base premium rate (basis points)
    pub base_premium_rate: u32,
    /// Minimum bond requirement (basis points of coverage)
    pub min_bond_rate: u32,
    /// Oracle public key for this risk type
    pub oracle_pubkey: PublicKey,
    /// Whether this risk type is active
    pub active: bool,
    /// Block height when registered
    pub created_at: u64,
}

/// Represents an insurance market for a specific risk
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct InsuranceMarket {
    /// Unique market identifier
    pub id: MarketId,
    /// Risk type being insured
    pub risk_type: RiskTypeId,
    /// Current premium rate (basis points)
    pub premium_rate: u32,
    /// Total coverage available
    pub total_coverage: u64,
    /// Total coverage sold
    pub coverage_sold: u64,
    /// Coverage period in blocks
    pub coverage_period: u64,
    /// Deductible amount
    pub deductible: u64,
    /// Maximum coverage per buyer
    pub max_coverage_per_buyer: u64,
    /// Market state (active/inactive)
    pub active: bool,
    /// Block height when market was created
    pub created_at: u64,
    /// Block height when market closes (0 = no close)
    pub closes_at: u64,
    /// Required capability ID for underwriters (None = any underwriter)
    pub required_underwriter_capability: Option<[u8; 32]>,
    /// Required capability ID for buyers (None = any buyer)
    pub required_buyer_capability: Option<[u8; 32]>,
    /// Required DAG ID for coverage tier qualification (None = no DAG)
    pub required_dag_id: Option<[u8; 32]>,
}

/// Represents an underwriter's position
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Underwriter {
    /// Unique underwriter identifier
    pub id: UnderwriterId,
    /// Underwriter public key
    pub owner: PublicKey,
    /// Market this underwriter operates in
    pub market_id: MarketId,
    /// Bond amount posted (at risk)
    pub bond_amount: u64,
    /// Total coverage provided by this underwriter (max they can sell)
    pub coverage_provided: u64,
    /// Coverage sold so far (decremented from coverage_provided)
    pub coverage_sold: u64,
    /// Premiums earned (available for withdrawal)
    pub earned_premiums: u64,
    /// Claims paid out
    pub claims_paid: u64,
    /// Slash count (for performance scoring)
    pub slash_count: u32,
    /// Performance score (higher = better)
    pub performance_score: u32,
    /// Whether underwriter is active
    pub active: bool,
    /// Block height when registered
    pub created_at: u64,
}

/// Represents an insurance coverage policy
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Coverage {
    /// Unique coverage identifier
    pub id: CoverageId,
    /// Market this coverage is from
    pub market_id: MarketId,
    /// Buyer public key
    pub buyer: PublicKey,
    /// Underwriter providing this coverage
    pub underwriter_id: UnderwriterId,
    /// Coverage amount
    pub amount: u64,
    /// Premium paid
    pub premium_paid: u64,
    /// Coverage state
    pub state: CoverageState,
    /// Block height when coverage starts
    pub starts_at: u64,
    /// Block height when coverage expires
    pub expires_at: u64,
    /// Claim identifier if claimed
    pub claim_id: Option<ClaimId>,
}

/// Represents an insurance claim
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Claim {
    /// Unique claim identifier
    pub id: ClaimId,
    /// Coverage this claim is against
    pub coverage_id: CoverageId,
    /// Market this claim is in
    pub market_id: MarketId,
    /// Claim amount requested
    pub amount: u64,
    /// Actual payout amount (after deductible)
    pub payout: u64,
    /// Claim state
    pub state: ClaimState,
    /// Evidence/description of the claim
    pub evidence: Vec<u8>,
    /// Oracle attestation
    pub attestation: Vec<u8>,
    /// Oracle signature
    pub oracle_signature: pallas::Base,
    /// Resolved at block height
    pub resolved_at: u64,
}

/// Endowment pool for LP capital
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct EndowmentPool {
    /// Unique pool identifier
    pub id: pallas::Base,
    /// Market this pool backs
    pub market_id: MarketId,
    /// Total capital in pool
    pub total_capital: u64,
    /// Capital currently deployed (backing coverage)
    pub deployed_capital: u64,
    /// LP shares outstanding
    pub total_shares: u64,
    /// Historical returns for APY calculation
    pub returns_history: Vec<u64>,
    /// Block height when created
    pub created_at: u64,
}

// ============================================================================
// PARAMETER TYPES
// ============================================================================

/// Parameters for `InsuranceMarket::RegisterRiskTypeV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct RegisterRiskTypeParamsV1 {
    pub category: RiskCategory,
    pub description: Vec<u8>,
    pub base_premium_rate: u32,
    pub min_bond_rate: u32,
    pub oracle_pubkey: PublicKey,
}

/// State update for `RegisterRiskTypeV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct RegisterRiskTypeUpdateV1 {
    pub risk_type_id: RiskTypeId,
    pub category: RiskCategory,
    pub description: Vec<u8>,
    pub base_premium_rate: u32,
    pub min_bond_rate: u32,
    pub oracle_pubkey: PublicKey,
    pub created_at: u64,
}

/// Parameters for `InsuranceMarket::CreateMarketV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CreateMarketParamsV1 {
    pub risk_type_id: RiskTypeId,
    pub initial_premium_rate: u32,
    pub total_coverage: u64,
    pub coverage_period: u64,
    pub deductible: u64,
    pub max_coverage_per_buyer: u64,
    pub closes_at: u64,
    pub required_underwriter_capability: Option<[u8; 32]>,
    pub required_buyer_capability: Option<[u8; 32]>,
    pub required_dag_id: Option<[u8; 32]>,
}

/// State update for `CreateMarketV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CreateMarketUpdateV1 {
    pub market_id: MarketId,
    pub risk_type: RiskTypeId,
    pub premium_rate: u32,
    pub total_coverage: u64,
    pub coverage_period: u64,
    pub deductible: u64,
    pub max_coverage_per_buyer: u64,
    pub created_at: u64,
    pub required_underwriter_capability: Option<[u8; 32]>,
    pub required_buyer_capability: Option<[u8; 32]>,
    pub required_dag_id: Option<[u8; 32]>,
}

/// Parameters for `InsuranceMarket::UnderwriteV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct UnderwriteParamsV1 {
    pub market_id: MarketId,
    pub bond_amount: u64,
    pub coverage_limit: u64,
    pub underwriter: PublicKey,
}

/// State update for `UnderwriteV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct UnderwriteUpdateV1 {
    pub underwriter_id: UnderwriterId,
    pub market_id: MarketId,
    pub owner: PublicKey,
    pub bond_amount: u64,
    pub coverage_provided: u64,
    pub created_at: u64,
}

/// Parameters for `InsuranceMarket::PurchaseCoverageV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct PurchaseCoverageParamsV1 {
    pub market_id: MarketId,
    pub underwriter_id: UnderwriterId,
    pub buyer: PublicKey,
    pub coverage_amount: u64,
    pub value_commit: pallas::Point,
    pub signature: pallas::Base,
}

/// State update for `PurchaseCoverageV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct PurchaseCoverageUpdateV1 {
    pub coverage_id: CoverageId,
    pub market_id: MarketId,
    pub underwriter_id: UnderwriterId,
    pub buyer: PublicKey,
    pub amount: u64,
    pub premium_paid: u64,
    pub starts_at: u64,
    pub expires_at: u64,
}

/// Parameters for `InsuranceMarket::FileClaimV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct FileClaimParamsV1 {
    pub coverage_id: CoverageId,
    pub market_id: MarketId,
    pub buyer: PublicKey, // Access control: must match coverage.buyer
    pub amount: u64,
    pub evidence: Vec<u8>,
}

/// State update for `FileClaimV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct FileClaimUpdateV1 {
    pub claim_id: ClaimId,
    pub coverage_id: CoverageId,
    pub market_id: MarketId,
    pub amount: u64,
    pub state: ClaimState,
    pub created_at: u64,
}

/// Parameters for `InsuranceMarket::ResolveClaimV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ResolveClaimParamsV1 {
    pub claim_id: ClaimId,
    pub market_id: MarketId,
    pub is_valid: bool,
    pub payout_amount: u64,
    pub attestation: Vec<u8>,
    pub oracle_signature: pallas::Base,
}

/// State update for `ResolveClaimV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ResolveClaimUpdateV1 {
    pub claim_id: ClaimId,
    pub coverage_id: CoverageId,
    pub is_valid: bool,
    pub payout_amount: u64,
    pub slash_amount: u64,
    pub resolved_at: u64,
}

/// Parameters for `InsuranceMarket::WithdrawPremiumV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct WithdrawPremiumParamsV1 {
    pub underwriter_id: UnderwriterId,
    pub owner: PublicKey, // Access control: must match underwriter.owner
    pub amount: u64,
}

/// State update for `WithdrawPremiumV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct WithdrawPremiumUpdateV1 {
    pub underwriter_id: UnderwriterId,
    pub amount: u64,
    pub remaining_balance: u64,
}

/// Parameters for `InsuranceMarket::UpdatePremiumV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct UpdatePremiumParamsV1 {
    pub market_id: MarketId,
    pub new_premium_rate: u32,
}

// ============================================================================
// O-CAP ENABLED PARAMETERS
// ============================================================================

/// Parameters for `InsuranceMarket::UnderwriteWithCapabilityV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct UnderwriteWithCapabilityParamsV1 {
    pub market_id: MarketId,
    pub bond_amount: u64,
    pub coverage_limit: u64,
    pub underwriter: PublicKey,
    /// Capability proof from Identity contract
    pub capability_proof: Vec<u8>,
    /// Capability secret (proves ownership)
    pub capability_secret: [u8; 32],
}

/// State update for `UnderwriteWithCapabilityV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct UnderwriteWithCapabilityUpdateV1 {
    pub underwriter_id: UnderwriterId,
    pub market_id: MarketId,
    pub owner: PublicKey,
    pub bond_amount: u64,
    pub coverage_provided: u64,
    pub required_capability_id: [u8; 32],
    pub created_at: u64,
}

/// Parameters for `InsuranceMarket::PurchaseCoverageWithCapabilityV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct PurchaseCoverageWithCapabilityParamsV1 {
    pub market_id: MarketId,
    pub underwriter_id: UnderwriterId,
    pub buyer: PublicKey,
    pub coverage_amount: u64,
    pub value_commit: pallas::Point,
    pub signature: pallas::Base,
    /// Capability proof from Identity contract
    pub capability_proof: Vec<u8>,
    /// Capability secret (proves ownership)
    pub capability_secret: [u8; 32],
}

/// State update for `PurchaseCoverageWithCapabilityV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct PurchaseCoverageWithCapabilityUpdateV1 {
    pub coverage_id: CoverageId,
    pub market_id: MarketId,
    pub underwriter_id: UnderwriterId,
    pub buyer: PublicKey,
    pub amount: u64,
    pub premium_paid: u64,
    pub starts_at: u64,
    pub expires_at: u64,
    pub required_capability_id: [u8; 32],
}

/// Parameters for `InsuranceMarket::PurchaseCoverageWithDAGV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct PurchaseCoverageWithDAGParamsV1 {
    pub market_id: MarketId,
    pub underwriter_id: UnderwriterId,
    pub buyer: PublicKey,
    pub coverage_amount: u64,
    pub value_commit: pallas::Point,
    pub signature: pallas::Base,
    /// DAG claim proof from Identity contract (CreateClaimDAGV1)
    pub dag_proof: Vec<u8>,
    /// Path index in the DAG that was satisfied
    pub dag_path_index: u32,
    /// Required DAG ID for this coverage tier
    pub required_dag_id: [u8; 32],
}

/// State update for `PurchaseCoverageWithDAGV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct PurchaseCoverageWithDAGUpdateV1 {
    pub coverage_id: CoverageId,
    pub market_id: MarketId,
    pub underwriter_id: UnderwriterId,
    pub buyer: PublicKey,
    pub amount: u64,
    pub premium_paid: u64,
    pub starts_at: u64,
    pub expires_at: u64,
    pub required_dag_id: [u8; 32],
    pub dag_path_satisfied: u32,
}

/// Parameters for `InsuranceMarket::ResolveClaimWithCapabilityV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ResolveClaimWithCapabilityParamsV1 {
    pub claim_id: ClaimId,
    pub market_id: MarketId,
    pub is_valid: bool,
    pub payout_amount: u64,
    pub attestation: Vec<u8>,
    pub oracle_signature: pallas::Base,
    /// Capability proof from Identity contract
    pub capability_proof: Vec<u8>,
    /// Capability secret (proves ownership)
    pub capability_secret: [u8; 32],
}

/// State update for `ResolveClaimWithCapabilityV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ResolveClaimWithCapabilityUpdateV1 {
    pub claim_id: ClaimId,
    pub coverage_id: CoverageId,
    pub is_valid: bool,
    pub payout_amount: u64,
    pub slash_amount: u64,
    pub resolved_at: u64,
}

// ============================================================================
// IDENTITY DERIVATION
// ============================================================================

/// Derive a risk type ID from its parameters
pub fn derive_risk_type_id(
    category: RiskCategory,
    description: &[u8],
    oracle_pubkey: &PublicKey,
) -> RiskTypeId {
    let (ox, oy) = oracle_pubkey.xy();
    let mut bytes = [0u8; 8];
    let d_len = description.len().min(8);
    bytes[..d_len].copy_from_slice(&description[..d_len]);
    let desc_field = pallas::Base::from(u64::from_le_bytes(bytes));
    poseidon_hash([
        pallas::Base::from(category as u64),
        desc_field,
        ox,
        oy,
    ])
}

/// Derive an underwriter ID
pub fn derive_underwriter_id(
    market_id: MarketId,
    owner: &PublicKey,
    bond_amount: u64,
) -> UnderwriterId {
    let (ox, oy) = owner.xy();
    poseidon_hash([
        market_id,
        ox,
        oy,
        pallas::Base::from(bond_amount),
    ])
}

/// Derive a coverage ID
pub fn derive_coverage_id(
    market_id: MarketId,
    buyer: &PublicKey,
    amount: u64,
    timestamp: u64,
) -> CoverageId {
    let (bx, by) = buyer.xy();
    poseidon_hash([
        market_id,
        bx,
        by,
        pallas::Base::from(amount),
        pallas::Base::from(timestamp),
    ])
}

/// Derive a claim ID
pub fn derive_claim_id(
    coverage_id: CoverageId,
    evidence_hash: pallas::Base,
    timestamp: u64,
) -> ClaimId {
    poseidon_hash([coverage_id, evidence_hash, pallas::Base::from(timestamp)])
}

// ============================================================================
// CALCULATION HELPERS
// ============================================================================

/// Calculate premium from coverage amount and rate
pub fn calculate_premium(
    coverage_amount: u64,
    premium_rate: u32,
) -> Result<u64, InsuranceMarketError> {
    // premium_rate is in basis points (10000 = 100%)
    let product = coverage_amount
        .checked_mul(premium_rate as u64)
        .ok_or(InsuranceMarketError::ArithmeticOverflow)?;
    Ok(product / 10000)
}

/// Calculate maximum coverage supported by bond
pub fn calculate_max_coverage(
    bond_amount: u64,
    coverage_leverage: u32,
) -> Result<u64, InsuranceMarketError> {
    // coverage_leverage is typically > 1 (e.g., 10x means $100 bond backs $1000 coverage)
    bond_amount
        .checked_mul(coverage_leverage as u64)
        .ok_or(InsuranceMarketError::ArithmeticOverflow)
}

/// Calculate slash amount based on coverage and performance
pub fn calculate_slash(
    claim_amount: u64,
    _coverage_amount: u64,
    bond_amount: u64,
    performance_score: u32,
) -> Result<u64, InsuranceMarketError> {
    // Slash proportional to claim, capped at bond
    // Better performance = smaller slash
    let slash_ratio = 10000u64.checked_sub(performance_score as u64).ok_or(InsuranceMarketError::ArithmeticOverflow)?;
    let proportional_slash = claim_amount
        .checked_mul(slash_ratio)
        .ok_or(InsuranceMarketError::ArithmeticOverflow)?
        / 10000;
    Ok(proportional_slash.min(bond_amount))
}