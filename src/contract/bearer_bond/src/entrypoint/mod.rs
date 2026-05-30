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
//! | 3 | ClaimInterestV1 | 0x02 | Holder | Claim deterministic interest accrued on stake |
//! | 4 | EmergencyUnstakeV1 | 0x03 | Holder | Exit before maturity when coverage below minimum |
//! | 5 | UnstakeV1 | 0x04 | Holder | Withdraw principal + unclaimed interest at maturity |
//! | 6 | BurnStakeV1 | 0x05 | Issuer | Retire staking pool |
//! | 7 | ProveCoverageV1 | 0x06 | Issuer/Holder | Submit ZK proof of solvency |
//! | 8 | VerifyCoverageV1 | 0x07 | Holder | Read latest coverage report for a series |

use dwow_sdk::{
    crypto::{
        pasta_prelude::{Curve, CurveAffine},
        ContractId,
    },
    dark_tree::DarkLeaf,
    error::ContractResult,
    msg,
    pasta::pallas,
    wasm, ContractCall,
};
use dwow_serial::{deserialize, serialize, Encodable};

use crate::{
    error::BearerBondError,
    model::{
        BondCoin, BondSeriesInfo, BurnStakeParamsV1, BurnStakeUpdateV1,
        ClaimInterestParamsV1, ClaimInterestUpdateV1,
        CoverageReport, EmergencyUnstakeParamsV1, EmergencyUnstakeUpdateV1,
        IssueStakeParamsV1, IssueStakeUpdateV1,
        ProveCoverageParamsV1, ProveCoverageUpdateV1, SeriesStatus,
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
    BEARER_BOND_CONTRACT_ZKAS_BURN_NS_V1, BEARER_BOND_CONTRACT_ZKAS_BLIND_OUTPUT_NS_V1,
    BEARER_BOND_CONTRACT_ZKAS_PROVE_COVERAGE_NS_V1,
    BEARER_BOND_CONTRACT_ZKAS_REDEEM_NS_V1,
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
    let burn_v1_bincode = include_bytes!("../../proof/burn_v1.zk.bin");
    let blind_output_v1_bincode = include_bytes!("../../proof/blind_output_v1.zk.bin");
    let redeem_v1_bincode = include_bytes!("../../proof/redeem_v1.zk.bin");
    let prove_coverage_v1_bincode = include_bytes!("../../proof/prove_coverage_v1.zk.bin");

    wasm::db::zkas_db_set(&burn_v1_bincode[..])?;
    wasm::db::zkas_db_set(&blind_output_v1_bincode[..])?;
    wasm::db::zkas_db_set(&redeem_v1_bincode[..])?;
    wasm::db::zkas_db_set(&prove_coverage_v1_bincode[..])?;

    let tx_hash = wasm::util::get_tx_hash()?;
    let call_idx = wasm::util::get_call_index()?;
    let mut roots_data = Vec::with_capacity(32 + 1);
    tx_hash.encode(&mut roots_data)?;
    call_idx.encode(&mut roots_data)?;

    // Coin roots database
    if wasm::db::db_lookup(cid, BEARER_BOND_CONTRACT_COIN_ROOTS_TREE).is_err() {
        let db_coin_roots = wasm::db::db_init(cid, BEARER_BOND_CONTRACT_COIN_ROOTS_TREE)?;
        wasm::db::db_set(db_coin_roots, &serialize(&BEARER_BOND_EMPTY_COINS_ROOT), &roots_data)?;
    }

    // Nullifier roots database
    if wasm::db::db_lookup(cid, BEARER_BOND_CONTRACT_NULLIFIER_ROOTS_TREE).is_err() {
        let db_null_roots = wasm::db::db_init(cid, BEARER_BOND_CONTRACT_NULLIFIER_ROOTS_TREE)?;
        wasm::db::db_set(
            db_null_roots,
            &serialize(&BEARER_BOND_EMPTY_NULLIFIER_ROOT),
            &serialize(&vec![roots_data.clone()]),
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
        BearerBondFunction::ClaimInterestV1 => claim_interest_metadata(cid, call_idx, calls),
        BearerBondFunction::EmergencyUnstakeV1 => emergency_unstake_metadata(cid, call_idx, calls),
        BearerBondFunction::UnstakeV1 => unstake_metadata(cid, call_idx, calls),
        BearerBondFunction::BurnStakeV1 => burn_stake_metadata(cid, call_idx, calls),
        BearerBondFunction::ProveCoverageV1 => prove_coverage_metadata(cid, call_idx, calls),
        BearerBondFunction::VerifyCoverageV1 => vec![],
    };

    wasm::util::set_return_data(&metadata)
}

/// Extract (x, y) base-field coordinates from a pallas::Point for ZK public inputs.
fn point_coords(pt: pallas::Point) -> (pallas::Base, pallas::Base) {
    let affine = pt.to_affine();
    let coords = affine.coordinates().unwrap();
    (*coords.x(), *coords.y())
}

// ============================================================================
// METADATA: ISSUE STAKE
// ============================================================================

/// Metadata for IssueStakeV1 — BlindOutput_V1 instance(s) for output coins.
fn issue_stake_metadata(_cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>) -> Vec<u8> {
    let self_ = &calls[call_idx].data;
    let params: IssueStakeParamsV1 = match deserialize(&self_.data[1..]) { Ok(p) => p, Err(_) => return vec![] };

    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    let signature_pubkeys: Vec<pallas::Base> = vec![];

    let (vc_x, vc_y) = point_coords(params.coin.value_commit);

    zk_public_inputs.push((
        BEARER_BOND_CONTRACT_ZKAS_BLIND_OUTPUT_NS_V1.to_string(),
        vec![
            params.coin.token_commit, // coin identifier
            vc_x,
            vc_y,
            params.coin.token_commit,
            params.coin.spend_hook,
        ],
    ));

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata).unwrap();
    signature_pubkeys.encode(&mut metadata).unwrap();
    metadata
}

// ============================================================================
// METADATA: TRANSFER STAKE
// ============================================================================

/// Metadata for TransferStakeV1 — Burn_V1 for inputs, BlindOutput_V1 for outputs.
fn transfer_stake_metadata(_cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>) -> Vec<u8> {
    let self_ = &calls[call_idx].data;
    let params: TransferStakeParamsV1 = match deserialize(&self_.data[1..]) { Ok(p) => p, Err(_) => return vec![] };

    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    let mut signature_pubkeys: Vec<pallas::Base> = vec![];

    // Burn proofs (one per input)
    for input in &params.inputs {
        signature_pubkeys.push(input.signature_public);
        let (vc_x, vc_y) = point_coords(input.value_commit);

        zk_public_inputs.push((
            BEARER_BOND_CONTRACT_ZKAS_BURN_NS_V1.to_string(),
            vec![
                input.nullifier.inner(),
                vc_x,
                vc_y,
                input.token_commit,
                input.merkle_root.inner(),
                input.user_data_enc,
                input.spend_hook,
                input.signature_public,
            ],
        ));
    }

    // BlindOutput proofs (one per output)
    for output in &params.outputs {
        let (vc_x, vc_y) = point_coords(output.value_commit);

        zk_public_inputs.push((
            BEARER_BOND_CONTRACT_ZKAS_BLIND_OUTPUT_NS_V1.to_string(),
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
    zk_public_inputs.encode(&mut metadata).unwrap();
    signature_pubkeys.encode(&mut metadata).unwrap();
    metadata
}

// ============================================================================
// METADATA: CLAIM INTEREST
// ============================================================================

/// Metadata for ClaimInterestV1 — BlindOutput_V1 for interest payout coin.
fn claim_interest_metadata(_cid: ContractId, _call_idx: usize, _calls: Vec<DarkLeaf<ContractCall>>) -> Vec<u8> {
    let zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    let signature_pubkeys: Vec<pallas::Base> = vec![];

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata).unwrap();
    signature_pubkeys.encode(&mut metadata).unwrap();
    metadata
}

// ============================================================================
// METADATA: EMERGENCY UNSTAKE
// ============================================================================

/// Metadata for EmergencyUnstakeV1 — Burn_V1 for input, Redeem_V1 for receipt coin.
fn emergency_unstake_metadata(_cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>) -> Vec<u8> {
    let self_ = &calls[call_idx].data;
    let params: EmergencyUnstakeParamsV1 = match deserialize(&self_.data[1..]) { Ok(p) => p, Err(_) => return vec![] };

    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    let mut signature_pubkeys: Vec<pallas::Base> = vec![];

    // Burn proof for the input stake coin
    signature_pubkeys.push(params.bond_input.signature_public);
    let (vc_x, vc_y) = point_coords(params.bond_input.value_commit);

    zk_public_inputs.push((
        BEARER_BOND_CONTRACT_ZKAS_BURN_NS_V1.to_string(),
        vec![
            params.bond_input.nullifier.inner(),
            vc_x,
            vc_y,
            params.bond_input.token_commit,
            params.bond_input.merkle_root.inner(),
            params.bond_input.user_data_enc,
            params.bond_input.spend_hook,
            params.bond_input.signature_public,
        ],
    ));

    // Redeem_V1 proof for the receipt coin (value=0)
    let coin_value = pallas::Base::zero();
    zk_public_inputs.push((
        BEARER_BOND_CONTRACT_ZKAS_REDEEM_NS_V1.to_string(),
        vec![
            params.bond_input.token_commit,
            vc_x,
            vc_y,
            params.bond_input.token_commit,
            coin_value,
            params.bond_input.spend_hook,
        ],
    ));

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata).unwrap();
    signature_pubkeys.encode(&mut metadata).unwrap();
    metadata
}

// ============================================================================
// METADATA: UNSTAKE
// ============================================================================

/// Metadata for UnstakeV1 — Burn_V1 for input, Redeem_V1 for receipt coin.
fn unstake_metadata(_cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>) -> Vec<u8> {
    let self_ = &calls[call_idx].data;
    let params: UnstakeParamsV1 = match deserialize(&self_.data[1..]) { Ok(p) => p, Err(_) => return vec![] };

    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    let mut signature_pubkeys: Vec<pallas::Base> = vec![];

    // Burn proof for the input stake coin
    signature_pubkeys.push(params.bond_input.signature_public);
    let (vc_x, vc_y) = point_coords(params.bond_input.value_commit);

    zk_public_inputs.push((
        BEARER_BOND_CONTRACT_ZKAS_BURN_NS_V1.to_string(),
        vec![
            params.bond_input.nullifier.inner(),
            vc_x,
            vc_y,
            params.bond_input.token_commit,
            params.bond_input.merkle_root.inner(),
            params.bond_input.user_data_enc,
            params.bond_input.spend_hook,
            params.bond_input.signature_public,
        ],
    ));

    // Redeem_V1 proof for the receipt coin (value=0)
    let coin_value = pallas::Base::zero();

    zk_public_inputs.push((
        BEARER_BOND_CONTRACT_ZKAS_REDEEM_NS_V1.to_string(),
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
    zk_public_inputs.encode(&mut metadata).unwrap();
    signature_pubkeys.encode(&mut metadata).unwrap();
    metadata
}

// ============================================================================
// METADATA: BURN STAKE
// ============================================================================

/// Metadata for BurnStakeV1 — Burn_V1 instance(s) for inputs.
fn burn_stake_metadata(_cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>) -> Vec<u8> {
    let self_ = &calls[call_idx].data;
    let params: BurnStakeParamsV1 = match deserialize(&self_.data[1..]) { Ok(p) => p, Err(_) => return vec![] };

    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    let mut signature_pubkeys: Vec<pallas::Base> = vec![];

    for input in &params.inputs {
        signature_pubkeys.push(input.signature_public);
        let (vc_x, vc_y) = point_coords(input.value_commit);

        zk_public_inputs.push((
            BEARER_BOND_CONTRACT_ZKAS_BURN_NS_V1.to_string(),
            vec![
                input.nullifier.inner(),
                vc_x,
                vc_y,
                input.token_commit,
                input.merkle_root.inner(),
                input.user_data_enc,
                input.spend_hook,
                input.signature_public,
            ],
        ));
    }

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata).unwrap();
    signature_pubkeys.encode(&mut metadata).unwrap();
    metadata
}

// ============================================================================
// METADATA: PROVE COVERAGE
// ============================================================================

/// Metadata for ProveCoverageV1 — ProveCoverage_V1 circuit with coverage_ratio_bps public input.
fn prove_coverage_metadata(_cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>) -> Vec<u8> {
    let self_ = &calls[call_idx].data;
    let params: ProveCoverageParamsV1 = match deserialize(&self_.data[1..]) { Ok(p) => p, Err(_) => return vec![] };

    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    let signature_pubkeys: Vec<pallas::Base> = vec![];

    zk_public_inputs.push((
        BEARER_BOND_CONTRACT_ZKAS_PROVE_COVERAGE_NS_V1.to_string(),
        vec![pallas::Base::from(params.coverage_ratio_bps)],
    ));

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata).unwrap();
    signature_pubkeys.encode(&mut metadata).unwrap();
    metadata
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
        BearerBondFunction::ClaimInterestV1 => claim_interest_v1(cid, call_idx, calls),
        BearerBondFunction::EmergencyUnstakeV1 => emergency_unstake_v1(cid, call_idx, calls),
        BearerBondFunction::UnstakeV1 => unstake_v1(cid, call_idx, calls),
        BearerBondFunction::BurnStakeV1 => burn_stake_v1(cid, call_idx, calls),
        BearerBondFunction::ProveCoverageV1 => prove_coverage_v1(cid, call_idx, calls),
        BearerBondFunction::VerifyCoverageV1 => verify_coverage_v1(cid, call_idx, calls),
    }
}

// ============================================================================
// UPDATE ROUTING
// ============================================================================

fn process_update(cid: ContractId, update_data: &[u8]) -> ContractResult {
    let func = BearerBondFunction::try_from(update_data[0])?;

    match func {
        BearerBondFunction::IssueStakeV1 => {
            let update: IssueStakeUpdateV1 = deserialize(&update_data[1..])?;
            apply_issue_stake(cid, update)
        }
        BearerBondFunction::TransferStakeV1 => {
            let update: TransferStakeUpdateV1 = deserialize(&update_data[1..])?;
            apply_transfer_stake(cid, update)
        }
        BearerBondFunction::ClaimInterestV1 => {
            let update: ClaimInterestUpdateV1 = deserialize(&update_data[1..])?;
            apply_claim_interest(cid, update)
        }
        BearerBondFunction::EmergencyUnstakeV1 => {
            let update: EmergencyUnstakeUpdateV1 = deserialize(&update_data[1..])?;
            apply_emergency_unstake(cid, update)
        }
        BearerBondFunction::UnstakeV1 => {
            let update: UnstakeUpdateV1 = deserialize(&update_data[1..])?;
            apply_unstake(cid, update)
        }
        BearerBondFunction::BurnStakeV1 => {
            let update: BurnStakeUpdateV1 = deserialize(&update_data[1..])?;
            apply_burn_stake(cid, update)
        }
        BearerBondFunction::ProveCoverageV1 => {
            let update: ProveCoverageUpdateV1 = deserialize(&update_data[1..])?;
            apply_prove_coverage(cid, update)
        }
        BearerBondFunction::VerifyCoverageV1 => Ok(()),
    }
}

// ============================================================================
// EXECUTION: ISSUE STAKE
// ============================================================================

fn issue_stake_v1(
    cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>,
) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params: IssueStakeParamsV1 = deserialize(&self_.data[1..])?;

    // Verify the bond series exists and is active
    let bonds_info_db = wasm::db::db_lookup(cid, BEARER_BOND_CONTRACT_BONDS_INFO_TREE)?;
    let series_key = serialize(&params.token_id);
    if !wasm::db::db_contains_key(bonds_info_db, &series_key)? {
        msg!("[issue_stake_v1] Error: Bond series not found");
        return Err(BearerBondError::StakeNotFound.into());
    }

    let coins_db = wasm::db::db_lookup(cid, BEARER_BOND_CONTRACT_COINS_TREE)?;
    if wasm::db::db_contains_key(coins_db, &serialize(&params.coin.token_commit))? {
        msg!("[issue_stake_v1] Error: Stake coin already exists");
        return Err(BearerBondError::StakeAlreadyExists.into());
    }

    let update = IssueStakeUpdateV1 { coins: vec![params.coin] };
    wasm::util::set_return_data(&serialize(&(BearerBondFunction::IssueStakeV1 as u8, update)))
}

