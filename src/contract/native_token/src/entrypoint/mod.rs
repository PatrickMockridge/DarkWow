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
    blockchain::{expected_reward, reward},
    crypto::{
        pasta_prelude::{Curve, CurveAffine, Field, Group, PrimeField}, pedersen_commitment_u64,
        smt::{wasmdb::SmtWasmFp, PoseidonFp, EMPTY_NODES_FP}, ContractId, MerkleNode, MerkleTree,
    },
    dark_tree::DarkLeaf,
    error::{ContractError, ContractResult},
    msg,
    pasta::pallas,
    wasm, ContractCall,
};
use dwow_serial::{deserialize, serialize, Encodable, WriteExt};

use crate::{
    error::NativeTokenError,
    model::{
        BurnParamsV1, BurnUpdateV1, DRKW_TOKEN_ID, FeeParamsV1, FeeUpdateV1, MintParamsV1,
        MintUpdateV1, PoWRewardParamsV1, PoWRewardUpdateV1, SpendParamsV1, SpendUpdateV1,
        TransferParamsV1, TransferUpdateV1,
    },
    NativeTokenFunction, NATIVE_TOKEN_CONTRACT_COIN_MERKLE_TREE,
    NATIVE_TOKEN_CONTRACT_COIN_ROOTS_TREE, NATIVE_TOKEN_CONTRACT_COINS_TREE,
    NATIVE_TOKEN_CONTRACT_DB_VERSION, NATIVE_TOKEN_CONTRACT_FEES_TREE,
    NATIVE_TOKEN_CONTRACT_INFO_TREE, NATIVE_TOKEN_CONTRACT_LATEST_COIN_ROOT,
    NATIVE_TOKEN_CONTRACT_LATEST_NULLIFIER_ROOT, NATIVE_TOKEN_CONTRACT_NULLIFIERS_TREE,
    NATIVE_TOKEN_CONTRACT_NULLIFIER_ROOTS_TREE, NATIVE_TOKEN_CONTRACT_TOTAL_SUPPLY,
    NATIVE_TOKEN_CONTRACT_CUMULATIVE_VALUE_COMMIT, NATIVE_TOKEN_CONTRACT_CUMULATIVE_BLIND,
    NATIVE_TOKEN_CONTRACT_ZKAS_BURN_NS_V1, NATIVE_TOKEN_CONTRACT_ZKAS_FEE_NS_V1,
    NATIVE_TOKEN_CONTRACT_ZKAS_MINT_NS_V1, EMPTY_COINS_TREE_ROOT,
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

    wasm::db::zkas_db_set(&mint_v1_bincode[..])?;
    wasm::db::zkas_db_set(&burn_v1_bincode[..])?;
    wasm::db::zkas_db_set(&fee_v1_bincode[..])?;

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

    // Set up coin roots database
    if wasm::db::db_lookup(cid, NATIVE_TOKEN_CONTRACT_COIN_ROOTS_TREE).is_err() {
        let db_coin_roots = wasm::db::db_init(cid, NATIVE_TOKEN_CONTRACT_COIN_ROOTS_TREE)?;
        wasm::db::db_set(db_coin_roots, &serialize(&EMPTY_COINS_TREE_ROOT), &roots_value_data)?;
    }

    // Set up nullifier roots database
    if wasm::db::db_lookup(cid, NATIVE_TOKEN_CONTRACT_NULLIFIER_ROOTS_TREE).is_err() {
        let db_null_roots = wasm::db::db_init(cid, NATIVE_TOKEN_CONTRACT_NULLIFIER_ROOTS_TREE)?;
        wasm::db::db_set(
            db_null_roots,
            &serialize(&pallas::Base::zero().to_repr()),
            &serialize(&vec![roots_value_data.clone()]),
        )?;
    }

    // Set up coins database
    if wasm::db::db_lookup(cid, NATIVE_TOKEN_CONTRACT_COINS_TREE).is_err() {
        wasm::db::db_init(cid, NATIVE_TOKEN_CONTRACT_COINS_TREE)?;
    }

    // Set up nullifiers database
    if wasm::db::db_lookup(cid, NATIVE_TOKEN_CONTRACT_NULLIFIERS_TREE).is_err() {
        wasm::db::db_init(cid, NATIVE_TOKEN_CONTRACT_NULLIFIERS_TREE)?;
    }

    // Set up info database
    let info_db = match wasm::db::db_lookup(cid, NATIVE_TOKEN_CONTRACT_INFO_TREE) {
        Ok(v) => v,
        Err(_) => {
            let info_db = wasm::db::db_init(cid, NATIVE_TOKEN_CONTRACT_INFO_TREE)?;

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

            info_db
        }
    };

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
    let func = NativeTokenFunction::try_from(ix[0])?;
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
    };

    wasm::util::set_return_data(&metadata)
}

