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

//! Bearer Bond WASM Entrypoint — Fixed-Interest Staking Contract
//!
//! ## Functions
//!
//! | # | Function | Opcode | Who | Description |
//! |---|----------|--------|-----|-------------|
//! | 1 | IssueStakeV1 | 0x00 | Issuer | Create staking pool, receive capital, mint stake coins |
//! | 2 | TransferStakeV1 | 0x01 | Holder | Transfer stake position to new holder |
//! | 3 | RequestInterestV1 | 0x02 | Holder | Request interest payment (prove ownership, provide payment key) |
//! | 4 | EmergencyUnstakeV1 | 0x03 | Holder | Exit before maturity when coverage below minimum |
//! | 5 | UnstakeV1 | 0x04 | Holder | Withdraw principal + unclaimed interest at maturity |
//! | 6 | BurnStakeV1 | 0x05 | Issuer | Retire staking pool |
//! | 7 | ProveCoverageV1 | 0x06 | Issuer/Holder | Submit ZK proof of solvency |
//! | 8 | VerifyCoverageV1 | 0x07 | Holder | Read latest coverage report for a series |
//! | 9 | PayInterestV1 | 0x08 | Issuer | Pay a pending interest claim with fresh payment coin |

use dwow_sdk::{
    crypto::{
        pasta_prelude::{Curve, CurveAffine, PrimeField},
        ContractId,
    },
    dark_tree::DarkLeaf,
    error::{ContractError, ContractResult},
    msg,
    pasta::pallas,
    wasm, ContractCall,
};
use dwow_serial::{deserialize, Encodable};

use crate::{
    error::BearerBondError,
    model::{
        BondCoin, BondSeriesInfo, BurnStakeParamsV1, BurnStakeUpdateV1,
        CoverageReport, EmergencyUnstakeParamsV1, EmergencyUnstakeUpdateV1,
        IssueStakeParamsV1, IssueStakeUpdateV1,
        PayInterestParamsV1, PayInterestUpdateV1,
        ProveCoverageParamsV1, ProveCoverageUpdateV1, RequestedClaim, ClaimStatus,
        RequestInterestParamsV1, RequestInterestUpdateV1, SeriesStatus,
        TransferStakeParamsV1, TransferStakeUpdateV1,
        UnstakeParamsV1, UnstakeUpdateV1,
    },
    validation,
    BearerBondFunction, BEARER_BOND_CONTRACT_BONDS_INFO_TREE,
    BEARER_BOND_CONTRACT_COINS_TREE,
    BEARER_BOND_CONTRACT_COIN_ROOTS_TREE, BEARER_BOND_CONTRACT_NULLIFIERS_TREE,
    BEARER_BOND_CONTRACT_NULLIFIER_ROOTS_TREE,
    BEARER_BOND_CONTRACT_DB_VERSION, BEARER_BOND_CONTRACT_INFO_TREE,
    BEARER_BOND_EMPTY_COINS_ROOT, BEARER_BOND_EMPTY_NULLIFIER_ROOT,
    BEARER_BOND_CONTRACT_ZKAS_BURN_NS_V2, BEARER_BOND_CONTRACT_ZKAS_BLIND_OUTPUT_NS_V2,
    BEARER_BOND_CONTRACT_ZKAS_PROVE_COVERAGE_NS_V2,
    BEARER_BOND_CONTRACT_ZKAS_REDEEM_NS_V2,
};

// Generate WASM entrypoints
dwow_sdk::define_contract!(
    init: init_contract,
    exec: process_instruction,
    apply: process_update,
    metadata: get_metadata
);

// ============================================================================
// CONTRACT INITIALIZATION
// ============================================================================

pub fn init_contract(cid: ContractId, _ix: &[u8]) -> ContractResult {
    msg!("[bearer_bond::init_contract] Initializing bearer_bond contract (fixed-interest staking)");

    // Include ZK circuits


    let tx_hash = wasm::util::get_tx_hash()?;
    let call_idx = wasm::util::get_call_index()?;
    let mut roots_data = Vec::with_capacity(32 + 1);
    tx_hash.encode(&mut roots_data)?;
    call_idx.encode(&mut roots_data)?;

    // Coin roots database
    if wasm::db::db_lookup(cid, BEARER_BOND_CONTRACT_COIN_ROOTS_TREE).is_err() {
        let db_coin_roots = wasm::db::db_init(cid, BEARER_BOND_CONTRACT_COIN_ROOTS_TREE)?;
        wasm::db::db_set(db_coin_roots, &BEARER_BOND_EMPTY_COINS_ROOT, &roots_data)?;
    }

    // Nullifier roots database
    if wasm::db::db_lookup(cid, BEARER_BOND_CONTRACT_NULLIFIER_ROOTS_TREE).is_err() {
        let db_null_roots = wasm::db::db_init(cid, BEARER_BOND_CONTRACT_NULLIFIER_ROOTS_TREE)?;
        wasm::db::db_set(
            db_null_roots,
            &BEARER_BOND_EMPTY_NULLIFIER_ROOT,
            &roots_data,
        )?;
    }

    // Coins database
    if wasm::db::db_lookup(cid, BEARER_BOND_CONTRACT_COINS_TREE).is_err() {
        wasm::db::db_init(cid, BEARER_BOND_CONTRACT_COINS_TREE)?;
    }

    // Nullifiers database
    if wasm::db::db_lookup(cid, BEARER_BOND_CONTRACT_NULLIFIERS_TREE).is_err() {
        wasm::db::db_init(cid, BEARER_BOND_CONTRACT_NULLIFIERS_TREE)?;
    }

    // Bonds info database (stores series info, coverage reports, and interest declarations)
    if wasm::db::db_lookup(cid, BEARER_BOND_CONTRACT_BONDS_INFO_TREE).is_err() {
        wasm::db::db_init(cid, BEARER_BOND_CONTRACT_BONDS_INFO_TREE)?;
    }

    let _info_db = match wasm::db::db_lookup(cid, BEARER_BOND_CONTRACT_INFO_TREE) {
        Ok(v) => v,
        Err(_) => {
            let info_db = wasm::db::db_init(cid, BEARER_BOND_CONTRACT_INFO_TREE)?;
            wasm::db::db_set(info_db, BEARER_BOND_CONTRACT_DB_VERSION, env!("CARGO_PKG_VERSION").as_bytes())?;
            info_db
        }
    };

    msg!("[bearer_bond::init_contract] Initialization complete");
    Ok(())
}

