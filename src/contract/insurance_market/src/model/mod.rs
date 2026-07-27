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

use dwow_sdk::{
    crypto::{poseidon_hash, pasta_prelude::PrimeField, PublicKey},
    error::ContractError,
    pasta::{group::GroupEncoding, pallas},
};
use dwow_serial::{SerialDecodable, SerialEncodable};

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
    type Error = dwow_sdk::error::ContractError;

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
            _ => Err(dwow_sdk::error::ContractError::InvalidFunction),
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
    type Error = dwow_sdk::error::ContractError;

    fn try_from(b: u8) -> Result<Self, Self::Error> {
        match b {
            0 => Ok(Self::Active),
            1 => Ok(Self::Expired),
            2 => Ok(Self::Claimed),
            3 => Ok(Self::Cancelled),
            _ => Err(dwow_sdk::error::ContractError::InvalidFunction),
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
    type Error = dwow_sdk::error::ContractError;

    fn try_from(b: u8) -> Result<Self, Self::Error> {
        match b {
            0 => Ok(Self::Filed),
            1 => Ok(Self::Resolved),
            2 => Ok(Self::Rejected),
            3 => Ok(Self::Paid),
            _ => Err(dwow_sdk::error::ContractError::InvalidFunction),
        }
    }
}

// ============================================================================
// CORE DATA STRUCTURES
// ============================================================================

/// Represents a registered risk type
#[derive(Debug, Clone)]
pub struct RiskType {
    pub version: u8,
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
#[derive(Debug, Clone)]
pub struct InsuranceMarket {
    pub version: u8,
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
#[derive(Debug, Clone)]
pub struct Underwriter {
    pub version: u8,
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
#[derive(Debug, Clone)]
pub struct Coverage {
    pub version: u8,
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
#[derive(Debug, Clone)]
pub struct Claim {
    pub version: u8,
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
#[derive(Debug, Clone)]
pub struct EndowmentPool {
    pub version: u8,
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
#[derive(Debug, Clone)]
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
#[derive(Debug, Clone)]
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
#[derive(Debug, Clone)]
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
    pub buyer_nullifier: pallas::Base,
}

/// State update for `PurchaseCoverageV1`
#[derive(Debug, Clone)]
pub struct PurchaseCoverageUpdateV1 {
    pub coverage_id: CoverageId,
    pub market_id: MarketId,
    pub underwriter_id: UnderwriterId,
    pub buyer: PublicKey,
    pub amount: u64,
    pub premium_paid: u64,
    pub starts_at: u64,
    pub expires_at: u64,
    pub buyer_nullifier: pallas::Base,
}

/// Parameters for `InsuranceMarket::FileClaimV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct FileClaimParamsV1 {
    pub coverage_id: CoverageId,
    pub market_id: MarketId,
    pub buyer: PublicKey, // Access control: must match coverage.buyer
    pub amount: u64,
    pub evidence: Vec<u8>,
    pub oracle_signature: pallas::Base,
}

/// State update for `FileClaimV1`
#[derive(Debug, Clone)]
pub struct FileClaimUpdateV1 {
    pub claim_id: ClaimId,
    pub coverage_id: CoverageId,
    pub market_id: MarketId,
    pub amount: u64,
    pub state: ClaimState,
    pub created_at: u64,
    pub oracle_signature: pallas::Base,
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
#[derive(Debug, Clone)]
pub struct ResolveClaimUpdateV1 {
    pub claim_id: ClaimId,
    pub coverage_id: CoverageId,
    pub is_valid: bool,
    pub payout_amount: u64,
    pub slash_amount: u64,
    pub resolved_at: u64,
    pub oracle_signature: pallas::Base,
}

/// Parameters for `InsuranceMarket::WithdrawPremiumV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct WithdrawPremiumParamsV1 {
    pub underwriter_id: UnderwriterId,
    pub owner: PublicKey, // Access control: must match underwriter.owner
    pub amount: u64,
}

/// State update for `WithdrawPremiumV1`
#[derive(Debug, Clone)]
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
#[derive(Debug, Clone)]
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
    pub buyer_nullifier: pallas::Base,
    /// Capability proof from Identity contract
    pub capability_proof: Vec<u8>,
    /// Capability secret (proves ownership)
    pub capability_secret: [u8; 32],
}

/// State update for `PurchaseCoverageWithCapabilityV1`
#[derive(Debug, Clone)]
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
    pub buyer_nullifier: pallas::Base,
}

/// Parameters for `InsuranceMarket::PurchaseCoverageWithDAGV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct PurchaseCoverageWithDAGParamsV1 {
    pub market_id: MarketId,
    pub underwriter_id: UnderwriterId,
    pub buyer: PublicKey,
    pub coverage_amount: u64,
    pub value_commit: pallas::Point,
    pub buyer_nullifier: pallas::Base,
    /// DAG claim proof from Identity contract (CreateClaimDAGV1)
    pub dag_proof: Vec<u8>,
    /// Path index in the DAG that was satisfied
    pub dag_path_index: u32,
    /// Required DAG ID for this coverage tier
    pub required_dag_id: [u8; 32],
}

/// State update for `PurchaseCoverageWithDAGV1`
#[derive(Debug, Clone)]
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
    pub buyer_nullifier: pallas::Base,
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
#[derive(Debug, Clone)]
pub struct ResolveClaimWithCapabilityUpdateV1 {
    pub claim_id: ClaimId,
    pub coverage_id: CoverageId,
    pub is_valid: bool,
    pub payout_amount: u64,
    pub slash_amount: u64,
    pub resolved_at: u64,
    pub oracle_signature: pallas::Base,
}

// ============================================================================
// DEACTIVATION PARAMETERS
// ============================================================================

/// Parameters for `InsuranceMarket::DeactivateUnderwriterV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct DeactivateUnderwriterParamsV1 {
    pub underwriter_id: UnderwriterId,
    pub owner: PublicKey,
}

/// State update for `DeactivateUnderwriterV1`
#[derive(Debug, Clone)]
pub struct DeactivateUnderwriterUpdateV1 {
    pub underwriter_id: UnderwriterId,
}

/// Parameters for `InsuranceMarket::CloseMarketV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CloseMarketParamsV1 {
    pub market_id: MarketId,
}