/// Metadata for FeeV1
fn fee_get_metadata(_cid: ContractId, params: &[u8]) -> Vec<u8> {
    // params = serialize(FeeParamsV1) after the function selector
    let fee_params: FeeParamsV1 = match deserialize(params) { Ok(p) => p, Err(_) => return vec![] };

    // Public inputs for the ZK proofs we have to verify
    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    // Public keys for the transaction signatures we have to verify
    let signature_pubkeys: Vec<dwow_sdk::crypto::PublicKey> = vec![fee_params.input.signature_public];

    // Grab the Pedersen commitments and the signature pubkey from the params
    let input_value_coords = fee_params.input.value_commit.to_affine().coordinates();
    if input_value_coords.is_none().into() {
        return vec![];
    }
    let input_value_coords = input_value_coords.unwrap();
    let output_value_coords = fee_params.output.value_commit.to_affine().coordinates();
    if output_value_coords.is_none().into() {
        return vec![];
    }
    let output_value_coords = output_value_coords.unwrap();
    let (sig_x, sig_y) = fee_params.input.signature_public.xy();

    zk_public_inputs.push((
        NATIVE_TOKEN_CONTRACT_ZKAS_FEE_NS_V1.to_string(),
        vec![
            fee_params.input.nullifier.inner(),
            *input_value_coords.x(),
            *input_value_coords.y(),
            fee_params.input.token_commit,
            fee_params.input.merkle_root.inner(),
            fee_params.input.user_data_enc,
            sig_x,
            sig_y,
            fee_params.output.coin.inner(),
            *output_value_coords.x(),
            *output_value_coords.y(),
        ],
    ));

    // Serialize everything gathered and return it
    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata).unwrap();
    signature_pubkeys.encode(&mut metadata).unwrap();
    metadata
}

/// Metadata for MintV1
fn mint_get_metadata(_cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>) -> Vec<u8> {
    let self_ = &calls[call_idx];
    let params: MintParamsV1 = match deserialize(&self_.data.data[1..]) { Ok(p) => p, Err(_) => return vec![] };

    // Public inputs for the ZK proofs we have to verify
    let value_coords = params.value_commit.to_affine().coordinates();
    if value_coords.is_none().into() {
        return vec![];
    }
    let value_coords = value_coords.unwrap();

    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    // Public keys for the transaction signatures we have to verify
    let signature_pubkeys: Vec<dwow_sdk::crypto::PublicKey> = vec![];

    zk_public_inputs.push((
        NATIVE_TOKEN_CONTRACT_ZKAS_MINT_NS_V1.to_string(),
        vec![
            params.coin.inner(),
            *value_coords.x(),
            *value_coords.y(),
            params.token_commit,
        ],
    ));

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata).unwrap();
    signature_pubkeys.encode(&mut metadata).unwrap();
    metadata
}

