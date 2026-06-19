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

//! Contract Import Graph for drk Wallet
//!
//! Architecture:
//! - `money` module: Promissory Note for DeFi tokens (ERC-20 style)
//! - `native_token` module: DRKW token for fees and native operations
//! - `dao_escrow` module: DAO with treasury and endowment management
//!
//! ## Contract Registry
//!
//! The wallet uses a [`Contract`] trait-based registry for dependency resolution.
//! See [`crate::contract_registry`] for the generic registry system.

use dwow_sdk::pasta::pallas;
use dwow_sdk::crypto::ContractId;

// ============================================================================
// CONTRACT IDs
// ============================================================================

pub use dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID;

// Promissory Note Contract ID — genesis-deployed, hardcoded.
// Universal DeFi dependency (bridges, stablecoins, DEXes, escrows, bearer bonds).
// Plays ZERO role in chain consensus. Not consensus-critical.
pub use dwow_sdk::crypto::{DEPLOYOOOR_CONTRACT_ID, PROMISSORY_NOTE_CONTRACT_ID};

// DAO-Escrow Contract ID - user deployed, no hardcoded ID
pub static DAO_ESCROW_CONTRACT_ID: std::sync::OnceLock<dwow_sdk::crypto::ContractId> =
    std::sync::OnceLock::new();

// DrainProtection Contract ID - user deployed, no hardcoded ID
pub static DRAIN_PROTECTION_CONTRACT_ID: std::sync::OnceLock<dwow_sdk::crypto::ContractId> =
    std::sync::OnceLock::new();

// DEX Contract ID - user deployed, no hardcoded ID
pub static DEX_CONTRACT_ID: std::sync::OnceLock<dwow_sdk::crypto::ContractId> =
    std::sync::OnceLock::new();

// Auction Contract ID - user deployed, no hardcoded ID
pub static AUCTION_CONTRACT_ID: std::sync::OnceLock<dwow_sdk::crypto::ContractId> =
    std::sync::OnceLock::new();

// Stablecoin Contract ID - user deployed, no hardcoded ID
pub static STABLECOIN_CONTRACT_ID: std::sync::OnceLock<dwow_sdk::crypto::ContractId> =
    std::sync::OnceLock::new();

// Attestation Contract ID
pub static ATTESTATION_CONTRACT_ID: std::sync::OnceLock<dwow_sdk::crypto::ContractId> =
    std::sync::OnceLock::new();

// Baccarat Contract ID
pub static BACCARAT_CONTRACT_ID: std::sync::OnceLock<dwow_sdk::crypto::ContractId> =
    std::sync::OnceLock::new();

// Bearer Bond Contract ID
pub static BEARER_BOND_CONTRACT_ID: std::sync::OnceLock<dwow_sdk::crypto::ContractId> =
    std::sync::OnceLock::new();

// BettingStake Contract ID
pub static BETTING_STAKE_CONTRACT_ID: std::sync::OnceLock<dwow_sdk::crypto::ContractId> =
    std::sync::OnceLock::new();

// Bridge Contract ID
pub static BRIDGE_CONTRACT_ID: std::sync::OnceLock<dwow_sdk::crypto::ContractId> =
    std::sync::OnceLock::new();

// DarkbetExchange Contract ID
pub static DARKBET_EXCHANGE_CONTRACT_ID: std::sync::OnceLock<dwow_sdk::crypto::ContractId> =
    std::sync::OnceLock::new();

// DarktoshiDice Contract ID
pub static DARKTOSHI_DICE_CONTRACT_ID: std::sync::OnceLock<dwow_sdk::crypto::ContractId> =
    std::sync::OnceLock::new();

// Escrow Contract ID
pub static ESCROW_CONTRACT_ID: std::sync::OnceLock<dwow_sdk::crypto::ContractId> =
    std::sync::OnceLock::new();

// GameRoom Contract ID
pub static GAME_ROOM_CONTRACT_ID: std::sync::OnceLock<dwow_sdk::crypto::ContractId> =
    std::sync::OnceLock::new();

