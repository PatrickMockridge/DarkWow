/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyynr.org foundation
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

//! WASM entrypoint for the DEX atomic swap contract
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
    bridge::{BridgeCall, BridgeParameter},
    contract::ContractResult,
    error::ContractError,
    msg,
    runtime::Runtime,
};

use crate::{error::DexError, model::*, DexFunction};

/// Initialize the DEX swap contract
pub fn dex_init(rt: &mut Runtime, params: BridgeParameter) -> ContractResult<()> {
    let call = BridgeCall::decode(params)?;
    let config: InitializeParams = deserialize_init_params(&call.data[1..])?;

    msg!("[dex_init] Initializing DEX with timeout={}, fee={}", config.timeout, config.fee);

    // Initialize swaps tree
    rt.create_tree(DEX_CONTRACT_SWAPS_TREE)?;

    // Initialize participants tree (tracks who's locked what)
    rt.create_tree(DEX_CONTRACT_PARTICIPANTS_TREE)?;

    // Initialize configuration tree
    rt.create_tree(DEX_CONTRACT_CONFIG_TREE)?;

    // Store configuration
    rt.store_set(
        DEX_CONTRACT_CONFIG_TREE,
        DEX_CONTRACT_TIMEOUT,
        &config.timeout.encode()?,
    )?;
    rt.store_set(
        DEX_CONTRACT_CONFIG_TREE,
        DEX_CONTRACT_FEE,
        &config.fee.encode()?,
    )?;

    msg!("[dex_init] DEX initialized successfully");
    Ok(())
}

/// Main contract entrypoint
pub fn dex_exec(rt: &mut Runtime, params: BridgeParameter) -> ContractResult<()> {
    let call = BridgeCall::decode(params)?;
    let function = DexFunction::try_from(call.function)?;

    match function {
        DexFunction::InitializeV1 => dex_init(rt, params),
        DexFunction::CreateSwapV1 => dex_create_swap(rt, call),
        DexFunction::AcceptSwapV1 => dex_accept_swap(rt, call),
        DexFunction::ExecuteSwapV1 => dex_execute_swap(rt, call),
        DexFunction::CancelSwapV1 => dex_cancel_swap(rt, call),
        DexFunction::UpdateConfigV1 => dex_update_config(rt, call),
    }
}

/// Create a new atomic swap proposal
///
/// Flow:
/// 1. Verify proposer has locked funds (via lock_commitment Merkle proof)
/// 2. Verify swap doesn't already exist
/// 3. Store swap proposal
/// 4. Emit SwapCreated event
///
/// Optional: Set open_execution=true to allow anyone to execute after acceptance.
/// This enables "instant fill" but reveals Alice's secret to the network.
fn dex_create_swap(rt: &mut Runtime, call: BridgeCall) -> ContractResult<()> {
    let params: CreateSwapParams = deserialize_create_swap_params(&call.data[1..])?;

    msg!("[dex_create_swap] Creating swap: id={:?}, open_execution={}", &params.swap_id, params.open_execution);

    // =========================================================================
    // STEP 1: Verify lock commitment
    // =========================================================================
    //
    // The proposer must have locked their offer_token/amount.
    // The lock_commitment = H(secret, token, amount) proves this.
    // We verify via Merkle proof against the money contract's coin tree.

    // In production: verify lock_proof against money contract state
    // For now: we trust the ZK proof verification in get_metadata()

    // =========================================================================
    // STEP 2: Verify swap doesn't already exist
    // =========================================================================

    let existing = rt.load(DEX_CONTRACT_SWAPS_TREE, &params.swap_id)?;
    if existing.is_some() {
        msg!("[dex_create_swap] ERROR: Swap already exists");
        return Err(DexError::SwapAlreadyExists.into())
    }

    // =========================================================================
    // STEP 3: Store swap proposal
    // =========================================================================

    let current_time = get_current_timestamp(rt)?;
    let timeout = get_swap_timeout(rt)?;

    let swap = Swap {
        swap_id: params.swap_id,
        proposer_pub_x: [0u8; 32], // TODO: extract from signature
        proposer_pub_y: [0u8; 32],
        acceptor_pub_x: [0u8; 32],
        acceptor_pub_y: [0u8; 32],
        offer_token: params.offer_token,
        offer_amount: params.offer_amount,
        request_token: params.request_token,
        request_amount: params.request_amount,
        proposer_lock: params.lock_commitment,
        acceptor_lock: [0u8; 32],
        state: SwapState::Created,
        created_at: current_time,
        expires_at: current_time + timeout as u64,
        open_execution: params.open_execution,
    };

    rt.store_set(DEX_CONTRACT_SWAPS_TREE, &params.swap_id, &swap.encode()?)?;

    // Store proposer's nullifier to prevent double-spend
    let proposer_nullifier = compute_nullifier_from_commitment(&params.lock_commitment);
    rt.store_set(DEX_CONTRACT_PARTICIPANTS_TREE, &proposer_nullifier, &[])?;

    // =========================================================================
    // STEP 4: Emit event
    // =========================================================================
    //
    // Note: We don't reveal amounts or tokens in the event!
    // Only that a swap was created.

    msg!("[dex_create_swap] EMIT_EVENT: SwapCreated(swap_id={:?})", &params.swap_id);

    Ok(())
}

