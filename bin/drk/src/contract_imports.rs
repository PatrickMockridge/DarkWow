/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 0-2026 Dyne.org foundation
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
//! - `money` module: Money V3 for DeFi tokens (ERC-20 style)
//! - `native_token` module: DARK token for fees and native operations
//! - `dao_escrow` module: DAO with treasury and endowment management
//!
//! ## Contract Registry
//!
//! The wallet uses a [`Contract`] trait-based registry for dependency resolution.
//! See [`crate::contract_registry`] for the generic registry system.

use darkfi_sdk::pasta::pallas;
use darkfi_sdk::crypto::ContractId;

// ============================================================================
// CONTRACT IDs
// ============================================================================

pub use darkfi_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID;

// Money V3 Contract ID - user deployed, no hardcoded ID
// Use OnceLock to allow runtime registration
pub static MONEY_V3_CONTRACT_ID: std::sync::OnceLock<darkfi_sdk::crypto::ContractId> =
    std::sync::OnceLock::new();

pub const DEPLOYOOOR_CONTRACT_ID: &str = "deployooor";

// DAO-Escrow Contract ID - user deployed, no hardcoded ID
pub static DAO_ESCROW_CONTRACT_ID: std::sync::OnceLock<darkfi_sdk::crypto::ContractId> =
    std::sync::OnceLock::new();

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
pub enum MoneyV3Opcodes {
    TokenMintV1 = 0x00,
    AuthTokenMintV1 = 0x01,
    MintV1 = 0x02,
    BurnV1 = 0x03,
    TransferV1 = 0x04,
}

impl From<MoneyV3Opcodes> for u8 {
    fn from(v: MoneyV3Opcodes) -> u8 { v as u8 }
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

pub const DARK_TOKEN_ID: pallas::Base = pallas::Base::zero();
pub const DARK_TOKEN_ID_BYTES: [u8; 32] = [0u8; 32];

// ============================================================================
// MONEY MODULE (Money V3 - DeFi tokens / ERC-20 style)
// ============================================================================

pub mod money {
    pub use darkfi_money_v3_contract::MoneyV3Function;

    pub use darkfi_money_v3_contract::MONEY_V3_CONTRACT_ZKAS_TOKEN_MINT_NS_V1;
    pub use darkfi_money_v3_contract::MONEY_V3_CONTRACT_ZKAS_AUTH_TOKEN_MINT_NS_V1;
    pub use darkfi_money_v3_contract::MONEY_V3_CONTRACT_ZKAS_MINT_NS_V1;
    pub use darkfi_money_v3_contract::MONEY_V3_CONTRACT_ZKAS_BURN_NS_V1;

    // ZK Circuit binaries
    pub use darkfi_money_v3_contract::MONEY_V3_CONTRACT_ZKAS_TOKEN_MINT_V1_BIN;
    pub use darkfi_money_v3_contract::MONEY_V3_CONTRACT_ZKAS_AUTH_TOKEN_MINT_V1_BIN;
    pub use darkfi_money_v3_contract::MONEY_V3_CONTRACT_ZKAS_MINT_V1_BIN;
    pub use darkfi_money_v3_contract::MONEY_V3_CONTRACT_ZKAS_BURN_V1_BIN;

    pub use darkfi_money_v3_contract::MONEY_V3_CONTRACT_COINS_TREE;
    pub use darkfi_money_v3_contract::MONEY_V3_CONTRACT_NULLIFIERS_TREE;
    pub use darkfi_money_v3_contract::MONEY_V3_CONTRACT_MERKLE_TREE;
    pub use darkfi_money_v3_contract::MONEY_V3_CONTRACT_INFO_TREE;
    pub use darkfi_money_v3_contract::MONEY_V3_CONTRACT_FEES_TREE;

    // Client types
    pub use darkfi_money_v3_contract::client::MoneyV3Note;
    pub use darkfi_money_v3_contract::client::transfer_v1::{
        TransferCallBuilder, TransferCallDebris, TransferCallInput, TransferCallOutput,
    };
    pub use darkfi_money_v3_contract::client::token_mint_v1::{TokenMintCallBuilder, TokenMintCallInput};
    pub use darkfi_money_v3_contract::client::auth_token_mint_v1::{AuthTokenMintCallBuilder, AuthTokenMintCallInput};
    pub use darkfi_money_v3_contract::client::mint_v1::{MintCallBuilder, MintCallInput};
    pub use darkfi_money_v3_contract::client::burn_v1::BurnCallBuilder;

    // Model types
    pub use darkfi_money_v3_contract::model::{
        Coin, CoinAttributes, Input as MoneyV3Input, Output as MoneyV3Output,
        TokenMintParamsV1, AuthTokenMintParamsV1, MintParamsV1, BurnParamsV1, TransferParamsV1,
    };

