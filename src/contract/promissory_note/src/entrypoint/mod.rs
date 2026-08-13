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

//! Promissory Note WASM Entrypoint - DeFi Token Contract
//!
//! Design: PRIVACY FIRST, COMPOSABILITY SECOND, SIMPLICITY THIRD
//!
//! PromissoryNote is the privacy-focused token contract for DeFi use cases:
//! - Wrapped tokens (wBTC, wETH, etc.)
//! - Stablecoins (USD, EUR, etc.)
//! - ERC-20 style tokens
//!
//! ## Token Model
//!
//! - RegisterTypeV1: Creates a new token type (returns token_id)
//! - IssueV1: Mints tokens (proves backing capability)
//! - RevokeV1: Burns tokens
//! - TransferV1: Private token transfer
//! - OtcSwapV1: Atomic OTC token swap
//!
//! ## Value Conservation
//!
//! Value commitments use Pedersen (additively homomorphic). The entrypoint
//! enforces per-token-commit value conservation: for each token_commit group,
//! sum(input value_commits) == sum(output value_commits). This prevents
//! value inflation/deflation while preserving privacy (no plaintext values).

use dwow_sdk::{
    crypto::{
        pasta_prelude::{Curve, CurveAffine, Field, PrimeField},
        ContractId, MerkleNode, MerkleTree,
    },
    dark_tree::DarkLeaf,
    error::ContractResult,
    msg,
    pasta::pallas,
    wasm, ContractCall,
};
use dwow_serial::{deserialize, Encodable, WriteExt};

use crate::{
    error::{ContractError, PromissoryNoteError},
    model::{
        RevokeParamsV1, RevokeSpendHookPayload, RevokeUpdateV1,
        IssueParamsV1, IssueUpdateV1, OtcSwapParamsV1,
        OtcSwapUpdateV1, RedeemParamsV1, RedeemUpdateV1,
        RegisterTypeParamsV1, RegisterTypeUpdateV1, TransferParamsV1,
        TransferUpdateV1,
    },
    PromissoryNoteFunction, PROMISSORY_NOTE_CONTRACT_COIN_MERKLE_TREE,
    PROMISSORY_NOTE_CONTRACT_COIN_ROOTS_TREE, PROMISSORY_NOTE_CONTRACT_COINS_TREE,
    PROMISSORY_NOTE_CONTRACT_DB_VERSION,
    PROMISSORY_NOTE_CONTRACT_INFO_TREE, PROMISSORY_NOTE_CONTRACT_LATEST_COIN_ROOT,
    PROMISSORY_NOTE_CONTRACT_LATEST_NULLIFIER_ROOT,
    PROMISSORY_NOTE_CONTRACT_TOTAL_SUPPLY,
    PROMISSORY_NOTE_CONTRACT_LATEST_TOKEN_REGISTRY_ROOT,
    PROMISSORY_NOTE_CONTRACT_NULLIFIERS_TREE,
    PROMISSORY_NOTE_CONTRACT_NULLIFIER_ROOTS_TREE,
    PROMISSORY_NOTE_CONTRACT_TOKEN_REGISTRY_MERKLE_TREE,
    PROMISSORY_NOTE_CONTRACT_TOKEN_REGISTRY_ROOTS_TREE,
    PROMISSORY_NOTE_CONTRACT_TOKEN_REGISTRY_TREE,
    PROMISSORY_NOTE_CONTRACT_ZKAS_REGISTER_TYPE_NS_V2,
    PROMISSORY_NOTE_CONTRACT_ZKAS_ISSUE_NS_V2,
    PROMISSORY_NOTE_CONTRACT_ZKAS_REVOKE_NS_V2,
    PROMISSORY_NOTE_CONTRACT_ZKAS_TRANSFER_NS_V2,
    PROMISSORY_NOTE_CONTRACT_ZKAS_REDEEM_NS_V2,
    EMPTY_COINS_TREE_ROOT, EMPTY_TOKEN_REGISTRY_TREE_ROOT,
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
    msg!("[promissory_note::init_contract] Initializing promissory_note contract (DeFi tokens)");

    // Include ZK circuits


    // V2 circuits (HAZOP RC3: domain separation)
    let register_type_v2_bincode = include_bytes!("../../proof/register_type.zk.bin");
    let issue_v2_bincode = include_bytes!("../../proof/issue.zk.bin");
    let revoke_v2_bincode = include_bytes!("../../proof/revoke.zk.bin");
    let transfer_v2_bincode = include_bytes!("../../proof/transfer.zk.bin");
    let redeem_v2_bincode = include_bytes!("../../proof/redeem.zk.bin");
    wasm::db::zkas_db_set(&register_type_v2_bincode[..])?;
    wasm::db::zkas_db_set(&issue_v2_bincode[..])?;
    wasm::db::zkas_db_set(&revoke_v2_bincode[..])?;
    wasm::db::zkas_db_set(&transfer_v2_bincode[..])?;
    wasm::db::zkas_db_set(&redeem_v2_bincode[..])?;

    let tx_hash = wasm::util::get_tx_hash()?;
    let call_idx = wasm::util::get_call_index()?;
    let mut roots_value_data = Vec::with_capacity(32 + 1);
    tx_hash.encode(&mut roots_value_data)?;
    call_idx.encode(&mut roots_value_data)?;
    if roots_value_data.len() != 32 + 1 {
        msg!(
            "[promissory_note::init_contract] Error: Roots value data length is not expected (32 + 1): {}",
            roots_value_data.len()
        );
        return Err(PromissoryNoteError::RootsValueDataMismatch.into())
    }

    // Set up coin roots database
    if wasm::db::db_lookup(cid, PROMISSORY_NOTE_CONTRACT_COIN_ROOTS_TREE).is_err() {
        let db_coin_roots = wasm::db::db_init(cid, PROMISSORY_NOTE_CONTRACT_COIN_ROOTS_TREE)?;
        wasm::db::db_set(db_coin_roots, &EMPTY_COINS_TREE_ROOT, &roots_value_data)?;
    }

    // Set up nullifier roots database
    if wasm::db::db_lookup(cid, PROMISSORY_NOTE_CONTRACT_NULLIFIER_ROOTS_TREE).is_err() {
        let db_null_roots = wasm::db::db_init(cid, PROMISSORY_NOTE_CONTRACT_NULLIFIER_ROOTS_TREE)?;
        wasm::db::db_set(
            db_null_roots,
            &pallas::Base::zero().to_repr(),
            &roots_value_data,
        )?;
    }

    // Set up coins database
    if wasm::db::db_lookup(cid, PROMISSORY_NOTE_CONTRACT_COINS_TREE).is_err() {
        wasm::db::db_init(cid, PROMISSORY_NOTE_CONTRACT_COINS_TREE)?;
    }

    // Set up nullifiers database
    if wasm::db::db_lookup(cid, PROMISSORY_NOTE_CONTRACT_NULLIFIERS_TREE).is_err() {
        wasm::db::db_init(cid, PROMISSORY_NOTE_CONTRACT_NULLIFIERS_TREE)?;
    }

    // Set up token registry database
    if wasm::db::db_lookup(cid, PROMISSORY_NOTE_CONTRACT_TOKEN_REGISTRY_TREE).is_err() {
        wasm::db::db_init(cid, PROMISSORY_NOTE_CONTRACT_TOKEN_REGISTRY_TREE)?;
    }

    // Set up token registry roots database
    if wasm::db::db_lookup(cid, PROMISSORY_NOTE_CONTRACT_TOKEN_REGISTRY_ROOTS_TREE).is_err() {
        let db_token_registry_roots = wasm::db::db_init(cid, PROMISSORY_NOTE_CONTRACT_TOKEN_REGISTRY_ROOTS_TREE)?;
        wasm::db::db_set(
            db_token_registry_roots,
            &EMPTY_TOKEN_REGISTRY_TREE_ROOT,
            &roots_value_data,
        )?;
    }

    // Set up info database (always resolve handle, init if needed)
    let info_db = match wasm::db::db_lookup(cid, PROMISSORY_NOTE_CONTRACT_INFO_TREE) {
        Ok(v) => v,
        Err(_) => wasm::db::db_init(cid, PROMISSORY_NOTE_CONTRACT_INFO_TREE)?,
    };

    // Initialize Merkle trees if not already present (defense in depth:
    // tree may not exist even if info_db handle resolves)
    if !wasm::db::db_contains_key(info_db, PROMISSORY_NOTE_CONTRACT_COIN_MERKLE_TREE)? {

            // Create Merkle tree for coins
            let mut coin_tree = MerkleTree::new(1);
            coin_tree.append(MerkleNode::from_base(pallas::Base::ZERO));
            let mut coin_tree_data = vec![];
            coin_tree_data.write_u32(0)?;
            coin_tree.encode(&mut coin_tree_data)?;
            wasm::db::db_set(info_db, PROMISSORY_NOTE_CONTRACT_COIN_MERKLE_TREE, &coin_tree_data)?;

            // Create Merkle tree for token registry
            let mut token_registry_tree = MerkleTree::new(1);
            token_registry_tree.append(MerkleNode::from_base(pallas::Base::ZERO));
            let mut token_registry_tree_data = vec![];
            token_registry_tree_data.write_u32(0)?;
            token_registry_tree.encode(&mut token_registry_tree_data)?;
            wasm::db::db_set(
                info_db,
                PROMISSORY_NOTE_CONTRACT_TOKEN_REGISTRY_MERKLE_TREE,
                &token_registry_tree_data,
            )?;

            // Initialize latest roots
            wasm::db::db_set(
                info_db,
                PROMISSORY_NOTE_CONTRACT_LATEST_COIN_ROOT,
                &EMPTY_COINS_TREE_ROOT,
            )?;
            wasm::db::db_set(
                info_db,
                PROMISSORY_NOTE_CONTRACT_LATEST_NULLIFIER_ROOT,
                &pallas::Base::zero().to_repr(),
            )?;
            wasm::db::db_set(
                info_db,
                PROMISSORY_NOTE_CONTRACT_LATEST_TOKEN_REGISTRY_ROOT,
                &EMPTY_TOKEN_REGISTRY_TREE_ROOT,
            )?;
    }

    wasm::db::db_set(info_db, PROMISSORY_NOTE_CONTRACT_DB_VERSION, env!("CARGO_PKG_VERSION").as_bytes())?;

    msg!("[promissory_note::init_contract] Database trees initialized");
    Ok(())
}