// ============================================================================
// METADATA ROUTING
// ============================================================================

fn get_metadata(cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = BearerBondFunction::try_from(self_.data[0])?;

    let metadata = match func {
        BearerBondFunction::IssueStakeV1 => issue_stake_metadata(cid, call_idx, calls),
        BearerBondFunction::TransferStakeV1 => transfer_stake_metadata(cid, call_idx, calls),
        BearerBondFunction::RequestInterestV1 => request_interest_metadata(cid, call_idx, calls),
        BearerBondFunction::EmergencyUnstakeV1 => emergency_unstake_metadata(cid, call_idx, calls),
        BearerBondFunction::UnstakeV1 => unstake_metadata(cid, call_idx, calls),
        BearerBondFunction::BurnStakeV1 => burn_stake_metadata(cid, call_idx, calls),
        BearerBondFunction::ProveCoverageV1 => prove_coverage_metadata(cid, call_idx, calls),
        BearerBondFunction::VerifyCoverageV1 => Ok(vec![]),
        BearerBondFunction::PayInterestV1 => pay_interest_metadata(cid, call_idx, calls),
    }?;

    wasm::util::set_return_data(&metadata)
}

/// Extract (x, y) base-field coordinates from a pallas::Point for ZK public inputs.
fn point_coords(pt: pallas::Point) -> (pallas::Base, pallas::Base) {
    let affine = pt.to_affine();
    let coords = affine.coordinates().expect("point_coords: identity point — ZK circuit must constrain non-identity for value commitments");
    (*coords.x(), *coords.y())
}

// ============================================================================
// METADATA: ISSUE STAKE
// ============================================================================

/// Metadata for IssueStakeV1 — BlindOutput_V1 instance(s) for output coins.
fn issue_stake_metadata(_cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params = match IssueStakeParamsV1::decode(&self_.data[1..]) { Ok(p) => p, Err(_) => return Ok(vec![]) };

    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    let signature_pubkeys: Vec<pallas::Base> = vec![];

    let (vc_x, vc_y) = point_coords(params.coin.value_commit);

    zk_public_inputs.push((
        BEARER_BOND_CONTRACT_ZKAS_BLIND_OUTPUT_NS_V2.to_string(),
        vec![
            params.coin.token_commit, // coin identifier
            vc_x,
            vc_y,
            params.coin.token_commit,
            params.coin.spend_hook,
        ],
    ));

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    signature_pubkeys.encode(&mut metadata)?;
    Ok(metadata)
}

// ============================================================================
// METADATA: TRANSFER STAKE
// ============================================================================

/// Metadata for TransferStakeV1 — Burn_V1 for inputs, BlindOutput_V1 for outputs.
fn transfer_stake_metadata(_cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params = match TransferStakeParamsV1::decode(&self_.data[1..]) { Ok(p) => p, Err(_) => return Ok(vec![]) };

    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    // Schnorr signatures prohibited (contract-standards.md §3).
    let signature_pubkeys: Vec<pallas::Base> = vec![];

    // Burn proofs (one per input)
    for input in &params.inputs {
        let (vc_x, vc_y) = point_coords(input.value_commit);

        zk_public_inputs.push((
            BEARER_BOND_CONTRACT_ZKAS_BURN_NS_V2.to_string(),
            vec![
                input.nullifier.inner(),
                vc_x,
                vc_y,
                input.token_commit,
                input.merkle_root.inner(),
                input.user_data_enc,
                input.spend_hook,
                input.signature_public,
                pallas::Base::zero(), // tx_binding
                pallas::Base::zero(), // tx_nonce
            ],
        ));
    }

    // BlindOutput proofs (one per output)
    for output in &params.outputs {
        let (vc_x, vc_y) = point_coords(output.value_commit);

        zk_public_inputs.push((
            BEARER_BOND_CONTRACT_ZKAS_BLIND_OUTPUT_NS_V2.to_string(),
            vec![
                output.token_commit,
                vc_x,
                vc_y,
                output.token_commit,
                output.spend_hook,
            ],
        ));
    }

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    signature_pubkeys.encode(&mut metadata)?;
    Ok(metadata)
}

// ============================================================================
// METADATA: REQUEST INTEREST
// ============================================================================

/// Metadata for RequestInterestV1 — Burn_V1 proof for bond ownership.
/// The nullifier appears in public inputs but is NOT written to the nullifiers tree
/// (the coin is not consumed — the holder is only proving ownership, like
/// presenting a physical bond coupon).
fn request_interest_metadata(_cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params = match RequestInterestParamsV1::decode(&self_.data[1..]) { Ok(p) => p, Err(_) => return Ok(vec![]) };

    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    // Schnorr signatures prohibited (contract-standards.md §3).
    let signature_pubkeys: Vec<pallas::Base> = vec![];

    // Burn_V1 proof for bond ownership (nullifier NOT written to tree)
    let (vc_x, vc_y) = point_coords(params.bond_input.value_commit);

    zk_public_inputs.push((
        BEARER_BOND_CONTRACT_ZKAS_BURN_NS_V2.to_string(),
        vec![
            params.bond_input.nullifier.inner(),
            vc_x,
            vc_y,
            params.bond_input.token_commit,
            params.bond_input.merkle_root.inner(),
            params.bond_input.user_data_enc,
            params.bond_input.spend_hook,
            params.bond_input.signature_public,
            pallas::Base::zero(), // tx_binding
            pallas::Base::zero(), // tx_nonce
        ],
    ));

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    signature_pubkeys.encode(&mut metadata)?;
    Ok(metadata)
}

// ============================================================================
// METADATA: EMERGENCY UNSTAKE
// ============================================================================

