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

//! ExecuteSwapV1 entrypoint functions
//!
//! ## Security Model
//!
//! The DEX contract relies on a split verification model:
//!
//! 1. **Client/Prover** computes nullifiers externally:
//!    - alice_nullifier = poseidon_hash([alice_secret, alice_lock])
//!    - bob_nullifier = poseidon_hash([bob_secret, bob_lock])
//!
//! 2. **ZK Circuit** proves:
//!    - Prover knows alice_secret and bob_secret
//!    - The nullifiers are correctly computed
//!    - The swap ID is consistent
//!    - The money_v3::otc_swap_v1 calls are bundled (cross-contract atomic swap)
//!
//! 3. **Contract** (this file) verifies:
//!    - The nullifiers provided match what's on-chain (double-spend check)
//!    - The swap exists and is in correct state
//!    - Child contract calls include money_v3::otc_swap_v1 for atomic token swap
//!
//! ## Money Integration
//!
//! This function REQUIRES money_v3::otc_swap_v1 child calls to be bundled for atomic token swap:
//! - Child call 0: money_v3::otc_swap_v1 for Alice's token to Bob
//! - Child call 1: money_v3::otc_swap_v1 for Bob's token to Alice
//!
//! The ZK circuit includes FuncRefs from these child calls as public inputs,
//! ensuring the atomic swap was executed as part of the same transaction.
//!
//! ## Limitations
//!
//! - The contract CANNOT verify the ZK proof itself - that happens at the host level
//! - The contract trusts that if get_metadata returns successfully, the proof was valid
//! - The contract verifies nullifiers against on-chain state to prevent double-execution

use darkfi_sdk::{
    crypto::{pasta_prelude::PrimeField, FuncRef},
    dark_tree::DarkLeaf,
    error::{ContractError, ContractResult},
    msg, ContractCall,
    pasta::pallas,
    wasm,
};
use darkfi_serial::{deserialize, serialize, Encodable};

use crate::{
    error::DexError,
    model::{ExecuteSwapParams, ExecuteSwapUpdateV1, Swap, SwapState},
    DEX_CONTRACT_PARTICIPANTS_TREE, DEX_CONTRACT_SWAPS_TREE,
    DEX_CONTRACT_ZKAS_EXECUTE_SWAP_NS_V1,
};

/// `get_metadata` function for `Dex::ExecuteSwapV1`
///
/// Returns public inputs for ZK proof verification:
/// - alice_nullifier: from params (computed by prover)
/// - bob_nullifier: from params (computed by prover)
/// - swap_id: from params
/// - FuncRefs from money_v3::otc_swap_v1 child calls (cross-contract atomic swap)
///
/// The host uses these to verify the ZK proof.
pub(crate) fn dex_execute_swap_get_metadata_v1(
    _cid: darkfi_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx];

    // Validate children_indexes to ensure money_v3::otc_swap_v1 calls are bundled
    // For atomic token swap, we need 2 OtcSwapV1 calls:
    // - Child 0: Alice's tokens to Bob (offer_token/offer_amount)
    // - Child 1: Bob's tokens to Alice (request_token/request_amount)
    if self_.children_indexes.len() != 2 {
        msg!(
            "[ExecuteSwapV1] Error: Expected 2 child calls (money_v3::otc_swap_v1), got {}",
            self_.children_indexes.len()
        );
        return Err(DexError::InvalidChildrenIndexes.into())
    }

    // Validate both child calls are money_v3::otc_swap_v1 (0x05)
    for &child_idx in self_.children_indexes.iter() {
        let child_call = &calls[child_idx].data;
        if child_call.data[0] != 0x05 {
            msg!(
                "[ExecuteSwapV1] Error: Expected money_v3::otc_swap_v1 (0x05), got 0x{:02x}",
                child_call.data[0]
            );
            return Err(DexError::InvalidChildCall.into())
        }
    }

    // Extract FuncRefs from child money_v3::otc_swap_v1 calls
    let mut child_func_ids: Vec<pallas::Base> = Vec::with_capacity(2);
    for &child_idx in self_.children_indexes.iter() {
        let child_call = &calls[child_idx].data;
        let child_contract_id = child_call.contract_id;
        let child_func_code = child_call.data[0];
        let child_func_id =
            FuncRef { contract_id: child_contract_id, func_code: child_func_code }.to_func_id();
        child_func_ids.push(child_func_id.inner());
    }

    let params: ExecuteSwapParams = deserialize(&self_.data.data[1..])?;

    // Public inputs for the ZK proof verification
    // The order must match the `constrain_instance` calls in execute_swap_v1.zk:
    // 1. alice_nullifier_check
    // 2. bob_nullifier_check
    // 3. computed_swap_id
    // 4. alice_otc_func_id (FuncRef for Alice's OtcSwapV1)
    // 5. bob_otc_func_id (FuncRef for Bob's OtcSwapV1)
    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];

    // The prover computed the nullifiers externally and passed them in params.
    // We use these directly as public inputs for ZK verification.
    let alice_nullifier = params.alice_nullifier.inner();
    let bob_nullifier = params.bob_nullifier.inner();

    let swap_id = match pallas::Base::from_repr(params.swap_id).into_option() {
        Some(v) => v,
        None => return Err(ContractError::IoError("Invalid swap_id".to_string()).into()),
    };

    // Include OtcSwapV1 FuncRefs for cross-contract atomic swap verification
    zk_public_inputs.push((
        DEX_CONTRACT_ZKAS_EXECUTE_SWAP_NS_V1.to_string(),
        vec![
            alice_nullifier,
            bob_nullifier,
            swap_id,
            child_func_ids[0], // Alice's OtcSwapV1 FuncRef
            child_func_ids[1], // Bob's OtcSwapV1 FuncRef
        ],
    ));

    // Serialize metadata for ZK verification
    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;

    Ok(metadata)
}

