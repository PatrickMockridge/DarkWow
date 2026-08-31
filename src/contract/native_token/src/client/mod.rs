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

/// ZK circuit binary constants
pub mod zkbins;

/// `NativeToken::BurnV1` API
pub mod burn;

/// `NativeToken::FeeV2` API
pub mod fee;

/// `NativeToken::FeeCollectV1` API
pub mod fee_collect;

/// `NativeToken::PoWRewardV1` API
pub mod pow_reward;

/// `NativeToken::UncleMintV1` API — per-uncle spendable note mint
pub mod uncle_mint;

/// TransferV1 builder — internal use by pow_reward (not a client-dispatched function)
pub mod transfer;

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
            "FeeV2" => Some(0x08),
            "BurnV1" => Some(0x02),
            "PoWRewardV1" => Some(0x05),
            _ => None,
        }
    }

    fn supported_functions(&self) -> Vec<&'static str> {
        vec!["FeeV2", "PoWRewardV1", "BurnV1"]
    }

    fn build(&self, function: &str, _params: &str, _wallet_state: &dyn WalletStateProvider) -> std::result::Result<(Vec<u8>, Vec<Vec<u8>>), String> {
        match function {
            "FeeV2" | "PoWRewardV1" | "BurnV1" => Ok((vec![], vec![])),
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

/// NativeToken holds the inner attributes of a Commitment.
///
/// It does not store the public key since it's encrypted for that key,
/// and so is not needed to infer the commitment attributes.
/// All other commitment attributes must be present.
///
/// `spend_secret` is the per-output secret the recipient needs to spend the
/// commitment (compute the nullifier `nf = poseidon(1, spend_secret, C)` and satisfy
/// Mint_V2 C2 `commitment_public == from_secret(spend_secret)`). For self-change
/// outputs (FeeV2/FeeCollectV1/PoWRewardV1) it equals the recipient's own
/// secret; for TransferV1/SpendV1 it is a fresh per-output secret the sender
/// generates and hands to the recipient inside this AEAD-encrypted note.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct NativeToken {
    pub value: u64,
    pub asset_id: pallas::Base,
    pub spend_hook: pallas::Base,
    pub user_data: pallas::Base,
    pub commitment_blind: pallas::Base,
    pub spend_secret: pallas::Base,
    pub value_blind: pallas::Scalar,
    pub token_blind: pallas::Base,
    pub memo: Vec<u8>,
}

// Manual Encodable/Decodable bridge impls required by AeadEncryptedNote API.
impl dwow_serial::Encodable for NativeToken {
    fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> {
        let mut len = 0;
        len += dwow_serial::Encodable::encode(&self.value, w)?;
        len += dwow_serial::Encodable::encode(&self.asset_id, w)?;
        len += dwow_serial::Encodable::encode(&self.spend_hook, w)?;
        len += dwow_serial::Encodable::encode(&self.user_data, w)?;
        len += dwow_serial::Encodable::encode(&self.commitment_blind, w)?;
        len += dwow_serial::Encodable::encode(&self.spend_secret, w)?;
        len += dwow_serial::Encodable::encode(&self.value_blind, w)?;
        len += dwow_serial::Encodable::encode(&self.token_blind, w)?;
        len += dwow_serial::Encodable::encode(&self.memo, w)?;
        Ok(len)
    }
}

impl dwow_serial::Decodable for NativeToken {
    fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> {
        Ok(NativeToken {
            value: dwow_serial::Decodable::decode(d)?,
            asset_id: dwow_serial::Decodable::decode(d)?,
            spend_hook: dwow_serial::Decodable::decode(d)?,
            user_data: dwow_serial::Decodable::decode(d)?,
            commitment_blind: dwow_serial::Decodable::decode(d)?,
            spend_secret: dwow_serial::Decodable::decode(d)?,
            value_blind: dwow_serial::Decodable::decode(d)?,
            token_blind: dwow_serial::Decodable::decode(d)?,
            memo: dwow_serial::Decodable::decode(d)?,
        })
    }
}