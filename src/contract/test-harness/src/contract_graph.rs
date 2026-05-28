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
//! use dwow_contract_test_harness::contract_graph::{Contract, get_contracts};
//!
//! // Get only NativeToken contract circuits
//! let native_circuits = get_contracts(&[Contract::NativeToken]);
//!
//! // Get DAO-Escrow (no dependencies)
//! let dao_circuits = get_contracts(&[Contract::DaoEscrow]);
//!
//! // Get Roulette only (isolated, no dependencies)
//! let roulette_circuits = get_contracts(&[Contract::Roulette]);
//! ```

use std::collections::HashSet;

/// Supported contracts in the DarkWow ecosystem
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Contract {
    /// DAO-Escrow WASM - DAO with escrow
    DaoEscrow,
    /// Stablecoin WASM - USD-pegged token
    Stablecoin,
    /// Identity WASM - credentials and claims
    Identity,
    /// DEX WASM - decentralized exchange
    Dex,
    /// Money V3 WASM - privacy-first token (Poseidon-only, no EC)
    MoneyV3,
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
    /// Subscription - recurring payments
    Subscription,
    /// Tender - tender marketplace
    Tender,
}

impl Contract {
    /// Get all circuit namespaces for this contract
    pub fn circuits(&self) -> Vec<&'static str> {
        match self {
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
            Contract::MoneyV3 => vec![
                "TokenMint_V1",
                "AuthTokenMint_V1",
                "Mint_V1",
                "Burn_V1",
            ],
            Contract::NativeToken => vec![
                "Mint_V1",
                "Burn_V1",
                "Fee_V1",
            ],
            Contract::Auction => vec![
                "CreateAuctionV1",
                "PlaceBidV1",
                "CloseAuctionV1",
                "ClaimWinningsV1",
                "SettleAuctionV1",
                "RefundBidV1",
            ],
            Contract::Lottery => vec!["CommitTicket_V1", "RevealTicket_V1"],
            Contract::Slot => vec!["CommitBet_V1", "SettleBet_V1"],
            Contract::Baccarat => vec!["CommitBet_V1", "SettleBet_V1"],
            Contract::DarkToshiDice => vec!["CommitBet_V1", "SettleBet_V1"],
            Contract::Roulette => vec!["PlaceBet_V1", "SettleBet_V1"],
            Contract::Deployooor => vec![],
            Contract::Attestation => vec![
                "CreateAttestationV1",
                "CreateClaimV1",
                "VerifyClaimV1",
                "ConsumeClaimV1",
                "DelegateAttestationV1",
            ],
            Contract::Bridge => vec!["DepositV1"],
            Contract::Escrow => vec!["CreateEscrowV1", "ClaimEscrowV1", "RefundEscrowV1"],
            Contract::LaborMarket => vec![
                "CreateJobV1",
                "SubmitDeliverableV1",
                "SubmitGitDeliverableV1",
                "AcceptJobV1",
                "ConfirmDeliveryV1",
                "DisputeV1",
                "RefundV1",
            ],
            Contract::Oracle => vec!["RegisterOracleV1"],
            Contract::Subscription => vec!["SubscribeV1", "VerifyAccessV1", "RateLimitV1", "UpdateUsageV1"],
            Contract::Tender => vec!["CreateTenderV1", "SubmitBidV1", "RevealBidV1", "SelectWinnerV1"],
            Contract::BettingStake => vec!["Init", "Stake", "Unstake", "Claim", "UpdateRisk"],
            // Contracts with no circuit binaries yet
            Contract::BlockHeightPrediction
            | Contract::DarkbetExchange
            | Contract::DrainProtection
            | Contract::GameRoom
            | Contract::InsuranceMarket
            | Contract::PoolStake
            | Contract::RelayerEndowment => vec![],
        }
    }

    /// Human-readable name for logging
    pub fn name(&self) -> &'static str {
        match self {
            Contract::Stablecoin => "Stablecoin",
            Contract::Identity => "Identity",
            Contract::Dex => "Dex",
            Contract::DaoEscrow => "DaoEscrow",
            Contract::MoneyV3 => "MoneyV3",
            Contract::NativeToken => "NativeToken",
            Contract::Auction => "Auction",
            Contract::Lottery => "Lottery",
            Contract::Slot => "Slot",
            Contract::Baccarat => "Baccarat",
            Contract::DarkToshiDice => "DarkToshiDice",
            Contract::Roulette => "Roulette",
            Contract::Deployooor => "Deployooor",
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
            Contract::Subscription => "Subscription",
            Contract::Tender => "Tender",
        }
    }

    /// Get direct dependencies of this contract
    pub fn dependencies(&self) -> Vec<Contract> {
        match self {
            // Native contracts have no dependencies on other contracts
            Contract::Deployooor => vec![],
            // WASM contracts depend on Deployooor being deployed first
            // but for VK purposes, they're standalone
            Contract::Stablecoin
            | Contract::Identity
            | Contract::Dex
            | Contract::DaoEscrow
            | Contract::MoneyV3
            | Contract::NativeToken
            | Contract::Auction
            | Contract::Lottery
            | Contract::Slot
            | Contract::Baccarat
            | Contract::DarkToshiDice
            | Contract::Roulette
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
            | Contract::Subscription
            | Contract::Tender => vec![],
        }
    }

    /// Returns true if this is a native contract (deployed at genesis)
    pub fn is_native(&self) -> bool {
        matches!(self, Contract::Deployooor)
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
    fn test_resolve_dependencies_native_token_alone() {
        let contracts = vec![Contract::NativeToken];
        let resolved = resolve_dependencies(&contracts);
        assert_eq!(resolved, vec![Contract::NativeToken]);
    }

    #[test]
    fn test_resolve_dependencies_daoescrow_isolated() {
        let contracts = vec![Contract::DaoEscrow];
        let resolved = resolve_dependencies(&contracts);
        // DaoEscrow has no dependencies, should only contain itself
        assert_eq!(resolved, vec![Contract::DaoEscrow]);
    }

    #[test]
    fn test_resolve_dependencies_roulette_isolated() {
        let contracts = vec![Contract::Roulette];
        let resolved = resolve_dependencies(&contracts);
        assert_eq!(resolved, vec![Contract::Roulette]);
    }

    #[test]
    fn test_resolve_dependencies_multiple() {
        let contracts = vec![Contract::DaoEscrow, Contract::Roulette];
        let resolved = resolve_dependencies(&contracts);
        assert!(resolved.contains(&Contract::DaoEscrow));
        assert!(resolved.contains(&Contract::Roulette));
    }
}