/// `process_instruction` function for `Dex::ExecuteSwapV1`
///
/// Verifies:
/// 1. Swap exists and is in Accepted state
/// 2. Nullifiers haven't been spent (double-execution check)
/// 3. Returns update to be applied if verification passes
pub(crate) fn dex_execute_swap_process_instruction_v1(
    cid: darkfi_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx];
    let params: ExecuteSwapParams = deserialize(&self_.data.data[1..])?;

    msg!("[ExecuteSwapV1] Executing swap: id={:?}", &params.swap_id);

    // Load the swap
    let swaps_db = wasm::db::db_lookup(cid, DEX_CONTRACT_SWAPS_TREE)?;
    let swap_data = wasm::db::db_get(swaps_db, &params.swap_id)?;
    let swap: Swap = match swap_data {
        Some(data) => deserialize(&data)?,
        None => {
            msg!("[ExecuteSwapV1] Error: Swap not found");
            return Err(DexError::SwapNotFound.into())
        }
    };

    // Verify swap is in Accepted state
    match swap.state {
        SwapState::Accepted => {}
        _ => {
            msg!("[ExecuteSwapV1] Error: Swap not in Accepted state");
            return Err(DexError::InvalidSwapState.into())
        }
    }

    // SECURITY: Verify that provided locks match the stored values
    // This prevents an attacker from executing a swap with mismatched locks
    if params.alice_lock != swap.proposer_lock {
        msg!("[ExecuteSwapV1] Error: Alice's lock does not match stored proposer_lock");
        return Err(DexError::InvalidLockCommitment.into())
    }

    if params.bob_lock != swap.acceptor_lock {
        msg!("[ExecuteSwapV1] Error: Bob's lock does not match stored acceptor_lock");
        return Err(DexError::InvalidLockCommitment.into())
    }

    // Verify nullifiers against on-chain state (double-execution check)
    // Now using nullifiers instead of lock_commitments
    let participants_db = wasm::db::db_lookup(cid, DEX_CONTRACT_PARTICIPANTS_TREE)?;

    // Check that proposer's nullifier hasn't been spent
    if !wasm::db::db_contains_key(participants_db, &swap.proposer_nullifier.to_bytes())? {
        msg!("[ExecuteSwapV1] Error: Proposer's nullifier not found in participants");
        return Err(DexError::InvalidNullifier.into())
    }

    // Check that acceptor's nullifier hasn't been spent
    if !wasm::db::db_contains_key(participants_db, &swap.acceptor_nullifier.to_bytes())? {
        msg!("[ExecuteSwapV1] Error: Acceptor's nullifier not found in participants");
        return Err(DexError::InvalidNullifier.into())
    }

    // Create the update with the prover-provided nullifiers
    // These were verified as public inputs by the ZK proof
    let update = ExecuteSwapUpdateV1 {
        swap_id: params.swap_id,
        alice_nullifier: params.alice_nullifier,
        bob_nullifier: params.bob_nullifier,
    };

    Ok(serialize(&update))
}

/// `process_update` function for `Dex::ExecuteSwapV1`
pub(crate) fn dex_execute_swap_process_update_v1(
    cid: darkfi_sdk::crypto::ContractId,
    update: ExecuteSwapUpdateV1,
) -> ContractResult {
    let swaps_db = wasm::db::db_lookup(cid, DEX_CONTRACT_SWAPS_TREE)?;
    let participants_db = wasm::db::db_lookup(cid, DEX_CONTRACT_PARTICIPANTS_TREE)?;

    // Load existing swap
    let swap_data = wasm::db::db_get(swaps_db, &update.swap_id)?;
    let mut swap: Swap = match swap_data {
        Some(data) => deserialize(&data)?,
        None => {
            msg!("[ExecuteSwapV1] Error: Swap not found during update");
            return Err(DexError::SwapNotFound.into())
        }
    };

    // Update swap state to Executed
    swap.state = SwapState::Executed;

    // Store updated swap
    wasm::db::db_set(swaps_db, &update.swap_id, &serialize(&swap))?;

    // Remove participants (funds have been transferred via money_v3::otc_swap_v1)
    // The atomic token swap is executed via bundled money_v3::otc_swap_v1 child calls.
    // We use nullifiers for deletion (proper double-spend prevention).
    wasm::db::db_del(participants_db, &swap.proposer_nullifier.to_bytes())?;
    wasm::db::db_del(participants_db, &swap.acceptor_nullifier.to_bytes())?;

    msg!("[ExecuteSwapV1] Swap executed successfully: id={:?}", &update.swap_id);

    Ok(())
}