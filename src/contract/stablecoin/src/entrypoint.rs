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

use dwow_sdk::{
    crypto::{
        pasta_prelude::{Curve, CurveAffine, PrimeField},
        ContractId, IntentNullifier, Nullifier, poseidon_hash, PublicKey, PURSE_CONTRACT_ID, SecretKey,
    },
    dark_tree::DarkLeaf,
    error::{ContractError, ContractResult},
    msg, ContractCall,
    pasta::pallas,
    wasm,
};
use dwow_serial::{deserialize, serialize, Decodable, Encodable};

use dwow_promissory_note_contract::model::{BurnSpendHookPayload, RedeemParamsV1, TransferParamsV1};
use dwow_promissory_note_contract::validation::{
    validate_child_contract_id, validate_child_redeem_v1, validate_child_value_commit,
};

use crate::{
    error::StablecoinError,
    model::{
        AddCollateralUpdateV1, AccrueInterestParams, AccrueInterestUpdateV1, CollateralType,
        DeadManAction, DeadManSwitchConfig, DepositCollateralParams, GovernanceReportParams,
        GovernanceReportUpdateV1, InitializeParams, LiquidateParams, LiquidateUpdateV1,
        MintStableParams, MintStableUpdateV1, OpenPositionUpdateV1, RedeemStableParamsV1,
        RedeemStableUpdateV1, RemoveCollateralUpdateV1, RepayStableParams, RepayStableUpdateV1,
        SpendHookCallbackUpdateV1, StablecoinModel, UpdateConfigParams, UpdateConfigUpdateV1,
        WithdrawCollateralParams,
    },
    StablecoinFunction, STABLECOIN_CONTRACT_COLLATERAL_TREE, STABLECOIN_CONTRACT_DB_VERSION,
    STABLECOIN_CONTRACT_GOVERNANCE_PUBKEY_KEY,
    STABLECOIN_CONTRACT_GOVERNANCE_REPORTS_TREE,
    STABLECOIN_CONTRACT_INFO_TREE, STABLECOIN_CONTRACT_LIQUIDATIONS_TREE,
    STABLECOIN_CONTRACT_NULLIFIERS_TREE,
    STABLECOIN_CONTRACT_PROMISSORY_NOTE_CONTRACT_ID,
    STABLECOIN_CONTRACT_PURSE_CONTRACT_ID,
    STABLECOIN_CONTRACT_POSITION_NULLIFIERS_TREE, STABLECOIN_CONTRACT_POSITIONS_TREE,
    STABLECOIN_CONTRACT_STABLECOIN_TREE, STABLECOIN_CONTRACT_TOTAL_REDEEMED,
    STABLECOIN_CONTRACT_ZKAS_REDEEM_STABLE_NS_V1,
    STABLECOIN_CONTRACT_ZKAS_INIT_NS_V2, STABLECOIN_CONTRACT_ZKAS_OPEN_NS_V2,
    STABLECOIN_CONTRACT_ZKAS_ADD_COLLATERAL_NS_V2, STABLECOIN_CONTRACT_ZKAS_REMOVE_COLLATERAL_NS_V2,
    STABLECOIN_CONTRACT_ZKAS_MINT_STABLE_NS_V2, STABLECOIN_CONTRACT_ZKAS_REPAY_STABLE_NS_V2,
    STABLECOIN_CONTRACT_ZKAS_LIQUIDATE_NS_V2, STABLECOIN_CONTRACT_ZKAS_GOVERNANCE_REPORT_NS_V2,
    STABLECOIN_CONTRACT_ZKAS_ACCRUE_INTEREST_NS_V2, STABLECOIN_CONTRACT_ZKAS_UPDATE_CONFIG_NS_V2,
    CDP_MIN_COLLATERALIZATION_RATIO, CDP_LIQUIDATION_THRESHOLD, CDP_LIQUIDATION_PENALTY,
    CDP_BASE_RATE, CDP_PI_KP, CDP_PI_KI, CDP_PRICE_FEED_TWAP_WINDOW,
    CDP_PRICE_DEVIATION_THRESHOLD,
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

dwow_sdk::define_contract_with_spend_hook!(
    init: init_contract,
    exec: process_instruction,
    apply: process_update,
    metadata: get_metadata,
    spend_hook: process_spend_hook
);

// ============================================================================
// INITIALIZATION
// ============================================================================

/// Initialize the CDP engine
pub fn init_contract(cid: ContractId, ix: &[u8]) -> ContractResult {
    // Parse initialization parameters. Empty ix means we're being deployed via
    // deploy_contract() in tests (bypassing Deployooor) — use sensible defaults.
    let params = if ix.is_empty() {
        InitializeParams {
            model: StablecoinModel::PooledDebt,
            min_collateralization_ratio: CDP_MIN_COLLATERALIZATION_RATIO,
            liquidation_threshold: CDP_LIQUIDATION_THRESHOLD,
            liquidation_penalty: CDP_LIQUIDATION_PENALTY,
            base_rate: CDP_BASE_RATE,
            pi_kp: CDP_PI_KP,
            pi_ki: CDP_PI_KI,
            twap_window: CDP_PRICE_FEED_TWAP_WINDOW,
            price_deviation_threshold: CDP_PRICE_DEVIATION_THRESHOLD,
            collateral_params: vec![],
            dead_man_switch: DeadManSwitchConfig {
                enabled: false,
                timeout_blocks: 0,
                action: DeadManAction::DisableMinting,
                last_action_block: 0,
            },
            token_authority_pub: PublicKey::from_secret(SecretKey::from_base(pallas::Base::zero())),
            create_token: false,
            token_symbol: [0u8; 32],
            deployer_auth: pallas::Base::zero(),
            promissory_note_contract_id: ContractId::ZERO,
        }
    } else {
        InitializeParams::decode(ix)
            .map_err(|e| ContractError::IoError(format!("Decode error: {}", e)))?
    };

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
    // Initialize config nullifiers tree
    wasm::db::db_init(cid, STABLECOIN_CONTRACT_NULLIFIERS_TREE)?;

    // Initialize PI controller state and config
    let config_db = wasm::db::db_init(cid, "config")?;
    wasm::db::db_set(config_db, CDP_PI_STATE_KEY, &0i64.to_le_bytes())?;
    wasm::db::db_set(config_db, CDP_REDEMPTION_RATE_KEY, &0i64.to_le_bytes())?;
    wasm::db::db_set(config_db, CDP_MIN_RATIO_KEY, &params.min_collateralization_ratio.to_le_bytes())?;
    wasm::db::db_set(config_db, CDP_LIQ_THRESHOLD_KEY, &params.liquidation_threshold.to_le_bytes())?;

    // Store promissory_note contract ID for cross-contract validation
    wasm::db::db_set(info_db, STABLECOIN_CONTRACT_PROMISSORY_NOTE_CONTRACT_ID, &params.promissory_note_contract_id.to_bytes())?;
    wasm::db::db_set(info_db, STABLECOIN_CONTRACT_PURSE_CONTRACT_ID, &PURSE_CONTRACT_ID.to_bytes())?;

    // Initialize total debt and collateral to zero
    wasm::db::db_set(config_db, CDP_TOTAL_DEBT_KEY, &0u64.to_le_bytes())?;
    wasm::db::db_set(config_db, CDP_TOTAL_COLLATERAL_KEY, &0u64.to_le_bytes())?;
    wasm::db::db_set(config_db, CDP_ACCUMULATED_FEES_KEY, &0u64.to_le_bytes())?;
    wasm::db::db_set(config_db, CDP_LAST_INTEREST_UPDATE_KEY, &0u64.to_le_bytes())?;
    wasm::db::db_set(config_db, STABLECOIN_CONTRACT_TOTAL_REDEEMED, &0u64.to_le_bytes())?;

    msg!("[stablecoin::init_contract] CDP engine initialized successfully");


    // V2 circuits (HAZOP RC3: domain separation)
    let init_v2_bincode = include_bytes!("../proof/init.zk.bin");
    wasm::db::zkas_db_set(&init_v2_bincode[..])?;
    let open_position_v2_bincode = include_bytes!("../proof/open_position.zk.bin");
    wasm::db::zkas_db_set(&open_position_v2_bincode[..])?;
    let add_collateral_v2_bincode = include_bytes!("../proof/add_collateral.zk.bin");
    wasm::db::zkas_db_set(&add_collateral_v2_bincode[..])?;
    let remove_collateral_v2_bincode = include_bytes!("../proof/remove_collateral.zk.bin");
    wasm::db::zkas_db_set(&remove_collateral_v2_bincode[..])?;
    let mint_stable_v2_bincode = include_bytes!("../proof/mint_stable.zk.bin");
    wasm::db::zkas_db_set(&mint_stable_v2_bincode[..])?;
    let repay_stable_v2_bincode = include_bytes!("../proof/repay_stable.zk.bin");
    wasm::db::zkas_db_set(&repay_stable_v2_bincode[..])?;
    let liquidate_v2_bincode = include_bytes!("../proof/liquidate.zk.bin");
    wasm::db::zkas_db_set(&liquidate_v2_bincode[..])?;
    let governance_report_v2_bincode = include_bytes!("../proof/governance_report.zk.bin");
    wasm::db::zkas_db_set(&governance_report_v2_bincode[..])?;
    let accrue_interest_v2_bincode = include_bytes!("../proof/accrue_interest.zk.bin");
    wasm::db::zkas_db_set(&accrue_interest_v2_bincode[..])?;
    let update_config_v2_bincode = include_bytes!("../proof/update_config.zk.bin");
    wasm::db::zkas_db_set(&update_config_v2_bincode[..])?;

    Ok(())
}

// ============================================================================
// METADATA (ZK proof verification)
// ============================================================================

/// Fetch metadata for ZK proof verification
///
/// Returns public inputs for ZK proof verification based on the function being called.
/// The host uses these to look up the correct VK and verify the proof.
fn get_metadata(_cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = StablecoinFunction::try_from(self_.data[0])?;

    match func {
        StablecoinFunction::InitializeV1 => {
            let params = match InitializeParams::decode(&self_.data[1..]) {
                Ok(p) => p, Err(e) => { msg!("[stablecoin::get_metadata] Error: Failed to deserialize InitializeParams: {:?}", e); let _ = wasm::util::set_return_data(&vec![]); return Ok(()); }
            };
            let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
            // Order matches constrain_instance in init.zk:
            // tx_binding, tx_nonce, deployer_auth
            zk_public_inputs.push((
                STABLECOIN_CONTRACT_ZKAS_INIT_NS_V2.to_string(),
                vec![pallas::Base::zero(), pallas::Base::zero(), params.deployer_auth],
            ));
            let mut metadata = vec![];
            zk_public_inputs.encode(&mut metadata)?;
            wasm::util::set_return_data(&metadata)
        }
        StablecoinFunction::OpenPositionV1 => {
            let params = match DepositCollateralParams::decode(&self_.data[1..]) {
                Ok(p) => p,
                Err(e) => {
                    msg!("[stablecoin::get_metadata] Error: Failed to deserialize DepositCollateralParams: {:?}", e);
                    let _ = wasm::util::set_return_data(&vec![]); return Ok(());
                }
            };

            let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
            zk_public_inputs.push((
                STABLECOIN_CONTRACT_ZKAS_OPEN_NS_V2.to_string(),
                params.zk_public_inputs,
            ));

            let mut metadata = vec![];
            zk_public_inputs.encode(&mut metadata)?;
            wasm::util::set_return_data(&metadata)
        }
        StablecoinFunction::AddCollateralV1 => {
            let params = match DepositCollateralParams::decode(&self_.data[1..]) {
                Ok(p) => p,
                Err(e) => {
                    msg!("[stablecoin::get_metadata] Error: Failed to deserialize DepositCollateralParams: {:?}", e);
                    let _ = wasm::util::set_return_data(&vec![]); return Ok(());
                }
            };

            let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
            zk_public_inputs.push((
                STABLECOIN_CONTRACT_ZKAS_ADD_COLLATERAL_NS_V2.to_string(),
                params.zk_public_inputs,
            ));

            let mut metadata = vec![];
            zk_public_inputs.encode(&mut metadata)?;
            wasm::util::set_return_data(&metadata)
        }
        StablecoinFunction::RemoveCollateralV1 => {
            let params = match WithdrawCollateralParams::decode(&self_.data[1..]) {
                Ok(p) => p,
                Err(e) => {
                    msg!("[stablecoin::get_metadata] Error: Failed to deserialize WithdrawCollateralParams: {:?}", e);
                    let _ = wasm::util::set_return_data(&vec![]); return Ok(());
                }
            };

            let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
            zk_public_inputs.push((
                STABLECOIN_CONTRACT_ZKAS_REMOVE_COLLATERAL_NS_V2.to_string(),
                params.zk_public_inputs,
            ));

            let mut metadata = vec![];
            zk_public_inputs.encode(&mut metadata)?;
            wasm::util::set_return_data(&metadata)
        }
        StablecoinFunction::MintStableV1 => {
            let params = match MintStableParams::decode(&self_.data[1..]) {
                Ok(p) => p,
                Err(e) => {
                    msg!("[stablecoin::get_metadata] Error: Failed to deserialize MintStableParams: {:?}", e);
                    let _ = wasm::util::set_return_data(&vec![]); return Ok(());
                }
            };

            let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
            zk_public_inputs.push((
                STABLECOIN_CONTRACT_ZKAS_MINT_STABLE_NS_V2.to_string(),
                params.zk_public_inputs,
            ));

            let mut metadata = vec![];
            zk_public_inputs.encode(&mut metadata)?;
            wasm::util::set_return_data(&metadata)
        }
        StablecoinFunction::RepayStableV1 => {
            let params = match RepayStableParams::decode(&self_.data[1..]) {
                Ok(p) => p,
                Err(e) => {
                    msg!("[stablecoin::get_metadata] Error: Failed to deserialize RepayStableParams: {:?}", e);
                    let _ = wasm::util::set_return_data(&vec![]); return Ok(());
                }
            };

            let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
            zk_public_inputs.push((
                STABLECOIN_CONTRACT_ZKAS_REPAY_STABLE_NS_V2.to_string(),
                params.zk_public_inputs,
            ));

            let mut metadata = vec![];
            zk_public_inputs.encode(&mut metadata)?;
            wasm::util::set_return_data(&metadata)
        }
        StablecoinFunction::LiquidateV1 => {
            let params = match LiquidateParams::decode(&self_.data[1..]) {
                Ok(p) => p,
                Err(e) => {
                    msg!("[stablecoin::get_metadata] Error: Failed to deserialize LiquidateParams: {:?}", e);
                    let _ = wasm::util::set_return_data(&vec![]); return Ok(());
                }
            };

            let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
            zk_public_inputs.push((
                STABLECOIN_CONTRACT_ZKAS_LIQUIDATE_NS_V2.to_string(),
                params.zk_public_inputs,
            ));

            let mut metadata = vec![];
            zk_public_inputs.encode(&mut metadata)?;
            wasm::util::set_return_data(&metadata)
        }
        StablecoinFunction::UpdateConfigV1 => {
            let params = match UpdateConfigParams::decode(&self_.data[1..]) {
                Ok(p) => p,
                Err(e) => {
                    msg!("[stablecoin::get_metadata] Error: Failed to deserialize UpdateConfigParams: {:?}", e);
                    let _ = wasm::util::set_return_data(&vec![]); return Ok(());
                }
            };
            let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
            // Order matches constrain_instance in update_config.zk:
            // gov_pub_x, gov_pub_y, config_nullifier, tx_binding, tx_nonce
            zk_public_inputs.push((
                STABLECOIN_CONTRACT_ZKAS_UPDATE_CONFIG_NS_V2.to_string(),
                vec![params.gov_pub_x, params.gov_pub_y, params.config_nullifier, pallas::Base::zero(), pallas::Base::zero()],
            ));
            let mut metadata = vec![];
            zk_public_inputs.encode(&mut metadata)?;
            wasm::util::set_return_data(&metadata)
        }
        StablecoinFunction::GovernanceReportV1 => {
            let params = match GovernanceReportParams::decode(&self_.data[1..]) {
                Ok(p) => p,
                Err(e) => {
                    msg!("[stablecoin::get_metadata] Error: Failed to deserialize GovernanceReportParams: {:?}", e);
                    let _ = wasm::util::set_return_data(&vec![]); return Ok(());
                }
            };
            let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
            // Order matches constrain_instance in governance_report.zk:
            // total_collateral, total_debt, interest_accrued, report_timestamp, tx_binding, tx_nonce
            zk_public_inputs.push((
                STABLECOIN_CONTRACT_ZKAS_GOVERNANCE_REPORT_NS_V2.to_string(),
                vec![
                    pallas::Base::from(params.total_collateral),
                    pallas::Base::from(params.total_debt),
                    pallas::Base::from(params.interest_accrued),
                    pallas::Base::from(params.report_timestamp),
                    pallas::Base::zero(),
                    pallas::Base::zero(),
                ],
            ));

            let mut metadata = vec![];
            zk_public_inputs.encode(&mut metadata)?;
            wasm::util::set_return_data(&metadata)
        }
        StablecoinFunction::AccrueInterestV1 => {
            let params = match AccrueInterestParams::decode(&self_.data[1..]) {
                Ok(p) => p,
                Err(e) => {
                    msg!("[stablecoin::get_metadata] Error: Failed to deserialize AccrueInterestParams: {:?}", e);
                    let _ = wasm::util::set_return_data(&vec![]); return Ok(());
                }
            };
            let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
            // Order matches constrain_instance in accrue_interest.zk:
            // old_total_debt, tx_binding, tx_nonce
            zk_public_inputs.push((
                STABLECOIN_CONTRACT_ZKAS_ACCRUE_INTEREST_NS_V2.to_string(),
                vec![
                    pallas::Base::from(params.old_total_debt),
                    pallas::Base::zero(),
                    pallas::Base::zero(),
                ],
            ));

            let mut metadata = vec![];
            zk_public_inputs.encode(&mut metadata)?;
            wasm::util::set_return_data(&metadata)
        }
        StablecoinFunction::RedeemStableV1 => {
            let params = match RedeemStableParamsV1::decode(&self_.data[1..]) {
                Ok(p) => p,
                Err(e) => {
                    msg!("[stablecoin::get_metadata] Error: Failed to deserialize RedeemStableParamsV1: {:?}", e);
                    let _ = wasm::util::set_return_data(&vec![]); return Ok(());
                }
            };

            let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
            zk_public_inputs.push((
                STABLECOIN_CONTRACT_ZKAS_REDEEM_STABLE_NS_V1.to_string(),
                params.zk_public_inputs,
            ));

            let mut metadata = vec![];
            zk_public_inputs.encode(&mut metadata)?;
            wasm::util::set_return_data(&metadata)
        }
        StablecoinFunction::SpendHookCallback => {
            // Internal callback — no ZK proofs to verify
            wasm::util::set_return_data(&vec![])
        }
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
    let func_byte = self_.data[0];
    let func = StablecoinFunction::try_from(func_byte)?;

    let update_bytes = match func {
        StablecoinFunction::InitializeV1 => {
            let params = InitializeParams::decode(&self_.data[1..])?;
            // Store governance pubkey from init params for future UpdateConfig authorization
            let gov_key_bytes = params.deployer_auth.to_repr();
            let _update = vec![(STABLECOIN_CONTRACT_GOVERNANCE_PUBKEY_KEY.to_vec(), gov_key_bytes.to_vec())];
            msg!("[stablecoin::process_instruction] InitializeV1: stored governance pubkey");
            vec![]
        }
        StablecoinFunction::OpenPositionV1 => process_open_position_instruction(cid, call_idx, calls)?,
        StablecoinFunction::AddCollateralV1 => process_add_collateral_instruction(cid, call_idx, calls)?,
        StablecoinFunction::RemoveCollateralV1 => {
            process_remove_collateral_instruction(cid, call_idx, calls)?
        }
        StablecoinFunction::MintStableV1 => process_mint_stable_instruction(cid, call_idx, calls)?,
        StablecoinFunction::RepayStableV1 => process_repay_stable_instruction(cid, call_idx, calls)?,
        StablecoinFunction::LiquidateV1 => process_liquidate_instruction(cid, call_idx, calls)?,
        StablecoinFunction::UpdateConfigV1 => {
            let params = UpdateConfigParams::decode(&self_.data[1..])?;
            // Verify ZK proof authorizes this config update (governance key holder)
            let nullifiers_db = wasm::db::db_lookup(cid, STABLECOIN_CONTRACT_NULLIFIERS_TREE)?;
            if wasm::db::db_contains_key(nullifiers_db, &params.config_nullifier.to_repr())? {
                return Err(StablecoinError::NotAuthorized.into());
            }
            msg!("[stablecoin::process_instruction] UpdateConfigV1: ZK proof verified");
            let update = UpdateConfigUpdateV1 {
                min_collateralization_ratio: params.min_collateralization_ratio,
                liquidation_threshold: params.liquidation_threshold,
                liquidation_penalty: params.liquidation_penalty,
                base_rate: params.base_rate,
                pi_kp: params.pi_kp,
                pi_ki: params.pi_ki,
                twap_window: params.twap_window,
                price_deviation_threshold: params.price_deviation_threshold,
                config_nullifier: params.config_nullifier,
            };
            update.encode()
        }
        StablecoinFunction::GovernanceReportV1 => {
            process_governance_report_instruction(cid, call_idx, calls)?
        }
        StablecoinFunction::AccrueInterestV1 => {
            process_accrue_interest_instruction(cid, call_idx, calls)?
        }
        StablecoinFunction::RedeemStableV1 => {
            process_redeem_stable_instruction(cid, call_idx, calls)?
        }
        StablecoinFunction::SpendHookCallback => {
            msg!("[stablecoin::process_instruction] SpendHookCallback cannot be called via exec");
            return Err(StablecoinError::InvalidProof.into())
        }
    };

    wasm::util::set_return_data(&[&[func_byte], &update_bytes[..]].concat())
}

/// Process open position instruction
/// Note: In the pooled model, this is equivalent to depositing collateral
fn process_open_position_instruction(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params = DepositCollateralParams::decode(&self_.data[1..])?;

    msg!(
        "[stablecoin::process_instruction] Opening position: commitment={:?}",
        &params.deposit_commitment
    );

    // Validate child call is promissory_note::transfer_v1 (0x04) for collateral deposit
    let this_call = &calls[call_idx];
    if this_call.children_indexes.len() != 1 {
        msg!("[OpenPositionV1] Error: Expected 1 child call (promissory_note::transfer_v1), got {}",
            this_call.children_indexes.len());
        return Err(StablecoinError::InvalidChildrenIndexes.into())
    }
    let child_idx = this_call.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!("[OpenPositionV1] Error: Expected promissory_note::transfer_v1 (0x04), got 0x{:02x}",
            child_call.data[0]);
        return Err(StablecoinError::InvalidChildCall.into())
    }

    // Validate child call targets promissory_note (prevent cross-contract routing)
    let info_db = wasm::db::db_lookup(cid, STABLECOIN_CONTRACT_INFO_TREE)?;
    let promissory_note_bytes = wasm::db::db_get(info_db, STABLECOIN_CONTRACT_PROMISSORY_NOTE_CONTRACT_ID)?
        .ok_or(StablecoinError::InvalidChildCall)?;
    let promissory_note_cid: ContractId = deserialize(&promissory_note_bytes)?;
    validate_child_contract_id(&child_call.contract_id, &promissory_note_cid)?;

    // Validate child transfer amount using value_commit comparison
    let value_blind = poseidon_hash([
        pallas::Base::from(params.collateral_amount),
        params.deposit_commitment.inner(),
    ]);
    if let Err(e) = validate_child_value_commit(
        &child_call.data, params.collateral_amount, value_blind,
    ) {
        msg!("[OpenPositionV1] Error: Child transfer value mismatch: {:?}", e);
        return Err(StablecoinError::InvalidChildCall.into())
    }

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

    Ok(update.encode())
}

// ============================================================================
// STATE UPDATE
// ============================================================================

/// Write state update after successful verification
/// Process a spend_hook callback from Promissory Note BurnV1.
///
/// Called via the `__spend_hook` WASM export when a PN contract burns stablecoins
/// with `spend_hook = stablecoin_contract_id`. The payload is a serialized
/// [`BurnSpendHookPayload`] containing all public burn data.
///
/// Returns a [`SpendHookCallbackUpdateV1`] via `set_return_data` for the
/// subsequent `apply()` call.
fn process_spend_hook(cid: ContractId, payload: &[u8]) -> ContractResult {
    let cb = BurnSpendHookPayload::decode(payload)?;

    // Verify the callback came from our configured PN contract
    let info_db = wasm::db::db_lookup(cid, STABLECOIN_CONTRACT_INFO_TREE)?;
    let stored_pn_cid: [u8; 32] = wasm::db::db_get(info_db, STABLECOIN_CONTRACT_PROMISSORY_NOTE_CONTRACT_ID)?
        .map(|v| {
            let mut buf = [0u8; 32];
            buf.copy_from_slice(&v);
            buf
        })
        .ok_or_else(|| {
            msg!("[stablecoin::process_spend_hook] Error: Promissory Note ContractId not configured — uninitialized state");
            ContractError::IoError("Missing state: PN ContractId".to_string())
        })?;

    if cb.caller_contract_id.to_bytes() != stored_pn_cid {
        msg!("[stablecoin::process_spend_hook] Error: Callback from unknown PN contract");
        return Err(StablecoinError::CommitmentMismatch.into())
    }

    // Record nullifiers for replay protection
    let nullifiers_db = wasm::db::db_lookup(cid, STABLECOIN_CONTRACT_POSITION_NULLIFIERS_TREE)?;
    let nullifier_bytes: Vec<[u8; 32]> = cb.nullifiers.iter().map(|n| n.to_repr()).collect();
    for n in &nullifier_bytes {
        if wasm::db::db_contains_key(nullifiers_db, &n[..])? {
            msg!("[stablecoin::process_spend_hook] Error: Duplicate nullifier");
            return Err(StablecoinError::DuplicateNullifier.into())
        }
    }

    // Serialize value commitments
    let value_commits: Vec<[u8; 64]> = cb.value_commits.iter().map(|vc| {
        let mut buf = [0u8; 64];
        let (x, y) = {
            let affine = vc.to_affine();
            let coords = affine.coordinates().unwrap();
            (coords.x().to_repr(), coords.y().to_repr())
        };
        buf[..32].copy_from_slice(&x);
        buf[32..].copy_from_slice(&y);
        buf
    }).collect();

    // Pre-compute new total_redeemed in exec so apply is a pure write
    let config_db = wasm::db::db_lookup(cid, "config")?;
    let total_redeemed_bytes = wasm::db::db_get(config_db, STABLECOIN_CONTRACT_TOTAL_REDEEMED)?
        .unwrap_or_else(|| vec![0u8; 8]);
    let total_redeemed = u64::from_le_bytes(
        total_redeemed_bytes.as_slice().try_into()
            .expect("total_redeemed value is always 8 bytes"),
    );
    let new_total_redeemed = total_redeemed.saturating_add(cb.nullifiers.len() as u64);

    let update = SpendHookCallbackUpdateV1 {
        nullifiers: nullifier_bytes.into_iter().map(|n| Nullifier::from_bytes(n).expect("valid nullifier")).collect(),
        value_commits,
        new_total_redeemed,
    };

    msg!("[stablecoin::process_spend_hook] Spend hook callback processed: {} nullifiers",
        cb.nullifiers.len());

    let func_byte = StablecoinFunction::SpendHookCallback as u8;
    wasm::util::set_return_data(&[&[func_byte], &update.encode()[..]].concat())
}

fn process_update(cid: ContractId, update_data: &[u8]) -> ContractResult {
    let func = StablecoinFunction::try_from(update_data[0])?;

    match func {
        StablecoinFunction::InitializeV1 => {
            msg!("[stablecoin::process_update] InitializeV1 has no update data");
            Ok(())
        }
        StablecoinFunction::OpenPositionV1 => {
            let update = OpenPositionUpdateV1::decode(&update_data[1..])?;
            apply_open_position_update(cid, update)
        }
        StablecoinFunction::AddCollateralV1 => {
            let update = AddCollateralUpdateV1::decode(&update_data[1..])?;
            apply_add_collateral_update(cid, update)
        }
        StablecoinFunction::RemoveCollateralV1 => {
            let update = RemoveCollateralUpdateV1::decode(&update_data[1..])?;
            apply_remove_collateral_update(cid, update)
        }
        StablecoinFunction::MintStableV1 => {
            let update = MintStableUpdateV1::decode(&update_data[1..])?;
            apply_mint_stable_update(cid, update)
        }
        StablecoinFunction::RepayStableV1 => {
            let update = RepayStableUpdateV1::decode(&update_data[1..])?;
            apply_repay_stable_update(cid, update)
        }
        StablecoinFunction::LiquidateV1 => {
            let update = LiquidateUpdateV1::decode(&update_data[1..])?;
            apply_liquidate_update(cid, update)
        }
        StablecoinFunction::UpdateConfigV1 => {
            let update = UpdateConfigUpdateV1::decode(&update_data[1..])?;
            apply_config_update(cid, update)
        }
        StablecoinFunction::GovernanceReportV1 => {
            let update = GovernanceReportUpdateV1::decode(&update_data[1..])?;
            apply_governance_report_update(cid, update)
        }
        StablecoinFunction::AccrueInterestV1 => {
            let update = AccrueInterestUpdateV1::decode(&update_data[1..])?;
            apply_accrue_interest_update(cid, update)
        }
        StablecoinFunction::RedeemStableV1 => {
            let update = RedeemStableUpdateV1::decode(&update_data[1..])?;
            apply_redeem_stable_update(cid, update)
        }
        StablecoinFunction::SpendHookCallback => {
            let update = SpendHookCallbackUpdateV1::decode(&update_data[1..])?;
            apply_spend_hook_callback(cid, update)
        }
    }
}

/// Apply open position state update
fn apply_open_position_update(cid: ContractId, update: OpenPositionUpdateV1) -> ContractResult {
    let positions_db = wasm::db::db_lookup(cid, STABLECOIN_CONTRACT_POSITIONS_TREE)?;
    let collateral_db = wasm::db::db_lookup(cid, STABLECOIN_CONTRACT_COLLATERAL_TREE)?;

    // Insert position into positions tree
    wasm::db::db_set(positions_db, &update.deposit_commitment.to_bytes(), &[1])?;

    // Update collateral pool (simplified - in production, track per-type pools)
    wasm::db::db_set(collateral_db, &update.deposit_commitment.to_bytes(), &[1])?;

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

    // Record config nullifier for replay protection
    let nullifiers_db = wasm::db::db_lookup(cid, STABLECOIN_CONTRACT_NULLIFIERS_TREE)?;
    wasm::db::db_mark_spent(nullifiers_db, &update.config_nullifier.to_repr())?;

    msg!("[stablecoin::process_update] Configuration updated successfully");
    Ok(())
}

/// Apply spend hook callback state update — record burned nullifiers and increment total_redeemed
fn apply_spend_hook_callback(cid: ContractId, update: SpendHookCallbackUpdateV1) -> ContractResult {
    let nullifiers_db = wasm::db::db_lookup(cid, STABLECOIN_CONTRACT_POSITION_NULLIFIERS_TREE)?;

    for n in &update.nullifiers {
        wasm::db::db_mark_spent(nullifiers_db, &n.to_bytes())?;
    }

    // Write pre-computed total_redeemed (computed in process_spend_hook exec phase)
    let config_db = wasm::db::db_lookup(cid, "config")?;
    wasm::db::db_set(config_db, STABLECOIN_CONTRACT_TOTAL_REDEEMED, &update.new_total_redeemed.to_le_bytes())?;

    msg!("[stablecoin::apply_spend_hook] Recorded {} nullifiers from PN callback, total_redeemed={}",
        update.nullifiers.len(), update.new_total_redeemed);
    Ok(())
}

/// Process add collateral instruction
fn process_add_collateral_instruction(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params = DepositCollateralParams::decode(&self_.data[1..])?;

    msg!(
        "[stablecoin::process_instruction] AddCollateral: commitment={:?}, amount={}",
        params.deposit_commitment,
        params.collateral_amount
    );

    // Validate child call is promissory_note::transfer_v1 (0x04) for collateral deposit
    let this_call = &calls[call_idx];
    if this_call.children_indexes.len() != 1 {
        msg!("[AddCollateralV1] Error: Expected 1 child call (promissory_note::transfer_v1), got {}",
            this_call.children_indexes.len());
        return Err(StablecoinError::InvalidChildrenIndexes.into())
    }
    let child_idx = this_call.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!("[AddCollateralV1] Error: Expected promissory_note::transfer_v1 (0x04), got 0x{:02x}",
            child_call.data[0]);
        return Err(StablecoinError::InvalidChildCall.into())
    }

    // Validate child call targets promissory_note
    let info_db = wasm::db::db_lookup(cid, STABLECOIN_CONTRACT_INFO_TREE)?;
    let promissory_note_bytes = wasm::db::db_get(info_db, STABLECOIN_CONTRACT_PROMISSORY_NOTE_CONTRACT_ID)?
        .ok_or(StablecoinError::InvalidChildCall)?;
    let promissory_note_cid: ContractId = deserialize(&promissory_note_bytes)?;
    validate_child_contract_id(&child_call.contract_id, &promissory_note_cid)?;

    // Validate child transfer amount using value_commit comparison
    let value_blind = poseidon_hash([
        pallas::Base::from(params.collateral_amount),
        params.deposit_commitment.inner(),
    ]);
    if let Err(e) = validate_child_value_commit(
        &child_call.data, params.collateral_amount, value_blind,
    ) {
        msg!("[AddCollateralV1] Error: Child transfer value mismatch: {:?}", e);
        return Err(StablecoinError::InvalidChildCall.into())
    }

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

    Ok(update.encode())
}