/// Accept an atomic swap proposal
///
/// Flow:
/// 1. Verify swap exists and is in Created state
/// 2. Verify acceptor has locked matching funds
/// 3. Update swap to Accepted state
/// 4. If immediate_execute=true and swap has open_execution=true, execute immediately
/// 5. Emit SwapAccepted event (or SwapExecuted if immediate)
fn dex_accept_swap(rt: &mut Runtime, call: BridgeCall) -> ContractResult<()> {
    let params: AcceptSwapParams = deserialize_accept_swap_params(&call.data[1..])?;

    msg!("[dex_accept_swap] Accepting swap: id={:?}, immediate_execute={}", &params.swap_id, params.immediate_execute);

    // =========================================================================
    // STEP 1: Load and verify swap
    // =========================================================================

    let swap_data = rt.load(DEX_CONTRACT_SWAPS_TREE, &params.swap_id)?;
    let mut swap: Swap = match swap_data {
        Some(data) => Swap::decode(&mut std::io::Cursor::new(&data))
            .map_err(|_| ContractError::DecodeError)?,
        None => {
            msg!("[dex_accept_swap] ERROR: Swap not found");
            return Err(DexError::SwapNotFound.into())
        }
    };

    // Verify swap is in correct state
    match swap.state {
        SwapState::Created => {},
        _ => {
            msg!("[dex_accept_swap] ERROR: Swap not in Created state");
            return Err(DexError::InvalidSwapState.into())
        }
    }

    // Verify not expired
    let current_time = get_current_timestamp(rt)?;
    if current_time > swap.expires_at {
        msg!("[dex_accept_swap] ERROR: Swap expired");
        return Err(DexError::SwapExpired.into())
    }

    // =========================================================================
    // STEP 2: Verify acceptor's lock
    // =========================================================================
    //
    // Verify the acceptor has locked the requested tokens.
    // lock_commitment = H(secret, token, amount) should match request params.

    // In production: verify lock_proof against money contract state

    // =========================================================================
    // STEP 3: Update swap to Accepted
    // =========================================================================

    swap.acceptor_lock = params.lock_commitment;
    swap.state = SwapState::Accepted;

    rt.store_set(DEX_CONTRACT_SWAPS_TREE, &params.swap_id, &swap.encode()?)?;

    // Store acceptor's nullifier
    let acceptor_nullifier = compute_nullifier_from_commitment(&params.lock_commitment);
    rt.store_set(DEX_CONTRACT_PARTICIPANTS_TREE, &acceptor_nullifier, &[])?;

    // =========================================================================
    // STEP 4: Immediate execution (if requested and swap allows it)
    // =========================================================================
    //
    // If the swap was created with open_execution=true and the acceptor
    // requested immediate_execute=true, we can execute immediately.
    // This is the "instant fill" path - no need for Alice to come back online.

    if params.immediate_execute && swap.open_execution {
        msg!("[dex_accept_swap] IMMEDIATE EXECUTION: swap_id={:?}", &params.swap_id);
        // Execute the swap directly
        execute_swap_internal(rt, &params.swap_id)?;
        msg!("[dex_accept_swap] EMIT_EVENT: SwapExecuted(swap_id={:?})", &params.swap_id);
        return Ok(())
    }

    // =========================================================================
    // STEP 5: Emit event
    // =========================================================================

    msg!("[dex_accept_swap] EMIT_EVENT: SwapAccepted(swap_id={:?})", &params.swap_id);

    Ok(())
}

