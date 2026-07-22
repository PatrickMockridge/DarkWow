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

//! NativeToken WASM Entrypoint
//!
//! # Possible Future Upgrade
//!
//! This module implements WASM contract execution including Pedersen cumulative
//! supply chain validation (`S_H = S_{H-1} + C_H`). It is intentionally **not
//! wired** into the current block application pipeline.
//!
//! **Design decision:** Keep supply audit as a **passive capability** (like
//! Bitcoin's halving schedule) rather than an active consensus circuit breaker.
//! Any node can verify the chain via `verify_cumulative_supply()` without
//! trusting ZK proofs. Block production does not halt if the chain diverges —
//! nodes detect the divergence and can choose to fork.
//!
//! Activating this path (by wiring `execute_block` into `connect_block`) would
//! make cumulative supply validation an **active** consensus rule — blocks with
//! invalid cumulative commitments would be rejected at execution time. The
//! validation logic below is correct and ready for activation.
//! - Uses Pedersen commitments for hidden values
//! - Uses AeadEncryptedNote for encrypted notes
//! - Uses nullifiers for double-spend prevention

use dwow_sdk::crypto::poseidon_hash;
use dwow_sdk::{
    blockchain::{expected_reward, BlockHeight},
    crypto::{
        pasta_prelude::{Curve, CurveAffine, Field, Group, PrimeField}, pedersen_commitment_u64,
        smt::{wasmdb::SmtWasmFp, PoseidonFp, EMPTY_NODES_FP}, ContractId, MerkleNode, MerkleTree,
    },
    error::{ContractError, ContractResult},
    msg,
    pasta::pallas,
    wasm,
};
use dwow_serial::{deserialize, serialize, Encodable, WriteExt};

use crate::{
    error::NativeTokenError,
    model::{
        BurnParamsV1, BurnUpdateV1, DRKW_TOKEN_ID, FeeCollectParamsV1, FeeCollectUpdateV1,
        FeeParamsV1, FeeUpdateV1, PoWRewardParamsV1, PoWRewardUpdateV1,
        SpendParamsV1, SpendUpdateV1, TransferParamsV1, TransferUpdateV1,
    },
    NativeTokenFunction, NATIVE_TOKEN_CONTRACT_COIN_MERKLE_TREE,
    NATIVE_TOKEN_CONTRACT_COIN_ROOTS_TREE, NATIVE_TOKEN_CONTRACT_COINS_TREE,
    NATIVE_TOKEN_CONTRACT_DB_VERSION, NATIVE_TOKEN_CONTRACT_FEES_TREE,
    NATIVE_TOKEN_CONTRACT_INFO_TREE, NATIVE_TOKEN_CONTRACT_LATEST_COIN_ROOT,
    NATIVE_TOKEN_CONTRACT_LATEST_NULLIFIER_ROOT, NATIVE_TOKEN_CONTRACT_NULLIFIERS_TREE,
    NATIVE_TOKEN_CONTRACT_NULLIFIER_ROOTS_TREE, NATIVE_TOKEN_CONTRACT_TOTAL_SUPPLY,
    NATIVE_TOKEN_CONTRACT_CUMULATIVE_VALUE_COMMIT, NATIVE_TOKEN_CONTRACT_CUMULATIVE_BLIND,
    NATIVE_TOKEN_CONTRACT_ZKAS_BURN_NS_V2, NATIVE_TOKEN_CONTRACT_ZKAS_FEE_NS_V2,
    NATIVE_TOKEN_CONTRACT_ZKAS_FEE_COLLECT_NS_V2,
    NATIVE_TOKEN_CONTRACT_ZKAS_MINT_NS_V2, EMPTY_COINS_TREE_ROOT,
};

// Generate WASM entrypoints
dwow_sdk::define_contract!(
    init: init_contract,
    exec: process_instruction,
    apply: process_update,
    metadata: get_metadata
);

// ============================================================================
// CONTRACT INITIALIZATION (CONSENSUS CRITICAL)
// ============================================================================

pub fn init_contract(cid: ContractId, _ix: &[u8]) -> ContractResult {
    msg!("[native_token::init_contract] Initializing native_token contract");

    // WASM entrypoint: load circuits into on-chain database at deploy time.
    // These are LOCAL variables for zkas_db_set(), NOT the client _BIN constants.
    // The client _BIN constants live in client/zkbins.rs (different compilation target).
    // This two-location pattern is inherited from upstream.
    let mint_v1_bincode = include_bytes!("../../proof/mint_v1.zk.bin");
    let burn_v1_bincode = include_bytes!("../../proof/burn_v1.zk.bin");
    let fee_v1_bincode = include_bytes!("../../proof/fee_v1.zk.bin");
    let fee_collect_v1_bincode = include_bytes!("../../proof/fee_collect_v1.zk.bin");

    wasm::db::zkas_db_set(&mint_v1_bincode[..])?;
    wasm::db::zkas_db_set(&burn_v1_bincode[..])?;
    wasm::db::zkas_db_set(&fee_v1_bincode[..])?;
    wasm::db::zkas_db_set(&fee_collect_v1_bincode[..])?;

    // V2 circuits (HAZOP H11: domain separation, M8: coin_public binding)
    let mint_v2_bincode = include_bytes!("../../proof/mint_v2.zk.bin");
    let burn_v2_bincode = include_bytes!("../../proof/burn_v2.zk.bin");
    let fee_v2_bincode = include_bytes!("../../proof/fee_v2.zk.bin");
    let fee_collect_v2_bincode = include_bytes!("../../proof/fee_collect_v2.zk.bin");
    wasm::db::zkas_db_set(&mint_v2_bincode[..])?;
    wasm::db::zkas_db_set(&burn_v2_bincode[..])?;
    wasm::db::zkas_db_set(&fee_v2_bincode[..])?;
    wasm::db::zkas_db_set(&fee_collect_v2_bincode[..])?;

    let tx_hash = wasm::util::get_tx_hash()?;
    let call_idx = wasm::util::get_call_index()?;
    let mut roots_value_data = Vec::with_capacity(32 + 1);
    tx_hash.encode(&mut roots_value_data)?;
    call_idx.encode(&mut roots_value_data)?;
    if roots_value_data.len() != 32 + 1 {
        msg!(
            "[native_token::init_contract] Error: Roots value data length is not expected (32 + 1): {}",
            roots_value_data.len()
        );
        return Err(NativeTokenError::RootsValueDataMismatch.into())
    }

    // Set up coin roots database.
    // db_lookup always succeeds (it's a handle allocator, not an existence
    // check).  Use db_contains_key on a known marker to test whether the
    // tree has actually been initialized.
    let db_coin_roots = wasm::db::db_lookup(cid, NATIVE_TOKEN_CONTRACT_COIN_ROOTS_TREE)?;
    if !wasm::db::db_contains_key(db_coin_roots, &serialize(&EMPTY_COINS_TREE_ROOT))? {
        wasm::db::db_set(db_coin_roots, &serialize(&EMPTY_COINS_TREE_ROOT), &roots_value_data)?;
    }

    // Set up nullifier roots database
    let db_null_roots = wasm::db::db_lookup(cid, NATIVE_TOKEN_CONTRACT_NULLIFIER_ROOTS_TREE)?;
    if !wasm::db::db_contains_key(db_null_roots, &serialize(&pallas::Base::zero().to_repr()))? {
        wasm::db::db_set(
            db_null_roots,
            &serialize(&pallas::Base::zero().to_repr()),
            &serialize(&vec![roots_value_data.clone()]),
        )?;
    }

    // Set up coins database
    let _coins_db = wasm::db::db_lookup(cid, NATIVE_TOKEN_CONTRACT_COINS_TREE)?;

    // Set up nullifiers database
    let _nullifiers_db = wasm::db::db_lookup(cid, NATIVE_TOKEN_CONTRACT_NULLIFIERS_TREE)?;

    // Set up fees database and seed the height-2 accumulator.
    // Genesis (height 1) bypasses WASM execution, so apply_pow_reward never
    // runs at H=1 and cannot create fees_db[2]. Without this seed, the first
    // FeeV1 or FeeCollectV1 at height 2 aborts with DbGetEmpty
    // (consensus-coinbase.md §3.13, Genesis). From height 2 onward,
    // apply_pow_reward seeds fees_db[H+1] for each block.
    let fees_db = wasm::db::db_lookup(cid, NATIVE_TOKEN_CONTRACT_FEES_TREE)?;
    let height_2_key = serialize(&BlockHeight::GENESIS.succ());
    if !wasm::db::db_contains_key(fees_db, &height_2_key)? {
        wasm::db::db_set(fees_db, &height_2_key, &serialize(&0_u64))?;
    }

    // Set up info database.
    // Use db_contains_key to check whether the info tree has actually been
    // initialized (db_lookup always succeeds — it only allocates a handle).
    let info_db = wasm::db::db_lookup(cid, NATIVE_TOKEN_CONTRACT_INFO_TREE)?;
    let version_key = NATIVE_TOKEN_CONTRACT_DB_VERSION;
    if !wasm::db::db_contains_key(info_db, version_key)? {
        // Create Merkle tree for coins
        let mut coin_tree = MerkleTree::new(1);
        coin_tree.append(MerkleNode::from(pallas::Base::ZERO));
        let mut coin_tree_data = vec![];
        coin_tree_data.write_u32(0)?;
        coin_tree.encode(&mut coin_tree_data)?;
        wasm::db::db_set(info_db, NATIVE_TOKEN_CONTRACT_COIN_MERKLE_TREE, &coin_tree_data)?;

        // Initialize latest roots
        wasm::db::db_set(
            info_db,
            NATIVE_TOKEN_CONTRACT_LATEST_COIN_ROOT,
            &serialize(&EMPTY_COINS_TREE_ROOT),
        )?;
        wasm::db::db_set(
            info_db,
            NATIVE_TOKEN_CONTRACT_LATEST_NULLIFIER_ROOT,
            &serialize(&pallas::Base::zero().to_repr()),
        )?;
    }

    wasm::db::db_set(info_db, NATIVE_TOKEN_CONTRACT_DB_VERSION, env!("CARGO_PKG_VERSION").as_bytes())?;

    msg!("[native_token::init_contract] Database trees initialized");
    Ok(())
}

