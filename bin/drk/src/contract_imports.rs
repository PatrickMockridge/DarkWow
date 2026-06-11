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

// Promissory Note Contract ID - user deployed, no hardcoded ID
// Use OnceLock to allow runtime registration
pub static PROMISSORY_NOTE_CONTRACT_ID: std::sync::OnceLock<dwow_sdk::crypto::ContractId> =
    std::sync::OnceLock::new();

pub const DEPLOYOOOR_CONTRACT_ID: &str = "deployooor";

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
            PROMISSORY_NOTE_CONTRACT_ID.set(cid)
                .map_err(|_| "promissory_note contract ID already registered".to_string())
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
        "promissory_note" => PROMISSORY_NOTE_CONTRACT_ID.get().copied(),
        "native_token" => Some(*NATIVE_TOKEN_CONTRACT_ID),
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
        _ => None,
    }
}

// ============================================================================
// FUNCTION OPCODES
// ============================================================================

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum NativeTokenOpcodes {
    FeeV1 = 0x00,
    MintV1 = 0x01,
    BurnV1 = 0x02,
    TransferV1 = 0x03,
    SpendV1 = 0x04,
    PoWRewardV1 = 0x05,
}

impl From<NativeTokenOpcodes> for u8 {
    fn from(v: NativeTokenOpcodes) -> u8 { v as u8 }
}

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum PromissoryNoteOpcodes {
    TokenMintV1 = 0x00,
    RedeemV1 = 0x01,
    MintV1 = 0x02,
    BurnV1 = 0x03,
    TransferV1 = 0x04,
    OtcSwapV1 = 0x05,
}

impl From<PromissoryNoteOpcodes> for u8 {
    fn from(v: PromissoryNoteOpcodes) -> u8 { v as u8 }
}

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum DaoEscrowOpcodes {
    InitializeV1 = 0x00,
    UpdateV1 = 0x01,
    PayPremiumV1 = 0x02,
    WithdrawV1 = 0x03,
    EndowmentWithdrawV1 = 0x04,
    TreasurySpendV1 = 0x05,
    EnableDrainProtectionV1 = 0x06,
    ProposeClaimV1 = 0x07,
    VoteClaimV1 = 0x08,
}

impl From<DaoEscrowOpcodes> for u8 {
    fn from(v: DaoEscrowOpcodes) -> u8 { v as u8 }
}

// ============================================================================
// TOKEN ID CONSTANTS
// ============================================================================

pub const DRKW_TOKEN_ID: pallas::Base = pallas::Base::zero();
pub const DRKW_TOKEN_ID_BYTES: [u8; 32] = [0u8; 32];

// ============================================================================
// MONEY MODULE (Promissory Note - DeFi tokens / ERC-20 style)
// ============================================================================

pub mod promissory_note {
    pub use dwow_promissory_note_contract::PromissoryNoteFunction;

    pub use dwow_promissory_note_contract::PROMISSORY_NOTE_CONTRACT_ZKAS_TOKEN_MINT_NS_V1;
    pub use dwow_promissory_note_contract::PROMISSORY_NOTE_CONTRACT_ZKAS_MINT_NS_V1;
    pub use dwow_promissory_note_contract::PROMISSORY_NOTE_CONTRACT_ZKAS_BURN_NS_V1;

    // ZK Circuit binaries
    pub use dwow_promissory_note_contract::PROMISSORY_NOTE_CONTRACT_ZKAS_TOKEN_MINT_V1_BIN;
    pub use dwow_promissory_note_contract::PROMISSORY_NOTE_CONTRACT_ZKAS_MINT_V1_BIN;
    pub use dwow_promissory_note_contract::PROMISSORY_NOTE_CONTRACT_ZKAS_BURN_V1_BIN;
    pub use dwow_promissory_note_contract::PROMISSORY_NOTE_CONTRACT_ZKAS_BLIND_OUTPUT_V1_BIN;
    pub use dwow_promissory_note_contract::PROMISSORY_NOTE_CONTRACT_ZKAS_REDEEM_V1_BIN;

    pub use dwow_promissory_note_contract::PROMISSORY_NOTE_CONTRACT_COINS_TREE;
    pub use dwow_promissory_note_contract::PROMISSORY_NOTE_CONTRACT_NULLIFIERS_TREE;
    pub use dwow_promissory_note_contract::PROMISSORY_NOTE_CONTRACT_MERKLE_TREE;
    pub use dwow_promissory_note_contract::PROMISSORY_NOTE_CONTRACT_INFO_TREE;

