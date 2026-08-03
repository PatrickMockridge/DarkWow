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
    /// Promissory Note WASM - privacy-first token (Poseidon-only, no EC)
    PromissoryNote,
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
                "OpenPositionV2",
                "MintStableV2",
                "LiquidateV2",
                "AccrueInterestV2",
                "GovernanceReportV2",
            ],
            Contract::Identity => vec![
                "CreateClaimV2",
                "CreateClaimV2L1",
                "CreateClaimV2DAG",
                "CreateClaimV2L1V2",
                "CreateClaimV2Multi",
                "CreateClaimV2Ratio",
                "IssueCredentialV2",
                "VerifyCapabilityV2",
            ],
            Contract::Dex => vec![
                "CreateSwapV2",
                "AcceptSwapV2",
                "ExecuteSwapV2",
                "CancelSwapV2",
                "ExecuteSwapSlippageV2",
                "ExecuteSwapFeeV2",
            ],
            Contract::DaoEscrow => vec!["InitV2", "PayPremiumV2"],
            Contract::PromissoryNote => vec![
                "TokenMint_V2",
                "Mint_V2",
                "Burn_V2",
            ],
            Contract::NativeToken => vec![
                "Mint_V2",
                "Burn_V2",
                "Fee_V2",
            ],
            Contract::Auction => vec![
                "CreateAuctionV2",
                "PlaceBidV2",
                "CloseAuctionV2",
                "ClaimWinningsV2",
                "SettleAuctionV2",
                "RefundBidV2",
            ],
            Contract::Lottery => vec!["CommitTicket_V2", "RevealTicket_V2"],
            Contract::Slot => vec!["CommitBet_V2", "SettleBet_V2"],
            Contract::Baccarat => vec!["CommitBet_V2", "SettleBet_V2"],
            Contract::DarkToshiDice => vec!["CommitBet_V2", "SettleBet_V2"],
            Contract::Roulette => vec!["PlaceBet_V2", "SettleBet_V2"],
            Contract::Deployooor => vec![],
            Contract::Attestation => vec![
                "CreateAttestationV2",
                "CreateClaimV2",
                "VerifyClaimV2",
                "ConsumeClaimV2",
                "DelegateAttestationV2",
            ],
            Contract::Bridge => vec!["DepositV2"],
            Contract::Escrow => vec!["CreateEscrowV2", "ClaimEscrowV2", "RefundEscrowV2"],
            Contract::LaborMarket => vec![
                "CreateJobV2",
                "SubmitDeliverableV2",
                "SubmitGitDeliverableV2",
                "AcceptJobV2",
                "ConfirmDeliveryV2",
                "DisputeV2",
                "RefundV2",
            ],
            Contract::Oracle => vec!["RegisterOracleV2"],
            Contract::Subscription => vec!["SubscribeV2", "VerifyAccessV2", "RateLimitV2", "UpdateUsageV2"],
            Contract::Tender => vec!["CreateTenderV2", "SubmitBidV2", "RevealBidV2", "SelectWinnerV2"],
            Contract::BettingStake => vec!["InitV2", "StakeV2", "UnstakeV2", "ClaimV2", "UpdateRiskV2"],
            // Contracts with no circuit binaries yet (circuits exist but not registered)
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
            Contract::PromissoryNote => "PromissoryNote",
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
            | Contract::PromissoryNote
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