/// State update for `CloseMarketV1`
#[derive(Debug, Clone)]
pub struct CloseMarketUpdateV1 {
    pub market_id: MarketId,
}

/// Parameters for `InsuranceMarket::RetireRiskTypeV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct RetireRiskTypeParamsV1 {
    pub risk_type_id: RiskTypeId,
}

/// State update for `RetireRiskTypeV1`
#[derive(Debug, Clone)]
pub struct RetireRiskTypeUpdateV1 {
    pub risk_type_id: RiskTypeId,
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
    let (ox, oy) = oracle_pubkey.xy().expect("pk not identity");
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
    let (ox, oy) = owner.xy().expect("pk not identity");
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
    let (bx, by) = buyer.xy().expect("pk not identity");
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

// ============================================================================
// RHO-CALCULUS EXPLICIT ENCODE/DECODE
// ============================================================================

impl RiskType {
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(77 + self.description.len());
        b.push(self.version);
        b.extend_from_slice(&self.id.to_repr());
        b.push(self.category as u8);
        b.push(self.description.len() as u8);
        b.extend_from_slice(&self.description);
        b.extend_from_slice(&self.base_premium_rate.to_le_bytes());
        b.extend_from_slice(&self.min_bond_rate.to_le_bytes());
        b.extend_from_slice(&self.oracle_pubkey.to_bytes());
        b.push(self.active as u8);
        b.extend_from_slice(&self.created_at.to_le_bytes());
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 77 { return Err(ContractError::IoError(format!("RiskType: expected at least 77 bytes, got {}", data.len()))); }
        let version = data[0];
        let id = Option::<pallas::Base>::from(pallas::Base::from_repr(data[1..33].try_into().unwrap())).ok_or_else(|| ContractError::IoError("RiskType: invalid id".into()))?;
        let category = RiskCategory::try_from(data[33])?;
        let desc_len = data[34] as usize;
        if data.len() != 77 + desc_len { return Err(ContractError::IoError(format!("RiskType: expected {} bytes, got {}", 77 + desc_len, data.len()))); }
        let description = data[35..35+desc_len].to_vec();
        let base_premium_rate = u32::from_le_bytes(data[35+desc_len..39+desc_len].try_into().unwrap());
        let min_bond_rate = u32::from_le_bytes(data[39+desc_len..43+desc_len].try_into().unwrap());
        let oracle_pubkey = PublicKey::from_bytes(data[43+desc_len..75+desc_len].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("RiskType: invalid oracle_pubkey: {}", e)))?;
        let active = data[75+desc_len] != 0;
        let created_at = u64::from_le_bytes(data[76+desc_len..84+desc_len].try_into().unwrap());
        Ok(RiskType { version, id, category, description, base_premium_rate, min_bond_rate, oracle_pubkey, active, created_at })
    }
}

impl Coverage {
    pub fn encode(&self) -> Vec<u8> {
        let cap = 163 + if self.claim_id.is_some() { 32 } else { 0 };
        let mut b = Vec::with_capacity(cap);
        b.push(self.version);
        b.extend_from_slice(&self.id.to_repr());
        b.extend_from_slice(&self.market_id.to_repr());
        b.extend_from_slice(&self.buyer.to_bytes());
        b.extend_from_slice(&self.underwriter_id.to_repr());
        b.extend_from_slice(&self.amount.to_le_bytes());
        b.extend_from_slice(&self.premium_paid.to_le_bytes());
        b.push(self.state as u8);
        b.extend_from_slice(&self.starts_at.to_le_bytes());
        b.extend_from_slice(&self.expires_at.to_le_bytes());
        b.push(self.claim_id.is_some() as u8);
        if let Some(ref cid) = self.claim_id { b.extend_from_slice(&cid.to_repr()); }
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 163 { return Err(ContractError::IoError(format!("Coverage: expected at least 163 bytes, got {}", data.len()))); }
        let version = data[0];
        let id = Option::<pallas::Base>::from(pallas::Base::from_repr(data[1..33].try_into().unwrap())).ok_or_else(|| ContractError::IoError("Coverage: invalid id".into()))?;
        let market_id = Option::<pallas::Base>::from(pallas::Base::from_repr(data[33..65].try_into().unwrap())).ok_or_else(|| ContractError::IoError("Coverage: invalid market_id".into()))?;
        let buyer = PublicKey::from_bytes(data[65..97].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("Coverage: invalid buyer: {}", e)))?;
        let underwriter_id = Option::<pallas::Base>::from(pallas::Base::from_repr(data[97..129].try_into().unwrap())).ok_or_else(|| ContractError::IoError("Coverage: invalid underwriter_id".into()))?;
        let amount = u64::from_le_bytes(data[129..137].try_into().unwrap());
        let premium_paid = u64::from_le_bytes(data[137..145].try_into().unwrap());
        let state = CoverageState::try_from(data[145])?;
        let starts_at = u64::from_le_bytes(data[146..154].try_into().unwrap());
        let expires_at = u64::from_le_bytes(data[154..162].try_into().unwrap());
        let has_claim = data[162] != 0;
        let claim_id = if has_claim { Some(Option::<pallas::Base>::from(pallas::Base::from_repr(data[163..195].try_into().unwrap())).ok_or_else(|| ContractError::IoError("Coverage: invalid claim_id".into()))?) } else { None };
        Ok(Coverage { version, id, market_id, buyer, underwriter_id, amount, premium_paid, state, starts_at, expires_at, claim_id })
    }
}