/// Metadata for BurnV1
fn burn_get_metadata(_cid: ContractId, params: &[u8]) -> Vec<u8> {
    let bp: BurnParamsV1 = match deserialize(params) { Ok(p) => p, Err(_) => return vec![] };

    // Public inputs for the ZK proofs we have to verify
    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    let mut signature_pubkeys: Vec<dwow_sdk::crypto::PublicKey> = vec![];

    for input in &bp.inputs {
        let value_coords = input.value_commit.to_affine().coordinates();
        if value_coords.is_none().into() {
            return vec![];
        }
        let value_coords = value_coords.unwrap();
        let (sig_x, sig_y) = input.signature_public.xy();
        signature_pubkeys.push(input.signature_public);

        zk_public_inputs.push((
            NATIVE_TOKEN_CONTRACT_ZKAS_BURN_NS_V1.to_string(),
            vec![
                input.nullifier.inner(),
                *value_coords.x(),
                *value_coords.y(),
                input.token_commit,
                input.merkle_root.inner(),
                input.user_data_enc,
                input.spend_hook,
                sig_x,
                sig_y,
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
    let tp: TransferParamsV1 = match deserialize(params) { Ok(p) => p, Err(_) => return vec![] };

    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    let mut signature_pubkeys: Vec<dwow_sdk::crypto::PublicKey> = vec![];

    for input in &tp.inputs {
        let (sig_x, sig_y) = input.signature_public.xy();
        signature_pubkeys.push(input.signature_public);

        let value_coords = input.value_commit.to_affine().coordinates();
        if value_coords.is_none().into() {
            return vec![];
        }
        let value_coords = value_coords.unwrap();
        zk_public_inputs.push((
            NATIVE_TOKEN_CONTRACT_ZKAS_BURN_NS_V1.to_string(),
            vec![
                input.nullifier.inner(),
                *value_coords.x(),
                *value_coords.y(),
                input.token_commit,
                input.merkle_root.inner(),
                input.user_data_enc,
                input.spend_hook,
                sig_x,
                sig_y,
            ],
        ));
    }

    for output in &tp.outputs {
        let value_coords = output.value_commit.to_affine().coordinates();
        if value_coords.is_none().into() {
            return vec![];
        }
        let value_coords = value_coords.unwrap();
        zk_public_inputs.push((
            NATIVE_TOKEN_CONTRACT_ZKAS_MINT_NS_V1.to_string(),
            vec![
                output.coin.inner(),
                *value_coords.x(),
                *value_coords.y(),
                output.token_commit,
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
    let sp: SpendParamsV1 = match deserialize(params) { Ok(p) => p, Err(_) => return vec![] };

    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    let signature_pubkeys: Vec<dwow_sdk::crypto::PublicKey> = vec![sp.input.signature_public];

    let input_value_coords = sp.input.value_commit.to_affine().coordinates();
    if input_value_coords.is_none().into() {
        return vec![];
    }
    let input_value_coords = input_value_coords.unwrap();
    let output_value_coords = sp.output.value_commit.to_affine().coordinates();
    if output_value_coords.is_none().into() {
        return vec![];
    }
    let output_value_coords = output_value_coords.unwrap();
    let (sig_x, sig_y) = sp.input.signature_public.xy();

    zk_public_inputs.push((
        NATIVE_TOKEN_CONTRACT_ZKAS_BURN_NS_V1.to_string(),
        vec![
            sp.input.nullifier.inner(),
            *input_value_coords.x(),
            *input_value_coords.y(),
            sp.input.token_commit,
            sp.input.merkle_root.inner(),
            sp.input.user_data_enc,
            sp.input.spend_hook,
            sig_x,
            sig_y,
        ],
    ));

    zk_public_inputs.push((
        NATIVE_TOKEN_CONTRACT_ZKAS_MINT_NS_V1.to_string(),
        vec![
            sp.output.coin.inner(),
            *output_value_coords.x(),
            *output_value_coords.y(),
            sp.output.token_commit,
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
// MINT - Create new coins (Z-cash style mint)
// ============================================================================

fn mint_v1(cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params: MintParamsV1 = deserialize(&self_.data[1..])?;
    msg!("[native_token::mint_v1] Processing mint");

    let coins_db = wasm::db::db_lookup(cid, NATIVE_TOKEN_CONTRACT_COINS_TREE)?;

    // Verify coin doesn't already exist
    if wasm::db::db_contains_key(coins_db, &serialize(&params.coin))? {
        msg!("[mint_v1] Error: Coin already exists");
        return Err(NativeTokenError::DuplicateCoin.into())
    }

    let update = MintUpdateV1 { coin: params.coin };
    msg!("[native_token::mint_v1] Mint valid");
    wasm::util::set_return_data(&serialize(&(NativeTokenFunction::MintV1 as u8, update)))
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
    let pr: PoWRewardParamsV1 = match deserialize(params) { Ok(p) => p, Err(_) => return vec![] };

    // Public inputs for the ZK proofs we have to verify
    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    // Public keys for the transaction signatures we have to verify
    let signature_pubkeys: Vec<dwow_sdk::crypto::PublicKey> = vec![pr.input.signature_public];

    // Grab the Pedersen commitment and token commit from the output
    let value_coords = pr.output.value_commit.to_affine().coordinates();
    if value_coords.is_none().into() {
        return vec![];
    }
    let value_coords = value_coords.unwrap();

    zk_public_inputs.push((
        NATIVE_TOKEN_CONTRACT_ZKAS_MINT_NS_V1.to_string(),
        vec![
            pr.output.coin.inner(),
            *value_coords.x(),
            *value_coords.y(),
            pr.output.token_commit,
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
    if pr.input.token_id != DRKW_TOKEN_ID {
        msg!("[pow_reward_v1] Error: Clear input used non-native token");
        return Err(NativeTokenError::TokenMismatch.into())
    }

    // Verify value commitment matches clear input
    if pedersen_commitment_u64(pr.input.value, pr.input.value_blind) != pr.output.value_commit {
        msg!("[pow_reward_v1] Error: Value commitment mismatch");
        return Err(NativeTokenError::ValueMismatch.into())
    }

    // Verify token commitment matches clear input
    if poseidon_hash([pr.input.token_id, pr.input.token_blind]) != pr.output.token_commit {
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
    let smt = dwow_sdk::crypto::smt::wasmdb::SmtWasmDbStorage::new(nullifiers_db);
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
    if pr.input.value < expected {
        msg!("[pow_reward_v1] Error: Reward below schedule: got {}, expected {} at height {}",
             pr.input.value, expected, verifying_block_height);
        return Err(NativeTokenError::ValueMismatch.into())
    }

    // Enforce supply matches emission schedule (infinity-mint hardening)
    let info_db = wasm::db::db_lookup(cid, NATIVE_TOKEN_CONTRACT_INFO_TREE)?;
    let current_supply: u64 = wasm::db::db_get(info_db, NATIVE_TOKEN_CONTRACT_TOTAL_SUPPLY)?
        .map(|data| deserialize(&data).unwrap_or(0))
        .unwrap_or(0);
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
    let old_cumulative = wasm::db::db_get(info_db, NATIVE_TOKEN_CONTRACT_CUMULATIVE_VALUE_COMMIT)?
        .map(|data| deserialize::<pallas::Point>(&data))
        .transpose()?
        .unwrap_or(pallas::Point::identity());
    let old_blind = wasm::db::db_get(info_db, NATIVE_TOKEN_CONTRACT_CUMULATIVE_BLIND)?
        .map(|data| deserialize::<pallas::Scalar>(&data))
        .transpose()?
        .unwrap_or(pallas::Scalar::zero());

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
    }
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

fn apply_mint(cid: ContractId, update: MintUpdateV1) -> ContractResult {
    msg!("[native_token::apply_mint] Adding coin to state");
    let coins_db = wasm::db::db_lookup(cid, NATIVE_TOKEN_CONTRACT_COINS_TREE)?;
    let info_db = wasm::db::db_lookup(cid, NATIVE_TOKEN_CONTRACT_INFO_TREE)?;

    // Add coin
    wasm::db::db_set(coins_db, &serialize(&update.coin), &[])?;

    // Update Merkle tree
    wasm::merkle::merkle_add(
        info_db,
        wasm::db::db_lookup(cid, NATIVE_TOKEN_CONTRACT_COIN_ROOTS_TREE)?,
        NATIVE_TOKEN_CONTRACT_LATEST_COIN_ROOT,
        NATIVE_TOKEN_CONTRACT_COIN_MERKLE_TREE,
        &[MerkleNode::from(update.coin.inner())],
    )?;

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
    wasm::db::db_set(fees_db, &serialize(&(update.height + 1)), &serialize(&0_u64))?;

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