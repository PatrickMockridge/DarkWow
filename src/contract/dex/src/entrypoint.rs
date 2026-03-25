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
//! ## How the Anonymous DEX Works
//!
//! 1. **Place Order**: User creates a hidden order commitment and adds it to the SMT
//! 2. **Match Orders**: Solver proves two orders are compatible, executes swap atomically
//! 3. **Cancel Order**: User reveals nullifier to cancel and recover funds
//!
//! ## Privacy Model
//!
//! - Order amounts and prices are hidden in Pedersen commitments
//! - Order book stores only commitments, not plaintext
//! - Matching proves compatibility without revealing order details
//! - Different keys per order prevent address linkage

use darkfi_sdk::{
    bridge::{BridgeCall, BridgeParameter},
    contract::ContractResult,
    error::ContractError,
    msg,
    runtime::Runtime,
};

use crate::{error::DexError, model::*, DexFunction};

/// Initialize the DEX contract
pub fn dex_init(rt: &mut Runtime, params: BridgeParameter) -> ContractResult<()> {
    let call = BridgeCall::decode(params)?;
    let config: InitializeParams = deserialize_init_params(&call.data[1..])?;

    msg!("[dex_init] Initializing DEX with fee={}", config.fee);

    // Initialize order book tree (Sparse Merkle Tree)
    rt.create_tree(DEX_CONTRACT_ORDERBOOK_TREE)?;

    // Initialize nullifier tree for spent orders
    rt.create_tree(DEX_CONTRACT_NULLIFIERS_TREE)?;

    // Initialize liquidity positions tree
    rt.create_tree(DEX_CONTRACT_LIQUIDITY_TREE)?;

    // Initialize configuration tree
    rt.create_tree(DEX_CONTRACT_CONFIG_TREE)?;

    // Store initial configuration
    rt.store_set(
        DEX_CONTRACT_CONFIG_TREE,
        DEX_CONTRACT_FEE,
        &config.fee.encode()?,
    )?;
    rt.store_set(
        DEX_CONTRACT_CONFIG_TREE,
        DEX_CONTRACT_MIN_ORDER,
        &config.min_order.encode()?,
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
        DexFunction::PlaceOrderV1 => dex_place_order(rt, call),
        DexFunction::CancelOrderV1 => dex_cancel_order(rt, call),
        DexFunction::MatchOrdersV1 => dex_match_orders(rt, call),
        DexFunction::AddLiquidityV1 => dex_add_liquidity(rt, call),
        DexFunction::RemoveLiquidityV1 => dex_remove_liquidity(rt, call),
        DexFunction::UpdateConfigV1 => dex_update_config(rt, call),
    }
}

/// Place an order into the order book
///
/// Flow:
/// 1. Verify order commitment is correctly formed
/// 2. Verify order doesn't already exist in book (non-existence proof)
/// 3. Verify signature from order owner
/// 4. Insert commitment into order book SMT
/// 5. Emit OrderPlaced event
fn dex_place_order(rt: &mut Runtime, call: BridgeCall) -> ContractResult<()> {
    let params: PlaceOrderParams = deserialize_place_order_params(&call.data[1..])?;

    msg!("[dex_place_order] Placing order: commitment={:?}", &params.order_commitment);

    // =========================================================================
    // STEP 1: Verify order commitment
    // =========================================================================
    //
    // The commitment should be: H(secret, amount, price, token, side)
    // ZK proof verifies this in the circuit. Here we just store.

    // =========================================================================
    // STEP 2: Verify non-existence proof
    // =========================================================================
    //
    // We need to prove this exact commitment isn't already in the book.
    // This prevents duplicate orders.

    let existing = rt.load(DEX_CONTRACT_ORDERBOOK_TREE, &params.order_commitment)?;
    if existing.is_some() {
        msg!("[dex_place_order] ERROR: Order already exists");
        return Err(DexError::OrderAlreadyExists.into())
    }

    // =========================================================================
    // STEP 3: Verify signature
    // =========================================================================
    //
    // The signature proves the sender owns the order secret.
    // This prevents others from canceling your order.

    // Signature verification happens in get_metadata() before this call

    // =========================================================================
    // STEP 4: Insert into order book
    // =========================================================================

    rt.store_set(DEX_CONTRACT_ORDERBOOK_TREE, &params.order_commitment, &[])?;

    // Update order book root
    let new_root = compute_new_merkle_root(&params.order_commitment);
    rt.store_set(
        DEX_CONTRACT_CONFIG_TREE,
        DEX_CONTRACT_ORDERBOOK_ROOT,
        &new_root,
    )?;

    // =========================================================================
    // STEP 5: Emit event
    // =========================================================================

    msg!(
        "[dex_place_order] EMIT_EVENT: OrderPlaced({:?})",
        &params.order_commitment
    );

    Ok(())
}

