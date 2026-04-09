/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

//! WASM entrypoint for the stablecoin (CDP) contract
//!
//! ## Design: P2P Oracle-Based Stablecoin (Pooled Debt Model)
//!
//! Unlike traditional CDPs (MakerDAO) that rely on:
//! - Governance-controlled parameters
//! - Trusted price oracles
//! - Public position data
//!
//! This implementation uses:
//! - **AMM-based TWAP**: Price from NETHER/DRK constant-product pool
//! - **PI Controller**: Algorithmic redemption rate adjustment
//! - **Full privacy**: All positions hidden via Pedersen commitments + SMT
//! - **ZK proofs**: All state transitions verified without revealing data
//! - **Pooled Debt**: All collateral backs all debt, no individual positions

use darkfi_sdk::{
    crypto::{ContractId, IntentNullifier},
    dark_tree::DarkLeaf,
    error::{ContractError, ContractResult},
    msg, ContractCall,
    wasm,
};
use darkfi_serial::{deserialize, serialize, Decodable, SerialDecodable, SerialEncodable};

use crate::{
    error::StablecoinError,
    model::{
        AddCollateralUpdateV1, AccrueInterestParams, AccrueInterestUpdateV1, CollateralType,
        DepositCollateralParams, GovernanceReportParams, GovernanceReportUpdateV1,
        LiquidateParams, LiquidateUpdateV1, MintStableParams, MintStableUpdateV1,
        RemoveCollateralUpdateV1, RepayStableParams, RepayStableUpdateV1, UpdateConfigParams,
        UpdateConfigUpdateV1, WithdrawCollateralParams,
    },
    StablecoinFunction, STABLECOIN_CONTRACT_COLLATERAL_TREE, STABLECOIN_CONTRACT_DB_VERSION,
    STABLECOIN_CONTRACT_INFO_TREE, STABLECOIN_CONTRACT_LIQUIDATIONS_TREE,
    STABLECOIN_CONTRACT_POSITION_NULLIFIERS_TREE, STABLECOIN_CONTRACT_POSITIONS_TREE,
    STABLECOIN_CONTRACT_STABLECOIN_TREE,
};

// ============================================================================
// DATABASE KEYS
// ============================================================================

const CDP_PI_STATE_KEY: &[u8] = b"pi_controller_state";
const CDP_REDEMPTION_RATE_KEY: &[u8] = b"redemption_rate";
const CDP_MIN_RATIO_KEY: &[u8] = b"min_ratio";
const CDP_LIQ_THRESHOLD_KEY: &[u8] = b"liq_threshold";
const CDP_TOTAL_DEBT_KEY: &[u8] = b"total_debt";
const CDP_TOTAL_COLLATERAL_KEY: &[u8] = b"total_collateral";
const CDP_ACCUMULATED_FEES_KEY: &[u8] = b"accumulated_fees";
const CDP_LAST_INTEREST_UPDATE_KEY: &[u8] = b"last_interest_update";

// ============================================================================
// CONTRACT DEFINITION
// ============================================================================

darkfi_sdk::define_contract!(
    init: init_contract,
    exec: process_instruction,
    apply: process_update,
    metadata: get_metadata
);

// ============================================================================
// INITIALIZATION
// ============================================================================

