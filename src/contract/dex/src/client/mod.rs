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

//! Client-side transaction builders for DEX contract
//!
//! ## How to Place an Order
//!
//! ```ignore
//! // 1. Create order parameters
//! let order = PlaceOrderBuilder::new()
//!     .secret(secret)
//!     .amount(1000)
//!     .price(0_01_0000) // 0.01 ETH
//!     .token(token_drk)
//!     .side(OrderSide::Sell)
//!     .order_type(OrderType::GTC)
//!     .build()?;
//!
//! // 2. Submit to DEX contract
//! client.submit(order).await?;
//! ```
//!
//! ## How to Match Orders
//!
//! ```ignore
//! // 1. Solver finds compatible orders
//! let (order_a, order_b) = solver.find_match(buy_order, sell_order)?;
//!
//! // 2. Build match transaction
//! let match_tx = MatchOrdersBuilder::new()
//!     .order_a(order_a.commitment)
//!     .order_b(order_b.commitment)
//!     .match_amount(500) // partial fill
//!     .execution_price(0_01_1000) // slight premium
//!     .build()?;
//!
//! // 3. Submit match to DEX
//! client.submit(match_tx).await?;
//! ```

use darkfi_sdk::error::ClientError;

/// DEX client errors
#[derive(Debug, thiserror::Error)]
pub enum DexClientError {
    #[error("Invalid order: {0}")]
    InvalidOrder(String),

    #[error("Invalid match: {0}")]
    InvalidMatch(String),

    #[error("Insufficient funds")]
    InsufficientFunds,

    #[error("Price out of range")]
    PriceOutOfRange,

    #[error("Invalid ZK proof: {0}")]
    InvalidProof(String),
}

// ============================================================================
// PLACE ORDER BUILDER
// ============================================================================

/// Builder for placing an order
pub struct PlaceOrderBuilder {
    secret: Option<[u8; 32]>,
    amount: Option<u64>,
    price: Option<u64>,
    token: Option<[u8; 32]>,
    side: Option<OrderSide>,
    order_type: Option<OrderType>,
}

impl PlaceOrderBuilder {
    pub fn new() -> Self {
        Self {
            secret: None,
            amount: None,
            price: None,
            token: None,
            side: None,
            order_type: None,
        }
    }

    pub fn secret(&mut self, secret: [u8; 32]) -> &mut Self {
        self.secret = Some(secret);
        self
    }

    pub fn amount(&mut self, amount: u64) -> &mut Self {
        self.amount = Some(amount);
        self
    }

    pub fn price(&mut self, price: u64) -> &mut Self {
        self.price = Some(price);
        self
    }

    pub fn token(&mut self, token: [u8; 32]) -> &mut Self {
        self.token = Some(token);
        self
    }

    pub fn side(&mut self, side: OrderSide) -> &mut Self {
        self.side = Some(side);
        self
    }

    pub fn order_type(&mut self, order_type: OrderType) -> &mut Self {
        self.order_type = Some(order_type);
        self
    }

    /// Build the place order transaction
    ///
    /// commitment = H(secret, amount, price, token, side)
    pub fn build(&self) -> Result<Vec<u8>, ClientError> {
        let secret = self.secret.ok_or_else(|| ClientError::InvalidInput("secret required".into()))?;
        let amount = self.amount.ok_or_else(|| ClientError::InvalidInput("amount required".into()))?;
        let price = self.price.ok_or_else(|| ClientError::InvalidInput("price required".into()))?;
        let token = self.token.ok_or_else(|| ClientError::InvalidInput("token required".into()))?;
        let side = self.side.clone().ok_or_else(|| ClientError::InvalidInput("side required".into()))?;

        // Compute order commitment
        let commitment = compute_order_commitment(secret, amount, price, token, &side);

        // Generate ZK proof
        // In production: generate place_order.zk proof
        let proof = vec![0u8; 64];

        // TODO: Generate non-existence proof
        let non_existence_proof = vec![];

        // TODO: Generate signature
        let signature = vec![];

        // Encode call data
        let mut call_data = Vec::new();
        call_data.push(0x01); // PlaceOrderV1
        call_data.extend_from_slice(&commitment);
        call_data.extend_from_slice(&(non_existence_proof.len() as u32).to_le_bytes());
        for p in &non_existence_proof {
            call_data.extend_from_slice(p);
        }
        call_data.extend_from_slice(&(signature.len() as u32).to_le_bytes());
        call_data.extend_from_slice(&signature);
        call_data.extend_from_slice(&0u64.to_le_bytes()); // fee

        Ok(call_data)
    }
}

/// Compute order commitment
fn compute_order_commitment(
    secret: [u8; 32],
    amount: u64,
    price: u64,
    token: [u8; 32],
    side: &OrderSide,
) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"order_commitment");
    hasher.update(&secret);
    hasher.update(&amount.to_le_bytes());
    hasher.update(&price.to_le_bytes());
    hasher.update(&token);
    hasher.update(match side {
        OrderSide::Buy => b"buy",
        OrderSide::Sell => b"sell",
    });
    *hasher.finalize().as_bytes()
}

