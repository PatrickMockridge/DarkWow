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

//! WASM entrypoint for the DEX contract
//!
//! ## Level 0 MVP: Atomic Swap DAO
//!
//! This contract coordinates bilateral atomic swaps without revealing:
//! - What swaps are being proposed
//! - Who is proposing/acquiring
//! - Amounts being traded
//!
//! ## Flow
//!
//! 1. **CreateSwap**: Proposer locks funds, creates swap proposal
//! 2. **AcceptSwap**: Acceptor locks matching funds
//! 3. **ExecuteSwap**: Both get each other's funds atomically
//! 4. **CancelSwap**: Either party can cancel (triggers refund)

use dwow_sdk::{
    crypto::ContractId,
    dark_tree::DarkLeaf,
    error::{ContractError, ContractResult},
    msg,
    wasm, ContractCall,
};
use dwow_promissory_note_contract::validation::validate_child_contract_id;
use dwow_serial::deserialize;

use crate::{
    model::{
        AcceptSwapUpdateV1, CancelSwapUpdateV1, CreateSwapUpdateV1, ExecuteSwapUpdateV1,
        SetTransparencyLevelParams, UpdateConfigParams,
    },
    DexFunction, DEX_CONTRACT_CONFIG_TREE, DEX_CONTRACT_INFO_TREE,
    DEX_CONTRACT_NULLIFIERS_TREE, DEX_CONTRACT_PARTICIPANTS_TREE,
    DEX_CONTRACT_SWAPS_TREE,
    PROMISSORY_NOTE_CONTRACT_ID_KEY,
};

// ============================================================================
// DATABASE KEYS
// ============================================================================

const DEX_DB_VERSION_KEY: &[u8] = b"db_version";
const DEX_SWAP_TIMEOUT_KEY: &[u8] = b"swap_timeout";
const DEX_FEE_KEY: &[u8] = b"dex_fee";
/// Key for storing the trusted money contract Merkle root
pub const DEX_TRUSTED_MONEY_MERKLE_ROOT_KEY: &[u8] = b"trusted_money_merkle_root";
/// Key for storing transparency level
const DEX_TRANSPARENCY_LEVEL_KEY: &[u8] = b"transparency_level";

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

use crate::model::TransparencyLevel;

// ============================================================================
// SUBMODULES
// ============================================================================

mod create_swap_v1;
use create_swap_v1::{
    dex_create_swap_get_metadata_v1, dex_create_swap_process_instruction_v1,
    dex_create_swap_process_update_v1,
};

mod accept_swap_v1;
use accept_swap_v1::{
    dex_accept_swap_get_metadata_v1, dex_accept_swap_process_instruction_v1,
    dex_accept_swap_process_update_v1,
};

mod execute_swap_v1;
use execute_swap_v1::{
    dex_execute_swap_get_metadata_v1, dex_execute_swap_process_instruction_v1,
    dex_execute_swap_process_update_v1,
};

mod cancel_swap_v1;
use cancel_swap_v1::{
    dex_cancel_swap_get_metadata_v1, dex_cancel_swap_process_instruction_v1,
    dex_cancel_swap_process_update_v1,
};

mod set_transparency_level_v1;
use set_transparency_level_v1::{
    dex_set_transparency_get_metadata_v1, dex_set_transparency_level_process_instruction_v1,
};

mod update_config_v1;
use update_config_v1::{
    dex_update_config_get_metadata_v1, dex_update_config_process_instruction_v1,
};

mod execute_swap_fee_v1;
use execute_swap_fee_v1::{
    dex_execute_swap_fee_get_metadata_v1, dex_execute_swap_fee_process_instruction_v1,
    dex_execute_swap_fee_process_update_v1,
};

mod execute_swap_slippage_v1;
use execute_swap_slippage_v1::{
    dex_execute_swap_slippage_get_metadata_v1, dex_execute_swap_slippage_process_instruction_v1,
    dex_execute_swap_slippage_process_update_v1,
};

// ============================================================================
// CONTRACT DEFINITION
// ============================================================================

dwow_sdk::define_contract!(
    init: init_contract,
    exec: process_instruction,
    apply: process_update,
    metadata: get_metadata
);