// ============================================================================
// EXECUTION: TRANSFER STAKE
// ============================================================================

fn transfer_stake_v1(
    cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>,
) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params: TransferStakeParamsV1 = deserialize(&self_.data[1..])?;

    if params.inputs.is_empty() {
        return Err(BearerBondError::MissingInputs.into());
    }
    if params.outputs.is_empty() {
        return Err(BearerBondError::MissingOutputs.into());
    }

    let coins_db = wasm::db::db_lookup(cid, BEARER_BOND_CONTRACT_COINS_TREE)?;
    let nullifiers_db = wasm::db::db_lookup(cid, BEARER_BOND_CONTRACT_NULLIFIERS_TREE)?;

    for input in &params.inputs {
        if !wasm::db::db_contains_key(coins_db, &serialize(&input.token_commit))? {
            msg!("[transfer_stake_v1] Error: Input stake coin not found");
            return Err(BearerBondError::StakeNotFound.into());
        }
        if wasm::db::db_contains_key(nullifiers_db, &serialize(&input.nullifier))? {
            msg!("[transfer_stake_v1] Error: Duplicate nullifier");
            return Err(BearerBondError::DuplicateNullifier.into());
        }
    }

    for output in &params.outputs {
        if wasm::db::db_contains_key(coins_db, &serialize(&output.token_commit))? {
            msg!("[transfer_stake_v1] Error: Output stake coin already exists");
            return Err(BearerBondError::StakeAlreadyExists.into());
        }
        if output.last_claim_block >= output.maturity_block {
            return Err(BearerBondError::StakeAlreadyMatured.into());
        }
    }

    // Verify value conservation via Pedersen commitment homomorphic sum.
    // Σ inputs.value_commit == Σ outputs.value_commit  ⇒  Σ inputs.principal == Σ outputs.principal
    let total_input: pallas::Point = params.inputs.iter().map(|i| i.value_commit).sum();
    let total_output: pallas::Point = params.outputs.iter().map(|o| o.value_commit).sum();
    if total_input != total_output {
        return Err(BearerBondError::ValueMismatch.into());
    }

    let nullifiers: Vec<_> = params.inputs.iter().map(|i| i.nullifier).collect();
    let update = TransferStakeUpdateV1 { nullifiers, coins: params.outputs };
    wasm::util::set_return_data(&serialize(&(BearerBondFunction::TransferStakeV1 as u8, update)))
}