// ============================================================================
// METADATA (ZK PROOF SETUP)
// ============================================================================

/// Returns ZK proof public inputs and signature pubkeys for the host to verify.
/// The host will verify ZK proofs using this metadata, then call
/// process_instruction() to validate the state transition.
fn get_metadata(cid: ContractId, ix: &[u8]) -> ContractResult {
    // ix = [function_selector] + serialize(params) — individual call dispatched by host
    if ix.is_empty() {
        msg!("[native_token::get_metadata] Error: Empty call data");
        return Ok(());
    }
    let func = match NativeTokenFunction::try_from(ix[0]) {
        Ok(f) => f,
        Err(_) => return Ok(()),
    };
    let params = &ix[1..];

    let metadata = match func {
        NativeTokenFunction::FeeV1 => fee_get_metadata(cid, params),
        NativeTokenFunction::MintV1 => {
            msg!("[native_token::get_metadata] MintV1 is disabled (unauthorized mint path)");
            return Err(ContractError::InvalidFunction)
        }
        NativeTokenFunction::BurnV1 => burn_get_metadata(cid, params),
        NativeTokenFunction::TransferV1 => transfer_get_metadata(cid, params),
        NativeTokenFunction::SpendV1 => spend_get_metadata(cid, params),
        NativeTokenFunction::PoWRewardV1 => pow_reward_get_metadata(cid, params),
        NativeTokenFunction::FeeCollectV1 => fee_collect_get_metadata(cid, params),
    };

    wasm::util::set_return_data(&metadata)
}

/// Metadata for FeeV1
fn fee_get_metadata(_cid: ContractId, params: &[u8]) -> Vec<u8> {
    // FeeV1 call data after the selector: [fee u64 LE (8)][FeeParamsV1] —
    // the SAME layout `fee_v1` (process) and the block balance checker
    // (proof_of_token_balance::process_fee_call) parse.
    if params.len() < 9 {
        msg!("[native_token::fee_get_metadata] Error: Params too short ({} bytes, need >= 9)", params.len());
        return vec![];
    }
    let fee_params: FeeParamsV1 = match deserialize(&params[8..]) { Ok(p) => p, Err(e) => { msg!("[native_token::fee_get_metadata] Error: Failed to deserialize FeeParamsV1: {:?}", e); return vec![]; } };

    // Public inputs for the ZK proofs we have to verify
    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    // Public keys for the transaction signatures we have to verify
    let signature_pubkeys: Vec<dwow_sdk::crypto::PublicKey> = vec![fee_params.input.signature_public];

    // Grab the Pedersen commitments and the signature pubkey from the params
    let input_value_coords = fee_params.input.value_commit.to_affine().coordinates();
    if input_value_coords.is_none().into() {
        msg!("[native_token::fee_get_metadata] Error: Input value commitment is identity (cannot extract coordinates)");
        return vec![];
    }
    let input_value_coords = input_value_coords.unwrap();
    let output_value_coords = fee_params.output.value_commit.to_affine().coordinates();
    if output_value_coords.is_none().into() {
        msg!("[native_token::fee_get_metadata] Error: Output value commitment is identity (cannot extract coordinates)");
        return vec![];
    }
    let output_value_coords = output_value_coords.unwrap();
    let (sig_x, sig_y) = fee_params.input.signature_public.xy().expect("pk not identity");

    zk_public_inputs.push((
        NATIVE_TOKEN_CONTRACT_ZKAS_FEE_NS_V2.to_string(),
        vec![
            fee_params.input.nullifier.inner(),     // 1
            *input_value_coords.x(),                // 2
            *input_value_coords.y(),                // 3
            fee_params.input.token_commit,          // 4
            fee_params.input.merkle_root.inner(),   // 5
            fee_params.input.user_data_enc,         // 6
            sig_x,                                  // 7
            sig_y,                                  // 8
            fee_params.output.coin.inner(),         // 9
            *output_value_coords.x(),               // 10
            *output_value_coords.y(),               // 11
            pallas::Base::from(fee_params.fee),     // 12: fee
            fee_params.tx_binding,                  // 13: tx_binding
            fee_params.tx_nonce,                    // 14: tx_nonce
        ],
    ));

    // Serialize everything gathered and return it
    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata).unwrap();
    signature_pubkeys.encode(&mut metadata).unwrap();
    metadata
}

/// Metadata for BurnV1
fn burn_get_metadata(_cid: ContractId, params: &[u8]) -> Vec<u8> {
    if params.is_empty() { msg!("[native_token::burn_get_metadata] Error: Empty params"); return vec![]; }
    let bp: BurnParamsV1 = match deserialize(params) { Ok(p) => p, Err(e) => { msg!("[native_token] Error: Failed to deserialize params: {:?}", e); return vec![]; } };

    // Public inputs for the ZK proofs we have to verify
    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    let mut signature_pubkeys: Vec<dwow_sdk::crypto::PublicKey> = vec![];

    for input in &bp.inputs {
        let value_coords = input.value_commit.to_affine().coordinates();
        if value_coords.is_none().into() {
            msg!("[native_token] Error: Value commitment is identity (cannot extract coordinates)");
            return vec![];
        }
        let value_coords = value_coords.unwrap();
        let (sig_x, sig_y) = input.signature_public.xy().expect("pk not identity");
        signature_pubkeys.push(input.signature_public);

        zk_public_inputs.push((
            NATIVE_TOKEN_CONTRACT_ZKAS_BURN_NS_V2.to_string(),
            vec![
                input.nullifier.inner(),        // 1
                *value_coords.x(),              // 2
                *value_coords.y(),              // 3
                input.token_commit,             // 4
                input.merkle_root.inner(),      // 5
                input.user_data_enc,            // 6
                input.spend_hook.inner(),        // 7
                sig_x,                          // 8
                sig_y,                          // 9
                bp.tx_binding,                  // 10: tx_binding
                bp.tx_nonce,                    // 11: tx_nonce
            ],
        ));
    }

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata).unwrap();
    signature_pubkeys.encode(&mut metadata).unwrap();
    metadata
}