impl Claim {
    pub fn encode(&self) -> Vec<u8> {
        let cap = 156 + self.evidence.len() + self.attestation.len();
        let mut b = Vec::with_capacity(cap);
        b.push(self.version);
        b.extend_from_slice(&self.id.to_repr());
        b.extend_from_slice(&self.coverage_id.to_repr());
        b.extend_from_slice(&self.market_id.to_repr());
        b.extend_from_slice(&self.amount.to_le_bytes());
        b.extend_from_slice(&self.payout.to_le_bytes());
        b.push(self.state as u8);
        b.push(self.evidence.len() as u8);
        b.extend_from_slice(&self.evidence);
        b.push(self.attestation.len() as u8);
        b.extend_from_slice(&self.attestation);
        b.extend_from_slice(&self.oracle_signature.to_repr());
        b.extend_from_slice(&self.resolved_at.to_le_bytes());
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 156 { return Err(ContractError::IoError(format!("Claim: expected at least 156 bytes, got {}", data.len()))); }
        let version = data[0];
        let id = Option::<pallas::Base>::from(pallas::Base::from_repr(data[1..33].try_into().unwrap())).ok_or_else(|| ContractError::IoError("Claim: invalid id".into()))?;
        let coverage_id = Option::<pallas::Base>::from(pallas::Base::from_repr(data[33..65].try_into().unwrap())).ok_or_else(|| ContractError::IoError("Claim: invalid coverage_id".into()))?;
        let market_id = Option::<pallas::Base>::from(pallas::Base::from_repr(data[65..97].try_into().unwrap())).ok_or_else(|| ContractError::IoError("Claim: invalid market_id".into()))?;
        let amount = u64::from_le_bytes(data[97..105].try_into().unwrap());
        let payout = u64::from_le_bytes(data[105..113].try_into().unwrap());
        let state = ClaimState::try_from(data[113])?;
        let ev_len = data[114] as usize;
        if data.len() < 115 + ev_len + 1 { return Err(ContractError::IoError("Claim: data too short for evidence".into())); }
        let evidence = data[115..115+ev_len].to_vec();
        let at_len = data[115+ev_len] as usize;
        let at_end = 116 + ev_len + at_len;
        if data.len() != at_end + 40 { return Err(ContractError::IoError(format!("Claim: expected {} bytes, got {}", at_end + 40, data.len()))); }
        let attestation = data[116+ev_len..at_end].to_vec();
        let oracle_signature = Option::<pallas::Base>::from(pallas::Base::from_repr(data[at_end..at_end+32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("Claim: invalid oracle_signature".into()))?;
        let resolved_at = u64::from_le_bytes(data[at_end+32..at_end+40].try_into().unwrap());
        Ok(Claim { version, id, coverage_id, market_id, amount, payout, state, evidence, attestation, oracle_signature, resolved_at })
    }
}

impl InsuranceMarket {
    pub fn encode(&self) -> Vec<u8> {
        let cap = 129 + if self.required_underwriter_capability.is_some() { 32 } else { 0 }
            + if self.required_buyer_capability.is_some() { 32 } else { 0 }
            + if self.required_dag_id.is_some() { 32 } else { 0 };
        let mut b = Vec::with_capacity(cap);
        b.push(self.version);
        b.extend_from_slice(&self.id.to_repr());
        b.extend_from_slice(&self.risk_type.to_repr());
        b.extend_from_slice(&self.premium_rate.to_le_bytes());
        b.extend_from_slice(&self.total_coverage.to_le_bytes());
        b.extend_from_slice(&self.coverage_sold.to_le_bytes());
        b.extend_from_slice(&self.coverage_period.to_le_bytes());
        b.extend_from_slice(&self.deductible.to_le_bytes());
        b.extend_from_slice(&self.max_coverage_per_buyer.to_le_bytes());
        b.push(self.active as u8);
        b.extend_from_slice(&self.created_at.to_le_bytes());
        b.extend_from_slice(&self.closes_at.to_le_bytes());
        b.push(self.required_underwriter_capability.is_some() as u8);
        if let Some(ref c) = self.required_underwriter_capability { b.extend_from_slice(c); }
        b.push(self.required_buyer_capability.is_some() as u8);
        if let Some(ref c) = self.required_buyer_capability { b.extend_from_slice(c); }
        b.push(self.required_dag_id.is_some() as u8);
        if let Some(ref d) = self.required_dag_id { b.extend_from_slice(d); }
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 129 { return Err(ContractError::IoError(format!("InsuranceMarket: expected at least 129 bytes, got {}", data.len()))); }
        let version = data[0];
        let id = Option::<pallas::Base>::from(pallas::Base::from_repr(data[1..33].try_into().unwrap())).ok_or_else(|| ContractError::IoError("InsuranceMarket: invalid id".into()))?;
        let risk_type = Option::<pallas::Base>::from(pallas::Base::from_repr(data[33..65].try_into().unwrap())).ok_or_else(|| ContractError::IoError("InsuranceMarket: invalid risk_type".into()))?;
        let premium_rate = u32::from_le_bytes(data[65..69].try_into().unwrap());
        let total_coverage = u64::from_le_bytes(data[69..77].try_into().unwrap());
        let coverage_sold = u64::from_le_bytes(data[77..85].try_into().unwrap());
        let coverage_period = u64::from_le_bytes(data[85..93].try_into().unwrap());
        let deductible = u64::from_le_bytes(data[93..101].try_into().unwrap());
        let max_coverage_per_buyer = u64::from_le_bytes(data[101..109].try_into().unwrap());
        let active = data[109] != 0;
        let created_at = u64::from_le_bytes(data[110..118].try_into().unwrap());
        let closes_at = u64::from_le_bytes(data[118..126].try_into().unwrap());
        let mut pos = 126;
        let has_uw = data[pos] != 0; pos += 1;
        let (required_underwriter_capability, mut pos) = if has_uw { (Some(data[pos..pos+32].try_into().unwrap()), pos + 32) } else { (None, pos) };
        let has_buy = data[pos] != 0; pos += 1;
        let (required_buyer_capability, mut pos) = if has_buy { (Some(data[pos..pos+32].try_into().unwrap()), pos + 32) } else { (None, pos) };
        let has_dag = data[pos] != 0; pos += 1;
        let (required_dag_id, _) = if has_dag { (Some(data[pos..pos+32].try_into().unwrap()), pos + 32) } else { (None, pos) };
        Ok(InsuranceMarket { version, id, risk_type, premium_rate, total_coverage, coverage_sold, coverage_period, deductible, max_coverage_per_buyer, active, created_at, closes_at, required_underwriter_capability, required_buyer_capability, required_dag_id })
    }
}

impl EndowmentPool {
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(98 + self.returns_history.len() * 8);
        b.push(self.version);
        b.extend_from_slice(&self.id.to_repr());
        b.extend_from_slice(&self.market_id.to_repr());
        b.extend_from_slice(&self.total_capital.to_le_bytes());
        b.extend_from_slice(&self.deployed_capital.to_le_bytes());
        b.extend_from_slice(&self.total_shares.to_le_bytes());
        b.push(self.returns_history.len() as u8);
        for r in &self.returns_history { b.extend_from_slice(&r.to_le_bytes()); }
        b.extend_from_slice(&self.created_at.to_le_bytes());
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 98 { return Err(ContractError::IoError(format!("EndowmentPool: expected at least 98 bytes, got {}", data.len()))); }
        let version = data[0];
        let id = Option::<pallas::Base>::from(pallas::Base::from_repr(data[1..33].try_into().unwrap())).ok_or_else(|| ContractError::IoError("EndowmentPool: invalid id".into()))?;
        let market_id = Option::<pallas::Base>::from(pallas::Base::from_repr(data[33..65].try_into().unwrap())).ok_or_else(|| ContractError::IoError("EndowmentPool: invalid market_id".into()))?;
        let total_capital = u64::from_le_bytes(data[65..73].try_into().unwrap());
        let deployed_capital = u64::from_le_bytes(data[73..81].try_into().unwrap());
        let total_shares = u64::from_le_bytes(data[81..89].try_into().unwrap());
        let rh_count = data[89] as usize;
        let expected = 98 + rh_count * 8;
        if data.len() != expected { return Err(ContractError::IoError(format!("EndowmentPool: expected {} bytes for {} returns, got {}", expected, rh_count, data.len()))); }
        let mut returns_history = Vec::with_capacity(rh_count);
        for i in 0..rh_count { returns_history.push(u64::from_le_bytes(data[90+i*8..90+(i+1)*8].try_into().unwrap())); }
        let created_at = u64::from_le_bytes(data[90+rh_count*8..98+rh_count*8].try_into().unwrap());
        Ok(EndowmentPool { version, id, market_id, total_capital, deployed_capital, total_shares, returns_history, created_at })
    }
}

// --- Bridge update structs ---

impl UnderwriteUpdateV1 { pub const ENCODED_SIZE: usize = 120; pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(120); b.extend_from_slice(&self.underwriter_id.to_repr()); b.extend_from_slice(&self.market_id.to_repr()); b.extend_from_slice(&self.owner.to_bytes()); b.extend_from_slice(&self.bond_amount.to_le_bytes()); b.extend_from_slice(&self.coverage_provided.to_le_bytes()); b.extend_from_slice(&self.created_at.to_le_bytes()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 120 { return Err(ContractError::IoError(format!("UnderwriteUpdateV1: expected 120 bytes, got {}", data.len()))); } Ok(UnderwriteUpdateV1 { underwriter_id: Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("UnderwriteUpdateV1: invalid underwriter_id".into()))?, market_id: Option::<pallas::Base>::from(pallas::Base::from_repr(data[32..64].try_into().unwrap())).ok_or_else(|| ContractError::IoError("UnderwriteUpdateV1: invalid market_id".into()))?, owner: PublicKey::from_bytes(data[64..96].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("UnderwriteUpdateV1: invalid owner: {}", e)))?, bond_amount: u64::from_le_bytes(data[96..104].try_into().unwrap()), coverage_provided: u64::from_le_bytes(data[104..112].try_into().unwrap()), created_at: u64::from_le_bytes(data[112..120].try_into().unwrap()) }) } }