// ============================================================================
// METADATA (ZK PROOF SETUP)
// ============================================================================

fn get_metadata(cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = PromissoryNoteFunction::try_from(self_.data[0])?;

    let metadata = match func {
        PromissoryNoteFunction::RegisterTypeV1 => register_type_get_metadata(cid, call_idx, calls),
        PromissoryNoteFunction::RedeemV1 => redeem_get_metadata(cid, call_idx, calls),
        PromissoryNoteFunction::IssueV1 => issue_get_metadata(cid, call_idx, calls),
        PromissoryNoteFunction::RevokeV1 => revoke_get_metadata(cid, call_idx, calls),
        PromissoryNoteFunction::TransferV1 => transfer_get_metadata(cid, call_idx, calls),
        PromissoryNoteFunction::OtcSwapV1 => otc_swap_get_metadata(cid, call_idx, calls),
    }?;

    wasm::util::set_return_data(&metadata)
}

/// Extract (x, y) base-field coordinates from a pallas::Point for ZK public inputs.
fn point_coords(pt: pallas::Point) -> (pallas::Base, pallas::Base) {
    let affine = pt.to_affine();
    let coords = affine.coordinates().expect("point_coords: identity point — ZK circuit must constrain non-identity for value commitments");
    (*coords.x(), *coords.y())
}

/// Metadata for RegisterTypeV1
/// Circuit instances: token_id, token_auth_parent, coin, value_commit_x, value_commit_y
fn register_type_get_metadata(_cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx];
    let params= match RegisterTypeParamsV1::decode(&self_.data.data[1..]) { Ok(p) => p, Err(_) => return Ok(vec![]) };

    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    let signature_pubkeys: Vec<pallas::Base> = vec![];

    let (vc_x, vc_y) = point_coords(params.value_commit);

    zk_public_inputs.push((
        PROMISSORY_NOTE_CONTRACT_ZKAS_REGISTER_TYPE_NS_V2.to_string(),
        vec![
            params.token_id.inner(),
            params.token_auth_parent,
            params.commitment.inner(),
            vc_x,
            vc_y,
            params.spend_hook.inner(),
            params.tx_binding,
            params.tx_nonce,
        ],
    ));

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    signature_pubkeys.encode(&mut metadata)?;
    Ok(metadata)
}

/// Metadata for IssueV1
/// Circuit instances: token_root, issue_public, coin, value_commit_x, value_commit_y, token_id, spend_hook
fn issue_get_metadata(_cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params= match IssueParamsV1::decode(&self_.data[1..]) { Ok(p) => p, Err(_) => return Ok(vec![]) };

    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    // Schnorr signatures prohibited (contract-standards.md §3).
    let signature_pubkeys: Vec<pallas::Base> = vec![];

    let (vc_x, vc_y) = point_coords(params.value_commit);

    // IssueV1 circuit expects: token_root, issue_public, coin, vc_x, vc_y, token_id,
    //                          spend_hook, tx_binding, tx_nonce
    zk_public_inputs.push((
        PROMISSORY_NOTE_CONTRACT_ZKAS_ISSUE_NS_V2.to_string(),
        vec![
            params.token_registry_root.inner(),
            params.issue_public,
            params.commitment.inner(),
            vc_x,
            vc_y,
            params.token_id.inner(),
            params.spend_hook.inner(),
            params.tx_binding,
            params.tx_nonce,
        ],
    ));

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    signature_pubkeys.encode(&mut metadata)?;
    Ok(metadata)
}