/// Metadata for TransferV1
fn transfer_get_metadata(_cid: ContractId, params: &[u8]) -> Vec<u8> {
    if params.is_empty() { msg!("[native_token::transfer_get_metadata] Error: Empty params"); return vec![]; }
    let tp: TransferParamsV1 = match deserialize(params) { Ok(p) => p, Err(e) => { msg!("[native_token] Error: Failed to deserialize params: {:?}", e); return vec![]; } };

    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    let mut signature_pubkeys: Vec<dwow_sdk::crypto::PublicKey> = vec![];

    for input in &tp.inputs {
        let (sig_x, sig_y) = input.signature_public.xy().expect("pk not identity");
        signature_pubkeys.push(input.signature_public);

        let value_coords = input.value_commit.to_affine().coordinates();
        if value_coords.is_none().into() {
            msg!("[native_token] Error: Value commitment is identity (cannot extract coordinates)");
            return vec![];
        }
        let value_coords = value_coords.unwrap();
        zk_public_inputs.push((
            NATIVE_TOKEN_CONTRACT_ZKAS_BURN_NS_V2.to_string(),
            vec![
                input.nullifier.inner(),        // 1
                *value_coords.x(),              // 2
                *value_coords.y(),              // 3
                input.token_commit,             // 4
                input.merkle_root.inner(),      // 5
                input.user_data_enc,            // 6
                input.spend_hook.inner(),        // 7
                sig_x,                          // 8
                sig_y,                          // 9
                tp.tx_binding,                  // 10: tx_binding
                tp.tx_nonce,                    // 11: tx_nonce
            ],
        ));
    }

    for output in &tp.outputs {
        let value_coords = output.value_commit.to_affine().coordinates();
        if value_coords.is_none().into() {
            msg!("[native_token] Error: Value commitment is identity (cannot extract coordinates)");
            return vec![];
        }
        let value_coords = value_coords.unwrap();
        // Transfer mints: cumulative supply doesn't advance (only coinbase does).
        // The mint proof computes new_cumulative = identity + value_commit (since
        // old_cumulative = identity for non-coinbase), so S_H.x, S_H.y ARE the
        // value_commit coordinates (matching positions 3,4). The identity-point
        // coordinates were incorrect — the proof reveals value_commit, and the
        // metadata MUST match for L2 verification to pass.
        zk_public_inputs.push((
            NATIVE_TOKEN_CONTRACT_ZKAS_MINT_NS_V2.to_string(),
            vec![
                output.coin.inner(),            // 1: C
                output.nullifier.inner(),       // 2: nf
                *value_coords.x(),              // 3: vc.x
                *value_coords.y(),              // 4: vc.y
                output.token_commit,            // 5: tc
                *value_coords.x(),              // 6: S_H.x (== vc.x — identity + vc = vc)
                *value_coords.y(),              // 7: S_H.y (== vc.y)
                tp.tx_binding,                  // 8: tx_binding
                tp.tx_nonce,                    // 9: tx_nonce
            ],
        ));
    }

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata).unwrap();
    signature_pubkeys.encode(&mut metadata).unwrap();
    metadata
}

/// Metadata for SpendV1
fn spend_get_metadata(_cid: ContractId, params: &[u8]) -> Vec<u8> {
    if params.is_empty() { msg!("[native_token::spend_get_metadata] Error: Empty params"); return vec![]; }
    let sp: SpendParamsV1 = match deserialize(params) { Ok(p) => p, Err(e) => { msg!("[native_token] Error: Failed to deserialize params: {:?}", e); return vec![]; } };

    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    let signature_pubkeys: Vec<dwow_sdk::crypto::PublicKey> = vec![sp.input.signature_public];

    let input_value_coords = sp.input.value_commit.to_affine().coordinates();
    if input_value_coords.is_none().into() {
        msg!("[native_token] Error: Input value commitment is identity (cannot extract coordinates)");
        return vec![];
    }
    let input_value_coords = input_value_coords.unwrap();
    let output_value_coords = sp.output.value_commit.to_affine().coordinates();
    if output_value_coords.is_none().into() {
        msg!("[native_token] Error: Output value commitment is identity (cannot extract coordinates)");
        return vec![];
    }
    let output_value_coords = output_value_coords.unwrap();
    let (sig_x, sig_y) = sp.input.signature_public.xy().expect("pk not identity");

    zk_public_inputs.push((
        NATIVE_TOKEN_CONTRACT_ZKAS_BURN_NS_V2.to_string(),
        vec![
            sp.input.nullifier.inner(),         // 1
            *input_value_coords.x(),            // 2
            *input_value_coords.y(),            // 3
            sp.input.token_commit,              // 4
            sp.input.merkle_root.inner(),       // 5
            sp.input.user_data_enc,             // 6
            sp.input.spend_hook.inner(),         // 7
            sig_x,                              // 8
            sig_y,                              // 9
            sp.tx_binding,                      // 10: tx_binding
            sp.tx_nonce,                        // 11: tx_nonce
        ],
    ));

    zk_public_inputs.push((
        NATIVE_TOKEN_CONTRACT_ZKAS_MINT_NS_V2.to_string(),
        vec![
            sp.output.coin.inner(),             // 1: C
            sp.output.nullifier.inner(),        // 2: nf
            *output_value_coords.x(),           // 3: vc.x
            *output_value_coords.y(),           // 4: vc.y
            sp.output.token_commit,             // 5: tc
            *output_value_coords.x(),           // 6: S_H.x (== vc.x — identity + vc = vc, non-coinbase mint)
            *output_value_coords.y(),           // 7: S_H.y (== vc.y)
            sp.tx_binding,                      // 8: tx_binding
            sp.tx_nonce,                        // 9: tx_nonce
        ],
    ));

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata).unwrap();
    signature_pubkeys.encode(&mut metadata).unwrap();
    metadata
}

// ============================================================================
// INSTRUCTION PROCESSING (STATE TRANSITION VERIFICATION)
// ============================================================================

fn process_instruction(cid: ContractId, ix: &[u8]) -> ContractResult {
    if ix.is_empty() {
        msg!("[native_token::process_instruction] Error: Empty call data");
        return Err(ContractError::IoError("Empty call data".to_string()));
    }
    let func = NativeTokenFunction::try_from(ix[0])?;
    let params = &ix[1..];

    match func {
        NativeTokenFunction::FeeV1 => fee_v1(cid, params),
        NativeTokenFunction::MintV1 => {
            msg!("[native_token::process_instruction] MintV1 is disabled (unauthorized mint path — use PoWRewardV1 for block rewards)");
            Err(ContractError::InvalidFunction)
        }
        NativeTokenFunction::BurnV1 => burn_v1(cid, params),
        NativeTokenFunction::TransferV1 => transfer_v1(cid, params),
        NativeTokenFunction::SpendV1 => spend_v1(cid, params),
        NativeTokenFunction::PoWRewardV1 => pow_reward_v1(cid, params),
        NativeTokenFunction::FeeCollectV1 => fee_collect_v1(cid, params),
    }
}

// ============================================================================
// FEE - Pay network fees (CONSENSUS CRITICAL)
// ============================================================================