// Identity Contract ID
pub static IDENTITY_CONTRACT_ID: std::sync::OnceLock<dwow_sdk::crypto::ContractId> =
    std::sync::OnceLock::new();

// InsuranceMarket Contract ID
pub static INSURANCE_MARKET_CONTRACT_ID: std::sync::OnceLock<dwow_sdk::crypto::ContractId> =
    std::sync::OnceLock::new();

// LaborMarket Contract ID
pub static LABOR_MARKET_CONTRACT_ID: std::sync::OnceLock<dwow_sdk::crypto::ContractId> =
    std::sync::OnceLock::new();

// Lottery Contract ID
pub static LOTTERY_CONTRACT_ID: std::sync::OnceLock<dwow_sdk::crypto::ContractId> =
    std::sync::OnceLock::new();

// Oracle Contract ID
pub static ORACLE_CONTRACT_ID: std::sync::OnceLock<dwow_sdk::crypto::ContractId> =
    std::sync::OnceLock::new();

// OTC Swap Contract ID
pub static OTC_SWAP_CONTRACT_ID: std::sync::OnceLock<dwow_sdk::crypto::ContractId> =
    std::sync::OnceLock::new();

// PoolStake Contract ID
pub static POOL_STAKE_CONTRACT_ID: std::sync::OnceLock<dwow_sdk::crypto::ContractId> =
    std::sync::OnceLock::new();

// RelayerEndowment Contract ID
pub static RELAYER_ENDOWMENT_CONTRACT_ID: std::sync::OnceLock<dwow_sdk::crypto::ContractId> =
    std::sync::OnceLock::new();

// Roulette Contract ID
pub static ROULETTE_CONTRACT_ID: std::sync::OnceLock<dwow_sdk::crypto::ContractId> =
    std::sync::OnceLock::new();

// Slot Contract ID
pub static SLOT_CONTRACT_ID: std::sync::OnceLock<dwow_sdk::crypto::ContractId> =
    std::sync::OnceLock::new();

// Subscription Contract ID
pub static SUBSCRIPTION_CONTRACT_ID: std::sync::OnceLock<dwow_sdk::crypto::ContractId> =
    std::sync::OnceLock::new();

// Tender Contract ID
pub static TENDER_CONTRACT_ID: std::sync::OnceLock<dwow_sdk::crypto::ContractId> =
    std::sync::OnceLock::new();