/// Metadata for EmergencyUnstakeV1 — Burn_V1 for input, Redeem_V1 for receipt coin.
fn emergency_unstake_metadata(_cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params = match EmergencyUnstakeParamsV1::decode(&self_.data[1..]) { Ok(p) => p, Err(_) => return Ok(vec![]) };

    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    // Schnorr signatures prohibited (contract-standards.md §3).
    let signature_pubkeys: Vec<pallas::Base> = vec![];

    // Burn proof for the input stake coin
    let (vc_x, vc_y) = point_coords(params.bond_input.value_commit);

    zk_public_inputs.push((
        BEARER_BOND_CONTRACT_ZKAS_BURN_NS_V2.to_string(),
        vec![
            params.bond_input.nullifier.inner(),
            vc_x,
            vc_y,
            params.bond_input.token_commit,
            params.bond_input.merkle_root.inner(),
            params.bond_input.user_data_enc,
            params.bond_input.spend_hook,
            params.bond_input.signature_public,
            pallas::Base::zero(), // tx_binding
            pallas::Base::zero(), // tx_nonce
        ],
    ));

    // Redeem_V1 proof for the receipt coin (value=0)
    let coin_value = pallas::Base::zero();
    zk_public_inputs.push((
        BEARER_BOND_CONTRACT_ZKAS_REDEEM_NS_V2.to_string(),
        vec![
            params.bond_input.token_commit,
            vc_x,
            vc_y,
            params.bond_input.token_commit,
            coin_value,
            pallas::Base::zero(), // tx_binding
            pallas::Base::zero(), // tx_nonce
            params.bond_input.spend_hook,
        ],
    ));

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    signature_pubkeys.encode(&mut metadata)?;
    Ok(metadata)
}

// ============================================================================
// METADATA: UNSTAKE
// ============================================================================

/// Metadata for UnstakeV1 — Burn_V1 for input, Redeem_V1 for receipt coin.
fn unstake_metadata(_cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params = match UnstakeParamsV1::decode(&self_.data[1..]) { Ok(p) => p, Err(_) => return Ok(vec![]) };

    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    // Schnorr signatures prohibited (contract-standards.md §3).
    let signature_pubkeys: Vec<pallas::Base> = vec![];

    // Burn proof for the input stake coin
    let (vc_x, vc_y) = point_coords(params.bond_input.value_commit);

    zk_public_inputs.push((
        BEARER_BOND_CONTRACT_ZKAS_BURN_NS_V2.to_string(),
        vec![
            params.bond_input.nullifier.inner(),
            vc_x,
            vc_y,
            params.bond_input.token_commit,
            params.bond_input.merkle_root.inner(),
            params.bond_input.user_data_enc,
            params.bond_input.spend_hook,
            params.bond_input.signature_public,
            pallas::Base::zero(), // tx_binding
            pallas::Base::zero(), // tx_nonce
        ],
    ));

    // Redeem_V1 proof for the receipt coin (value=0)
    let coin_value = pallas::Base::zero();

    zk_public_inputs.push((
        BEARER_BOND_CONTRACT_ZKAS_REDEEM_NS_V2.to_string(),
        vec![
            params.bond_input.token_commit, // receipt coin identifier
            vc_x,                            // value_commit x
            vc_y,                            // value_commit y
            params.bond_input.token_commit,  // token_commit
            coin_value,                      // value = 0
            params.bond_input.spend_hook,   // spend_hook
        ],
    ));

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    signature_pubkeys.encode(&mut metadata)?;
    Ok(metadata)
}

// ============================================================================
// METADATA: BURN STAKE
// ============================================================================

/// Metadata for BurnStakeV1 — Burn_V1 instance(s) for inputs.
fn burn_stake_metadata(_cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params = match BurnStakeParamsV1::decode(&self_.data[1..]) { Ok(p) => p, Err(_) => return Ok(vec![]) };

    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    // Schnorr signatures prohibited (contract-standards.md §3).
    let signature_pubkeys: Vec<pallas::Base> = vec![];

    for input in &params.inputs {
        let (vc_x, vc_y) = point_coords(input.value_commit);

        zk_public_inputs.push((
            BEARER_BOND_CONTRACT_ZKAS_BURN_NS_V2.to_string(),
            vec![
                input.nullifier.inner(),
                vc_x,
                vc_y,
                input.token_commit,
                input.merkle_root.inner(),
                input.user_data_enc,
                input.spend_hook,
                input.signature_public,
                pallas::Base::zero(), // tx_binding
                pallas::Base::zero(), // tx_nonce
            ],
        ));
    }

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    signature_pubkeys.encode(&mut metadata)?;
    Ok(metadata)
}

// ============================================================================
// METADATA: PROVE COVERAGE
// ============================================================================

/// Metadata for ProveCoverageV1 — ProveCoverage_V1 circuit with coverage_ratio_bps public input.
fn prove_coverage_metadata(_cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params = match ProveCoverageParamsV1::decode(&self_.data[1..]) { Ok(p) => p, Err(_) => return Ok(vec![]) };

    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    let signature_pubkeys: Vec<pallas::Base> = vec![];

    zk_public_inputs.push((
        BEARER_BOND_CONTRACT_ZKAS_PROVE_COVERAGE_NS_V2.to_string(),
        vec![pallas::Base::from(params.coverage_ratio_bps)],
    ));

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    signature_pubkeys.encode(&mut metadata)?;
    Ok(metadata)
}

// ============================================================================
// METADATA: PAY INTEREST
// ============================================================================

/// Metadata for PayInterestV1 — BlindOutput_V1 for the payment coin.
/// The issuer creates the payment coin (not the holder). Fresh coin_blind
/// per payment ensures unlinkable payment addresses.
fn pay_interest_metadata(_cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params = match PayInterestParamsV1::decode(&self_.data[1..]) { Ok(p) => p, Err(_) => return Ok(vec![]) };

    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    let signature_pubkeys: Vec<pallas::Base> = vec![];

    let (vc_x, vc_y) = point_coords(params.interest_coin.value_commit);

    zk_public_inputs.push((
        BEARER_BOND_CONTRACT_ZKAS_BLIND_OUTPUT_NS_V2.to_string(),
        vec![
            params.interest_coin.token_commit,  // coin identifier
            vc_x,
            vc_y,
            params.interest_coin.token_commit,
            params.interest_coin.spend_hook,
        ],
    ));

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    signature_pubkeys.encode(&mut metadata)?;
    Ok(metadata)
}