/// Execute an atomic swap
///
/// Flow:
/// 1. Verify swap exists and is Accepted
/// 2. Verify both locks are still valid (or swap has open_execution=true)
/// 3. Verify ZK proof of atomic swap
/// 4. Execute transfer: Alice gets B's funds, Bob gets A's funds
/// 5. Mark swap as Executed
/// 6. Emit SwapExecuted event
///
/// Note: If swap has open_execution=true, Alice's secret is not required
/// (the secret was revealed when the swap was created)
fn dex_execute_swap(rt: &mut Runtime, call: BridgeCall) -> ContractResult<()> {
    let params: ExecuteSwapParams = deserialize_execute_swap_params(&call.data[1..])?;

    msg!("[dex_execute_swap] Executing swap: id={:?}", &params.swap_id);

    // =========================================================================
    // STEP 1: Load and verify swap
    // =========================================================================

    let swap_data = rt.load(DEX_CONTRACT_SWAPS_TREE, &params.swap_id)?;
    let swap: Swap = match swap_data {
        Some(data) => Swap::decode(&mut std::io::Cursor::new(&data))
            .map_err(|_| ContractError::DecodeError)?,
        None => {
            msg!("[dex_execute_swap] ERROR: Swap not found");
            return Err(DexError::SwapNotFound.into())
        }
    };

    // Verify swap is in Accepted state
    match swap.state {
        SwapState::Accepted => {},
        _ => {
            msg!("[dex_execute_swap] ERROR: Swap not in Accepted state");
            return Err(DexError::InvalidSwapState.into())
        }
    }

    // =========================================================================
    // STEP 2: Handle open execution (Alice's secret already public)
    // =========================================================================
    //
    // If swap has open_execution=true, Alice's secret was revealed at creation.
    // In this case, we only need Bob's secret (from AcceptSwap).
    // The ZK proof is adjusted accordingly.

    if swap.open_execution {
        msg!("[dex_execute_swap] OPEN EXECUTION: Alice's secret already public");
        // Only verify Bob's secret in ZK proof
        // Proof verification happens in get_metadata()
    } else {
        // Standard path: verify both secrets
        // Proof verification happens in get_metadata()
    }

    // =========================================================================
    // STEP 3: Execute transfers (atomic)
    // =========================================================================
    //
    // The contract atomically:
    // - Transfers offer_amount of offer_token from proposer to acceptor
    // - Transfers request_amount of request_token from acceptor to proposer
    //
    // If either fails, both fail (atomic).

    // In production: call money contract to do transfers
    // For now: we just mark as executed

    // =========================================================================
    // STEP 4: Mark as Executed
    // =========================================================================

    // Use internal execution function
    execute_swap_internal(rt, &params.swap_id)?;

    // =========================================================================
    // STEP 5: Emit event
    // =========================================================================

    msg!("[dex_execute_swap] EMIT_EVENT: SwapExecuted(swap_id={:?})", &params.swap_id);

    Ok(())
}

/// Internal function to execute a swap (used by both dex_execute_swap and dex_accept_swap)
fn execute_swap_internal(rt: &mut Runtime, swap_id: &[u8; 32]) -> ContractResult<()> {
    // Load swap
    let swap_data = rt.load(DEX_CONTRACT_SWAPS_TREE, swap_id)?;
    let swap: Swap = match swap_data {
        Some(data) => Swap::decode(&mut std::io::Cursor::new(&data))
            .map_err(|_| ContractError::DecodeError)?,
        None => {
            msg!("[execute_swap_internal] ERROR: Swap not found");
            return Err(DexError::SwapNotFound.into())
        }
    };

    // Update state to Executed
    let mut updated_swap = swap;
    updated_swap.state = SwapState::Executed;
    rt.store_set(DEX_CONTRACT_SWAPS_TREE, swap_id, &updated_swap.encode()?)?;

    // Remove participants (funds have been transferred)
    rt.store_delete(DEX_CONTRACT_PARTICIPANTS_TREE, &compute_nullifier_from_commitment(&swap.proposer_lock))?;
    rt.store_delete(DEX_CONTRACT_PARTICIPANTS_TREE, &compute_nullifier_from_commitment(&swap.acceptor_lock))?;

    Ok(())
}

