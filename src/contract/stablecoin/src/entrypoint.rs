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
    crypto::ContractId,
    dark_tree::DarkLeaf,
    error::{ContractError, ContractResult},
    msg, ContractCall,
    wasm,
};
use darkfi_serial::{deserialize, serialize, Decodable, SerialDecodable, SerialEncodable};

use crate::{
    error::StablecoinError,
    model::{DepositCollateralParams, UpdateConfigParams, UpdateConfigUpdateV1},
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
        StablecoinFunction::AddCollateralV1 => {
            msg!("[stablecoin::process_instruction] AddCollateralV1 not yet implemented");
            Err(ContractError::IoError("Not yet implemented".to_string()).into())
        }
        StablecoinFunction::RemoveCollateralV1 => {
            msg!("[stablecoin::process_instruction] RemoveCollateralV1 not yet implemented");
            Err(ContractError::IoError("Not yet implemented".to_string()).into())
        }
        StablecoinFunction::MintStableV1 => {
            msg!("[stablecoin::process_instruction] MintStableV1 not yet implemented");
            Err(ContractError::IoError("Not yet implemented".to_string()).into())
        }
        StablecoinFunction::RepayStableV1 => {
            msg!("[stablecoin::process_instruction] RepayStableV1 not yet implemented");
            Err(ContractError::IoError("Not yet implemented".to_string()).into())
        }
        StablecoinFunction::LiquidateV1 => {
            msg!("[stablecoin::process_instruction] LiquidateV1 not yet implemented");
            Err(ContractError::IoError("Not yet implemented".to_string()).into())
        }
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
            msg!("[stablecoin::process_instruction] GovernanceReportV1 not yet implemented");
            Err(ContractError::IoError("Not yet implemented".to_string()).into())
        }
        StablecoinFunction::AccrueInterestV1 => {
            msg!("[stablecoin::process_instruction] AccrueInterestV1 not yet implemented");
            Err(ContractError::IoError("Not yet implemented".to_string()).into())
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
            msg!("[stablecoin::process_update] AddCollateralV1 not yet implemented");
            Ok(())
        }
        StablecoinFunction::RemoveCollateralV1 => {
            msg!("[stablecoin::process_update] RemoveCollateralV1 not yet implemented");
            Ok(())
        }
        StablecoinFunction::MintStableV1 => {
            msg!("[stablecoin::process_update] MintStableV1 not yet implemented");
            Ok(())
        }
        StablecoinFunction::RepayStableV1 => {
            msg!("[stablecoin::process_update] RepayStableV1 not yet implemented");
            Ok(())
        }
        StablecoinFunction::LiquidateV1 => {
            msg!("[stablecoin::process_update] LiquidateV1 not yet implemented");
            Ok(())
        }
        StablecoinFunction::UpdateConfigV1 => {
            let update: UpdateConfigUpdateV1 = deserialize(&update_data[1..])?;
            apply_config_update(cid, update)
        }
        StablecoinFunction::GovernanceReportV1 => {
            msg!("[stablecoin::process_update] GovernanceReportV1 not yet implemented");
            Ok(())
        }
        StablecoinFunction::AccrueInterestV1 => {
            msg!("[stablecoin::process_update] AccrueInterestV1 not yet implemented");
            Ok(())
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