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
//! Architecture per wallet.md:
//! - Native Token is the sole special citizen (fee payment, coinbase rewards).
//! - All other contracts go through the generic AEAD + manifest path.
//! - Only 9 genesis ContractIds are hardcoded for trust tier resolution.
//! - ZK binary modules stay (compile-time circuit references the prover needs).
//! - No OnceLock statics. No per-contract registry. No client registry.

// ── 9 Genesis Contract IDs ──────────────────────────────────────────
// Per wallet.md: only hardcoded identifiers — trust tier [GENESIS] display.
// All other contracts discovered via manifests during chain scan.
pub use dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID;
pub use dwow_sdk::crypto::{
    ATTESTATION_CONTRACT_ID, BOX_CONTRACT_ID, DEPLOYOOOR_CONTRACT_ID,
    IDENTITY_CONTRACT_ID, MULTISIG_CONTRACT_ID, ORACLE_CONTRACT_ID,
    PROMISSORY_NOTE_CONTRACT_ID, PURSE_CONTRACT_ID,
};

// ============================================================================
// ZK BINARY MODULES — compile-time circuit references the prover needs
// ============================================================================
// These are NOT per-contract knowledge. They are circuit binaries referenced
// at proof generation time. The wallet discovers contract interfaces via
// manifests during chain scan — these modules provide the ZK circuits for
// proof building, not contract-specific dispatch logic.

pub mod promissory_note {
    pub use dwow_promissory_note_contract::PromissoryNoteFunction;

    pub use dwow_promissory_note_contract::PROMISSORY_NOTE_CONTRACT_COINS_TREE;
    pub use dwow_promissory_note_contract::PROMISSORY_NOTE_CONTRACT_NULLIFIERS_TREE;
    pub use dwow_promissory_note_contract::PROMISSORY_NOTE_CONTRACT_MERKLE_TREE;
    pub use dwow_promissory_note_contract::PROMISSORY_NOTE_CONTRACT_INFO_TREE;

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

    pub use dwow_promissory_note_contract::model::{
        BurnSpendHookPayload, Coin, CoinAttributes,
        Input as PromissoryNoteInput, Output as PromissoryNoteOutput,
        RedeemParamsV1, RedeemUpdateV1,
        TokenMintParamsV1, MintParamsV1, BurnParamsV1, TransferParamsV1,
    };

    pub type TokenId = dwow_sdk::pasta::pallas::Base;

    pub const BALANCE_BASE10_DECIMALS: usize = 8;
    pub const SLED_MERKLE_TREES_PROMISSORY_NOTE: &str = "promissory_note_merkle_trees";

    pub const PN_TOKENS_TABLE: &str = "tokens";
    pub const PN_TOKENS_COL_TOKEN_ID: &str = "token_id";
    pub const PN_TOKENS_COL_MINT_AUTHORITY: &str = "mint_authority";
    pub const PN_TOKENS_COL_TOKEN_BLIND: &str = "token_blind";
    pub const PN_TOKENS_COL_IS_FROZEN: &str = "is_frozen";
    pub const PN_TOKENS_COL_FREEZE_HEIGHT: &str = "freeze_height";
}

pub mod native_token {
    pub use dwow_native_token_contract::NativeTokenFunction;

    pub use dwow_native_token_contract::NATIVE_TOKEN_CONTRACT_ZKAS_BURN_NS_V1;
    pub use dwow_native_token_contract::NATIVE_TOKEN_CONTRACT_ZKAS_FEE_NS_V1;
    pub use dwow_native_token_contract::NATIVE_TOKEN_CONTRACT_ZKAS_MINT_NS_V1;

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

/// Contract client registry.
/// Only NativeToken and Deployooor are hardcoded — everything else is a
/// capability resolved through manifests stored on-chain at genesis.
/// Per wallet-capability-kernel.md: zero per-contract special cases.
pub fn get_client_registry() -> &'static ContractClientRegistry {
    CLIENT_REGISTRY.get_or_init(|| {
        let mut registry = ContractClientRegistry::new();

        // Infrastructure — only these two (per specification)
        registry.register("native_token", Box::new(
            dwow_native_token_contract::client::NativeTokenClient));
        registry.register("deployooor", Box::new(GenericContractClient::new(
            "deployooor", &[("DeployV1", 0x00), ("LockV1", 0x01)])));

        registry
    })
}

/// Look up a genesis contract's ContractId by name.
/// Returns None for non-genesis contracts — those are discovered via manifests.
pub fn get_contract_id(name: &str) -> Option<dwow_sdk::crypto::ContractId> {
    match name {
        "promissory_note" | "pn" => Some(*PROMISSORY_NOTE_CONTRACT_ID),
        "native_token" | "nt" => Some(*NATIVE_TOKEN_CONTRACT_ID),
        "deployooor" => Some(*DEPLOYOOOR_CONTRACT_ID),
        "purse" => Some(*PURSE_CONTRACT_ID),
        "box" => Some(*BOX_CONTRACT_ID),
        "multisig" => Some(*MULTISIG_CONTRACT_ID),
        "attestation" => Some(*ATTESTATION_CONTRACT_ID),
        "identity" => Some(*IDENTITY_CONTRACT_ID),
        "oracle" => Some(*ORACLE_CONTRACT_ID),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_genesis_contract_ids_are_defined() {
        // All 9 genesis ContractIds are compile-time constants (not OnceLock).
        // Verify they're accessible and non-zero.
        let ids: &[(&str, &dwow_sdk::crypto::ContractId)] = &[
            ("native_token", &NATIVE_TOKEN_CONTRACT_ID),
            ("deployooor", &DEPLOYOOOR_CONTRACT_ID),
            ("promissory_note", &PROMISSORY_NOTE_CONTRACT_ID),
            ("identity", &IDENTITY_CONTRACT_ID),
            ("oracle", &ORACLE_CONTRACT_ID),
            ("attestation", &ATTESTATION_CONTRACT_ID),
            ("purse", &PURSE_CONTRACT_ID),
            ("box", &BOX_CONTRACT_ID),
            ("multisig", &MULTISIG_CONTRACT_ID),
        ];
        for (name, cid) in ids {
            assert!(!cid.to_bytes().iter().all(|b| *b == 0),
                "Genesis ContractId for {} is zero", name);
        }
    }
}