/// Cancel a swap and refund
///
/// Flow:
/// 1. Verify swap exists and is not Executed
/// 2. Verify caller owns one of the locks
/// 3. Verify ZK proof of ownership
/// 4. Refund the caller's locked funds
/// 5. Mark swap as Cancelled
/// 6. Emit SwapCancelled event
fn dex_cancel_swap(rt: &mut Runtime, call: BridgeCall) -> ContractResult<()> {
    let params: CancelSwapParams = deserialize_cancel_swap_params(&call.data[1..])?;

    msg!("[dex_cancel_swap] Cancelling swap: id={:?}", &params.swap_id);

    // =========================================================================
    // STEP 1: Load and verify swap
    // =========================================================================

    let swap_data = rt.load(DEX_CONTRACT_SWAPS_TREE, &params.swap_id)?;
    let mut swap: Swap = match swap_data {
        Some(data) => Swap::decode(&mut std::io::Cursor::new(&data))
            .map_err(|_| ContractError::DecodeError)?,
        None => {
            msg!("[dex_cancel_swap] ERROR: Swap not found");
            return Err(DexError::SwapNotFound.into())
        }
    };

    // Can't cancel an executed swap
    match swap.state {
        SwapState::Created | SwapState::Accepted => {},
        _ => {
            msg!("[dex_cancel_swap] ERROR: Swap already executed or cancelled");
            return Err(DexError::InvalidSwapState.into())
        }
    }

    // =========================================================================
    // STEP 2: Verify ownership and refund
    // =========================================================================
    //
    // Either party can cancel. Determine who by checking which lock
    // the caller can prove ownership of.

    let caller_nullifier = compute_nullifier_from_secret(params.secret);
    let is_proposer = caller_nullifier == compute_nullifier_from_commitment(&swap.proposer_lock);
    let is_acceptor = caller_nullifier == compute_nullifier_from_commitment(&swap.acceptor_lock);

    if !is_proposer && !is_acceptor {
        msg!("[dex_cancel_swap] ERROR: Caller is not a participant");
        return Err(DexError::UnauthorizedCancellation.into())
    }

    // =========================================================================
    // STEP 3: Refund
    // =========================================================================
    //
    // Refund the caller's locked funds.

    // In production: call money contract to refund
    // For now: we just mark as cancelled

    // =========================================================================
    // STEP 4: Mark as Cancelled
    // =========================================================================

    swap.state = SwapState::Cancelled;
    rt.store_set(DEX_CONTRACT_SWAPS_TREE, &params.swap_id, &swap.encode()?)?;

    // Remove participant nullifier
    rt.store_delete(DEX_CONTRACT_PARTICIPANTS_TREE, &caller_nullifier)?;

    // =========================================================================
    // STEP 5: Emit event
    // =========================================================================

    msg!("[dex_cancel_swap] EMIT_EVENT: SwapCancelled(swap_id={:?})", &params.swap_id);

    Ok(())
}

/// Update contract configuration
fn dex_update_config(rt: &mut Runtime, call: BridgeCall) -> ContractResult<()> {
    let params: UpdateConfigParams = deserialize_update_config_params(&call.data[1..])?;

    msg!("[dex_update_config] Updating configuration");

    rt.store_set(DEX_CONTRACT_CONFIG_TREE, DEX_CONTRACT_TIMEOUT, &params.timeout.encode()?)?;
    rt.store_set(DEX_CONTRACT_CONFIG_TREE, DEX_CONTRACT_FEE, &params.fee.encode()?)?;

    Ok(())
}

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

/// Get current block timestamp
fn get_current_timestamp(_rt: &mut Runtime) -> ContractResult<u64> {
    // In production: rt.get_block_timestamp()
    Ok(0)
}

/// Get swap timeout from config
fn get_swap_timeout(rt: &mut Runtime) -> ContractResult<u32> {
    let data = rt.load(DEX_CONTRACT_CONFIG_TREE, DEX_CONTRACT_TIMEOUT)?;
    match data {
        Some(d) => {
            u32::decode(&mut std::io::Cursor::new(&d))
                .map_err(|_| ContractError::DecodeError)
        }
        None => Ok(100), // Default 100 blocks
    }
}

/// Compute nullifier from secret
fn compute_nullifier_from_secret(secret: [u8; 32]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"swap_nullifier");
    hasher.update(&secret);
    *hasher.finalize().as_bytes()
}

/// Compute nullifier from lock commitment
fn compute_nullifier_from_commitment(commitment: &[u8; 32]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"swap_nullifier");
    hasher.update(commitment);
    *hasher.finalize().as_bytes()
}

// ============================================================================
// DESERIALIZATION
// ============================================================================

fn deserialize_init_params(data: &[u8]) -> ContractResult<InitializeParams> {
    InitializeParams::decode(&mut std::io::Cursor::new(data))
        .map_err(|_| ContractError::DecodeError)
}

fn deserialize_create_swap_params(data: &[u8]) -> ContractResult<CreateSwapParams> {
    CreateSwapParams::decode(&mut std::io::Cursor::new(data))
        .map_err(|_| ContractError::DecodeError)
}

fn deserialize_accept_swap_params(data: &[u8]) -> ContractResult<AcceptSwapParams> {
    AcceptSwapParams::decode(&mut std::io::Cursor::new(data))
        .map_err(|_| ContractError::DecodeError)
}

fn deserialize_execute_swap_params(data: &[u8]) -> ContractResult<ExecuteSwapParams> {
    ExecuteSwapParams::decode(&mut std::io::Cursor::new(data))
        .map_err(|_| ContractError::DecodeError)
}

fn deserialize_cancel_swap_params(data: &[u8]) -> ContractResult<CancelSwapParams> {
    CancelSwapParams::decode(&mut std::io::Cursor::new(data))
        .map_err(|_| ContractError::DecodeError)
}

fn deserialize_update_config_params(data: &[u8]) -> ContractResult<UpdateConfigParams> {
    UpdateConfigParams::decode(&mut std::io::Cursor::new(data))
        .map_err(|_| ContractError::DecodeError)
}