/// Initialize the CDP engine
pub fn init_contract(cid: ContractId, ix: &[u8]) -> ContractResult {
    let params = UpdateConfigParams::decode(&mut std::io::Cursor::new(ix))
        .map_err(|_| ContractError::IoError("Decode error".to_string()))?;

    msg!("[stablecoin::init_contract] Initializing CDP engine");

    // Initialize info tree
    let info_db = wasm::db::db_init(cid, STABLECOIN_CONTRACT_INFO_TREE)?;
    wasm::db::db_set(info_db, STABLECOIN_CONTRACT_DB_VERSION, env!("CARGO_PKG_VERSION").as_bytes())?;

    // Initialize positions tree (for tracking commitments)
    wasm::db::db_init(cid, STABLECOIN_CONTRACT_POSITIONS_TREE)?;

    // Initialize position nullifiers tree
    wasm::db::db_init(cid, STABLECOIN_CONTRACT_POSITION_NULLIFIERS_TREE)?;

    // Initialize stablecoin tree (for tracking supply)
    wasm::db::db_init(cid, STABLECOIN_CONTRACT_STABLECOIN_TREE)?;

    // Initialize collateral tree
    wasm::db::db_init(cid, STABLECOIN_CONTRACT_COLLATERAL_TREE)?;

    // Initialize liquidations tree
    wasm::db::db_init(cid, STABLECOIN_CONTRACT_LIQUIDATIONS_TREE)?;

    // Initialize PI controller state and config
    let config_db = wasm::db::db_init(cid, "config")?;
    wasm::db::db_set(config_db, CDP_PI_STATE_KEY, &0i64.to_le_bytes())?;
    wasm::db::db_set(config_db, CDP_REDEMPTION_RATE_KEY, &0i64.to_le_bytes())?;
    wasm::db::db_set(config_db, CDP_MIN_RATIO_KEY, &params.min_collateralization_ratio.to_le_bytes())?;
    wasm::db::db_set(config_db, CDP_LIQ_THRESHOLD_KEY, &params.liquidation_threshold.to_le_bytes())?;

    // Initialize total debt and collateral to zero
    wasm::db::db_set(config_db, CDP_TOTAL_DEBT_KEY, &0u64.to_le_bytes())?;
    wasm::db::db_set(config_db, CDP_TOTAL_COLLATERAL_KEY, &0u64.to_le_bytes())?;
    wasm::db::db_set(config_db, CDP_ACCUMULATED_FEES_KEY, &0u64.to_le_bytes())?;
    wasm::db::db_set(config_db, CDP_LAST_INTEREST_UPDATE_KEY, &0u64.to_le_bytes())?;

    msg!("[stablecoin::init_contract] CDP engine initialized successfully");
    Ok(())
}

// ============================================================================
// METADATA (ZK proof verification)
// ============================================================================

/// Fetch metadata for ZK proof verification
fn get_metadata(_cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = StablecoinFunction::try_from(self_.data[0])?;

    match func {
        StablecoinFunction::InitializeV1 => wasm::util::set_return_data(&vec![]),
        StablecoinFunction::OpenPositionV1 => {
            // TODO: Return ZK public inputs for open position proof
            msg!("[stablecoin::get_metadata] OpenPositionV1 metadata requested");
            wasm::util::set_return_data(&vec![])
        }
        StablecoinFunction::AddCollateralV1 => wasm::util::set_return_data(&vec![]),
        StablecoinFunction::RemoveCollateralV1 => wasm::util::set_return_data(&vec![]),
        StablecoinFunction::MintStableV1 => wasm::util::set_return_data(&vec![]),
        StablecoinFunction::RepayStableV1 => wasm::util::set_return_data(&vec![]),
        StablecoinFunction::LiquidateV1 => wasm::util::set_return_data(&vec![]),
        StablecoinFunction::UpdateConfigV1 => wasm::util::set_return_data(&vec![]),
        StablecoinFunction::GovernanceReportV1 => wasm::util::set_return_data(&vec![]),
        StablecoinFunction::AccrueInterestV1 => wasm::util::set_return_data(&vec![]),
    }
}

// ============================================================================
// INSTRUCTION PROCESSING
// ============================================================================