/// Metadata for RevokeV1
/// Circuit instances: nullifier, value_commit_x, value_commit_y, token_commit,
///                     merkle_root, user_data_enc, spend_hook, signature_public
fn revoke_get_metadata(_cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params = match RevokeParamsV1::decode(&self_.data[1..]) {
        Ok(p) => p,
        Err(_) => return Ok(vec![]),
    };

    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    // Schnorr signatures prohibited (contract-standards.md §3).
    let signature_pubkeys: Vec<pallas::Base> = vec![];

    for input in &params.inputs {

        let (vc_x, vc_y) = point_coords(input.value_commit);

        zk_public_inputs.push((
            PROMISSORY_NOTE_CONTRACT_ZKAS_REVOKE_NS_V2.to_string(),
            vec![
                input.nullifier.inner(),
                vc_x,
                vc_y,
                input.token_commit,
                input.merkle_root.inner(),
                input.user_data_enc,
                input.spend_hook.inner(),
                input.signature_public,
                params.tx_binding,
                params.tx_nonce,
            ],
        ));
    }

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    signature_pubkeys.encode(&mut metadata)?;
    Ok(metadata)
}

/// Metadata for TransferV1 (atomic burn + blind output)
/// Burn instances: nullifier, value_commit_x, value_commit_y, token_commit,
///                  merkle_root, user_data_enc, spend_hook, signature_public
/// BlindOutput instances: coin, value_commit_x, value_commit_y, token_commit, spend_hook
fn transfer_get_metadata(_cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params= match TransferParamsV1::decode(&self_.data[1..]) { Ok(p) => p, Err(_) => return Ok(vec![]) };

    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    // Schnorr signatures prohibited (contract-standards.md §3).
    let signature_pubkeys: Vec<pallas::Base> = vec![];

    // Burn proofs (one per input)
    for input in &params.inputs {

        let (vc_x, vc_y) = point_coords(input.value_commit);

        zk_public_inputs.push((
            PROMISSORY_NOTE_CONTRACT_ZKAS_REVOKE_NS_V2.to_string(),
            vec![
                input.nullifier.inner(),
                vc_x,
                vc_y,
                input.token_commit,
                input.merkle_root.inner(),
                input.user_data_enc,
                input.spend_hook.inner(),
                input.signature_public,
                params.tx_binding,
                params.tx_nonce,
            ],
        ));
    }

    // BlindOutput proofs (one per output) — includes spend_hook, tx_binding, tx_nonce
    for output in &params.outputs {
        let (vc_x, vc_y) = point_coords(output.value_commit);

        zk_public_inputs.push((
            PROMISSORY_NOTE_CONTRACT_ZKAS_TRANSFER_NS_V2.to_string(),
            vec![output.commitment.inner(), vc_x, vc_y, output.token_commit,
                 output.spend_hook.inner(), params.tx_binding, params.tx_nonce],
        ));
    }

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    signature_pubkeys.encode(&mut metadata)?;
    Ok(metadata)
}

// ============================================================================
// INSTRUCTION PROCESSING (STATE TRANSITION VERIFICATION)
// ============================================================================

fn process_instruction(cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = PromissoryNoteFunction::try_from(self_.data[0])?;

    match func {
        PromissoryNoteFunction::RegisterTypeV1 => register_type_v1(cid, call_idx, calls),
        PromissoryNoteFunction::RedeemV1 => redeem_v1(cid, call_idx, calls),
        PromissoryNoteFunction::IssueV1 => issue_v1(cid, call_idx, calls),
        PromissoryNoteFunction::RevokeV1 => revoke_v1(cid, call_idx, calls),
        PromissoryNoteFunction::TransferV1 => transfer_v1(cid, call_idx, calls),
        PromissoryNoteFunction::OtcSwapV1 => otc_swap_v1(cid, call_idx, calls),
    }
}

// ============================================================================
// VALUE CONSERVATION HELPERS
// ============================================================================

/// Verify per-token-commit value conservation: for each token_commit group,
/// sum(input Pedersen value_commits) == sum(output Pedersen value_commits).
///
/// Uses the additive homomorphism of Pedersen commitments: C(v1,b1) + C(v2,b2) = C(v1+v2,b1+b2).
/// Without this check, a prover with one coin of value 1 could burn it
/// and create a new coin of value 1,000,000 — both proofs verify independently
/// but the sums wouldn't match.
///
/// The check groups inputs and outputs by their `token_commit` (ZK-constrained
/// in both RevokeV1 and TransferV1).  This prevents value from crossing token
/// types and enforces conservation per token type.
fn verify_value_conservation(inputs: &[crate::model::Input], outputs: &[crate::model::Output]) -> ContractResult {
    use dwow_sdk::pasta::pallas;

    // Build per-token-commit sums using linear scan (transfer/OTC have ~1-4 entries).
    // Keyed by token_commit bytes for reliable comparison in the map.
    let mut input_sums: Vec<(pallas::Base, pallas::Point)> = Vec::new();
    for input in inputs {
        match input_sums.iter_mut().find(|(tc, _)| *tc == input.token_commit) {
            Some((_, sum)) => *sum = *sum + input.value_commit,
            None => input_sums.push((input.token_commit, input.value_commit)),
        }
    }

    let mut output_sums: Vec<(pallas::Base, pallas::Point)> = Vec::new();
    for output in outputs {
        match output_sums.iter_mut().find(|(tc, _)| *tc == output.token_commit) {
            Some((_, sum)) => *sum = *sum + output.value_commit,
            None => output_sums.push((output.token_commit, output.value_commit)),
        }
    }

    // Every token_commit present in inputs must have a matching sum in outputs.
    for (token_commit, input_sum) in &input_sums {
        match output_sums.iter().find(|(tc, _)| tc == token_commit) {
            Some((_, output_sum)) if *output_sum == *input_sum => {},
            _ => {
                msg!("[promissory_note] Error: Value conservation failed for token_commit {:?}", token_commit.to_repr());
                return Err(PromissoryNoteError::ValueMismatch.into())
            }
        }
    }

    // No extra token types in outputs.
    for (token_commit, _) in &output_sums {
        if !input_sums.iter().any(|(tc, _)| tc == token_commit) {
            msg!("[promissory_note] Error: Output token_commit not present in inputs {:?}", token_commit.to_repr());
            return Err(PromissoryNoteError::ValueMismatch.into())
        }
    }

    Ok(())
}

