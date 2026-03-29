/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

use darkfi_sdk::{
    crypto::ContractId,
    dark_tree::DarkLeaf,
    error::ContractResult,
    msg,
    wasm, ContractCall,
};
use darkfi_serial::deserialize;

use crate::{
    model::{
        AcceptSwapUpdateV1, CancelSwapUpdateV1, CreateSwapUpdateV1, ExecuteSwapUpdateV1,
    },
    DexFunction, DEX_CONTRACT_CONFIG_TREE, DEX_CONTRACT_INFO_TREE,
    DEX_CONTRACT_PARTICIPANTS_TREE, DEX_CONTRACT_SWAPS_TREE,
};

// ============================================================================
// DATABASE KEYS
// ============================================================================

const DEX_DB_VERSION_KEY: &[u8] = b"db_version";
const DEX_SWAP_TIMEOUT_KEY: &[u8] = b"swap_timeout";
const DEX_FEE_KEY: &[u8] = b"dex_fee";
/// Key for storing the trusted money contract Merkle root
pub const DEX_TRUSTED_MONEY_MERKLE_ROOT_KEY: &[u8] = b"trusted_money_merkle_root";

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

// ============================================================================
// CONTRACT DEFINITION
// ============================================================================

darkfi_sdk::define_contract!(
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
    use darkfi_serial::Decodable;
    use crate::model::InitializeParams;

    msg!("[dex::init_contract] Initializing DEX contract");

    // Parse initialization parameters
    let params = InitializeParams::decode(&mut std::io::Cursor::new(ix))
        .map_err(|_| darkfi_sdk::error::ContractError::DecodeError)?;

    msg!(
        "[dex::init_contract] Trusted money Merkle root: {:?}",
        &params.trusted_money_merkle_root
    );

    // Initialize info tree
    let info_db = wasm::db::db_init(cid, DEX_CONTRACT_INFO_TREE)?;
    wasm::db::db_set(info_db, DEX_DB_VERSION_KEY, &env!("CARGO_PKG_VERSION").as_bytes())?;

    // Initialize swaps tree
    wasm::db::db_init(cid, DEX_CONTRACT_SWAPS_TREE)?;

    // Initialize participants tree
    wasm::db::db_init(cid, DEX_CONTRACT_PARTICIPANTS_TREE)?;

    // Initialize config tree
    let config_db = wasm::db::db_init(cid, DEX_CONTRACT_CONFIG_TREE)?;
    wasm::db::db_set(config_db, DEX_SWAP_TIMEOUT_KEY, &params.timeout.encode())?;
    wasm::db::db_set(config_db, DEX_FEE_KEY, &params.fee.encode())?;
    wasm::db::db_set(config_db, DEX_TRUSTED_MONEY_MERKLE_ROOT_KEY, &params.trusted_money_merkle_root)?;

    msg!("[dex::init_contract] DEX contract initialized successfully");
    msg!("[dex::init_contract] WARNING: Using trusted Merkle root from initialization");
    msg!("[dex::init_contract] This is a workaround for lack of cross-contract ZK composition");
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
        DexFunction::UpdateConfigV1 => vec![],
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
        DexFunction::UpdateConfigV1 => vec![],
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
            let update: CreateSwapUpdateV1 = deserialize(&update_data[1..])?;
            dex_create_swap_process_update_v1(cid, update)
        }
        DexFunction::AcceptSwapV1 => {
            let update: AcceptSwapUpdateV1 = deserialize(&update_data[1..])?;
            dex_accept_swap_process_update_v1(cid, update)
        }
        DexFunction::ExecuteSwapV1 => {
            let update: ExecuteSwapUpdateV1 = deserialize(&update_data[1..])?;
            dex_execute_swap_process_update_v1(cid, update)
        }
        DexFunction::CancelSwapV1 => {
            let update: CancelSwapUpdateV1 = deserialize(&update_data[1..])?;
            dex_cancel_swap_process_update_v1(cid, update)
        }
        DexFunction::UpdateConfigV1 => {
            msg!("[dex::process_update] UpdateConfigV1 handled in process_instruction");
            Ok(())
        }
    }
}