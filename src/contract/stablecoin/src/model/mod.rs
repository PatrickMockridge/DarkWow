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

//! Data structures for stablecoin (Pooled Debt) contract calls
//!
//! ## Architecture: Pooled Debt (Synthetix-style)
//!
//! Unlike individual CDP models (MakerDAO) where each position is tracked separately,
//! this contract uses pooled debt where:
//!
//! - All collateral goes into a global pool
//! - All debt is pooled together
//! - Users hold "debt shares" representing their proportion of total debt
//! - No individual position tracking = simpler privacy
//!
//! ## Why Pooled Debt for Privacy
//!
//! CDP Model (MakerDAO) problems:
//! - Must prove individual position is valid (complex ZK)
//! - Liquidator sees "position ID X was liquidated" (privacy leak)
//! - Individual nullifiers/commitments for each position
//!
//! Pooled Model advantages:
//! - No individual positions to track
//! - Liquidation is "pool had shortfall" not "this person was liquidated"
//! - Simpler ZK circuits
//! - No position IDs that could leak information

use dwow_sdk::{
    crypto::{pasta_prelude::PrimeField, ContractId, IntentCommitment, IntentNullifier, Nullifier, PublicKey},
    error::ContractError,
    pasta::{group::GroupEncoding, pallas},
};
/// Namespace for stablecoin intents
pub const STABLECOIN_NAMESPACE: u64 = 0x0005;

/// Collateral type identifier
#[derive(Debug, Clone)]
pub enum CollateralType {
    /// XMR (Monero) collateral
    Xmr,
    /// DRK (DarkWow) collateral
    Drk,
    /// ETH (Ethereum) collateral - large cap, DAI-backed
    Eth,
}

impl TryFrom<u8> for CollateralType {
    type Error = ContractError;

    fn try_from(b: u8) -> Result<Self, Self::Error> {
        match b {
            0 => Ok(Self::Xmr),
            1 => Ok(Self::Drk),
            2 => Ok(Self::Eth),
            _ => Err(ContractError::InvalidFunction),
        }
    }
}

/// Stablecoin model selector - determines the behavior of the stablecoin
#[derive(Debug, Clone)]
pub enum StablecoinModel {
    /// Synthetix-style pooled debt (default)
    /// All collateral backs all debt. No individual position tracking.
    /// Liquidation is global - pool is either healthy or not.
    PooledDebt,
    /// Liquity-style: Minimum 110% collateralization
    /// Uses stability pool for redemptions. Instant liquidation.
    /// No governance, no stability fee.
    Liquity,
    /// Frax-style: Fractional collateralization (e.g., 80% collateral + 20% algorithmic)
    /// Partial backing with seigniorage share mechanism.
    Fractional,
    /// Individual CDP: Per-position tracking (complex ZK)
    /// Each position has its own collateral/debt tracked separately.
    /// More control but leaks more data.
    IndividualCdp,
}

impl TryFrom<u8> for StablecoinModel {
    type Error = ContractError;

    fn try_from(b: u8) -> Result<Self, Self::Error> {
        match b {
            0 => Ok(Self::PooledDebt),
            1 => Ok(Self::Liquity),
            2 => Ok(Self::Fractional),
            3 => Ok(Self::IndividualCdp),
            _ => Err(ContractError::InvalidFunction),
        }
    }
}

/// Per-collateral risk parameters (used for multi-collateral support)
#[derive(Debug, Clone)]
pub struct CollateralParams {
    /// Collateral type
    pub collateral_type: CollateralType,
    /// Haircut applied to this collateral (basis points, e.g., 9850 = 98.5% value)
    /// Protects against price volatility before liquidation
    pub haircut: u64,
    /// Liquidation threshold for this collateral type (basis points)
    pub liquidation_threshold: u64,
    /// Maximum share of total debt this collateral can back (basis points)
    /// e.g., 30000 = max 30% of debt can be backed by this collateral
    pub max_debt_share: u64,
}

/// What action to take when dead man switch triggers
#[derive(Debug, Clone)]
pub enum DeadManAction {
    /// Liquidate all positions at current prices (emergency settlement)
    LiquidateAll,
    /// Disable new minting but allow existing positions to remain
    DisableMinting,
    /// Allow free withdrawals without collateralization checks
    EnableFreeWithdrawals,
}

impl TryFrom<u8> for DeadManAction {
    type Error = ContractError;

    fn try_from(b: u8) -> Result<Self, Self::Error> {
        match b {
            0 => Ok(Self::LiquidateAll),
            1 => Ok(Self::DisableMinting),
            2 => Ok(Self::EnableFreeWithdrawals),
            _ => Err(ContractError::InvalidFunction),
        }
    }
}

/// Dead man switch configuration - emergency shutdown if no executive action
#[derive(Debug, Clone)]
pub struct DeadManSwitchConfig {
    /// Enable dead man switch
    pub enabled: bool,
    /// Timeout in blocks (if no executive action for this many blocks, trigger)
    pub timeout_blocks: u64,
    /// Action to take when triggered
    pub action: DeadManAction,
    /// Last executive action block (tracked internally, not set by user)
    #[doc(hidden)]
    pub last_action_block: u64,
}

/// Pooled Debt Engine initialization parameters
#[derive(Debug, Clone)]
pub struct InitializeParams {
    /// The stablecoin model to use (PooledDebt, Liquity, Fractional, IndividualCdp)
    pub model: StablecoinModel,

    /// Initial minimum collateralization ratio for pool (basis points, e.g., 15000 = 150%)
    pub min_collateralization_ratio: u64,

    /// Liquidation threshold for pool (basis points)
    pub liquidation_threshold: u64,

    /// Liquidation penalty (basis points)
    pub liquidation_penalty: u64,

    /// Base stability fee (annual rate in basis points)
    /// Note: Liquity model ignores this (no stability fee)
    pub base_rate: u64,

    /// PI controller proportional gain
    pub pi_kp: i64,

    /// PI controller integral gain
    pub pi_ki: i64,

    /// Price feed TWAP window in seconds
    pub twap_window: u64,

    /// Price deviation threshold for PI adjustment (basis points)
    pub price_deviation_threshold: u64,

    /// Per-collateral risk parameters (for multi-collateral support)
    /// If empty, uses default single-collateral (DRK) with above params
    pub collateral_params: Vec<CollateralParams>,

    /// Dead man switch configuration (emergency shutdown)
    pub dead_man_switch: DeadManSwitchConfig,

    /// Authority public key for PromissoryNote token minting authorization
    pub token_authority_pub: PublicKey,

    /// Whether to create a new PromissoryNote token for this stablecoin
    pub create_token: bool,

    /// Token symbol for the stablecoin (e.g., "USDx") - used if create_token is true
    pub token_symbol: [u8; 32],

    /// Deployer authorization for InitV1 ZK proof (poseidon_hash(deployer_secret, contract_salt))
    pub deployer_auth: pallas::Base,

    /// PromissoryNote contract ID for cross-contract validation
    pub promissory_note_contract_id: ContractId,
}

impl InitializeParams {
    /// Encode to canonical bytes (ρ-calculus: quote).
    pub fn encode(&self) -> Vec<u8> {
        let cp_count = self.collateral_params.len();
        let cap = 181 + cp_count * 25;
        let mut buf = Vec::with_capacity(cap);
        buf.push(self.model.clone() as u8);
        buf.extend_from_slice(&self.min_collateralization_ratio.to_le_bytes());
        buf.extend_from_slice(&self.liquidation_threshold.to_le_bytes());
        buf.extend_from_slice(&self.liquidation_penalty.to_le_bytes());
        buf.extend_from_slice(&self.base_rate.to_le_bytes());
        buf.extend_from_slice(&self.pi_kp.to_le_bytes());
        buf.extend_from_slice(&self.pi_ki.to_le_bytes());
        buf.extend_from_slice(&self.twap_window.to_le_bytes());
        buf.extend_from_slice(&self.price_deviation_threshold.to_le_bytes());
        buf.push(cp_count as u8);
        for cp in &self.collateral_params {
            buf.push(cp.collateral_type.clone() as u8);
            buf.extend_from_slice(&cp.haircut.to_le_bytes());
            buf.extend_from_slice(&cp.liquidation_threshold.to_le_bytes());
            buf.extend_from_slice(&cp.max_debt_share.to_le_bytes());
        }
        buf.push(self.dead_man_switch.enabled as u8);
        buf.extend_from_slice(&self.dead_man_switch.timeout_blocks.to_le_bytes());
        buf.push(self.dead_man_switch.action.clone() as u8);
        buf.extend_from_slice(&self.dead_man_switch.last_action_block.to_le_bytes());
        buf.extend_from_slice(&self.token_authority_pub.to_bytes());
        buf.push(self.create_token as u8);
        buf.extend_from_slice(&self.token_symbol);
        buf.extend_from_slice(&self.deployer_auth.to_repr());
        buf.extend_from_slice(&self.promissory_note_contract_id.to_bytes());
        buf
    }

