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

//! Contract Import Graph for dwow_wallet
//!
//! Architecture per wallet.md:
//! - Native Token is consensus-critical: fee payment, coinbase rewards.
//! - Deployooor is deployment infrastructure: contract deploy/lock detection.
//! - These two are the ONLY hardcoded contracts. Everything else (PN, BB,
//!   escrow, identity, oracle, attestation, purse, box, multisig — all 25+
//!   contracts) goes through the generic AEAD + manifest path.
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

pub mod native_token {
    pub use dwow_native_token_contract::NativeTokenFunction;

    // HAZOP V1/V2 fix: add V2 namespace and binary exports
    pub use dwow_native_token_contract::NATIVE_TOKEN_CONTRACT_ZKAS_BURN_NS_V2;
    pub use dwow_native_token_contract::NATIVE_TOKEN_CONTRACT_ZKAS_FEE_NS_V2;
    pub use dwow_native_token_contract::NATIVE_TOKEN_CONTRACT_ZKAS_MINT_NS_V2;
    pub use dwow_native_token_contract::NATIVE_TOKEN_CONTRACT_ZKAS_FEE_COLLECT_NS_V2;

    pub use dwow_native_token_contract::NATIVE_TOKEN_CONTRACT_ZKAS_BURN_V2_BIN;
    pub use dwow_native_token_contract::NATIVE_TOKEN_CONTRACT_ZKAS_FEE_V2_BIN;
    pub use dwow_native_token_contract::NATIVE_TOKEN_CONTRACT_ZKAS_MINT_V2_BIN;
    pub use dwow_native_token_contract::NATIVE_TOKEN_CONTRACT_ZKAS_FEE_COLLECT_V2_BIN;

    pub use dwow_native_token_contract::client::pow_reward::PoWRewardCallBuilder;
    pub use dwow_native_token_contract::client::burn::{BurnCallBuilder, BurnCallDebris, BurnCallInput};
    pub use dwow_native_token_contract::client::fee::{FeeV2CallBuilder, FeeV2CallInput, FeeV2CallOutput};
    pub use dwow_native_token_contract::client::zkbins::NATIVE_TOKEN_CONTRACT_ZKAS_FEE_THRESHOLD_V1_BIN;
    pub use dwow_native_token_contract::client::transfer::{TransferCallBuilder, TransferCallDebris, TransferCallInput, TransferCallOutput};
    pub use dwow_native_token_contract::client::NativeToken;

    pub use dwow_native_token_contract::model::{
        Coin as NativeCoin, CoinAttributes as NativeCoinAttributes,
        Input as NativeInput, InputWitness, Output as NativeOutput,
        BurnParamsV1, TransferParamsV1 as NativeTransferParamsV1,
        DRKW_TOKEN_ID,
    };

    pub use dwow_native_token_contract::NATIVE_TOKEN_CONTRACT_COINS_TREE;
    pub use dwow_native_token_contract::NATIVE_TOKEN_CONTRACT_NULLIFIERS_TREE;
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
///
/// Only two contracts are hardcoded — everything else is a capability
/// resolved through manifests stored on-chain at genesis:
///
///   NativeToken — consensus-critical. Fee payment and coinbase rewards
///       are consensus operations. The wallet MUST attach fees and scan
///       coinbase to function. Not per-contract business logic.
///
///   Deployooor — deployment infrastructure. The wallet MUST detect
///       DeployV1 transactions to discover new contracts and their
///       manifests. Without this, manifest discovery is impossible.
///       Not per-contract business logic.
///
/// Per wallet-capability-kernel.md: zero per-contract special cases
/// beyond these two infrastructure contracts.
pub fn get_client_registry() -> &'static ContractClientRegistry {
    CLIENT_REGISTRY.get_or_init(|| {
        let mut registry = ContractClientRegistry::new();

        // Infrastructure — NativeToken (consensus) + Deployooor (deployment).
        // Only these two are hardcoded; all other contracts use manifests.
        registry.register("native_token", Box::new(
            dwow_native_token_contract::client::NativeTokenClient));
        registry.register("deployooor", Box::new(GenericContractClient::new(
            "deployooor", &[("DeployV1", 0x00), ("LockV1", 0x01)])));

        // ZK circuit builders removed — the generic prover (wallet.md §6.4.1,
        // Phase 6) builds from the zkas binary + manifest witness_map.
        // No per-contract compiled-in builder is required; the
        // circuit_registry (D2) is deleted.

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