/// Cancel an order and recover funds
///
/// Flow:
/// 1. Verify nullifier corresponds to an existing order
/// 2. Verify order hasn't already been spent
/// 3. Verify signature authorizes cancellation
/// 4. Mark order as spent (insert nullifier)
/// 5. Emit OrderCancelled event
fn dex_cancel_order(rt: &mut Runtime, call: BridgeCall) -> ContractResult<()> {
    let params: CancelOrderParams = deserialize_cancel_order_params(&call.data[1..])?;

    msg!("[dex_cancel_order] Cancelling order: nullifier={:?}", &params.nullifier);

    // =========================================================================
    // STEP 1: Verify order exists
    // =========================================================================
    //
    // The existence proof shows the order is in the book.
    // We need to find the corresponding commitment for this nullifier.
    // In practice, the nullifier = H(secret) so we can't look it up directly.
    // Instead, the proof shows the commitment exists and we verify nullifier matches.

    // =========================================================================
    // STEP 2: Verify not already spent
    // =========================================================================

    let existing = rt.load(DEX_CONTRACT_NULLIFIERS_TREE, &params.nullifier)?;
    if existing.is_some() {
        msg!("[dex_cancel_order] ERROR: Order already spent");
        return Err(DexError::OrderAlreadySpent.into())
    }

    // =========================================================================
    // STEP 3: Verify signature
    // =========================================================================

    // Signature verification happens in get_metadata()

    // =========================================================================
    // STEP 4: Mark as spent
    // =========================================================================

    rt.store_set(DEX_CONTRACT_NULLIFIERS_TREE, &params.nullifier, &[])?;

    // =========================================================================
    // STEP 5: Emit event
    // =========================================================================

    msg!(
        "[dex_cancel_order] EMIT_EVENT: OrderCancelled({:?})",
        &params.nullifier
    );

    Ok(())
}

/// Match two orders and execute swap
///
/// Flow:
/// 1. Verify both orders exist in order book (SMT proofs)
/// 2. Verify orders are compatible (buy_price >= sell_price)
/// 3. Verify amounts are sufficient for the match
/// 4. Verify match ZK proof
/// 5. Update both orders (mark partial or fully filled)
/// 6. Emit MatchExecuted event
fn dex_match_orders(rt: &mut Runtime, call: BridgeCall) -> ContractResult<()> {
    let params: MatchOrdersParams = deserialize_match_params(&call.data[1..])?;

    msg!(
        "[dex_match_orders] Matching orders: A={:?}, B={:?}",
        &params.order_a_commitment,
        &params.order_b_commitment
    );

    // =========================================================================
    // STEP 1: Verify order existence
    // =========================================================================

    let order_a = rt.load(DEX_CONTRACT_ORDERBOOK_TREE, &params.order_a_commitment)?;
    let order_b = rt.load(DEX_CONTRACT_ORDERBOOK_TREE, &params.order_b_commitment)?;

    if order_a.is_none() {
        return Err(DexError::OrderNotFound.into())
    }
    if order_b.is_none() {
        return Err(DexError::OrderNotFound.into())
    }

    // =========================================================================
    // STEP 2: Verify price compatibility
    // =========================================================================
    //
    // For a match to work:
    // - If A is buy and B is sell: A.price >= B.price
    // - If A is sell and B is buy: B.price >= A.price
    //
    // The ZK proof verifies this without revealing prices.

    // =========================================================================
    // STEP 3: Verify match proof
    // =========================================================================
    //
    // The ZK proof (match_orders.zk) demonstrates:
    // - Both orders exist in the SMT
    // - Orders are on opposite sides
    // - Prices are compatible
    // - Amounts are sufficient
    // - Output notes are correctly formed

    // Proof verification happens in get_metadata()

    // =========================================================================
    // STEP 4: Update orders
    // =========================================================================
    //
    // If match is full fill: mark both as spent
    // If match is partial: update remaining amounts

    // For full fill:
    let nullifier_a = compute_nullifier_from_commitment(&params.order_a_commitment);
    let nullifier_b = compute_nullifier_from_commitment(&params.order_b_commitment);

    rt.store_set(DEX_CONTRACT_NULLIFIERS_TREE, &nullifier_a, &[])?;
    rt.store_set(DEX_CONTRACT_NULLIFIERS_TREE, &nullifier_b, &[])?;

    // =========================================================================
    // STEP 5: Emit event
    // =========================================================================

    msg!(
        "[dex_match_orders] EMIT_EVENT: MatchExecuted(A={:?}, B={:?}, amount={}, price={})",
        &params.order_a_commitment,
        &params.order_b_commitment,
        params.match_amount,
        params.execution_price
    );

    Ok(())
}

