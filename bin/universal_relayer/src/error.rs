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

#![allow(dead_code)]

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

    // Stake = Coverage errors
    #[error("Insufficient stake for withdrawal: have {available}, need {required}")]
    InsufficientStake { available: u64, required: u64 },

    #[error("Stake locked for in-flight withdrawal")]
    StakeLocked,

    #[error("Stake claim failed verification")]
    StakeClaimFailed,

    // Feed market errors
    #[error("Feed mode not supported")]
    UnsupportedFeedMode,

    #[error("Guarantee premium not paid")]
    GuaranteePremiumNotPaid,

    // Pool errors
    #[error("Pool membership error: {0}")]
    PoolError(String),

    #[error("Pool full, cannot accept more coverage")]
    PoolFull,

    // Capital deployer errors
    #[error("Capital deployer error: {0}")]
    DeployerError(String),

    // Betting errors
    #[error("Betting market error: {0}")]
    BettingError(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON-RPC error: {0}")]
    JsonRpc(#[from] tinyjson::JsonParseError),

    #[error("Serde error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("TOML parse error: {0}")]
    Toml(#[from] toml::de::Error),
}

/// Pending withdrawal data from DarkWow bridge
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
    /// Feed mode: 0=Standard, 1=Guaranteed
    pub feed_mode: u8,
    /// Guarantee premium paid (for Guaranteed mode)
    pub guarantee_premium: u64,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chain::ExternalChain;

    fn test_withdrawal(timeout: u64, chain: u8) -> PendingWithdrawal {
        PendingWithdrawal {
            withdrawal_id: [1u8; 32],
            recipient_hash: [2u8; 32],
            amount: 100,
            chain,
            request_height: 10,
            timeout_height: timeout,
            relayer_fee: 5,
            feed_mode: 0,
            guarantee_premium: 0,
        }
    }

    #[test]
    fn test_pending_withdrawal_is_timed_out() {
        // timeout_height = 50, current = 50 => timed out
        assert!(test_withdrawal(50, 0).is_timed_out(50));
        // timeout_height = 50, current = 100 => timed out
        assert!(test_withdrawal(50, 0).is_timed_out(100));
    }

    #[test]
    fn test_pending_withdrawal_is_not_timed_out() {
        // timeout_height = 50, current = 49 => not timed out
        assert!(!test_withdrawal(50, 0).is_timed_out(49));
        // timeout_height = 50, current = 0 => not timed out
        assert!(!test_withdrawal(50, 0).is_timed_out(0));
    }

    #[test]
    fn test_pending_withdrawal_get_chain_all() {
        assert_eq!(test_withdrawal(100, 0).get_chain(), ExternalChain::Ethereum);
        assert_eq!(test_withdrawal(100, 1).get_chain(), ExternalChain::Monero);
        assert_eq!(test_withdrawal(100, 2).get_chain(), ExternalChain::Zcash);
        assert_eq!(test_withdrawal(100, 3).get_chain(), ExternalChain::Aztec);
        assert_eq!(test_withdrawal(100, 4).get_chain(), ExternalChain::Litecoin);
    }

    #[test]
    fn test_pending_withdrawal_get_chain_unknown_defaults_to_eth() {
        assert_eq!(test_withdrawal(100, 99).get_chain(), ExternalChain::Ethereum);
    }

    #[test]
    fn test_tx_hash_display() {
        let mut hash = [0u8; 32];
        hash[0] = 0xAB;
        hash[1] = 0xCD;
        hash[31] = 0xEF;
        let tx = TxHash { chain: 0, hash };
        let display = format!("{}", tx);
        assert!(display.starts_with("abcd"));
        assert!(display.ends_with("ef"));
        assert_eq!(display.len(), 64);
    }

    #[test]
    fn test_relayer_error_display() {
        let err = RelayerError::InsufficientStake { available: 100, required: 200 };
        let msg = format!("{}", err);
        assert!(msg.contains("100"));
        assert!(msg.contains("200"));

        let err2 = RelayerError::Config("bad config".to_string());
        assert!(format!("{}", err2).contains("bad config"));
    }
}