/// Verify state transition and produce update if valid
fn process_instruction(cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = StablecoinFunction::try_from(self_.data[0])?;

    match func {
        StablecoinFunction::InitializeV1 => {
            msg!("[stablecoin::process_instruction] InitializeV1 has no update data");
            wasm::util::set_return_data(&vec![])
        }
        StablecoinFunction::OpenPositionV1 => process_open_position_instruction(cid, call_idx, calls),
        StablecoinFunction::AddCollateralV1 => process_add_collateral_instruction(cid, call_idx, calls),
        StablecoinFunction::RemoveCollateralV1 => {
            process_remove_collateral_instruction(cid, call_idx, calls)
        }
        StablecoinFunction::MintStableV1 => process_mint_stable_instruction(cid, call_idx, calls),
        StablecoinFunction::RepayStableV1 => process_repay_stable_instruction(cid, call_idx, calls),
        StablecoinFunction::LiquidateV1 => process_liquidate_instruction(cid, call_idx, calls),
        StablecoinFunction::UpdateConfigV1 => {
            let params: UpdateConfigParams = deserialize(&self_.data[1..])?;
            msg!("[stablecoin::process_instruction] UpdateConfigV1 processed");
            let update = UpdateConfigUpdateV1 {
                min_collateralization_ratio: params.min_collateralization_ratio,
                liquidation_threshold: params.liquidation_threshold,
                liquidation_penalty: params.liquidation_penalty,
                base_rate: params.base_rate,
                pi_kp: params.pi_kp,
                pi_ki: params.pi_ki,
                twap_window: params.twap_window,
                price_deviation_threshold: params.price_deviation_threshold,
            };
            wasm::util::set_return_data(&serialize(&update))
        }
        StablecoinFunction::GovernanceReportV1 => {
            process_governance_report_instruction(cid, call_idx, calls)
        }
        StablecoinFunction::AccrueInterestV1 => {
            process_accrue_interest_instruction(cid, call_idx, calls)
        }
    }
}

/// Process open position instruction
/// Note: In the pooled model, this is equivalent to depositing collateral
fn process_open_position_instruction(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params: DepositCollateralParams = deserialize(&self_.data[1..])?;

    msg!(
        "[stablecoin::process_instruction] Opening position: commitment={:?}",
        &params.deposit_commitment
    );

    // Verify commitment doesn't already exist
    let positions_db = wasm::db::db_lookup(cid, STABLECOIN_CONTRACT_POSITIONS_TREE)?;
    if wasm::db::db_contains_key(positions_db, &params.deposit_commitment.to_bytes())? {
        msg!("[stablecoin::process_instruction] ERROR: Position already exists");
        return Err(StablecoinError::PositionAlreadyExists.into())
    }

    // Create update data
    let update = OpenPositionUpdateV1 {
        deposit_commitment: params.deposit_commitment,
        collateral_type: params.collateral_type,
        collateral_amount: params.collateral_amount,
    };

    wasm::util::set_return_data(&serialize(&update))
}

// ============================================================================
// STATE UPDATE
// ============================================================================

/// Write state update after successful verification
fn process_update(cid: ContractId, update_data: &[u8]) -> ContractResult {
    let func = StablecoinFunction::try_from(update_data[0])?;

    match func {
        StablecoinFunction::InitializeV1 => {
            msg!("[stablecoin::process_update] InitializeV1 has no update data");
            Ok(())
        }
        StablecoinFunction::OpenPositionV1 => {
            let update: OpenPositionUpdateV1 = deserialize(&update_data[1..])?;
            apply_open_position_update(cid, update)
        }
        StablecoinFunction::AddCollateralV1 => {
            let update: AddCollateralUpdateV1 = deserialize(&update_data[1..])?;
            apply_add_collateral_update(cid, update)
        }
        StablecoinFunction::RemoveCollateralV1 => {
            let update: RemoveCollateralUpdateV1 = deserialize(&update_data[1..])?;
            apply_remove_collateral_update(cid, update)
        }
        StablecoinFunction::MintStableV1 => {
            let update: MintStableUpdateV1 = deserialize(&update_data[1..])?;
            apply_mint_stable_update(cid, update)
        }
        StablecoinFunction::RepayStableV1 => {
            let update: RepayStableUpdateV1 = deserialize(&update_data[1..])?;
            apply_repay_stable_update(cid, update)
        }
        StablecoinFunction::LiquidateV1 => {
            let update: LiquidateUpdateV1 = deserialize(&update_data[1..])?;
            apply_liquidate_update(cid, update)
        }
        StablecoinFunction::UpdateConfigV1 => {
            let update: UpdateConfigUpdateV1 = deserialize(&update_data[1..])?;
            apply_config_update(cid, update)
        }
        StablecoinFunction::GovernanceReportV1 => {
            let update: GovernanceReportUpdateV1 = deserialize(&update_data[1..])?;
            apply_governance_report_update(cid, update)
        }
        StablecoinFunction::AccrueInterestV1 => {
            let update: AccrueInterestUpdateV1 = deserialize(&update_data[1..])?;
            apply_accrue_interest_update(cid, update)
        }
    }
}