impl PurchaseCoverageUpdateV1 { pub const ENCODED_SIZE: usize = 192; pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(192); b.extend_from_slice(&self.coverage_id.to_repr()); b.extend_from_slice(&self.market_id.to_repr()); b.extend_from_slice(&self.underwriter_id.to_repr()); b.extend_from_slice(&self.buyer.to_bytes()); b.extend_from_slice(&self.amount.to_le_bytes()); b.extend_from_slice(&self.premium_paid.to_le_bytes()); b.extend_from_slice(&self.starts_at.to_le_bytes()); b.extend_from_slice(&self.expires_at.to_le_bytes()); b.extend_from_slice(&self.buyer_nullifier.to_repr()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 192 { return Err(ContractError::IoError(format!("PurchaseCoverageUpdateV1: expected 192 bytes, got {}", data.len()))); } Ok(PurchaseCoverageUpdateV1 { coverage_id: Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("PurchaseCoverageUpdateV1: invalid coverage_id".into()))?, market_id: Option::<pallas::Base>::from(pallas::Base::from_repr(data[32..64].try_into().unwrap())).ok_or_else(|| ContractError::IoError("PurchaseCoverageUpdateV1: invalid market_id".into()))?, underwriter_id: Option::<pallas::Base>::from(pallas::Base::from_repr(data[64..96].try_into().unwrap())).ok_or_else(|| ContractError::IoError("PurchaseCoverageUpdateV1: invalid underwriter_id".into()))?, buyer: PublicKey::from_bytes(data[96..128].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("PurchaseCoverageUpdateV1: invalid buyer: {}", e)))?, amount: u64::from_le_bytes(data[128..136].try_into().unwrap()), premium_paid: u64::from_le_bytes(data[136..144].try_into().unwrap()), starts_at: u64::from_le_bytes(data[144..152].try_into().unwrap()), expires_at: u64::from_le_bytes(data[152..160].try_into().unwrap()), buyer_nullifier: Option::<pallas::Base>::from(pallas::Base::from_repr(data[160..192].try_into().unwrap())).ok_or_else(|| ContractError::IoError("PurchaseCoverageUpdateV1: invalid buyer_nullifier".into()))? }) } }