// ============================================================================
// TOKEN MINT - Create a new token type (stablecoin, wrapped, etc.)
// ============================================================================

fn register_type_v1(cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params= RegisterTypeParamsV1::decode(&self_.data[1..])?;
    msg!("[promissory_note::register_type_v1] Creating new token type");

    let coins_db = wasm::db::db_lookup(cid, PROMISSORY_NOTE_CONTRACT_COINS_TREE)?;

    // Verify coin doesn't already exist
    if wasm::db::db_contains_key(coins_db, &params.commitment.to_bytes())? {
        msg!("[register_type_v1] Error: Coin already exists");
        return Err(PromissoryNoteError::DuplicateCoin.into())
    }

    // HAZOP P-1 fix: reject duplicate capability namespace registration.
    // Previously apply_register_type unconditionally db_set the token_registry_db
    // entry, overwriting the stored backing capability commitment. Defense-in-depth
    // against accidental blind reuse or client bugs — a deliberate collision
    // requires breaking Poseidon's collision resistance.
    let token_registry_db = wasm::db::db_lookup(cid, PROMISSORY_NOTE_CONTRACT_TOKEN_REGISTRY_TREE)?;
    if wasm::db::db_contains_key(token_registry_db, &params.token_id.to_bytes())? {
        msg!("[register_type_v1] Error: Capability namespace already registered");
        return Err(PromissoryNoteError::DuplicateCoin.into())
    }

    let update = RegisterTypeUpdateV1 { token_id: params.token_id, commitment: params.commitment, token_auth_parent: params.token_auth_parent };
    msg!("[promissory_note::register_type_v1] Token type created successfully");
    wasm::util::set_return_data(&[&[PromissoryNoteFunction::RegisterTypeV1 as u8], &update.encode()[..]].concat())
}

// ============================================================================
// MINT - Mint tokens of existing token type (proves backing capability)
// ============================================================================

fn issue_v1(cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params= IssueParamsV1::decode(&self_.data[1..])?;
    msg!("[promissory_note::issue_v1] Minting tokens");

    let coins_db = wasm::db::db_lookup(cid, PROMISSORY_NOTE_CONTRACT_COINS_TREE)?;
    let token_registry_db = wasm::db::db_lookup(cid, PROMISSORY_NOTE_CONTRACT_TOKEN_REGISTRY_TREE)?;

    // Verify coin doesn't already exist
    if wasm::db::db_contains_key(coins_db, &params.commitment.to_bytes())? {
        msg!("[issue_v1] Error: Coin already exists");
        return Err(PromissoryNoteError::DuplicateCoin.into())
    }

    // Verify token_id exists in token registry (must be created via RegisterTypeV1)
    if !wasm::db::db_contains_key(token_registry_db, &params.token_id.to_bytes())? {
        msg!("[issue_v1] Error: Token not registered");
        return Err(PromissoryNoteError::TokenNotRegistered.into())
    }

    // Verify mint authority: the prover must know the backing secret whose hash
    // matches the stored token_auth_parent from RegisterTypeV1. Without this check,
    // anyone with ANY valid IssueV1 proof can mint ANY registered token.
    let stored_auth_bytes = wasm::db::db_get(token_registry_db, &params.token_id.to_bytes())?
        .ok_or(PromissoryNoteError::TokenNotRegistered)?;
    let stored_auth = Option::<pallas::Base>::from(pallas::Base::from_repr(
        stored_auth_bytes.try_into().map_err(|_| ContractError::IoError("Corrupt state: token_auth_parent wrong size".into()))?,
    )).ok_or_else(|| ContractError::IoError("Corrupt state: invalid token_auth_parent".into()))?;
    if params.issue_public != stored_auth {
        msg!("[issue_v1] Error: Mint authority mismatch — issue_public does not match stored token_auth_parent");
        return Err(PromissoryNoteError::InvalidIssueAuthority.into())
    }

    // Verify token_registry_root matches the current on-chain registry root.
    // Without this check, an old root could be replayed after the registry has changed.
    let info_db = wasm::db::db_lookup(cid, PROMISSORY_NOTE_CONTRACT_INFO_TREE)?;
    let current_root_bytes = wasm::db::db_get(info_db, PROMISSORY_NOTE_CONTRACT_LATEST_TOKEN_REGISTRY_ROOT)?
        .ok_or(PromissoryNoteError::TokenNotRegistered)?;
    let current_root = MerkleNode::from_bytes(
        current_root_bytes.try_into().map_err(|_| ContractError::IoError("Corrupt state: LATEST_TOKEN_REGISTRY_ROOT wrong size".into()))?,
    ).ok_or_else(|| ContractError::IoError("Corrupt state: invalid LATEST_TOKEN_REGISTRY_ROOT".into()))?;
    if params.token_registry_root != current_root {
        msg!("[issue_v1] Error: Token registry root mismatch (stale or replayed proof)");
        return Err(PromissoryNoteError::TokenNotRegistered.into())
    }

    // Track total coin count for this token (infinity-mint hardening).
    // Values are hidden behind Pedersen commitments so we track coin count,
    // not value supply.  An off-chain auditor can compare on-chain coin
    // counts with expected issuance per token type.
    let info_db = wasm::db::db_lookup(cid, PROMISSORY_NOTE_CONTRACT_INFO_TREE)?;
    let mut supply_key = PROMISSORY_NOTE_CONTRACT_TOTAL_SUPPLY.to_vec();
    supply_key.extend_from_slice(&params.token_id.to_bytes());
    let current_count: u64 = match wasm::db::db_get(info_db, &supply_key)? {
        Some(data) => {
            let data_len = data.len();
            let bytes: [u8; 8] = data.try_into().map_err(|_| {
                msg!("[promissory_note::issue_v1] Error: Corrupt state — coin count wrong size: {}", data_len);
                ContractError::IoError("Corrupt state: coin count wrong size".to_string())
            })?;
            u64::from_le_bytes(bytes)
        },
        None => {
            // First mint for this token type — zero coins before first issue.
            0
        }
    };
    let new_coin_count = current_count.saturating_add(1);

    let update = IssueUpdateV1 {
        commitment: params.commitment,
        token_id: params.token_id,
        new_coin_count,
    };
    msg!("[promissory_note::issue_v1] Mint valid");
    wasm::util::set_return_data(&[&[PromissoryNoteFunction::IssueV1 as u8], &update.encode()[..]].concat())
}

// ============================================================================
// BURN - Destroy tokens
// ============================================================================