    /// Decode from canonical bytes (ρ-calculus: eval).
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 181 {
            return Err(ContractError::IoError(format!(
                "InitializeParams: expected at least 181 bytes, got {}", data.len()
            )));
        }
        let model = StablecoinModel::decode(&data[0..1])?;
        let min_collateralization_ratio = u64::from_le_bytes(data[1..9].try_into().unwrap());
        let liquidation_threshold = u64::from_le_bytes(data[9..17].try_into().unwrap());
        let liquidation_penalty = u64::from_le_bytes(data[17..25].try_into().unwrap());
        let base_rate = u64::from_le_bytes(data[25..33].try_into().unwrap());
        let pi_kp = i64::from_le_bytes(data[33..41].try_into().unwrap());
        let pi_ki = i64::from_le_bytes(data[41..49].try_into().unwrap());
        let twap_window = u64::from_le_bytes(data[49..57].try_into().unwrap());
        let price_deviation_threshold = u64::from_le_bytes(data[57..65].try_into().unwrap());
        let cp_count = data[65] as usize;
        let dm_start = 66 + cp_count * 25;
        if data.len() < dm_start + 18 + 97 {
            return Err(ContractError::IoError(format!(
                "InitializeParams: expected at least {} bytes for {} collateral_params, got {}",
                dm_start + 18 + 97, cp_count, data.len()
            )));
        }
        let mut collateral_params = Vec::with_capacity(cp_count);
        for i in 0..cp_count {
            let off = 66 + i * 25;
            let collateral_type = CollateralType::decode(&data[off..off + 1])?;
            let haircut = u64::from_le_bytes(data[off + 1..off + 9].try_into().unwrap());
            let lt = u64::from_le_bytes(data[off + 9..off + 17].try_into().unwrap());
            let max_debt_share = u64::from_le_bytes(data[off + 17..off + 25].try_into().unwrap());
            collateral_params.push(CollateralParams { collateral_type, haircut, liquidation_threshold: lt, max_debt_share });
        }
        let enabled = data[dm_start] != 0;
        let timeout_blocks = u64::from_le_bytes(data[dm_start + 1..dm_start + 9].try_into().unwrap());
        let action = DeadManAction::try_from(data[dm_start + 9])?;
        let last_action_block = u64::from_le_bytes(data[dm_start + 10..dm_start + 18].try_into().unwrap());
        let dead_man_switch = DeadManSwitchConfig { enabled, timeout_blocks, action, last_action_block };
        let token_authority_pub = PublicKey::from_bytes(data[dm_start + 18..dm_start + 50].try_into().unwrap())
            .map_err(|e| ContractError::IoError(format!("InitializeParams: invalid token_authority_pub: {}", e)))?;
        let create_token = data[dm_start + 50] != 0;
        let token_symbol: [u8; 32] = data[dm_start + 51..dm_start + 83].try_into().unwrap();
        let deployer_auth = Option::<pallas::Base>::from(pallas::Base::from_repr(data[dm_start + 83..dm_start + 115].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("InitializeParams: invalid deployer_auth".into()))?;
        let promissory_note_contract_id = ContractId::from_bytes(data[dm_start + 115..dm_start + 147].try_into().unwrap())
            .map_err(|_| ContractError::IoError("InitializeParams: invalid promissory_note_contract_id".into()))?;
        Ok(InitializeParams {
            model, min_collateralization_ratio, liquidation_threshold, liquidation_penalty,
            base_rate, pi_kp, pi_ki, twap_window, price_deviation_threshold,
            collateral_params, dead_man_switch, token_authority_pub, create_token,
            token_symbol, deployer_auth, promissory_note_contract_id,
        })
    }
}

/// Deposit collateral into the pool
#[derive(Debug, Clone)]
pub struct DepositCollateralParams {
    /// Commitment to the deposit (hides amount)
    pub deposit_commitment: IntentCommitment,

    /// Collateral amount (hidden in commitment)
    pub collateral_amount: u64,

    /// Collateral type
    pub collateral_type: CollateralType,

    /// ZK proof: deposit is valid
    pub proof: Vec<u8>,

    /// Fee paid for this operation
    pub fee: u64,

    /// ZK public inputs for proof verification: [position_nullifier, position_commitment]
    /// The prover computes these from their secret values
    pub zk_public_inputs: Vec<pallas::Base>,
}

impl DepositCollateralParams {
    pub fn encode(&self) -> Vec<u8> {
        let cap = 51 + self.proof.len() + self.zk_public_inputs.len() * 32;
        let mut buf = Vec::with_capacity(cap);
        buf.extend_from_slice(&self.deposit_commitment.to_bytes());
        buf.extend_from_slice(&self.collateral_amount.to_le_bytes());
        buf.extend_from_slice(&self.collateral_type.encode());
        buf.push(self.proof.len() as u8);
        buf.extend_from_slice(&self.proof);
        buf.extend_from_slice(&self.fee.to_le_bytes());
        buf.push(self.zk_public_inputs.len() as u8);
        for pi in &self.zk_public_inputs {
            buf.extend_from_slice(&pi.to_repr());
        }
        buf
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 52 {
            return Err(ContractError::IoError(format!(
                "DepositCollateralParams: expected at least 52 bytes, got {}", data.len()
            )));
        }
        let deposit_commitment = IntentCommitment::from_bytes(data[0..32].try_into().unwrap())
            .map_err(|_| ContractError::IoError("DepositCollateralParams: invalid deposit_commitment".into()))?;
        let collateral_amount = u64::from_le_bytes(data[32..40].try_into().unwrap());
        let collateral_type = CollateralType::decode(&data[40..41])?;
        let proof_len = data[41] as usize;
        if data.len() < 42 + proof_len + 9 {
            return Err(ContractError::IoError(format!(
                "DepositCollateralParams: proof truncated at offset {}", 42 + proof_len
            )));
        }
        let proof = data[42..42 + proof_len].to_vec();
        let fee = u64::from_le_bytes(data[42 + proof_len..50 + proof_len].try_into().unwrap());
        let pi_count = data[50 + proof_len] as usize;
        let expected = 51 + proof_len + pi_count * 32;
        if data.len() != expected {
            return Err(ContractError::IoError(format!(
                "DepositCollateralParams: expected {} bytes, got {}", expected, data.len()
            )));
        }
        let mut zk_public_inputs = Vec::with_capacity(pi_count);
        for i in 0..pi_count {
            let start = 51 + proof_len + i * 32;
            zk_public_inputs.push(
                Option::<pallas::Base>::from(pallas::Base::from_repr(data[start..start + 32].try_into().unwrap()))
                    .ok_or_else(|| ContractError::IoError(format!("DepositCollateralParams: invalid zk_public_inputs[{}]", i)))?,
            );
        }
        Ok(DepositCollateralParams { deposit_commitment, collateral_amount, collateral_type, proof, fee, zk_public_inputs })
    }
}

/// Withdraw collateral from the pool (only if collateralization ratio allows)
#[derive(Debug, Clone)]
pub struct WithdrawCollateralParams {
    /// Nullifier to prove withdrawal is authorized
    pub withdrawal_nullifier: IntentNullifier,

    /// New commitment after withdrawal
    pub new_commitment: IntentCommitment,

    /// Amount of collateral to withdraw
    pub withdraw_amount: u64,

    /// ZK proof: withdrawal doesn't violate pool collateralization
    pub proof: Vec<u8>,

    /// Fee paid for this operation
    pub fee: u64,

    /// ZK public inputs for proof verification: [nullifier]
    /// The prover computes the nullifier from their secret
    pub zk_public_inputs: Vec<pallas::Base>,
}

impl WithdrawCollateralParams {
    pub fn encode(&self) -> Vec<u8> {
        let cap = 83 + self.proof.len() + self.zk_public_inputs.len() * 32;
        let mut buf = Vec::with_capacity(cap);
        buf.extend_from_slice(&self.withdrawal_nullifier.to_bytes());
        buf.extend_from_slice(&self.new_commitment.to_bytes());
        buf.extend_from_slice(&self.withdraw_amount.to_le_bytes());
        buf.push(self.proof.len() as u8);
        buf.extend_from_slice(&self.proof);
        buf.extend_from_slice(&self.fee.to_le_bytes());
        buf.push(self.zk_public_inputs.len() as u8);
        for pi in &self.zk_public_inputs {
            buf.extend_from_slice(&pi.to_repr());
        }
        buf
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 84 {
            return Err(ContractError::IoError(format!(
                "WithdrawCollateralParams: expected at least 84 bytes, got {}", data.len()
            )));
        }
        let withdrawal_nullifier = IntentNullifier::from_bytes(data[0..32].try_into().unwrap())
            .map_err(|_| ContractError::IoError("WithdrawCollateralParams: invalid withdrawal_nullifier".into()))?;
        let new_commitment = IntentCommitment::from_bytes(data[32..64].try_into().unwrap())
            .map_err(|_| ContractError::IoError("WithdrawCollateralParams: invalid new_commitment".into()))?;
        let withdraw_amount = u64::from_le_bytes(data[64..72].try_into().unwrap());
        let proof_len = data[72] as usize;
        if data.len() < 73 + proof_len + 9 {
            return Err(ContractError::IoError(format!(
                "WithdrawCollateralParams: proof truncated at offset {}", 73 + proof_len
            )));
        }
        let proof = data[73..73 + proof_len].to_vec();
        let fee = u64::from_le_bytes(data[73 + proof_len..81 + proof_len].try_into().unwrap());
        let pi_count = data[81 + proof_len] as usize;
        let expected = 82 + proof_len + pi_count * 32;
        if data.len() != expected {
            return Err(ContractError::IoError(format!(
                "WithdrawCollateralParams: expected {} bytes, got {}", expected, data.len()
            )));
        }
        let mut zk_public_inputs = Vec::with_capacity(pi_count);
        for i in 0..pi_count {
            let start = 82 + proof_len + i * 32;
            zk_public_inputs.push(
                Option::<pallas::Base>::from(pallas::Base::from_repr(data[start..start + 32].try_into().unwrap()))
                    .ok_or_else(|| ContractError::IoError(format!("WithdrawCollateralParams: invalid zk_public_inputs[{}]", i)))?,
            );
        }
        Ok(WithdrawCollateralParams { withdrawal_nullifier, new_commitment, withdraw_amount, proof, fee, zk_public_inputs })
    }
}

/// Mint stablecoin against collateral pool
#[derive(Debug, Clone)]
pub struct MintStableParams {
    /// Commitment to the mint (hides debt amount)
    pub mint_commitment: IntentCommitment,

    /// Amount of stablecoin to mint
    pub mint_amount: u64,

    /// Current total debt in pool (for ratio check)
    pub total_debt: u64,

    /// Current total collateral in pool (for ratio check)
    pub total_collateral: u64,

    /// ZK proof: mint doesn't violate pool collateralization
    pub proof: Vec<u8>,

    /// Fee paid for this operation
    pub fee: u64,

    /// ZK public inputs for proof verification: [old_commitment, new_commitment, position_nullifier]
    /// The prover computes these from their secret values
    pub zk_public_inputs: Vec<pallas::Base>,
}