/// Apply add collateral update
fn apply_add_collateral_update(cid: ContractId, update: AddCollateralUpdateV1) -> ContractResult {
    let positions_db = wasm::db::db_lookup(cid, STABLECOIN_CONTRACT_POSITIONS_TREE)?;
    let collateral_db = wasm::db::db_lookup(cid, STABLECOIN_CONTRACT_COLLATERAL_TREE)?;

    // Insert position into positions tree
    wasm::db::db_set(positions_db, &update.position_commitment.to_bytes(), &[1])?;
    wasm::db::db_set(collateral_db, &update.position_commitment.to_bytes(), &[1])?;

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
) -> Result<Vec<u8>, ContractError> {
    let this_call = &calls[call_idx];

    // Validate children_indexes for token payout
    if this_call.children_indexes.len() != 1 {
        msg!("[stablecoin::RemoveCollateral] Error: Expected 1 child call (promissory_note::transfer_v1), got {}",
            this_call.children_indexes.len());
        return Err(StablecoinError::InvalidChildrenIndexes.into())
    }
    let child_idx = this_call.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!("[stablecoin::RemoveCollateral] Error: Expected promissory_note::transfer_v1 (0x04), got 0x{:02x}",
            child_call.data[0]);
        return Err(StablecoinError::InvalidChildCall.into())
    }

    // Validate child call targets promissory_note
    let info_db = wasm::db::db_lookup(cid, STABLECOIN_CONTRACT_INFO_TREE)?;
    let promissory_note_bytes = wasm::db::db_get(info_db, STABLECOIN_CONTRACT_PROMISSORY_NOTE_CONTRACT_ID)?
        .ok_or(StablecoinError::InvalidChildCall)?;
    let promissory_note_cid: ContractId = deserialize(&promissory_note_bytes)?;
    validate_child_contract_id(&child_call.contract_id, &promissory_note_cid)?;

    let self_ = &calls[call_idx].data;
    let params = WithdrawCollateralParams::decode(&self_.data[1..])?;

    // Validate child transfer amount using value_commit comparison
    let value_blind = poseidon_hash([
        pallas::Base::from(params.withdraw_amount),
        params.withdrawal_nullifier.inner(),
    ]);
    validate_child_value_commit(&child_call.data, params.withdraw_amount, value_blind)?;

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

    Ok(update.encode())
}