fn revoke_v1(cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params= RevokeParamsV1::decode(&self_.data[1..])?;
    msg!("[promissory_note::revoke_v1] Processing burn: {} inputs", params.inputs.len());

    if params.inputs.is_empty() {
        return Err(PromissoryNoteError::RevokeMissingInputs.into())
    }

    // Zero-value burn is rejected at the circuit level: revoke_v1.zk constrains
    // `less_than_strict(ZERO, coin_value)`, so a zero-value coin cannot produce a
    // valid Merkle proof (zero_cond would select the empty leaf). No entrypoint
    // value check is required — the circuit enforces coin_value > 0.

    let coin_roots_db = wasm::db::db_lookup(cid, PROMISSORY_NOTE_CONTRACT_COIN_ROOTS_TREE)?;
    let nullifiers_db = wasm::db::db_lookup(cid, PROMISSORY_NOTE_CONTRACT_NULLIFIERS_TREE)?;

    let mut new_nullifiers = Vec::new();
    for (i, input) in params.inputs.iter().enumerate() {
        // Verify Merkle root exists
        if !wasm::db::db_contains_key(coin_roots_db, &input.merkle_root.to_bytes())? {
            msg!("[revoke_v1] Error: Merkle root not found for input {}", i);
            return Err(PromissoryNoteError::TransferMerkleRootNotFound.into())
        }

        // Verify nullifier is NOT already spent
        if wasm::db::db_contains_key(nullifiers_db, &input.nullifier.to_bytes())? {
            msg!("[revoke_v1] Error: Nullifier already spent for input {}", i);
            return Err(PromissoryNoteError::DuplicateNullifier.into())
        }

        new_nullifiers.push(input.nullifier);
    }

    // Spend hook callback — if the first input has a non-zero spend_hook, all
    // inputs must share the same spend_hook and we dispatch a callback to the
    // target contract after this exec() succeeds.
    let spend_hook = params.inputs[0].spend_hook;
    if spend_hook.inner() != pallas::Base::zero() {
        for input in &params.inputs[1..] {
            if input.spend_hook != spend_hook {
                msg!("[revoke_v1] Error: Spend hook mismatch in inputs");
                return Err(PromissoryNoteError::SpendHookMismatch.into())
            }
        }

        let target_cid_bytes: [u8; 32] = spend_hook.inner().to_repr();
        let target_cid = ContractId::from_bytes(target_cid_bytes)
            .map_err(|_| PromissoryNoteError::InvalidChildContractId)?;

        let payload = RevokeSpendHookPayload {
            caller_contract_id: cid,
            nullifiers: new_nullifiers.iter().map(|n| n.inner()).collect(),
            token_commits: params.inputs.iter().map(|i| i.token_commit).collect(),
            value_commits: params.inputs.iter().map(|i| i.value_commit).collect(),
            user_data_encs: params.inputs.iter().map(|i| i.user_data_enc).collect(),
        };

        let payload_bytes = payload.encode();
        wasm::util::emit_spend_hook(&target_cid, &payload_bytes)?;
    }

    let update = RevokeUpdateV1 { nullifiers: new_nullifiers };
    msg!("[promissory_note::revoke_v1] Burn valid");
    wasm::util::set_return_data(&[&[PromissoryNoteFunction::RevokeV1 as u8], &update.encode()[..]].concat())
}

// ============================================================================
// TRANSFER - Private token transfer
// ============================================================================

fn transfer_v1(cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params= TransferParamsV1::decode(&self_.data[1..])?;
    msg!(
        "[promissory_note::transfer_v1] Processing transfer: {} inputs, {} outputs",
        params.inputs.len(),
        params.outputs.len()
    );

    if params.inputs.is_empty() {
        return Err(PromissoryNoteError::TransferMissingInputs.into())
    }
    if params.outputs.is_empty() {
        return Err(PromissoryNoteError::TransferMissingOutputs.into())
    }

    let coins_db = wasm::db::db_lookup(cid, PROMISSORY_NOTE_CONTRACT_COINS_TREE)?;
    let coin_roots_db = wasm::db::db_lookup(cid, PROMISSORY_NOTE_CONTRACT_COIN_ROOTS_TREE)?;
    let nullifiers_db = wasm::db::db_lookup(cid, PROMISSORY_NOTE_CONTRACT_NULLIFIERS_TREE)?;

    // Verify all input nullifiers are unique and not already spent
    let mut new_nullifiers = Vec::new();
    for (i, input) in params.inputs.iter().enumerate() {
        // Check Merkle root exists
        if !wasm::db::db_contains_key(coin_roots_db, &input.merkle_root.to_bytes())? {
            msg!("[transfer_v1] Error: Merkle root not found for input {}", i);
            return Err(PromissoryNoteError::TransferMerkleRootNotFound.into())
        }

        // Verify nullifier is NOT already spent
        if wasm::db::db_contains_key(nullifiers_db, &input.nullifier.to_bytes())? {
            msg!("[transfer_v1] Error: Nullifier already spent for input {}", i);
            return Err(PromissoryNoteError::DuplicateNullifier.into())
        }

        new_nullifiers.push(input.nullifier);
    }

    // Verify outputs are unique
    let mut new_commitments = Vec::new();
    for (i, output) in params.outputs.iter().enumerate() {
        if wasm::db::db_contains_key(coins_db, &output.commitment.to_bytes())? {
            msg!("[transfer_v1] Error: Duplicate coin in output {}", i);
            return Err(PromissoryNoteError::DuplicateCoin.into())
        }
        new_commitments.push(output.commitment);
    }

    // CROSS-PROOF VALUE CONSERVATION: sum(inputs) == sum(outputs) per token_commit.
    // This prevents value inflation — a prover with one coin of value 1 could
    // otherwise burn it and create a new coin of value 1,000,000 with both
    // proofs verifying independently. Pedersen's additive homomorphism makes
    // this check possible without revealing plaintext values.
    verify_value_conservation(&params.inputs, &params.outputs)?;

    let update = TransferUpdateV1 { nullifiers: new_nullifiers, commitments: new_commitments };
    msg!("[promissory_note::transfer_v1] Transfer valid");
    wasm::util::set_return_data(&[&[PromissoryNoteFunction::TransferV1 as u8], &update.encode()[..]].concat())
}

// ============================================================================
// STATE UPDATE (WRITE STATE AFTER VERIFICATION)
// ============================================================================