impl WithdrawPremiumUpdateV1 { pub const ENCODED_SIZE: usize = 48; pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(48); b.extend_from_slice(&self.underwriter_id.to_repr()); b.extend_from_slice(&self.amount.to_le_bytes()); b.extend_from_slice(&self.remaining_balance.to_le_bytes()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 48 { return Err(ContractError::IoError(format!("WithdrawPremiumUpdateV1: expected 48 bytes, got {}", data.len()))); } Ok(WithdrawPremiumUpdateV1 { underwriter_id: Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("WithdrawPremiumUpdateV1: invalid underwriter_id".into()))?, amount: u64::from_le_bytes(data[32..40].try_into().unwrap()), remaining_balance: u64::from_le_bytes(data[40..48].try_into().unwrap()) }) } }

impl ResolveClaimUpdateV1 { pub const ENCODED_SIZE: usize = 137; pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(137); b.extend_from_slice(&self.claim_id.to_repr()); b.extend_from_slice(&self.coverage_id.to_repr()); b.push(self.is_valid as u8); b.extend_from_slice(&self.payout_amount.to_le_bytes()); b.extend_from_slice(&self.slash_amount.to_le_bytes()); b.extend_from_slice(&self.resolved_at.to_le_bytes()); b.extend_from_slice(&self.oracle_signature.to_repr()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 137 { return Err(ContractError::IoError(format!("ResolveClaimUpdateV1: expected 137 bytes, got {}", data.len()))); } Ok(ResolveClaimUpdateV1 { claim_id: Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("ResolveClaimUpdateV1: invalid claim_id".into()))?, coverage_id: Option::<pallas::Base>::from(pallas::Base::from_repr(data[32..64].try_into().unwrap())).ok_or_else(|| ContractError::IoError("ResolveClaimUpdateV1: invalid coverage_id".into()))?, is_valid: data[64] != 0, payout_amount: u64::from_le_bytes(data[65..73].try_into().unwrap()), slash_amount: u64::from_le_bytes(data[73..81].try_into().unwrap()), resolved_at: u64::from_le_bytes(data[81..89].try_into().unwrap()), oracle_signature: Option::<pallas::Base>::from(pallas::Base::from_repr(data[89..121].try_into().unwrap())).ok_or_else(|| ContractError::IoError("ResolveClaimUpdateV1: invalid oracle_signature".into()))? }) } }

impl FileClaimUpdateV1 { pub const ENCODED_SIZE: usize = 169; pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(169); b.extend_from_slice(&self.claim_id.to_repr()); b.extend_from_slice(&self.coverage_id.to_repr()); b.extend_from_slice(&self.market_id.to_repr()); b.extend_from_slice(&self.amount.to_le_bytes()); b.push(self.state as u8); b.extend_from_slice(&self.created_at.to_le_bytes()); b.extend_from_slice(&self.oracle_signature.to_repr()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 169 { return Err(ContractError::IoError(format!("FileClaimUpdateV1: expected 169 bytes, got {}", data.len()))); } Ok(FileClaimUpdateV1 { claim_id: Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("FileClaimUpdateV1: invalid claim_id".into()))?, coverage_id: Option::<pallas::Base>::from(pallas::Base::from_repr(data[32..64].try_into().unwrap())).ok_or_else(|| ContractError::IoError("FileClaimUpdateV1: invalid coverage_id".into()))?, market_id: Option::<pallas::Base>::from(pallas::Base::from_repr(data[64..96].try_into().unwrap())).ok_or_else(|| ContractError::IoError("FileClaimUpdateV1: invalid market_id".into()))?, amount: u64::from_le_bytes(data[96..104].try_into().unwrap()), state: ClaimState::try_from(data[104])?, created_at: u64::from_le_bytes(data[105..113].try_into().unwrap()), oracle_signature: Option::<pallas::Base>::from(pallas::Base::from_repr(data[113..145].try_into().unwrap())).ok_or_else(|| ContractError::IoError("FileClaimUpdateV1: invalid oracle_signature".into()))? }) } }