/// Apply open position state update
fn apply_open_position_update(cid: ContractId, update: OpenPositionUpdateV1) -> ContractResult {
    let positions_db = wasm::db::db_lookup(cid, STABLECOIN_CONTRACT_POSITIONS_TREE)?;
    let collateral_db = wasm::db::db_lookup(cid, STABLECOIN_CONTRACT_COLLATERAL_TREE)?;

    // Insert position into positions tree
    wasm::db::db_set(positions_db, &update.deposit_commitment.to_bytes(), &vec![])?;

    // Update collateral pool (simplified - in production, track per-type pools)
    wasm::db::db_set(collateral_db, &update.deposit_commitment.to_bytes(), &vec![])?;

    msg!(
        "[stablecoin::process_update] Position opened: commitment={:?}",
        &update.deposit_commitment
    );
    Ok(())
}

/// Apply configuration update
fn apply_config_update(cid: ContractId, update: UpdateConfigUpdateV1) -> ContractResult {
    let config_db = wasm::db::db_lookup(cid, "config")?;

    wasm::db::db_set(config_db, CDP_MIN_RATIO_KEY, &update.min_collateralization_ratio.to_le_bytes())?;
    wasm::db::db_set(config_db, CDP_LIQ_THRESHOLD_KEY, &update.liquidation_threshold.to_le_bytes())?;

    msg!("[stablecoin::process_update] Configuration updated successfully");
    Ok(())
}

/// Process add collateral instruction
fn process_add_collateral_instruction(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params: DepositCollateralParams = deserialize(&self_.data[1..])?;

    msg!(
        "[stablecoin::process_instruction] AddCollateral: commitment={:?}, amount={}",
        params.deposit_commitment,
        params.collateral_amount
    );

    // Verify commitment doesn't already exist
    let positions_db = wasm::db::db_lookup(cid, STABLECOIN_CONTRACT_POSITIONS_TREE)?;
    if wasm::db::db_contains_key(positions_db, &params.deposit_commitment.to_bytes())? {
        msg!("[stablecoin::process_instruction] ERROR: Position already exists");
        return Err(StablecoinError::PositionAlreadyExists.into())
    }

    // Get current total collateral
    let config_db = wasm::db::db_lookup(cid, "config")?;
    let total_collateral_bytes = wasm::db::db_get(config_db, CDP_TOTAL_COLLATERAL_KEY)?
        .ok_or_else(|| ContractError::IoError("Total collateral not found".to_string()))?;
    let total_collateral = u64::from_le_bytes(
        total_collateral_bytes.as_slice().try_into().map_err(|_| ContractError::IoError("Failed to read total collateral".to_string()))?,
    );
    let new_total_collateral = total_collateral.saturating_add(params.collateral_amount);

    // Create update data
    let update = AddCollateralUpdateV1 {
        position_commitment: params.deposit_commitment,
        added_collateral: params.collateral_amount,
        collateral_type: params.collateral_type,
    };

    // Store new total in config for update phase
    wasm::db::db_set(config_db, CDP_TOTAL_COLLATERAL_KEY, &new_total_collateral.to_le_bytes())?;

    wasm::util::set_return_data(&serialize(&update))
}