// ============================================================================
// CANCEL ORDER BUILDER
// ============================================================================

/// Builder for canceling an order
pub struct CancelOrderBuilder {
    secret: Option<[u8; 32]>,
}

impl CancelOrderBuilder {
    pub fn new() -> Self {
        Self { secret: None }
    }

    pub fn secret(&mut self, secret: [u8; 32]) -> &mut Self {
        self.secret = Some(secret);
        self
    }

    /// Build the cancel order transaction
    ///
    /// nullifier = H(secret)
    pub fn build(&self) -> Result<Vec<u8>, ClientError> {
        let secret = self.secret.ok_or_else(|| ClientError::InvalidInput("secret required".into()))?;

        let nullifier = compute_nullifier(secret);

        // TODO: Generate existence proof
        let existence_proof = vec![];

        // TODO: Generate signature
        let signature = vec![];

        let mut call_data = Vec::new();
        call_data.push(0x02); // CancelOrderV1
        call_data.extend_from_slice(&nullifier);
        call_data.extend_from_slice(&(existence_proof.len() as u32).to_le_bytes());
        for p in &existence_proof {
            call_data.extend_from_slice(p);
        }
        call_data.extend_from_slice(&(signature.len() as u32).to_le_bytes());
        call_data.extend_from_slice(&signature);
        call_data.extend_from_slice(&0u64.to_le_bytes()); // fee

        Ok(call_data)
    }
}

/// Compute nullifier from secret
fn compute_nullifier(secret: [u8; 32]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"order_nullifier");
    hasher.update(&secret);
    *hasher.finalize().as_bytes()
}

// ============================================================================
// MATCH ORDERS BUILDER
// ============================================================================

/// Builder for matching two orders
pub struct MatchOrdersBuilder {
    order_a_commitment: Option<[u8; 32]>,
    order_b_commitment: Option<[u8; 32]>,
    match_amount: Option<u64>,
    execution_price: Option<u64>,
}

impl MatchOrdersBuilder {
    pub fn new() -> Self {
        Self {
            order_a_commitment: None,
            order_b_commitment: None,
            match_amount: None,
            execution_price: None,
        }
    }

    pub fn order_a(&mut self, commitment: [u8; 32]) -> &mut Self {
        self.order_a_commitment = Some(commitment);
        self
    }

    pub fn order_b(&mut self, commitment: [u8; 32]) -> &mut Self {
        self.order_b_commitment = Some(commitment);
        self
    }

    pub fn match_amount(&mut self, amount: u64) -> &mut Self {
        self.match_amount = Some(amount);
        self
    }

    pub fn execution_price(&mut self, price: u64) -> &mut Self {
        self.execution_price = Some(price);
        self
    }

    /// Build the match orders transaction
    pub fn build(&self) -> Result<Vec<u8>, ClientError> {
        let order_a = self.order_a_commitment.ok_or_else(|| ClientError::InvalidInput("order_a required".into()))?;
        let order_b = self.order_b_commitment.ok_or_else(|| ClientError::InvalidInput("order_b required".into()))?;
        let amount = self.match_amount.ok_or_else(|| ClientError::InvalidInput("match_amount required".into()))?;
        let price = self.execution_price.ok_or_else(|| ClientError::InvalidInput("execution_price required".into()))?;

        // Generate ZK proof
        // In production: generate match_orders.zk proof
        let match_proof = vec![0u8; 64];

        // TODO: Generate SMT proofs
        let proof_a = vec![];
        let proof_b = vec![];

        let mut call_data = Vec::new();
        call_data.push(0x03); // MatchOrdersV1
        call_data.extend_from_slice(&order_a);
        call_data.extend_from_slice(&order_b);
        call_data.extend_from_slice(&amount.to_le_bytes());
        call_data.extend_from_slice(&price.to_le_bytes());
        call_data.extend_from_slice(&(proof_a.len() as u32).to_le_bytes());
        for p in &proof_a {
            call_data.extend_from_slice(p);
        }
        call_data.extend_from_slice(&(proof_b.len() as u32).to_le_bytes());
        for p in &proof_b {
            call_data.extend_from_slice(p);
        }
        call_data.extend_from_slice(&(match_proof.len() as u32).to_le_bytes());
        call_data.extend_from_slice(&match_proof);
        call_data.extend_from_slice(&0u64.to_le_bytes()); // fee

        Ok(call_data)
    }
}

// ============================================================================
// ADD LIQUIDITY BUILDER
// ============================================================================

/// Builder for adding liquidity
pub struct AddLiquidityBuilder {
    base_token: Option<[u8; 32]>,
    quote_token: Option<[u8; 32]>,
    base_amount: Option<u64>,
    quote_amount: Option<u64>,
    secret: Option<[u8; 32]>,
}

impl AddLiquidityBuilder {
    pub fn new() -> Self {
        Self {
            base_token: None,
            quote_token: None,
            base_amount: None,
            quote_amount: None,
            secret: None,
        }
    }

    pub fn pool(&mut self, base_token: [u8; 32], quote_token: [u8; 32]) -> &mut Self {
        self.base_token = Some(base_token);
        self.quote_token = Some(quote_token);
        self
    }