fn fee_v1(cid: ContractId, params: &[u8]) -> ContractResult {
    // Extract fee from raw tx data (bytes 0-8, before FeeParamsV1)
    let fee: u64 = deserialize(&params[0..8])?;
    let fee_val: FeeParamsV1 = deserialize(&params[8..])?;
    msg!("[native_token::fee_v1] Processing fee: {}", fee);

    // Access the necessary databases
    let coins_db = wasm::db::db_lookup(cid, NATIVE_TOKEN_CONTRACT_COINS_TREE)?;
    let nullifiers_db = wasm::db::db_lookup(cid, NATIVE_TOKEN_CONTRACT_NULLIFIERS_TREE)?;
    let coin_roots_db = wasm::db::db_lookup(cid, NATIVE_TOKEN_CONTRACT_COIN_ROOTS_TREE)?;

    // Token must be DARK (native token)
    let token_commit = poseidon_hash([pallas::Base::zero(), pallas::Base::zero()]);
    if fee_val.input.token_commit != token_commit {
        msg!("[fee_v1] Error: Input token commitment is not the native token");
        return Err(NativeTokenError::TokenMismatch.into())
    }
    if fee_val.output.token_commit != token_commit {
        msg!("[fee_v1] Error: Output token commitment is not native token");
        return Err(NativeTokenError::TokenMismatch.into())
    }

    // Minimum fee enforcement — prevents 0-fee transactions.
    // DEFAULT_FEE = 42_000_000 is the minimum; higher fees allowed for priority.
    if fee < crate::MIN_FEE_PER_CALL {
        msg!("[fee_v1] Error: Fee {} below minimum {}", fee, crate::MIN_FEE_PER_CALL);
        return Err(NativeTokenError::InsufficientBalance.into())
    }

    // Verify Merkle root exists
    if !wasm::db::db_contains_key(coin_roots_db, &serialize(&fee_val.input.merkle_root))? {
        msg!("[fee_v1] Error: Input Merkle root not found in previous state");
        return Err(NativeTokenError::TransferMerkleRootNotFound.into())
    }

    // Verify nullifier is NOT already spent (SMT lookup)
    let smt_store = dwow_sdk::crypto::smt::wasmdb::SmtWasmDbStorage::new(nullifiers_db);
    let smt = SmtWasmFp::new(smt_store, PoseidonFp::new(), &EMPTY_NODES_FP);
    if smt.get_leaf(&fee_val.input.nullifier.inner()) != pallas::Base::zero() {
        msg!("[fee_v1] Error: Duplicate nullifier found");
        return Err(NativeTokenError::DuplicateNullifier.into())
    }

    // Verify new coin does not already exist
    if wasm::db::db_contains_key(coins_db, &serialize(&fee_val.output.coin))? {
        msg!("[fee_v1] Error: Duplicate coin found");
        return Err(NativeTokenError::DuplicateCoin.into())
    }

    // Get verifying block height
    let verifying_block_height = wasm::util::get_verifying_block_height()?;

    // Create state update
    let update = FeeUpdateV1 {
        nullifier: fee_val.input.nullifier,
        coin: fee_val.output.coin,
        height: verifying_block_height,
        fee,
    };

    msg!("[native_token::fee_v1] Fee valid");
    wasm::util::set_return_data(&serialize(&(NativeTokenFunction::FeeV1 as u8, update)))
}

// ============================================================================
// TRANSFER - Private token transfer (PRIVACY)
// ============================================================================

fn transfer_v1(cid: ContractId, params: &[u8]) -> ContractResult {
    let tp: TransferParamsV1 = deserialize(params)?;
    msg!(
        "[native_token::transfer_v1] Processing transfer: {} inputs, {} outputs",
        tp.inputs.len(),
        tp.outputs.len()
    );

    if tp.inputs.is_empty() {
        return Err(NativeTokenError::TransferMissingInputs.into())
    }
    if tp.outputs.is_empty() {
        return Err(NativeTokenError::TransferMissingOutputs.into())
    }

    let coins_db = wasm::db::db_lookup(cid, NATIVE_TOKEN_CONTRACT_COINS_TREE)?;
    let coin_roots_db = wasm::db::db_lookup(cid, NATIVE_TOKEN_CONTRACT_COIN_ROOTS_TREE)?;
    let nullifiers_db = wasm::db::db_lookup(cid, NATIVE_TOKEN_CONTRACT_NULLIFIERS_TREE)?;

    // SMT for nullifier lookup
    let smt_store = dwow_sdk::crypto::smt::wasmdb::SmtWasmDbStorage::new(nullifiers_db);
    let smt = SmtWasmFp::new(smt_store, PoseidonFp::new(), &EMPTY_NODES_FP);

    // Verify all input nullifiers are unique and not already spent
    let mut new_nullifiers = Vec::new();
    for (i, input) in tp.inputs.iter().enumerate() {
        // Check Merkle root exists
        if !wasm::db::db_contains_key(coin_roots_db, &serialize(&input.merkle_root))? {
            msg!("[transfer_v1] Error: Merkle root not found for input {}", i);
            return Err(NativeTokenError::TransferMerkleRootNotFound.into())
        }

        // Verify nullifier is NOT already spent
        if smt.get_leaf(&input.nullifier.inner()) != pallas::Base::zero() {
            msg!("[transfer_v1] Error: Nullifier already spent for input {}", i);
            return Err(NativeTokenError::DuplicateNullifier.into())
        }

        new_nullifiers.push(input.nullifier);
    }

    // Verify outputs are unique
    let mut new_coins = Vec::new();
    for (i, output) in tp.outputs.iter().enumerate() {
        if wasm::db::db_contains_key(coins_db, &serialize(&output.coin))? {
            msg!("[transfer_v1] Error: Duplicate coin in output {}", i);
            return Err(NativeTokenError::DuplicateCoin.into())
        }
        new_coins.push(output.coin);
    }

    // CROSS-PROOF VALUE CONSERVATION: sum(inputs) == sum(outputs) per token_commit.
    // This prevents value inflation — a prover with one coin of value 1 could
    // otherwise burn it and create a new coin of value 1,000,000 with both
    // proofs verifying independently. Pedersen's additive homomorphism makes
    // this check possible without revealing plaintext values.
    {
        let mut input_sums: Vec<(pallas::Base, pallas::Point)> = Vec::new();
        for input in &tp.inputs {
            match input_sums.iter_mut().find(|(tc, _)| *tc == input.token_commit) {
                Some((_, sum)) => *sum = *sum + input.value_commit,
                None => input_sums.push((input.token_commit, input.value_commit)),
            }
        }
        let mut output_sums: Vec<(pallas::Base, pallas::Point)> = Vec::new();
        for output in &tp.outputs {
            match output_sums.iter_mut().find(|(tc, _)| *tc == output.token_commit) {
                Some((_, sum)) => *sum = *sum + output.value_commit,
                None => output_sums.push((output.token_commit, output.value_commit)),
            }
        }
        for (token_commit, input_sum) in &input_sums {
            match output_sums.iter().find(|(tc, _)| tc == token_commit) {
                Some((_, output_sum)) if *output_sum == *input_sum => {},
                _ => {
                    msg!("[transfer_v1] Error: Value conservation failed for token_commit {:?}", token_commit.to_repr());
                    return Err(NativeTokenError::ValueMismatch.into())
                }
            }
        }
        for (token_commit, _) in &output_sums {
            if !input_sums.iter().any(|(tc, _)| tc == token_commit) {
                msg!("[transfer_v1] Error: Output token_commit not present in inputs {:?}", token_commit.to_repr());
                return Err(NativeTokenError::ValueMismatch.into())
            }
        }
    }

    let update = TransferUpdateV1 { nullifiers: new_nullifiers, coins: new_coins };
    msg!("[native_token::transfer_v1] Transfer valid");
    wasm::util::set_return_data(&serialize(&(NativeTokenFunction::TransferV1 as u8, update)))
}

// ============================================================================
// SPEND - Spend with change (PRIVACY)
// ============================================================================