/// Register a contract ID at runtime. Called after deploying a contract
/// so that subsequent operations (transfer, invoke) can find it.
pub fn register_contract_id(name: &str, cid: dwow_sdk::crypto::ContractId) -> Result<(), String> {
    match name {
        "promissory_note" => {
            // Hardcoded at genesis — verify deployed ID matches canonical constant
            if cid != *PROMISSORY_NOTE_CONTRACT_ID {
                return Err(format!(
                    "PN contract ID mismatch: expected {}, got {}",
                    *PROMISSORY_NOTE_CONTRACT_ID, cid
                ));
            }
            Ok(())
        }
        "dao_escrow" => {
            DAO_ESCROW_CONTRACT_ID.set(cid)
                .map_err(|_| "dao_escrow contract ID already registered".to_string())
        }
        "drain_protection" => {
            DRAIN_PROTECTION_CONTRACT_ID.set(cid)
                .map_err(|_| "drain_protection contract ID already registered".to_string())
        }
        "dex" => {
            DEX_CONTRACT_ID.set(cid)
                .map_err(|_| "dex contract ID already registered".to_string())
        }
        "auction" => {
            AUCTION_CONTRACT_ID.set(cid)
                .map_err(|_| "auction contract ID already registered".to_string())
        }
        "stablecoin" => {
            STABLECOIN_CONTRACT_ID.set(cid)
                .map_err(|_| "stablecoin contract ID already registered".to_string())
        }
        "attestation" => {
            ATTESTATION_CONTRACT_ID.set(cid)
                .map_err(|_| "attestation contract ID already registered".to_string())
        }
        "baccarat" => {
            BACCARAT_CONTRACT_ID.set(cid)
                .map_err(|_| "baccarat contract ID already registered".to_string())
        }
        "betting_stake" => {
            BETTING_STAKE_CONTRACT_ID.set(cid)
                .map_err(|_| "betting_stake contract ID already registered".to_string())
        }
        "bridge" => {
            BRIDGE_CONTRACT_ID.set(cid)
                .map_err(|_| "bridge contract ID already registered".to_string())
        }
        "darkbet_exchange" => {
            DARKBET_EXCHANGE_CONTRACT_ID.set(cid)
                .map_err(|_| "darkbet_exchange contract ID already registered".to_string())
        }
        "darktoshi_dice" => {
            DARKTOSHI_DICE_CONTRACT_ID.set(cid)
                .map_err(|_| "darktoshi_dice contract ID already registered".to_string())
        }
        "escrow" => {
            ESCROW_CONTRACT_ID.set(cid)
                .map_err(|_| "escrow contract ID already registered".to_string())
        }
        "game_room" => {
            GAME_ROOM_CONTRACT_ID.set(cid)
                .map_err(|_| "game_room contract ID already registered".to_string())
        }
        "identity" => {
            IDENTITY_CONTRACT_ID.set(cid)
                .map_err(|_| "identity contract ID already registered".to_string())
        }
        "insurance_market" => {
            INSURANCE_MARKET_CONTRACT_ID.set(cid)
                .map_err(|_| "insurance_market contract ID already registered".to_string())
        }
        "labor_market" => {
            LABOR_MARKET_CONTRACT_ID.set(cid)
                .map_err(|_| "labor_market contract ID already registered".to_string())
        }
        "lottery" => {
            LOTTERY_CONTRACT_ID.set(cid)
                .map_err(|_| "lottery contract ID already registered".to_string())
        }
        "oracle" => {
            ORACLE_CONTRACT_ID.set(cid)
                .map_err(|_| "oracle contract ID already registered".to_string())
        }
        "otc_swap" => {
            OTC_SWAP_CONTRACT_ID.set(cid)
                .map_err(|_| "otc_swap contract ID already registered".to_string())
        }
        "pool_stake" => {
            POOL_STAKE_CONTRACT_ID.set(cid)
                .map_err(|_| "pool_stake contract ID already registered".to_string())
        }
        "relayer_endowment" => {
            RELAYER_ENDOWMENT_CONTRACT_ID.set(cid)
                .map_err(|_| "relayer_endowment contract ID already registered".to_string())
        }
        "roulette" => {
            ROULETTE_CONTRACT_ID.set(cid)
                .map_err(|_| "roulette contract ID already registered".to_string())
        }
        "slot" => {
            SLOT_CONTRACT_ID.set(cid)
                .map_err(|_| "slot contract ID already registered".to_string())
        }
        "subscription" => {
            SUBSCRIPTION_CONTRACT_ID.set(cid)
                .map_err(|_| "subscription contract ID already registered".to_string())
        }
        "tender" => {
            TENDER_CONTRACT_ID.set(cid)
                .map_err(|_| "tender contract ID already registered".to_string())
        }
        "bearer_bond" => {
            BEARER_BOND_CONTRACT_ID.set(cid)
                .map_err(|_| "bearer_bond contract ID already registered".to_string())
        }
        _ => Err(format!("Unknown contract name: {}", name)),
    }
}