impl DeactivateUnderwriterUpdateV1 { pub const ENCODED_SIZE: usize = 32; pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(32); b.extend_from_slice(&self.underwriter_id.to_repr()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 32 { return Err(ContractError::IoError(format!("DeactivateUnderwriterUpdateV1: expected 32 bytes, got {}", data.len()))); } Ok(DeactivateUnderwriterUpdateV1 { underwriter_id: Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("DeactivateUnderwriterUpdateV1: invalid underwriter_id".into()))? }) } }

impl CloseMarketUpdateV1 { pub const ENCODED_SIZE: usize = 32; pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(32); b.extend_from_slice(&self.market_id.to_repr()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 32 { return Err(ContractError::IoError(format!("CloseMarketUpdateV1: expected 32 bytes, got {}", data.len()))); } Ok(CloseMarketUpdateV1 { market_id: Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("CloseMarketUpdateV1: invalid market_id".into()))? }) } }

impl RetireRiskTypeUpdateV1 { pub const ENCODED_SIZE: usize = 32; pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(32); b.extend_from_slice(&self.risk_type_id.to_repr()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 32 { return Err(ContractError::IoError(format!("RetireRiskTypeUpdateV1: expected 32 bytes, got {}", data.len()))); } Ok(RetireRiskTypeUpdateV1 { risk_type_id: Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("RetireRiskTypeUpdateV1: invalid risk_type_id".into()))? }) } }

impl CreateMarketUpdateV1 {
    pub fn encode(&self) -> Vec<u8> {
        let cap = 109 + if self.required_underwriter_capability.is_some() { 32 } else { 0 }
            + if self.required_buyer_capability.is_some() { 32 } else { 0 }
            + if self.required_dag_id.is_some() { 32 } else { 0 };
        let mut b = Vec::with_capacity(cap);
        b.extend_from_slice(&self.market_id.to_repr());
        b.extend_from_slice(&self.risk_type.to_repr());
        b.extend_from_slice(&self.premium_rate.to_le_bytes());
        b.extend_from_slice(&self.total_coverage.to_le_bytes());
        b.extend_from_slice(&self.coverage_period.to_le_bytes());
        b.extend_from_slice(&self.deductible.to_le_bytes());
        b.extend_from_slice(&self.max_coverage_per_buyer.to_le_bytes());
        b.extend_from_slice(&self.created_at.to_le_bytes());
        b.push(self.required_underwriter_capability.is_some() as u8);
        if let Some(ref c) = self.required_underwriter_capability { b.extend_from_slice(c); }
        b.push(self.required_buyer_capability.is_some() as u8);
        if let Some(ref c) = self.required_buyer_capability { b.extend_from_slice(c); }
        b.push(self.required_dag_id.is_some() as u8);
        if let Some(ref d) = self.required_dag_id { b.extend_from_slice(d); }
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 109 { return Err(ContractError::IoError(format!("CreateMarketUpdateV1: expected at least 109 bytes, got {}", data.len()))); }
        let market_id = Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("CreateMarketUpdateV1: invalid market_id".into()))?;
        let risk_type = Option::<pallas::Base>::from(pallas::Base::from_repr(data[32..64].try_into().unwrap())).ok_or_else(|| ContractError::IoError("CreateMarketUpdateV1: invalid risk_type".into()))?;
        let premium_rate = u32::from_le_bytes(data[64..68].try_into().unwrap());
        let total_coverage = u64::from_le_bytes(data[68..76].try_into().unwrap());
        let coverage_period = u64::from_le_bytes(data[76..84].try_into().unwrap());
        let deductible = u64::from_le_bytes(data[84..92].try_into().unwrap());
        let max_coverage_per_buyer = u64::from_le_bytes(data[92..100].try_into().unwrap());
        let created_at = u64::from_le_bytes(data[100..108].try_into().unwrap());
        let mut pos = 108;
        let has_uw = data[pos] != 0; pos += 1;
        let (required_underwriter_capability, mut pos) = if has_uw { (Some(data[pos..pos+32].try_into().unwrap()), pos + 32) } else { (None, pos) };
        let has_buy = data[pos] != 0; pos += 1;
        let (required_buyer_capability, mut pos) = if has_buy { (Some(data[pos..pos+32].try_into().unwrap()), pos + 32) } else { (None, pos) };
        let has_dag = data[pos] != 0; pos += 1;
        let (required_dag_id, _) = if has_dag { (Some(data[pos..pos+32].try_into().unwrap()), pos + 32) } else { (None, pos) };
        Ok(CreateMarketUpdateV1 { market_id, risk_type, premium_rate, total_coverage, coverage_period, deductible, max_coverage_per_buyer, created_at, required_underwriter_capability, required_buyer_capability, required_dag_id })
    }
}

impl RegisterRiskTypeUpdateV1 {
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(77 + self.description.len());
        b.extend_from_slice(&self.risk_type_id.to_repr());
        b.push(self.category as u8);
        b.push(self.description.len() as u8);
        b.extend_from_slice(&self.description);
        b.extend_from_slice(&self.base_premium_rate.to_le_bytes());
        b.extend_from_slice(&self.min_bond_rate.to_le_bytes());
        b.extend_from_slice(&self.oracle_pubkey.to_bytes());
        b.extend_from_slice(&self.created_at.to_le_bytes());
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 77 { return Err(ContractError::IoError(format!("RegisterRiskTypeUpdateV1: expected at least 77 bytes, got {}", data.len()))); }
        let risk_type_id = Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("RegisterRiskTypeUpdateV1: invalid risk_type_id".into()))?;
        let category = RiskCategory::try_from(data[32])?;
        let desc_len = data[33] as usize;
        if data.len() != 77 + desc_len { return Err(ContractError::IoError(format!("RegisterRiskTypeUpdateV1: expected {} bytes, got {}", 77 + desc_len, data.len()))); }
        let description = data[34..34+desc_len].to_vec();
        let base_premium_rate = u32::from_le_bytes(data[34+desc_len..38+desc_len].try_into().unwrap());
        let min_bond_rate = u32::from_le_bytes(data[38+desc_len..42+desc_len].try_into().unwrap());
        let oracle_pubkey = PublicKey::from_bytes(data[42+desc_len..74+desc_len].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("RegisterRiskTypeUpdateV1: invalid oracle_pubkey: {}", e)))?;
        let created_at = u64::from_le_bytes(data[74+desc_len..82+desc_len].try_into().unwrap());
        Ok(RegisterRiskTypeUpdateV1 { risk_type_id, category, description, base_premium_rate, min_bond_rate, oracle_pubkey, created_at })
    }
}