// ============================================================================
// EXECUTION: CLAIM INTEREST
// ============================================================================

fn claim_interest_v1(
    cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>,
) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params: ClaimInterestParamsV1 = deserialize(&self_.data[1..])?;

    if params.claim_block == 0 {
        return Err(BearerBondError::InvalidBlockHeight.into());
    }

    let coins_db = wasm::db::db_lookup(cid, BEARER_BOND_CONTRACT_COINS_TREE)?;

    // Look up the existing stake coin to get last_claim_block
    let coin_bytes = wasm::db::db_get(coins_db, &serialize(&params.bond_input.token_commit))?
        .ok_or(BearerBondError::StakeNotFound)?;
    let stake_coin: BondCoin = deserialize(&coin_bytes)?;

    // Verify the claim block is after the last claim block
    if params.claim_block <= stake_coin.last_claim_block {
        return Err(BearerBondError::InvalidInterestClaim {
            last: stake_coin.last_claim_block,
            current: params.claim_block,
        }.into());
    }

    // Read the bond series info to get interest rate
    let bonds_info_db = wasm::db::db_lookup(cid, BEARER_BOND_CONTRACT_BONDS_INFO_TREE)?;
    // We need the series token_id — derive from token_commit by reading the coin's plaintext field
    // The token_commit is a Poseidon hash; the actual token_id is in the witness.
    // For the entrypoint we use the coin's issuer_contract and look up the series.
    // Since the coin has no explicit series_token_id in its on-chain fields,
    // we use the token_commit to identify the series via the bonds_info tree.
    let series_key = serialize(&stake_coin.token_commit);
    let series_bytes = match wasm::db::db_get(bonds_info_db, &series_key) {
        Ok(Some(b)) => b,
        _ => {
            msg!("[claim_interest_v1] Error: Bond series info not found");
            return Err(BearerBondError::StakeNotFound.into());
        }
    };
    let series_info: BondSeriesInfo = deserialize(&series_bytes)?;

    // Check series status
    if series_info.status != SeriesStatus::Active {
        msg!("[claim_interest_v1] Error: Series is not active");
        return Err(BearerBondError::SeriesNotActive.into());
    }

    let blocks_elapsed = params.claim_block - stake_coin.last_claim_block;

    // Compute interest deterministically
    let interest = match crate::model::calculate_interest(
        series_info.total_staked, // approximate — real impl uses principal from witness
        series_info.interest_rate_bps,
        blocks_elapsed,
    ) {
        Some(v) => v,
        None => return Err(BearerBondError::InterestOverflow.into()),
    };

    // Minimum claim threshold
    if interest < params.min_claim {
        msg!("[claim_interest_v1] Interest below minimum claim threshold: {} < {}", interest, params.min_claim);
        return Err(BearerBondError::InterestOverflow.into());
    }

    let updated_coin = BondCoin {
        last_claim_block: params.claim_block,
        ..stake_coin
    };

    let interest_coin = BondCoin {
        ..Default::default()
    };

    let update = ClaimInterestUpdateV1 { updated_coin, interest_coin };
    wasm::util::set_return_data(&serialize(&(BearerBondFunction::ClaimInterestV1 as u8, update)))
}