fn process_update(cid: ContractId, update_data: &[u8]) -> ContractResult {
    let func = PromissoryNoteFunction::try_from(update_data[0])?;

    match func {
        PromissoryNoteFunction::RegisterTypeV1 => {
            let update = RegisterTypeUpdateV1::decode(&update_data[1..])?;
            apply_register_type(cid, update)
        }
        PromissoryNoteFunction::RedeemV1 => {
            let update = RedeemUpdateV1::decode(&update_data[1..])?;
            apply_redeem(cid, update)
        }
        PromissoryNoteFunction::IssueV1 => {
            let update = IssueUpdateV1::decode(&update_data[1..])?;
            apply_issue(cid, update)
        }
        PromissoryNoteFunction::RevokeV1 => {
            let update = RevokeUpdateV1::decode(&update_data[1..])?;
            apply_revoke(cid, update)
        }
        PromissoryNoteFunction::TransferV1 => {
            let update = TransferUpdateV1::decode(&update_data[1..])?;
            apply_transfer(cid, update)
        }
        PromissoryNoteFunction::OtcSwapV1 => {
            let update = OtcSwapUpdateV1::decode(&update_data[1..])?;
            apply_otc_swap(cid, update)
        }
    }
}

fn apply_register_type(cid: ContractId, update: RegisterTypeUpdateV1) -> ContractResult {
    msg!("[promissory_note::apply_register_type] Adding coin and registering token");

    let coins_db = wasm::db::db_lookup(cid, PROMISSORY_NOTE_CONTRACT_COINS_TREE)?;
    let info_db = wasm::db::db_lookup(cid, PROMISSORY_NOTE_CONTRACT_INFO_TREE)?;
    let token_registry_db = wasm::db::db_lookup(cid, PROMISSORY_NOTE_CONTRACT_TOKEN_REGISTRY_TREE)?;

    // Add coin
    wasm::db::db_set(coins_db, &update.commitment.to_bytes(), &[1])?;

    // Update coin Merkle tree
    wasm::merkle::merkle_add(
        info_db,
        wasm::db::db_lookup(cid, PROMISSORY_NOTE_CONTRACT_COIN_ROOTS_TREE)?,
        PROMISSORY_NOTE_CONTRACT_LATEST_COIN_ROOT,
        PROMISSORY_NOTE_CONTRACT_COIN_MERKLE_TREE,
        &[MerkleNode::from_base(update.commitment.inner())],
    )?;

    // Store token authority key in registry (capability datum for rotation)
    wasm::db::db_set(token_registry_db, &update.token_id.to_bytes(), &update.token_auth_parent.to_repr())?;

    // Update token registry Merkle tree
    wasm::merkle::merkle_add(
        info_db,
        wasm::db::db_lookup(cid, PROMISSORY_NOTE_CONTRACT_TOKEN_REGISTRY_ROOTS_TREE)?,
        PROMISSORY_NOTE_CONTRACT_LATEST_TOKEN_REGISTRY_ROOT,
        PROMISSORY_NOTE_CONTRACT_TOKEN_REGISTRY_MERKLE_TREE,
        &[MerkleNode::from_base(update.token_id.inner())],
    )?;

    // Initialize coin count for this token type (infinity-mint hardening).
    // RegisterTypeV1 creates the initial coin, so count starts at 1.
    let mut supply_key = PROMISSORY_NOTE_CONTRACT_TOTAL_SUPPLY.to_vec();
    supply_key.extend_from_slice(&update.token_id.to_bytes());
    wasm::db::db_set(info_db, &supply_key, &1u64.to_le_bytes())?;

    Ok(())
}

fn apply_issue(cid: ContractId, update: IssueUpdateV1) -> ContractResult {
    msg!("[promissory_note::apply_issue] Adding coin to state");
    let coins_db = wasm::db::db_lookup(cid, PROMISSORY_NOTE_CONTRACT_COINS_TREE)?;
    let info_db = wasm::db::db_lookup(cid, PROMISSORY_NOTE_CONTRACT_INFO_TREE)?;

    // Add coin
    wasm::db::db_set(coins_db, &update.commitment.to_bytes(), &[1])?;

    // Update Merkle tree
    wasm::merkle::merkle_add(
        info_db,
        wasm::db::db_lookup(cid, PROMISSORY_NOTE_CONTRACT_COIN_ROOTS_TREE)?,
        PROMISSORY_NOTE_CONTRACT_LATEST_COIN_ROOT,
        PROMISSORY_NOTE_CONTRACT_COIN_MERKLE_TREE,
        &[MerkleNode::from_base(update.commitment.inner())],
    )?;

    // Persist updated coin count for this token (infinity-mint hardening)
    let mut supply_key = PROMISSORY_NOTE_CONTRACT_TOTAL_SUPPLY.to_vec();
    supply_key.extend_from_slice(&update.token_id.to_bytes());
    wasm::db::db_set(info_db, &supply_key, &update.new_coin_count.to_le_bytes())?;

    Ok(())
}

fn apply_revoke(cid: ContractId, update: RevokeUpdateV1) -> ContractResult {
    msg!("[promissory_note::apply_revoke] Marking {} nullifiers", update.nullifiers.len());
    let nullifiers_db = wasm::db::db_lookup(cid, PROMISSORY_NOTE_CONTRACT_NULLIFIERS_TREE)?;

    // Mark all burn nullifiers as spent (flat marker, not SMT)
    for n in &update.nullifiers {
        wasm::db::db_mark_spent(nullifiers_db, &n.to_bytes())?;
    }

    Ok(())
}

fn apply_transfer(cid: ContractId, update: TransferUpdateV1) -> ContractResult {
    msg!(
        "[promissory_note::apply_transfer] Marking {} nullifiers, adding {} coins",
        update.nullifiers.len(),
        update.commitments.len()
    );

    let coins_db = wasm::db::db_lookup(cid, PROMISSORY_NOTE_CONTRACT_COINS_TREE)?;
    let nullifiers_db = wasm::db::db_lookup(cid, PROMISSORY_NOTE_CONTRACT_NULLIFIERS_TREE)?;
    let info_db = wasm::db::db_lookup(cid, PROMISSORY_NOTE_CONTRACT_INFO_TREE)?;

    // Mark nullifiers (coins spent) as flat markers, not SMT
    for n in &update.nullifiers {
        wasm::db::db_mark_spent(nullifiers_db, &n.to_bytes())?;
    }

    // Add new coins
    let mut new_commitments = Vec::new();
    for commitment in &update.commitments {
        wasm::db::db_set(coins_db, &commitment.to_bytes(), &[1])?;
        new_commitments.push(MerkleNode::from_base(commitment.inner()));
    }

    // Update Merkle tree
    wasm::merkle::merkle_add(
        info_db,
        wasm::db::db_lookup(cid, PROMISSORY_NOTE_CONTRACT_COIN_ROOTS_TREE)?,
        PROMISSORY_NOTE_CONTRACT_LATEST_COIN_ROOT,
        PROMISSORY_NOTE_CONTRACT_COIN_MERKLE_TREE,
        &new_commitments,
    )?;

    Ok(())
}

// ============================================================================
// REDEEM - Redeem a coin, destroying monetary value, creating a receipt
// ============================================================================