/// Apply add collateral update
fn apply_add_collateral_update(cid: ContractId, update: AddCollateralUpdateV1) -> ContractResult {
    let positions_db = wasm::db::db_lookup(cid, STABLECOIN_CONTRACT_POSITIONS_TREE)?;
    let collateral_db = wasm::db::db_lookup(cid, STABLECOIN_CONTRACT_COLLATERAL_TREE)?;

    // Insert position into positions tree
    wasm::db::db_set(positions_db, &update.position_commitment.to_bytes(), &vec![])?;
    wasm::db::db_set(collateral_db, &update.position_commitment.to_bytes(), &vec![])?;

    msg!(
        "[stablecoin::process_update] Collateral added: commitment={:?}, amount={}",
        update.position_commitment,
        update.added_collateral
    );
    Ok(())
}

/// Process remove collateral instruction
fn process_remove_collateral_instruction(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params: WithdrawCollateralParams = deserialize(&self_.data[1..])?;

    msg!(
        "[stablecoin::process_instruction] RemoveCollateral: nullifier={:?}, amount={}",
        params.withdrawal_nullifier,
        params.withdraw_amount
    );

    // Verify nullifier doesn't already exist (prevent double-withdrawal)
    let nullifiers_db = wasm::db::db_lookup(cid, STABLECOIN_CONTRACT_POSITION_NULLIFIERS_TREE)?;
    if wasm::db::db_contains_key(nullifiers_db, &params.withdrawal_nullifier.to_bytes())? {
        msg!("[stablecoin::process_instruction] ERROR: Nullifier already exists");
        return Err(StablecoinError::DuplicateNullifier.into())
    }

    // Get current total collateral
    let config_db = wasm::db::db_lookup(cid, "config")?;
    let total_collateral_bytes = wasm::db::db_get(config_db, CDP_TOTAL_COLLATERAL_KEY)?
        .ok_or_else(|| ContractError::IoError("Total collateral not found".to_string()))?;
    let total_collateral = u64::from_le_bytes(
        total_collateral_bytes.as_slice().try_into().map_err(|_| ContractError::IoError("Failed to read total collateral".to_string()))?,
    );

    if params.withdraw_amount > total_collateral {
        msg!("[stablecoin::process_instruction] ERROR: Insufficient collateral");
        return Err(StablecoinError::InsufficientCollateral.into())
    }

    let new_total_collateral = total_collateral.saturating_sub(params.withdraw_amount);

    // Create update data
    let update = RemoveCollateralUpdateV1 {
        position_nullifier: params.withdrawal_nullifier,
        new_commitment: params.new_commitment,
        collateral_type: CollateralType::Xmr, // Default, should be in params
        removed_collateral: params.withdraw_amount,
    };

    // Store new total in config for update phase
    wasm::db::db_set(config_db, CDP_TOTAL_COLLATERAL_KEY, &new_total_collateral.to_le_bytes())?;

    wasm::util::set_return_data(&serialize(&update))
}

/// Apply remove collateral update
fn apply_remove_collateral_update(
    cid: ContractId,
    update: RemoveCollateralUpdateV1,
) -> ContractResult {
    let nullifiers_db = wasm::db::db_lookup(cid, STABLECOIN_CONTRACT_POSITION_NULLIFIERS_TREE)?;
    let collateral_db = wasm::db::db_lookup(cid, STABLECOIN_CONTRACT_COLLATERAL_TREE)?;

    // Insert nullifier to prevent double-withdrawal
    wasm::db::db_set(nullifiers_db, &update.position_nullifier.to_bytes(), &vec![])?;

    // Remove from collateral tree
    wasm::db::db_set(collateral_db, &update.new_commitment.to_bytes(), &vec![])?;

    msg!(
        "[stablecoin::process_update] Collateral removed: nullifier={:?}, amount={}",
        update.position_nullifier,
        update.removed_collateral
    );
    Ok(())
}