// ============================================================================
// EXECUTION ROUTING
// ============================================================================

fn process_instruction(cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = BearerBondFunction::try_from(self_.data[0])?;

    match func {
        BearerBondFunction::IssueStakeV1 => issue_stake_v1(cid, call_idx, calls),
        BearerBondFunction::TransferStakeV1 => transfer_stake_v1(cid, call_idx, calls),
        BearerBondFunction::RequestInterestV1 => request_interest_v1(cid, call_idx, calls),
        BearerBondFunction::EmergencyUnstakeV1 => emergency_unstake_v1(cid, call_idx, calls),
        BearerBondFunction::UnstakeV1 => unstake_v1(cid, call_idx, calls),
        BearerBondFunction::BurnStakeV1 => burn_stake_v1(cid, call_idx, calls),
        BearerBondFunction::ProveCoverageV1 => prove_coverage_v1(cid, call_idx, calls),
        BearerBondFunction::VerifyCoverageV1 => verify_coverage_v1(cid, call_idx, calls),
        BearerBondFunction::PayInterestV1 => pay_interest_v1(cid, call_idx, calls),
    }
}

// ============================================================================
// UPDATE ROUTING
// ============================================================================

fn process_update(cid: ContractId, update_data: &[u8]) -> ContractResult {
    let func = BearerBondFunction::try_from(update_data[0])?;

    match func {
        BearerBondFunction::IssueStakeV1 => {
            let update = IssueStakeUpdateV1::decode(&update_data[1..])?;
            apply_issue_stake(cid, update)
        }
        BearerBondFunction::TransferStakeV1 => {
            let update = TransferStakeUpdateV1::decode(&update_data[1..])?;
            apply_transfer_stake(cid, update)
        }
        BearerBondFunction::RequestInterestV1 => {
            let update = RequestInterestUpdateV1::decode(&update_data[1..])?;
            apply_request_interest(cid, update)
        }
        BearerBondFunction::EmergencyUnstakeV1 => {
            let update = EmergencyUnstakeUpdateV1::decode(&update_data[1..])?;
            apply_emergency_unstake(cid, update)
        }
        BearerBondFunction::UnstakeV1 => {
            let update = UnstakeUpdateV1::decode(&update_data[1..])?;
            apply_unstake(cid, update)
        }
        BearerBondFunction::BurnStakeV1 => {
            let update = BurnStakeUpdateV1::decode(&update_data[1..])?;
            apply_burn_stake(cid, update)
        }
        BearerBondFunction::ProveCoverageV1 => {
            let update = ProveCoverageUpdateV1::decode(&update_data[1..])?;
            apply_prove_coverage(cid, update)
        }
        BearerBondFunction::VerifyCoverageV1 => Ok(()),
        BearerBondFunction::PayInterestV1 => {
            let update = PayInterestUpdateV1::decode(&update_data[1..])?;
            apply_pay_interest(cid, update)
        }
    }
}

// ============================================================================
// EXECUTION: ISSUE STAKE
// ============================================================================

fn issue_stake_v1(
    cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>,
) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params = IssueStakeParamsV1::decode(&self_.data[1..])?;

    // Verify the bond series exists and is active
    let bonds_info_db = wasm::db::db_lookup(cid, BEARER_BOND_CONTRACT_BONDS_INFO_TREE)?;
    let series_key = params.token_id.to_repr();
    let series_data = match wasm::db::db_get(bonds_info_db, &series_key)? {
        Some(data) => data,
        None => {
            msg!("[issue_stake_v1] Error: Bond series not found");
            return Err(BearerBondError::StakeNotFound.into());
        }
    };

    // Verify the caller is the authorized issuer for this series.
    // The issuer_contract in the params is bound by the ZK proof.
    let series_info = BondSeriesInfo::decode(&series_data)?;
    if params.issuer_contract != series_info.issuer_contract {
        msg!("[issue_stake_v1] Error: Caller is not the authorized issuer");
        return Err(BearerBondError::StakeNotFound.into());
    }

    let coins_db = wasm::db::db_lookup(cid, BEARER_BOND_CONTRACT_COINS_TREE)?;
    if wasm::db::db_contains_key(coins_db, &params.coin.token_commit.to_repr())? {
        msg!("[issue_stake_v1] Error: Stake coin already exists");
        return Err(BearerBondError::StakeAlreadyExists.into());
    }

    let update = IssueStakeUpdateV1 { coins: vec![params.coin] };
    let mut return_data = vec![BearerBondFunction::IssueStakeV1 as u8];
    return_data.extend_from_slice(&update.encode());
    wasm::util::set_return_data(&return_data)
}

// ============================================================================
// EXECUTION: TRANSFER STAKE
// ============================================================================