/// Apply remove collateral update
fn apply_remove_collateral_update(
    cid: ContractId,
    update: RemoveCollateralUpdateV1,
) -> ContractResult {
    let nullifiers_db = wasm::db::db_lookup(cid, STABLECOIN_CONTRACT_POSITION_NULLIFIERS_TREE)?;
    let collateral_db = wasm::db::db_lookup(cid, STABLECOIN_CONTRACT_COLLATERAL_TREE)?;

    // Insert nullifier to prevent double-withdrawal
    wasm::db::db_mark_spent(nullifiers_db, &update.position_nullifier.to_bytes())?;

    // Record the new position commitment in the collateral tree (non-empty
    // marker — empty is invisible to db_contains_key, per §9.1)
    wasm::db::db_set(collateral_db, &update.new_commitment.to_bytes(), &[1])?;

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
) -> Result<Vec<u8>, ContractError> {
    let this_call = &calls[call_idx];

    // Validate children_indexes for token payout
    if this_call.children_indexes.len() != 1 {
        msg!("[stablecoin::MintStable] Error: Expected 1 child call (promissory_note::transfer_v1), got {}",
            this_call.children_indexes.len());
        return Err(StablecoinError::InvalidChildrenIndexes.into())
    }
    let child_idx = this_call.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!("[stablecoin::MintStable] Error: Expected promissory_note::transfer_v1 (0x04), got 0x{:02x}",
            child_call.data[0]);
        return Err(StablecoinError::InvalidChildCall.into())
    }

    // Validate child call targets promissory_note
    let info_db = wasm::db::db_lookup(cid, STABLECOIN_CONTRACT_INFO_TREE)?;
    let promissory_note_bytes = wasm::db::db_get(info_db, STABLECOIN_CONTRACT_PROMISSORY_NOTE_CONTRACT_ID)?
        .ok_or(StablecoinError::InvalidChildCall)?;
    let promissory_note_cid: ContractId = deserialize(&promissory_note_bytes)?;
    validate_child_contract_id(&child_call.contract_id, &promissory_note_cid)?;

    let self_ = &calls[call_idx].data;
    let params = MintStableParams::decode(&self_.data[1..])?;

    // Validate child transfer amount using value_commit comparison
    let value_blind = poseidon_hash([
        pallas::Base::from(params.mint_amount),
        params.mint_commitment.inner(),
    ]);
    validate_child_value_commit(&child_call.data, params.mint_amount, value_blind)?;

    // Validate output coins have spend_hook set to this contract.
    // Ensures the stablecoin receives burn callbacks when tokens are burned,
    // maintaining balance sheet integrity. The ZK circuit constrains
    // coin_spend_hook as a public input — this is defense-in-depth.
    let expected_spend_hook = cid.inner();
    let transfer_params = TransferParamsV1::decode(&child_call.data[1..])?;
    for output in &transfer_params.outputs {
        if output.spend_hook.inner() != expected_spend_hook {
            msg!("[stablecoin::MintStable] Error: Output coin spend_hook does not match stablecoin contract ID");
            return Err(StablecoinError::InvalidChildCall.into())
        }
    }

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

    Ok(update.encode())
}

