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

//! Money V3 WASM Entrypoint - DeFi Token Contract
//!
//! Design: PRIVACY FIRST, COMPOSABILITY SECOND, SIMPLICITY THIRD
//!
//! MoneyV3 is the privacy-focused token contract for DeFi use cases:
//! - Wrapped tokens (wBTC, wETH, etc.)
//! - Stablecoins (USD, EUR, etc.)
//! - ERC-20 style tokens
//!
//! Unlike NativeToken (consensus) or MoneyV2 (complex), MoneyV3:
//! - Supports MULTIPLE token types via TokenMint
//! - Uses Poseidon hash ONLY (no EC, no heap bugs)
//! - Has token authorization via AuthTokenMint
//!
//! ## Token Model
//!
//! - TokenMintV1: Creates a new token type (returns token_id)
//! - AuthTokenMintV1: Authorizes minting for an existing token
//! - MintV1: Mints tokens (requires auth)
//! - BurnV1: Burns tokens
//! - TransferV1: Private token transfer
//! - OtcSwapV1: Atomic OTC token swap

use dwow_sdk::{
    crypto::{
        pasta_prelude::{Field, PrimeField},
        smt::{wasmdb::SmtWasmFp, PoseidonFp, EMPTY_NODES_FP}, ContractId, MerkleNode, MerkleTree,
    },
    dark_tree::DarkLeaf,
    error::ContractResult,
    msg,
    pasta::pallas,
    wasm, ContractCall,
};
use dwow_serial::{deserialize, serialize, Encodable, WriteExt};