impl UnderwriteWithCapabilityUpdateV1 { pub const ENCODED_SIZE: usize = 152; pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(152); b.extend_from_slice(&self.underwriter_id.to_repr()); b.extend_from_slice(&self.market_id.to_repr()); b.extend_from_slice(&self.owner.to_bytes()); b.extend_from_slice(&self.bond_amount.to_le_bytes()); b.extend_from_slice(&self.coverage_provided.to_le_bytes()); b.extend_from_slice(&self.required_capability_id); b.extend_from_slice(&self.created_at.to_le_bytes()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 152 { return Err(ContractError::IoError(format!("UnderwriteWithCapabilityUpdateV1: expected 152 bytes, got {}", data.len()))); } Ok(UnderwriteWithCapabilityUpdateV1 { underwriter_id: Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("UnderwriteWithCapabilityUpdateV1: invalid underwriter_id".into()))?, market_id: Option::<pallas::Base>::from(pallas::Base::from_repr(data[32..64].try_into().unwrap())).ok_or_else(|| ContractError::IoError("UnderwriteWithCapabilityUpdateV1: invalid market_id".into()))?, owner: PublicKey::from_bytes(data[64..96].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("UnderwriteWithCapabilityUpdateV1: invalid owner: {}", e)))?, bond_amount: u64::from_le_bytes(data[96..104].try_into().unwrap()), coverage_provided: u64::from_le_bytes(data[104..112].try_into().unwrap()), required_capability_id: data[112..144].try_into().unwrap(), created_at: u64::from_le_bytes(data[144..152].try_into().unwrap()) }) } }

impl PurchaseCoverageWithCapabilityUpdateV1 { pub const ENCODED_SIZE: usize = 224; pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(224); b.extend_from_slice(&self.coverage_id.to_repr()); b.extend_from_slice(&self.market_id.to_repr()); b.extend_from_slice(&self.underwriter_id.to_repr()); b.extend_from_slice(&self.buyer.to_bytes()); b.extend_from_slice(&self.amount.to_le_bytes()); b.extend_from_slice(&self.premium_paid.to_le_bytes()); b.extend_from_slice(&self.starts_at.to_le_bytes()); b.extend_from_slice(&self.expires_at.to_le_bytes()); b.extend_from_slice(&self.required_capability_id); b.extend_from_slice(&self.buyer_nullifier.to_repr()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 224 { return Err(ContractError::IoError(format!("PurchaseCoverageWithCapabilityUpdateV1: expected 224 bytes, got {}", data.len()))); } Ok(PurchaseCoverageWithCapabilityUpdateV1 { coverage_id: Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("PurchaseCoverageWithCapabilityUpdateV1: invalid coverage_id".into()))?, market_id: Option::<pallas::Base>::from(pallas::Base::from_repr(data[32..64].try_into().unwrap())).ok_or_else(|| ContractError::IoError("PurchaseCoverageWithCapabilityUpdateV1: invalid market_id".into()))?, underwriter_id: Option::<pallas::Base>::from(pallas::Base::from_repr(data[64..96].try_into().unwrap())).ok_or_else(|| ContractError::IoError("PurchaseCoverageWithCapabilityUpdateV1: invalid underwriter_id".into()))?, buyer: PublicKey::from_bytes(data[96..128].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("PurchaseCoverageWithCapabilityUpdateV1: invalid buyer: {}", e)))?, amount: u64::from_le_bytes(data[128..136].try_into().unwrap()), premium_paid: u64::from_le_bytes(data[136..144].try_into().unwrap()), starts_at: u64::from_le_bytes(data[144..152].try_into().unwrap()), expires_at: u64::from_le_bytes(data[152..160].try_into().unwrap()), required_capability_id: data[160..192].try_into().unwrap(), buyer_nullifier: Option::<pallas::Base>::from(pallas::Base::from_repr(data[192..224].try_into().unwrap())).ok_or_else(|| ContractError::IoError("PurchaseCoverageWithCapabilityUpdateV1: invalid buyer_nullifier".into()))? }) } }