// ============================================================================
// INITIALIZATION
// ============================================================================

/// Initialize the DEX contract with trusted setup for money contract integration
///
/// # Trusted Setup
///
/// This function accepts a `trusted_money_merkle_root` which is used to verify
/// lock_proofs in CreateSwap and AcceptSwap. This is a TEMPORARY WORKAROUND due
/// to the lack of cross-contract ZK composition opcodes.
///
/// See module-level documentation for full security considerations.
pub fn init_contract(cid: ContractId, ix: &[u8]) -> ContractResult {
    use dwow_serial::Decodable;
    use crate::model::InitializeParams;

    msg!("[dex::init_contract] Initializing DEX contract");

    // Parse initialization parameters. Empty ix means we're being deployed via
    // deploy_contract() in tests (bypassing Deployooor) — use sensible defaults.
    let params = if ix.is_empty() {
        InitializeParams {
            // Default to the empty Poseidon SMT root over pallas::Base —
            // same as promissory_note::EMPTY_COINS_TREE_ROOT. This allows
            // lock_proofs with zero-sibling paths to verify against the
            // zero leaf (empty tree). Per the o-cap model, this is the
            // root of the token contract's coin tree when no tokens exist.
            timeout: 100, fee: 0, trusted_money_merkle_root: [
                0xb8, 0xc1, 0x07, 0x5a, 0x80, 0xa8, 0x09, 0x65, 0xc2, 0x39, 0x8f, 0x71,
                0x1f, 0xe7, 0x3e, 0x05, 0xb4, 0xed, 0xae, 0xde, 0xf1, 0x62, 0xf2, 0x61,
                0xd4, 0xee, 0xd7, 0xcd, 0x72, 0x74, 0x8d, 0x17,
            ],
            transparency_config: Default::default(),
        }
    } else {
        InitializeParams::decode(&mut std::io::Cursor::new(ix))
            .map_err(|_| dwow_sdk::error::ContractError::IoError("Decode error".to_string()))?
    };

    msg!(
        "[dex::init_contract] Trusted money Merkle root: {:?}",
        &params.trusted_money_merkle_root
    );

    // Initialize info tree
    let info_db = wasm::db::db_init(cid, DEX_CONTRACT_INFO_TREE)?;
    wasm::db::db_set(info_db, DEX_DB_VERSION_KEY, &env!("CARGO_PKG_VERSION").as_bytes())?;
    wasm::db::db_set(info_db, PROMISSORY_NOTE_CONTRACT_ID_KEY, &[0u8; 32])?;

    // Initialize swaps tree
    wasm::db::db_init(cid, DEX_CONTRACT_SWAPS_TREE)?;

    // Initialize participants tree
    wasm::db::db_init(cid, DEX_CONTRACT_PARTICIPANTS_TREE)?;

    // Initialize config tree
    let config_db = wasm::db::db_init(cid, DEX_CONTRACT_CONFIG_TREE)?;
    // Initialize nullifiers tree for governance ZK replay protection
    wasm::db::db_init(cid, DEX_CONTRACT_NULLIFIERS_TREE)?;
    wasm::db::db_set(config_db, DEX_SWAP_TIMEOUT_KEY, &params.timeout.to_le_bytes())?;
    wasm::db::db_set(config_db, DEX_FEE_KEY, &params.fee.to_le_bytes())?;
    wasm::db::db_set(config_db, DEX_TRUSTED_MONEY_MERKLE_ROOT_KEY, &params.trusted_money_merkle_root)?;
    wasm::db::db_set(config_db, DEX_TRANSPARENCY_LEVEL_KEY, &[params.transparency_config.level as u8])?;

    msg!("[dex::init_contract] Transparency level: {:?}", params.transparency_config.level);
    msg!("[dex::init_contract] Price band size: {:?}", params.transparency_config.price_band_size);
    msg!("[dex::init_contract] Volume bucket size: {:?}", params.transparency_config.volume_bucket_size);
    msg!("[dex::init_contract] Anonymity group size: {:?}", params.transparency_config.anonymity_group_size);

    msg!("[dex::init_contract] DEX contract initialized successfully");
    msg!("[dex::init_contract] WARNING: Using trusted Merkle root from initialization");
    msg!("[dex::init_contract] This is a workaround for lack of cross-contract ZK composition");

    let accept_swap_v1_bincode = include_bytes!("../../proof/accept_swap_v1.zk.bin");
    wasm::db::zkas_db_set(&accept_swap_v1_bincode[..])?;
    let cancel_swap_v1_bincode = include_bytes!("../../proof/cancel_swap_v1.zk.bin");
    wasm::db::zkas_db_set(&cancel_swap_v1_bincode[..])?;
    let create_swap_v1_bincode = include_bytes!("../../proof/create_swap_v1.zk.bin");
    wasm::db::zkas_db_set(&create_swap_v1_bincode[..])?;
    let execute_swap_fee_v1_bincode = include_bytes!("../../proof/execute_swap_fee_v1.zk.bin");
    wasm::db::zkas_db_set(&execute_swap_fee_v1_bincode[..])?;
    let execute_swap_slippage_v1_bincode = include_bytes!("../../proof/execute_swap_slippage_v1.zk.bin");
    wasm::db::zkas_db_set(&execute_swap_slippage_v1_bincode[..])?;
    let execute_swap_v1_bincode = include_bytes!("../../proof/execute_swap_v1.zk.bin");
    wasm::db::zkas_db_set(&execute_swap_v1_bincode[..])?;
    let update_config_v1_bincode = include_bytes!("../../proof/update_config_v1.zk.bin");
    wasm::db::zkas_db_set(&update_config_v1_bincode[..])?;
    let set_transparency_level_v1_bincode = include_bytes!("../../proof/set_transparency_level_v1.zk.bin");
    wasm::db::zkas_db_set(&set_transparency_level_v1_bincode[..])?;

    // V2 circuits (HAZOP RC3: domain separation)
    let accept_swap_v2_bincode = include_bytes!("../../proof/accept_swap_v2.zk.bin");
    wasm::db::zkas_db_set(&accept_swap_v2_bincode[..])?;
    let cancel_swap_v2_bincode = include_bytes!("../../proof/cancel_swap_v2.zk.bin");
    wasm::db::zkas_db_set(&cancel_swap_v2_bincode[..])?;
    let create_swap_v2_bincode = include_bytes!("../../proof/create_swap_v2.zk.bin");
    wasm::db::zkas_db_set(&create_swap_v2_bincode[..])?;
    let execute_swap_fee_v2_bincode = include_bytes!("../../proof/execute_swap_fee_v2.zk.bin");
    wasm::db::zkas_db_set(&execute_swap_fee_v2_bincode[..])?;
    let execute_swap_slippage_v2_bincode = include_bytes!("../../proof/execute_swap_slippage_v2.zk.bin");
    wasm::db::zkas_db_set(&execute_swap_slippage_v2_bincode[..])?;
    let execute_swap_v2_bincode = include_bytes!("../../proof/execute_swap_v2.zk.bin");
    wasm::db::zkas_db_set(&execute_swap_v2_bincode[..])?;
    let update_config_v2_bincode = include_bytes!("../../proof/update_config_v2.zk.bin");
    wasm::db::zkas_db_set(&update_config_v2_bincode[..])?;
    let set_transparency_level_v2_bincode = include_bytes!("../../proof/set_transparency_level_v2.zk.bin");
    wasm::db::zkas_db_set(&set_transparency_level_v2_bincode[..])?;

    Ok(())
}