    // Client types
    pub use dwow_promissory_note_contract::client::PromissoryNote;
    pub use dwow_promissory_note_contract::client::verify_received_coin;
    pub use dwow_promissory_note_contract::client::transfer_v1::{
        TransferCallBuilder, TransferCallDebris, TransferCallInput, TransferCallOutput,
    };
    pub use dwow_promissory_note_contract::client::token_mint_v1::{TokenMintCallBuilder, TokenMintCallInput};
    pub use dwow_promissory_note_contract::client::mint_v1::{MintCallBuilder, MintCallInput};
    pub use dwow_promissory_note_contract::client::burn_v1::{BurnCallBuilder, BurnCallInput};
    pub use dwow_promissory_note_contract::client::redeem_v1::{
        RedeemCallBuilder, RedeemCallDebris, RedeemCallInput, RedeemCallOutput,
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

// ============================================================================
// STABLECOIN MODULE (CDP stablecoin - collateralized debt position)
// ============================================================================

pub mod stablecoin {
    pub use dwow_stablecoin_contract::StablecoinFunction;

    // ZK namespaces
    pub use dwow_stablecoin_contract::STABLECOIN_CONTRACT_ZKAS_OPEN_NS_V1;
    pub use dwow_stablecoin_contract::STABLECOIN_CONTRACT_ZKAS_ADD_COLLATERAL_NS_V1;
    pub use dwow_stablecoin_contract::STABLECOIN_CONTRACT_ZKAS_REMOVE_COLLATERAL_NS_V1;
    pub use dwow_stablecoin_contract::STABLECOIN_CONTRACT_ZKAS_MINT_STABLE_NS_V1;
    pub use dwow_stablecoin_contract::STABLECOIN_CONTRACT_ZKAS_REPAY_STABLE_NS_V1;
    pub use dwow_stablecoin_contract::STABLECOIN_CONTRACT_ZKAS_LIQUIDATE_NS_V1;

    // Database tree names
    pub use dwow_stablecoin_contract::STABLECOIN_CONTRACT_INFO_TREE;
    pub use dwow_stablecoin_contract::STABLECOIN_CONTRACT_POSITIONS_TREE;
    pub use dwow_stablecoin_contract::STABLECOIN_CONTRACT_POSITION_NULLIFIERS_TREE;
    pub use dwow_stablecoin_contract::STABLECOIN_CONTRACT_STABLECOIN_TREE;
    pub use dwow_stablecoin_contract::STABLECOIN_CONTRACT_COLLATERAL_TREE;
    pub use dwow_stablecoin_contract::STABLECOIN_CONTRACT_LIQUIDATIONS_TREE;

    // Client types
    pub use dwow_stablecoin_contract::client::*;

    // Model types
    pub use dwow_stablecoin_contract::model::*;

    // Opcodes
    #[derive(Debug, Clone, Copy)]
    #[repr(u8)]
    pub enum StablecoinOpcodes {
        InitializeV1 = 0x00,
        OpenPositionV1 = 0x01,
        AddCollateralV1 = 0x02,
        RemoveCollateralV1 = 0x03,
        MintStableV1 = 0x04,
        RepayStableV1 = 0x05,
        LiquidateV1 = 0x06,
        UpdateConfigV1 = 0x07,
        GovernanceReportV1 = 0x08,
        AccrueInterestV1 = 0x09,
    }

    impl From<StablecoinOpcodes> for u8 {
        fn from(v: StablecoinOpcodes) -> u8 { v as u8 }
    }
}

// ============================================================================
// DRAIN PROTECTION MODULE (governance for endowment/treasury funds)
// ============================================================================

pub mod drain_protection {
    pub use dwow_drain_protection_contract::DrainProtectionFunction;

    // Database tree names
    pub use dwow_drain_protection_contract::DRAIN_PROTECTION_CONTRACT_INFO_TREE;
    pub use dwow_drain_protection_contract::DRAIN_PROTECTION_CONTRACT_PROPOSALS_TREE;
    pub use dwow_drain_protection_contract::DRAIN_PROTECTION_CONTRACT_VOTES_TREE;
    pub use dwow_drain_protection_contract::DRAIN_PROTECTION_CONTRACT_FUNDS_TREE;

    // Model types
    pub use dwow_drain_protection_contract::model::*;

    // Client types
    pub use dwow_drain_protection_contract::client::*;
}

// ============================================================================
// CONTRACT REGISTRY INTEGRATION
// ============================================================================
// Contract implementations for the generic registry system.
// See [`crate::contract_registry`] for the registry infrastructure.

use crate::contract_registry::Contract;

/// PromissoryNote contract info for registry
pub struct PromissoryNoteContract;

impl Contract for PromissoryNoteContract {
    fn contract_id(&self) -> ContractId {
        *PROMISSORY_NOTE_CONTRACT_ID.get().unwrap()
    }

    fn name(&self) -> &'static str {
        "PromissoryNote"
    }

    fn dependencies(&self) -> Vec<ContractId> {
        vec![]
    }

    fn is_initialized(&self) -> bool {
        PROMISSORY_NOTE_CONTRACT_ID.get().is_some()
    }
}

/// NativeToken contract info for registry
pub struct NativeTokenContract;

impl Contract for NativeTokenContract {
    fn contract_id(&self) -> ContractId {
        *NATIVE_TOKEN_CONTRACT_ID
    }

    fn name(&self) -> &'static str {
        "NativeToken"
    }

    fn dependencies(&self) -> Vec<ContractId> {
        vec![]
    }

    fn is_initialized(&self) -> bool {
        true // Native token is hardcoded genesis
    }
}

/// DaoEscrow contract info for registry
pub struct DaoEscrowContract;

impl Contract for DaoEscrowContract {
    fn contract_id(&self) -> ContractId {
        *DAO_ESCROW_CONTRACT_ID.get().unwrap()
    }

    fn name(&self) -> &'static str {
        "DaoEscrow"
    }

    fn dependencies(&self) -> Vec<ContractId> {
        // DaoEscrow uses promissory_note::transfer_v1 for endowment withdrawals
        vec![*PROMISSORY_NOTE_CONTRACT_ID.get().unwrap()]
    }

    fn is_initialized(&self) -> bool {
        DAO_ESCROW_CONTRACT_ID.get().is_some()
    }
}

/// Stablecoin contract info for registry
pub struct StablecoinContract;

impl Contract for StablecoinContract {
    fn contract_id(&self) -> ContractId {
        *STABLECOIN_CONTRACT_ID.get().unwrap()
    }