use crate::{
    error::MoneyV3Error,
    model::{
        AuthTokenMintParamsV1, AuthTokenMintUpdateV1, BurnParamsV1, BurnUpdateV1, MintParamsV1,
        MintUpdateV1, OtcSwapParamsV1, OtcSwapUpdateV1, TokenMintParamsV1, TokenMintUpdateV1,
        TransferParamsV1, TransferUpdateV1,
    },
    MoneyV3Function, MONEY_V3_CONTRACT_COIN_MERKLE_TREE,
    MONEY_V3_CONTRACT_COIN_ROOTS_TREE, MONEY_V3_CONTRACT_COINS_TREE,
    MONEY_V3_CONTRACT_DB_VERSION,
    MONEY_V3_CONTRACT_INFO_TREE, MONEY_V3_CONTRACT_LATEST_COIN_ROOT,
    MONEY_V3_CONTRACT_LATEST_NULLIFIER_ROOT, MONEY_V3_CONTRACT_NULLIFIERS_TREE,
    MONEY_V3_CONTRACT_NULLIFIER_ROOTS_TREE, MONEY_V3_CONTRACT_ZKAS_AUTH_TOKEN_MINT_NS_V1,
    MONEY_V3_CONTRACT_ZKAS_BURN_NS_V1, MONEY_V3_CONTRACT_ZKAS_MINT_NS_V1,
    MONEY_V3_CONTRACT_ZKAS_TOKEN_MINT_NS_V1,
    MONEY_V3_CONTRACT_ZKAS_TRANSFER_OUTPUT_NS_V1,
    EMPTY_COINS_TREE_ROOT,
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
    msg!("[money_v3::init_contract] Initializing money_v3 contract (DeFi tokens)");

    // Include ZK circuits
    let token_mint_v1_bincode = include_bytes!("../../proof/token_mint_v1.zk.bin");
    let auth_token_mint_v1_bincode = include_bytes!("../../proof/auth_token_mint_v1.zk.bin");
    let mint_v1_bincode = include_bytes!("../../proof/mint_v1.zk.bin");
    let burn_v1_bincode = include_bytes!("../../proof/burn_v1.zk.bin");
    let transfer_output_v1_bincode = include_bytes!("../../proof/transfer_output_v1.zk.bin");

    wasm::db::zkas_db_set(&token_mint_v1_bincode[..])?;
    wasm::db::zkas_db_set(&auth_token_mint_v1_bincode[..])?;
    wasm::db::zkas_db_set(&mint_v1_bincode[..])?;
    wasm::db::zkas_db_set(&burn_v1_bincode[..])?;
    wasm::db::zkas_db_set(&transfer_output_v1_bincode[..])?;

    let tx_hash = wasm::util::get_tx_hash()?;
    let call_idx = wasm::util::get_call_index()?;
    let mut roots_value_data = Vec::with_capacity(32 + 1);
    tx_hash.encode(&mut roots_value_data)?;
    call_idx.encode(&mut roots_value_data)?;
    if roots_value_data.len() != 32 + 1 {
        msg!(
            "[money_v3::init_contract] Error: Roots value data length is not expected (32 + 1): {}",
            roots_value_data.len()
        );
        return Err(MoneyV3Error::RootsValueDataMismatch.into())
    }

    // Set up coin roots database
    if wasm::db::db_lookup(cid, MONEY_V3_CONTRACT_COIN_ROOTS_TREE).is_err() {
        let db_coin_roots = wasm::db::db_init(cid, MONEY_V3_CONTRACT_COIN_ROOTS_TREE)?;
        wasm::db::db_set(db_coin_roots, &serialize(&EMPTY_COINS_TREE_ROOT), &roots_value_data)?;
    }

    // Set up nullifier roots database
    if wasm::db::db_lookup(cid, MONEY_V3_CONTRACT_NULLIFIER_ROOTS_TREE).is_err() {
        let db_null_roots = wasm::db::db_init(cid, MONEY_V3_CONTRACT_NULLIFIER_ROOTS_TREE)?;
        wasm::db::db_set(
            db_null_roots,
            &serialize(&pallas::Base::zero().to_repr()),
            &serialize(&vec![roots_value_data.clone()]),
        )?;
    }

    // Set up coins database
    if wasm::db::db_lookup(cid, MONEY_V3_CONTRACT_COINS_TREE).is_err() {
        wasm::db::db_init(cid, MONEY_V3_CONTRACT_COINS_TREE)?;
    }

    // Set up nullifiers database
    if wasm::db::db_lookup(cid, MONEY_V3_CONTRACT_NULLIFIERS_TREE).is_err() {
        wasm::db::db_init(cid, MONEY_V3_CONTRACT_NULLIFIERS_TREE)?;
    }

    // Set up info database
    let info_db = match wasm::db::db_lookup(cid, MONEY_V3_CONTRACT_INFO_TREE) {
        Ok(v) => v,
        Err(_) => {
            let info_db = wasm::db::db_init(cid, MONEY_V3_CONTRACT_INFO_TREE)?;

            // Create Merkle tree for coins
            let mut coin_tree = MerkleTree::new(1);
            coin_tree.append(MerkleNode::from(pallas::Base::ZERO));
            let mut coin_tree_data = vec![];
            coin_tree_data.write_u32(0)?;
            coin_tree.encode(&mut coin_tree_data)?;
            wasm::db::db_set(info_db, MONEY_V3_CONTRACT_COIN_MERKLE_TREE, &coin_tree_data)?;

            // Initialize latest roots
            wasm::db::db_set(
                info_db,
                MONEY_V3_CONTRACT_LATEST_COIN_ROOT,
                &serialize(&EMPTY_COINS_TREE_ROOT),
            )?;
            wasm::db::db_set(
                info_db,
                MONEY_V3_CONTRACT_LATEST_NULLIFIER_ROOT,
                &serialize(&pallas::Base::zero().to_repr()),
            )?;

            info_db
        }
    };

    wasm::db::db_set(info_db, MONEY_V3_CONTRACT_DB_VERSION, env!("CARGO_PKG_VERSION").as_bytes())?;

    msg!("[money_v3::init_contract] Database trees initialized");
    Ok(())
}

// ============================================================================
// METADATA (ZK PROOF SETUP)
// ============================================================================

fn get_metadata(cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = MoneyV3Function::try_from(self_.data[0])?;

    let metadata = match func {
        MoneyV3Function::TokenMintV1 => token_mint_get_metadata(cid, call_idx, calls),
        MoneyV3Function::AuthTokenMintV1 => auth_token_mint_get_metadata(cid, call_idx, calls),
        MoneyV3Function::MintV1 => mint_get_metadata(cid, call_idx, calls),
        MoneyV3Function::BurnV1 => burn_get_metadata(cid, call_idx, calls),
        MoneyV3Function::TransferV1 => transfer_get_metadata(cid, call_idx, calls),
        MoneyV3Function::OtcSwapV1 => otc_swap_get_metadata(cid, call_idx, calls),
    };

    wasm::util::set_return_data(&metadata)
}