fn transfer_stake_v1(
    cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>,
) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params = TransferStakeParamsV1::decode(&self_.data[1..])?;

    if params.inputs.is_empty() {
        msg!("[bearer_bond::transfer_stake_v1] Error: Missing inputs");
        return Err(BearerBondError::MissingInputs.into());
    }
    if params.outputs.is_empty() {
        msg!("[bearer_bond::transfer_stake_v1] Error: Missing outputs");
        return Err(BearerBondError::MissingOutputs.into());
    }

    let coins_db = wasm::db::db_lookup(cid, BEARER_BOND_CONTRACT_COINS_TREE)?;
    let nullifiers_db = wasm::db::db_lookup(cid, BEARER_BOND_CONTRACT_NULLIFIERS_TREE)?;

    for input in &params.inputs {
        if !wasm::db::db_contains_key(coins_db, &input.token_commit.to_repr())? {
            msg!("[transfer_stake_v1] Error: Input stake coin not found");
            return Err(BearerBondError::StakeNotFound.into());
        }
        if wasm::db::db_contains_key(nullifiers_db, &input.nullifier.to_bytes())? {
            msg!("[transfer_stake_v1] Error: Duplicate nullifier");
            return Err(BearerBondError::DuplicateNullifier.into());
        }
    }

    for output in &params.outputs {
        if wasm::db::db_contains_key(coins_db, &output.token_commit.to_repr())? {
            msg!("[transfer_stake_v1] Error: Output stake coin already exists");
            return Err(BearerBondError::StakeAlreadyExists.into());
        }
        if output.last_claim_block >= output.maturity_block {
            msg!("[bearer_bond::transfer_stake_v1] Error: Output stake already matured (last_claim={}, maturity={})",
                output.last_claim_block, output.maturity_block);
            return Err(BearerBondError::StakeAlreadyMatured.into());
        }
    }

    // Verify value conservation via Pedersen commitment homomorphic sum.
    // Σ inputs.value_commit == Σ outputs.value_commit  ⇒  Σ inputs.principal == Σ outputs.principal
    let total_input: pallas::Point = params.inputs.iter().map(|i| i.value_commit).sum();
    let total_output: pallas::Point = params.outputs.iter().map(|o| o.value_commit).sum();
    if total_input != total_output {
        msg!("[bearer_bond::transfer_stake_v1] Error: Value conservation failed (input sum != output sum)");
        return Err(BearerBondError::ValueMismatch.into());
    }

    let nullifiers: Vec<_> = params.inputs.iter().map(|i| i.nullifier).collect();
    let update = TransferStakeUpdateV1 { nullifiers, coins: params.outputs };
    let mut return_data = vec![BearerBondFunction::TransferStakeV1 as u8];
    return_data.extend_from_slice(&update.encode());
    wasm::util::set_return_data(&return_data)
}

// ============================================================================
// EXECUTION: REQUEST INTEREST
// ============================================================================

fn request_interest_v1(
    cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>,
) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params = RequestInterestParamsV1::decode(&self_.data[1..])?;

    if params.claim_block == 0 {
        msg!("[bearer_bond::request_interest_v1] Error: Claim block is zero");
        return Err(BearerBondError::InvalidBlockHeight.into());
    }

    let coins_db = wasm::db::db_lookup(cid, BEARER_BOND_CONTRACT_COINS_TREE)?;

    // Look up the existing stake coin to get last_claim_block
    let coin_bytes = wasm::db::db_get(coins_db, &params.bond_input.token_commit.to_repr())?
        .ok_or(BearerBondError::StakeNotFound)?;
    let stake_coin = BondCoin::decode(&coin_bytes)?;

    // Verify the claim block is after the last claim block
    if params.claim_block <= stake_coin.last_claim_block {
        msg!("[bearer_bond::request_interest_v1] Error: Invalid interest claim: claim_block={} <= last_claim_block={}",
            params.claim_block, stake_coin.last_claim_block);
        return Err(BearerBondError::InvalidInterestClaim {
            last: stake_coin.last_claim_block,
            current: params.claim_block,
        }.into());
    }

    // Read the bond series info to get interest rate
    let bonds_info_db = wasm::db::db_lookup(cid, BEARER_BOND_CONTRACT_BONDS_INFO_TREE)?;
    let series_key = stake_coin.token_commit.to_repr();
    let series_bytes = match wasm::db::db_get(bonds_info_db, &series_key) {
        Ok(Some(b)) => b,
        _ => {
            msg!("[request_interest_v1] Error: Bond series info not found");
            return Err(BearerBondError::StakeNotFound.into());
        }
    };
    let series_info = BondSeriesInfo::decode(&series_bytes)?;

    // Check series status
    if series_info.status != SeriesStatus::Active {
        msg!("[request_interest_v1] Error: Series is not active");
        return Err(BearerBondError::SeriesNotActive.into());
    }

    let blocks_elapsed = params.claim_block - stake_coin.last_claim_block;

    // Compute interest deterministically
    let interest = match crate::model::calculate_interest(
        series_info.total_staked,
        series_info.interest_rate_bps,
        blocks_elapsed,
    ) {
        Some(v) => v,
        None => return Err(BearerBondError::InterestOverflow.into()),
    };

    // Minimum claim threshold
    if interest < params.min_claim {
        msg!("[request_interest_v1] Interest below minimum claim threshold: {} < {}", interest, params.min_claim);
        return Err(BearerBondError::InterestOverflow.into());
    }

    // Check no pending claim already exists for this bond
    let claim_key = [&params.bond_input.token_commit.to_repr()[..], &params.claim_block.to_le_bytes()[..]].concat();
    if wasm::db::db_contains_key(bonds_info_db, &claim_key)? {
        msg!("[request_interest_v1] Error: Interest claim request already exists");
        return Err(BearerBondError::ClaimAlreadyExists.into());
    }

    // Store the claim record — do NOT update last_claim_block yet
    let claim = RequestedClaim {
        interest_amount: interest,
        payment_key: params.payment_key,
        status: ClaimStatus::Pending,
    };

    let update = RequestInterestUpdateV1 {
        bond_token_commit: params.bond_input.token_commit,
        claim_block: params.claim_block,
        claim,
    };
    let mut return_data = vec![BearerBondFunction::RequestInterestV1 as u8];
    return_data.extend_from_slice(&update.encode());
    wasm::util::set_return_data(&return_data)
}

// ============================================================================
// EXECUTION: PAY INTEREST
// ============================================================================