fn spend_v1(cid: ContractId, params: &[u8]) -> ContractResult {
    let sp: SpendParamsV1 = deserialize(params)?;
    msg!("[native_token::spend_v1] Processing spend");

    // Validate DARK token
    let token_commit = poseidon_hash([pallas::Base::zero(), pallas::Base::zero()]);
    if sp.input.token_commit != token_commit {
        msg!("[spend_v1] Error: Input token commitment is not the native token");
        return Err(NativeTokenError::TokenMismatch.into())
    }
    if sp.output.token_commit != token_commit {
        msg!("[spend_v1] Error: Output token commitment is not native token");
        return Err(NativeTokenError::TokenMismatch.into())
    }

    // Verify Merkle root exists
    let coin_roots_db = wasm::db::db_lookup(cid, NATIVE_TOKEN_CONTRACT_COIN_ROOTS_TREE)?;
    if !wasm::db::db_contains_key(coin_roots_db, &serialize(&sp.input.merkle_root))? {
        msg!("[spend_v1] Error: Input Merkle root not found in previous state");
        return Err(NativeTokenError::TransferMerkleRootNotFound.into())
    }

    // Verify nullifier not already spent (SMT lookup)
    let nullifiers_db = wasm::db::db_lookup(cid, NATIVE_TOKEN_CONTRACT_NULLIFIERS_TREE)?;
    let smt_store = dwow_sdk::crypto::smt::wasmdb::SmtWasmDbStorage::new(nullifiers_db);
    let smt = SmtWasmFp::new(smt_store, PoseidonFp::new(), &EMPTY_NODES_FP);
    if smt.get_leaf(&sp.input.nullifier.inner()) != pallas::Base::zero() {
        msg!("[spend_v1] Error: Duplicate nullifier found");
        return Err(NativeTokenError::DuplicateNullifier.into())
    }

    // Verify new coin doesn't already exist
    let coins_db = wasm::db::db_lookup(cid, NATIVE_TOKEN_CONTRACT_COINS_TREE)?;
    if wasm::db::db_contains_key(coins_db, &serialize(&sp.output.coin))? {
        msg!("[spend_v1] Error: Duplicate coin found");
        return Err(NativeTokenError::DuplicateCoin.into())
    }

    let update = SpendUpdateV1 { nullifier: sp.input.nullifier, coin: sp.output.coin };

    msg!("[native_token::spend_v1] Spend valid");
    wasm::util::set_return_data(&serialize(&(NativeTokenFunction::SpendV1 as u8, update)))
}

// ============================================================================
// BURN - Destroy coins (Z-cash style burn)
// ============================================================================

fn burn_v1(cid: ContractId, params: &[u8]) -> ContractResult {
    let bp: BurnParamsV1 = deserialize(params)?;
    msg!("[native_token::burn_v1] Processing burn: {} inputs", bp.inputs.len());

    if bp.inputs.is_empty() {
        return Err(NativeTokenError::TransferMissingInputs.into())
    }

    let coin_roots_db = wasm::db::db_lookup(cid, NATIVE_TOKEN_CONTRACT_COIN_ROOTS_TREE)?;
    let nullifiers_db = wasm::db::db_lookup(cid, NATIVE_TOKEN_CONTRACT_NULLIFIERS_TREE)?;

    // SMT for nullifier lookup
    let smt_store = dwow_sdk::crypto::smt::wasmdb::SmtWasmDbStorage::new(nullifiers_db);
    let smt = SmtWasmFp::new(smt_store, PoseidonFp::new(), &EMPTY_NODES_FP);

    let mut new_nullifiers = Vec::new();
    for (i, input) in bp.inputs.iter().enumerate() {
        // Verify Merkle root exists
        if !wasm::db::db_contains_key(coin_roots_db, &serialize(&input.merkle_root))? {
            msg!("[burn_v1] Error: Merkle root not found for input {}", i);
            return Err(NativeTokenError::TransferMerkleRootNotFound.into())
        }

        // Verify nullifier is NOT already spent
        if smt.get_leaf(&input.nullifier.inner()) != pallas::Base::zero() {
            msg!("[burn_v1] Error: Nullifier already spent for input {}", i);
            return Err(NativeTokenError::DuplicateNullifier.into())
        }

        new_nullifiers.push(input.nullifier);
    }

    let update = BurnUpdateV1 { nullifiers: new_nullifiers };
    msg!("[native_token::burn_v1] Burn valid");
    wasm::util::set_return_data(&serialize(&(NativeTokenFunction::BurnV1 as u8, update)))
}

// ============================================================================
// POW REWARD - Distribute block rewards (CONSENSUS CRITICAL)
// ============================================================================

fn pow_reward_get_metadata(_cid: ContractId, params: &[u8]) -> Vec<u8> {
    if params.is_empty() { msg!("[native_token::pow_reward_get_metadata] Error: Empty params"); return vec![]; }
    let pr: PoWRewardParamsV1 = match deserialize(params) { Ok(p) => p, Err(e) => { msg!("[native_token] Error: Failed to deserialize params: {:?}", e); return vec![]; } };

    // Public inputs for the ZK proofs we have to verify
    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    // Public keys for the transaction signatures we have to verify
    let signature_pubkeys: Vec<dwow_sdk::crypto::PublicKey> = vec![pr.input.signature_public];

    // Grab the Pedersen commitment and token commit from the output
    let value_coords = pr.output.value_commit.to_affine().coordinates();
    if value_coords.is_none().into() {
        msg!("[native_token] Error: Output value commitment is identity (cannot extract coordinates)");
        return vec![];
    }
    let value_coords = value_coords.unwrap();

    let cumcom_coords = pr.new_cumulative_commit.to_affine().coordinates();
    if cumcom_coords.is_none().into() {
        msg!("[native_token::pow_reward_get_metadata] Error: Cumulative commitment cannot be identity");
        return vec![];
    }
    let cumcom_coords = cumcom_coords.unwrap();

    zk_public_inputs.push((
        NATIVE_TOKEN_CONTRACT_ZKAS_MINT_NS_V2.to_string(),
        vec![
            pr.output.coin.inner(),         // 1: C
            pr.nullifier.inner(),           // 2: nf
            *value_coords.x(),              // 3: vc.x
            *value_coords.y(),              // 4: vc.y
            pr.output.token_commit,         // 5: tc
            *cumcom_coords.x(),             // 6: S_H.x
            *cumcom_coords.y(),             // 7: S_H.y
            pr.tx_binding,                  // 8: tx_binding
            pr.tx_nonce,                    // 9: tx_nonce
        ],
    ));

    // Serialize everything gathered and return it
    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata).unwrap();
    signature_pubkeys.encode(&mut metadata).unwrap();
    metadata
}

