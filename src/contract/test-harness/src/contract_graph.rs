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

//! Contract graph for selective VK/PK loading.
//!
//! This module defines which circuits belong to which contract,
//! and their dependencies. This enables isolated testing and
//! incremental VK generation.
//!
//! ## Architecture
//!
//! Instead of loading ALL 50+ circuits at once, we load only the
//! circuits needed for specific contracts and their dependencies.
//!
//! ## Example
//!
//! ```rust
//! use darkfi_contract_test_harness::contract_graph::{Contract, get_contracts};
//!
//! // Get only Money contract circuits
//! let money_circuits = get_contracts(&[Contract::Money]);
//!
//! // Get DAO + its dependencies (Money)
//! let dao_circuits = get_contracts(&[Contract::Dao]);
//!
//! // Get Roulette only (isolated, no dependencies)
//! let roulette_circuits = get_contracts(&[Contract::Roulette]);
//! ```

use std::collections::HashSet;

/// Supported contracts in the DarkFi ecosystem
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Contract {
    /// Money V1 (native) - handles DARK token
    Money,
    /// DAO (native) - governance contract
    Dao,
    /// Stablecoin WASM - USD-pegged token
    Stablecoin,
    /// Identity WASM - credentials and claims
    Identity,
    /// DEX WASM - decentralized exchange
    Dex,
    /// DAO-Escrow WASM - DAO with escrow
    DaoEscrow,
    /// Money V2 WASM - next version of Money
    MoneyV2,
    /// Native Token WASM - consensus-first native token
    NativeToken,
    /// Auction WASM - auction house
    Auction,
    /// Lottery WASM - lottery game
    Lottery,
    /// Slot WASM - slot machine game
    Slot,
    /// Baccarat WASM - baccarat game
    Baccarat,
    /// DarkToshi Dice WASM - dice game
    DarkToshiDice,
    /// Roulette WASM - roulette game
    Roulette,
    /// Deployooor (native) - deploys WASM contracts
    Deployooor,
    /// Atomic Swap - peer-to-peer token swap
    AtomicSwap,
    /// Attestation - claims and attestations
    Attestation,
    /// Betting Stake - betting platform
    BettingStake,
    /// Block Height Prediction - prediction market
    BlockHeightPrediction,
    /// Bridge - cross-chain bridge
    Bridge,
    /// Darkbet Exchange - betting exchange
    DarkbetExchange,
    /// Drain Protection - liquidity protection
    DrainProtection,
    /// Escrow - basic escrow contract
    Escrow,
    /// Game Room - general game room
    GameRoom,
    /// Insurance Market - insurance marketplace
    InsuranceMarket,
    /// Labor Market - labor marketplace
    LaborMarket,
    /// Oracle - price oracle
    Oracle,
    /// Pool Stake - staking pool
    PoolStake,
    /// Relayer Endowment - relayer funding
    RelayerEndowment,
    /// SafeMath - math utilities library
    SafeMath,
    /// Subscription - recurring payments
    Subscription,
    /// Tender - tender marketplace
    Tender,
}