    pub type TokenId = darkfi_sdk::pasta::pallas::Base;

    /// Balance decimal places
    pub const BALANCE_BASE10_DECIMALS: usize = 8;

    // SLED database tree names
    pub const SLED_MERKLE_TREES_MONEY: &str = "money_merkle_trees";
    pub const SLED_MONEY_SMT_TREE: &str = "money_smt_tree";

    // Token management constants
    pub const MONEY_TOKENS_TABLE: &str = "tokens";
    pub const MONEY_TOKENS_COL_TOKEN_ID: &str = "token_id";
    pub const MONEY_TOKENS_COL_MINT_AUTHORITY: &str = "mint_authority";
    pub const MONEY_TOKENS_COL_TOKEN_BLIND: &str = "token_blind";
    pub const MONEY_TOKENS_COL_IS_FROZEN: &str = "is_frozen";
    pub const MONEY_TOKENS_COL_FREEZE_HEIGHT: &str = "freeze_height";
}

// ============================================================================
// NATIVE TOKEN MODULE (DARK token - fees and native operations)
// ============================================================================

pub mod native_token {
    pub use darkfi_native_token_contract::NativeTokenFunction;

    pub use darkfi_native_token_contract::NATIVE_TOKEN_CONTRACT_ZKAS_BURN_NS_V1;
    pub use darkfi_native_token_contract::NATIVE_TOKEN_CONTRACT_ZKAS_FEE_NS_V1;
    pub use darkfi_native_token_contract::NATIVE_TOKEN_CONTRACT_ZKAS_MINT_NS_V1;

    // ZK Circuit binaries
    pub use darkfi_native_token_contract::NATIVE_TOKEN_CONTRACT_ZKAS_BURN_V1_BIN;
    pub use darkfi_native_token_contract::NATIVE_TOKEN_CONTRACT_ZKAS_FEE_V1_BIN;
    pub use darkfi_native_token_contract::NATIVE_TOKEN_CONTRACT_ZKAS_MINT_V1_BIN;

    pub use darkfi_native_token_contract::client::pow_reward_v1::PoWRewardCallBuilder;
    pub use darkfi_native_token_contract::client::burn_v1::{BurnCallBuilder, BurnCallDebris, BurnCallInput};
    pub use darkfi_native_token_contract::client::fee_v1::{FeeCallBuilder, FeeCallInput, FeeCallOutput, FeeRevealed, FeeCallDebris as FeeDebris};
    pub use darkfi_native_token_contract::client::NativeNote;

    pub use darkfi_native_token_contract::model::{
        Coin as NativeCoin, CoinAttributes as NativeCoinAttributes,
        Input as NativeInput, Output as NativeOutput,
        FeeParamsV1, BurnParamsV1, TransferParamsV1 as NativeTransferParamsV1,
        DARK_TOKEN_ID,
    };

    pub use darkfi_native_token_contract::NATIVE_TOKEN_CONTRACT_COINS_TREE;
    pub use darkfi_native_token_contract::NATIVE_TOKEN_CONTRACT_NULLIFIERS_TREE;
    pub use darkfi_native_token_contract::NATIVE_TOKEN_CONTRACT_MERKLE_TREE;
    pub use darkfi_native_token_contract::NATIVE_TOKEN_CONTRACT_INFO_TREE;
    pub use darkfi_native_token_contract::NATIVE_TOKEN_CONTRACT_FEES_TREE;
}

// ============================================================================
// DAO ESCROW MODULE (dao_escrow contract)
// ============================================================================

pub mod dao_escrow {
    // Opcodes
    pub use darkfi_dao_escrow_contract::DaoEscrowFunction;

    // ZK namespaces
    pub use darkfi_dao_escrow_contract::DAO_ESCROW_ZKAS_INIT_NS;
    pub use darkfi_dao_escrow_contract::DAO_ESCROW_ZKAS_PREMIUM_NS;

    // Database tree names
    pub use darkfi_dao_escrow_contract::DAO_ESCROW_CONTRACT_INFO_TREE;
    pub use darkfi_dao_escrow_contract::DAO_ESCROW_CONTRACT_BULLAS_TREE;
    pub use darkfi_dao_escrow_contract::DAO_ESCROW_CONTRACT_MEMBERSHIP_TREE;
    pub use darkfi_dao_escrow_contract::DAO_ESCROW_CONTRACT_ENDOWMENT_TREE;

    // Mode constants
    pub use darkfi_dao_escrow_contract::modes::MODE_ESCROW;
    pub use darkfi_dao_escrow_contract::modes::MODE_TREASURY;
    pub use darkfi_dao_escrow_contract::modes::MODE_TREASURY_ENDOWMENT;