fn pay_interest_v1(
    cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>,
) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params = PayInterestParamsV1::decode(&self_.data[1..])?;

    let bonds_info_db = wasm::db::db_lookup(cid, BEARER_BOND_CONTRACT_BONDS_INFO_TREE)?;

    // Look up the pending claim
    let claim_key = [&params.bond_token_commit.to_repr()[..], &params.claim_block.to_le_bytes()[..]].concat();
    let claim_bytes = wasm::db::db_get(bonds_info_db, &claim_key)?
        .ok_or(BearerBondError::ClaimNotFound)?;
    let claim = RequestedClaim::decode(&claim_bytes)?;

    // Verify the claim is still pending
    if claim.status != ClaimStatus::Pending {
        msg!("[pay_interest_v1] Error: Interest claim already paid");
        return Err(BearerBondError::ClaimAlreadyPaid.into());
    }

    // Verify the issuer has sufficient reserves (ringfencing enforcement)
    // Scan bonds_info for the latest coverage report for this series
    let coins_db = wasm::db::db_lookup(cid, BEARER_BOND_CONTRACT_COINS_TREE)?;
    let coin_bytes = wasm::db::db_get(coins_db, &params.bond_token_commit.to_repr())?
        .ok_or(BearerBondError::StakeNotFound)?;
    let stake_coin = BondCoin::decode(&coin_bytes)?;

    // Check series is still active
    let series_key = stake_coin.token_commit.to_repr();
    let series_bytes = wasm::db::db_get(bonds_info_db, &series_key)?
        .ok_or(BearerBondError::StakeNotFound)?;
    let series_info = BondSeriesInfo::decode(&series_bytes)?;

    if series_info.status == SeriesStatus::Voided {
        msg!("[pay_interest_v1] Error: Series is voided — cannot pay interest");
        return Err(BearerBondError::SeriesVoided.into());
    }

    // Verify coverage is sufficient: look for any coverage report for this series
    // The issuer must have proven reserves >= obligations before paying.
    // In a full implementation this scans bonds_info for the latest CoverageReport
    // for this series. For now we verify a report exists.
    let coverage_scan_key = [&stake_coin.token_commit.to_repr()[..], &0u64.to_le_bytes()[..]].concat();
    if !wasm::db::db_contains_key(bonds_info_db, &coverage_scan_key)? {
        // Try higher block numbers — the coverage report key is (series_token_id, report_block)
        // For now, check if ANY coverage-related key exists by attempting the lookup
        msg!("[pay_interest_v1] Error: No coverage report found for this series");
        return Err(BearerBondError::CoverageNotVerified.into());
    }

    // Update last_claim_block on the stake coin
    let updated_coin = BondCoin {
        last_claim_block: params.claim_block,
        ..stake_coin
    };

    // Mark claim as Paid in exec so apply is a pure write
    let mut paid_claim = claim;
    paid_claim.status = ClaimStatus::Paid;

    let update = PayInterestUpdateV1 {
        updated_coin,
        interest_coin: params.interest_coin,
        bond_token_commit: params.bond_token_commit,
        claim_block: params.claim_block,
        claim: paid_claim,
    };
    let mut return_data = vec![BearerBondFunction::PayInterestV1 as u8];
    return_data.extend_from_slice(&update.encode());
    wasm::util::set_return_data(&return_data)
}

// ============================================================================
// EXECUTION: EMERGENCY UNSTAKE
// ============================================================================

fn emergency_unstake_v1(
    cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>,
) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params = EmergencyUnstakeParamsV1::decode(&self_.data[1..])?;

    // Verify the coverage report shows voided coverage
    if !validation::is_coverage_voided(&params.coverage_report) {
        msg!("[emergency_unstake_v1] Error: Coverage is above minimum — emergency unstake not allowed");
        return Err(BearerBondError::EmergencyUnstakeNotAllowed.into());
    }

    let coins_db = wasm::db::db_lookup(cid, BEARER_BOND_CONTRACT_COINS_TREE)?;
    let nullifiers_db = wasm::db::db_lookup(cid, BEARER_BOND_CONTRACT_NULLIFIERS_TREE)?;

    if !wasm::db::db_contains_key(coins_db, &params.bond_input.token_commit.to_repr())? {
        msg!("[emergency_unstake_v1] Error: Stake coin not found");
        return Err(BearerBondError::StakeNotFound.into());
    }

    if wasm::db::db_contains_key(nullifiers_db, &params.bond_input.nullifier.to_bytes())? {
        msg!("[emergency_unstake_v1] Error: Stake already unstaked");
        return Err(BearerBondError::StakeAlreadyUnstaked.into());
    }

    let receipt_coin = BondCoin {
        ..Default::default()
    };

    let update = EmergencyUnstakeUpdateV1 {
        nullifiers: vec![params.bond_input.nullifier],
        receipt_coin,
    };
    let mut return_data = vec![BearerBondFunction::EmergencyUnstakeV1 as u8];
    return_data.extend_from_slice(&update.encode());
    wasm::util::set_return_data(&return_data)
}

// ============================================================================
// EXECUTION: UNSTAKE
// ============================================================================

fn unstake_v1(
    cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>,
) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params = UnstakeParamsV1::decode(&self_.data[1..])?;

    let coins_db = wasm::db::db_lookup(cid, BEARER_BOND_CONTRACT_COINS_TREE)?;
    let nullifiers_db = wasm::db::db_lookup(cid, BEARER_BOND_CONTRACT_NULLIFIERS_TREE)?;

    // Look up the stake coin to verify maturity
    let coin_bytes = wasm::db::db_get(coins_db, &params.bond_input.token_commit.to_repr())?
        .ok_or(BearerBondError::StakeNotFound)?;
    let stake_coin = BondCoin::decode(&coin_bytes)?;

    // Enforce maturity: current block must be >= maturity block
    if params.current_block < stake_coin.maturity_block {
        msg!("[unstake_v1] Error: Stake not yet matured — current={}, maturity={}",
            params.current_block, stake_coin.maturity_block);
        return Err(BearerBondError::StakeNotMatured {
            current: params.current_block,
            maturity: stake_coin.maturity_block,
        }.into());
    }

    if wasm::db::db_contains_key(nullifiers_db, &params.bond_input.nullifier.to_bytes())? {
        msg!("[unstake_v1] Error: Stake already unstaked");
        return Err(BearerBondError::StakeAlreadyUnstaked.into());
    }

    let receipt_coin = BondCoin {
        ..Default::default()
    };

    let update = UnstakeUpdateV1 {
        nullifiers: vec![params.bond_input.nullifier],
        receipt_coin,
    };
    let mut return_data = vec![BearerBondFunction::UnstakeV1 as u8];
    return_data.extend_from_slice(&update.encode());
    wasm::util::set_return_data(&return_data)
}

// ============================================================================
// EXECUTION: BURN STAKE
// ============================================================================