    fn name(&self) -> &'static str {
        "Stablecoin"
    }

    fn dependencies(&self) -> Vec<ContractId> {
        // Stablecoin uses promissory_note::transfer_v1 for collateral transfers
        vec![*PROMISSORY_NOTE_CONTRACT_ID.get().unwrap()]
    }

    fn is_initialized(&self) -> bool {
        STABLECOIN_CONTRACT_ID.get().is_some()
    }
}

/// DEX contract info for registry
pub struct DexContract;

impl Contract for DexContract {
    fn contract_id(&self) -> ContractId {
        *DEX_CONTRACT_ID.get().unwrap()
    }

    fn name(&self) -> &'static str {
        "DEX"
    }

    fn dependencies(&self) -> Vec<ContractId> {
        // DEX uses promissory_note::transfer_v1 for token swaps
        vec![*PROMISSORY_NOTE_CONTRACT_ID.get().unwrap()]
    }

    fn is_initialized(&self) -> bool {
        DEX_CONTRACT_ID.get().is_some()
    }
}

/// Auction contract info for registry
pub struct AuctionContract;

impl Contract for AuctionContract {
    fn contract_id(&self) -> ContractId {
        *AUCTION_CONTRACT_ID.get().unwrap()
    }

    fn name(&self) -> &'static str {
        "Auction"
    }

    fn dependencies(&self) -> Vec<ContractId> {
        vec![*PROMISSORY_NOTE_CONTRACT_ID.get().unwrap()]
    }

    fn is_initialized(&self) -> bool {
        AUCTION_CONTRACT_ID.get().is_some()
    }
}

/// DrainProtection contract info for registry
pub struct DrainProtectionContract;

impl Contract for DrainProtectionContract {
    fn contract_id(&self) -> ContractId {
        *DRAIN_PROTECTION_CONTRACT_ID.get().unwrap()
    }

    fn name(&self) -> &'static str {
        "DrainProtection"
    }

    fn dependencies(&self) -> Vec<ContractId> {
        vec![]
    }

    fn is_initialized(&self) -> bool {
        DRAIN_PROTECTION_CONTRACT_ID.get().is_some()
    }
}
// ============================================================================
// ATTESTATION MODULE
// ============================================================================

pub mod attestation {
    pub use dwow_attestation_contract::AttestationFunction;
    pub use dwow_attestation_contract::ATTESTATION_CONTRACT_ATTESTATIONS_TREE;
    pub use dwow_attestation_contract::ATTESTATION_CONTRACT_CLAIMS_TREE;
    pub use dwow_attestation_contract::ATTESTATION_CONTRACT_NULLIFIERS_TREE;
    pub use dwow_attestation_contract::ATTESTATION_CONTRACT_INDEX_TREE as ATTESTATION_INFO_TREE;
}

pub struct AttestationContract;
impl Contract for AttestationContract {
    fn contract_id(&self) -> ContractId { *ATTESTATION_CONTRACT_ID.get().unwrap() }
    fn name(&self) -> &'static str { "Attestation" }
    fn dependencies(&self) -> Vec<ContractId> { vec![] }
    fn is_initialized(&self) -> bool { ATTESTATION_CONTRACT_ID.get().is_some() }
}

// ============================================================================
// BACCARAT MODULE
// ============================================================================

pub mod baccarat {
    pub use dwow_baccarat_contract::BaccaratFunction;
    pub use dwow_baccarat_contract::BACCARAT_CONTRACT_BETS_TREE;
    pub use dwow_baccarat_contract::BACCARAT_CONTRACT_NULLIFIERS_TREE;
    pub use dwow_baccarat_contract::BACCARAT_CONTRACT_INFO_TREE;
    pub use dwow_baccarat_contract::BACCARAT_CONTRACT_HOUSE_TREE;
    pub use dwow_baccarat_contract::BACCARAT_CONTRACT_ZKAS_COMMIT_NS;
    pub use dwow_baccarat_contract::BACCARAT_CONTRACT_ZKAS_SETTLE_NS;
}

pub struct BaccaratContract;
impl Contract for BaccaratContract {
    fn contract_id(&self) -> ContractId { *BACCARAT_CONTRACT_ID.get().unwrap() }
    fn name(&self) -> &'static str { "Baccarat" }
    fn dependencies(&self) -> Vec<ContractId> { vec![] }
    fn is_initialized(&self) -> bool { BACCARAT_CONTRACT_ID.get().is_some() }
}

// ============================================================================
// BETTING STAKE MODULE
// ============================================================================

pub mod betting_stake {
    pub use dwow_betting_stake_contract::BettingStakeFunction;
    pub use dwow_betting_stake_contract::BETTING_STAKE_REGISTRY_TREE;
    pub use dwow_betting_stake_contract::BETTING_STAKE_STAKES_TREE;
    pub use dwow_betting_stake_contract::BETTING_STAKE_EARNINGS_TREE;
    pub use dwow_betting_stake_contract::BETTING_STAKE_ZKAS_INIT_NS;
    pub use dwow_betting_stake_contract::BETTING_STAKE_ZKAS_STAKE_NS;
    pub use dwow_betting_stake_contract::BETTING_STAKE_ZKAS_UNSTAKE_NS;
    pub use dwow_betting_stake_contract::BETTING_STAKE_ZKAS_CLAIM_NS;
    pub use dwow_betting_stake_contract::BETTING_STAKE_ZKAS_UPDATE_RISK_NS;
}