fn pow_reward_v1(cid: ContractId, params: &[u8]) -> ContractResult {
    let pr: PoWRewardParamsV1 = deserialize(params)?;
    msg!("[native_token::pow_reward_v1] Processing PoW reward for height verification");

    // Access the necessary databases
    let coins_db = wasm::db::db_lookup(cid, NATIVE_TOKEN_CONTRACT_COINS_TREE)?;

    // Verify input token is DARK (native token)
    if pr.input.token_id != DRKW_TOKEN_ID.inner() {
        msg!("[pow_reward_v1] Error: Clear input used non-native token");
        return Err(NativeTokenError::TokenMismatch.into())
    }

    // Verify value commitment matches clear input
    if pedersen_commitment_u64(pr.input.value, pr.input.value_blind) != pr.output.value_commit {
        msg!("[pow_reward_v1] Error: Value commitment mismatch");
        return Err(NativeTokenError::ValueMismatch.into())
    }

    // Verify token commitment matches clear input
    if poseidon_hash([pr.input.token_id, pr.input.token_blind.inner()]) != pr.output.token_commit {
        msg!("[pow_reward_v1] Error: Token commitment mismatch");
        return Err(NativeTokenError::TokenMismatch.into())
    }

    // Check that the coin from the output hasn't existed before
    if wasm::db::db_contains_key(coins_db, &serialize(&pr.output.coin))? {
        msg!("[pow_reward_v1] Error: Duplicate coin in output");
        return Err(NativeTokenError::DuplicateCoin.into())
    }

    // Verify nullifier: nf = poseidon_hash(coin_secret, coin).
    // Per formal guardrail: the nullifier is the capability claim — the miner
    // exercises the coinbase capability by publishing this nullifier.
    // Phase 0 (structural) already rejects zero nullifier; this is defense-in-depth.
    if pr.nullifier.inner() == pallas::Base::zero() {
        msg!("[pow_reward_v1] Error: Null nullifier — unclaimed reward");
        return Err(ContractError::InvalidFunction)
    }
    // Check nullifier is NOT already in the nullifier SMT (duplicate claim prevention)
    let nullifiers_db = wasm::db::db_lookup(cid, NATIVE_TOKEN_CONTRACT_NULLIFIERS_TREE)?;
    let smt_store = dwow_sdk::crypto::smt::wasmdb::SmtWasmDbStorage::new(nullifiers_db);
    let smt = SmtWasmFp::new(smt_store, PoseidonFp::new(), &EMPTY_NODES_FP);
    if smt.get_leaf(&pr.nullifier.inner()) != pallas::Base::zero() {
        msg!("[pow_reward_v1] Error: Duplicate nullifier — coinbase already claimed");
        return Err(NativeTokenError::DuplicateNullifier.into())
    }

    // Get verifying block height
    let verifying_block_height = wasm::util::get_verifying_block_height()?;

    // Validate block reward matches the emission schedule.
    // Canonical miner mints full base_reward. Uncle rewards are subtracted
    // at the consensus level (connect_block) via Pedersen mass balance —
    // no pin_deductions needed in the contract.
    let expected = expected_reward(verifying_block_height);
    // HAZOP F1: exact equality — prevents inflationary minting above emission schedule.
    // Previously lower-bound only (pr.input.value < expected) which allowed unlimited over-minting.
    if pr.input.value != expected.get() {
        msg!("[pow_reward_v1] Error: Reward below schedule: got {}, expected {} at height {}",
             pr.input.value, expected, verifying_block_height);
        return Err(NativeTokenError::ValueMismatch.into())
    }

    // Enforce supply matches emission schedule (infinity-mint hardening)
    let info_db = wasm::db::db_lookup(cid, NATIVE_TOKEN_CONTRACT_INFO_TREE)?;
    let current_supply: u64 = match wasm::db::db_get(info_db, NATIVE_TOKEN_CONTRACT_TOTAL_SUPPLY)? {
        Some(data) => deserialize(&data).map_err(|e| {
            msg!("[native_token::pow_reward_v1] Error: Corrupt state — TOTAL_SUPPLY deserialization failed: {:?}", e);
            ContractError::IoError("Corrupt state: TOTAL_SUPPLY deserialization failed".to_string())
        })?,
        None => {
            // Genesis block: no prior supply exists. Zero is the correct initial value.
            0
        }
    };
    let new_supply = current_supply.saturating_add(pr.input.value);
    if new_supply != pr.expected_cumulative_supply {
        msg!("[pow_reward_v1] Supply mismatch at height={}: current={} + reward={} = {} (expected={})",
             verifying_block_height, current_supply, pr.input.value, new_supply, pr.expected_cumulative_supply);
        return Err(ContractError::InvalidFunction)
    }

    // Pedersen cumulative supply chain verification.
    // The ZK circuit constrains: S_H = S_{H-1} + C_H where C_H is this coinbase's
    // value commitment. This creates a verifiable chain from genesis to tip.
    // The entrypoint verifies that the old cumulative values match on-chain state,
    // and persists the new cumulative values for the next block.
    let old_cumulative = match wasm::db::db_get(info_db, NATIVE_TOKEN_CONTRACT_CUMULATIVE_VALUE_COMMIT)? {
        Some(data) => deserialize::<pallas::Point>(&data).map_err(|e| {
            msg!("[native_token::pow_reward_v1] Error: Corrupt state — CUMULATIVE_VALUE_COMMIT deserialization failed: {:?}", e);
            ContractError::IoError("Corrupt state: CUMULATIVE_VALUE_COMMIT".to_string())
        })?,
        None => {
            // Genesis block: no prior cumulative supply commitment exists.
            // The identity point is the additive identity for Pedersen accumulation:
            // S_0 = identity, S_1 = identity + C_1 = C_1.
            pallas::Point::identity()
        }
    };
    let old_blind = match wasm::db::db_get(info_db, NATIVE_TOKEN_CONTRACT_CUMULATIVE_BLIND)? {
        Some(data) => deserialize::<pallas::Scalar>(&data).map_err(|e| {
            msg!("[native_token::pow_reward_v1] Error: Corrupt state — CUMULATIVE_BLIND deserialization failed: {:?}", e);
            ContractError::IoError("Corrupt state: CUMULATIVE_BLIND".to_string())
        })?,
        None => {
            // Genesis block: no prior cumulative blind exists.
            pallas::Scalar::zero()
        }
    };

    // Verify the old cumulative values match what the prover claims.
    // The ZK circuit reconstructs S_{H-1} from these witnesses and constrains
    // S_H = S_{H-1} + coin_value_commit. If the prover supplies wrong old values,
    // the reconstructed point won't match the commitment chain.
    if pr.old_cumulative_commit != old_cumulative {
        msg!("[pow_reward_v1] Error: old_cumulative_commit does not match on-chain state");
        return Err(ContractError::InvalidFunction)
    }
    if current_supply > 0 && pr.old_cumulative_blind != old_blind {
        // Skip blind check for genesis (first block has no prior blind)
        msg!("[pow_reward_v1] Error: old_cumulative_blind does not match on-chain state");
        return Err(ContractError::InvalidFunction)
    }

    // Compute new cumulative blind for persistence.
    // The ZK circuit constrains the point; the entrypoint tracks the scalar.
    let new_blind = old_blind + pr.input.value_blind.inner();
    let new_cumulative = old_cumulative + pr.output.value_commit;

    // Verify the circuit's new_cumulative matches our computation
    if pr.new_cumulative_commit != new_cumulative {
        msg!("[pow_reward_v1] Error: new_cumulative_commit does not match S_{H-1} + C_H");
        return Err(ContractError::InvalidFunction)
    }

    // Create state update
    let update = PoWRewardUpdateV1 {
        coin: pr.output.coin,
        height: verifying_block_height,
        new_total_supply: new_supply,
        cumulative_value_commit: new_cumulative,
        aggregate_blind: new_blind,
    };
    msg!("[native_token::pow_reward_v1] PoW reward valid");
    wasm::util::set_return_data(&serialize(&(NativeTokenFunction::PoWRewardV1 as u8, update)))
}

// ============================================================================
// STATE UPDATE (WRITE STATE AFTER VERIFICATION)
// ============================================================================

fn process_update(cid: ContractId, update_data: &[u8]) -> ContractResult {
    if update_data.is_empty() {
        msg!("[native_token::process_update] Error: Empty update data");
        return Err(ContractError::IoError("Empty update data".to_string()));
    }
    let func = NativeTokenFunction::try_from(update_data[0])?;

    match func {
        NativeTokenFunction::FeeV1 => {
            let update: FeeUpdateV1 = deserialize(&update_data[1..])?;
            apply_fee(cid, update)
        }
        NativeTokenFunction::MintV1 => {
            msg!("[native_token::process_update] MintV1 is disabled (unauthorized mint path)");
            Err(ContractError::InvalidFunction)
        }
        NativeTokenFunction::BurnV1 => {
            let update: BurnUpdateV1 = deserialize(&update_data[1..])?;
            apply_burn(cid, update)
        }
        NativeTokenFunction::TransferV1 => {
            let update: TransferUpdateV1 = deserialize(&update_data[1..])?;
            apply_transfer(cid, update)
        }
        NativeTokenFunction::SpendV1 => {
            let update: SpendUpdateV1 = deserialize(&update_data[1..])?;
            apply_spend(cid, update)
        }
        NativeTokenFunction::PoWRewardV1 => {
            let update: PoWRewardUpdateV1 = deserialize(&update_data[1..])?;
            apply_pow_reward(cid, update)
        }
        NativeTokenFunction::FeeCollectV1 => {
            let update: FeeCollectUpdateV1 = deserialize(&update_data[1..])?;
            apply_fee_collect(cid, update)
        }
    }
}