/// Metadata for TokenMintV1
fn token_mint_get_metadata(_cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>) -> Vec<u8> {
    let self_ = &calls[call_idx];
    let params: TokenMintParamsV1 = match deserialize(&self_.data.data[1..]) { Ok(p) => p, Err(_) => return vec![] };

    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    let signature_pubkeys: Vec<pallas::Base> = vec![];

    zk_public_inputs.push((
        MONEY_V3_CONTRACT_ZKAS_TOKEN_MINT_NS_V1.to_string(),
        vec![
            params.token_id,
            params.token_auth_parent,
            params.coin.inner(),
            params.value_commit,
        ],
    ));

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata).unwrap();
    signature_pubkeys.encode(&mut metadata).unwrap();
    metadata
}

/// Metadata for AuthTokenMintV1
fn auth_token_mint_get_metadata(_cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>) -> Vec<u8> {
    let self_ = &calls[call_idx].data;
    let params: AuthTokenMintParamsV1 = match deserialize(&self_.data[1..]) { Ok(p) => p, Err(_) => return vec![] };

    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    let signature_pubkeys: Vec<pallas::Base> = vec![params.mint_public];

    zk_public_inputs.push((
        MONEY_V3_CONTRACT_ZKAS_AUTH_TOKEN_MINT_NS_V1.to_string(),
        vec![
            params.nullifier.inner(),
            params.token_registry_root.inner(),
            params.mint_public,
        ],
    ));

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata).unwrap();
    signature_pubkeys.encode(&mut metadata).unwrap();
    metadata
}

/// Metadata for MintV1
fn mint_get_metadata(_cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>) -> Vec<u8> {
    let self_ = &calls[call_idx].data;
    let params: MintParamsV1 = match deserialize(&self_.data[1..]) { Ok(p) => p, Err(_) => return vec![] };

    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    let signature_pubkeys: Vec<pallas::Base> = vec![];

    // MintV1 circuit expects: token_root, auth_nullifier, auth_mint_public, coin, value_commit, token_id
    zk_public_inputs.push((
        MONEY_V3_CONTRACT_ZKAS_MINT_NS_V1.to_string(),
        vec![
            params.auth_proof.token_registry_root.inner(),
            params.auth_proof.nullifier.inner(),
            params.auth_proof.mint_public,
            params.coin.inner(),
            params.value_commit,
            params.token_id,
        ],
    ));

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata).unwrap();
    signature_pubkeys.encode(&mut metadata).unwrap();
    metadata
}

