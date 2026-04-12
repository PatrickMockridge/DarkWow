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

//! SimpleCoin WASM Entrypoint
//!
//! This contract uses SIGNATURE-BASED verification for transfers.
//! No ZK proofs are required for basic operations.
//!
//! This is a key design decision: we use existing DarkFi signature
//! infrastructure instead of complex ZK circuits for the baseline token.

use darkfi_sdk::{
    crypto::{MerkleNode, ContractId, pasta_prelude::Field},
    dark_tree::DarkLeaf,
    error::{ContractResult},
    msg, ContractCall,
    pasta::pallas,
    wasm,
};
use darkfi_serial::{deserialize, serialize};

use crate::{
    error::SimplecoinError,
    model::{
        Coin, GenesisParamsV1, GenesisUpdateV1, Input, MeltParamsV1, MeltUpdateV1,
        Nullifier, Output, SpendParamsV1, SpendUpdateV1, TransferParamsV1, TransferUpdateV1,
    },
    SimplecoinFunction, SIMPLECOIN_CONTRACT_COINS_TREE, SIMPLECOIN_CONTRACT_GENESIS_ROOT,
    SIMPLECOIN_CONTRACT_INFO_TREE, SIMPLECOIN_CONTRACT_MERKLE_TREE,
    SIMPLECOIN_CONTRACT_NULLIFIERS_TREE, SIMPLECOIN_CONTRACT_TOTAL_SUPPLY,
};

// Generate WASM entrypoints
darkfi_sdk::define_contract!(
    init: init_contract,
    exec: process_instruction,
    apply: process_update,
    metadata: get_metadata
);

// ============================================================================
// CONTRACT INITIALIZATION
// ============================================================================

pub fn init_contract(cid: ContractId, _ix: &[u8]) -> ContractResult {
    msg!("[simplecoin::init_contract] Initializing simplecoin contract");

    // Initialize database trees
    let info_db = wasm::db::db_init(cid, SIMPLECOIN_CONTRACT_INFO_TREE)?;
    wasm::db::db_set(info_db, b"db_version", env!("CARGO_PKG_VERSION").as_bytes())?;

    let _coins_db = wasm::db::db_init(cid, SIMPLECOIN_CONTRACT_COINS_TREE)?;
    let _nullifiers_db = wasm::db::db_init(cid, SIMPLECOIN_CONTRACT_NULLIFIERS_TREE)?;
    let _merkle_db = wasm::db::db_init(cid, SIMPLECOIN_CONTRACT_MERKLE_TREE)?;

    msg!("[simplecoin::init_contract] Database trees initialized");

    Ok(())
}

// ============================================================================
// METADATA (ZK PROOF SETUP) - Simplified for baseline
// ============================================================================

fn get_metadata(_cid: ContractId, _ix: &[u8]) -> ContractResult {
    // For baseline simplecoin, we don't use ZK proofs for basic operations.
    // This would be extended if we add privacy features later.
    msg!("[simplecoin::get_metadata] Called - no ZK metadata needed for baseline");
    Ok(())
}

// ============================================================================
// INSTRUCTION PROCESSING (STATE TRANSITION VERIFICATION)
// ============================================================================

fn process_instruction(cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = SimplecoinFunction::try_from(self_.data[0])?;

    match func {
        SimplecoinFunction::GenesisV1 => genesis_v1(cid, call_idx, calls),
        SimplecoinFunction::TransferV1 => transfer_v1(cid, call_idx, calls),
        SimplecoinFunction::SpendV1 => spend_v1(cid, call_idx, calls),
        SimplecoinFunction::MeltV1 => melt_v1(cid, call_idx, calls),
    }
}

// ============================================================================
// GENESIS - Create initial coin supply
// ============================================================================

fn genesis_v1(cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params: GenesisParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[simplecoin::genesis_v1] Creating {} initial coins", params.coins.len());

    // Check genesis hasn't already happened
    let info_db = wasm::db::db_lookup(cid, SIMPLECOIN_CONTRACT_INFO_TREE)?;
    if wasm::db::db_contains_key(info_db, SIMPLECOIN_CONTRACT_GENESIS_ROOT)? {
        return Err(SimplecoinError::GenesisAlreadyExists.into())
    }

    // Validate coins
    let mut total_supply: u64 = 0;
    for coin in &params.coins {
        if coin.value < 1 {
            return Err(SimplecoinError::InvalidCoinValue.into())
        }
        total_supply = total_supply.checked_add(coin.value).ok_or(SimplecoinError::ValueOverflow)?;
    }

    // For genesis, we don't need Merkle proof verification - just add coins
    let update = GenesisUpdateV1 { coins: params.coins.clone() };

    // Store genesis root (empty for now since we're starting fresh)
    let merkle_root = MerkleNode::from(pallas::Base::ZERO);
    wasm::db::db_set(info_db, SIMPLECOIN_CONTRACT_GENESIS_ROOT, &serialize(&merkle_root))?;
    wasm::db::db_set(info_db, SIMPLECOIN_CONTRACT_TOTAL_SUPPLY, &serialize(&total_supply))?;

    msg!("[simplecoin::genesis_v1] Genesis complete. Total supply: {}", total_supply);

    wasm::util::set_return_data(&serialize(&(SimplecoinFunction::GenesisV1 as u8, update)))
}