impl Contract {
    /// Get all circuit namespaces for this contract
    pub fn circuits(&self) -> Vec<&'static str> {
        match self {
            Contract::Money => vec![
                "Fee_V1",
                "Mint_V1",
                "Burn_V1",
                "TokenMint_V1",
                "AuthTokenMint_V1",
            ],
            Contract::Dao => vec![
                "Mint",
                "ProposeInput",
                "ProposeMain",
                "VoteInput",
                "VoteMain",
                "Exec",
                "EarlyExec",
                "AuthTransfer",
                "AuthTransferEnc",
            ],
            Contract::Stablecoin => vec![
                "OpenPositionV1",
                "MintStableV1",
                "LiquidateV1",
                "AccrueInterestV1",
                "GovernanceReportV1",
            ],
            Contract::Identity => vec![
                "CreateClaimV1",
                "CreateClaimV1L1",
                "CreateClaimV1DAG",
                "CreateClaimV1L1V2",
                "CreateClaimV1Multi",
                "CreateClaimV1Ratio",
                "IssueCredentialV1",
                "VerifyCapabilityV1",
            ],
            Contract::Dex => vec![
                "CreateSwapV1",
                "AcceptSwapV1",
                "ExecuteSwapV1",
                "CancelSwapV1",
                "ExecuteSwapSlippageV1",
                "ExecuteSwapFeeV1",
            ],
            Contract::DaoEscrow => vec!["Init", "PayPremium"],
            Contract::MoneyV2 => vec![
                "Fee_V2",
                "Mint_V2",
                "Burn_V2",
                "TokenMint_V1",
                "AuthTokenMint_V1",
            ],
            Contract::NativeToken => vec![
                "Mint_V1",
                "Burn_V1",
                "Fee_V1",
            ],
            Contract::Auction => vec![
                "CreateAuction",
                "PlaceBid",
                "CloseAuction",
                "ClaimWinnings",
                "SettleAuction",
                "RefundBid",
            ],
            Contract::Lottery => vec!["CommitTicket_V1", "RevealTicket_V1"],
            Contract::Slot => vec!["CommitBet_V1", "SettleBet_V1"],
            Contract::Baccarat => vec!["CommitBet_V1", "SettleBet_V1"],
            Contract::DarkToshiDice => vec!["CommitBet_V1", "SettleBet_V1"],
            Contract::Roulette => vec!["PlaceBet_V1", "SettleBet_V1"],
            Contract::Deployooor => vec![],
            // Newer contracts - many have no circuit binaries yet
            Contract::AtomicSwap
            | Contract::Attestation
            | Contract::BettingStake
            | Contract::BlockHeightPrediction
            | Contract::Bridge
            | Contract::DarkbetExchange
            | Contract::DrainProtection
            | Contract::Escrow
            | Contract::GameRoom
            | Contract::InsuranceMarket
            | Contract::LaborMarket
            | Contract::Oracle
            | Contract::PoolStake
            | Contract::RelayerEndowment
            | Contract::SafeMath
            | Contract::Subscription
            | Contract::Tender => vec![],
        }
    }

    /// Human-readable name for logging
    pub fn name(&self) -> &'static str {
        match self {
            Contract::Money => "Money",
            Contract::Dao => "Dao",
            Contract::Stablecoin => "Stablecoin",
            Contract::Identity => "Identity",
            Contract::Dex => "Dex",
            Contract::DaoEscrow => "DaoEscrow",
            Contract::MoneyV2 => "MoneyV2",
            Contract::NativeToken => "NativeToken",
            Contract::Auction => "Auction",
            Contract::Lottery => "Lottery",
            Contract::Slot => "Slot",
            Contract::Baccarat => "Baccarat",
            Contract::DarkToshiDice => "DarkToshiDice",
            Contract::Roulette => "Roulette",
            Contract::Deployooor => "Deployooor",
            Contract::AtomicSwap => "AtomicSwap",
            Contract::Attestation => "Attestation",
            Contract::BettingStake => "BettingStake",
            Contract::BlockHeightPrediction => "BlockHeightPrediction",
            Contract::Bridge => "Bridge",
            Contract::DarkbetExchange => "DarkbetExchange",
            Contract::DrainProtection => "DrainProtection",
            Contract::Escrow => "Escrow",
            Contract::GameRoom => "GameRoom",
            Contract::InsuranceMarket => "InsuranceMarket",
            Contract::LaborMarket => "LaborMarket",
            Contract::Oracle => "Oracle",
            Contract::PoolStake => "PoolStake",
            Contract::RelayerEndowment => "RelayerEndowment",
            Contract::SafeMath => "SafeMath",
            Contract::Subscription => "Subscription",
            Contract::Tender => "Tender",
        }
    }

    /// Get direct dependencies of this contract
    pub fn dependencies(&self) -> Vec<Contract> {
        match self {
            // Native contracts have no dependencies on other contracts
            Contract::Money => vec![],
            Contract::Dao => vec![Contract::Money], // DAO uses Money for governance token
            Contract::Deployooor => vec![],
            // WASM contracts depend on Deployooor being deployed first
            // but for VK purposes, they're standalone
            Contract::Stablecoin
            | Contract::Identity
            | Contract::Dex
            | Contract::DaoEscrow
            | Contract::MoneyV2
            | Contract::NativeToken
            | Contract::Auction
            | Contract::Lottery
            | Contract::Slot
            | Contract::Baccarat
            | Contract::DarkToshiDice
            | Contract::Roulette
            | Contract::AtomicSwap
            | Contract::Attestation
            | Contract::BettingStake
            | Contract::BlockHeightPrediction
            | Contract::Bridge
            | Contract::DarkbetExchange
            | Contract::DrainProtection
            | Contract::Escrow
            | Contract::GameRoom
            | Contract::InsuranceMarket
            | Contract::LaborMarket
            | Contract::Oracle
            | Contract::PoolStake
            | Contract::RelayerEndowment
            | Contract::SafeMath
            | Contract::Subscription
            | Contract::Tender => vec![],
        }
    }

    /// Returns true if this is a native contract (deployed at genesis)
    pub fn is_native(&self) -> bool {
        matches!(
            self,
            Contract::Money | Contract::Dao | Contract::Deployooor
        )
    }
}