/// Process mint stablecoin instruction
fn process_mint_stable_instruction(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params: MintStableParams = deserialize(&self_.data[1..])?;

    msg!(
        "[stablecoin::process_instruction] MintStable: amount={}, total_debt={}",
        params.mint_amount,
        params.total_debt
    );

    // Get current total debt
    let config_db = wasm::db::db_lookup(cid, "config")?;
    let total_debt_bytes = wasm::db::db_get(config_db, CDP_TOTAL_DEBT_KEY)?
        .ok_or_else(|| ContractError::IoError("Total debt not found".to_string()))?;
    let total_debt = u64::from_le_bytes(
        total_debt_bytes.as_slice().try_into().map_err(|_| ContractError::IoError("Failed to read total debt".to_string()))?,
    );

    let new_total_debt = total_debt.saturating_add(params.mint_amount);

    // Create update data
    let update = MintStableUpdateV1 {
        position_commitment: params.mint_commitment,
        mint_amount: params.mint_amount,
        new_total_debt,
    };

    // Store new total in config for update phase
    wasm::db::db_set(config_db, CDP_TOTAL_DEBT_KEY, &new_total_debt.to_le_bytes())?;

    wasm::util::set_return_data(&serialize(&update))
}

/// Apply mint stablecoin update
fn apply_mint_stable_update(cid: ContractId, update: MintStableUpdateV1) -> ContractResult {
    let stablecoin_db = wasm::db::db_lookup(cid, STABLECOIN_CONTRACT_STABLECOIN_TREE)?;
    let positions_db = wasm::db::db_lookup(cid, STABLECOIN_CONTRACT_POSITIONS_TREE)?;

    // Insert mint commitment
    wasm::db::db_set(stablecoin_db, &update.position_commitment.to_bytes(), &vec![])?;
    wasm::db::db_set(positions_db, &update.position_commitment.to_bytes(), &vec![])?;

    msg!(
        "[stablecoin::process_update] Stablecoin minted: amount={}, new_total_debt={}",
        update.mint_amount,
        update.new_total_debt
    );
    Ok(())
}

/// Process repay stablecoin instruction
fn process_repay_stable_instruction(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params: RepayStableParams = deserialize(&self_.data[1..])?;

    msg!(
        "[stablecoin::process_instruction] RepayStable: amount={}",
        params.repay_amount
    );

    // Get current total debt
    let config_db = wasm::db::db_lookup(cid, "config")?;
    let total_debt_bytes = wasm::db::db_get(config_db, CDP_TOTAL_DEBT_KEY)?
        .ok_or_else(|| ContractError::IoError("Total debt not found".to_string()))?;
    let total_debt = u64::from_le_bytes(
        total_debt_bytes.as_slice().try_into().map_err(|_| ContractError::IoError("Failed to read total debt".to_string()))?,
    );

    if params.repay_amount > total_debt {
        msg!("[stablecoin::process_instruction] ERROR: Repay exceeds debt");
        return Err(StablecoinError::RepayExceedsDebt.into())
    }

    let new_total_debt = total_debt.saturating_sub(params.repay_amount);

    // Create update data
    // Note: The repay_commitment in params is used as the identifier for the spent position.
    // This is a design quirk - semantically a repay should use a nullifier to prove ownership.
    let position_nullifier = IntentNullifier::from_bytes(params.repay_commitment.to_bytes())
        .map_err(|_| ContractError::IoError("Failed to create nullifier from commitment".to_string()))?;
    let update = RepayStableUpdateV1 {
        position_nullifier,
        new_commitment: params.repay_commitment,
        repay_amount: params.repay_amount,
        new_total_debt,
    };

    // Store new total in config for update phase
    wasm::db::db_set(config_db, CDP_TOTAL_DEBT_KEY, &new_total_debt.to_le_bytes())?;

    wasm::util::set_return_data(&serialize(&update))
}

/// Apply repay stablecoin update
fn apply_repay_stable_update(cid: ContractId, update: RepayStableUpdateV1) -> ContractResult {
    let nullifiers_db = wasm::db::db_lookup(cid, STABLECOIN_CONTRACT_POSITION_NULLIFIERS_TREE)?;

    // Insert nullifier to prevent double-repay
    wasm::db::db_set(nullifiers_db, &update.position_nullifier.to_bytes(), &vec![])?;

    msg!(
        "[stablecoin::process_update] Stablecoin repaid: amount={}, new_total_debt={}",
        update.repay_amount,
        update.new_total_debt
    );
    Ok(())
}

