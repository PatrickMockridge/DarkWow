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

//! NativeToken Client API
//!
//! This module implements the client-side API for NativeToken contract interaction.

use dwow_sdk::pasta::pallas;
use dwow_serial::{SerialDecodable, SerialEncodable};

/// ZK circuit binary constants
pub mod zkbins;

/// `NativeToken::BurnV1` API
pub mod burn_v1;

/// `NativeToken::FeeV1` API
pub mod fee_v1;

/// `NativeToken::FeeCollectV1` API
pub mod fee_collect_v1;

/// `NativeToken::PoWRewardV1` API
pub mod pow_reward_v1;

/// `NativeToken::TransferV1` API
pub mod transfer_v1;

use dwow_sdk::contract_client::{ContractClient, WalletStateProvider};

/// NativeToken contract client — implements ContractClient for the wallet's
/// generic dispatch. Lives in the contract crate, NOT the wallet.
///
/// Native Token is the sole special citizen — it is the consensus asset
/// required for fee payment. Its ContractClient is registered like every
/// other contract; the wallet accesses it through the same generic path.
pub struct NativeTokenClient;

impl ContractClient for NativeTokenClient {
    fn contract_name(&self) -> &'static str { "native_token" }

    fn function_selector(&self, function: &str) -> Option<u8> {
        match function {
            "FeeV1" => Some(0x00),
            "BurnV1" => Some(0x02),
            "PoWRewardV1" => Some(0x05),
            _ => None,
        }
    }

    fn supported_functions(&self) -> Vec<&'static str> {
        vec!["FeeV1", "PoWRewardV1", "BurnV1"]
    }

    fn build(&self, function: &str, _params: &str, _wallet_state: &dyn WalletStateProvider) -> std::result::Result<(Vec<u8>, Vec<Vec<u8>>), String> {
        match function {
            "FeeV1" | "PoWRewardV1" | "BurnV1" => Ok((vec![], vec![])),
            // Native transfers NEVER go through ContractClient dispatch —
            // wallet.md §6.4: the wallet's bespoke path (build_native_transfer
            // → TransferCallBuilder) constructs them with real burn/mint
            // proofs, fee attach, published nullifiers, and per-call signing.
            // A string-dispatch stub here can only emit an invalid transaction.
            "TransferV1" => Err(
                "NativeToken TransferV1 is the bespoke write path (wallet.md §6.4) — \
                 use the wallet's native transfer, not contract dispatch".to_string(),
            ),
            _ => Err(format!("NativeToken: unsupported function '{}'", function)),
        }
    }
}

/// NativeToken holds the inner attributes of a Coin.
///
/// It does not store the public key since it's encrypted for that key,
/// and so is not needed to infer the coin attributes.
/// All other coin attributes must be present.
#[derive(Debug, Clone, Eq, PartialEq, SerialEncodable, SerialDecodable)]
pub struct NativeToken {
    /// Value of the coin
    pub value: u64,
    /// Token ID of the coin
    pub token_id: pallas::Base,
    /// Spend hook used for protocol-owned liquidity.
    pub spend_hook: pallas::Base,
    /// User data used by protocol when spend hook is enabled
    pub user_data: pallas::Base,
    /// Blinding factor for the coin
    pub coin_blind: pallas::Base,
    /// Blinding factor for the value pedersen commitment
    pub value_blind: pallas::Scalar,
    /// Blinding factor for the token ID pedersen commitment
    pub token_blind: pallas::Base,
    /// Attached memo (arbitrary data)
    pub memo: Vec<u8>,
}