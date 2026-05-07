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

//! WASM entrypoint for the atomic swap contract
//!
//! ## Cross-Chain Atomic Swap
//!
//! This contract enables trustless cross-chain swaps via Hashed Timelock Contract.
//!
//! ## HTLC Flow
//!
//! ```
//! DarkWow                          External Chain (e.g., Ethereum)
//! ──────────────────────────────────────────────────────────────────
//!
//!  1. Alice (initiator) creates swap on DarkWow
//!     - Locks X tokens
//!     - hash = poseidon_hash(secret)
//!     - timelock = current_block + N
//!
//!  2. Alice sends hash to Bob on external chain              ──────────►
//!
//!  3. Bob verifies hash matches, creates HTLC on external chain
//!     - Locks Y tokens
//!     - Same hash
//!     - timelock = current_block + N + δ (δ = verification delay)
//!
//!  4. Bob claims on DarkWow with secret
//!     - funds released to Bob
//!
//!  5. Alice claims on external chain with secret
//!     - funds released to Alice
//!
//!  If timelock expires:
//!  - Alice refunds on DarkWow (after timelock)
//!  - Bob refunds on external chain (after external timelock)
//! ```

use darkfi_sdk::{
    crypto::ContractId,
    error::ContractResult,
    msg,
    wasm,
};
use darkfi_serial::deserialize;

use crate::{
    model::{
        ClaimUpdateV1, CreateSwapUpdateV1, RefundUpdateV1,
    },
    AtomicSwapFunction, ATOMIC_SWAP_CONTRACT_INFO_TREE,
    ATOMIC_SWAP_CONTRACT_NULLIFIERS_TREE,
    ATOMIC_SWAP_CONTRACT_SECRETS_TREE,
    ATOMIC_SWAP_CONTRACT_SWAPS_TREE,
};

// ============================================================================
// DATABASE KEYS
// ============================================================================

const ATOMIC_SWAP_DB_VERSION_KEY: &[u8] = b"db_version";

// ============================================================================
// ENTRYPOINT SUBMODULES
// ============================================================================

/// `AtomicSwap::CreateSwap` functions
mod create_swap;
use create_swap::{
    atomic_swap_create_get_metadata_v1, atomic_swap_create_process_instruction_v1,
    atomic_swap_create_process_update_v1,
};

/// `AtomicSwap::Claim` functions
mod claim_swap;
use claim_swap::{
    atomic_swap_claim_get_metadata_v1, atomic_swap_claim_process_instruction_v1,
    atomic_swap_claim_process_update_v1,
};

/// `AtomicSwap::Refund` functions
mod refund_swap;
use refund_swap::{
    atomic_swap_refund_get_metadata_v1, atomic_swap_refund_process_instruction_v1,
    atomic_swap_refund_process_update_v1,
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

/// Initialize atomic swap contract state
///
/// Sets up:
/// - Info tree (version, config)
/// - Swaps tree (active swaps)
/// - Secrets tree (revealed secrets)
/// - Nullifiers tree (prevents double-claim/refund)
pub fn init_contract(cid: ContractId, _ix: &[u8]) -> ContractResult {
    msg!("[atomic_swap::init_contract] Initializing atomic swap contract");

    // Initialize info tree
    let info_db = wasm::db::db_init(cid, ATOMIC_SWAP_CONTRACT_INFO_TREE)?;
    wasm::db::db_set(
        info_db,
        ATOMIC_SWAP_DB_VERSION_KEY,
        &env!("CARGO_PKG_VERSION").as_bytes(),
    )?;

    // Initialize swaps tree
    wasm::db::db_init(cid, ATOMIC_SWAP_CONTRACT_SWAPS_TREE)?;

    // Initialize secrets tree
    wasm::db::db_init(cid, ATOMIC_SWAP_CONTRACT_SECRETS_TREE)?;

    // Initialize nullifiers tree
    wasm::db::db_init(cid, ATOMIC_SWAP_CONTRACT_NULLIFIERS_TREE)?;

    msg!("[atomic_swap::init_contract] Atomic swap contract initialized successfully");
    Ok(())
}

// ============================================================================
// METADATA (ZK proof verification)
// ============================================================================

/// Fetch metadata for ZK proof verification
fn get_metadata(cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<darkfi_sdk::dark_tree::DarkLeaf<darkfi_sdk::ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = AtomicSwapFunction::try_from(self_.data[0])?;

    let metadata = match func {
        AtomicSwapFunction::CreateSwapV1 => atomic_swap_create_get_metadata_v1(cid, call_idx, calls)?,
        AtomicSwapFunction::ClaimV1 => atomic_swap_claim_get_metadata_v1(cid, call_idx, calls)?,
        AtomicSwapFunction::RefundV1 => atomic_swap_refund_get_metadata_v1(cid, call_idx, calls)?,
        AtomicSwapFunction::InitializeV1 => vec![],
    };

    wasm::util::set_return_data(&metadata)
}

// ============================================================================
// INSTRUCTION PROCESSING (state transition verification)
// ============================================================================

/// Verify state transition and produce update if valid
fn process_instruction(cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<darkfi_sdk::dark_tree::DarkLeaf<darkfi_sdk::ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = AtomicSwapFunction::try_from(self_.data[0])?;

    let update_data = match func {
        AtomicSwapFunction::CreateSwapV1 => {
            atomic_swap_create_process_instruction_v1(cid, call_idx, calls)?
        }
        AtomicSwapFunction::ClaimV1 => {
            atomic_swap_claim_process_instruction_v1(cid, call_idx, calls)?
        }
        AtomicSwapFunction::RefundV1 => {
            atomic_swap_refund_process_instruction_v1(cid, call_idx, calls)?
        }
        AtomicSwapFunction::InitializeV1 => vec![],
    };

    wasm::util::set_return_data(&update_data)
}

// ============================================================================
// STATE UPDATE (write new state)
// ============================================================================

/// Write state update after successful verification
fn process_update(cid: ContractId, update_data: &[u8]) -> ContractResult {
    match AtomicSwapFunction::try_from(update_data[0])? {
        AtomicSwapFunction::CreateSwapV1 => {
            let update: CreateSwapUpdateV1 = deserialize(&update_data[1..])?;
            Ok(atomic_swap_create_process_update_v1(cid, update)?)
        }
        AtomicSwapFunction::ClaimV1 => {
            let update: ClaimUpdateV1 = deserialize(&update_data[1..])?;
            Ok(atomic_swap_claim_process_update_v1(cid, update)?)
        }
        AtomicSwapFunction::RefundV1 => {
            let update: RefundUpdateV1 = deserialize(&update_data[1..])?;
            Ok(atomic_swap_refund_process_update_v1(cid, update)?)
        }
        AtomicSwapFunction::InitializeV1 => {
            msg!("[atomic_swap::process_update] InitializeV1 has no update data");
            Ok(())
        }
    }
}

// ============================================================================
// HTLC LOGIC OVERVIEW
// ============================================================================
//
// The HTLC logic verifies:
//
// CreateSwap:
//   - User locks funds in contract
//   - Hash is provided: hash = poseidon_hash(secret)
//   - Timelock set to prevent premature refund
//   - Swap state = Created
//
// Claim:
//   - User proves knowledge of secret
//   - hash(secret) must equal the stored hash
//   - State transitions: Created -> Claimed
//   - Secret is revealed (Bob can now claim on external chain)
//
// Refund:
//   - Prover verifies current_block >= timelock
//   - User didn't claim in time
//   - State transitions: Created -> Refunded
//   - Emit nullifier to prevent claim after refund
//
// ============================================================================