impl MintStableParams {
    pub fn encode(&self) -> Vec<u8> {
        let cap = 66 + self.proof.len() + self.zk_public_inputs.len() * 32;
        let mut buf = Vec::with_capacity(cap);
        buf.extend_from_slice(&self.mint_commitment.to_bytes());
        buf.extend_from_slice(&self.mint_amount.to_le_bytes());
        buf.extend_from_slice(&self.total_debt.to_le_bytes());
        buf.extend_from_slice(&self.total_collateral.to_le_bytes());
        buf.push(self.proof.len() as u8);
        buf.extend_from_slice(&self.proof);
        buf.extend_from_slice(&self.fee.to_le_bytes());
        buf.push(self.zk_public_inputs.len() as u8);
        for pi in &self.zk_public_inputs {
            buf.extend_from_slice(&pi.to_repr());
        }
        buf
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 67 {
            return Err(ContractError::IoError(format!(
                "MintStableParams: expected at least 67 bytes, got {}", data.len()
            )));
        }
        let mint_commitment = IntentCommitment::from_bytes(data[0..32].try_into().unwrap())
            .map_err(|_| ContractError::IoError("MintStableParams: invalid mint_commitment".into()))?;
        let mint_amount = u64::from_le_bytes(data[32..40].try_into().unwrap());
        let total_debt = u64::from_le_bytes(data[40..48].try_into().unwrap());
        let total_collateral = u64::from_le_bytes(data[48..56].try_into().unwrap());
        let proof_len = data[56] as usize;
        if data.len() < 57 + proof_len + 9 {
            return Err(ContractError::IoError(format!(
                "MintStableParams: proof truncated at offset {}", 57 + proof_len
            )));
        }
        let proof = data[57..57 + proof_len].to_vec();
        let fee = u64::from_le_bytes(data[57 + proof_len..65 + proof_len].try_into().unwrap());
        let pi_count = data[65 + proof_len] as usize;
        let expected = 66 + proof_len + pi_count * 32;
        if data.len() != expected {
            return Err(ContractError::IoError(format!(
                "MintStableParams: expected {} bytes, got {}", expected, data.len()
            )));
        }
        let mut zk_public_inputs = Vec::with_capacity(pi_count);
        for i in 0..pi_count {
            let start = 66 + proof_len + i * 32;
            zk_public_inputs.push(
                Option::<pallas::Base>::from(pallas::Base::from_repr(data[start..start + 32].try_into().unwrap()))
                    .ok_or_else(|| ContractError::IoError(format!("MintStableParams: invalid zk_public_inputs[{}]", i)))?,
            );
        }
        Ok(MintStableParams { mint_commitment, mint_amount, total_debt, total_collateral, proof, fee, zk_public_inputs })
    }
}

/// Repay stablecoin debt to reduce debt share
#[derive(Debug, Clone)]
pub struct RepayStableParams {
    /// Commitment to the repayment
    pub repay_commitment: IntentCommitment,

    /// Amount of stablecoin to burn (repay)
    pub repay_amount: u64,

    /// ZK proof: repayment is valid
    pub proof: Vec<u8>,

    /// Fee paid for this operation
    pub fee: u64,

    /// ZK public inputs for proof verification: [commitment]
    /// The prover computes the commitment from their secret values
    pub zk_public_inputs: Vec<pallas::Base>,
}

impl RepayStableParams {
    pub fn encode(&self) -> Vec<u8> {
        let cap = 50 + self.proof.len() + self.zk_public_inputs.len() * 32;
        let mut buf = Vec::with_capacity(cap);
        buf.extend_from_slice(&self.repay_commitment.to_bytes());
        buf.extend_from_slice(&self.repay_amount.to_le_bytes());
        buf.push(self.proof.len() as u8);
        buf.extend_from_slice(&self.proof);
        buf.extend_from_slice(&self.fee.to_le_bytes());
        buf.push(self.zk_public_inputs.len() as u8);
        for pi in &self.zk_public_inputs {
            buf.extend_from_slice(&pi.to_repr());
        }
        buf
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 51 {
            return Err(ContractError::IoError(format!(
                "RepayStableParams: expected at least 51 bytes, got {}", data.len()
            )));
        }
        let repay_commitment = IntentCommitment::from_bytes(data[0..32].try_into().unwrap())
            .map_err(|_| ContractError::IoError("RepayStableParams: invalid repay_commitment".into()))?;
        let repay_amount = u64::from_le_bytes(data[32..40].try_into().unwrap());
        let proof_len = data[40] as usize;
        if data.len() < 41 + proof_len + 9 {
            return Err(ContractError::IoError(format!(
                "RepayStableParams: proof truncated at offset {}", 41 + proof_len
            )));
        }
        let proof = data[41..41 + proof_len].to_vec();
        let fee = u64::from_le_bytes(data[41 + proof_len..49 + proof_len].try_into().unwrap());
        let pi_count = data[49 + proof_len] as usize;
        let expected = 50 + proof_len + pi_count * 32;
        if data.len() != expected {
            return Err(ContractError::IoError(format!(
                "RepayStableParams: expected {} bytes, got {}", expected, data.len()
            )));
        }
        let mut zk_public_inputs = Vec::with_capacity(pi_count);
        for i in 0..pi_count {
            let start = 50 + proof_len + i * 32;
            zk_public_inputs.push(
                Option::<pallas::Base>::from(pallas::Base::from_repr(data[start..start + 32].try_into().unwrap()))
                    .ok_or_else(|| ContractError::IoError(format!("RepayStableParams: invalid zk_public_inputs[{}]", i)))?,
            );
        }
        Ok(RepayStableParams { repay_commitment, repay_amount, proof, fee, zk_public_inputs })
    }
}

/// Liquidate pool if undercollateralized
///
/// Note: In pooled model, liquidation is global - the entire pool is either
/// liquidated or not. Individual users' collateral is seized proportionally.
#[derive(Debug, Clone)]
pub struct LiquidateParams {
    /// Liquidation commitment
    pub liquidation_commitment: IntentCommitment,

    /// Current total debt in pool
    pub total_debt: u64,

    /// Current total collateral in pool
    pub total_collateral: u64,

    /// Current TWAP price
    pub current_price: u64,

    /// Amount of debt to cover
    pub debt_to_cover: u64,

    /// ZK proof: pool is undercollateralized
    pub proof: Vec<u8>,

    /// Liquidation reward
    pub liquidation_reward: u64,

    /// Fee paid for this operation
    pub fee: u64,

    /// ZK public inputs for proof verification: [old_commitment, new_commitment, position_nullifier]
    /// The prover computes these from their secret values
    pub zk_public_inputs: Vec<pallas::Base>,
}

impl LiquidateParams {
    pub fn encode(&self) -> Vec<u8> {
        let cap = 82 + self.proof.len() + self.zk_public_inputs.len() * 32;
        let mut buf = Vec::with_capacity(cap);
        buf.extend_from_slice(&self.liquidation_commitment.to_bytes());
        buf.extend_from_slice(&self.total_debt.to_le_bytes());
        buf.extend_from_slice(&self.total_collateral.to_le_bytes());
        buf.extend_from_slice(&self.current_price.to_le_bytes());
        buf.extend_from_slice(&self.debt_to_cover.to_le_bytes());
        buf.push(self.proof.len() as u8);
        buf.extend_from_slice(&self.proof);
        buf.extend_from_slice(&self.liquidation_reward.to_le_bytes());
        buf.extend_from_slice(&self.fee.to_le_bytes());
        buf.push(self.zk_public_inputs.len() as u8);
        for pi in &self.zk_public_inputs {
            buf.extend_from_slice(&pi.to_repr());
        }
        buf
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 83 {
            return Err(ContractError::IoError(format!(
                "LiquidateParams: expected at least 83 bytes, got {}", data.len()
            )));
        }
        let liquidation_commitment = IntentCommitment::from_bytes(data[0..32].try_into().unwrap())
            .map_err(|_| ContractError::IoError("LiquidateParams: invalid liquidation_commitment".into()))?;
        let total_debt = u64::from_le_bytes(data[32..40].try_into().unwrap());
        let total_collateral = u64::from_le_bytes(data[40..48].try_into().unwrap());
        let current_price = u64::from_le_bytes(data[48..56].try_into().unwrap());
        let debt_to_cover = u64::from_le_bytes(data[56..64].try_into().unwrap());
        let proof_len = data[64] as usize;
        if data.len() < 65 + proof_len + 17 {
            return Err(ContractError::IoError(format!(
                "LiquidateParams: proof truncated at offset {}", 65 + proof_len
            )));
        }
        let proof = data[65..65 + proof_len].to_vec();
        let liquidation_reward = u64::from_le_bytes(data[65 + proof_len..73 + proof_len].try_into().unwrap());
        let fee = u64::from_le_bytes(data[73 + proof_len..81 + proof_len].try_into().unwrap());
        let pi_count = data[81 + proof_len] as usize;
        let expected = 82 + proof_len + pi_count * 32;
        if data.len() != expected {
            return Err(ContractError::IoError(format!(
                "LiquidateParams: expected {} bytes, got {}", expected, data.len()
            )));
        }
        let mut zk_public_inputs = Vec::with_capacity(pi_count);
        for i in 0..pi_count {
            let start = 82 + proof_len + i * 32;
            zk_public_inputs.push(
                Option::<pallas::Base>::from(pallas::Base::from_repr(data[start..start + 32].try_into().unwrap()))
                    .ok_or_else(|| ContractError::IoError(format!("LiquidateParams: invalid zk_public_inputs[{}]", i)))?,
            );
        }
        Ok(LiquidateParams { liquidation_commitment, total_debt, total_collateral, current_price, debt_to_cover, proof, liquidation_reward, fee, zk_public_inputs })
    }
}

/// Update pool configuration (governance)
#[derive(Debug, Clone)]
pub struct UpdateConfigParams {
    /// New minimum collateralization ratio
    pub min_collateralization_ratio: u64,

    /// New liquidation threshold
    pub liquidation_threshold: u64,

    /// New liquidation penalty
    pub liquidation_penalty: u64,

    /// New base rate
    pub base_rate: u64,

    /// New PI controller Kp
    pub pi_kp: i64,

    /// New PI controller Ki
    pub pi_ki: i64,

    /// New TWAP window
    pub twap_window: u64,

    /// New price deviation threshold
    pub price_deviation_threshold: u64,