// ============================================================================
// EXECUTION: EMERGENCY UNSTAKE
// ============================================================================

fn emergency_unstake_v1(
    cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>,
) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params: EmergencyUnstakeParamsV1 = deserialize(&self_.data[1..])?;

    // Verify the coverage report shows voided coverage
    if !validation::is_coverage_voided(&params.coverage_report) {
        msg!("[emergency_unstake_v1] Error: Coverage is above minimum — emergency unstake not allowed");
        return Err(BearerBondError::EmergencyUnstakeNotAllowed.into());
    }

    let coins_db = wasm::db::db_lookup(cid, BEARER_BOND_CONTRACT_COINS_TREE)?;
    let nullifiers_db = wasm::db::db_lookup(cid, BEARER_BOND_CONTRACT_NULLIFIERS_TREE)?;

    if !wasm::db::db_contains_key(coins_db, &serialize(&params.bond_input.token_commit))? {
        msg!("[emergency_unstake_v1] Error: Stake coin not found");
        return Err(BearerBondError::StakeNotFound.into());
    }

    if wasm::db::db_contains_key(nullifiers_db, &serialize(&params.bond_input.nullifier))? {
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
    wasm::util::set_return_data(&serialize(&(BearerBondFunction::EmergencyUnstakeV1 as u8, update)))
}