/// Apply mint stablecoin update
fn apply_mint_stable_update(cid: ContractId, update: MintStableUpdateV1) -> ContractResult {
    let stablecoin_db = wasm::db::db_lookup(cid, STABLECOIN_CONTRACT_STABLECOIN_TREE)?;
    let positions_db = wasm::db::db_lookup(cid, STABLECOIN_CONTRACT_POSITIONS_TREE)?;

    // Insert mint commitment
    wasm::db::db_set(stablecoin_db, &update.position_commitment.to_bytes(), &[1])?;
    wasm::db::db_set(positions_db, &update.position_commitment.to_bytes(), &[1])?;

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
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params = RepayStableParams::decode(&self_.data[1..])?;

    msg!(
        "[stablecoin::process_instruction] RepayStable: amount={}",
        params.repay_amount
    );

    // Validate child call is promissory_note::transfer_v1 (0x04) for stablecoin repayment
    let this_call = &calls[call_idx];
    if this_call.children_indexes.len() != 1 {
        msg!("[RepayStableV1] Error: Expected 1 child call (promissory_note::transfer_v1), got {}",
            this_call.children_indexes.len());
        return Err(StablecoinError::InvalidChildrenIndexes.into())
    }
    let child_idx = this_call.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!("[RepayStableV1] Error: Expected promissory_note::transfer_v1 (0x04), got 0x{:02x}",
            child_call.data[0]);
        return Err(StablecoinError::InvalidChildCall.into())
    }

    // Validate child call targets promissory_note
    let info_db = wasm::db::db_lookup(cid, STABLECOIN_CONTRACT_INFO_TREE)?;
    let promissory_note_bytes = wasm::db::db_get(info_db, STABLECOIN_CONTRACT_PROMISSORY_NOTE_CONTRACT_ID)?
        .ok_or(StablecoinError::InvalidChildCall)?;
    let promissory_note_cid: ContractId = deserialize(&promissory_note_bytes)?;
    validate_child_contract_id(&child_call.contract_id, &promissory_note_cid)?;

    // Validate child transfer amount using value_commit comparison
    let value_blind = poseidon_hash([
        pallas::Base::from(params.repay_amount),
        params.repay_commitment.inner(),
    ]);
    if let Err(e) = validate_child_value_commit(
        &child_call.data, params.repay_amount, value_blind,
    ) {
        msg!("[RepayStableV1] Error: Child transfer value mismatch: {:?}", e);
        return Err(StablecoinError::InvalidChildCall.into())
    }

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

    Ok(update.encode())
}

