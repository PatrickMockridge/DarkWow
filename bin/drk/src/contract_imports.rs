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

use darkfi_sdk::pasta::pallas;

// ============================================================================
// CONTRACT IDs
// ============================================================================

pub use darkfi_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID;

// Money V3 Contract ID - user deployed, no hardcoded ID
// Use OnceLock to allow runtime registration
pub static MONEY_V3_CONTRACT_ID: std::sync::OnceLock<darkfi_sdk::crypto::ContractId> =
    std::sync::OnceLock::new();

// For backwards compatibility - Money V3 Contract ID
pub static MONEY_CONTRACT_ID: std::sync::OnceLock<darkfi_sdk::crypto::ContractId> =
    std::sync::OnceLock::new();

pub const DEPLOYOOOR_CONTRACT_ID: &str = "deployooor";

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
// DAO ESCROW MODULE (placeholder - dao_escrow contract has compilation issues)
// ============================================================================

pub mod dao_escrow {
    // DAO-Escrow modes
    pub const MODE_ESCROW: u8 = 0x00;
    pub const MODE_TREASURY: u8 = 0x01;
    pub const MODE_TREASURY_ENDOWMENT: u8 = 0x02;

    // Tree names
    pub const DAO_ESCROW_CONTRACT_INFO_TREE: &str = "info";
    pub const DAO_ESCROW_CONTRACT_BULLAS_TREE: &str = "bullas";
    pub const DAO_ESCROW_CONTRACT_MEMBERSHIP_TREE: &str = "membership";
    pub const DAO_ESCROW_CONTRACT_ENDOWMENT_TREE: &str = "endowment";

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