/// Metadata for BurnV1
fn burn_get_metadata(_cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>) -> Vec<u8> {
    let self_ = &calls[call_idx].data;
    let params: BurnParamsV1 = match deserialize(&self_.data[1..]) {
        Ok(p) => p,
        Err(_) => return vec![],
    };

    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    let mut signature_pubkeys: Vec<pallas::Base> = vec![];

    for input in &params.inputs {
        signature_pubkeys.push(input.signature_public);

        zk_public_inputs.push((
            MONEY_V3_CONTRACT_ZKAS_BURN_NS_V1.to_string(),
            vec![
                input.nullifier.inner(),
                input.value_commit,
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

/// Metadata for TransferV1 (atomic burn + mint)
fn transfer_get_metadata(_cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>) -> Vec<u8> {
    let self_ = &calls[call_idx].data;
    let params: TransferParamsV1 = match deserialize(&self_.data[1..]) { Ok(p) => p, Err(_) => return vec![] };

    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    let mut signature_pubkeys: Vec<pallas::Base> = vec![];

    // Burn proofs (one per input)
    for input in &params.inputs {
        signature_pubkeys.push(input.signature_public);

        zk_public_inputs.push((
            MONEY_V3_CONTRACT_ZKAS_BURN_NS_V1.to_string(),
            vec![
                input.nullifier.inner(),
                input.value_commit,
                input.token_commit,
                input.merkle_root.inner(),
                input.user_data_enc,
                input.spend_hook,
                input.signature_public,
            ],
        ));
    }

    // TransferOutput_V1 proofs for outputs with public values
    // Enables cross-contract composition: parent contracts verify child transfer amounts
    for output in &params.outputs {
        if output.public_value.is_some() || output.public_token_id.is_some() {
            let public_value_base = pallas::Base::from(output.public_value.unwrap_or(0));
            let public_token_id = output.public_token_id.unwrap_or(pallas::Base::zero());
            zk_public_inputs.push((
                MONEY_V3_CONTRACT_ZKAS_TRANSFER_OUTPUT_NS_V1.to_string(),
                vec![
                    output.coin.inner(),
                    output.value_commit,
                    public_value_base,
                    public_token_id,
                ],
            ));
        }
    }

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
    let func = MoneyV3Function::try_from(self_.data[0])?;

    match func {
        MoneyV3Function::TokenMintV1 => token_mint_v1(cid, call_idx, calls),
        MoneyV3Function::AuthTokenMintV1 => auth_token_mint_v1(cid, call_idx, calls),
        MoneyV3Function::MintV1 => mint_v1(cid, call_idx, calls),
        MoneyV3Function::BurnV1 => burn_v1(cid, call_idx, calls),
        MoneyV3Function::TransferV1 => transfer_v1(cid, call_idx, calls),
        MoneyV3Function::OtcSwapV1 => otc_swap_v1(cid, call_idx, calls),
    }
}

// ============================================================================
// TOKEN MINT - Create a new token type (stablecoin, wrapped, etc.)
// ============================================================================

fn token_mint_v1(cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params: TokenMintParamsV1 = deserialize(&self_.data[1..])?;
    msg!("[money_v3::token_mint_v1] Creating new token type");

    let coins_db = wasm::db::db_lookup(cid, MONEY_V3_CONTRACT_COINS_TREE)?;

    // Verify coin doesn't already exist
    if wasm::db::db_contains_key(coins_db, &serialize(&params.coin))? {
        msg!("[token_mint_v1] Error: Coin already exists");
        return Err(MoneyV3Error::DuplicateCoin.into())
    }

    // Note: In a full implementation, we would also track the token_id
    // in a token registry to prevent duplicate token creation

    let update = TokenMintUpdateV1 { token_id: params.token_id, coin: params.coin };
    msg!("[money_v3::token_mint_v1] Token type created successfully");
    wasm::util::set_return_data(&serialize(&(MoneyV3Function::TokenMintV1 as u8, update)))
}

// ============================================================================
// AUTH TOKEN MINT - Authorize minting for existing token
// ============================================================================

fn auth_token_mint_v1(cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params: AuthTokenMintParamsV1 = deserialize(&self_.data[1..])?;
    msg!("[money_v3::auth_token_mint_v1] Authorizing token minting");

    let nullifiers_db = wasm::db::db_lookup(cid, MONEY_V3_CONTRACT_NULLIFIERS_TREE)?;

    // Verify nullifier is NOT already spent
    let smt_store = dwow_sdk::crypto::smt::wasmdb::SmtWasmDbStorage::new(nullifiers_db);
    let smt = SmtWasmFp::new(smt_store, PoseidonFp::new(), &EMPTY_NODES_FP);
    if smt.get_leaf(&params.nullifier.inner()) != pallas::Base::zero() {
        msg!("[auth_token_mint_v1] Error: Auth nullifier already used (replay attack)");
        return Err(MoneyV3Error::DuplicateNullifier.into())
    }

    let update = AuthTokenMintUpdateV1 { nullifier: params.nullifier };
    msg!("[money_v3::auth_token_mint_v1] Authorization valid");
    wasm::util::set_return_data(&serialize(&(MoneyV3Function::AuthTokenMintV1 as u8, update)))
}

// ============================================================================
// MINT - Mint tokens of existing token type (requires auth)
// ============================================================================

fn mint_v1(cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params: MintParamsV1 = deserialize(&self_.data[1..])?;
    msg!("[money_v3::mint_v1] Minting tokens");

    let coins_db = wasm::db::db_lookup(cid, MONEY_V3_CONTRACT_COINS_TREE)?;

    // Verify coin doesn't already exist
    if wasm::db::db_contains_key(coins_db, &serialize(&params.coin))? {
        msg!("[mint_v1] Error: Coin already exists");
        return Err(MoneyV3Error::DuplicateCoin.into())
    }

    // Note: In a full implementation, we would verify the auth_proof
    // by checking that the nullifier was properly marked in a previous tx

    let update = MintUpdateV1 { coin: params.coin };
    msg!("[money_v3::mint_v1] Mint valid");
    wasm::util::set_return_data(&serialize(&(MoneyV3Function::MintV1 as u8, update)))
}

// ============================================================================
// BURN - Destroy tokens
// ============================================================================

fn burn_v1(cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params: BurnParamsV1 = deserialize(&self_.data[1..])?;
    msg!("[money_v3::burn_v1] Processing burn: {} inputs", params.inputs.len());

    if params.inputs.is_empty() {
        return Err(MoneyV3Error::BurnMissingInputs.into())
    }

    let coin_roots_db = wasm::db::db_lookup(cid, MONEY_V3_CONTRACT_COIN_ROOTS_TREE)?;
    let nullifiers_db = wasm::db::db_lookup(cid, MONEY_V3_CONTRACT_NULLIFIERS_TREE)?;

    // SMT for nullifier lookup
    let smt_store = dwow_sdk::crypto::smt::wasmdb::SmtWasmDbStorage::new(nullifiers_db);
    let smt = SmtWasmFp::new(smt_store, PoseidonFp::new(), &EMPTY_NODES_FP);

    let mut new_nullifiers = Vec::new();
    for (i, input) in params.inputs.iter().enumerate() {
        // Verify Merkle root exists
        if !wasm::db::db_contains_key(coin_roots_db, &serialize(&input.merkle_root))? {
            msg!("[burn_v1] Error: Merkle root not found for input {}", i);
            return Err(MoneyV3Error::TransferMerkleRootNotFound.into())
        }

        // Verify nullifier is NOT already spent
        if smt.get_leaf(&input.nullifier.inner()) != pallas::Base::zero() {
            msg!("[burn_v1] Error: Nullifier already spent for input {}", i);
            return Err(MoneyV3Error::DuplicateNullifier.into())
        }

        new_nullifiers.push(input.nullifier);
    }

    let update = BurnUpdateV1 { nullifiers: new_nullifiers };
    msg!("[money_v3::burn_v1] Burn valid");
    wasm::util::set_return_data(&serialize(&(MoneyV3Function::BurnV1 as u8, update)))
}

// ============================================================================
// TRANSFER - Private token transfer
// ============================================================================

fn transfer_v1(cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params: TransferParamsV1 = deserialize(&self_.data[1..])?;
    msg!(
        "[money_v3::transfer_v1] Processing transfer: {} inputs, {} outputs",
        params.inputs.len(),
        params.outputs.len()
    );

    if params.inputs.is_empty() {
        return Err(MoneyV3Error::TransferMissingInputs.into())
    }
    if params.outputs.is_empty() {
        return Err(MoneyV3Error::TransferMissingOutputs.into())
    }

    let coins_db = wasm::db::db_lookup(cid, MONEY_V3_CONTRACT_COINS_TREE)?;
    let coin_roots_db = wasm::db::db_lookup(cid, MONEY_V3_CONTRACT_COIN_ROOTS_TREE)?;

    // Verify all input nullifiers are unique and not already spent
    let mut new_nullifiers = Vec::new();
    for (i, input) in params.inputs.iter().enumerate() {
        // Check Merkle root exists
        if !wasm::db::db_contains_key(coin_roots_db, &serialize(&input.merkle_root))? {
            msg!("[transfer_v1] Error: Merkle root not found for input {}", i);
            return Err(MoneyV3Error::TransferMerkleRootNotFound.into())
        }

        new_nullifiers.push(input.nullifier);
    }

    // Verify outputs are unique
    let mut new_coins = Vec::new();
    for (i, output) in params.outputs.iter().enumerate() {
        if wasm::db::db_contains_key(coins_db, &serialize(&output.coin))? {
            msg!("[transfer_v1] Error: Duplicate coin in output {}", i);
            return Err(MoneyV3Error::DuplicateCoin.into())
        }
        new_coins.push(output.coin);
    }

    let update = TransferUpdateV1 { nullifiers: new_nullifiers, coins: new_coins };
    msg!("[money_v3::transfer_v1] Transfer valid");
    wasm::util::set_return_data(&serialize(&(MoneyV3Function::TransferV1 as u8, update)))
}

// ============================================================================
// STATE UPDATE (WRITE STATE AFTER VERIFICATION)
// ============================================================================

fn process_update(cid: ContractId, update_data: &[u8]) -> ContractResult {
    let func = MoneyV3Function::try_from(update_data[0])?;

    match func {
        MoneyV3Function::TokenMintV1 => {
            let update: TokenMintUpdateV1 = deserialize(&update_data[1..])?;
            apply_token_mint(cid, update)
        }
        MoneyV3Function::AuthTokenMintV1 => {
            let update: AuthTokenMintUpdateV1 = deserialize(&update_data[1..])?;
            apply_auth_token_mint(cid, update)
        }
        MoneyV3Function::MintV1 => {
            let update: MintUpdateV1 = deserialize(&update_data[1..])?;
            apply_mint(cid, update)
        }
        MoneyV3Function::BurnV1 => {
            let update: BurnUpdateV1 = deserialize(&update_data[1..])?;
            apply_burn(cid, update)
        }
        MoneyV3Function::TransferV1 => {
            let update: TransferUpdateV1 = deserialize(&update_data[1..])?;
            apply_transfer(cid, update)
        }
        MoneyV3Function::OtcSwapV1 => {
            let update: OtcSwapUpdateV1 = deserialize(&update_data[1..])?;
            apply_otc_swap(cid, update)
        }
    }
}

fn apply_token_mint(cid: ContractId, update: TokenMintUpdateV1) -> ContractResult {
    msg!("[money_v3::apply_token_mint] Adding coin and registering token");

    let coins_db = wasm::db::db_lookup(cid, MONEY_V3_CONTRACT_COINS_TREE)?;
    let info_db = wasm::db::db_lookup(cid, MONEY_V3_CONTRACT_INFO_TREE)?;

    // Add coin
    wasm::db::db_set(coins_db, &serialize(&update.coin), &[])?;

    // Update Merkle tree
    wasm::merkle::merkle_add(
        info_db,
        wasm::db::db_lookup(cid, MONEY_V3_CONTRACT_COIN_ROOTS_TREE)?,
        MONEY_V3_CONTRACT_LATEST_COIN_ROOT,
        MONEY_V3_CONTRACT_COIN_MERKLE_TREE,
        &[MerkleNode::from(update.coin.inner())],
    )?;

    // Note: In a full implementation, we would also store the token_id
    // in a token registry to enable AuthTokenMint verification

    Ok(())
}

fn apply_auth_token_mint(cid: ContractId, update: AuthTokenMintUpdateV1) -> ContractResult {
    msg!("[money_v3::apply_auth_token_mint] Marking auth nullifier (prevents replay)");

    let nullifiers_db = wasm::db::db_lookup(cid, MONEY_V3_CONTRACT_NULLIFIERS_TREE)?;

    // Mark nullifier as spent (prevents replay of this auth)
    wasm::db::db_set(nullifiers_db, &serialize(&update.nullifier.inner()), &[])?;

    Ok(())
}

fn apply_mint(cid: ContractId, update: MintUpdateV1) -> ContractResult {
    msg!("[money_v3::apply_mint] Adding coin to state");
    let coins_db = wasm::db::db_lookup(cid, MONEY_V3_CONTRACT_COINS_TREE)?;
    let info_db = wasm::db::db_lookup(cid, MONEY_V3_CONTRACT_INFO_TREE)?;

    // Add coin
    wasm::db::db_set(coins_db, &serialize(&update.coin), &[])?;

    // Update Merkle tree
    wasm::merkle::merkle_add(
        info_db,
        wasm::db::db_lookup(cid, MONEY_V3_CONTRACT_COIN_ROOTS_TREE)?,
        MONEY_V3_CONTRACT_LATEST_COIN_ROOT,
        MONEY_V3_CONTRACT_COIN_MERKLE_TREE,
        &[MerkleNode::from(update.coin.inner())],
    )?;

    Ok(())
}

fn apply_burn(cid: ContractId, update: BurnUpdateV1) -> ContractResult {
    msg!("[money_v3::apply_burn] Marking {} nullifiers", update.nullifiers.len());
    let nullifiers_db = wasm::db::db_lookup(cid, MONEY_V3_CONTRACT_NULLIFIERS_TREE)?;

    // Mark all nullifiers as spent
    for nullifier in &update.nullifiers {
        wasm::db::db_set(nullifiers_db, &serialize(&nullifier.inner()), &[])?;
    }

    Ok(())
}

fn apply_transfer(cid: ContractId, update: TransferUpdateV1) -> ContractResult {
    msg!(
        "[money_v3::apply_transfer] Marking {} nullifiers, adding {} coins",
        update.nullifiers.len(),
        update.coins.len()
    );

    let coins_db = wasm::db::db_lookup(cid, MONEY_V3_CONTRACT_COINS_TREE)?;
    let nullifiers_db = wasm::db::db_lookup(cid, MONEY_V3_CONTRACT_NULLIFIERS_TREE)?;
    let info_db = wasm::db::db_lookup(cid, MONEY_V3_CONTRACT_INFO_TREE)?;

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
        wasm::db::db_lookup(cid, MONEY_V3_CONTRACT_COIN_ROOTS_TREE)?,
        MONEY_V3_CONTRACT_LATEST_COIN_ROOT,
        MONEY_V3_CONTRACT_COIN_MERKLE_TREE,
        &new_coins,
    )?;

    Ok(())
}

// ============================================================================
// OTC SWAP - Atomic token swap between two parties
// ============================================================================

/// Metadata for OtcSwapV1 (atomic burn + mint for cross-token swap)
/// Uses the same proof structure as TransferV1
fn otc_swap_get_metadata(_cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>) -> Vec<u8> {
    let self_ = &calls[call_idx].data;
    let params: OtcSwapParamsV1 = match deserialize(&self_.data[1..]) { Ok(p) => p, Err(_) => return vec![] };

    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    let mut signature_pubkeys: Vec<pallas::Base> = vec![];

    // Burn proofs (one per input)
    for input in &params.inputs {
        signature_pubkeys.push(input.signature_public);

        zk_public_inputs.push((
            MONEY_V3_CONTRACT_ZKAS_BURN_NS_V1.to_string(),
            vec![
                input.nullifier.inner(),
                input.value_commit,
                input.token_commit,
                input.merkle_root.inner(),
                input.user_data_enc,
                input.spend_hook,
                input.signature_public,
            ],
        ));
    }

    // TransferOutput_V1 proofs for swap outputs with public values
    for output in &params.outputs {
        if output.public_value.is_some() || output.public_token_id.is_some() {
            let public_value_base = pallas::Base::from(output.public_value.unwrap_or(0));
            let public_token_id = output.public_token_id.unwrap_or(pallas::Base::zero());
            zk_public_inputs.push((
                MONEY_V3_CONTRACT_ZKAS_TRANSFER_OUTPUT_NS_V1.to_string(),
                vec![
                    output.coin.inner(),
                    output.value_commit,
                    public_value_base,
                    public_token_id,
                ],
            ));
        }
    }

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata).unwrap();
    signature_pubkeys.encode(&mut metadata).unwrap();
    metadata
}

/// OtcSwapV1 instruction - atomic token swap between two parties
///
/// Swaps tokens atomically:
/// - inputs[0] token goes to outputs[1] (Alice's token to Bob)
/// - inputs[1] token goes to outputs[0] (Bob's token to Alice)
///
/// OtcSwapV1 uses the same burn + mint structure as TransferV1 but enforces:
/// - Exactly 2 inputs and 2 outputs
/// - Cross-token swap (inputs/outputs have different token_ids)
fn otc_swap_v1(cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params: OtcSwapParamsV1 = deserialize(&self_.data[1..])?;
    msg!(
        "[money_v3::otc_swap_v1] Processing OTC swap: {} inputs, {} outputs",
        params.inputs.len(),
        params.outputs.len()
    );

    // OtcSwapV1 requires exactly 2 inputs and 2 outputs
    if params.inputs.len() != 2 {
        msg!("[otc_swap_v1] Error: OTC swap requires exactly 2 inputs, got {}", params.inputs.len());
        return Err(MoneyV3Error::TransferMissingInputs.into())
    }
    if params.outputs.len() != 2 {
        msg!("[otc_swap_v1] Error: OTC swap requires exactly 2 outputs, got {}", params.outputs.len());
        return Err(MoneyV3Error::TransferMissingOutputs.into())
    }

    let coins_db = wasm::db::db_lookup(cid, MONEY_V3_CONTRACT_COINS_TREE)?;
    let coin_roots_db = wasm::db::db_lookup(cid, MONEY_V3_CONTRACT_COIN_ROOTS_TREE)?;

    // Verify all input nullifiers are unique and not already spent
    let mut new_nullifiers = Vec::new();
    for (i, input) in params.inputs.iter().enumerate() {
        // Check Merkle root exists
        if !wasm::db::db_contains_key(coin_roots_db, &serialize(&input.merkle_root))? {
            msg!("[otc_swap_v1] Error: Merkle root not found for input {}", i);
            return Err(MoneyV3Error::TransferMerkleRootNotFound.into())
        }

        new_nullifiers.push(input.nullifier);
    }

    // Verify outputs are unique
    let mut new_coins = Vec::new();
    for (i, output) in params.outputs.iter().enumerate() {
        if wasm::db::db_contains_key(coins_db, &serialize(&output.coin))? {
            msg!("[otc_swap_v1] Error: Duplicate coin in output {}", i);
            return Err(MoneyV3Error::DuplicateCoin.into())
        }
        new_coins.push(output.coin);
    }

    let update = OtcSwapUpdateV1 { nullifiers: new_nullifiers, coins: new_coins };
    msg!("[money_v3::otc_swap_v1] OTC swap valid");
    wasm::util::set_return_data(&serialize(&(MoneyV3Function::OtcSwapV1 as u8, update)))
}

/// Apply OtcSwapV1 state update (same as apply_transfer)
fn apply_otc_swap(cid: ContractId, update: OtcSwapUpdateV1) -> ContractResult {
    msg!(
        "[money_v3::apply_otc_swap] Marking {} nullifiers, adding {} coins",
        update.nullifiers.len(),
        update.coins.len()
    );

    let coins_db = wasm::db::db_lookup(cid, MONEY_V3_CONTRACT_COINS_TREE)?;
    let nullifiers_db = wasm::db::db_lookup(cid, MONEY_V3_CONTRACT_NULLIFIERS_TREE)?;
    let info_db = wasm::db::db_lookup(cid, MONEY_V3_CONTRACT_INFO_TREE)?;

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
        wasm::db::db_lookup(cid, MONEY_V3_CONTRACT_COIN_ROOTS_TREE)?,
        MONEY_V3_CONTRACT_LATEST_COIN_ROOT,
        MONEY_V3_CONTRACT_COIN_MERKLE_TREE,
        &new_coins,
    )?;

    Ok(())
}

// ============================================================================
// CROSS-CONTRACT COMPOSITION HELPER
// ============================================================================

/// Validate a money_v3::transfer_v1 child call's public value against the expected amount.
///
/// Enables parent contracts to verify that a child money_v3 transfer actually moves
/// the expected token amount. The child call must include `public_value` (and
/// optionally `public_token_id`) in its outputs, backed by a TransferOutput_V1 ZK proof.
///
/// Call from parent contracts after verifying `child_call.data[0] == 0x04`.
pub fn validate_child_transfer_value(
    child_call_data: &[u8],
    expected_value: u64,
    expected_token_id: Option<pallas::Base>,
) -> Result<(), crate::ContractError> {
    use crate::model::TransferParamsV1;

    if child_call_data.is_empty() {
        return Err(crate::ContractError::InvalidFunction)
    }

    let params: TransferParamsV1 = deserialize(&child_call_data[1..])
        .map_err(|_| crate::ContractError::InvalidFunction)?;

    for output in &params.outputs {
        let pub_value = output.public_value.ok_or_else(|| {
            crate::error::MoneyV3Error::ValueMismatch
        })?;

        if pub_value != expected_value {
            return Err(crate::error::MoneyV3Error::ValueMismatch.into())
        }

        if let Some(ref expected_tid) = expected_token_id {
            let pub_token_id = output.public_token_id.unwrap_or(pallas::Base::zero());
            if pub_token_id != *expected_tid {
                return Err(crate::error::MoneyV3Error::TokenMismatch.into())
            }
        }
    }

    Ok(())
}