/// Apply repay stablecoin update
fn apply_repay_stable_update(cid: ContractId, update: RepayStableUpdateV1) -> ContractResult {
    let nullifiers_db = wasm::db::db_lookup(cid, STABLECOIN_CONTRACT_POSITION_NULLIFIERS_TREE)?;

    // Insert nullifier to prevent double-repay
    wasm::db::db_mark_spent(nullifiers_db, &update.position_nullifier.to_bytes())?;

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
) -> Result<Vec<u8>, ContractError> {
    let this_call = &calls[call_idx];

    // Validate children_indexes for collateral payout to liquidator
    if this_call.children_indexes.len() != 1 {
        msg!("[stablecoin::Liquidate] Error: Expected 1 child call (promissory_note::transfer_v1), got {}",
            this_call.children_indexes.len());
        return Err(StablecoinError::InvalidChildrenIndexes.into())
    }
    let child_idx = this_call.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!("[stablecoin::Liquidate] Error: Expected promissory_note::transfer_v1 (0x04), got 0x{:02x}",
            child_call.data[0]);
        return Err(StablecoinError::InvalidChildCall.into())
    }

    // Validate child call targets promissory_note
    let info_db = wasm::db::db_lookup(cid, STABLECOIN_CONTRACT_INFO_TREE)?;
    let promissory_note_bytes = wasm::db::db_get(info_db, STABLECOIN_CONTRACT_PROMISSORY_NOTE_CONTRACT_ID)?
        .ok_or(StablecoinError::InvalidChildCall)?;
    let promissory_note_cid: ContractId = deserialize(&promissory_note_bytes)?;
    validate_child_contract_id(&child_call.contract_id, &promissory_note_cid)?;

    let self_ = &calls[call_idx].data;
    let params = LiquidateParams::decode(&self_.data[1..])?;

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

    // HAZOP CRIT-2 defense: The ZK circuit (liquidate_v1.zk) computes debt_value and
    // collateral_value but does NOT actually enforce undercollateralization in-circuit.
    // This entrypoint check IS the defense — it reads on-chain totals, computes the
    // ratio from config DB state, and enforces the liquidation threshold.
    // The circuit gap means a compromised entrypoint could bypass liquidation checks,
    // but the entrypoint IS the contract execution environment — if it's compromised,
    // all bets are off regardless of circuit constraints.

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

    // Validate child transfer amount using value_commit comparison
    let value_blind = poseidon_hash([
        pallas::Base::from(collateral_seized),
        poseidon_hash([pallas::Base::from(params.debt_to_cover), pallas::Base::from(params.total_collateral)]),
    ]);
    validate_child_value_commit(&child_call.data, collateral_seized, value_blind)?;

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

    Ok(update.encode())
}

