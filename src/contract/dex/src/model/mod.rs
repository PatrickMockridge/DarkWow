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

//! Data structures for DEX contract calls
//!
//! ## Order Model
//!
//! Orders are stored as commitments in a Sparse Merkle Tree:
//! - order_commitment = H(secret, amount, price, token, side)
//! - nullifier = H(secret) when order is spent
//!
//! ## Matching Logic
//!
//! Two orders match when:
//! - They are on opposite sides (buy vs sell)
//! - Buy price >= Sell price
//! - Amounts are compatible

use darkfi_serial::{SerialDecodable, SerialEncodable};

/// Token pair identifier
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct TokenPair {
    /// Base token (e.g., DRK)
    pub base_token: [u8; 32],
    /// Quote token (e.g., ETH)
    pub quote_token: [u8; 32],
}

/// Order side (buy or sell)
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub enum OrderSide {
    Buy,
    Sell,
}

/// Order type
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub enum OrderType {
    /// Immediate-or-cancel
    IOC,
    /// Good-till-cancel
    GTC,
    /// Fill-or-kill
    FOK,
}

/// Initialize DEX parameters
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct InitializeParams {
    /// DEX trading fee (basis points)
    pub fee: u64,

    /// Minimum order size
    pub min_order: u64,

    /// Maximum slippage tolerance (basis points)
    pub max_slippage: u64,
}

/// Place order parameters
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct PlaceOrderParams {
    /// Commitment to the order parameters
    /// commitment = H(secret, amount, price, token, side)
    pub order_commitment: [u8; 32],

    /// Merkle proof that commitment doesn't already exist
    pub non_existence_proof: Vec<[u8; 32]>,

    /// Order signature (proves ownership)
    pub signature: Vec<u8>,

    /// Fee paid for placing the order
    pub fee: u64,
}

/// Cancel order parameters
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CancelOrderParams {
    /// Nullifier = H(secret) - proves ownership
    pub nullifier: [u8; 32],

    /// Merkle proof that order exists in book
    pub existence_proof: Vec<[u8; 32]>,

    /// Signature authorizing cancellation
    pub signature: Vec<u8>,

    /// Fee paid for cancellation
    pub fee: u64,
}

/// Match orders parameters
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct MatchOrdersParams {
    /// Commitment for order A
    pub order_a_commitment: [u8; 32],

    /// Commitment for order B
    pub order_b_commitment: [u8; 32],

    /// Amount to match (may be partial)
    pub match_amount: u64,

    /// Execution price
    pub execution_price: u64,

    /// Merkle proofs for both orders
    pub proof_a: Vec<[u8; 32]>,
    pub proof_b: Vec<[u8; 32]>,

    /// ZK proof of valid match
    pub match_proof: Vec<u8>,

    /// Fee paid for the match
    pub fee: u64,
}

/// Add liquidity parameters
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct AddLiquidityParams {
    /// Pool identifier
    pub pool_id: TokenPair,

    /// Amount of base token to add
    pub base_amount: u64,

    /// Amount of quote token to add
    pub quote_amount: u64,

    /// LP share commitment
    pub lp_commitment: [u8; 32],

    /// ZK proof of liquidity addition
    pub proof: Vec<u8>,

    /// Fee paid
    pub fee: u64,
}

/// Remove liquidity parameters
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct RemoveLiquidityParams {
    /// Pool identifier
    pub pool_id: TokenPair,

    /// LP share nullifier
    pub lp_nullifier: [u8; 32],

    /// Amount of LP shares to burn
    pub share_amount: u64,

    /// Recipient commitment for withdrawal
    pub recipient_commitment: [u8; 32],

    /// ZK proof of valid withdrawal
    pub proof: Vec<u8>,

    /// Fee paid
    pub fee: u64,
}

/// Update configuration parameters
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct UpdateConfigParams {
    /// New trading fee
    pub fee: u64,

    /// New minimum order size
    pub min_order: u64,

    /// New maximum slippage
    pub max_slippage: u64,
}

/// Stored order record
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Order {
    /// Order commitment
    pub commitment: [u8; 32],

    /// Owner public key
    pub owner_pub_x: [u8; 32],
    pub owner_pub_y: [u8; 32],

    /// Token pair
    pub pair: TokenPair,

    /// Order side
    pub side: OrderSide,

    /// Order type
    pub order_type: OrderType,

    /// Order amount (hidden in commitment)
    pub amount: u64,

    /// Limit price (hidden in commitment)
    pub price: u64,

    /// Whether order has been filled/spent
    pub spent: bool,

    /// Creation timestamp
    pub created_at: u64,
}

/// Stored liquidity position
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct LiquidityPosition {
    /// LP commitment
    pub commitment: [u8; 32],

    /// Pool identifier
    pub pool_id: TokenPair,

    /// Owner public key
    pub owner_pub_x: [u8; 32],
    pub owner_pub_y: [u8; 32],

    /// LP share amount
    pub share_amount: u64,

    /// Base token amount in pool
    pub base_amount: u64,

    /// Quote token amount in pool
    pub quote_amount: u64,

    /// Whether position has been withdrawn
    pub withdrawn: bool,

    /// Creation timestamp
    pub created_at: u64,
}

/// Trade pair (for looking up pools)
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Pool {
    /// Pool identifier
    pub pair: TokenPair,

    /// Total base token liquidity
    pub total_base: u64,

    /// Total quote token liquidity
    pub total_quote: u64,

    /// Accumulated fees
    pub accumulated_fees: u64,

    /// Pool creation timestamp
    pub created_at: u64,
}

// ============================================================================
// ZK CIRCUIT SPECIFICATIONS
// ============================================================================
//
// PlaceOrder Circuit:
//   - Verifies order commitment = H(secret, amount, price, token, side)
//   - Verifies non-existence in order book
//   - Verifies signature from owner
//
// CancelOrder Circuit:
//   - Verifies nullifier = H(secret)
//   - Verifies order exists in book
//   - Verifies ownership via signature
//
// MatchOrders Circuit:
//   - Verifies both orders exist in SMT
//   - Verifies prices compatible (buy_price >= sell_price)
//   - Verifies amounts sufficient
//   - Verifies output notes correctly computed
//
// AddLiquidity Circuit:
//   - Verifies LP commitment well-formed
//   - Verifies token amounts match
//   - Updates pool totals
//
// RemoveLiquidity Circuit:
//   - Verifies LP nullifier not spent
//   - Verifies share amount valid
//   - Computes withdrawal amounts
//
// ============================================================================