pub struct BettingStakeContract;
impl Contract for BettingStakeContract {
    fn contract_id(&self) -> ContractId { *BETTING_STAKE_CONTRACT_ID.get().unwrap() }
    fn name(&self) -> &'static str { "BettingStake" }
    fn dependencies(&self) -> Vec<ContractId> { vec![*PROMISSORY_NOTE_CONTRACT_ID.get().unwrap()] }
    fn is_initialized(&self) -> bool { BETTING_STAKE_CONTRACT_ID.get().is_some() }
}

// ============================================================================
// BEARER BOND MODULE (Profit-Share Staking)
// ============================================================================

pub mod bearer_bond {
    pub use dwow_bearer_bond_contract::BearerBondFunction;

    // ZK namespaces
    pub use dwow_bearer_bond_contract::BEARER_BOND_CONTRACT_ZKAS_BURN_NS_V1;
    pub use dwow_bearer_bond_contract::BEARER_BOND_CONTRACT_ZKAS_BLIND_OUTPUT_NS_V1;
    pub use dwow_bearer_bond_contract::BEARER_BOND_CONTRACT_ZKAS_REDEEM_NS_V1;
    pub use dwow_bearer_bond_contract::BEARER_BOND_CONTRACT_ZKAS_PROVE_COVERAGE_NS_V1;

    // ZK Circuit binaries
    pub use dwow_bearer_bond_contract::BEARER_BOND_CONTRACT_ZKAS_BURN_V1_BIN;
    pub use dwow_bearer_bond_contract::BEARER_BOND_CONTRACT_ZKAS_BLIND_OUTPUT_V1_BIN;
    pub use dwow_bearer_bond_contract::BEARER_BOND_CONTRACT_ZKAS_REDEEM_V1_BIN;
    pub use dwow_bearer_bond_contract::BEARER_BOND_CONTRACT_ZKAS_PROVE_COVERAGE_V1_BIN;

    // Database tree names
    pub use dwow_bearer_bond_contract::BEARER_BOND_CONTRACT_COINS_TREE;
    pub use dwow_bearer_bond_contract::BEARER_BOND_CONTRACT_NULLIFIERS_TREE;
    pub use dwow_bearer_bond_contract::BEARER_BOND_CONTRACT_COIN_MERKLE_TREE;
    pub use dwow_bearer_bond_contract::BEARER_BOND_CONTRACT_INFO_TREE;
    pub use dwow_bearer_bond_contract::BEARER_BOND_CONTRACT_COIN_ROOTS_TREE;
    pub use dwow_bearer_bond_contract::BEARER_BOND_CONTRACT_NULLIFIER_ROOTS_TREE;
    pub use dwow_bearer_bond_contract::BEARER_BOND_CONTRACT_BONDS_INFO_TREE;

    // Capability constants
    pub use dwow_bearer_bond_contract::capability::{
        CAP_STAKE, CAP_INTEREST_RIGHT, CAP_UNSTAKE_RIGHT, CAP_RECEIPT, CAP_COVERAGE_REPORT,
        CAP_EMERGENCY_UNSTAKE,
    };

    // Client types
    pub use dwow_bearer_bond_contract::client::{
        BearerBondNote, point_coords,
        issue_stake_v1::{IssueStakeCallBuilder, IssueStakeCallInput},
        transfer_stake_v1::{TransferStakeCallBuilder, TransferStakeCallInput},
        request_interest_v1::{RequestInterestCallBuilder, RequestInterestCallInput},
        emergency_unstake_v1::{EmergencyUnstakeCallBuilder, EmergencyUnstakeCallInput},
        unstake_v1::{UnstakeCallBuilder, UnstakeCallInput},
        burn_stake_v1::{BurnStakeCallBuilder, BurnStakeCallInput},
        prove_coverage_v1::{ProveCoverageCallBuilder, ProveCoverageCallInput},
        pay_interest_v1::{PayInterestCallBuilder, PayInterestCallInput},
    };

    // Model types
    pub use dwow_bearer_bond_contract::model::{
        BondCoin, BondCoinWitness, BondInput, BondInputWitness, CoinAttributes,
        IssueStakeParamsV1, IssueStakeUpdateV1,
        TransferStakeParamsV1, TransferStakeUpdateV1,
        RequestInterestParamsV1, RequestInterestUpdateV1,
        PayInterestParamsV1, PayInterestUpdateV1,
        RequestedClaim, ClaimStatus,
        EmergencyUnstakeParamsV1, EmergencyUnstakeUpdateV1,
        UnstakeParamsV1, UnstakeUpdateV1,
        BurnStakeParamsV1, BurnStakeUpdateV1,
        ProveCoverageParamsV1, ProveCoverageUpdateV1,
        CoverageReport, BondSeriesInfo, SeriesStatus, Nullifier,
        calculate_interest, BP_PRECISION, BLOCKS_PER_YEAR,
    };
}

pub struct BearerBondContract;
impl Contract for BearerBondContract {
    fn contract_id(&self) -> ContractId { *BEARER_BOND_CONTRACT_ID.get().unwrap() }
    fn name(&self) -> &'static str { "BearerBond" }
    fn dependencies(&self) -> Vec<ContractId> {
        vec![*PROMISSORY_NOTE_CONTRACT_ID.get().unwrap()]
    }
    fn is_initialized(&self) -> bool { BEARER_BOND_CONTRACT_ID.get().is_some() }
}

// ============================================================================
// BRIDGE MODULE
// ============================================================================