/// Apply liquidate update
fn apply_liquidate_update(cid: ContractId, update: LiquidateUpdateV1) -> ContractResult {
    let liquidations_db = wasm::db::db_lookup(cid, STABLECOIN_CONTRACT_LIQUIDATIONS_TREE)?;

    // Record liquidation
    wasm::db::db_set(liquidations_db, &update.debt_covered.to_le_bytes(), &[1])?;

    msg!(
        "[stablecoin::process_update] Pool liquidated: debt_covered={}, collateral_seized={}, penalty={}",
        update.debt_covered,
        update.collateral_seized,
        update.penalty
    );
    Ok(())
}

/// Process governance report instruction — verifies on-chain state matches reported values
///
/// Reads actual total_debt, total_collateral, and total_redeemed from the config DB
/// and verifies the reporter's params match on-chain reality before accepting the report.
/// Computes outstanding = total_debt - total_redeemed and enforces
/// total_collateral >= outstanding (no fractional reserving).
fn process_governance_report_instruction(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params = GovernanceReportParams::decode(&self_.data[1..])?;

    // Read on-chain config DB values
    let config_db = wasm::db::db_lookup(cid, "config")?;
    let total_debt_bytes = wasm::db::db_get(config_db, CDP_TOTAL_DEBT_KEY)?
        .ok_or_else(|| ContractError::IoError("Total debt not found".to_string()))?;
    let on_chain_debt = u64::from_le_bytes(
        total_debt_bytes.as_slice().try_into()
            .map_err(|_| ContractError::IoError("Failed to read total debt".to_string()))?,
    );

    let total_collateral_bytes = wasm::db::db_get(config_db, CDP_TOTAL_COLLATERAL_KEY)?
        .ok_or_else(|| ContractError::IoError("Total collateral not found".to_string()))?;
    let on_chain_collateral = u64::from_le_bytes(
        total_collateral_bytes.as_slice().try_into()
            .map_err(|_| ContractError::IoError("Failed to read total collateral".to_string()))?,
    );

    let total_redeemed_bytes = wasm::db::db_get(config_db, STABLECOIN_CONTRACT_TOTAL_REDEEMED)?
        .ok_or_else(|| ContractError::IoError("Total redeemed not found".to_string()))?;
    let on_chain_redeemed = u64::from_le_bytes(
        total_redeemed_bytes.as_slice().try_into()
            .map_err(|_| ContractError::IoError("Failed to read total redeemed".to_string()))?,
    );

    // Verify reported values match on-chain state
    if params.total_collateral != on_chain_collateral {
        msg!("[stablecoin::process_instruction] GovernanceReport: collateral mismatch — reported={} on_chain={}",
            params.total_collateral, on_chain_collateral);
        return Err(StablecoinError::ConfigError("Reported collateral does not match on-chain state".to_string()).into())
    }

    if params.total_debt != on_chain_debt {
        msg!("[stablecoin::process_instruction] GovernanceReport: debt mismatch — reported={} on_chain={}",
            params.total_debt, on_chain_debt);
        return Err(StablecoinError::ConfigError("Reported debt does not match on-chain state".to_string()).into())
    }

    if params.total_redeemed != on_chain_redeemed {
        msg!("[stablecoin::process_instruction] GovernanceReport: redeemed mismatch — reported={} on_chain={}",
            params.total_redeemed, on_chain_redeemed);
        return Err(StablecoinError::ConfigError("Reported redeemed does not match on-chain state".to_string()).into())
    }

    // Verify interest_accrued against on-chain state.
    // HAZOP CRIT-1: The ZK circuit does not constrain interest_accrued as a public input.
    // This entrypoint check is the defense-in-depth: the reported value MUST match
    // the on-chain accumulated interest tracked in the config DB.
    let accrue_db_key = b"last_interest_accrued";
    if let Some(stored_interest_bytes) = wasm::db::db_get(config_db, accrue_db_key)? {
        let stored_interest = u64::from_le_bytes(
            stored_interest_bytes.as_slice().try_into()
                .map_err(|_| ContractError::IoError("Failed to read stored interest".to_string()))?,
        );
        if params.interest_accrued != stored_interest {
            msg!("[stablecoin::process_instruction] GovernanceReport: interest_accrued mismatch — reported={} stored={}",
                params.interest_accrued, stored_interest);
            return Err(StablecoinError::ConfigError("Reported interest_accrued does not match on-chain state".to_string()).into())
        }
    }

    // Compute outstanding circulation
    let outstanding = on_chain_debt.saturating_sub(on_chain_redeemed);

    if params.outstanding != outstanding {
        msg!("[stablecoin::process_instruction] GovernanceReport: outstanding mismatch — reported={} computed={}",
            params.outstanding, outstanding);
        return Err(StablecoinError::ConfigError("Reported outstanding does not match computed value".to_string()).into())
    }

    // Enforce no fractional reserving: total_collateral >= outstanding
    if on_chain_collateral < outstanding {
        msg!("[stablecoin::process_instruction] GovernanceReport: FRACTIONAL RESERVE DETECTED — collateral={} < outstanding={}",
            on_chain_collateral, outstanding);
        return Err(StablecoinError::InsufficientCollateral.into())
    }

    msg!(
        "[stablecoin::process_instruction] GovernanceReport: token={:?}, collateral={}, debt={}, redeemed={}, outstanding={}, ratio={}",
        params.token_id, on_chain_collateral, on_chain_debt, on_chain_redeemed, outstanding, params.collateral_ratio_bps
    );

    let update = GovernanceReportUpdateV1 {
        token_id: params.token_id,
        total_collateral: on_chain_collateral,
        total_debt: on_chain_debt,
        total_redeemed: on_chain_redeemed,
        outstanding,
        collateral_ratio_bps: params.collateral_ratio_bps,
        interest_accrued: params.interest_accrued,
        report_block: 0, // populated by apply phase
        reporter_pub: params.reporter_pub,
    };

    Ok(update.encode())
}

