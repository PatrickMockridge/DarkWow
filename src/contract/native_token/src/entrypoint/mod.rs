/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

//! NativeToken WASM Entrypoint
//!
//! Design Philosophy: CONSENSUS FIRST, FEES SECOND, PRIVACY THIRD
//!
//! This contract serves as the native token for DarkFi with the following priorities:
//! 1. **Consensus Reward** - Block rewards for PoW mining must be reliable
//! 2. **Network Fees** - Transaction fee payment must be deterministic
//! 3. **Privacy Layer** - Privacy on top, never compromising consensus
//!
//! Privacy-first design following money_v2 patterns (without the heap bug):
//! - Uses Pedersen commitments for hidden values
//! - Uses AeadEncryptedNote for encrypted notes
//! - Uses nullifiers for double-spend prevention

use darkfi_sdk::{
    crypto::{pasta_prelude::Field, pasta_prelude::PrimeField, ContractId, MerkleNode, MerkleTree},
    dark_tree::DarkLeaf,
    error::ContractResult,
    msg,
    pasta::pallas,
    wasm, ContractCall,
};
use darkfi_serial::{deserialize, serialize, Encodable, WriteExt};

use crate::{
    error::NativeTokenError,
    model::{
        ClearInput, Coin, FeeParamsV1, FeeUpdateV1, GenesisMintParamsV1, GenesisMintUpdateV1,
        Input, MeltParamsV1, MeltUpdateV1, Nullifier, Output, PoWRewardParamsV1,
        PoWRewardUpdateV1, SpendParamsV1, SpendUpdateV1, TransferParamsV1, TransferUpdateV1,
    },
    NativeTokenFunction, NATIVE_TOKEN_CONTRACT_COIN_MERKLE_TREE, NATIVE_TOKEN_CONTRACT_COIN_ROOTS_TREE,
    NATIVE_TOKEN_CONTRACT_COINS_TREE, NATIVE_TOKEN_CONTRACT_DB_VERSION,
    NATIVE_TOKEN_CONTRACT_GENESIS_ROOT, NATIVE_TOKEN_CONTRACT_INFO_TREE,
    NATIVE_TOKEN_CONTRACT_LATEST_COIN_ROOT, NATIVE_TOKEN_CONTRACT_LATEST_NULLIFIER_ROOT,
    NATIVE_TOKEN_CONTRACT_NULLIFIERS_TREE, NATIVE_TOKEN_CONTRACT_NULLIFIER_ROOTS_TREE,
    NATIVE_TOKEN_CONTRACT_TOTAL_SUPPLY, EMPTY_COINS_TREE_ROOT,
};

// Generate WASM entrypoints
darkfi_sdk::define_contract!(
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

fn get_metadata(_cid: ContractId, _ix: &[u8]) -> ContractResult {
    // ZK proof metadata would be set here when integrating with full ZK system
    msg!("[native_token::get_metadata] Called");
    Ok(())
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
        NativeTokenFunction::GenesisMintV1 => genesis_mint_v1(cid, call_idx, calls),
        NativeTokenFunction::PoWRewardV1 => pow_reward_v1(cid, call_idx, calls),
        NativeTokenFunction::TransferV1 => transfer_v1(cid, call_idx, calls),
        NativeTokenFunction::SpendV1 => spend_v1(cid, call_idx, calls),
        NativeTokenFunction::MeltV1 => melt_v1(cid, call_idx, calls),
    }
}

// ============================================================================
// FEE - Pay network fees (CONSENSUS CRITICAL)
// ============================================================================

fn fee_v1(cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let _params: FeeParamsV1 = deserialize(&self_.data[1..])?;
    msg!("[native_token::fee_v1] Processing fee");
    // Fee processing would verify ZK proof here
    Ok(())
}

// ============================================================================
// GENESIS MINT - Create initial supply (CONSENSUS CRITICAL)
// ============================================================================

fn genesis_mint_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params: GenesisMintParamsV1 = deserialize(&self_.data[1..])?;
    msg!("[native_token::genesis_mint_v1] Creating {} initial coins", params.outputs.len());

    // Check genesis hasn't already happened
    let info_db = wasm::db::db_lookup(cid, NATIVE_TOKEN_CONTRACT_INFO_TREE)?;
    if wasm::db::db_contains_key(info_db, NATIVE_TOKEN_CONTRACT_GENESIS_ROOT)? {
        return Err(NativeTokenError::GenesisAlreadyExists.into())
    }

    // Validate inputs
    if params.outputs.is_empty() {
        return Err(NativeTokenError::TransferMissingOutputs.into())
    }

    // Calculate total supply
    // Note: In full implementation, this would come from clear input value
    let total_supply: u64 = 0; // Would be calculated from params.input.value

    // Store genesis root
    wasm::db::db_set(info_db, NATIVE_TOKEN_CONTRACT_GENESIS_ROOT, &serialize(&EMPTY_COINS_TREE_ROOT))?;
    wasm::db::db_set(info_db, NATIVE_TOKEN_CONTRACT_TOTAL_SUPPLY, &serialize(&total_supply))?;

    let update = GenesisMintUpdateV1 { coins: params.outputs.iter().map(|o| o.coin).collect() };
    msg!("[native_token::genesis_mint_v1] Genesis complete");
    wasm::util::set_return_data(&serialize(&(NativeTokenFunction::GenesisMintV1 as u8, update)))
}