pub mod bridge {
    pub use dwow_bridge_contract::BridgeFunction;
    pub use dwow_bridge_contract::BRIDGE_CONTRACT_INFO_TREE;
    pub use dwow_bridge_contract::BRIDGE_CONTRACT_DEPOSITS_TREE;
    pub use dwow_bridge_contract::BRIDGE_CONTRACT_WITHDRAWALS_TREE;
    pub use dwow_bridge_contract::BRIDGE_CONTRACT_NULLIFIERS_TREE;
    pub use dwow_bridge_contract::BRIDGE_CONTRACT_HTLCS_TREE;
    pub use dwow_bridge_contract::BRIDGE_CONTRACT_ZKAS_DEPOSIT_NS_V1;
    pub use dwow_bridge_contract::BRIDGE_CONTRACT_ZKAS_WITHDRAW_NS_V1;
}

pub struct BridgeContract;
impl Contract for BridgeContract {
    fn contract_id(&self) -> ContractId { *BRIDGE_CONTRACT_ID.get().unwrap() }
    fn name(&self) -> &'static str { "Bridge" }
    fn dependencies(&self) -> Vec<ContractId> { vec![] }
    fn is_initialized(&self) -> bool { BRIDGE_CONTRACT_ID.get().is_some() }
}

// ============================================================================
// DARKBET EXCHANGE MODULE
// ============================================================================

pub mod darkbet_exchange {
    pub use dwow_darkbet_exchange_contract::DarkbetFunction;
    pub use dwow_darkbet_exchange_contract::DARKBET_EXCHANGE_MARKETS_TREE;
    pub use dwow_darkbet_exchange_contract::DARKBET_EXCHANGE_BACK_ORDERS_TREE;
    pub use dwow_darkbet_exchange_contract::DARKBET_EXCHANGE_LAY_ORDERS_TREE;
    pub use dwow_darkbet_exchange_contract::DARKBET_EXCHANGE_MATCHES_TREE;
    pub use dwow_darkbet_exchange_contract::DARKBET_EXCHANGE_POSITIONS_TREE;
    pub use dwow_darkbet_exchange_contract::DARKBET_EXCHANGE_NULLIFIERS_TREE;
    pub use dwow_darkbet_exchange_contract::DARKBET_EXCHANGE_INFO_TREE;
}

pub struct DarkbetExchangeContract;
impl Contract for DarkbetExchangeContract {
    fn contract_id(&self) -> ContractId { *DARKBET_EXCHANGE_CONTRACT_ID.get().unwrap() }
    fn name(&self) -> &'static str { "DarkbetExchange" }
    fn dependencies(&self) -> Vec<ContractId> { vec![*PROMISSORY_NOTE_CONTRACT_ID.get().unwrap()] }
    fn is_initialized(&self) -> bool { DARKBET_EXCHANGE_CONTRACT_ID.get().is_some() }
}

// ============================================================================
// DARKTOSHI DICE MODULE
// ============================================================================

pub mod darktoshi_dice {
    pub use dwow_darktoshi_dice_contract::DiceFunction;
    pub use dwow_darktoshi_dice_contract::DICE_CONTRACT_BETS_TREE;
    pub use dwow_darktoshi_dice_contract::DICE_CONTRACT_NULLIFIERS_TREE;
    pub use dwow_darktoshi_dice_contract::DICE_CONTRACT_INFO_TREE;
    pub use dwow_darktoshi_dice_contract::DICE_CONTRACT_HOUSE_TREE;
    pub use dwow_darktoshi_dice_contract::DICE_CONTRACT_ZKAS_COMMIT_NS;
    pub use dwow_darktoshi_dice_contract::DICE_CONTRACT_ZKAS_SETTLE_NS;
}

pub struct DarktoshiDiceContract;
impl Contract for DarktoshiDiceContract {
    fn contract_id(&self) -> ContractId { *DARKTOSHI_DICE_CONTRACT_ID.get().unwrap() }
    fn name(&self) -> &'static str { "DarktoshiDice" }
    fn dependencies(&self) -> Vec<ContractId> { vec![] }
    fn is_initialized(&self) -> bool { DARKTOSHI_DICE_CONTRACT_ID.get().is_some() }
}

// ============================================================================
// ESCROW MODULE
// ============================================================================

pub mod escrow {
    pub use dwow_escrow_contract::EscrowFunction;
    pub use dwow_escrow_contract::ESCROW_CONTRACT_INFO_TREE;
    pub use dwow_escrow_contract::ESCROW_CONTRACT_ESCROWS_TREE;
    pub use dwow_escrow_contract::ESCROW_CONTRACT_NULLIFIERS_TREE;
    pub use dwow_escrow_contract::ESCROW_CONTRACT_ZKAS_CREATE_NS_V1;
    pub use dwow_escrow_contract::ESCROW_CONTRACT_ZKAS_FUND_NS_V1;
    pub use dwow_escrow_contract::ESCROW_CONTRACT_ZKAS_CLAIM_NS_V1;
    pub use dwow_escrow_contract::ESCROW_CONTRACT_ZKAS_REFUND_NS_V1;
}

pub struct EscrowContract;
impl Contract for EscrowContract {
    fn contract_id(&self) -> ContractId { *ESCROW_CONTRACT_ID.get().unwrap() }
    fn name(&self) -> &'static str { "Escrow" }
    fn dependencies(&self) -> Vec<ContractId> { vec![] }
    fn is_initialized(&self) -> bool { ESCROW_CONTRACT_ID.get().is_some() }
}