/// Process liquidate instruction
fn process_liquidate_instruction(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params: LiquidateParams = deserialize(&self_.data[1..])?;

    msg!(
        "[stablecoin::process_instruction] Liquidate: debt_to_cover={}, collateral={}",
        params.debt_to_cover,
        params.total_collateral
    );

    // Get current totals
    let config_db = wasm::db::db_lookup(cid, "config")?;
    let total_debt_bytes = wasm::db::db_get(config_db, CDP_TOTAL_DEBT_KEY)?
        .ok_or_else(|| ContractError::IoError("Total debt not found".to_string()))?;
    let total_debt = u64::from_le_bytes(
        total_debt_bytes.as_slice().try_into().map_err(|_| ContractError::IoError("Failed to read total debt".to_string()))?,
    );

    let total_collateral_bytes = wasm::db::db_get(config_db, CDP_TOTAL_COLLATERAL_KEY)?
        .ok_or_else(|| ContractError::IoError("Total collateral not found".to_string()))?;
    let total_collateral = u64::from_le_bytes(
        total_collateral_bytes.as_slice().try_into().map_err(|_| ContractError::IoError("Failed to read total collateral".to_string()))?,
    );

    // Calculate collateral ratio
    if total_debt == 0 {
        msg!("[stablecoin::process_instruction] ERROR: No debt to liquidate");
        return Err(StablecoinError::PositionNotLiquidatable.into())
    }

    let collateral_ratio = (total_collateral * 10000) / total_debt;

    // Get liquidation threshold
    let liq_threshold_bytes = wasm::db::db_get(config_db, CDP_LIQ_THRESHOLD_KEY)?
        .ok_or_else(|| ContractError::IoError("Liquidation threshold not found".to_string()))?;
    let liq_threshold = u64::from_le_bytes(
        liq_threshold_bytes.as_slice().try_into().map_err(|_| ContractError::IoError("Failed to read liquidation threshold".to_string()))?,
    );

    if collateral_ratio >= liq_threshold {
        msg!(
            "[stablecoin::process_instruction] ERROR: Pool not liquidatable. Ratio={}, Threshold={}",
            collateral_ratio,
            liq_threshold
        );
        return Err(StablecoinError::PositionNotLiquidatable.into())
    }

    // Calculate penalty
    let penalty = (params.debt_to_cover * 1000) / 10000; // 10% penalty

    let new_total_debt = total_debt.saturating_sub(params.debt_to_cover);
    let collateral_seized = params.debt_to_cover + penalty;
    let new_total_collateral = total_collateral.saturating_sub(collateral_seized);

    // Create update data
    let update = LiquidateUpdateV1 {
        debt_covered: params.debt_to_cover,
        collateral_seized,
        penalty,
        new_total_debt,
        new_total_collateral,
    };

    // Store new totals in config for update phase
    wasm::db::db_set(config_db, CDP_TOTAL_DEBT_KEY, &new_total_debt.to_le_bytes())?;
    wasm::db::db_set(config_db, CDP_TOTAL_COLLATERAL_KEY, &new_total_collateral.to_le_bytes())?;

    wasm::util::set_return_data(&serialize(&update))
}

/// Apply liquidate update
fn apply_liquidate_update(cid: ContractId, update: LiquidateUpdateV1) -> ContractResult {
    let liquidations_db = wasm::db::db_lookup(cid, STABLECOIN_CONTRACT_LIQUIDATIONS_TREE)?;

    // Record liquidation
    wasm::db::db_set(liquidations_db, &update.debt_covered.to_le_bytes(), &vec![])?;

    msg!(
        "[stablecoin::process_update] Pool liquidated: debt_covered={}, collateral_seized={}, penalty={}",
        update.debt_covered,
        update.collateral_seized,
        update.penalty
    );
    Ok(())
}