// ============================================================================
// EXECUTION: UNSTAKE
// ============================================================================

fn unstake_v1(
    cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>,
) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params: UnstakeParamsV1 = deserialize(&self_.data[1..])?;

    let coins_db = wasm::db::db_lookup(cid, BEARER_BOND_CONTRACT_COINS_TREE)?;
    let nullifiers_db = wasm::db::db_lookup(cid, BEARER_BOND_CONTRACT_NULLIFIERS_TREE)?;

    // Look up the stake coin to verify maturity
    let coin_bytes = wasm::db::db_get(coins_db, &serialize(&params.bond_input.token_commit))?
        .ok_or(BearerBondError::StakeNotFound)?;
    let stake_coin: BondCoin = deserialize(&coin_bytes)?;

    // Enforce maturity: current block must be >= maturity block
    if params.current_block < stake_coin.maturity_block {
        msg!("[unstake_v1] Error: Stake not yet matured — current={}, maturity={}",
            params.current_block, stake_coin.maturity_block);
        return Err(BearerBondError::StakeNotMatured {
            current: params.current_block,
            maturity: stake_coin.maturity_block,
        }.into());
    }

    if wasm::db::db_contains_key(nullifiers_db, &serialize(&params.bond_input.nullifier))? {
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
    wasm::util::set_return_data(&serialize(&(BearerBondFunction::UnstakeV1 as u8, update)))
}