/// Look up a contract's registered ContractId by name.
/// Returns None if the contract hasn't been registered yet.
pub fn get_contract_id(name: &str) -> Option<dwow_sdk::crypto::ContractId> {
    match name {
        "promissory_note" => Some(*PROMISSORY_NOTE_CONTRACT_ID),
        "native_token" => Some(*NATIVE_TOKEN_CONTRACT_ID),
        "deployooor" => Some(*DEPLOYOOOR_CONTRACT_ID),
        "attestation" => ATTESTATION_CONTRACT_ID.get().copied(),
        "identity" => IDENTITY_CONTRACT_ID.get().copied(),
        "oracle" => ORACLE_CONTRACT_ID.get().copied(),
        "dao_escrow" => DAO_ESCROW_CONTRACT_ID.get().copied(),
        "drain_protection" => DRAIN_PROTECTION_CONTRACT_ID.get().copied(),
        "escrow" => ESCROW_CONTRACT_ID.get().copied(),
        "auction" => AUCTION_CONTRACT_ID.get().copied(),
        "dex" => DEX_CONTRACT_ID.get().copied(),
        "subscription" => SUBSCRIPTION_CONTRACT_ID.get().copied(),
        "bearer_bond" => BEARER_BOND_CONTRACT_ID.get().copied(),
        "darkbet_exchange" => DARKBET_EXCHANGE_CONTRACT_ID.get().copied(),
        "lottery" => LOTTERY_CONTRACT_ID.get().copied(),
        "otc_swap" => OTC_SWAP_CONTRACT_ID.get().copied(),
        "baccarat" => BACCARAT_CONTRACT_ID.get().copied(),
        "darktoshi_dice" => DARKTOSHI_DICE_CONTRACT_ID.get().copied(),
        "game_room" => GAME_ROOM_CONTRACT_ID.get().copied(),
        "roulette" => ROULETTE_CONTRACT_ID.get().copied(),
        "slot" => SLOT_CONTRACT_ID.get().copied(),
        "betting_stake" => BETTING_STAKE_CONTRACT_ID.get().copied(),
        "pool_stake" => POOL_STAKE_CONTRACT_ID.get().copied(),
        "relayer_endowment" => RELAYER_ENDOWMENT_CONTRACT_ID.get().copied(),
        "bridge" => BRIDGE_CONTRACT_ID.get().copied(),
        "insurance_market" => INSURANCE_MARKET_CONTRACT_ID.get().copied(),
        "labor_market" => LABOR_MARKET_CONTRACT_ID.get().copied(),
        _ => None,
    }
}

// ============================================================================
// FUNCTION OPCODES
// ============================================================================

pub mod promissory_note {
    pub use dwow_promissory_note_contract::PromissoryNoteFunction;

    pub use dwow_promissory_note_contract::PROMISSORY_NOTE_CONTRACT_COINS_TREE;
    pub use dwow_promissory_note_contract::PROMISSORY_NOTE_CONTRACT_NULLIFIERS_TREE;
    pub use dwow_promissory_note_contract::PROMISSORY_NOTE_CONTRACT_MERKLE_TREE;
    pub use dwow_promissory_note_contract::PROMISSORY_NOTE_CONTRACT_INFO_TREE;

    // Client types
    pub use dwow_promissory_note_contract::client::PromissoryNote;
    pub use dwow_promissory_note_contract::client::verify_received_capability;
    pub use dwow_promissory_note_contract::client::transfer_v1::{
        TransferCallInput, TransferCallOutput,
    };
    pub use dwow_promissory_note_contract::client::token_mint_v1::TokenMintCallInput;
    pub use dwow_promissory_note_contract::client::mint_v1::MintCallInput;
    pub use dwow_promissory_note_contract::client::burn_v1::BurnCallInput;
    pub use dwow_promissory_note_contract::client::redeem_v1::{
        RedeemCallInput, RedeemCallOutput,
    };

    // Model types
    pub use dwow_promissory_note_contract::model::{
        BurnSpendHookPayload, Coin, CoinAttributes,
        Input as PromissoryNoteInput, Output as PromissoryNoteOutput,
        RedeemParamsV1, RedeemUpdateV1,
        TokenMintParamsV1, MintParamsV1, BurnParamsV1, TransferParamsV1,
    };

    pub type TokenId = dwow_sdk::pasta::pallas::Base;

    /// Balance decimal places
    pub const BALANCE_BASE10_DECIMALS: usize = 8;

    // SLED database tree names
    pub const SLED_MERKLE_TREES_PROMISSORY_NOTE: &str = "promissory_note_merkle_trees";