/// Metadata for RedeemV1 (burn + zero-value receipt)
/// Burn instance: nullifier, value_commit_x, value_commit_y, token_commit,
///                 merkle_root, user_data_enc, spend_hook, signature_public
/// Redeem instance: coin, value_commit_x, value_commit_y, token_commit, coin_value, spend_hook
/// The entrypoint sets coin_value = 0; the circuit constrains it as a public input.
fn redeem_get_metadata(_cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params= match RedeemParamsV1::decode(&self_.data[1..]) { Ok(p) => p, Err(_) => return Ok(vec![]) };

    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    // Schnorr signatures prohibited (contract-standards.md §3).
    let signature_pubkeys: Vec<pallas::Base> = vec![];

    // Burn proof for the input coin being redeemed

    let (vc_x, vc_y) = point_coords(params.input.value_commit);

    zk_public_inputs.push((
        PROMISSORY_NOTE_CONTRACT_ZKAS_REVOKE_NS_V2.to_string(),
        vec![
            params.input.nullifier.inner(),
            vc_x,
            vc_y,
            params.input.token_commit,
            params.input.merkle_root.inner(),
            params.input.user_data_enc,
            params.input.spend_hook.inner(),
            params.input.signature_public,
            params.tx_binding,
            params.tx_nonce,
        ],
    ));

    // Redeem_V1 proof for the receipt coin (value=0).
    // Public input order: coin, vc_x, vc_y, token_commit, coin_value,
    //                      tx_binding, tx_nonce, spend_hook
    let coin_value = pallas::Base::zero();
    let (rvc_x, rvc_y) = point_coords(params.output.value_commit);

    zk_public_inputs.push((
        PROMISSORY_NOTE_CONTRACT_ZKAS_REDEEM_NS_V2.to_string(),
        vec![params.output.commitment.inner(), rvc_x, rvc_y, params.output.token_commit,
             coin_value, params.tx_binding, params.tx_nonce, params.output.spend_hook.inner()],
    ));

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    signature_pubkeys.encode(&mut metadata)?;
    Ok(metadata)
}

/// RedeemV1 instruction — burns the input coin and creates a zero-value receipt.
///
/// Redemption IS value destruction: the input coin's value is destroyed from
/// circulation and the issuer fulfills the promise by releasing the underlying
/// asset. Value conservation is deliberately NOT enforced here.
///
/// Checks:
/// 1. Merkle root exists (coin existed)
/// 2. Nullifier is unspent (no double-spend)
/// 3. Receipt coin is unique
fn redeem_v1(cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params= RedeemParamsV1::decode(&self_.data[1..])?;
    msg!("[promissory_note::redeem_v1] Processing redemption");

    let coins_db = wasm::db::db_lookup(cid, PROMISSORY_NOTE_CONTRACT_COINS_TREE)?;
    let coin_roots_db = wasm::db::db_lookup(cid, PROMISSORY_NOTE_CONTRACT_COIN_ROOTS_TREE)?;
    let nullifiers_db = wasm::db::db_lookup(cid, PROMISSORY_NOTE_CONTRACT_NULLIFIERS_TREE)?;

    // Verify Merkle root exists
    if !wasm::db::db_contains_key(coin_roots_db, &params.input.merkle_root.to_bytes())? {
        msg!("[redeem_v1] Error: Merkle root not found");
        return Err(PromissoryNoteError::TransferMerkleRootNotFound.into())
    }

    // Verify nullifier is NOT already spent
    if wasm::db::db_contains_key(nullifiers_db, &params.input.nullifier.to_bytes())? {
        msg!("[redeem_v1] Error: Nullifier already spent");
        return Err(PromissoryNoteError::DuplicateNullifier.into())
    }

    // Verify receipt coin is unique
    if wasm::db::db_contains_key(coins_db, &params.output.commitment.to_bytes())? {
        msg!("[redeem_v1] Error: Receipt coin already exists");
        return Err(PromissoryNoteError::DuplicateCoin.into())
    }

    let update = RedeemUpdateV1 { nullifier: params.input.nullifier, commitment: params.output.commitment };
    msg!("[promissory_note::redeem_v1] Redemption valid");
    wasm::util::set_return_data(&[&[PromissoryNoteFunction::RedeemV1 as u8], &update.encode()[..]].concat())
}

/// Apply RedeemV1 state update — mark the nullifier and add the receipt coin.
fn apply_redeem(cid: ContractId, update: RedeemUpdateV1) -> ContractResult {
    msg!("[promissory_note::apply_redeem] Marking nullifier, adding receipt coin");

    let coins_db = wasm::db::db_lookup(cid, PROMISSORY_NOTE_CONTRACT_COINS_TREE)?;
    let nullifiers_db = wasm::db::db_lookup(cid, PROMISSORY_NOTE_CONTRACT_NULLIFIERS_TREE)?;
    let info_db = wasm::db::db_lookup(cid, PROMISSORY_NOTE_CONTRACT_INFO_TREE)?;

    // Mark nullifier (coin redeemed) as flat marker, not SMT
    wasm::db::db_mark_spent(nullifiers_db, &update.nullifier.to_bytes())?;

    // Add receipt coin
    wasm::db::db_set(coins_db, &update.commitment.to_bytes(), &[1])?;

    // Update Merkle tree
    wasm::merkle::merkle_add(
        info_db,
        wasm::db::db_lookup(cid, PROMISSORY_NOTE_CONTRACT_COIN_ROOTS_TREE)?,
        PROMISSORY_NOTE_CONTRACT_LATEST_COIN_ROOT,
        PROMISSORY_NOTE_CONTRACT_COIN_MERKLE_TREE,
        &[MerkleNode::from_base(update.commitment.inner())],
    )?;

    Ok(())
}

// ============================================================================
// OTC SWAP - Atomic token swap between two parties
// ============================================================================

/// Metadata for OtcSwapV1 (atomic burn + mint for cross-token swap)
/// Same proof structure as TransferV1: Burn for inputs, BlindOutput for outputs.
fn otc_swap_get_metadata(_cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params= match OtcSwapParamsV1::decode(&self_.data[1..]) { Ok(p) => p, Err(_) => return Ok(vec![]) };

    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    // Schnorr signatures prohibited (contract-standards.md §3).
    let signature_pubkeys: Vec<pallas::Base> = vec![];

    // Burn proofs (one per input)
    for input in &params.inputs {

        let (vc_x, vc_y) = point_coords(input.value_commit);

        zk_public_inputs.push((
            PROMISSORY_NOTE_CONTRACT_ZKAS_REVOKE_NS_V2.to_string(),
            vec![
                input.nullifier.inner(),
                vc_x,
                vc_y,
                input.token_commit,
                input.merkle_root.inner(),
                input.user_data_enc,
                input.spend_hook.inner(),
                input.signature_public,
                params.tx_binding,
                params.tx_nonce,
            ],
        ));
    }

    // BlindOutput proofs (one per output) — includes spend_hook, tx_binding, tx_nonce
    for output in &params.outputs {
        let (vc_x, vc_y) = point_coords(output.value_commit);

        zk_public_inputs.push((
            PROMISSORY_NOTE_CONTRACT_ZKAS_TRANSFER_NS_V2.to_string(),
            vec![output.commitment.inner(), vc_x, vc_y, output.token_commit,
                 output.spend_hook.inner(), params.tx_binding, params.tx_nonce],
        ));
    }

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    signature_pubkeys.encode(&mut metadata)?;
    Ok(metadata)
}