impl PurchaseCoverageWithDAGUpdateV1 { pub const ENCODED_SIZE: usize = 228; pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(228); b.extend_from_slice(&self.coverage_id.to_repr()); b.extend_from_slice(&self.market_id.to_repr()); b.extend_from_slice(&self.underwriter_id.to_repr()); b.extend_from_slice(&self.buyer.to_bytes()); b.extend_from_slice(&self.amount.to_le_bytes()); b.extend_from_slice(&self.premium_paid.to_le_bytes()); b.extend_from_slice(&self.starts_at.to_le_bytes()); b.extend_from_slice(&self.expires_at.to_le_bytes()); b.extend_from_slice(&self.required_dag_id); b.extend_from_slice(&self.dag_path_satisfied.to_le_bytes()); b.extend_from_slice(&self.buyer_nullifier.to_repr()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 228 { return Err(ContractError::IoError(format!("PurchaseCoverageWithDAGUpdateV1: expected 228 bytes, got {}", data.len()))); } Ok(PurchaseCoverageWithDAGUpdateV1 { coverage_id: Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("PurchaseCoverageWithDAGUpdateV1: invalid coverage_id".into()))?, market_id: Option::<pallas::Base>::from(pallas::Base::from_repr(data[32..64].try_into().unwrap())).ok_or_else(|| ContractError::IoError("PurchaseCoverageWithDAGUpdateV1: invalid market_id".into()))?, underwriter_id: Option::<pallas::Base>::from(pallas::Base::from_repr(data[64..96].try_into().unwrap())).ok_or_else(|| ContractError::IoError("PurchaseCoverageWithDAGUpdateV1: invalid underwriter_id".into()))?, buyer: PublicKey::from_bytes(data[96..128].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("PurchaseCoverageWithDAGUpdateV1: invalid buyer: {}", e)))?, amount: u64::from_le_bytes(data[128..136].try_into().unwrap()), premium_paid: u64::from_le_bytes(data[136..144].try_into().unwrap()), starts_at: u64::from_le_bytes(data[144..152].try_into().unwrap()), expires_at: u64::from_le_bytes(data[152..160].try_into().unwrap()), required_dag_id: data[160..192].try_into().unwrap(), dag_path_satisfied: u32::from_le_bytes(data[192..196].try_into().unwrap()), buyer_nullifier: Option::<pallas::Base>::from(pallas::Base::from_repr(data[196..228].try_into().unwrap())).ok_or_else(|| ContractError::IoError("PurchaseCoverageWithDAGUpdateV1: invalid buyer_nullifier".into()))? }) } }

impl ResolveClaimWithCapabilityUpdateV1 { pub const ENCODED_SIZE: usize = 121; pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(121); b.extend_from_slice(&self.claim_id.to_repr()); b.extend_from_slice(&self.coverage_id.to_repr()); b.push(self.is_valid as u8); b.extend_from_slice(&self.payout_amount.to_le_bytes()); b.extend_from_slice(&self.slash_amount.to_le_bytes()); b.extend_from_slice(&self.resolved_at.to_le_bytes()); b.extend_from_slice(&self.oracle_signature.to_repr()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 121 { return Err(ContractError::IoError(format!("ResolveClaimWithCapabilityUpdateV1: expected 121 bytes, got {}", data.len()))); } Ok(ResolveClaimWithCapabilityUpdateV1 { claim_id: Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("ResolveClaimWithCapabilityUpdateV1: invalid claim_id".into()))?, coverage_id: Option::<pallas::Base>::from(pallas::Base::from_repr(data[32..64].try_into().unwrap())).ok_or_else(|| ContractError::IoError("ResolveClaimWithCapabilityUpdateV1: invalid coverage_id".into()))?, is_valid: data[64] != 0, payout_amount: u64::from_le_bytes(data[65..73].try_into().unwrap()), slash_amount: u64::from_le_bytes(data[73..81].try_into().unwrap()), resolved_at: u64::from_le_bytes(data[81..89].try_into().unwrap()), oracle_signature: Option::<pallas::Base>::from(pallas::Base::from_repr(data[89..121].try_into().unwrap())).ok_or_else(|| ContractError::IoError("ResolveClaimWithCapabilityUpdateV1: invalid oracle_signature".into()))? }) } }

impl Underwriter {
    pub const ENCODED_SIZE: usize = 154;
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(154);
        b.push(self.version);
        b.extend_from_slice(&self.id.to_repr());
        b.extend_from_slice(&self.owner.to_bytes());
        b.extend_from_slice(&self.market_id.to_repr());
        b.extend_from_slice(&self.bond_amount.to_le_bytes());
        b.extend_from_slice(&self.coverage_provided.to_le_bytes());
        b.extend_from_slice(&self.coverage_sold.to_le_bytes());
        b.extend_from_slice(&self.earned_premiums.to_le_bytes());
        b.extend_from_slice(&self.claims_paid.to_le_bytes());
        b.extend_from_slice(&self.slash_count.to_le_bytes());
        b.extend_from_slice(&self.performance_score.to_le_bytes());
        b.push(self.active as u8);
        b.extend_from_slice(&self.created_at.to_le_bytes());
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 154 { return Err(ContractError::IoError(format!("Underwriter: expected 154 bytes, got {}", data.len()))); }
        Ok(Underwriter { version: data[0], id: Option::<pallas::Base>::from(pallas::Base::from_repr(data[1..33].try_into().unwrap())).ok_or_else(|| ContractError::IoError("Underwriter: invalid id".into()))?, owner: PublicKey::from_bytes(data[33..65].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("Underwriter: invalid owner: {}", e)))?, market_id: Option::<pallas::Base>::from(pallas::Base::from_repr(data[65..97].try_into().unwrap())).ok_or_else(|| ContractError::IoError("Underwriter: invalid market_id".into()))?, bond_amount: u64::from_le_bytes(data[97..105].try_into().unwrap()), coverage_provided: u64::from_le_bytes(data[105..113].try_into().unwrap()), coverage_sold: u64::from_le_bytes(data[113..121].try_into().unwrap()), earned_premiums: u64::from_le_bytes(data[121..129].try_into().unwrap()), claims_paid: u64::from_le_bytes(data[129..137].try_into().unwrap()), slash_count: u32::from_le_bytes(data[137..141].try_into().unwrap()), performance_score: u32::from_le_bytes(data[141..145].try_into().unwrap()), active: data[145] != 0, created_at: u64::from_le_bytes(data[146..154].try_into().unwrap()) })
    }
}