    // Token management constants
    pub const PN_TOKENS_TABLE: &str = "tokens";
    pub const PN_TOKENS_COL_TOKEN_ID: &str = "token_id";
    pub const PN_TOKENS_COL_MINT_AUTHORITY: &str = "mint_authority";
    pub const PN_TOKENS_COL_TOKEN_BLIND: &str = "token_blind";
    pub const PN_TOKENS_COL_IS_FROZEN: &str = "is_frozen";
    pub const PN_TOKENS_COL_FREEZE_HEIGHT: &str = "freeze_height";
}

// ============================================================================
// NATIVE TOKEN MODULE (DRKW token - fees and native operations)
// ============================================================================

pub mod native_token {
    pub use dwow_native_token_contract::NativeTokenFunction;

    pub use dwow_native_token_contract::NATIVE_TOKEN_CONTRACT_ZKAS_BURN_NS_V1;
    pub use dwow_native_token_contract::NATIVE_TOKEN_CONTRACT_ZKAS_FEE_NS_V1;
    pub use dwow_native_token_contract::NATIVE_TOKEN_CONTRACT_ZKAS_MINT_NS_V1;

    // ZK Circuit binaries
    pub use dwow_native_token_contract::NATIVE_TOKEN_CONTRACT_ZKAS_BURN_V1_BIN;
    pub use dwow_native_token_contract::NATIVE_TOKEN_CONTRACT_ZKAS_FEE_V1_BIN;
    pub use dwow_native_token_contract::NATIVE_TOKEN_CONTRACT_ZKAS_MINT_V1_BIN;

    pub use dwow_native_token_contract::client::pow_reward_v1::PoWRewardCallBuilder;
    pub use dwow_native_token_contract::client::burn_v1::{BurnCallBuilder, BurnCallDebris, BurnCallInput};
    pub use dwow_native_token_contract::client::fee_v1::{FeeCallBuilder, FeeCallInput, FeeCallOutput, FeeRevealed, FeeCallDebris as FeeDebris};
    pub use dwow_native_token_contract::client::NativeToken;

    pub use dwow_native_token_contract::model::{
        Coin as NativeCoin, CoinAttributes as NativeCoinAttributes,
        Input as NativeInput, Output as NativeOutput,
        FeeParamsV1, BurnParamsV1, TransferParamsV1 as NativeTransferParamsV1,
        DRKW_TOKEN_ID,
    };

    pub use dwow_native_token_contract::NATIVE_TOKEN_CONTRACT_COINS_TREE;
    pub use dwow_native_token_contract::NATIVE_TOKEN_CONTRACT_NULLIFIERS_TREE;
    pub use dwow_native_token_contract::NATIVE_TOKEN_CONTRACT_MERKLE_TREE;
    pub use dwow_native_token_contract::NATIVE_TOKEN_CONTRACT_INFO_TREE;
    pub use dwow_native_token_contract::NATIVE_TOKEN_CONTRACT_FEES_TREE;
}

// ============================================================================
// DAO ESCROW MODULE (dao_escrow contract)
// ============================================================================

pub mod dao_escrow {
    // Opcodes
    pub use dwow_dao_escrow_contract::DaoEscrowFunction;

    // ZK namespaces
    pub use dwow_dao_escrow_contract::DAO_ESCROW_ZKAS_INIT_NS;
    pub use dwow_dao_escrow_contract::DAO_ESCROW_ZKAS_PREMIUM_NS;

    // ZK Circuit binaries
    pub use dwow_dao_escrow_contract::DAO_ESCROW_ZKAS_INIT_V1_BIN;
    pub use dwow_dao_escrow_contract::DAO_ESCROW_ZKAS_PAY_PREMIUM_V1_BIN;

    // Database tree names
    pub use dwow_dao_escrow_contract::DAO_ESCROW_CONTRACT_INFO_TREE;
    pub use dwow_dao_escrow_contract::DAO_ESCROW_CONTRACT_BULLAS_TREE;
    pub use dwow_dao_escrow_contract::DAO_ESCROW_CONTRACT_MEMBERSHIP_TREE;
    pub use dwow_dao_escrow_contract::DAO_ESCROW_CONTRACT_ENDOWMENT_TREE;