// ============================================================================
// METADATA (ZK proof verification)
// ============================================================================

/// Fetch metadata for ZK proof verification
fn get_metadata(cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = DexFunction::try_from(self_.data[0])?;

    let metadata = match func {
        DexFunction::InitializeV1 => vec![],
        DexFunction::CreateSwapV1 => dex_create_swap_get_metadata_v1(cid, call_idx, calls)?,
        DexFunction::AcceptSwapV1 => dex_accept_swap_get_metadata_v1(cid, call_idx, calls)?,
        DexFunction::ExecuteSwapV1 => dex_execute_swap_get_metadata_v1(cid, call_idx, calls)?,
        DexFunction::CancelSwapV1 => dex_cancel_swap_get_metadata_v1(cid, call_idx, calls)?,
        DexFunction::UpdateConfigV1 => {
            let params= UpdateConfigParams::decode(&self_.data[1..])?;
            dex_update_config_get_metadata_v1(params)?
        }
        DexFunction::SetTransparencyLevelV1 => {
            let params= SetTransparencyLevelParams::decode(&self_.data[1..])?;
            dex_set_transparency_get_metadata_v1(params)?
        }
        DexFunction::ExecuteSwapFeeV1 => dex_execute_swap_fee_get_metadata_v1(cid, call_idx, calls)?,
        DexFunction::ExecuteSwapSlippageV1 => dex_execute_swap_slippage_get_metadata_v1(cid, call_idx, calls)?,
    };

    wasm::util::set_return_data(&metadata)
}