    pub fn amounts(&mut self, base: u64, quote: u64) -> &mut Self {
        self.base_amount = Some(base);
        self.quote_amount = Some(quote);
        self
    }

    pub fn secret(&mut self, secret: [u8; 32]) -> &mut Self {
        self.secret = Some(secret);
        self
    }

    /// Build the add liquidity transaction
    pub fn build(&self) -> Result<Vec<u8>, ClientError> {
        let base_token = self.base_token.ok_or_else(|| ClientError::InvalidInput("base_token required".into()))?;
        let quote_token = self.quote_token.ok_or_else(|| ClientError::InvalidInput("quote_token required".into()))?;
        let base_amount = self.base_amount.ok_or_else(|| ClientError::InvalidInput("base_amount required".into()))?;
        let quote_amount = self.quote_amount.ok_or_else(|| ClientError::InvalidInput("quote_amount required".into()))?;
        let secret = self.secret.ok_or_else(|| ClientError::InvalidInput("secret required".into()))?;

        let lp_commitment = compute_lp_commitment(secret, base_amount, quote_amount);

        // TODO: Generate ZK proof
        let proof = vec![0u8; 64];

        let mut call_data = Vec::new();
        call_data.push(0x04); // AddLiquidityV1
        call_data.extend_from_slice(&base_token);
        call_data.extend_from_slice(&quote_token);
        call_data.extend_from_slice(&base_amount.to_le_bytes());
        call_data.extend_from_slice(&quote_amount.to_le_bytes());
        call_data.extend_from_slice(&lp_commitment);
        call_data.extend_from_slice(&(proof.len() as u32).to_le_bytes());
        call_data.extend_from_slice(&proof);
        call_data.extend_from_slice(&0u64.to_le_bytes()); // fee

        Ok(call_data)
    }
}

/// Compute LP share commitment
fn compute_lp_commitment(secret: [u8; 32], base_amount: u64, quote_amount: u64) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"lp_commitment");
    hasher.update(&secret);
    hasher.update(&base_amount.to_le_bytes());
    hasher.update(&quote_amount.to_le_bytes());
    *hasher.finalize().as_bytes()
}

// ============================================================================
// REMOVE LIQUIDITY BUILDER
// ============================================================================

/// Builder for removing liquidity
pub struct RemoveLiquidityBuilder {
    base_token: Option<[u8; 32]>,
    quote_token: Option<[u8; 32]>,
    share_amount: Option<u64>,
    secret: Option<[u8; 32]>,
    recipient_secret: Option<[u8; 32]>,
}

impl RemoveLiquidityBuilder {
    pub fn new() -> Self {
        Self {
            base_token: None,
            quote_token: None,
            share_amount: None,
            secret: None,
            recipient_secret: None,
        }
    }

    pub fn pool(&mut self, base_token: [u8; 32], quote_token: [u8; 32]) -> &mut Self {
        self.base_token = Some(base_token);
        self.quote_token = Some(quote_token);
        self
    }

    pub fn share_amount(&mut self, amount: u64) -> &mut Self {
        self.share_amount = Some(amount);
        self
    }

    pub fn secret(&mut self, secret: [u8; 32]) -> &mut Self {
        self.secret = Some(secret);
        self
    }

    pub fn recipient(&mut self, secret: [u8; 32]) -> &mut Self {
        self.recipient_secret = Some(secret);
        self
    }

    /// Build the remove liquidity transaction
    pub fn build(&self) -> Result<Vec<u8>, ClientError> {
        let base_token = self.base_token.ok_or_else(|| ClientError::InvalidInput("base_token required".into()))?;
        let quote_token = self.quote_token.ok_or_else(|| ClientError::InvalidInput("quote_token required".into()))?;
        let share_amount = self.share_amount.ok_or_else(|| ClientError::InvalidInput("share_amount required".into()))?;
        let secret = self.secret.ok_or_else(|| ClientError::InvalidInput("secret required".into()))?;
        let recipient_secret = self.recipient_secret.ok_or_else(|| ClientError::InvalidInput("recipient_secret required".into()))?;

        let lp_nullifier = compute_nullifier(secret);
        let recipient_commitment = compute_recipient_commitment(recipient_secret);

        // TODO: Generate ZK proof
        let proof = vec![0u8; 64];

        let mut call_data = Vec::new();
        call_data.push(0x05); // RemoveLiquidityV1
        call_data.extend_from_slice(&base_token);
        call_data.extend_from_slice(&quote_token);
        call_data.extend_from_slice(&share_amount.to_le_bytes());
        call_data.extend_from_slice(&lp_nullifier);
        call_data.extend_from_slice(&recipient_commitment);
        call_data.extend_from_slice(&(proof.len() as u32).to_le_bytes());
        call_data.extend_from_slice(&proof);
        call_data.extend_from_slice(&0u64.to_le_bytes()); // fee

        Ok(call_data)
    }
}

/// Compute recipient commitment
fn compute_recipient_commitment(secret: [u8; 32]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"recipient_commitment");
    hasher.update(&secret);
    *hasher.finalize().as_bytes()
}