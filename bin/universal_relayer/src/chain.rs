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

//! Chain enumeration and executor trait

use async_trait::async_trait;
use darkfi_bridge_contract::chain_handler::{
    ChainHandler as BridgeChainHandler, ChainId, ExternalDeposit, HtlcDeposit, TxHash as BridgeTxHash,
    VerifiedWithdrawal, WithdrawalRequest,
};
use dwow_sdk::{error::ContractResult, pasta::pallas};
use super::error::{PendingWithdrawal, Result, TxHash};

/// Supported external chains for the bridge
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExternalChain {
    Ethereum = 0,
    Monero = 1,
    Zcash = 2,
    Aztec = 3,
    Litecoin = 4,
}

impl ExternalChain {
    /// Convert from u8 to ExternalChain
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Ethereum),
            1 => Some(Self::Monero),
            2 => Some(Self::Zcash),
            3 => Some(Self::Aztec),
            4 => Some(Self::Litecoin),
            _ => None,
        }
    }

    /// Convert to u8
    pub fn as_u8(&self) -> u8 {
        match self {
            Self::Ethereum => 0,
            Self::Monero => 1,
            Self::Zcash => 2,
            Self::Aztec => 3,
            Self::Litecoin => 4,
        }
    }

    /// Get chain name as string
    pub fn name(&self) -> &'static str {
        match self {
            Self::Ethereum => "Ethereum",
            Self::Monero => "Monero",
            Self::Zcash => "Zcash",
            Self::Aztec => "Aztec",
            Self::Litecoin => "Litecoin",
        }
    }
}

impl std::fmt::Display for ExternalChain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

/// Trait for chain-specific withdrawal executors
#[async_trait]
pub trait ChainExecutor: Send + Sync {
    /// Execute a withdrawal on the external chain
    async fn execute(&self, withdrawal: &PendingWithdrawal) -> Result<TxHash>;

    /// Get the chain this executor handles
    fn chain(&self) -> ExternalChain;

    /// Check if this executor is enabled
    fn is_enabled(&self) -> bool;

    /// Get the chain name for logging
    fn name(&self) -> &'static str {
        self.chain().name()
    }

    /// Estimate gas/fees for a withdrawal
    async fn estimate_fee(&self, withdrawal: &PendingWithdrawal) -> Result<u64>;

    /// Verify a transaction confirmation on the external chain
    async fn verify_confirmation(&self, tx_hash: &TxHash) -> Result<bool>;
}

/// Null struct for disabled chains
pub struct DisabledExecutor;

impl DisabledExecutor {
    pub fn new() -> Self {
        Self
    }
}

impl Default for DisabledExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ChainExecutor for DisabledExecutor {
    async fn execute(&self, _withdrawal: &PendingWithdrawal) -> Result<TxHash> {
        Err(crate::error::RelayerError::ChainExecution("Chain is disabled".to_string()))
    }

    fn chain(&self) -> ExternalChain {
        unreachable!("Disabled executor has no chain")
    }

    fn is_enabled(&self) -> bool {
        false
    }

    async fn estimate_fee(&self, _withdrawal: &PendingWithdrawal) -> Result<u64> {
        Err(crate::error::RelayerError::ChainExecution("Chain is disabled".to_string()))
    }

    async fn verify_confirmation(&self, _tx_hash: &TxHash) -> Result<bool> {
        Err(crate::error::RelayerError::ChainExecution("Chain is disabled".to_string()))
    }
}

// DisabledExecutor also implements BridgeChainHandler (returns errors)
#[async_trait]
impl BridgeChainHandler for DisabledExecutor {
    fn chain_id(&self) -> ChainId {
        unreachable!("Disabled executor has no chain")
    }

    fn is_enabled(&self) -> bool {
        false
    }

    async fn verify_deposit(&self, _deposit: &ExternalDeposit) -> ContractResult {
        Err(dwow_sdk::error::ContractError::Custom(2))
    }

    async fn verify_withdrawal(&self, _withdrawal: &WithdrawalRequest) -> ContractResult {
        Err(dwow_sdk::error::ContractError::Custom(2))
    }

    async fn execute(&self, _verified: &VerifiedWithdrawal) -> ContractResult {
        Err(dwow_sdk::error::ContractError::Custom(2))
    }

    async fn estimate_fee(&self, _withdrawal: &WithdrawalRequest) -> ContractResult {
        Err(dwow_sdk::error::ContractError::Custom(2))
    }

    async fn verify_confirmation(&self, _tx_hash: &BridgeTxHash) -> ContractResult {
        Err(dwow_sdk::error::ContractError::Custom(2))
    }

    async fn verify_htlc_deposit(&self, _htlc_deposit: &HtlcDeposit) -> ContractResult {
        Err(dwow_sdk::error::ContractError::Custom(2))
    }

    async fn execute_htlc_claim(
        &self,
        _swap_id: &[u8; 32],
        _secret: pallas::Base,
        _recipient: &[u8],
    ) -> ContractResult {
        Err(dwow_sdk::error::ContractError::Custom(2))
    }

    async fn execute_htlc_refund(
        &self,
        _swap_id: &[u8; 32],
        _sender: &[u8],
    ) -> ContractResult {
        Err(dwow_sdk::error::ContractError::Custom(2))
    }

    async fn get_htlc_status(&self, _swap_id: &[u8; 32]) -> ContractResult {
        Err(dwow_sdk::error::ContractError::Custom(2))
    }
}