    // Mode constants
    pub use dwow_dao_escrow_contract::modes::MODE_ESCROW;
    pub use dwow_dao_escrow_contract::modes::MODE_TREASURY;
    pub use dwow_dao_escrow_contract::modes::MODE_TREASURY_ENDOWMENT;

    // Model types
    pub use dwow_dao_escrow_contract::client::{
        init_v1::*, pay_premium_v1::*,
    };
    pub use dwow_dao_escrow_contract::model::{
        DaoEscrow, DaoEscrowBulla, DaoEscrowMode, FeeConfig, Membership,
        MembershipNote, ClaimId, VoteType,
        InitializeParamsV1, InitializeUpdateV1,
        UpdateParamsV1, UpdateUpdateV1,
        PayPremiumParamsV1, PayPremiumUpdateV1,
        WithdrawParamsV1, WithdrawUpdateV1,
        EndowmentWithdrawParamsV1, EndowmentWithdrawUpdateV1,
        TreasurySpendParamsV1, TreasurySpendUpdateV1,
        EnableDrainProtectionParamsV1, EnableDrainProtectionUpdateV1,
        ProposeClaimParamsV1, ProposeClaimUpdateV1,
        VoteClaimParamsV1, VoteClaimUpdateV1,
    };

    // SLED database tree names
    pub const SLED_MERKLE_TREES_DAO_DAOS: &str = "dao_merkle_trees_dao_daos";
    pub const SLED_MERKLE_TREES_DAO_PROPOSALS: &str = "dao_merkle_trees_dao_proposals";
}

// ============================================================================
// DEPLOYOOOR MODULE (contract deployment)
// ============================================================================

pub mod deployooor {
    pub use dwow_deployooor_contract::DeployFunction;
    pub use dwow_deployooor_contract::client::deploy_v1::DeployCallBuilder;
    pub use dwow_deployooor_contract::client::lock_v1::LockCallBuilder;
    pub use dwow_deployooor_contract::model::*;
    pub use dwow_deployooor_contract::DEPLOY_CONTRACT_INFO_TREE;
    pub use dwow_deployooor_contract::DEPLOY_CONTRACT_LOCK_TREE;
}

use dwow_sdk::contract_client::{ContractClientRegistry, GenericContractClient};
use std::sync::OnceLock;

static CLIENT_REGISTRY: OnceLock<ContractClientRegistry> = OnceLock::new();