    // Model types
    #[cfg(feature = "client")]
    pub use darkfi_dao_escrow_contract::client::{
        init_v1::*, pay_premium_v1::*,
    };
    pub use darkfi_dao_escrow_contract::model::{
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
    pub use darkfi_deployooor_contract::DeployFunction;
    pub use darkfi_deployooor_contract::client::deploy_v1::DeployCallBuilder;
    pub use darkfi_deployooor_contract::client::lock_v1::LockCallBuilder;
    pub use darkfi_deployooor_contract::model::*;
    pub use darkfi_deployooor_contract::DEPLOY_CONTRACT_INFO_TREE;
    pub use darkfi_deployooor_contract::DEPLOY_CONTRACT_LOCK_TREE;
}

// ============================================================================
// STABLECOIN MODULE (CDP stablecoin - collateralized debt position)
// ============================================================================

pub mod stablecoin {
    pub use darkfi_stablecoin_contract::StablecoinFunction;

    // ZK namespaces
    pub use darkfi_stablecoin_contract::STABLECOIN_CONTRACT_ZKAS_OPEN_NS_V1;
    pub use darkfi_stablecoin_contract::STABLECOIN_CONTRACT_ZKAS_ADD_COLLATERAL_NS_V1;
    pub use darkfi_stablecoin_contract::STABLECOIN_CONTRACT_ZKAS_REMOVE_COLLATERAL_NS_V1;
    pub use darkfi_stablecoin_contract::STABLECOIN_CONTRACT_ZKAS_MINT_STABLE_NS_V1;
    pub use darkfi_stablecoin_contract::STABLECOIN_CONTRACT_ZKAS_REPAY_STABLE_NS_V1;
    pub use darkfi_stablecoin_contract::STABLECOIN_CONTRACT_ZKAS_LIQUIDATE_NS_V1;

    // Database tree names
    pub use darkfi_stablecoin_contract::STABLECOIN_CONTRACT_INFO_TREE;
    pub use darkfi_stablecoin_contract::STABLECOIN_CONTRACT_POSITIONS_TREE;
    pub use darkfi_stablecoin_contract::STABLECOIN_CONTRACT_POSITION_NULLIFIERS_TREE;
    pub use darkfi_stablecoin_contract::STABLECOIN_CONTRACT_STABLECOIN_TREE;
    pub use darkfi_stablecoin_contract::STABLECOIN_CONTRACT_COLLATERAL_TREE;
    pub use darkfi_stablecoin_contract::STABLECOIN_CONTRACT_LIQUIDATIONS_TREE;

    // Client types
    #[cfg(feature = "client")]
    pub use darkfi_stablecoin_contract::client::*;

    // Model types
    #[cfg(feature = "client")]
    pub use darkfi_stablecoin_contract::model::*;

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
// CONTRACT REGISTRY INTEGRATION
// ============================================================================
// Contract implementations for the generic registry system.
// See [`crate::contract_registry`] for the registry infrastructure.

use crate::contract_registry::Contract;

/// MoneyV3 contract info for registry
pub struct MoneyV3Contract;

impl Contract for MoneyV3Contract {
    fn contract_id(&self) -> ContractId {
        *MONEY_V3_CONTRACT_ID.get().unwrap()
    }

    fn name(&self) -> &'static str {
        "MoneyV3"
    }

    fn dependencies(&self) -> Vec<ContractId> {
        vec![]
    }

    fn is_initialized(&self) -> bool {
        MONEY_V3_CONTRACT_ID.get().is_some()
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
        // DaoEscrow uses money_v3::transfer_v1 for endowment withdrawals
        vec![*MONEY_V3_CONTRACT_ID.get().unwrap()]
    }

    fn is_initialized(&self) -> bool {
        DAO_ESCROW_CONTRACT_ID.get().is_some()
    }
}

/// Stablecoin contract info for registry
pub struct StablecoinContract;

impl Contract for StablecoinContract {
    fn contract_id(&self) -> ContractId {
        // Stablecoin contract ID is runtime-determined
        // This will be set when the contract is deployed
        *MONEY_V3_CONTRACT_ID.get().unwrap() // Placeholder
    }

    fn name(&self) -> &'static str {
        "Stablecoin"
    }

    fn dependencies(&self) -> Vec<ContractId> {
        // Stablecoin uses money_v3::transfer_v1 for collateral transfers
        vec![*MONEY_V3_CONTRACT_ID.get().unwrap()]
    }

    fn is_initialized(&self) -> bool {
        MONEY_V3_CONTRACT_ID.get().is_some()
    }
}