// ============================================================================
// INSTRUCTION PROCESSING (state transition verification)
// ============================================================================

/// Verify state transition and produce update if valid
fn process_instruction(cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = DexFunction::try_from(self_.data[0])?;

    let update_data = match func {
        DexFunction::InitializeV1 => vec![],
        DexFunction::CreateSwapV1 => dex_create_swap_process_instruction_v1(cid, call_idx, calls)?,
        DexFunction::AcceptSwapV1 => dex_accept_swap_process_instruction_v1(cid, call_idx, calls)?,
        DexFunction::ExecuteSwapV1 => dex_execute_swap_process_instruction_v1(cid, call_idx, calls)?,
        DexFunction::CancelSwapV1 => dex_cancel_swap_process_instruction_v1(cid, call_idx, calls)?,
        DexFunction::UpdateConfigV1 => dex_update_config_process_instruction_v1(cid, call_idx, calls)?,
        DexFunction::SetTransparencyLevelV1 => dex_set_transparency_level_process_instruction_v1(cid, call_idx, calls)?,
        DexFunction::ExecuteSwapFeeV1 => dex_execute_swap_fee_process_instruction_v1(cid, call_idx, calls)?,
        DexFunction::ExecuteSwapSlippageV1 => dex_execute_swap_slippage_process_instruction_v1(cid, call_idx, calls)?,
    };

    wasm::util::set_return_data(&update_data)
}

// ============================================================================
// STATE UPDATE (write new state)
// ============================================================================

/// Write state update after successful verification
fn process_update(cid: ContractId, update_data: &[u8]) -> ContractResult {
    match DexFunction::try_from(update_data[0])? {
        DexFunction::InitializeV1 => {
            msg!("[dex::process_update] InitializeV1 has no update data");
            Ok(())
        }
        DexFunction::CreateSwapV1 => {
            let update = CreateSwapUpdateV1::decode(&update_data[1..])?;
            dex_create_swap_process_update_v1(cid, update)
        }
        DexFunction::AcceptSwapV1 => {
            let update = AcceptSwapUpdateV1::decode(&update_data[1..])?;
            dex_accept_swap_process_update_v1(cid, update)
        }
        DexFunction::ExecuteSwapV1 => {
            let update = ExecuteSwapUpdateV1::decode(&update_data[1..])?;
            dex_execute_swap_process_update_v1(cid, update)
        }
        DexFunction::CancelSwapV1 => {
            let update = CancelSwapUpdateV1::decode(&update_data[1..])?;
            dex_cancel_swap_process_update_v1(cid, update)
        }
        DexFunction::UpdateConfigV1 => {
            msg!("[dex::process_update] UpdateConfigV1 handled in process_instruction");
            Ok(())
        }
        DexFunction::SetTransparencyLevelV1 => {
            msg!("[dex::process_update] SetTransparencyLevelV1 handled in process_instruction");
            Ok(())
        }
        DexFunction::ExecuteSwapFeeV1 => {
            let update = ExecuteSwapUpdateV1::decode(&update_data[1..])?;
            dex_execute_swap_fee_process_update_v1(cid, update)
        }
        DexFunction::ExecuteSwapSlippageV1 => {
            let update = ExecuteSwapUpdateV1::decode(&update_data[1..])?;
            dex_execute_swap_slippage_process_update_v1(cid, update)
        }
    }
}