pub fn get_client_registry() -> &'static ContractClientRegistry {
    CLIENT_REGISTRY.get_or_init(|| {
        let mut registry = ContractClientRegistry::new();
        // Each contract crate registers its client here.
        // All 29 contracts go through the same generic dispatch path.

        // Specialized clients (have zkbins.rs + real ZK builders)
        registry.register("native_token", Box::new(
            dwow_native_token_contract::client::NativeTokenClient));
        registry.register("promissory_note", Box::new(
            dwow_promissory_note_contract::client::PromissoryNoteClient));
        registry.register("escrow", Box::new(
            dwow_escrow_contract::client::EscrowClient));

        // Generic clients — all 26 capability contracts
        // Each contract's manifest declares these opcodes.
        registry.register("deployooor", Box::new(GenericContractClient::new(
            "deployooor", &[("DeployV1", 0x00), ("LockV1", 0x01)])));
        registry.register("bearer_bond", Box::new(GenericContractClient::new(
            "bearer_bond", &[("IssueStakeV1", 0x00), ("PayInterestV1", 0x01), ("UnstakeV1", 0x02)])));
        registry.register("dao_escrow", Box::new(GenericContractClient::new(
            "dao_escrow", &[("InitV1", 0x00), ("PayPremiumV1", 0x01), ("ProposeClaimV1", 0x02)])));
        registry.register("auction", Box::new(GenericContractClient::new(
            "auction", &[("BidV1", 0x00), ("SettleV1", 0x01)])));
        registry.register("game_room", Box::new(GenericContractClient::new(
            "game_room", &[("CreateRoomV1", 0x00), ("JoinRoomV1", 0x01)])));
        registry.register("lottery", Box::new(GenericContractClient::new(
            "lottery", &[("EnterV1", 0x00), ("DrawV1", 0x01)])));
        registry.register("stablecoin", Box::new(GenericContractClient::new(
            "stablecoin", &[("MintV1", 0x00), ("BurnV1", 0x01), ("AccrueInterestV1", 0x02)])));
        registry.register("dex", Box::new(GenericContractClient::new(
            "dex", &[("SwapV1", 0x00), ("AddLiquidityV1", 0x01)])));
        registry.register("bridge", Box::new(GenericContractClient::new(
            "bridge", &[("DepositV1", 0x00), ("WithdrawV1", 0x01), ("AcceptV1", 0x02)])));
        registry.register("attestation", Box::new(GenericContractClient::new(
            "attestation", &[("AttestV1", 0x00), ("RevokeV1", 0x01)])));
        registry.register("identity", Box::new(GenericContractClient::new(
            "identity", &[("CreateV1", 0x00), ("VerifyV1", 0x01)])));
        registry.register("oracle", Box::new(GenericContractClient::new(
            "oracle", &[("PublishV1", 0x00), ("QueryV1", 0x01)])));
        registry.register("subscription", Box::new(GenericContractClient::new(
            "subscription", &[("SubscribeV1", 0x00), ("RenewV1", 0x01), ("CancelV1", 0x02)])));
        registry.register("betting_stake", Box::new(GenericContractClient::new(
            "betting_stake", &[("InitV1", 0x00), ("StakeV1", 0x01), ("UnstakeV1", 0x02), ("ClaimV1", 0x03)])));
        registry.register("insurance_market", Box::new(GenericContractClient::new(
            "insurance_market", &[("UnderwriteV1", 0x00), ("ClaimV1", 0x01)])));
        registry.register("labor_market", Box::new(GenericContractClient::new(
            "labor_market", &[("PostJobV1", 0x00), ("AcceptJobV1", 0x01), ("CompleteJobV1", 0x02), ("PayV1", 0x03)])));
        registry.register("darkbet_exchange", Box::new(GenericContractClient::new(
            "darkbet_exchange", &[("PlaceOrderV1", 0x00), ("MatchOrdersV1", 0x01)])));
        registry.register("darktoshi_dice", Box::new(GenericContractClient::new(
            "darktoshi_dice", &[("RollV1", 0x00)])));
        registry.register("baccarat", Box::new(GenericContractClient::new(
            "baccarat", &[("DealV1", 0x00)])));
        registry.register("roulette", Box::new(GenericContractClient::new(
            "roulette", &[("SpinV1", 0x00)])));
        registry.register("slot", Box::new(GenericContractClient::new(
            "slot", &[("SpinV1", 0x00)])));
        registry.register("relayer_endowment", Box::new(GenericContractClient::new(
            "relayer_endowment", &[("InitV1", 0x00), ("FundV1", 0x01), ("SubmitProofV1", 0x02)])));
        registry.register("pool_stake", Box::new(GenericContractClient::new(
            "pool_stake", &[("InitV1", 0x00), ("DepositV1", 0x01), ("WithdrawV1", 0x02), ("ClaimRewardV1", 0x03)])));
        registry.register("tender", Box::new(GenericContractClient::new(
            "tender", &[("CreateRFQ", 0x00), ("SubmitBidV1", 0x01), ("AcceptBidV1", 0x02), ("SettleV1", 0x03)])));
        registry.register("otc_swap", Box::new(GenericContractClient::new(
            "otc_swap", &[("InitV1", 0x00), ("JoinV1", 0x01), ("SignV1", 0x02), ("ExecuteV1", 0x03)])));
        registry.register("drain_protection", Box::new(GenericContractClient::new(
            "drain_protection", &[("InitV1", 0x00), ("VoteV1", 0x01), ("ExecuteV1", 0x02)])));

        registry
    })
}