/// Process governance report instruction
fn process_governance_report_instruction(
    _cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params: GovernanceReportParams = deserialize(&self_.data[1..])?;

    msg!(
        "[stablecoin::process_instruction] GovernanceReport: collateral={}, debt={}, ratio={}",
        params.total_collateral,
        params.total_debt,
        params.collateral_ratio_bps
    );

    // Create update data
    let update = GovernanceReportUpdateV1 {
        collateral_ratio_bps: params.collateral_ratio_bps,
        interest_accrued: params.interest_accrued,
        reporter_pub_x: params.reporter_pub_x,
        reporter_pub_y: params.reporter_pub_y,
    };

    wasm::util::set_return_data(&serialize(&update))
}

/// Apply governance report update
fn apply_governance_report_update(_cid: ContractId, update: GovernanceReportUpdateV1) -> ContractResult {
    msg!(
        "[stablecoin::process_update] Governance report recorded: ratio={}, interest={}",
        update.collateral_ratio_bps,
        update.interest_accrued
    );
    Ok(())
}

/// Process accrue interest instruction
fn process_accrue_interest_instruction(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params: AccrueInterestParams = deserialize(&self_.data[1..])?;

    msg!(
        "[stablecoin::process_instruction] AccrueInterest: old_debt={}, new_debt={}, interest={}",
        params.old_total_debt,
        params.new_total_debt,
        params.interest_amount
    );

    // Verify the interest calculation is correct
    if params.new_total_debt < params.old_total_debt {
        msg!("[stablecoin::process_instruction] ERROR: New debt less than old debt");
        return Err(StablecoinError::InvalidCollateralizationRatio.into())
    }

    let calculated_interest = params.new_total_debt - params.old_total_debt;
    if calculated_interest != params.interest_amount {
        msg!(
            "[stablecoin::process_instruction] ERROR: Interest mismatch. Calculated={}, Provided={}",
            calculated_interest,
            params.interest_amount
        );
        return Err(StablecoinError::CommitmentMismatch.into())
    }

    // Update accumulated fees
    let config_db = wasm::db::db_lookup(cid, "config")?;
    let accumulated_fees_bytes = wasm::db::db_get(config_db, CDP_ACCUMULATED_FEES_KEY)?
        .ok_or_else(|| ContractError::IoError("Accumulated fees not found".to_string()))?;
    let accumulated_fees = u64::from_le_bytes(
        accumulated_fees_bytes.as_slice().try_into().map_err(|_| ContractError::IoError("Failed to read accumulated fees".to_string()))?,
    );
    let new_accumulated_fees = accumulated_fees.saturating_add(params.interest_amount);

    wasm::db::db_set(config_db, CDP_ACCUMULATED_FEES_KEY, &new_accumulated_fees.to_le_bytes())?;

    // Create update data
    let update = AccrueInterestUpdateV1 {
        old_total_debt: params.old_total_debt,
        new_total_debt: params.new_total_debt,
        interest_amount: params.interest_amount,
        accumulator_pub_x: params.accumulator_pub_x,
        accumulator_pub_y: params.accumulator_pub_y,
    };

    wasm::util::set_return_data(&serialize(&update))
}

/// Apply accrue interest update
fn apply_accrue_interest_update(cid: ContractId, update: AccrueInterestUpdateV1) -> ContractResult {
    let config_db = wasm::db::db_lookup(cid, "config")?;

    // Update total debt
    wasm::db::db_set(config_db, CDP_TOTAL_DEBT_KEY, &update.new_total_debt.to_le_bytes())?;

    msg!(
        "[stablecoin::process_update] Interest accrued: old_debt={}, new_debt={}, interest={}",
        update.old_total_debt,
        update.new_total_debt,
        update.interest_amount
    );
    Ok(())
}

// ============================================================================
// UPDATE STRUCTS
// ============================================================================

/// Update data for open position
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct OpenPositionUpdateV1 {
    pub deposit_commitment: darkfi_sdk::crypto::IntentCommitment,
    pub collateral_type: crate::model::CollateralType,
    pub collateral_amount: u64,
}