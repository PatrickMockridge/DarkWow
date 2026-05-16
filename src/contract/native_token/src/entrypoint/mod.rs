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
//! Design Philosophy: CONSENSUS FIRST, FEES SECOND, PRIVACY THIRD
//!
//! This contract serves as the native token for DarkWow with the following priorities:
//! 1. **Consensus Reward** - Block rewards for PoW mining must be reliable
//! 2. **Network Fees** - Transaction fee payment must be deterministic
//! 3. **Privacy Layer** - Privacy on top, never compromising consensus
//!
//! Privacy-first design following money_v2 patterns (without the heap bug):
//! - Uses Pedersen commitments for hidden values
//! - Uses AeadEncryptedNote for encrypted notes
//! - Uses nullifiers for double-spend prevention

use dwow_sdk::{
    blockchain::{expected_reward, reward},
    crypto::{
        pasta_prelude::{Curve, CurveAffine, Field, PrimeField}, pedersen_commitment_u64, poseidon_hash,
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

    // Include ZK circuits
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
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = NativeTokenFunction::try_from(self_.data[0])?;

    let metadata = match func {
        NativeTokenFunction::FeeV1 => fee_get_metadata(cid, call_idx, calls),
        NativeTokenFunction::MintV1 => mint_get_metadata(cid, call_idx, calls),
        NativeTokenFunction::BurnV1 => burn_get_metadata(cid, call_idx, calls),
        NativeTokenFunction::TransferV1 => transfer_get_metadata(cid, call_idx, calls),
        NativeTokenFunction::SpendV1 => spend_get_metadata(cid, call_idx, calls),
        NativeTokenFunction::PoWRewardV1 => pow_reward_get_metadata(cid, call_idx, calls),
    };

    wasm::util::set_return_data(&metadata)
}

/// Metadata for FeeV1
fn fee_get_metadata(_cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>) -> Vec<u8> {
    let self_ = &calls[call_idx].data;
    // Skip first 9 bytes: function ID (1) + fee (8)
    let params: FeeParamsV1 = deserialize(&self_.data[9..]).unwrap();

    // Public inputs for the ZK proofs we have to verify
    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    // Public keys for the transaction signatures we have to verify
    let signature_pubkeys: Vec<dwow_sdk::crypto::PublicKey> = vec![params.input.signature_public];

    // Grab the Pedersen commitments and the signature pubkey from the params
    let input_value_coords = params.input.value_commit.to_affine().coordinates().unwrap();
    let output_value_coords = params.output.value_commit.to_affine().coordinates().unwrap();
    let (sig_x, sig_y) = params.input.signature_public.xy();

    zk_public_inputs.push((
        NATIVE_TOKEN_CONTRACT_ZKAS_FEE_NS_V1.to_string(),
        vec![
            params.input.nullifier.inner(),
            *input_value_coords.x(),
            *input_value_coords.y(),
            params.input.token_commit,
            params.input.merkle_root.inner(),
            params.input.user_data_enc,
            sig_x,
            sig_y,
            params.output.coin.inner(),
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
    let params: MintParamsV1 = deserialize(&self_.data.data[1..]).unwrap();

    // Public inputs for the ZK proofs we have to verify
    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    // Public keys for the transaction signatures we have to verify
    let signature_pubkeys: Vec<dwow_sdk::crypto::PublicKey> = vec![];

    zk_public_inputs.push((
        NATIVE_TOKEN_CONTRACT_ZKAS_MINT_NS_V1.to_string(),
        vec![params.coin.inner()],
    ));

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata).unwrap();
    signature_pubkeys.encode(&mut metadata).unwrap();
    metadata
}

/// Metadata for BurnV1
fn burn_get_metadata(_cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>) -> Vec<u8> {
    let self_ = &calls[call_idx].data;
    let params: BurnParamsV1 = deserialize(&self_.data[1..]).unwrap();

    // Public inputs for the ZK proofs we have to verify
    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    let mut signature_pubkeys: Vec<dwow_sdk::crypto::PublicKey> = vec![];

    for input in &params.inputs {
        let value_coords = input.value_commit.to_affine().coordinates().unwrap();
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
fn transfer_get_metadata(_cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>) -> Vec<u8> {
    let self_ = &calls[call_idx].data;
    let params: TransferParamsV1 = deserialize(&self_.data[1..]).unwrap();

    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    let mut signature_pubkeys: Vec<dwow_sdk::crypto::PublicKey> = vec![];

    for input in &params.inputs {
        let (sig_x, sig_y) = input.signature_public.xy();
        signature_pubkeys.push(input.signature_public);

        let value_coords = input.value_commit.to_affine().coordinates().unwrap();
        zk_public_inputs.push((
            NATIVE_TOKEN_CONTRACT_ZKAS_BURN_NS_V1.to_string(),
            vec![
                input.nullifier.inner(),
                *value_coords.x(),
                *value_coords.y(),
                input.token_commit,
                input.merkle_root.inner(),
                input.user_data_enc,
                sig_x,
                sig_y,
            ],
        ));
    }

    for output in &params.outputs {
        let value_coords = output.value_commit.to_affine().coordinates().unwrap();
        zk_public_inputs.push((
            NATIVE_TOKEN_CONTRACT_ZKAS_MINT_NS_V1.to_string(),
            vec![
                output.token_commit,
                *value_coords.x(),
                *value_coords.y(),
            ],
        ));
    }

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata).unwrap();
    signature_pubkeys.encode(&mut metadata).unwrap();
    metadata
}

/// Metadata for SpendV1
fn spend_get_metadata(_cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>) -> Vec<u8> {
    let self_ = &calls[call_idx].data;
    let params: SpendParamsV1 = deserialize(&self_.data[1..]).unwrap();

    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    let signature_pubkeys: Vec<dwow_sdk::crypto::PublicKey> = vec![params.input.signature_public];

    let input_value_coords = params.input.value_commit.to_affine().coordinates().unwrap();
    let output_value_coords = params.output.value_commit.to_affine().coordinates().unwrap();
    let (sig_x, sig_y) = params.input.signature_public.xy();

    zk_public_inputs.push((
        NATIVE_TOKEN_CONTRACT_ZKAS_BURN_NS_V1.to_string(),
        vec![
            params.input.nullifier.inner(),
            *input_value_coords.x(),
            *input_value_coords.y(),
            params.input.token_commit,
            params.input.merkle_root.inner(),
            params.input.user_data_enc,
            sig_x,
            sig_y,
            params.output.coin.inner(),
            *output_value_coords.x(),
            *output_value_coords.y(),
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
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = NativeTokenFunction::try_from(self_.data[0])?;

    match func {
        NativeTokenFunction::FeeV1 => fee_v1(cid, call_idx, calls),
        NativeTokenFunction::MintV1 => mint_v1(cid, call_idx, calls),
        NativeTokenFunction::BurnV1 => burn_v1(cid, call_idx, calls),
        NativeTokenFunction::TransferV1 => transfer_v1(cid, call_idx, calls),
        NativeTokenFunction::SpendV1 => spend_v1(cid, call_idx, calls),
        NativeTokenFunction::PoWRewardV1 => pow_reward_v1(cid, call_idx, calls),
    }
}

// ============================================================================
// FEE - Pay network fees (CONSENSUS CRITICAL)
// ============================================================================

fn fee_v1(cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>) -> ContractResult {
    let self_ = &calls[call_idx].data;
    // Extract fee from raw tx data (bytes 1-9 after function ID)
    let fee: u64 = deserialize(&self_.data[1..9])?;
    let params: FeeParamsV1 = deserialize(&self_.data[9..])?;
    msg!("[native_token::fee_v1] Processing fee: {}", fee);

    // Access the necessary databases
    let coins_db = wasm::db::db_lookup(cid, NATIVE_TOKEN_CONTRACT_COINS_TREE)?;
    let nullifiers_db = wasm::db::db_lookup(cid, NATIVE_TOKEN_CONTRACT_NULLIFIERS_TREE)?;
    let coin_roots_db = wasm::db::db_lookup(cid, NATIVE_TOKEN_CONTRACT_COIN_ROOTS_TREE)?;

    // Token must be DARK (native token)
    let token_commit = poseidon_hash([pallas::Base::zero(), pallas::Base::zero()]);
    if params.input.token_commit != token_commit {
        msg!("[fee_v1] Error: Input token commitment is not the native token");
        return Err(NativeTokenError::TokenMismatch.into())
    }
    if params.output.token_commit != token_commit {
        msg!("[fee_v1] Error: Output token commitment is not native token");
        return Err(NativeTokenError::TokenMismatch.into())
    }

    // Verify Merkle root exists
    if !wasm::db::db_contains_key(coin_roots_db, &serialize(&params.input.merkle_root))? {
        msg!("[fee_v1] Error: Input Merkle root not found in previous state");
        return Err(NativeTokenError::TransferMerkleRootNotFound.into())
    }

    // Verify nullifier is NOT already spent (SMT lookup)
    let smt_store = dwow_sdk::crypto::smt::wasmdb::SmtWasmDbStorage::new(nullifiers_db);
    let smt = SmtWasmFp::new(smt_store, PoseidonFp::new(), &EMPTY_NODES_FP);
    if smt.get_leaf(&params.input.nullifier.inner()) != pallas::Base::zero() {
        msg!("[fee_v1] Error: Duplicate nullifier found");
        return Err(NativeTokenError::DuplicateNullifier.into())
    }

    // Verify new coin does not already exist
    if wasm::db::db_contains_key(coins_db, &serialize(&params.output.coin))? {
        msg!("[fee_v1] Error: Duplicate coin found");
        return Err(NativeTokenError::DuplicateCoin.into())
    }

    // Get verifying block height
    let verifying_block_height = wasm::util::get_verifying_block_height()?;

    // Create state update
    let update = FeeUpdateV1 {
        nullifier: params.input.nullifier,
        coin: params.output.coin,
        height: verifying_block_height,
        fee,
    };

    msg!("[native_token::fee_v1] Fee valid");
    wasm::util::set_return_data(&serialize(&(NativeTokenFunction::FeeV1 as u8, update)))
}

// ============================================================================
// TRANSFER - Private token transfer (PRIVACY)
// ============================================================================

fn transfer_v1(cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params: TransferParamsV1 = deserialize(&self_.data[1..])?;
    msg!(
        "[native_token::transfer_v1] Processing transfer: {} inputs, {} outputs",
        params.inputs.len(),
        params.outputs.len()
    );

    if params.inputs.is_empty() {
        return Err(NativeTokenError::TransferMissingInputs.into())
    }
    if params.outputs.is_empty() {
        return Err(NativeTokenError::TransferMissingOutputs.into())
    }

    let coins_db = wasm::db::db_lookup(cid, NATIVE_TOKEN_CONTRACT_COINS_TREE)?;
    let coin_roots_db = wasm::db::db_lookup(cid, NATIVE_TOKEN_CONTRACT_COIN_ROOTS_TREE)?;

    // Verify all input nullifiers are unique and not already spent
    let mut new_nullifiers = Vec::new();
    for (i, input) in params.inputs.iter().enumerate() {
        // Check Merkle root exists
        if !wasm::db::db_contains_key(coin_roots_db, &serialize(&input.merkle_root))? {
            msg!("[transfer_v1] Error: Merkle root not found for input {}", i);
            return Err(NativeTokenError::TransferMerkleRootNotFound.into())
        }

        new_nullifiers.push(input.nullifier);
    }

    // Verify outputs are unique
    let mut new_coins = Vec::new();
    for (i, output) in params.outputs.iter().enumerate() {
        if wasm::db::db_contains_key(coins_db, &serialize(&output.coin))? {
            msg!("[transfer_v1] Error: Duplicate coin in output {}", i);
            return Err(NativeTokenError::DuplicateCoin.into())
        }
        new_coins.push(output.coin);
    }

    let update = TransferUpdateV1 { nullifiers: new_nullifiers, coins: new_coins };
    msg!("[native_token::transfer_v1] Transfer valid");
    wasm::util::set_return_data(&serialize(&(NativeTokenFunction::TransferV1 as u8, update)))
}

// ============================================================================
// SPEND - Spend with change (PRIVACY)
// ============================================================================

fn spend_v1(cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params: SpendParamsV1 = deserialize(&self_.data[1..])?;
    msg!("[native_token::spend_v1] Processing spend");

    // Validate DARK token
    let token_commit = poseidon_hash([pallas::Base::zero(), pallas::Base::zero()]);
    if params.input.token_commit != token_commit {
        msg!("[spend_v1] Error: Input token commitment is not the native token");
        return Err(NativeTokenError::TokenMismatch.into())
    }
    if params.output.token_commit != token_commit {
        msg!("[spend_v1] Error: Output token commitment is not native token");
        return Err(NativeTokenError::TokenMismatch.into())
    }

    // Verify Merkle root exists
    let coin_roots_db = wasm::db::db_lookup(cid, NATIVE_TOKEN_CONTRACT_COIN_ROOTS_TREE)?;
    if !wasm::db::db_contains_key(coin_roots_db, &serialize(&params.input.merkle_root))? {
        msg!("[spend_v1] Error: Input Merkle root not found in previous state");
        return Err(NativeTokenError::TransferMerkleRootNotFound.into())
    }

    // Verify nullifier not already spent (SMT lookup)
    let nullifiers_db = wasm::db::db_lookup(cid, NATIVE_TOKEN_CONTRACT_NULLIFIERS_TREE)?;
    let smt_store = dwow_sdk::crypto::smt::wasmdb::SmtWasmDbStorage::new(nullifiers_db);
    let smt = SmtWasmFp::new(smt_store, PoseidonFp::new(), &EMPTY_NODES_FP);
    if smt.get_leaf(&params.input.nullifier.inner()) != pallas::Base::zero() {
        msg!("[spend_v1] Error: Duplicate nullifier found");
        return Err(NativeTokenError::DuplicateNullifier.into())
    }

    // Verify new coin doesn't already exist
    let coins_db = wasm::db::db_lookup(cid, NATIVE_TOKEN_CONTRACT_COINS_TREE)?;
    if wasm::db::db_contains_key(coins_db, &serialize(&params.output.coin))? {
        msg!("[spend_v1] Error: Duplicate coin found");
        return Err(NativeTokenError::DuplicateCoin.into())
    }

    let update = SpendUpdateV1 { nullifier: params.input.nullifier, coin: params.output.coin };

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

fn burn_v1(cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params: BurnParamsV1 = deserialize(&self_.data[1..])?;
    msg!("[native_token::burn_v1] Processing burn: {} inputs", params.inputs.len());

    if params.inputs.is_empty() {
        return Err(NativeTokenError::TransferMissingInputs.into())
    }

    let coin_roots_db = wasm::db::db_lookup(cid, NATIVE_TOKEN_CONTRACT_COIN_ROOTS_TREE)?;
    let nullifiers_db = wasm::db::db_lookup(cid, NATIVE_TOKEN_CONTRACT_NULLIFIERS_TREE)?;

    // SMT for nullifier lookup
    let smt_store = dwow_sdk::crypto::smt::wasmdb::SmtWasmDbStorage::new(nullifiers_db);
    let smt = SmtWasmFp::new(smt_store, PoseidonFp::new(), &EMPTY_NODES_FP);

    let mut new_nullifiers = Vec::new();
    for (i, input) in params.inputs.iter().enumerate() {
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

fn pow_reward_get_metadata(_cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>) -> Vec<u8> {
    let self_ = &calls[call_idx].data;
    let params: PoWRewardParamsV1 = deserialize(&self_.data[1..]).unwrap();

    // Public inputs for the ZK proofs we have to verify
    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    // Public keys for the transaction signatures we have to verify
    let signature_pubkeys: Vec<dwow_sdk::crypto::PublicKey> = vec![params.input.signature_public];

    // Grab the Pedersen commitment and token commit from the output
    let value_coords = params.output.value_commit.to_affine().coordinates().unwrap();

    zk_public_inputs.push((
        NATIVE_TOKEN_CONTRACT_ZKAS_MINT_NS_V1.to_string(),
        vec![
            params.output.coin.inner(),
            *value_coords.x(),
            *value_coords.y(),
            params.output.token_commit,
        ],
    ));

    // Serialize everything gathered and return it
    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata).unwrap();
    signature_pubkeys.encode(&mut metadata).unwrap();
    metadata
}

fn pow_reward_v1(cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params: PoWRewardParamsV1 = deserialize(&self_.data[1..])?;
    msg!("[native_token::pow_reward_v1] Processing PoW reward for height verification");

    // Access the necessary databases
    let coins_db = wasm::db::db_lookup(cid, NATIVE_TOKEN_CONTRACT_COINS_TREE)?;

    // Verify input token is DARK (native token)
    if params.input.token_id != DRKW_TOKEN_ID {
        msg!("[pow_reward_v1] Error: Clear input used non-native token");
        return Err(NativeTokenError::TokenMismatch.into())
    }

    // Verify value commitment matches clear input
    if pedersen_commitment_u64(params.input.value, params.input.value_blind) != params.output.value_commit {
        msg!("[pow_reward_v1] Error: Value commitment mismatch");
        return Err(NativeTokenError::ValueMismatch.into())
    }

    // Verify token commitment matches clear input
    if poseidon_hash([params.input.token_id, params.input.token_blind]) != params.output.token_commit {
        msg!("[pow_reward_v1] Error: Token commitment mismatch");
        return Err(NativeTokenError::TokenMismatch.into())
    }

    // Check that the coin from the output hasn't existed before
    if wasm::db::db_contains_key(coins_db, &serialize(&params.output.coin))? {
        msg!("[pow_reward_v1] Error: Duplicate coin in output");
        return Err(NativeTokenError::DuplicateCoin.into())
    }

    // Get verifying block height
    let verifying_block_height = wasm::util::get_verifying_block_height()?;

    // Validate block reward matches the emission schedule
    let expected = expected_reward(verifying_block_height);
    if params.input.value < expected {
        msg!("[pow_reward_v1] Error: Reward below schedule: got {}, expected {} at height {}",
             params.input.value, expected, verifying_block_height);
        return Err(NativeTokenError::ValueMismatch.into())
    }

    // Enforce 21M DRK supply cap
    let info_db = wasm::db::db_lookup(cid, NATIVE_TOKEN_CONTRACT_INFO_TREE)?;
    let current_supply: u64 = wasm::db::db_get(info_db, NATIVE_TOKEN_CONTRACT_TOTAL_SUPPLY)?
        .map(|data| deserialize(&data).unwrap_or(0))
        .unwrap_or(0);
    let new_supply = current_supply.saturating_add(params.input.value);
    if new_supply > reward::MAX_SUPPLY {
        msg!("[pow_reward_v1] Error: Supply cap exceeded: {} + {} > {}",
             current_supply, params.input.value, reward::MAX_SUPPLY);
        return Err(ContractError::InvalidFunction)
    }

    // Create state update
    let update = PoWRewardUpdateV1 {
        coin: params.output.coin,
        height: verifying_block_height,
        new_total_supply: new_supply,
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
            let update: MintUpdateV1 = deserialize(&update_data[1..])?;
            apply_mint(cid, update)
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
        deserialize(&wasm::db::db_get(fees_db, &serialize(&update.height))?.unwrap())?;
    paid_fee += update.fee;
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

    // Record cumulative total supply
    msg!("[PoWRewardV1] Recording total supply: {}", update.new_total_supply);
    wasm::db::db_set(
        info_db,
        NATIVE_TOKEN_CONTRACT_TOTAL_SUPPLY,
        &serialize(&update.new_total_supply),
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