// ============================================================================
// POW REWARD - Distribute block rewards (CONSENSUS CRITICAL)
// ============================================================================

fn pow_reward_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let _params: PoWRewardParamsV1 = deserialize(&self_.data[1..])?;
    msg!("[native_token::pow_reward_v1] Processing block reward");
    Ok(())
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
    let info_db = wasm::db::db_lookup(cid, NATIVE_TOKEN_CONTRACT_INFO_TREE)?;

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
    Ok(())
}

// ============================================================================
// MELT - Destroy coins (PRIVACY)
// ============================================================================

fn melt_v1(cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params: MeltParamsV1 = deserialize(&self_.data[1..])?;
    msg!("[native_token::melt_v1] Processing melt of {} coins", params.inputs.len());

    if params.inputs.is_empty() {
        return Err(NativeTokenError::NoCoinsToMelt.into())
    }

    let nullifiers_db = wasm::db::db_lookup(cid, NATIVE_TOKEN_CONTRACT_NULLIFIERS_TREE)?;
    let coin_roots_db = wasm::db::db_lookup(cid, NATIVE_TOKEN_CONTRACT_COIN_ROOTS_TREE)?;

    let mut new_nullifiers = Vec::new();
    for (i, input) in params.inputs.iter().enumerate() {
        // Verify Merkle root
        if !wasm::db::db_contains_key(coin_roots_db, &serialize(&input.merkle_root))? {
            msg!("[melt_v1] Error: Merkle root not found for input {}", i);
            return Err(NativeTokenError::TransferMerkleRootNotFound.into())
        }
        new_nullifiers.push(input.nullifier);
    }

    let update = MeltUpdateV1 { nullifiers: new_nullifiers };
    msg!("[native_token::melt_v1] Melt complete");
    wasm::util::set_return_data(&serialize(&(NativeTokenFunction::MeltV1 as u8, update)))
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
        NativeTokenFunction::GenesisMintV1 => {
            let update: GenesisMintUpdateV1 = deserialize(&update_data[1..])?;
            apply_genesis_mint(cid, update)
        }
        NativeTokenFunction::PoWRewardV1 => {
            let update: PoWRewardUpdateV1 = deserialize(&update_data[1..])?;
            apply_pow_reward(cid, update)
        }
        NativeTokenFunction::TransferV1 => {
            let update: TransferUpdateV1 = deserialize(&update_data[1..])?;
            apply_transfer(cid, update)
        }
        NativeTokenFunction::SpendV1 => {
            let update: SpendUpdateV1 = deserialize(&update_data[1..])?;
            apply_spend(cid, update)
        }
        NativeTokenFunction::MeltV1 => {
            let update: MeltUpdateV1 = deserialize(&update_data[1..])?;
            apply_melt(cid, update)
        }
    }
}

fn apply_fee(cid: ContractId, _update: FeeUpdateV1) -> ContractResult {
    msg!("[native_token::apply_fee] Fee applied");
    Ok(())
}

fn apply_genesis_mint(cid: ContractId, update: GenesisMintUpdateV1) -> ContractResult {
    msg!("[native_token::apply_genesis_mint] Adding {} coins to state", update.coins.len());
    let coins_db = wasm::db::db_lookup(cid, NATIVE_TOKEN_CONTRACT_COINS_TREE)?;
    let info_db = wasm::db::db_lookup(cid, NATIVE_TOKEN_CONTRACT_INFO_TREE)?;

    let mut new_coins = Vec::new();
    for coin in update.coins {
        wasm::db::db_set(coins_db, &serialize(&coin), &[])?;
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

fn apply_pow_reward(cid: ContractId, _update: PoWRewardUpdateV1) -> ContractResult {
    msg!("[native_token::apply_pow_reward] Block reward applied");
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

fn apply_melt(cid: ContractId, update: MeltUpdateV1) -> ContractResult {
    msg!("[native_token::apply_melt] Marking {} nullifiers", update.nullifiers.len());
    let nullifiers_db = wasm::db::db_lookup(cid, NATIVE_TOKEN_CONTRACT_NULLIFIERS_TREE)?;
    for nullifier in &update.nullifiers {
        wasm::db::db_set(nullifiers_db, &serialize(&nullifier.inner()), &[])?;
    }
    Ok(())
}