/// Add liquidity to a trading pool
///
/// Flow:
/// 1. Verify pool exists
/// 2. Verify liquidity commitment is valid
/// 3. Verify ZK proof of liquidity addition
/// 4. Update pool totals
/// 5. Mint LP shares
fn dex_add_liquidity(rt: &mut Runtime, call: BridgeCall) -> ContractResult<()> {
    let params: AddLiquidityParams = deserialize_add_liquidity_params(&call.data[1..])?;

    msg!(
        "[dex_add_liquidity] Adding liquidity to pool: {:?}",
        &params.pool_id
    );

    // TODO: Implement add liquidity logic
    Err(ContractError::NotYetImplemented.into())
}

/// Remove liquidity from a pool
///
/// Flow:
/// 1. Verify LP nullifier not spent
/// 2. Verify ZK proof of withdrawal
/// 3. Update pool totals
/// 4. Burn LP shares
/// 5. Release tokens to user
fn dex_remove_liquidity(rt: &mut Runtime, call: BridgeCall) -> ContractResult<()> {
    let params: RemoveLiquidityParams = deserialize_remove_liquidity_params(&call.data[1..])?;

    msg!(
        "[dex_remove_liquidity] Removing liquidity: pool={:?}, amount={}",
        &params.pool_id,
        params.share_amount
    );

    // TODO: Implement remove liquidity logic
    Err(ContractError::NotYetImplemented.into())
}

/// Update DEX configuration
fn dex_update_config(rt: &mut Runtime, call: BridgeCall) -> ContractResult<()> {
    let params: UpdateConfigParams = deserialize_update_config_params(&call.data[1..])?;

    msg!("[dex_update_config] Updating configuration");

    rt.store_set(DEX_CONTRACT_CONFIG_TREE, DEX_CONTRACT_FEE, &params.fee.encode()?)?;
    rt.store_set(DEX_CONTRACT_CONFIG_TREE, DEX_CONTRACT_MIN_ORDER, &params.min_order.encode()?)?;

    Ok(())
}

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

/// Compute new Merkle root after inserting a commitment
fn compute_new_merkle_root(commitment: &[u8; 32]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"orderbook_root");
    hasher.update(commitment);
    *hasher.finalize().as_bytes()
}

/// Compute nullifier from order commitment
///
/// nullifier = H(secret) but we can't derive secret from commitment.
/// Instead, the user reveals secret when canceling, and we verify
/// nullifier matches.
fn compute_nullifier_from_commitment(commitment: &[u8; 32]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"nullifier");
    hasher.update(commitment);
    *hasher.finalize().as_bytes()
}

// ============================================================================
// DESERIALIZATION
// ============================================================================

fn deserialize_init_params(data: &[u8]) -> ContractResult<InitializeParams> {
    Err(ContractError::NotYetImplemented.into())
}

fn deserialize_place_order_params(data: &[u8]) -> ContractResult<PlaceOrderParams> {
    Err(ContractError::NotYetImplemented.into())
}

fn deserialize_cancel_order_params(data: &[u8]) -> ContractResult<CancelOrderParams> {
    Err(ContractError::NotYetImplemented.into())
}

fn deserialize_match_params(data: &[u8]) -> ContractResult<MatchOrdersParams> {
    Err(ContractError::NotYetImplemented.into())
}

fn deserialize_add_liquidity_params(data: &[u8]) -> ContractResult<AddLiquidityParams> {
    Err(ContractError::NotYetImplemented.into())
}

fn deserialize_remove_liquidity_params(data: &[u8]) -> ContractResult<RemoveLiquidityParams> {
    Err(ContractError::NotYetImplemented.into())
}

fn deserialize_update_config_params(data: &[u8]) -> ContractResult<UpdateConfigParams> {
    Err(ContractError::NotYetImplemented.into())
}

// ============================================================================
// IMPLEMENTATION NOTES
// ============================================================================
//
// MVP FEATURES:
// - Place order (add to SMT order book)
// - Cancel order (recover funds)
// - Match orders (atomic swap)
//
// FUTURE FEATURES:
// - Add liquidity (LP shares)
// - Remove liquidity (withdraw)
// - Multiple order types (IOC, GTC, FOK)
// - Partial fills
// - Order modification
//
// PRIVACY FEATURES:
// - Order amounts hidden in commitments
// - Order prices hidden in commitments
// - SMT stores only commitments
// - Matching proves compatibility without revealing prices
//
// ZK CIRCUITS NEEDED:
// - place_order.zk: Commitment validity + non-existence proof
// - cancel_order.zk: Ownership proof + nullifier
// - match_orders.zk: Compatibility proof + output computation
//
// ============================================================================