// ============================================================================
// FEE COLLECT — Forward accumulated fees to miner (CONSENSUS CRITICAL)
// ============================================================================

fn fee_collect_get_metadata(_cid: ContractId, params: &[u8]) -> Vec<u8> {
    if params.is_empty() { msg!("[native_token::fee_collect_get_metadata] Error: Empty params"); return vec![]; }
    let fc: FeeCollectParamsV1 = match deserialize(params) { Ok(p) => p, Err(e) => { msg!("[native_token] Error: Failed to deserialize params: {:?}", e); return vec![]; } };

    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    let value_coords = fc.output.value_commit.to_affine().coordinates();
    if value_coords.is_none().into() {
        msg!("[native_token] Error: Output value commitment is identity (cannot extract coordinates)");
        return vec![];
    }
    let value_coords = value_coords.unwrap();

    // Dedicated FeeCollect_V1 circuit — 7 public inputs, no cumulative supply
    // (spec §3.5). Fees are redistribution, not minting: the circuit has no
    // S_H constraint and the supply chain is untouched (spec §3.10).
    zk_public_inputs.push((
        NATIVE_TOKEN_CONTRACT_ZKAS_FEE_COLLECT_NS_V2.to_string(),
        vec![
            fc.output.coin.inner(),         // 1: C
            fc.nullifier.inner(),           // 2: nf
            *value_coords.x(),              // 3: vc.x
            *value_coords.y(),              // 4: vc.y
            fc.output.token_commit,         // 5: tc
            fc.tx_binding,                  // 6: tx_binding = poseidon_hash(tx_commitment, tx_nonce)
            fc.tx_nonce,                    // 7: tx_nonce
        ],
    ));

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata).unwrap();
    // FeeCollectV1: no signature public key (miner identity proven via nullifier)
    let empty_sigs: Vec<dwow_sdk::crypto::PublicKey> = vec![];
    empty_sigs.encode(&mut metadata).unwrap();
    metadata
}

fn fee_collect_v1(cid: ContractId, params: &[u8]) -> ContractResult {
    let fc: FeeCollectParamsV1 = deserialize(params)?;
    let height = wasm::util::get_verifying_block_height()?;
    msg!("[native_token::fee_collect_v1] Collecting {} fee units at height {}", fc.total_fees, height);

    // Check 1 (spec §3.7): reject zero-value claims. Kills the 0-fee replay:
    // after the pot is zeroed, a second FeeCollect claiming total_fees = 0
    // would otherwise pass check 2 and mint a 0-value coin, reopening the
    // closed merkle tree (audit finding D12).
    if fc.total_fees == 0 {
        msg!("[fee_collect_v1] Zero-value fee claim rejected");
        return Err(NativeTokenError::FeeTotalMismatch.into())
    }

    // Check 2 (spec §3.7): claimed total matches the accumulated pot
    let fees_db = wasm::db::db_lookup(cid, NATIVE_TOKEN_CONTRACT_FEES_TREE)?;
    let accumulated: u64 =
        deserialize(&wasm::db::db_get(fees_db, &serialize(&height))?.ok_or(ContractError::DbGetEmpty)?)?;
    if fc.total_fees != accumulated {
        msg!("[fee_collect_v1] Fee total mismatch: claimed {} != accumulated {} at height {}",
             fc.total_fees, accumulated, height);
        return Err(NativeTokenError::FeeTotalMismatch.into())
    }

    // Check 3 (spec §3.7): the fee coin is not a duplicate
    let coins_db = wasm::db::db_lookup(cid, NATIVE_TOKEN_CONTRACT_COINS_TREE)?;
    if wasm::db::db_contains_key(coins_db, &serialize(&fc.output.coin))? {
        msg!("[fee_collect_v1] Duplicate fee coin");
        return Err(NativeTokenError::DuplicateCoin.into())
    }

    // Check 4 (spec §3.7): nullifier is not already spent
    let nullifiers_db = wasm::db::db_lookup(cid, NATIVE_TOKEN_CONTRACT_NULLIFIERS_TREE)?;
    let smt_store = dwow_sdk::crypto::smt::wasmdb::SmtWasmDbStorage::new(nullifiers_db);
    let smt = SmtWasmFp::new(smt_store, PoseidonFp::new(), &EMPTY_NODES_FP);
    if smt.get_leaf(&fc.nullifier.inner()) != pallas::Base::zero() {
        msg!("[fee_collect_v1] Duplicate nullifier");
        return Err(NativeTokenError::DuplicateNullifier.into())
    }

    // Check 5 (spec §3.7): token must be DARK (native token)
    let token_commit = poseidon_hash([pallas::Base::zero(), pallas::Base::zero()]);
    if fc.output.token_commit != token_commit {
        msg!("[fee_collect_v1] Non-native token in fee collection");
        return Err(NativeTokenError::TokenMismatch.into())
    }

    let update = FeeCollectUpdateV1 {
        coin: fc.output.coin,
        height,
        total_fees: fc.total_fees,
    };

    msg!("[native_token::fee_collect_v1] Fee collection valid — {} units to miner", fc.total_fees);
    wasm::util::set_return_data(&serialize(&(NativeTokenFunction::FeeCollectV1 as u8, update)))
}

fn apply_fee_collect(cid: ContractId, update: FeeCollectUpdateV1) -> ContractResult {
    msg!("[native_token::apply_fee_collect] Adding fee coin, clearing fee pot at height {}: {} units",
         update.height, update.total_fees);

    let coins_db = wasm::db::db_lookup(cid, NATIVE_TOKEN_CONTRACT_COINS_TREE)?;
    let info_db = wasm::db::db_lookup(cid, NATIVE_TOKEN_CONTRACT_INFO_TREE)?;
    let fees_db = wasm::db::db_lookup(cid, NATIVE_TOKEN_CONTRACT_FEES_TREE)?;

    // The claim nullifier nf = poseidon_hash(sk_H, C_fee) is NOT inserted
    // into the contract nullifiers_db — the SAME value is the future spend
    // nullifier for this fee coin. Inserting it would make the coin born-
    // unspendable (the spend path checks the SMT and rejects duplicates).
    // PoWRewardV1 uses the identical model: sparse_merkle_insert_batch with
    // an EMPTY batch. Claim-replay prevention lives in: zero-claim rejection
    // (check #1), pot zeroing (this function's last step), Phase 0.5
    // structural rules, and host-level nullifier tracking (tx.nullifiers,
    // sled batches, chain_state in-memory cache — COINBASE_MATURITY applies).
    // Defense-in-depth: check #4 in fee_collect_v1 catches SMT collision with
    // a previously-SPENT coin (same formula, different height/key collision).

    // Add fee coin to coin set
    wasm::db::db_set(coins_db, &serialize(&update.coin), &[])?;

    // Update Merkle tree (closes the tree for this block)
    wasm::merkle::merkle_add(
        info_db,
        wasm::db::db_lookup(cid, NATIVE_TOKEN_CONTRACT_COIN_ROOTS_TREE)?,
        NATIVE_TOKEN_CONTRACT_LATEST_COIN_ROOT,
        NATIVE_TOKEN_CONTRACT_COIN_MERKLE_TREE,
        &[MerkleNode::from(update.coin.inner())],
    )?;

    // Zero out the fee pot for this height (prevents double-claim)
    wasm::db::db_set(fees_db, &serialize(&update.height), &serialize(&0_u64))?;

    Ok(())
}