// ============================================================================
// GAME ROOM MODULE
// ============================================================================

pub mod game_room {
    pub use dwow_game_room_contract::GameRoomFunction;
    pub use dwow_game_room_contract::GAME_ROOM_ROOMS_TREE;
    pub use dwow_game_room_contract::GAME_ROOM_ACCOUNTS_TREE;
    pub use dwow_game_room_contract::GAME_ROOM_POTS_TREE;
    pub use dwow_game_room_contract::GAME_ROOM_BETS_TREE;
    pub use dwow_game_room_contract::GAME_ROOM_NULLIFIERS_TREE;
    pub use dwow_game_room_contract::GAME_ROOM_ZKAS_CREATE_ROOM_NS;
    pub use dwow_game_room_contract::GAME_ROOM_ZKAS_DEPOSIT_NS;
    pub use dwow_game_room_contract::GAME_ROOM_ZKAS_PLACE_BET_NS;
    pub use dwow_game_room_contract::GAME_ROOM_ZKAS_SETTLE_POT_NS;
    pub use dwow_game_room_contract::GAME_ROOM_ZKAS_CLAIM_NS;
}

pub struct GameRoomContract;
impl Contract for GameRoomContract {
    fn contract_id(&self) -> ContractId { *GAME_ROOM_CONTRACT_ID.get().unwrap() }
    fn name(&self) -> &'static str { "GameRoom" }
    fn dependencies(&self) -> Vec<ContractId> { vec![*PROMISSORY_NOTE_CONTRACT_ID.get().unwrap()] }
    fn is_initialized(&self) -> bool { GAME_ROOM_CONTRACT_ID.get().is_some() }
}

// ============================================================================
// IDENTITY MODULE
// ============================================================================

pub mod identity {
    pub use dwow_identity_contract::IdentityFunction;
    pub use dwow_identity_contract::IDENTITY_CONTRACT_CREDENTIALS_TREE;
    pub use dwow_identity_contract::IDENTITY_CONTRACT_NULLIFIERS_TREE;
    pub use dwow_identity_contract::IDENTITY_CONTRACT_ISSUERS_TREE;
    pub use dwow_identity_contract::IDENTITY_CONTRACT_INFO_TREE;
}

pub struct IdentityContract;
impl Contract for IdentityContract {
    fn contract_id(&self) -> ContractId { *IDENTITY_CONTRACT_ID.get().unwrap() }
    fn name(&self) -> &'static str { "Identity" }
    fn dependencies(&self) -> Vec<ContractId> { vec![] }
    fn is_initialized(&self) -> bool { IDENTITY_CONTRACT_ID.get().is_some() }
}

// ============================================================================
// INSURANCE MARKET MODULE
// ============================================================================

pub mod insurance_market {
    pub use dwow_insurance_market_contract::InsuranceMarketFunction;
    pub use dwow_insurance_market_contract::INSURANCE_CONTRACT_RISK_TYPES_TREE;
    pub use dwow_insurance_market_contract::INSURANCE_CONTRACT_MARKETS_TREE;
    pub use dwow_insurance_market_contract::INSURANCE_CONTRACT_UNDERWRITERS_TREE;
    pub use dwow_insurance_market_contract::INSURANCE_CONTRACT_COVERAGES_TREE;
    pub use dwow_insurance_market_contract::INSURANCE_CONTRACT_CLAIMS_TREE;
}

pub struct InsuranceMarketContract;
impl Contract for InsuranceMarketContract {
    fn contract_id(&self) -> ContractId { *INSURANCE_MARKET_CONTRACT_ID.get().unwrap() }
    fn name(&self) -> &'static str { "InsuranceMarket" }
    fn dependencies(&self) -> Vec<ContractId> { vec![*PROMISSORY_NOTE_CONTRACT_ID.get().unwrap()] }
    fn is_initialized(&self) -> bool { INSURANCE_MARKET_CONTRACT_ID.get().is_some() }
}

// ============================================================================
// LABOR MARKET MODULE
// ============================================================================

pub mod labor_market {
    pub use dwow_labor_market_contract::LaborMarketFunction;
    pub use dwow_labor_market_contract::LABOR_CONTRACT_JOBS_TREE;
    pub use dwow_labor_market_contract::LABOR_CONTRACT_NULLIFIERS_TREE;
    pub use dwow_labor_market_contract::LABOR_CONTRACT_INFO_TREE;
}

pub struct LaborMarketContract;
impl Contract for LaborMarketContract {
    fn contract_id(&self) -> ContractId { *LABOR_MARKET_CONTRACT_ID.get().unwrap() }
    fn name(&self) -> &'static str { "LaborMarket" }
    fn dependencies(&self) -> Vec<ContractId> { vec![*PROMISSORY_NOTE_CONTRACT_ID.get().unwrap()] }
    fn is_initialized(&self) -> bool { LABOR_MARKET_CONTRACT_ID.get().is_some() }
}

// ============================================================================
// LOTTERY MODULE
// ============================================================================

pub mod lottery {
    pub use dwow_lottery_contract::LotteryFunction;
    pub use dwow_lottery_contract::LOTTERY_CONTRACT_LOTTERIES_TREE;
    pub use dwow_lottery_contract::LOTTERY_CONTRACT_TICKETS_TREE;
    pub use dwow_lottery_contract::LOTTERY_CONTRACT_NULLIFIERS_TREE;
    pub use dwow_lottery_contract::LOTTERY_CONTRACT_CLAIMS_TREE;
    pub use dwow_lottery_contract::LOTTERY_CONTRACT_ZKAS_COMMIT_NS;
    pub use dwow_lottery_contract::LOTTERY_CONTRACT_ZKAS_REVEAL_NS;
}