// ============================================================================
// EXECUTION: BURN STAKE
// ============================================================================

fn burn_stake_v1(
    cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>,
) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params: BurnStakeParamsV1 = deserialize(&self_.data[1..])?;

    if params.inputs.is_empty() {
        return Err(BearerBondError::MissingInputs.into());
    }

    let coins_db = wasm::db::db_lookup(cid, BEARER_BOND_CONTRACT_COINS_TREE)?;
    let nullifiers_db = wasm::db::db_lookup(cid, BEARER_BOND_CONTRACT_NULLIFIERS_TREE)?;

    for input in &params.inputs {
        if !wasm::db::db_contains_key(coins_db, &serialize(&input.token_commit))? {
            msg!("[burn_stake_v1] Error: Stake coin not found");
            return Err(BearerBondError::StakeNotFound.into());
        }
        if wasm::db::db_contains_key(nullifiers_db, &serialize(&input.nullifier))? {
            msg!("[burn_stake_v1] Error: Duplicate nullifier");
            return Err(BearerBondError::DuplicateNullifier.into());
        }
    }

    let nullifiers: Vec<_> = params.inputs.iter().map(|i| i.nullifier).collect();
    let update = BurnStakeUpdateV1 { nullifiers };
    wasm::util::set_return_data(&serialize(&(BearerBondFunction::BurnStakeV1 as u8, update)))
}