/// Resolve a list of contracts and their dependencies into topological order.
/// Each contract appears once, with dependencies appearing before dependents.
pub fn resolve_dependencies(contracts: &[Contract]) -> Vec<Contract> {
    let mut resolved: Vec<Contract> = vec![];
    let mut seen: HashSet<Contract> = HashSet::new();

    fn visit(
        contract: Contract,
        resolved: &mut Vec<Contract>,
        seen: &mut HashSet<Contract>,
        visiting: &mut HashSet<Contract>,
    ) {
        if seen.contains(&contract) {
            return;
        }
        if visiting.contains(&contract) {
            // Cycle detected - should not happen with proper dependency graph
            // For now, just skip and continue
            return;
        }
        visiting.insert(contract);

        for dep in contract.dependencies() {
            visit(dep, resolved, seen, visiting);
        }

        visiting.remove(&contract);
        seen.insert(contract);
        resolved.push(contract);
    }

    let mut visiting: HashSet<Contract> = HashSet::new();
    for contract in contracts {
        visit(*contract, &mut resolved, &mut seen, &mut visiting);
    }

    resolved
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_dependencies_money_alone() {
        let contracts = vec![Contract::Money];
        let resolved = resolve_dependencies(&contracts);
        assert_eq!(resolved, vec![Contract::Money]);
    }

    #[test]
    fn test_resolve_dependencies_dao_includes_money() {
        let contracts = vec![Contract::Dao];
        let resolved = resolve_dependencies(&contracts);
        // Money should appear before DAO
        assert!(resolved.contains(&Contract::Money));
        assert!(resolved.contains(&Contract::Dao));
        let money_idx = resolved.iter().position(|c| *c == Contract::Money).unwrap();
        let dao_idx = resolved.iter().position(|c| *c == Contract::Dao).unwrap();
        assert!(money_idx < dao_idx);
    }

    #[test]
    fn test_resolve_dependencies_roulette_isolated() {
        let contracts = vec![Contract::Roulette];
        let resolved = resolve_dependencies(&contracts);
        assert_eq!(resolved, vec![Contract::Roulette]);
    }

    #[test]
    fn test_resolve_dependencies_multiple() {
        let contracts = vec![Contract::Dao, Contract::Roulette];
        let resolved = resolve_dependencies(&contracts);
        assert!(resolved.contains(&Contract::Money));
        assert!(resolved.contains(&Contract::Dao));
        assert!(resolved.contains(&Contract::Roulette));
    }
}