pub struct LotteryContract;
impl Contract for LotteryContract {
    fn contract_id(&self) -> ContractId { *LOTTERY_CONTRACT_ID.get().unwrap() }
    fn name(&self) -> &'static str { "Lottery" }
    fn dependencies(&self) -> Vec<ContractId> { vec![] }
    fn is_initialized(&self) -> bool { LOTTERY_CONTRACT_ID.get().is_some() }
}

// ============================================================================
// ORACLE MODULE
// ============================================================================

pub mod oracle {
    pub use dwow_oracle_contract::OracleFunction;
    pub use dwow_oracle_contract::ORACLE_CONTRACT_ORACLES_TREE;
    pub use dwow_oracle_contract::ORACLE_CONTRACT_ATTESTATIONS_TREE;
    pub use dwow_oracle_contract::ORACLE_CONTRACT_INFO_TREE;
}

pub struct OracleContract;
impl Contract for OracleContract {
    fn contract_id(&self) -> ContractId { *ORACLE_CONTRACT_ID.get().unwrap() }
    fn name(&self) -> &'static str { "Oracle" }
    fn dependencies(&self) -> Vec<ContractId> { vec![] }
    fn is_initialized(&self) -> bool { ORACLE_CONTRACT_ID.get().is_some() }
}

// ============================================================================
// OTC SWAP MODULE
// ============================================================================

pub mod otc_swap {
    pub use dwow_otc_swap_contract::OtcSwapFunction;
    pub use dwow_otc_swap_contract::OTC_SWAP_CONTRACT_INFO_TREE;
    pub use dwow_otc_swap_contract::OTC_SWAP_CONTRACT_SWAPS_TREE;
    pub use dwow_otc_swap_contract::OTC_SWAP_CONTRACT_NULLIFIERS_TREE;
    pub use dwow_otc_swap_contract::OTC_SWAP_CONTRACT_ZKAS_CREATE_NS_V1;
    pub use dwow_otc_swap_contract::OTC_SWAP_CONTRACT_ZKAS_FUND_NS_V1;
    pub use dwow_otc_swap_contract::OTC_SWAP_CONTRACT_ZKAS_EXECUTE_NS_V1;
    pub use dwow_otc_swap_contract::OTC_SWAP_CONTRACT_ZKAS_CANCEL_NS_V1;
}

pub struct OtcSwapContract;
impl Contract for OtcSwapContract {
    fn contract_id(&self) -> ContractId { *OTC_SWAP_CONTRACT_ID.get().unwrap() }
    fn name(&self) -> &'static str { "OtcSwap" }
    fn dependencies(&self) -> Vec<ContractId> { vec![] }
    fn is_initialized(&self) -> bool { OTC_SWAP_CONTRACT_ID.get().is_some() }
}

// ============================================================================
// POOL STAKE MODULE
// ============================================================================

pub mod pool_stake {
    pub use dwow_pool_stake_contract::PoolStakeFunction;
    pub use dwow_pool_stake_contract::POOL_STAKE_REGISTRY_TREE;
    pub use dwow_pool_stake_contract::POOL_STAKE_MEMBERS_TREE;
    pub use dwow_pool_stake_contract::POOL_STAKE_ALLOCATIONS_TREE;
    pub use dwow_pool_stake_contract::POOL_STAKE_FEES_TREE;
    pub use dwow_pool_stake_contract::POOL_STAKE_INFO_TREE;
}

pub struct PoolStakeContract;
impl Contract for PoolStakeContract {
    fn contract_id(&self) -> ContractId { *POOL_STAKE_CONTRACT_ID.get().unwrap() }
    fn name(&self) -> &'static str { "PoolStake" }
    fn dependencies(&self) -> Vec<ContractId> { vec![*PROMISSORY_NOTE_CONTRACT_ID.get().unwrap()] }
    fn is_initialized(&self) -> bool { POOL_STAKE_CONTRACT_ID.get().is_some() }
}

// ============================================================================
// RELAYER ENDOWMENT MODULE
// ============================================================================

pub mod relayer_endowment {
    pub use dwow_relayer_endowment_contract::RelayerEndowmentFunction;
    pub use dwow_relayer_endowment_contract::RELAYER_ENDOWMENT_REGISTRY_TREE;
    pub use dwow_relayer_endowment_contract::RELAYER_ENDOWMENT_DEPLOYMENTS_TREE;
    pub use dwow_relayer_endowment_contract::RELAYER_ENDOWMENT_FEES_TREE;
    pub use dwow_relayer_endowment_contract::RELAYER_ENDOWMENT_INFO_TREE;
}

pub struct RelayerEndowmentContract;
impl Contract for RelayerEndowmentContract {
    fn contract_id(&self) -> ContractId { *RELAYER_ENDOWMENT_CONTRACT_ID.get().unwrap() }
    fn name(&self) -> &'static str { "RelayerEndowment" }
    fn dependencies(&self) -> Vec<ContractId> { vec![*PROMISSORY_NOTE_CONTRACT_ID.get().unwrap()] }
    fn is_initialized(&self) -> bool { RELAYER_ENDOWMENT_CONTRACT_ID.get().is_some() }
}

// ============================================================================
// ROULETTE MODULE
// ============================================================================