// ============================================================================
// TRANSFER - Send coins to another party (SIGNATURE-BASED, NO ZK)
// ============================================================================

fn transfer_v1(cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params: TransferParamsV1 = deserialize(&self_.data[1..])?;

    msg!(
        "[simplecoin::transfer_v1] Processing transfer: {} inputs, {} outputs",
        params.inputs.len(),
        params.outputs.len()
    );

    let coins_db = wasm::db::db_lookup(cid, SIMPLECOIN_CONTRACT_COINS_TREE)?;
    let nullifiers_db = wasm::db::db_lookup(cid, SIMPLECOIN_CONTRACT_NULLIFIERS_TREE)?;
    let _merkle_db = wasm::db::db_lookup(cid, SIMPLECOIN_CONTRACT_MERKLE_TREE)?;

    // Validate input count
    if params.inputs.is_empty() || params.inputs.len() > 16 {
        return Err(SimplecoinError::TooManyCoins.into())
    }

    // Validate output count
    if params.outputs.is_empty() || params.outputs.len() > 16 {
        return Err(SimplecoinError::TooManyCoins.into())
    }

    // Verify value balance: sum(inputs) == sum(outputs)
    let input_total: u64 = params.inputs.iter().map(|i| i.coin.value).sum();
    let output_total: u64 = params.outputs.iter().map(|o| o.coin.value).sum();

    if input_total != output_total {
        return Err(SimplecoinError::InsufficientBalance.into())
    }

    // Process each input
    let mut nullifiers = Vec::new();
    for input in &params.inputs {
        // Verify coin not already spent
        let coin_id = input.coin.coin_id();
        let nullifier = Nullifier::new(coin_id);

        if wasm::db::db_contains_key(nullifiers_db, &serialize(&nullifier.inner()))? {
            return Err(SimplecoinError::CoinAlreadySpent.into())
        }

        // Note: For baseline, we skip Merkle proof verification
        // A full implementation would verify: merkle_verify(coin_id, input.merkle_root, input.merkle_path)

        nullifiers.push(nullifier);
    }

    // All inputs valid - create update
    let update = TransferUpdateV1 {
        nullifiers: nullifiers.clone(),
        coins: params.outputs.iter().map(|o| o.coin.clone()).collect(),
    };

    msg!("[simplecoin::transfer_v1] Transfer valid, {} coins spent", nullifiers.len());

    wasm::util::set_return_data(&serialize(&(SimplecoinFunction::TransferV1 as u8, update)))
}

// ============================================================================
// SPEND - Consume coins, create change
// ============================================================================

fn spend_v1(cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params: SpendParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[simplecoin::spend_v1] Processing spend");

    let nullifiers_db = wasm::db::db_lookup(cid, SIMPLECOIN_CONTRACT_NULLIFIERS_TREE)?;
    let _coins_db = wasm::db::db_lookup(cid, SIMPLECOIN_CONTRACT_COINS_TREE)?;

    // Verify input coin not already spent
    let coin_id = params.input.coin.coin_id();
    let nullifier = Nullifier::new(coin_id);

    if wasm::db::db_contains_key(nullifiers_db, &serialize(&nullifier.inner()))? {
        return Err(SimplecoinError::CoinAlreadySpent.into())
    }

    // Verify change output has valid value
    if params.change_output.coin.value < params.fee {
        return Err(SimplecoinError::InsufficientBalance.into())
    }

    let update = SpendUpdateV1 {
        nullifier: nullifier.clone(),
        change_coin: params.change_output.coin.clone(),
    };

    msg!("[simplecoin::spend_v1] Spend valid");

    wasm::util::set_return_data(&serialize(&(SimplecoinFunction::SpendV1 as u8, update)))
}

// ============================================================================
// MELT - Destroy coins (e.g., for fees)
// ============================================================================