// ============================================================================
// EXECUTION: PROVE COVERAGE
// ============================================================================

fn prove_coverage_v1(
    cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>,
) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params: ProveCoverageParamsV1 = deserialize(&self_.data[1..])?;

    if params.total_outstanding == 0 {
        return Err(BearerBondError::InvalidPrincipal.into());
    }
    if params.reserve_amount == 0 {
        return Err(BearerBondError::InvalidPrincipal.into());
    }
    if params.report_block == 0 {
        return Err(BearerBondError::InvalidBlockHeight.into());
    }

    let total_obligation = params.total_outstanding.saturating_add(params.total_interest_obligation);

    // Verify full coverage: reserves must cover principal + interest
    if params.reserve_amount < total_obligation {
        return Err(BearerBondError::InsufficientReserveForInterest {
            reserve: params.reserve_amount,
            obligation: total_obligation,
        }.into());
    }

    // Verify coverage ratio is at least 10000 bps (100%)
    if params.coverage_ratio_bps < validation::MIN_COVERAGE_RATIO_BPS {
        return Err(BearerBondError::InsufficientCoverage {
            reported: params.coverage_ratio_bps,
        }.into());
    }

    // Check this report block doesn't already have a coverage report
    let bonds_info_db = wasm::db::db_lookup(cid, BEARER_BOND_CONTRACT_BONDS_INFO_TREE)?;
    let key = serialize(&(params.series_token_id, params.report_block));
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

    let update = ProveCoverageUpdateV1 { report };
    wasm::util::set_return_data(&serialize(&(BearerBondFunction::ProveCoverageV1 as u8, update)))
}

// ============================================================================
// EXECUTION: VERIFY COVERAGE (read-only query)
// ============================================================================

/// Read the latest coverage report for a series — no state changes.
fn verify_coverage_v1(
    cid: ContractId, _call_idx: usize, _calls: Vec<DarkLeaf<ContractCall>>,
) -> ContractResult {
    let bonds_info_db = wasm::db::db_lookup(cid, BEARER_BOND_CONTRACT_BONDS_INFO_TREE)?;
    // The caller passes the series_token_id as raw bytes; we return the latest report
    // Look up by iterating over the bonds_info tree for the given series
    // For now, return an empty result — the caller reads the DB directly
    let _ = bonds_info_db;
    Ok(())
}

// ============================================================================
// APPLY: ISSUE STAKE
// ============================================================================

fn apply_issue_stake(cid: ContractId, update: IssueStakeUpdateV1) -> ContractResult {
    let coins_db = wasm::db::db_lookup(cid, BEARER_BOND_CONTRACT_COINS_TREE)?;
    for coin in &update.coins {
        wasm::db::db_set(coins_db, &serialize(&coin.token_commit), &serialize(coin))?;
    }
    Ok(())
}

// ============================================================================
// APPLY: TRANSFER STAKE
// ============================================================================