/// OtcSwapV1 instruction - atomic token swap between two parties
///
/// Swaps tokens atomically:
/// - inputs[0] token goes to outputs[1] (Alice's token to Bob)
/// - inputs[1] token goes to outputs[0] (Bob's token to Alice)
///
/// OtcSwapV1 uses the same burn + blind output structure as TransferV1 but enforces:
/// - Exactly 2 inputs and 2 outputs
/// - Cross-token swap (inputs/outputs have different token_ids)
fn otc_swap_v1(cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params= OtcSwapParamsV1::decode(&self_.data[1..])?;
    msg!(
        "[promissory_note::otc_swap_v1] Processing OTC swap: {} inputs, {} outputs",
        params.inputs.len(),
        params.outputs.len()
    );

    // OtcSwapV1 requires exactly 2 inputs and 2 outputs
    if params.inputs.len() != 2 {
        msg!("[otc_swap_v1] Error: OTC swap requires exactly 2 inputs, got {}", params.inputs.len());
        return Err(PromissoryNoteError::TransferMissingInputs.into())
    }
    if params.outputs.len() != 2 {
        msg!("[otc_swap_v1] Error: OTC swap requires exactly 2 outputs, got {}", params.outputs.len());
        return Err(PromissoryNoteError::TransferMissingOutputs.into())
    }

    let coins_db = wasm::db::db_lookup(cid, PROMISSORY_NOTE_CONTRACT_COINS_TREE)?;
    let coin_roots_db = wasm::db::db_lookup(cid, PROMISSORY_NOTE_CONTRACT_COIN_ROOTS_TREE)?;
    let nullifiers_db = wasm::db::db_lookup(cid, PROMISSORY_NOTE_CONTRACT_NULLIFIERS_TREE)?;

    // Verify all input nullifiers are unique and not already spent
    let mut new_nullifiers = Vec::new();
    for (i, input) in params.inputs.iter().enumerate() {
        // Check Merkle root exists
        if !wasm::db::db_contains_key(coin_roots_db, &input.merkle_root.to_bytes())? {
            msg!("[otc_swap_v1] Error: Merkle root not found for input {}", i);
            return Err(PromissoryNoteError::TransferMerkleRootNotFound.into())
        }

        // Verify nullifier is NOT already spent
        if wasm::db::db_contains_key(nullifiers_db, &input.nullifier.to_bytes())? {
            msg!("[otc_swap_v1] Error: Nullifier already spent for input {}", i);
            return Err(PromissoryNoteError::DuplicateNullifier.into())
        }

        new_nullifiers.push(input.nullifier);
    }

    // Verify outputs are unique
    let mut new_commitments = Vec::new();
    for (i, output) in params.outputs.iter().enumerate() {
        if wasm::db::db_contains_key(coins_db, &output.commitment.to_bytes())? {
            msg!("[otc_swap_v1] Error: Duplicate coin in output {}", i);
            return Err(PromissoryNoteError::DuplicateCoin.into())
        }
        new_commitments.push(output.commitment);
    }

    // CROSS-TOKEN PAIRING: for a correct OTC swap, each input's token is swapped
    // to the opposite output. inputs[0].token_commit == outputs[1].token_commit
    // (Alice→Bob) and inputs[1].token_commit == outputs[0].token_commit (Bob→Alice).
    if params.inputs[0].token_commit != params.outputs[1].token_commit {
        msg!("[otc_swap_v1] Error: inputs[0].token_commit != outputs[1].token_commit");
        return Err(PromissoryNoteError::TokenCommitmentMismatch.into())
    }
    if params.inputs[1].token_commit != params.outputs[0].token_commit {
        msg!("[otc_swap_v1] Error: inputs[1].token_commit != outputs[0].token_commit");
        return Err(PromissoryNoteError::TokenCommitmentMismatch.into())
    }

    // CROSS-PROOF VALUE CONSERVATION: sum(inputs) == sum(outputs) per token_commit.
    verify_value_conservation(&params.inputs, &params.outputs)?;

    let update = OtcSwapUpdateV1 { nullifiers: new_nullifiers, commitments: new_commitments };
    msg!("[promissory_note::otc_swap_v1] OTC swap valid");
    wasm::util::set_return_data(&[&[PromissoryNoteFunction::OtcSwapV1 as u8], &update.encode()[..]].concat())
}

/// Apply OtcSwapV1 state update (same as apply_transfer)
fn apply_otc_swap(cid: ContractId, update: OtcSwapUpdateV1) -> ContractResult {
    msg!(
        "[promissory_note::apply_otc_swap] Marking {} nullifiers, adding {} coins",
        update.nullifiers.len(),
        update.commitments.len()
    );

    let coins_db = wasm::db::db_lookup(cid, PROMISSORY_NOTE_CONTRACT_COINS_TREE)?;
    let nullifiers_db = wasm::db::db_lookup(cid, PROMISSORY_NOTE_CONTRACT_NULLIFIERS_TREE)?;
    let info_db = wasm::db::db_lookup(cid, PROMISSORY_NOTE_CONTRACT_INFO_TREE)?;

    // Mark nullifiers (coins spent) as flat markers, not SMT
    for n in &update.nullifiers {
        wasm::db::db_mark_spent(nullifiers_db, &n.to_bytes())?;
    }

    // Add new coins
    let mut new_commitments = Vec::new();
    for commitment in &update.commitments {
        wasm::db::db_set(coins_db, &commitment.to_bytes(), &[1])?;
        new_commitments.push(MerkleNode::from_base(commitment.inner()));
    }

    // Update Merkle tree
    wasm::merkle::merkle_add(
        info_db,
        wasm::db::db_lookup(cid, PROMISSORY_NOTE_CONTRACT_COIN_ROOTS_TREE)?,
        PROMISSORY_NOTE_CONTRACT_LATEST_COIN_ROOT,
        PROMISSORY_NOTE_CONTRACT_COIN_MERKLE_TREE,
        &new_commitments,
    )?;

    Ok(())
}

// ============================================================================
// CROSS-CONTRACT COMPOSITION HELPERS (re-exported from validation module)
// ============================================================================

pub use crate::validation::{validate_child_contract_id, validate_child_value_commit};