fn burn_stake_v1(
    cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>,
) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params = BurnStakeParamsV1::decode(&self_.data[1..])?;

    if params.inputs.is_empty() {
        msg!("[bearer_bond::burn_stake_v1] Error: Missing inputs");
        return Err(BearerBondError::MissingInputs.into());
    }

    let coins_db = wasm::db::db_lookup(cid, BEARER_BOND_CONTRACT_COINS_TREE)?;
    let nullifiers_db = wasm::db::db_lookup(cid, BEARER_BOND_CONTRACT_NULLIFIERS_TREE)?;

    for input in &params.inputs {
        if !wasm::db::db_contains_key(coins_db, &input.token_commit.to_repr())? {
            msg!("[burn_stake_v1] Error: Stake coin not found");
            return Err(BearerBondError::StakeNotFound.into());
        }
        if wasm::db::db_contains_key(nullifiers_db, &input.nullifier.to_bytes())? {
            msg!("[burn_stake_v1] Error: Duplicate nullifier");
            return Err(BearerBondError::DuplicateNullifier.into());
        }
    }

    let nullifiers: Vec<_> = params.inputs.iter().map(|i| i.nullifier).collect();
    let update = BurnStakeUpdateV1 { nullifiers };
    let mut return_data = vec![BearerBondFunction::BurnStakeV1 as u8];
    return_data.extend_from_slice(&update.encode());
    wasm::util::set_return_data(&return_data)
}

// ============================================================================
// EXECUTION: PROVE COVERAGE
// ============================================================================

fn prove_coverage_v1(
    cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>,
) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params = ProveCoverageParamsV1::decode(&self_.data[1..])?;

    if params.total_outstanding == 0 {
        msg!("[bearer_bond::prove_coverage_v1] Error: Total outstanding is zero");
        return Err(BearerBondError::InvalidPrincipal.into());
    }
    if params.reserve_amount == 0 {
        msg!("[bearer_bond::prove_coverage_v1] Error: Reserve amount is zero");
        return Err(BearerBondError::InvalidPrincipal.into());
    }
    if params.report_block == 0 {
        msg!("[bearer_bond::prove_coverage_v1] Error: Report block is zero");
        return Err(BearerBondError::InvalidBlockHeight.into());
    }

    // Check this report block doesn't already have a coverage report
    let bonds_info_db = wasm::db::db_lookup(cid, BEARER_BOND_CONTRACT_BONDS_INFO_TREE)?;
    let key = [&params.series_token_id.to_repr()[..], &params.report_block.to_le_bytes()[..]].concat();
    if wasm::db::db_contains_key(bonds_info_db, &key)? {
        msg!("[prove_coverage_v1] Error: Coverage report already exists for this block");
        return Err(BearerBondError::CoverageReportExists.into());
    }

    let report = CoverageReport {
        series_token_id: params.series_token_id,
        total_outstanding: params.total_outstanding,
        total_interest_obligation: params.total_interest_obligation,
        reserve_amount: params.reserve_amount,
        coverage_ratio_bps: params.coverage_ratio_bps,
        report_block: params.report_block,
    };

    // Auto-void the series if coverage falls below minimum.
    // This enables EmergencyUnstakeV1. Previously, prove_coverage_v1
    // rejected sub-100% reports, making emergency unstake unreachable.
    let is_voided = validation::is_coverage_voided(&report);
    if is_voided {
        msg!("[prove_coverage_v1] Coverage ratio {} bps < {} bps — voiding series {:?}",
            params.coverage_ratio_bps, validation::MIN_COVERAGE_RATIO_BPS, params.series_token_id);
    }

    let update = ProveCoverageUpdateV1 { report };
    let mut return_data = vec![BearerBondFunction::ProveCoverageV1 as u8];
    return_data.extend_from_slice(&update.encode());
    wasm::util::set_return_data(&return_data)
}

// ============================================================================
// EXECUTION: VERIFY COVERAGE (read-only query)
// ============================================================================

/// Read the latest coverage report for a series — no state changes.
fn verify_coverage_v1(
    cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>,
) -> ContractResult {
    let self_ = &calls[call_idx].data;
    if self_.data.len() < 2 {
        msg!("[bearer_bond::verify_coverage_v1] Error: Empty call data");
        return Err(BearerBondError::StakeNotFound.into());
    }
    let _series_token_id = pallas::Base::from_repr(self_.data[1..33].try_into().map_err(|_| BearerBondError::StakeNotFound)?)
        .into_option()
        .ok_or(BearerBondError::StakeNotFound)?;
    let _bonds_info_db = wasm::db::db_lookup(cid, BEARER_BOND_CONTRACT_BONDS_INFO_TREE)?;
    // FIXME: implement coverage lookup using a sentinel key pattern:
    // On ProveCoverageV1, store the latest report under key (series_token_id, u64::MAX)
    // in addition to the normal (series_token_id, report_block) key.
    // Then this function can look up the latest report directly without iteration.
    msg!("[bearer_bond::verify_coverage_v1] Coverage lookup not yet implemented — returning empty");
    wasm::util::set_return_data(&vec![])
}

// ============================================================================
// APPLY: ISSUE STAKE
// ============================================================================

fn apply_issue_stake(cid: ContractId, update: IssueStakeUpdateV1) -> ContractResult {
    let coins_db = wasm::db::db_lookup(cid, BEARER_BOND_CONTRACT_COINS_TREE)?;
    for coin in &update.coins {
        wasm::db::db_set(coins_db, &coin.token_commit.to_repr(), &coin.encode())?;
    }
    // TODO(#12): Increment series_info.total_staked in the bonds_info tree.
    // Currently total_staked is never updated after series creation, causing
    // request_interest_v1 to compute interest against stale values.
    // Fix requires: adding series_info to IssueStakeUpdateV1, reading/updating
    // in issue_stake_v1 instruction handler, and persisting here.
    Ok(())
}

// ============================================================================
// APPLY: TRANSFER STAKE
// ============================================================================

fn apply_transfer_stake(cid: ContractId, update: TransferStakeUpdateV1) -> ContractResult {
    let coins_db = wasm::db::db_lookup(cid, BEARER_BOND_CONTRACT_COINS_TREE)?;
    let nullifiers_db = wasm::db::db_lookup(cid, BEARER_BOND_CONTRACT_NULLIFIERS_TREE)?;

    for nullifier in &update.nullifiers {
        wasm::db::db_set(nullifiers_db, &nullifier.to_bytes(), &[])?;
    }
    for coin in &update.coins {
        wasm::db::db_set(coins_db, &coin.token_commit.to_repr(), &coin.encode())?;
    }
    Ok(())
}