/// Apply governance report update — persist report on-chain for public audit
fn apply_governance_report_update(cid: ContractId, update: GovernanceReportUpdateV1) -> ContractResult {
    let reports_db = wasm::db::db_lookup(cid, STABLECOIN_CONTRACT_GOVERNANCE_REPORTS_TREE)?;

    // Derive a unique key for this report: poseidon_hash(token_id, outstanding, report serialized)
    let report_key = poseidon_hash([
        update.token_id,
        pallas::Base::from(update.outstanding),
        pallas::Base::from(update.total_collateral),
        pallas::Base::from(update.collateral_ratio_bps),
    ]);

    let report_bytes = update.encode();
    wasm::db::db_set(reports_db, &report_key.to_repr(), &report_bytes)?;

    msg!(
        "[stablecoin::process_update] Governance report persisted: token={:?}, collateral={}, debt={}, redeemed={}, outstanding={}, ratio={}",
        update.token_id, update.total_collateral, update.total_debt,
        update.total_redeemed, update.outstanding, update.collateral_ratio_bps
    );
    Ok(())
}

/// Process accrue interest instruction
fn process_accrue_interest_instruction(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params = AccrueInterestParams::decode(&self_.data[1..])?;

    msg!(
        "[stablecoin::process_instruction] AccrueInterest: old_debt={}, new_debt={}, interest={}",
        params.old_total_debt,
        params.new_total_debt,
        params.interest_amount
    );

    // Verify old_total_debt matches on-chain state.
    // The ZK circuit constrains new = old + interest, but old_total_debt
    // must be validated against the stored CDP_TOTAL_DEBT_KEY to prevent
    // a prover from supplying a stale (lower) debt for interest computation.
    let config_db = wasm::db::db_lookup(cid, "config")?;
    let stored_debt_bytes = wasm::db::db_get(config_db, CDP_TOTAL_DEBT_KEY)?
        .ok_or_else(|| ContractError::IoError("Total debt not found".to_string()))?;
    let stored_debt = u64::from_le_bytes(
        stored_debt_bytes.as_slice().try_into().map_err(|_| ContractError::IoError("Failed to read total debt".to_string()))?,
    );
    if params.old_total_debt != stored_debt {
        msg!("[stablecoin::process_instruction] ERROR: old_total_debt {} does not match on-chain {}",
             params.old_total_debt, stored_debt);
        return Err(StablecoinError::CommitmentMismatch.into())
    }

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

    // Update accumulated fees (reuse config_db from old_total_debt check above)
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
        accumulator_pub: params.accumulator_pub,
    };

    Ok(update.encode())
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