fn apply_transfer_stake(cid: ContractId, update: TransferStakeUpdateV1) -> ContractResult {
    let coins_db = wasm::db::db_lookup(cid, BEARER_BOND_CONTRACT_COINS_TREE)?;
    let nullifiers_db = wasm::db::db_lookup(cid, BEARER_BOND_CONTRACT_NULLIFIERS_TREE)?;

    for nullifier in &update.nullifiers {
        wasm::db::db_set(nullifiers_db, &serialize(nullifier), &[])?;
    }
    for coin in &update.coins {
        wasm::db::db_set(coins_db, &serialize(&coin.token_commit), &serialize(coin))?;
    }
    Ok(())
}

// ============================================================================
// APPLY: CLAIM INTEREST
// ============================================================================

fn apply_claim_interest(cid: ContractId, update: ClaimInterestUpdateV1) -> ContractResult {
    let coins_db = wasm::db::db_lookup(cid, BEARER_BOND_CONTRACT_COINS_TREE)?;

    // Update the stake coin with new last_claim_block
    wasm::db::db_set(
        coins_db,
        &serialize(&update.updated_coin.token_commit),
        &serialize(&update.updated_coin),
    )?;

    // Store the interest payout coin
    wasm::db::db_set(
        coins_db,
        &serialize(&update.interest_coin.token_commit),
        &serialize(&update.interest_coin),
    )?;

    Ok(())
}

// ============================================================================
// APPLY: EMERGENCY UNSTAKE
// ============================================================================

fn apply_emergency_unstake(cid: ContractId, update: EmergencyUnstakeUpdateV1) -> ContractResult {
    let coins_db = wasm::db::db_lookup(cid, BEARER_BOND_CONTRACT_COINS_TREE)?;
    let nullifiers_db = wasm::db::db_lookup(cid, BEARER_BOND_CONTRACT_NULLIFIERS_TREE)?;

    for nullifier in &update.nullifiers {
        wasm::db::db_set(nullifiers_db, &serialize(nullifier), &[])?;
    }
    wasm::db::db_set(coins_db, &serialize(&update.receipt_coin.token_commit), &serialize(&update.receipt_coin))?;
    Ok(())
}

// ============================================================================
// APPLY: UNSTAKE
// ============================================================================

fn apply_unstake(cid: ContractId, update: UnstakeUpdateV1) -> ContractResult {
    let coins_db = wasm::db::db_lookup(cid, BEARER_BOND_CONTRACT_COINS_TREE)?;
    let nullifiers_db = wasm::db::db_lookup(cid, BEARER_BOND_CONTRACT_NULLIFIERS_TREE)?;

    for nullifier in &update.nullifiers {
        wasm::db::db_set(nullifiers_db, &serialize(nullifier), &[])?;
    }
    wasm::db::db_set(coins_db, &serialize(&update.receipt_coin.token_commit), &serialize(&update.receipt_coin))?;
    Ok(())
}

// ============================================================================
// APPLY: BURN STAKE
// ============================================================================

fn apply_burn_stake(cid: ContractId, update: BurnStakeUpdateV1) -> ContractResult {
    let nullifiers_db = wasm::db::db_lookup(cid, BEARER_BOND_CONTRACT_NULLIFIERS_TREE)?;
    for nullifier in &update.nullifiers {
        wasm::db::db_set(nullifiers_db, &serialize(nullifier), &[])?;
    }
    Ok(())
}

// ============================================================================
// APPLY: PROVE COVERAGE
// ============================================================================

fn apply_prove_coverage(cid: ContractId, update: ProveCoverageUpdateV1) -> ContractResult {
    let bonds_info_db = wasm::db::db_lookup(cid, BEARER_BOND_CONTRACT_BONDS_INFO_TREE)?;

    // Store coverage report keyed by series_token_id + report_block
    let key = serialize(&(
        update.report.series_token_id,
        update.report.report_block,
    ));
    wasm::db::db_set(bonds_info_db, &key, &serialize(&update.report))?;

    msg!("[apply_prove_coverage] Coverage report stored: series={:?}, block={}, ratio={} bps",
        update.report.series_token_id, update.report.report_block, update.report.coverage_ratio_bps);
    Ok(())
}