pub mod roulette {
    pub use dwow_roulette_contract::RouletteFunction;
    pub use dwow_roulette_contract::ROULETTE_CONTRACT_TABLES_TREE;
    pub use dwow_roulette_contract::ROULETTE_CONTRACT_BETS_TREE;
    pub use dwow_roulette_contract::ROULETTE_CONTRACT_NULLIFIERS_TREE;
    pub use dwow_roulette_contract::ROULETTE_CONTRACT_ZKAS_PLACE_BET_NS_V1;
    pub use dwow_roulette_contract::ROULETTE_CONTRACT_ZKAS_SETTLE_BET_NS_V1;
}

pub struct RouletteContract;
impl Contract for RouletteContract {
    fn contract_id(&self) -> ContractId { *ROULETTE_CONTRACT_ID.get().unwrap() }
    fn name(&self) -> &'static str { "Roulette" }
    fn dependencies(&self) -> Vec<ContractId> { vec![] }
    fn is_initialized(&self) -> bool { ROULETTE_CONTRACT_ID.get().is_some() }
}

// ============================================================================
// SLOT MODULE
// ============================================================================

pub mod slot {
    pub use dwow_slot_contract::SlotFunction;
    pub use dwow_slot_contract::SLOT_CONTRACT_SPINS_TREE;
    pub use dwow_slot_contract::SLOT_CONTRACT_NULLIFIERS_TREE;
    pub use dwow_slot_contract::SLOT_CONTRACT_CONFIG_TREE;
    pub use dwow_slot_contract::SLOT_CONTRACT_HOUSE_TREE;
    pub use dwow_slot_contract::SLOT_CONTRACT_ZKAS_COMMIT_NS;
    pub use dwow_slot_contract::SLOT_CONTRACT_ZKAS_SETTLE_NS;
}

pub struct SlotContract;
impl Contract for SlotContract {
    fn contract_id(&self) -> ContractId { *SLOT_CONTRACT_ID.get().unwrap() }
    fn name(&self) -> &'static str { "Slot" }
    fn dependencies(&self) -> Vec<ContractId> { vec![] }
    fn is_initialized(&self) -> bool { SLOT_CONTRACT_ID.get().is_some() }
}

// ============================================================================
// SUBSCRIPTION MODULE
// ============================================================================

pub mod subscription {
    pub use dwow_subscription_contract::SubscriptionFunction;
    pub use dwow_subscription_contract::SUBSCRIPTION_CONTRACT_INFO_TREE;
    pub use dwow_subscription_contract::SUBSCRIPTION_CONTRACT_SUBSCRIPTIONS_TREE;
    pub use dwow_subscription_contract::SUBSCRIPTION_CONTRACT_NULLIFIERS_TREE;
    pub use dwow_subscription_contract::SUBSCRIPTION_CONTRACT_PLANS_TREE;
    pub use dwow_subscription_contract::SUBSCRIPTION_CONTRACT_ZKAS_SUBSCRIBE_NS_V1;
    pub use dwow_subscription_contract::SUBSCRIPTION_CONTRACT_ZKAS_VERIFY_NS_V1;
}

pub struct SubscriptionContract;
impl Contract for SubscriptionContract {
    fn contract_id(&self) -> ContractId { *SUBSCRIPTION_CONTRACT_ID.get().unwrap() }
    fn name(&self) -> &'static str { "Subscription" }
    fn dependencies(&self) -> Vec<ContractId> { vec![] }
    fn is_initialized(&self) -> bool { SUBSCRIPTION_CONTRACT_ID.get().is_some() }
}

// ============================================================================
// TENDER MODULE
// ============================================================================

pub mod tender {
    pub use dwow_tender_contract::TenderFunction;
    pub use dwow_tender_contract::TENDER_CONTRACT_TENDERS_TREE;
    pub use dwow_tender_contract::TENDER_CONTRACT_BIDS_TREE;
    pub use dwow_tender_contract::TENDER_CONTRACT_NULLIFIERS_TREE;
    pub use dwow_tender_contract::TENDER_CONTRACT_INFO_TREE;
    pub use dwow_tender_contract::TENDER_CONTRACT_ZKAS_CREATE_NS_V1;
    pub use dwow_tender_contract::TENDER_CONTRACT_ZKAS_SUBMIT_BID_NS_V1;
    pub use dwow_tender_contract::TENDER_CONTRACT_ZKAS_REVEAL_BID_NS_V1;
    pub use dwow_tender_contract::TENDER_CONTRACT_ZKAS_SELECT_WINNER_NS_V1;
}

pub struct TenderContract;
impl Contract for TenderContract {
    fn contract_id(&self) -> ContractId { *TENDER_CONTRACT_ID.get().unwrap() }
    fn name(&self) -> &'static str { "Tender" }
    fn dependencies(&self) -> Vec<ContractId> { vec![] }
    fn is_initialized(&self) -> bool { TENDER_CONTRACT_ID.get().is_some() }
}

// ============================================================================
// Contract Client Registry — generic dispatch for contract invoke
// ============================================================================

use dwow_sdk::contract_client::ContractClientRegistry;
use std::sync::OnceLock;

static CLIENT_REGISTRY: OnceLock<ContractClientRegistry> = OnceLock::new();

pub fn get_client_registry() -> &'static ContractClientRegistry {
    CLIENT_REGISTRY.get_or_init(|| {
        let mut registry = ContractClientRegistry::new();
        // Each contract crate registers its client here.
        // Example:
        // registry.register("escrow", Box::new(
        //     dwow_escrow_contract::client::EscrowClient));
        registry
    })
}