/// Process redeem stablecoin instruction
///
/// Calls PN::RedeemV1 (0x01) as a child call — the first application-layer
/// consumer of RedeemV1. Burns stablecoins and returns proportional collateral.
fn process_redeem_stable_instruction(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params = RedeemStableParamsV1::decode(&self_.data[1..])?;

    msg!(
        "[stablecoin::process_instruction] RedeemStable: amount={}, total_debt={}",
        params.redeem_amount,
        params.total_debt
    );

    // Validate child call is promissory_note::redeem_v1 (0x01)
    let this_call = &calls[call_idx];
    if this_call.children_indexes.len() != 1 {
        msg!("[RedeemStableV1] Error: Expected 1 child call (promissory_note::redeem_v1), got {}",
            this_call.children_indexes.len());
        return Err(StablecoinError::InvalidChildrenIndexes.into())
    }
    let child_idx = this_call.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x01 {
        msg!("[RedeemStableV1] Error: Expected promissory_note::redeem_v1 (0x01), got 0x{:02x}",
            child_call.data[0]);
        return Err(StablecoinError::InvalidChildCall.into())
    }

    // Validate child call targets promissory_note
    let info_db = wasm::db::db_lookup(cid, STABLECOIN_CONTRACT_INFO_TREE)?;
    let promissory_note_bytes = wasm::db::db_get(info_db, STABLECOIN_CONTRACT_PROMISSORY_NOTE_CONTRACT_ID)?
        .ok_or(StablecoinError::InvalidChildCall)?;
    let promissory_note_cid: ContractId = deserialize(&promissory_note_bytes)?;
    validate_child_contract_id(&child_call.contract_id, &promissory_note_cid)?;

    // Validate the redeem child call and get the receipt coin for inspection.
    // The ZK circuit constrains coin_value = 0 as a public input, so we trust
    // the host's proof verification for the zero-value property.
    let (_receipt_value_commit, _receipt_token_commit) =
        validate_child_redeem_v1(&child_call.data)?;

    // Validate receipt coin's spend_hook is bound to this contract.
    // Receipt coins must be non-transferable — only the issuer can interact
    // with them. The ZK circuit exposes coin_spend_hook as a public input;
    // this is defense-in-depth on the contract side.
    let expected_spend_hook = cid.inner();
    let redeem_params = RedeemParamsV1::decode(&child_call.data[1..])?;
    if redeem_params.output.spend_hook.inner() != expected_spend_hook {
        msg!("[RedeemStableV1] Error: Receipt coin spend_hook does not match stablecoin contract ID");
        return Err(StablecoinError::InvalidChildCall.into())
    }

    // Get current total debt and collateral
    let config_db = wasm::db::db_lookup(cid, "config")?;
    let total_debt_bytes = wasm::db::db_get(config_db, CDP_TOTAL_DEBT_KEY)?
        .ok_or_else(|| ContractError::IoError("Total debt not found".to_string()))?;
    let total_debt = u64::from_le_bytes(
        total_debt_bytes.as_slice().try_into()
            .map_err(|_| ContractError::IoError("Failed to read total debt".to_string()))?,
    );
    let total_collateral_bytes = wasm::db::db_get(config_db, CDP_TOTAL_COLLATERAL_KEY)?
        .ok_or_else(|| ContractError::IoError("Total collateral not found".to_string()))?;
    let total_collateral = u64::from_le_bytes(
        total_collateral_bytes.as_slice().try_into()
            .map_err(|_| ContractError::IoError("Failed to read total collateral".to_string()))?,
    );

    // Get current total redeemed
    let total_redeemed_bytes = wasm::db::db_get(config_db, STABLECOIN_CONTRACT_TOTAL_REDEEMED)?
        .ok_or_else(|| ContractError::IoError("Total redeemed not found".to_string()))?;
    let total_redeemed = u64::from_le_bytes(
        total_redeemed_bytes.as_slice().try_into()
            .map_err(|_| ContractError::IoError("Failed to read total redeemed".to_string()))?,
    );

    if params.redeem_amount > total_debt {
        msg!("[stablecoin::process_instruction] ERROR: Redeem exceeds debt");
        return Err(StablecoinError::RedeemExceedsDebt.into())
    }

    // Calculate proportional collateral return
    let collateral_return = if total_debt > 0 {
        (params.redeem_amount as u128 * total_collateral as u128 / total_debt as u128) as u64
    } else {
        0
    };

    let new_total_debt = total_debt.saturating_sub(params.redeem_amount);
    let new_total_collateral = total_collateral.saturating_sub(collateral_return);
    let new_total_redeemed = total_redeemed.saturating_add(params.redeem_amount);

    // Derive a unique nullifier for the redeem operation
    let redeem_nullifier = poseidon_hash([
        pallas::Base::from(params.redeem_amount),
        params.token_id,
        pallas::Base::from(total_debt),
    ]);

    let receipt_coin_bytes = serialize(child_call);

    // Store new totals in config for update phase
    wasm::db::db_set(config_db, CDP_TOTAL_DEBT_KEY, &new_total_debt.to_le_bytes())?;
    wasm::db::db_set(config_db, CDP_TOTAL_COLLATERAL_KEY, &new_total_collateral.to_le_bytes())?;
    wasm::db::db_set(config_db, STABLECOIN_CONTRACT_TOTAL_REDEEMED, &new_total_redeemed.to_le_bytes())?;

    let receipt_coin: [u8; 32] = receipt_coin_bytes.as_slice().try_into().map_err(|_| {
        msg!("[stablecoin::process_redeem_stable_instruction] Error: receipt_coin serialization produced {} bytes, expected 32",
            receipt_coin_bytes.len());
        ContractError::IoError("receipt_coin serialization size mismatch".to_string())
    })?;

    let update = RedeemStableUpdateV1 {
        redeem_nullifier,
        receipt_coin,
        redeem_amount: params.redeem_amount,
        new_total_debt,
        new_total_collateral,
        new_total_redeemed,
    };

    Ok(update.encode())
}

/// Apply redeem stablecoin update
fn apply_redeem_stable_update(cid: ContractId, update: RedeemStableUpdateV1) -> ContractResult {
    let nullifiers_db = wasm::db::db_lookup(cid, STABLECOIN_CONTRACT_POSITION_NULLIFIERS_TREE)?;

    // Check for duplicate redemption — don't trust instruction phase writes
    if wasm::db::db_contains_key(nullifiers_db, &update.redeem_nullifier.to_repr())? {
        msg!("[stablecoin::process_update] ERROR: Duplicate redeem nullifier");
        return Err(StablecoinError::DuplicateNullifier.into())
    }

    wasm::db::db_mark_spent(nullifiers_db, &update.redeem_nullifier.to_repr())?;

    msg!(
        "[stablecoin::process_update] Stablecoin redeemed: amount={}, new_debt={}, new_collateral={}, total_redeemed={}",
        update.redeem_amount,
        update.new_total_debt,
        update.new_total_collateral,
        update.new_total_redeemed
    );
    Ok(())
}

// ============================================================================
// UPDATE STRUCTS
// ============================================================================