    /// Governance public key X (ZK-verified)
    pub gov_pub_x: pallas::Base,
    /// Governance public key Y (ZK-verified)
    pub gov_pub_y: pallas::Base,
    /// Nullifier = H(gov_pub_x, gov_pub_y, gov_secret) for ZK replay protection
    pub config_nullifier: pallas::Base,
}

impl UpdateConfigParams {
    /// Fixed canonical byte size: 8×64bit(64) + 3×pallas::Base(96)
    pub const ENCODED_SIZE: usize = 160;

    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.extend_from_slice(&self.min_collateralization_ratio.to_le_bytes());
        buf.extend_from_slice(&self.liquidation_threshold.to_le_bytes());
        buf.extend_from_slice(&self.liquidation_penalty.to_le_bytes());
        buf.extend_from_slice(&self.base_rate.to_le_bytes());
        buf.extend_from_slice(&self.pi_kp.to_le_bytes());
        buf.extend_from_slice(&self.pi_ki.to_le_bytes());
        buf.extend_from_slice(&self.twap_window.to_le_bytes());
        buf.extend_from_slice(&self.price_deviation_threshold.to_le_bytes());
        buf.extend_from_slice(&self.gov_pub_x.to_repr());
        buf.extend_from_slice(&self.gov_pub_y.to_repr());
        buf.extend_from_slice(&self.config_nullifier.to_repr());
        buf
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "UpdateConfigParams: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len()
            )));
        }
        let min_collateralization_ratio = u64::from_le_bytes(data[0..8].try_into().unwrap());
        let liquidation_threshold = u64::from_le_bytes(data[8..16].try_into().unwrap());
        let liquidation_penalty = u64::from_le_bytes(data[16..24].try_into().unwrap());
        let base_rate = u64::from_le_bytes(data[24..32].try_into().unwrap());
        let pi_kp = i64::from_le_bytes(data[32..40].try_into().unwrap());
        let pi_ki = i64::from_le_bytes(data[40..48].try_into().unwrap());
        let twap_window = u64::from_le_bytes(data[48..56].try_into().unwrap());
        let price_deviation_threshold = u64::from_le_bytes(data[56..64].try_into().unwrap());
        let gov_pub_x = Option::<pallas::Base>::from(pallas::Base::from_repr(data[64..96].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("UpdateConfigParams: invalid gov_pub_x".into()))?;
        let gov_pub_y = Option::<pallas::Base>::from(pallas::Base::from_repr(data[96..128].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("UpdateConfigParams: invalid gov_pub_y".into()))?;
        let config_nullifier = Option::<pallas::Base>::from(pallas::Base::from_repr(data[128..160].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("UpdateConfigParams: invalid config_nullifier".into()))?;
        Ok(UpdateConfigParams {
            min_collateralization_ratio,
            liquidation_threshold,
            liquidation_penalty,
            base_rate,
            pi_kp,
            pi_ki,
            twap_window,
            price_deviation_threshold,
            gov_pub_x,
            gov_pub_y,
            config_nullifier,
        })
    }
}

/// Update data for configuration changes (sent from instruction to update phase)
#[derive(Debug, Clone)]
pub struct UpdateConfigUpdateV1 {
    /// New minimum collateralization ratio
    pub min_collateralization_ratio: u64,

    /// New liquidation threshold
    pub liquidation_threshold: u64,

    /// New liquidation penalty
    pub liquidation_penalty: u64,

    /// New base rate
    pub base_rate: u64,

    /// New PI controller Kp
    pub pi_kp: i64,

    /// New PI controller Ki
    pub pi_ki: i64,

    /// New TWAP window
    pub twap_window: u64,

    /// New price deviation threshold
    pub price_deviation_threshold: u64,
    /// Config nullifier for ZK replay protection
    pub config_nullifier: pallas::Base,
}

/// Update data for adding collateral
#[derive(Debug, Clone)]
pub struct AddCollateralUpdateV1 {
    /// The position commitment (for tracking)
    pub position_commitment: IntentCommitment,
    /// Additional collateral amount
    pub added_collateral: u64,
    /// Collateral type
    pub collateral_type: CollateralType,
}

/// Update data for removing collateral
#[derive(Debug, Clone)]
pub struct RemoveCollateralUpdateV1 {
    /// The position nullifier (proves ownership)
    pub position_nullifier: IntentNullifier,
    /// New commitment after withdrawal
    pub new_commitment: IntentCommitment,
    /// Collateral type
    pub collateral_type: CollateralType,
    /// Amount removed
    pub removed_collateral: u64,
}

/// Update data for minting stablecoin
#[derive(Debug, Clone)]
pub struct MintStableUpdateV1 {
    /// Position commitment
    pub position_commitment: IntentCommitment,
    /// Amount minted
    pub mint_amount: u64,
    /// New total debt after minting
    pub new_total_debt: u64,
}

/// Update data for repaying stablecoin debt
#[derive(Debug, Clone)]
pub struct RepayStableUpdateV1 {
    /// Position nullifier
    pub position_nullifier: IntentNullifier,
    /// New commitment after repayment
    pub new_commitment: IntentCommitment,
    /// Amount repaid
    pub repay_amount: u64,
    /// New total debt after repayment
    pub new_total_debt: u64,
}

/// Update data for liquidating the pool
#[derive(Debug, Clone)]
pub struct LiquidateUpdateV1 {
    /// Liquidation record
    pub debt_covered: u64,
    /// Collateral seized
    pub collateral_seized: u64,
    /// Liquidation penalty
    pub penalty: u64,
    /// New total debt after liquidation
    pub new_total_debt: u64,
    /// New total collateral after liquidation
    pub new_total_collateral: u64,
}

/// Update data for governance report — persisted on-chain for public audit
#[derive(Debug, Clone)]
pub struct GovernanceReportUpdateV1 {
    /// Token ID being reported on
    pub token_id: pallas::Base,
    /// Total collateral verified on-chain
    pub total_collateral: u64,
    /// Total debt verified on-chain
    pub total_debt: u64,
    /// Total redeemed verified on-chain
    pub total_redeemed: u64,
    /// Outstanding circulation = total_debt - total_redeemed
    pub outstanding: u64,
    /// Collateral ratio in basis points (collateral / outstanding * 10000)
    pub collateral_ratio_bps: u64,
    /// Interest accrued since last report
    pub interest_accrued: u64,
    /// Block height when report was created
    pub report_block: u64,
    /// Reporter's public key
    pub reporter_pub: PublicKey,
}

/// Update data for interest accrual
#[derive(Debug, Clone)]
pub struct AccrueInterestUpdateV1 {
    /// Old total debt before accrual
    pub old_total_debt: u64,
    /// New total debt after accrual
    pub new_total_debt: u64,
    /// Interest amount accrued
    pub interest_amount: u64,
    /// Accumulator's public key
    pub accumulator_pub: PublicKey,
}

/// Redeem stablecoins for underlying collateral
///
/// The first application-layer consumer of PN::RedeemV1 (0x01).
/// Burns stablecoins and returns proportional collateral to the redeemer.
#[derive(Debug, Clone)]
pub struct RedeemStableParamsV1 {
    /// Recipient's public key (who receives the receipt coin)
    pub recipient_pub: PublicKey,
    /// Amount of stablecoins to redeem
    pub redeem_amount: u64,
    /// Token ID of the stablecoin being redeemed
    pub token_id: pallas::Base,
    /// Receipt coin's spend_hook (passed through to PN::RedeemV1)
    pub receipt_spend_hook: pallas::Base,
    /// Current total debt before redemption
    pub total_debt: u64,
    /// Current total collateral before redemption
    pub total_collateral: u64,
    /// ZK proof: redeem_stable_v1.zk
    pub proof: Vec<u8>,
    /// Fee paid for this operation
    pub fee: u64,
    /// ZK public inputs for proof verification
    pub zk_public_inputs: Vec<pallas::Base>,
}

impl RedeemStableParamsV1 {
    pub fn encode(&self) -> Vec<u8> {
        let cap = 130 + self.proof.len() + self.zk_public_inputs.len() * 32;
        let mut buf = Vec::with_capacity(cap);
        buf.extend_from_slice(&self.recipient_pub.to_bytes());
        buf.extend_from_slice(&self.redeem_amount.to_le_bytes());
        buf.extend_from_slice(&self.token_id.to_repr());
        buf.extend_from_slice(&self.receipt_spend_hook.to_repr());
        buf.extend_from_slice(&self.total_debt.to_le_bytes());
        buf.extend_from_slice(&self.total_collateral.to_le_bytes());
        buf.push(self.proof.len() as u8);
        buf.extend_from_slice(&self.proof);
        buf.extend_from_slice(&self.fee.to_le_bytes());
        buf.push(self.zk_public_inputs.len() as u8);
        for pi in &self.zk_public_inputs {
            buf.extend_from_slice(&pi.to_repr());
        }
        buf
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 131 {
            return Err(ContractError::IoError(format!(
                "RedeemStableParamsV1: expected at least 131 bytes, got {}", data.len()
            )));
        }
        let recipient_pub = PublicKey::from_bytes(data[0..32].try_into().unwrap())
            .map_err(|e| ContractError::IoError(format!("RedeemStableParamsV1: invalid recipient_pub: {}", e)))?;
        let redeem_amount = u64::from_le_bytes(data[32..40].try_into().unwrap());
        let token_id = Option::<pallas::Base>::from(pallas::Base::from_repr(data[40..72].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("RedeemStableParamsV1: invalid token_id".into()))?;
        let receipt_spend_hook = Option::<pallas::Base>::from(pallas::Base::from_repr(data[72..104].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("RedeemStableParamsV1: invalid receipt_spend_hook".into()))?;
        let total_debt = u64::from_le_bytes(data[104..112].try_into().unwrap());
        let total_collateral = u64::from_le_bytes(data[112..120].try_into().unwrap());
        let proof_len = data[120] as usize;
        if data.len() < 121 + proof_len + 9 {
            return Err(ContractError::IoError(format!(
                "RedeemStableParamsV1: proof truncated at offset {}", 121 + proof_len
            )));
        }
        let proof = data[121..121 + proof_len].to_vec();
        let fee = u64::from_le_bytes(data[121 + proof_len..129 + proof_len].try_into().unwrap());
        let pi_count = data[129 + proof_len] as usize;
        let expected = 130 + proof_len + pi_count * 32;
        if data.len() != expected {
            return Err(ContractError::IoError(format!(
                "RedeemStableParamsV1: expected {} bytes, got {}", expected, data.len()
            )));
        }
        let mut zk_public_inputs = Vec::with_capacity(pi_count);
        for i in 0..pi_count {
            let start = 130 + proof_len + i * 32;
            zk_public_inputs.push(
                Option::<pallas::Base>::from(pallas::Base::from_repr(data[start..start + 32].try_into().unwrap()))
                    .ok_or_else(|| ContractError::IoError(format!("RedeemStableParamsV1: invalid zk_public_inputs[{}]", i)))?,
            );
        }
        Ok(RedeemStableParamsV1 { recipient_pub, redeem_amount, token_id, receipt_spend_hook, total_debt, total_collateral, proof, fee, zk_public_inputs })
    }
}

/// Update data for stablecoin redemption
#[derive(Debug, Clone)]
pub struct RedeemStableUpdateV1 {
    /// Nullifier of the redeemed coin (prevents double-redeem)
    pub redeem_nullifier: pallas::Base,
    /// Receipt coin from PN::RedeemV1 child call
    pub receipt_coin: [u8; 32],
    /// Amount of stablecoins redeemed
    pub redeem_amount: u64,
    /// New total debt after redemption
    pub new_total_debt: u64,
    /// New total collateral after redemption
    pub new_total_collateral: u64,
    /// New total redeemed (cumulative)
    pub new_total_redeemed: u64,
}

// ============================================================================
// POOLED DEBT STATE (not per-user positions)
// ============================================================================

/// Global debt pool state
#[derive(Debug, Clone)]
pub struct DebtPool {
    /// Total debt in the pool (all stablecoins minted)
    pub total_debt: u64,

    /// Total collateral value (in stablecoin terms)
    pub total_collateral: u64,

    /// Accumulated fees from interest
    pub accumulated_fees: u64,

    /// Last update timestamp
    pub last_update: u64,
}

/// Collateral pool for a specific collateral type
#[derive(Debug, Clone)]
pub struct CollateralPool {
    /// Collateral type
    pub collateral_type: CollateralType,

    /// Total amount of this collateral type deposited
    pub total_deposited: u64,

    /// Current value ratio (to stablecoin)
    pub value_ratio: u64,

    /// Last update timestamp
    pub last_update: u64,
}

/// User's debt share record
///
/// Note: In the pooled model, we don't track individual positions.
/// Instead, users have "debt shares" that represent their proportion
/// of the total debt. Their actual collateral is pooled.
#[derive(Debug, Clone)]
pub struct DebtShare {
    /// Owner's public key
    pub owner_pub: PublicKey,

    /// Amount of stablecoin debt they owe
    pub debt_amount: u64,

    /// Commitment for this debt position
    pub commitment: IntentCommitment,

    /// Creation timestamp
    pub created_at: u64,

    /// Last update timestamp
    pub updated_at: u64,
}

/// PI Controller state for redemption rate
#[derive(Debug, Clone)]
pub struct PiControllerState {
    /// Current integral term
    pub integral: i64,

    /// Last update timestamp
    pub last_update: u64,

    /// Current redemption rate (basis points per second)
    pub current_rate: u64,

    /// Last known TWAP price
    pub last_twap: u64,
}

/// Price feed data from AMM TWAP
#[derive(Debug, Clone)]
pub struct PriceFeed {
    /// Current TWAP price
    pub twap: u64,

    /// TWAP window start
    pub window_start: u64,

    /// TWAP window end
    pub window_end: u64,

    /// Pool reserve0
    pub reserve0: u64,

    /// Pool reserve1
    pub reserve1: u64,

    /// Block timestamp
    pub timestamp: u64,
}

// ============================================================================
// Cold/Precise Operations (BaseDiv - expensive but accurate)
// ============================================================================

/// Governance report parameters (cold/precise - uses BaseDiv)
///
/// Proves on-chain that `total_collateral >= outstanding_circulation` per token type,
/// cryptographically guaranteeing no fractional reserving. The ZK circuit witness
/// computes `collateral_ratio_bps = base_div(total_collateral, outstanding_circulation)`
/// and the circuit constrains this against the public inputs.
///
/// The entrypoint MUST verify that `total_collateral`, `total_debt`, and
/// `total_redeemed` match the on-chain config DB values before accepting the report.
/// This prevents a malicious reporter from submitting a valid ZK proof against
/// cherry-picked inputs that don't reflect the actual contract state.
#[derive(Debug, Clone)]
pub struct GovernanceReportParams {
    /// Token ID being reported on (the stablecoin token)
    pub token_id: pallas::Base,

    /// Reported total collateral in pool (must match on-chain config)
    pub total_collateral: u64,

    /// Reported total debt in pool (must match on-chain config)
    pub total_debt: u64,

    /// Reported total redeemed (must match on-chain spend_hook nullifier count)
    pub total_redeemed: u64,

    /// Outstanding circulation = total_debt - total_redeemed
    pub outstanding: u64,

    /// Collateral ratio in basis points: base_div(total_collateral, outstanding) * 10000
    /// Must be >= 10000 (100%) to prove full collateral coverage — no fractional reserving
    pub collateral_ratio_bps: u64,

    /// Interest accrued since last report
    pub interest_accrued: u64,

    /// Timestamp of this report
    pub report_timestamp: u64,

    /// Reporter's public key
    pub reporter_pub: PublicKey,

    /// ZK proof: governance_report_v1.zk
    pub proof: Vec<u8>,

    /// Fee paid for this operation
    pub fee: u64,
}

impl GovernanceReportParams {
    pub fn encode(&self) -> Vec<u8> {
        let cap = 129 + self.proof.len();
        let mut buf = Vec::with_capacity(cap);
        buf.extend_from_slice(&self.token_id.to_repr());
        buf.extend_from_slice(&self.total_collateral.to_le_bytes());
        buf.extend_from_slice(&self.total_debt.to_le_bytes());
        buf.extend_from_slice(&self.total_redeemed.to_le_bytes());
        buf.extend_from_slice(&self.outstanding.to_le_bytes());
        buf.extend_from_slice(&self.collateral_ratio_bps.to_le_bytes());
        buf.extend_from_slice(&self.interest_accrued.to_le_bytes());
        buf.extend_from_slice(&self.report_timestamp.to_le_bytes());
        buf.extend_from_slice(&self.reporter_pub.to_bytes());
        buf.push(self.proof.len() as u8);
        buf.extend_from_slice(&self.proof);
        buf.extend_from_slice(&self.fee.to_le_bytes());
        buf
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 130 {
            return Err(ContractError::IoError(format!(
                "GovernanceReportParams: expected at least 130 bytes, got {}", data.len()
            )));
        }
        let token_id = Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("GovernanceReportParams: invalid token_id".into()))?;
        let total_collateral = u64::from_le_bytes(data[32..40].try_into().unwrap());
        let total_debt = u64::from_le_bytes(data[40..48].try_into().unwrap());
        let total_redeemed = u64::from_le_bytes(data[48..56].try_into().unwrap());
        let outstanding = u64::from_le_bytes(data[56..64].try_into().unwrap());
        let collateral_ratio_bps = u64::from_le_bytes(data[64..72].try_into().unwrap());
        let interest_accrued = u64::from_le_bytes(data[72..80].try_into().unwrap());
        let report_timestamp = u64::from_le_bytes(data[80..88].try_into().unwrap());
        let reporter_pub = PublicKey::from_bytes(data[88..120].try_into().unwrap())
            .map_err(|e| ContractError::IoError(format!("GovernanceReportParams: invalid reporter_pub: {}", e)))?;
        let proof_len = data[120] as usize;
        let expected = 121 + proof_len + 8;
        if data.len() != expected {
            return Err(ContractError::IoError(format!(
                "GovernanceReportParams: expected {} bytes, got {}", expected, data.len()
            )));
        }
        let proof = data[121..121 + proof_len].to_vec();
        let fee = u64::from_le_bytes(data[121 + proof_len..121 + proof_len + 8].try_into().unwrap());
        Ok(GovernanceReportParams {
            token_id, total_collateral, total_debt, total_redeemed, outstanding,
            collateral_ratio_bps, interest_accrued, report_timestamp, reporter_pub, proof, fee,
        })
    }
}

/// Accrue interest parameters (cold/precise - uses BaseDiv)
/// For precise interest accrual calculation
#[derive(Debug, Clone)]
pub struct AccrueInterestParams {
    /// Total debt before interest accrual
    pub old_total_debt: u64,

    /// Total debt after interest accrual
    pub new_total_debt: u64,

    /// Calculated interest amount using BaseDiv
    pub interest_amount: u64,

    /// Interest rate per second (in basis points)
    pub rate_per_second: u64,

    /// Time elapsed since last accrual (seconds)
    pub time_elapsed: u64,

    /// Accumulator's public key
    pub accumulator_pub: PublicKey,

    /// ZK proof: accrue_interest_v1.zk
    pub proof: Vec<u8>,

    /// Fee paid for this operation
    pub fee: u64,
}

impl AccrueInterestParams {
    pub fn encode(&self) -> Vec<u8> {
        let cap = 81 + self.proof.len();
        let mut buf = Vec::with_capacity(cap);
        buf.extend_from_slice(&self.old_total_debt.to_le_bytes());
        buf.extend_from_slice(&self.new_total_debt.to_le_bytes());
        buf.extend_from_slice(&self.interest_amount.to_le_bytes());
        buf.extend_from_slice(&self.rate_per_second.to_le_bytes());
        buf.extend_from_slice(&self.time_elapsed.to_le_bytes());
        buf.extend_from_slice(&self.accumulator_pub.to_bytes());
        buf.push(self.proof.len() as u8);
        buf.extend_from_slice(&self.proof);
        buf.extend_from_slice(&self.fee.to_le_bytes());
        buf
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 82 {
            return Err(ContractError::IoError(format!(
                "AccrueInterestParams: expected at least 82 bytes, got {}", data.len()
            )));
        }
        let old_total_debt = u64::from_le_bytes(data[0..8].try_into().unwrap());
        let new_total_debt = u64::from_le_bytes(data[8..16].try_into().unwrap());
        let interest_amount = u64::from_le_bytes(data[16..24].try_into().unwrap());
        let rate_per_second = u64::from_le_bytes(data[24..32].try_into().unwrap());
        let time_elapsed = u64::from_le_bytes(data[32..40].try_into().unwrap());
        let accumulator_pub = PublicKey::from_bytes(data[40..72].try_into().unwrap())
            .map_err(|e| ContractError::IoError(format!("AccrueInterestParams: invalid accumulator_pub: {}", e)))?;
        let proof_len = data[72] as usize;
        let expected = 73 + proof_len + 8;
        if data.len() != expected {
            return Err(ContractError::IoError(format!(
                "AccrueInterestParams: expected {} bytes, got {}", expected, data.len()
            )));
        }
        let proof = data[73..73 + proof_len].to_vec();
        let fee = u64::from_le_bytes(data[73 + proof_len..73 + proof_len + 8].try_into().unwrap());
        Ok(AccrueInterestParams { old_total_debt, new_total_debt, interest_amount, rate_per_second, time_elapsed, accumulator_pub, proof, fee })
    }
}

/// Global liquidation record
#[derive(Debug, Clone)]
pub struct LiquidationRecord {
    /// Total debt covered
    pub debt_covered: u64,

    /// Total collateral seized
    pub collateral_seized: u64,

    /// Liquidation penalty applied
    pub penalty: u64,

    /// Liquidator public key
    pub liquidator_pub: PublicKey,

    /// Timestamp
    pub liquidated_at: u64,
}

/// Update data for open position
#[derive(Debug, Clone)]
pub struct OpenPositionUpdateV1 {
    pub deposit_commitment: IntentCommitment,
    pub collateral_type: CollateralType,
    pub collateral_amount: u64,
}

// ============================================================================
// SPEND HOOK CALLBACK (received from PN BurnV1)
// ============================================================================

/// State update from a spend_hook callback received from Promissory Note BurnV1.
/// Records the burn so the stablecoin contract can track redemptions and adjust supply.
#[derive(Debug, Clone)]
pub struct SpendHookCallbackUpdateV1 {
    /// Nullifiers of burned stablecoins (replay protection)
    pub nullifiers: Vec<Nullifier>,
    /// Value commitments of burned coins
    pub value_commits: Vec<[u8; 64]>,
    /// Pre-computed new total_redeemed value (read in exec, written in apply)
    pub new_total_redeemed: u64,
}

// ============================================================================
// ENCODE / DECODE IMPL BLOCKS (ρ-calculus: eval(quote(x)) ~ x)
// ============================================================================

/// Helper: decode a pallas::Base from a 32-byte slice with validation.
macro_rules! decode_pallas_base {
    ($data:expr, $offset:expr, $name:literal) => {{
        let bytes: [u8; 32] = $data[$offset..$offset + 32].try_into().unwrap();
        Option::<pallas::Base>::from(pallas::Base::from_repr(bytes))
            .ok_or_else(|| ContractError::IoError(
                format!("{}: invalid pallas::Base", $name)
            ))?
    }};
}

// ---- CollateralType ----

impl CollateralType {
    pub fn encode(&self) -> Vec<u8> { vec![self.clone() as u8] }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.is_empty() { return Err(ContractError::IoError("CollateralType: empty data".into())); }
        CollateralType::try_from(data[0])
    }
}

// ---- StablecoinModel ----

impl StablecoinModel {
    pub fn encode(&self) -> Vec<u8> { vec![self.clone() as u8] }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.is_empty() { return Err(ContractError::IoError("StablecoinModel: empty data".into())); }
        StablecoinModel::try_from(data[0])
    }
}