fn apply_fee(cid: ContractId, update: FeeUpdateV1) -> ContractResult {
    msg!("[native_token::apply_fee] Marking nullifier, adding coin, accumulating fee: {}", update.fee);

    let nullifiers_db = wasm::db::db_lookup(cid, NATIVE_TOKEN_CONTRACT_NULLIFIERS_TREE)?;
    let coins_db = wasm::db::db_lookup(cid, NATIVE_TOKEN_CONTRACT_COINS_TREE)?;
    let info_db = wasm::db::db_lookup(cid, NATIVE_TOKEN_CONTRACT_INFO_TREE)?;
    let fees_db = wasm::db::db_lookup(cid, NATIVE_TOKEN_CONTRACT_FEES_TREE)?;

    // Mark nullifier as spent
    wasm::db::db_set(nullifiers_db, &serialize(&update.nullifier.inner()), &[])?;

    // Add new coin
    wasm::db::db_set(coins_db, &serialize(&update.coin), &[])?;

    // Update Merkle tree
    wasm::merkle::merkle_add(
        info_db,
        wasm::db::db_lookup(cid, NATIVE_TOKEN_CONTRACT_COIN_ROOTS_TREE)?,
        NATIVE_TOKEN_CONTRACT_LATEST_COIN_ROOT,
        NATIVE_TOKEN_CONTRACT_COIN_MERKLE_TREE,
        &[MerkleNode::from(update.coin.inner())],
    )?;

    // Update fee accumulator per block height
    let mut paid_fee: u64 =
        deserialize(&wasm::db::db_get(fees_db, &serialize(&update.height))?.ok_or(ContractError::DbGetEmpty)?)?;
    paid_fee = paid_fee.saturating_add(update.fee);
    wasm::db::db_set(fees_db, &serialize(&update.height), &serialize(&paid_fee))?;

    Ok(())
}

fn apply_transfer(cid: ContractId, update: TransferUpdateV1) -> ContractResult {
    msg!(
        "[native_token::apply_transfer] Marking {} nullifiers, adding {} coins",
        update.nullifiers.len(),
        update.coins.len()
    );

    let coins_db = wasm::db::db_lookup(cid, NATIVE_TOKEN_CONTRACT_COINS_TREE)?;
    let nullifiers_db = wasm::db::db_lookup(cid, NATIVE_TOKEN_CONTRACT_NULLIFIERS_TREE)?;
    let info_db = wasm::db::db_lookup(cid, NATIVE_TOKEN_CONTRACT_INFO_TREE)?;

    // Mark nullifiers (coins spent)
    for nullifier in &update.nullifiers {
        wasm::db::db_set(nullifiers_db, &serialize(&nullifier.inner()), &[])?;
    }

    // Add new coins
    let mut new_coins = Vec::new();
    for coin in &update.coins {
        wasm::db::db_set(coins_db, &serialize(coin), &[])?;
        new_coins.push(MerkleNode::from(coin.inner()));
    }

    // Update Merkle tree
    wasm::merkle::merkle_add(
        info_db,
        wasm::db::db_lookup(cid, NATIVE_TOKEN_CONTRACT_COIN_ROOTS_TREE)?,
        NATIVE_TOKEN_CONTRACT_LATEST_COIN_ROOT,
        NATIVE_TOKEN_CONTRACT_COIN_MERKLE_TREE,
        &new_coins,
    )?;

    Ok(())
}

fn apply_spend(cid: ContractId, update: SpendUpdateV1) -> ContractResult {
    msg!("[native_token::apply_spend] Marking nullifier and adding coin");
    let nullifiers_db = wasm::db::db_lookup(cid, NATIVE_TOKEN_CONTRACT_NULLIFIERS_TREE)?;
    let coins_db = wasm::db::db_lookup(cid, NATIVE_TOKEN_CONTRACT_COINS_TREE)?;

    wasm::db::db_set(nullifiers_db, &serialize(&update.nullifier.inner()), &[])?;
    wasm::db::db_set(coins_db, &serialize(&update.coin), &[])?;
    Ok(())
}

fn apply_burn(cid: ContractId, update: BurnUpdateV1) -> ContractResult {
    msg!("[native_token::apply_burn] Marking {} nullifiers", update.nullifiers.len());
    let nullifiers_db = wasm::db::db_lookup(cid, NATIVE_TOKEN_CONTRACT_NULLIFIERS_TREE)?;

    // Mark all nullifiers as spent
    for nullifier in &update.nullifiers {
        wasm::db::db_set(nullifiers_db, &serialize(&nullifier.inner()), &[])?;
    }

    Ok(())
}

fn apply_pow_reward(cid: ContractId, update: PoWRewardUpdateV1) -> ContractResult {
    msg!("[native_token::apply_pow_reward] Adding coin for block reward at height {}", update.height);

    let info_db = wasm::db::db_lookup(cid, NATIVE_TOKEN_CONTRACT_INFO_TREE)?;
    let coins_db = wasm::db::db_lookup(cid, NATIVE_TOKEN_CONTRACT_COINS_TREE)?;
    let coin_roots_db = wasm::db::db_lookup(cid, NATIVE_TOKEN_CONTRACT_COIN_ROOTS_TREE)?;
    let nullifiers_db = wasm::db::db_lookup(cid, NATIVE_TOKEN_CONTRACT_NULLIFIERS_TREE)?;
    let nullifier_roots_db = wasm::db::db_lookup(cid, NATIVE_TOKEN_CONTRACT_NULLIFIER_ROOTS_TREE)?;
    let fees_db = wasm::db::db_lookup(cid, NATIVE_TOKEN_CONTRACT_FEES_TREE)?;

    // Generate the accumulator for the next height
    msg!("[PoWRewardV1] Creating next height fees accumulator");
    wasm::db::db_set(fees_db, &serialize(&update.height.succ()), &serialize(&0_u64))?;

    // Update nullifiers snapshot
    msg!("[PoWRewardV1] Updating nullifiers snapshot");
    wasm::merkle::sparse_merkle_insert_batch(
        info_db,
        nullifiers_db,
        nullifier_roots_db,
        NATIVE_TOKEN_CONTRACT_LATEST_NULLIFIER_ROOT,
        &[],
    )?;

    // Record cumulative total supply (plaintext)
    msg!("[PoWRewardV1] Recording total supply: {}", update.new_total_supply);
    wasm::db::db_set(
        info_db,
        NATIVE_TOKEN_CONTRACT_TOTAL_SUPPLY,
        &serialize(&update.new_total_supply),
    )?;

    // Record Pedersen cumulative value commitment (cryptographic supply proof).
    // S_H = S_{H-1} + C_H — verifiable by any node against the emission schedule.
    wasm::db::db_set(
        info_db,
        NATIVE_TOKEN_CONTRACT_CUMULATIVE_VALUE_COMMIT,
        &serialize(&update.cumulative_value_commit),
    )?;
    wasm::db::db_set(
        info_db,
        NATIVE_TOKEN_CONTRACT_CUMULATIVE_BLIND,
        &serialize(&update.aggregate_blind),
    )?;

    // Add new coin
    msg!("[PoWRewardV1] Adding new coin to the set");
    wasm::db::db_set(coins_db, &serialize(&update.coin), &[])?;

    // Update Merkle tree
    msg!("[PoWRewardV1] Adding new coin to the Merkle tree");
    let coins = vec![MerkleNode::from(update.coin.inner())];
    wasm::merkle::merkle_add(
        info_db,
        coin_roots_db,
        NATIVE_TOKEN_CONTRACT_LATEST_COIN_ROOT,
        NATIVE_TOKEN_CONTRACT_COIN_MERKLE_TREE,
        &coins,
    )?;

    Ok(())
}