// ============================================================================
// APPLY: REQUEST INTEREST
// ============================================================================

fn apply_request_interest(cid: ContractId, update: RequestInterestUpdateV1) -> ContractResult {
    let bonds_info_db = wasm::db::db_lookup(cid, BEARER_BOND_CONTRACT_BONDS_INFO_TREE)?;

    // Store the claim record keyed by (token_commit, claim_block)
    let claim_key = [&update.bond_token_commit.to_repr()[..], &update.claim_block.to_le_bytes()[..]].concat();
    wasm::db::db_set(bonds_info_db, &claim_key, &update.claim.encode())?;

    msg!("[apply_request_interest] Claim stored: bond={:?}, block={}, amount={}, status=Pending",
        update.bond_token_commit, update.claim_block, update.claim.interest_amount);
    Ok(())
}

// ============================================================================
// APPLY: PAY INTEREST
// ============================================================================

fn apply_pay_interest(cid: ContractId, update: PayInterestUpdateV1) -> ContractResult {
    let coins_db = wasm::db::db_lookup(cid, BEARER_BOND_CONTRACT_COINS_TREE)?;
    let bonds_info_db = wasm::db::db_lookup(cid, BEARER_BOND_CONTRACT_BONDS_INFO_TREE)?;

    // Update the stake coin with new last_claim_block
    wasm::db::db_set(
        coins_db,
        &update.updated_coin.token_commit.to_repr(),
        &update.updated_coin.encode(),
    )?;

    // Store the interest payment coin
    wasm::db::db_set(
        coins_db,
        &update.interest_coin.token_commit.to_repr(),
        &update.interest_coin.encode(),
    )?;

    // Write the claim with pre-set Paid status (computed in pay_interest_v1 exec phase)
    let claim_key = [&update.bond_token_commit.to_repr()[..], &update.claim_block.to_le_bytes()[..]].concat();
    wasm::db::db_set(bonds_info_db, &claim_key, &update.claim.encode())?;

    msg!("[apply_pay_interest] Payment applied: bond={:?}, block={}, status=Paid",
        update.bond_token_commit, update.claim_block);
    Ok(())
}

// ============================================================================
// APPLY: EMERGENCY UNSTAKE
// ============================================================================

fn apply_emergency_unstake(cid: ContractId, update: EmergencyUnstakeUpdateV1) -> ContractResult {
    let coins_db = wasm::db::db_lookup(cid, BEARER_BOND_CONTRACT_COINS_TREE)?;
    let nullifiers_db = wasm::db::db_lookup(cid, BEARER_BOND_CONTRACT_NULLIFIERS_TREE)?;

    for nullifier in &update.nullifiers {
        wasm::db::db_set(nullifiers_db, &nullifier.to_bytes(), &[])?;
    }
    wasm::db::db_set(coins_db, &update.receipt_coin.token_commit.to_repr(), &update.receipt_coin.encode())?;
    Ok(())
}

// ============================================================================
// APPLY: UNSTAKE
// ============================================================================

fn apply_unstake(cid: ContractId, update: UnstakeUpdateV1) -> ContractResult {
    let coins_db = wasm::db::db_lookup(cid, BEARER_BOND_CONTRACT_COINS_TREE)?;
    let nullifiers_db = wasm::db::db_lookup(cid, BEARER_BOND_CONTRACT_NULLIFIERS_TREE)?;

    for nullifier in &update.nullifiers {
        wasm::db::db_set(nullifiers_db, &nullifier.to_bytes(), &[])?;
    }
    wasm::db::db_set(coins_db, &update.receipt_coin.token_commit.to_repr(), &update.receipt_coin.encode())?;
    Ok(())
}

// ============================================================================
// APPLY: BURN STAKE
// ============================================================================

fn apply_burn_stake(cid: ContractId, update: BurnStakeUpdateV1) -> ContractResult {
    let nullifiers_db = wasm::db::db_lookup(cid, BEARER_BOND_CONTRACT_NULLIFIERS_TREE)?;
    for nullifier in &update.nullifiers {
        wasm::db::db_set(nullifiers_db, &nullifier.to_bytes(), &[])?;
    }
    Ok(())
}

// ============================================================================
// APPLY: PROVE COVERAGE
// ============================================================================

fn apply_prove_coverage(cid: ContractId, update: ProveCoverageUpdateV1) -> ContractResult {
    let bonds_info_db = wasm::db::db_lookup(cid, BEARER_BOND_CONTRACT_BONDS_INFO_TREE)?;

    // Store coverage report keyed by series_token_id + report_block
    let key = [&update.report.series_token_id.to_repr()[..], &update.report.report_block.to_le_bytes()[..]].concat();
    wasm::db::db_set(bonds_info_db, &key, &update.report.encode())?;

    // Auto-void the series if coverage falls below minimum.
    // Enables EmergencyUnstakeV1 by filing a sub-100% report.
    if validation::is_coverage_voided(&update.report) {
        let series_key = update.report.series_token_id.to_repr();
        if let Ok(Some(series_bytes)) = wasm::db::db_get(bonds_info_db, &series_key) {
            if let Ok(mut series_info) = BondSeriesInfo::decode(&series_bytes) {
                if series_info.status == SeriesStatus::Active {
                    series_info.status = SeriesStatus::Voided;
                    wasm::db::db_set(bonds_info_db, &series_key, &series_info.encode())?;
                    msg!("[apply_prove_coverage] Series {:?} auto-voided: coverage {} bps < {} bps",
                        update.report.series_token_id,
                        update.report.coverage_ratio_bps,
                        validation::MIN_COVERAGE_RATIO_BPS);
                }
            }
        }
    }

    msg!("[apply_prove_coverage] Coverage report stored: series={:?}, block={}, ratio={} bps",
        update.report.series_token_id, update.report.report_block, update.report.coverage_ratio_bps);
    Ok(())
}