// ---- DeadManAction ----

impl DeadManAction {
    pub fn encode(&self) -> Vec<u8> { vec![self.clone() as u8] }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.is_empty() { return Err(ContractError::IoError("DeadManAction: empty data".into())); }
        DeadManAction::try_from(data[0])
    }
}

// ---- CollateralParams (25 bytes) ----
// Layout: collateral_type(1) + haircut(8) + liquidation_threshold(8) + max_debt_share(8)

impl CollateralParams {
    pub const ENCODED_SIZE: usize = 25;
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(Self::ENCODED_SIZE);
        b.push(self.collateral_type.clone() as u8);
        b.extend_from_slice(&self.haircut.to_le_bytes());
        b.extend_from_slice(&self.liquidation_threshold.to_le_bytes());
        b.extend_from_slice(&self.max_debt_share.to_le_bytes());
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE { return Err(ContractError::IoError(format!("CollateralParams: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len()))); }
        Ok(CollateralParams {
            collateral_type: CollateralType::try_from(data[0])?,
            haircut: u64::from_le_bytes(data[1..9].try_into().unwrap()),
            liquidation_threshold: u64::from_le_bytes(data[9..17].try_into().unwrap()),
            max_debt_share: u64::from_le_bytes(data[17..25].try_into().unwrap()),
        })
    }
}

// ---- DeadManSwitchConfig (18 bytes) ----
// Layout: enabled(1) + timeout_blocks(8) + action(1) + last_action_block(8)

impl DeadManSwitchConfig {
    pub const ENCODED_SIZE: usize = 18;
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(Self::ENCODED_SIZE);
        b.push(self.enabled as u8);
        b.extend_from_slice(&self.timeout_blocks.to_le_bytes());
        b.push(self.action.clone() as u8);
        b.extend_from_slice(&self.last_action_block.to_le_bytes());
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE { return Err(ContractError::IoError(format!("DeadManSwitchConfig: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len()))); }
        Ok(DeadManSwitchConfig {
            enabled: data[0] != 0,
            timeout_blocks: u64::from_le_bytes(data[1..9].try_into().unwrap()),
            action: DeadManAction::try_from(data[9])?,
            last_action_block: u64::from_le_bytes(data[10..18].try_into().unwrap()),
        })
    }
}

// ---- DebtPool (32 bytes) ----
// Layout: total_debt(8) + total_collateral(8) + accumulated_fees(8) + last_update(8)

impl DebtPool {
    pub const ENCODED_SIZE: usize = 32;
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(Self::ENCODED_SIZE);
        b.extend_from_slice(&self.total_debt.to_le_bytes());
        b.extend_from_slice(&self.total_collateral.to_le_bytes());
        b.extend_from_slice(&self.accumulated_fees.to_le_bytes());
        b.extend_from_slice(&self.last_update.to_le_bytes());
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE { return Err(ContractError::IoError(format!("DebtPool: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len()))); }
        Ok(DebtPool {
            total_debt: u64::from_le_bytes(data[0..8].try_into().unwrap()),
            total_collateral: u64::from_le_bytes(data[8..16].try_into().unwrap()),
            accumulated_fees: u64::from_le_bytes(data[16..24].try_into().unwrap()),
            last_update: u64::from_le_bytes(data[24..32].try_into().unwrap()),
        })
    }
}

// ---- CollateralPool (25 bytes) ----
// Layout: collateral_type(1) + total_deposited(8) + value_ratio(8) + last_update(8)

impl CollateralPool {
    pub const ENCODED_SIZE: usize = 25;
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(Self::ENCODED_SIZE);
        b.push(self.collateral_type.clone() as u8);
        b.extend_from_slice(&self.total_deposited.to_le_bytes());
        b.extend_from_slice(&self.value_ratio.to_le_bytes());
        b.extend_from_slice(&self.last_update.to_le_bytes());
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE { return Err(ContractError::IoError(format!("CollateralPool: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len()))); }
        Ok(CollateralPool {
            collateral_type: CollateralType::try_from(data[0])?,
            total_deposited: u64::from_le_bytes(data[1..9].try_into().unwrap()),
            value_ratio: u64::from_le_bytes(data[9..17].try_into().unwrap()),
            last_update: u64::from_le_bytes(data[17..25].try_into().unwrap()),
        })
    }
}

// ---- DebtShare (88 bytes) ----
// Layout: owner_pub(32) + debt_amount(8) + commitment(32) + created_at(8) + updated_at(8)

impl DebtShare {
    pub const ENCODED_SIZE: usize = 88;
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(Self::ENCODED_SIZE);
        b.extend_from_slice(&self.owner_pub.to_bytes());
        b.extend_from_slice(&self.debt_amount.to_le_bytes());
        b.extend_from_slice(&self.commitment.to_bytes());
        b.extend_from_slice(&self.created_at.to_le_bytes());
        b.extend_from_slice(&self.updated_at.to_le_bytes());
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE { return Err(ContractError::IoError(format!("DebtShare: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len()))); }
        Ok(DebtShare {
            owner_pub: PublicKey::from_bytes(data[0..32].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("DebtShare: invalid owner_pub: {}", e)))?,
            debt_amount: u64::from_le_bytes(data[32..40].try_into().unwrap()),
            commitment: IntentCommitment::from_bytes(data[40..72].try_into().unwrap()).map_err(|_| ContractError::IoError("DebtShare: invalid commitment".into()))?,
            created_at: u64::from_le_bytes(data[72..80].try_into().unwrap()),
            updated_at: u64::from_le_bytes(data[80..88].try_into().unwrap()),
        })
    }
}

// ---- PiControllerState (32 bytes) ----
// Layout: integral(8) + last_update(8) + current_rate(8) + last_twap(8)

impl PiControllerState {
    pub const ENCODED_SIZE: usize = 32;
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(Self::ENCODED_SIZE);
        b.extend_from_slice(&self.integral.to_le_bytes());
        b.extend_from_slice(&self.last_update.to_le_bytes());
        b.extend_from_slice(&self.current_rate.to_le_bytes());
        b.extend_from_slice(&self.last_twap.to_le_bytes());
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE { return Err(ContractError::IoError(format!("PiControllerState: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len()))); }
        Ok(PiControllerState {
            integral: i64::from_le_bytes(data[0..8].try_into().unwrap()),
            last_update: u64::from_le_bytes(data[8..16].try_into().unwrap()),
            current_rate: u64::from_le_bytes(data[16..24].try_into().unwrap()),
            last_twap: u64::from_le_bytes(data[24..32].try_into().unwrap()),
        })
    }
}

// ---- PriceFeed (48 bytes) ----
// Layout: twap(8) + window_start(8) + window_end(8) + reserve0(8) + reserve1(8) + timestamp(8)

impl PriceFeed {
    pub const ENCODED_SIZE: usize = 48;
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(Self::ENCODED_SIZE);
        b.extend_from_slice(&self.twap.to_le_bytes());
        b.extend_from_slice(&self.window_start.to_le_bytes());
        b.extend_from_slice(&self.window_end.to_le_bytes());
        b.extend_from_slice(&self.reserve0.to_le_bytes());
        b.extend_from_slice(&self.reserve1.to_le_bytes());
        b.extend_from_slice(&self.timestamp.to_le_bytes());
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE { return Err(ContractError::IoError(format!("PriceFeed: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len()))); }
        Ok(PriceFeed {
            twap: u64::from_le_bytes(data[0..8].try_into().unwrap()),
            window_start: u64::from_le_bytes(data[8..16].try_into().unwrap()),
            window_end: u64::from_le_bytes(data[16..24].try_into().unwrap()),
            reserve0: u64::from_le_bytes(data[24..32].try_into().unwrap()),
            reserve1: u64::from_le_bytes(data[32..40].try_into().unwrap()),
            timestamp: u64::from_le_bytes(data[40..48].try_into().unwrap()),
        })
    }
}

// ---- LiquidationRecord (64 bytes) ----
// Layout: debt_covered(8) + collateral_seized(8) + penalty(8) + liquidator_pub(32) + liquidated_at(8)

impl LiquidationRecord {
    pub const ENCODED_SIZE: usize = 64;
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(Self::ENCODED_SIZE);
        b.extend_from_slice(&self.debt_covered.to_le_bytes());
        b.extend_from_slice(&self.collateral_seized.to_le_bytes());
        b.extend_from_slice(&self.penalty.to_le_bytes());
        b.extend_from_slice(&self.liquidator_pub.to_bytes());
        b.extend_from_slice(&self.liquidated_at.to_le_bytes());
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE { return Err(ContractError::IoError(format!("LiquidationRecord: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len()))); }
        Ok(LiquidationRecord {
            debt_covered: u64::from_le_bytes(data[0..8].try_into().unwrap()),
            collateral_seized: u64::from_le_bytes(data[8..16].try_into().unwrap()),
            penalty: u64::from_le_bytes(data[16..24].try_into().unwrap()),
            liquidator_pub: PublicKey::from_bytes(data[24..56].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("LiquidationRecord: invalid liquidator_pub: {}", e)))?,
            liquidated_at: u64::from_le_bytes(data[56..64].try_into().unwrap()),
        })
    }
}

// ---- UpdateConfigUpdateV1 (96 bytes) ----
// Layout: min_collat_ratio(8) + liq_threshold(8) + liq_penalty(8) + base_rate(8)
//         + pi_kp(8) + pi_ki(8) + twap_window(8) + price_dev_threshold(8) + config_nullifier(32)

impl UpdateConfigUpdateV1 {
    pub const ENCODED_SIZE: usize = 96;
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(Self::ENCODED_SIZE);
        b.extend_from_slice(&self.min_collateralization_ratio.to_le_bytes());
        b.extend_from_slice(&self.liquidation_threshold.to_le_bytes());
        b.extend_from_slice(&self.liquidation_penalty.to_le_bytes());
        b.extend_from_slice(&self.base_rate.to_le_bytes());
        b.extend_from_slice(&self.pi_kp.to_le_bytes());
        b.extend_from_slice(&self.pi_ki.to_le_bytes());
        b.extend_from_slice(&self.twap_window.to_le_bytes());
        b.extend_from_slice(&self.price_deviation_threshold.to_le_bytes());
        b.extend_from_slice(&self.config_nullifier.to_repr());
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE { return Err(ContractError::IoError(format!("UpdateConfigUpdateV1: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len()))); }
        Ok(UpdateConfigUpdateV1 {
            min_collateralization_ratio: u64::from_le_bytes(data[0..8].try_into().unwrap()),
            liquidation_threshold: u64::from_le_bytes(data[8..16].try_into().unwrap()),
            liquidation_penalty: u64::from_le_bytes(data[16..24].try_into().unwrap()),
            base_rate: u64::from_le_bytes(data[24..32].try_into().unwrap()),
            pi_kp: i64::from_le_bytes(data[32..40].try_into().unwrap()),
            pi_ki: i64::from_le_bytes(data[40..48].try_into().unwrap()),
            twap_window: u64::from_le_bytes(data[48..56].try_into().unwrap()),
            price_deviation_threshold: u64::from_le_bytes(data[56..64].try_into().unwrap()),
            config_nullifier: decode_pallas_base!(data, 64, "UpdateConfigUpdateV1"),
        })
    }
}

// ---- AddCollateralUpdateV1 (41 bytes) ----
// Layout: position_commitment(32) + added_collateral(8) + collateral_type(1)

impl AddCollateralUpdateV1 {
    pub const ENCODED_SIZE: usize = 41;
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(Self::ENCODED_SIZE);
        b.extend_from_slice(&self.position_commitment.to_bytes());
        b.extend_from_slice(&self.added_collateral.to_le_bytes());
        b.push(self.collateral_type.clone() as u8);
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE { return Err(ContractError::IoError(format!("AddCollateralUpdateV1: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len()))); }
        Ok(AddCollateralUpdateV1 {
            position_commitment: IntentCommitment::from_bytes(data[0..32].try_into().unwrap()).map_err(|_| ContractError::IoError("AddCollateralUpdateV1: invalid position_commitment".into()))?,
            added_collateral: u64::from_le_bytes(data[32..40].try_into().unwrap()),
            collateral_type: CollateralType::try_from(data[40])?,
        })
    }
}

// ---- RemoveCollateralUpdateV1 (73 bytes) ----
// Layout: position_nullifier(32) + new_commitment(32) + collateral_type(1) + removed_collateral(8)

impl RemoveCollateralUpdateV1 {
    pub const ENCODED_SIZE: usize = 73;
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(Self::ENCODED_SIZE);
        b.extend_from_slice(&self.position_nullifier.to_bytes());
        b.extend_from_slice(&self.new_commitment.to_bytes());
        b.push(self.collateral_type.clone() as u8);
        b.extend_from_slice(&self.removed_collateral.to_le_bytes());
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE { return Err(ContractError::IoError(format!("RemoveCollateralUpdateV1: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len()))); }
        Ok(RemoveCollateralUpdateV1 {
            position_nullifier: IntentNullifier::from_bytes(data[0..32].try_into().unwrap()).map_err(|_| ContractError::IoError("RemoveCollateralUpdateV1: invalid position_nullifier".into()))?,
            new_commitment: IntentCommitment::from_bytes(data[32..64].try_into().unwrap()).map_err(|_| ContractError::IoError("RemoveCollateralUpdateV1: invalid new_commitment".into()))?,
            collateral_type: CollateralType::try_from(data[64])?,
            removed_collateral: u64::from_le_bytes(data[65..73].try_into().unwrap()),
        })
    }
}

// ---- MintStableUpdateV1 (48 bytes) ----
// Layout: position_commitment(32) + mint_amount(8) + new_total_debt(8)

impl MintStableUpdateV1 {
    pub const ENCODED_SIZE: usize = 48;
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(Self::ENCODED_SIZE);
        b.extend_from_slice(&self.position_commitment.to_bytes());
        b.extend_from_slice(&self.mint_amount.to_le_bytes());
        b.extend_from_slice(&self.new_total_debt.to_le_bytes());
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE { return Err(ContractError::IoError(format!("MintStableUpdateV1: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len()))); }
        Ok(MintStableUpdateV1 {
            position_commitment: IntentCommitment::from_bytes(data[0..32].try_into().unwrap()).map_err(|_| ContractError::IoError("MintStableUpdateV1: invalid position_commitment".into()))?,
            mint_amount: u64::from_le_bytes(data[32..40].try_into().unwrap()),
            new_total_debt: u64::from_le_bytes(data[40..48].try_into().unwrap()),
        })
    }
}

// ---- RepayStableUpdateV1 (80 bytes) ----
// Layout: position_nullifier(32) + new_commitment(32) + repay_amount(8) + new_total_debt(8)

impl RepayStableUpdateV1 {
    pub const ENCODED_SIZE: usize = 80;
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(Self::ENCODED_SIZE);
        b.extend_from_slice(&self.position_nullifier.to_bytes());
        b.extend_from_slice(&self.new_commitment.to_bytes());
        b.extend_from_slice(&self.repay_amount.to_le_bytes());
        b.extend_from_slice(&self.new_total_debt.to_le_bytes());
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE { return Err(ContractError::IoError(format!("RepayStableUpdateV1: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len()))); }
        Ok(RepayStableUpdateV1 {
            position_nullifier: IntentNullifier::from_bytes(data[0..32].try_into().unwrap()).map_err(|_| ContractError::IoError("RepayStableUpdateV1: invalid position_nullifier".into()))?,
            new_commitment: IntentCommitment::from_bytes(data[32..64].try_into().unwrap()).map_err(|_| ContractError::IoError("RepayStableUpdateV1: invalid new_commitment".into()))?,
            repay_amount: u64::from_le_bytes(data[64..72].try_into().unwrap()),
            new_total_debt: u64::from_le_bytes(data[72..80].try_into().unwrap()),
        })
    }
}

// ---- LiquidateUpdateV1 (40 bytes) ----
// Layout: debt_covered(8) + collateral_seized(8) + penalty(8) + new_total_debt(8) + new_total_collateral(8)

impl LiquidateUpdateV1 {
    pub const ENCODED_SIZE: usize = 40;
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(Self::ENCODED_SIZE);
        b.extend_from_slice(&self.debt_covered.to_le_bytes());
        b.extend_from_slice(&self.collateral_seized.to_le_bytes());
        b.extend_from_slice(&self.penalty.to_le_bytes());
        b.extend_from_slice(&self.new_total_debt.to_le_bytes());
        b.extend_from_slice(&self.new_total_collateral.to_le_bytes());
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE { return Err(ContractError::IoError(format!("LiquidateUpdateV1: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len()))); }
        Ok(LiquidateUpdateV1 {
            debt_covered: u64::from_le_bytes(data[0..8].try_into().unwrap()),
            collateral_seized: u64::from_le_bytes(data[8..16].try_into().unwrap()),
            penalty: u64::from_le_bytes(data[16..24].try_into().unwrap()),
            new_total_debt: u64::from_le_bytes(data[24..32].try_into().unwrap()),
            new_total_collateral: u64::from_le_bytes(data[32..40].try_into().unwrap()),
        })
    }
}

// ---- GovernanceReportUpdateV1 (120 bytes) ----
// Layout: token_id(32) + total_collateral(8) + total_debt(8) + total_redeemed(8) + outstanding(8)
//         + collateral_ratio_bps(8) + interest_accrued(8) + report_block(8) + reporter_pub(32)

impl GovernanceReportUpdateV1 {
    pub const ENCODED_SIZE: usize = 120;
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(Self::ENCODED_SIZE);
        b.extend_from_slice(&self.token_id.to_repr());
        b.extend_from_slice(&self.total_collateral.to_le_bytes());
        b.extend_from_slice(&self.total_debt.to_le_bytes());
        b.extend_from_slice(&self.total_redeemed.to_le_bytes());
        b.extend_from_slice(&self.outstanding.to_le_bytes());
        b.extend_from_slice(&self.collateral_ratio_bps.to_le_bytes());
        b.extend_from_slice(&self.interest_accrued.to_le_bytes());
        b.extend_from_slice(&self.report_block.to_le_bytes());
        b.extend_from_slice(&self.reporter_pub.to_bytes());
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE { return Err(ContractError::IoError(format!("GovernanceReportUpdateV1: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len()))); }
        Ok(GovernanceReportUpdateV1 {
            token_id: decode_pallas_base!(data, 0, "GovernanceReportUpdateV1:token_id"),
            total_collateral: u64::from_le_bytes(data[32..40].try_into().unwrap()),
            total_debt: u64::from_le_bytes(data[40..48].try_into().unwrap()),
            total_redeemed: u64::from_le_bytes(data[48..56].try_into().unwrap()),
            outstanding: u64::from_le_bytes(data[56..64].try_into().unwrap()),
            collateral_ratio_bps: u64::from_le_bytes(data[64..72].try_into().unwrap()),
            interest_accrued: u64::from_le_bytes(data[72..80].try_into().unwrap()),
            report_block: u64::from_le_bytes(data[80..88].try_into().unwrap()),
            reporter_pub: PublicKey::from_bytes(data[88..120].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("GovernanceReportUpdateV1: invalid reporter_pub: {}", e)))?,
        })
    }
}

// ---- AccrueInterestUpdateV1 (56 bytes) ----
// Layout: old_total_debt(8) + new_total_debt(8) + interest_amount(8) + accumulator_pub(32)

impl AccrueInterestUpdateV1 {
    pub const ENCODED_SIZE: usize = 56;
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(Self::ENCODED_SIZE);
        b.extend_from_slice(&self.old_total_debt.to_le_bytes());
        b.extend_from_slice(&self.new_total_debt.to_le_bytes());
        b.extend_from_slice(&self.interest_amount.to_le_bytes());
        b.extend_from_slice(&self.accumulator_pub.to_bytes());
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE { return Err(ContractError::IoError(format!("AccrueInterestUpdateV1: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len()))); }
        Ok(AccrueInterestUpdateV1 {
            old_total_debt: u64::from_le_bytes(data[0..8].try_into().unwrap()),
            new_total_debt: u64::from_le_bytes(data[8..16].try_into().unwrap()),
            interest_amount: u64::from_le_bytes(data[16..24].try_into().unwrap()),
            accumulator_pub: PublicKey::from_bytes(data[24..56].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("AccrueInterestUpdateV1: invalid accumulator_pub: {}", e)))?,
        })
    }
}

// ---- RedeemStableUpdateV1 (96 bytes) ----
// Layout: redeem_nullifier(32) + receipt_coin(32) + redeem_amount(8) + new_total_debt(8)
//         + new_total_collateral(8) + new_total_redeemed(8)

impl RedeemStableUpdateV1 {
    pub const ENCODED_SIZE: usize = 96;
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(Self::ENCODED_SIZE);
        b.extend_from_slice(&self.redeem_nullifier.to_repr());
        b.extend_from_slice(&self.receipt_coin);
        b.extend_from_slice(&self.redeem_amount.to_le_bytes());
        b.extend_from_slice(&self.new_total_debt.to_le_bytes());
        b.extend_from_slice(&self.new_total_collateral.to_le_bytes());
        b.extend_from_slice(&self.new_total_redeemed.to_le_bytes());
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE { return Err(ContractError::IoError(format!("RedeemStableUpdateV1: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len()))); }
        let mut receipt_coin = [0u8; 32];
        receipt_coin.copy_from_slice(&data[32..64]);
        Ok(RedeemStableUpdateV1 {
            redeem_nullifier: decode_pallas_base!(data, 0, "RedeemStableUpdateV1:redeem_nullifier"),
            receipt_coin,
            redeem_amount: u64::from_le_bytes(data[64..72].try_into().unwrap()),
            new_total_debt: u64::from_le_bytes(data[72..80].try_into().unwrap()),
            new_total_collateral: u64::from_le_bytes(data[80..88].try_into().unwrap()),
            new_total_redeemed: u64::from_le_bytes(data[88..96].try_into().unwrap()),
        })
    }
}

// ---- SpendHookCallbackUpdateV1 (variable: Vec<Nullifier> + Vec<[u8;64]> + u64) ----
// Layout: nullifier_count(u8) + nullifiers(count*32) + value_commit_count(u8) + value_commits(count*64) + new_total_redeemed(8)

impl SpendHookCallbackUpdateV1 {
    pub fn encode(&self) -> Vec<u8> {
        let cap = 2 + 8 + self.nullifiers.len() * 32 + self.value_commits.len() * 64;
        let mut b = Vec::with_capacity(cap);
        b.push(self.nullifiers.len() as u8);
        for n in &self.nullifiers { b.extend_from_slice(&n.to_bytes()); }
        b.push(self.value_commits.len() as u8);
        for vc in &self.value_commits { b.extend_from_slice(vc); }
        b.extend_from_slice(&self.new_total_redeemed.to_le_bytes());
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 2 { return Err(ContractError::IoError("SpendHookCallbackUpdateV1: data too short".into())); }
        let nf_count = data[0] as usize;
        let nf_end = 1 + nf_count * 32;
        if data.len() < nf_end + 1 { return Err(ContractError::IoError(format!("SpendHookCallbackUpdateV1: expected {} bytes for nullifiers, got {}", nf_end + 1, data.len()))); }
        let mut nullifiers = Vec::with_capacity(nf_count);
        for i in 0..nf_count {
            nullifiers.push(Nullifier::from_bytes(data[1 + i*32..1 + (i+1)*32].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("SpendHookCallbackUpdateV1: invalid nullifier[{}]: {}", i, e)))?);
        }
        let vc_count = data[nf_end] as usize;
        let vc_end = nf_end + 1 + vc_count * 64;
        if data.len() != vc_end + 8 { return Err(ContractError::IoError(format!("SpendHookCallbackUpdateV1: expected {} bytes total, got {}", vc_end + 8, data.len()))); }
        let mut value_commits = Vec::with_capacity(vc_count);
        for i in 0..vc_count {
            let start = nf_end + 1 + i * 64;
            let mut vc = [0u8; 64];
            vc.copy_from_slice(&data[start..start + 64]);
            value_commits.push(vc);
        }
        Ok(SpendHookCallbackUpdateV1 {
            nullifiers,
            value_commits,
            new_total_redeemed: u64::from_le_bytes(data[vc_end..vc_end + 8].try_into().unwrap()),
        })
    }
}

// ---- OpenPositionUpdateV1 (41 bytes) ----
// Layout: deposit_commitment(32) + collateral_type(1) + collateral_amount(8)

impl OpenPositionUpdateV1 {
    pub const ENCODED_SIZE: usize = 41;
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(Self::ENCODED_SIZE);
        b.extend_from_slice(&self.deposit_commitment.to_bytes());
        b.push(self.collateral_type.clone() as u8);
        b.extend_from_slice(&self.collateral_amount.to_le_bytes());
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE { return Err(ContractError::IoError(format!("OpenPositionUpdateV1: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len()))); }
        Ok(OpenPositionUpdateV1 {
            deposit_commitment: IntentCommitment::from_bytes(data[0..32].try_into().unwrap()).map_err(|_| ContractError::IoError("OpenPositionUpdateV1: invalid deposit_commitment".into()))?,
            collateral_type: CollateralType::try_from(data[32])?,
            collateral_amount: u64::from_le_bytes(data[33..41].try_into().unwrap()),
        })
    }
}

// ============================================================================
// DESIGN NOTES: Why Pooled Debt vs Individual CDP
// ============================================================================
//
// INDIVIDUAL CDP MODEL (original DarkWow approach, possible but complex):
//
// Pros:
// - Users have individual positions with specific collateral/debt
// - Can implement partial liquidations
// - More granular control over positions
//
// Cons:
// - Must prove individual position validity (complex ZK circuits)
// - Position IDs leak information: "position 123 was liquidated"
// - Liquidators can see specific positions being liquidated
// - Individual nullifiers/commitments for each position
// - ZK circuits need to verify per-position collateralization
//
// POOLED DEBT MODEL (this implementation, simpler for privacy):
//
// Pros:
// - No individual positions to track
// - Liquidation is "pool had shortfall" - no position IDs leaked
// - Simpler ZK circuits
// - All collateral backs all debt - more capital efficient
//
// Cons:
// - Cannot liquidate individual positions
// - Entire pool must be healthy or entire pool can be liquidated
// - User's collateral is always at risk from others' behavior
//
// The pooled model was chosen for the MVP because:
// 1. Simpler ZK circuits (no per-position proofs)
// 2. Better privacy (no position IDs that could be tracked)
// 3. Faster to implement
// 4. Individual CDP can be added later as a layer on top
//
// TO ADD INDIVIDUAL CDP LATER:
// - Layer individual position tracking on top of pooled debt
// - Use attestation contract to verify individual positions
// - Each user can choose pooled (simpler) or individual (more control)
// ============================================================================
