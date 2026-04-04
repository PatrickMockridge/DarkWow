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

//! Universal Relayer Error Types

use thiserror::Error;

/// Result type for relayer operations
pub type Result<T> = std::result::Result<T, RelayerError>;

/// Universal Relayer errors
#[derive(Debug, Error)]
pub enum RelayerError {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("RPC error: {0}")]
    Rpc(String),

    #[error("Chain execution error: {0}")]
    ChainExecution(String),

    #[error("Transaction signing error: {0}")]
    Signing(String),

    #[error("Address derivation error: {0}")]
    AddressDerivation(String),

    #[error("Timeout waiting for confirmation")]
    Timeout,

    #[error("Withdrawal cancelled by user")]
    Cancelled,

    #[error("Insufficient balance for withdrawal")]
    InsufficientBalance,

    #[error("Invalid withdrawal data: {0}")]
    InvalidWithdrawalData(String),

    #[error("Ethereum error: {0}")]
    Ethereum(String),

    #[error("Monero error: {0}")]
    Monero(String),

    #[error("Zcash error: {0}")]
    Zcash(String),

    #[error("Litecoin error: {0}")]
    Litecoin(String),

    #[error("Aztec error: {0}")]
    Aztec(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON-RPC error: {0}")]
    JsonRpc(#[from] tinyjson::JsonParseError),

    #[error("Serde error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("TOML parse error: {0}")]
    Toml(#[from] toml::de::Error),
}

/// Pending withdrawal data from DarkFi bridge
#[derive(Debug, Clone)]
pub struct PendingWithdrawal {
    /// Unique withdrawal ID
    pub withdrawal_id: [u8; 32],
    /// Recipient address hash (derivation depends on chain)
    pub recipient_hash: [u8; 32],
    /// Amount to withdraw (in smallest unit)
    pub amount: u64,
    /// Target chain
    pub chain: u8,  // 0=ETH, 1=XMR, 2=ZEC, 3=AZT, 4=LTC
    /// Block height when withdrawal was requested
    pub request_height: u64,
    /// Block height when timeout occurs
    pub timeout_height: u64,
    /// Fee percentage for relayer
    pub relayer_fee: u64,
}

impl PendingWithdrawal {
    /// Check if withdrawal has timed out
    pub fn is_timed_out(&self, current_height: u64) -> bool {
        current_height >= self.timeout_height
    }

    /// Get the target chain as enum
    pub fn get_chain(&self) -> super::chain::ExternalChain {
        match self.chain {
            0 => super::chain::ExternalChain::Ethereum,
            1 => super::chain::ExternalChain::Monero,
            2 => super::chain::ExternalChain::Zcash,
            3 => super::chain::ExternalChain::Aztec,
            4 => super::chain::ExternalChain::Litecoin,
            _ => super::chain::ExternalChain::Ethereum,  // Default to ETH
        }
    }
}

/// Transaction hash type (generic, chain-specific implementation)
#[derive(Debug, Clone)]
pub struct TxHash {
    /// Chain identifier
    pub chain: u8,
    /// Raw transaction hash bytes
    pub hash: [u8; 32],
}

impl std::fmt::Display for TxHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", hex::encode(self.hash))
    }
}