fn melt_v1(cid: ContractId, call_idx: usize, calls: Vec<DarkLeaf<ContractCall>>) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params: MeltParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[simplecoin::melt_v1] Processing melt of {} coins", params.inputs.len());

    if params.inputs.is_empty() {
        return Err(SimplecoinError::NoCoinsToMelt.into())
    }

    let nullifiers_db = wasm::db::db_lookup(cid, SIMPLECOIN_CONTRACT_NULLIFIERS_TREE)?;
    let info_db = wasm::db::db_lookup(cid, SIMPLECOIN_CONTRACT_INFO_TREE)?;

    // Verify total input value >= melt amount
    let input_total: u64 = params.inputs.iter().map(|i| i.coin.value).sum();
    if input_total < params.melt_amount {
        return Err(SimplecoinError::InsufficientBalance.into())
    }

    // Mark all inputs as spent
    let mut nullifiers = Vec::new();
    for input in &params.inputs {
        let coin_id = input.coin.coin_id();
        let nullifier = Nullifier::new(coin_id);

        if wasm::db::db_contains_key(nullifiers_db, &serialize(&nullifier.inner()))? {
            return Err(SimplecoinError::CoinAlreadySpent.into())
        }

        nullifiers.push(nullifier);
    }

    // Update total supply (subtract melted amount)
    // In a real implementation, we'd track and reduce supply here

    let update = MeltUpdateV1 { nullifiers, melt_amount: params.melt_amount };

    msg!("[simplecoin::melt_v1] Melt complete. Amount melted: {}", params.melt_amount);

    wasm::util::set_return_data(&serialize(&(SimplecoinFunction::MeltV1 as u8, update)))
}

// ============================================================================
// STATE UPDATE (WRITE STATE AFTER VERIFICATION)
// ============================================================================

fn process_update(cid: ContractId, update_data: &[u8]) -> ContractResult {
    let func = SimplecoinFunction::try_from(update_data[0])?;

    match func {
        SimplecoinFunction::GenesisV1 => {
            let update: GenesisUpdateV1 = deserialize(&update_data[1..])?;
            apply_genesis(cid, update)
        }
        SimplecoinFunction::TransferV1 => {
            let update: TransferUpdateV1 = deserialize(&update_data[1..])?;
            apply_transfer(cid, update)
        }
        SimplecoinFunction::SpendV1 => {
            let update: SpendUpdateV1 = deserialize(&update_data[1..])?;
            apply_spend(cid, update)
        }
        SimplecoinFunction::MeltV1 => {
            let update: MeltUpdateV1 = deserialize(&update_data[1..])?;
            apply_melt(cid, update)
        }
    }
}

fn apply_genesis(cid: ContractId, update: GenesisUpdateV1) -> ContractResult {
    msg!("[simplecoin::apply_genesis] Adding {} coins to state", update.coins.len());

    let coins_db = wasm::db::db_lookup(cid, SIMPLECOIN_CONTRACT_COINS_TREE)?;
    let _merkle_db = wasm::db::db_lookup(cid, SIMPLECOIN_CONTRACT_MERKLE_TREE)?;

    for coin in update.coins {
        let coin_id = coin.coin_id();
        wasm::db::db_set(coins_db, &serialize(&coin_id), &serialize(&coin))?;
        // Note: Would also update Merkle tree here
        msg!("[simplecoin::apply_genesis] Added coin: {:?}", coin_id);
    }

    Ok(())
}

fn apply_transfer(cid: ContractId, update: TransferUpdateV1) -> ContractResult {
    msg!(
        "[simplecoin::apply_transfer] Marking {} nullifiers, adding {} coins",
        update.nullifiers.len(),
        update.coins.len()
    );

    let coins_db = wasm::db::db_lookup(cid, SIMPLECOIN_CONTRACT_COINS_TREE)?;
    let nullifiers_db = wasm::db::db_lookup(cid, SIMPLECOIN_CONTRACT_NULLIFIERS_TREE)?;

    // Mark nullifiers (coins spent)
    for nullifier in update.nullifiers {
        wasm::db::db_set(nullifiers_db, &serialize(&nullifier.inner()), &[])?;
    }

    // Add new coins
    for coin in update.coins {
        let coin_id = coin.coin_id();
        wasm::db::db_set(coins_db, &serialize(&coin_id), &serialize(&coin))?;
    }

    Ok(())
}

fn apply_spend(cid: ContractId, update: SpendUpdateV1) -> ContractResult {
    msg!("[simplecoin::apply_spend] Marking nullifier and adding change coin");

    let nullifiers_db = wasm::db::db_lookup(cid, SIMPLECOIN_CONTRACT_NULLIFIERS_TREE)?;
    let coins_db = wasm::db::db_lookup(cid, SIMPLECOIN_CONTRACT_COINS_TREE)?;

    // Mark input as spent
    wasm::db::db_set(nullifiers_db, &serialize(&update.nullifier.inner()), &[])?;

    // Add change coin
    let coin_id = update.change_coin.coin_id();
    wasm::db::db_set(coins_db, &serialize(&coin_id), &serialize(&update.change_coin))?;

    Ok(())
}

fn apply_melt(cid: ContractId, update: MeltUpdateV1) -> ContractResult {
    msg!("[simplecoin::apply_melt] Marking {} nullifiers", update.nullifiers.len());

    let nullifiers_db = wasm::db::db_lookup(cid, SIMPLECOIN_CONTRACT_NULLIFIERS_TREE)?;

    for nullifier in update.nullifiers {
        wasm::db::db_set(nullifiers_db, &serialize(&nullifier.inner()), &[])?;
    }

    Ok(())
}