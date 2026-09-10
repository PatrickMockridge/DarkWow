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

//! NativeToken PoWRewardV1 Client API
//!
//! This module provides the ability to build PoW reward calls for block rewards.

use dwow_core::Result;
use dwow_sdk::{
    blockchain::BlockHeight,
    crypto::{
        note::AeadEncryptedNote, pasta_prelude::*, poseidon_hash,
        BaseBlind, Blind, FuncId, PublicKey, ScalarBlind, SecretKey, AssetId,
    },
    pasta::pallas,
};
use tracing::debug;

use super::{transfer::proof::compute_transfer_mint_revealed, NativeToken};
use crate::model::{ClearInput, CommitmentAttributes, DRKW_ASSET_ID, Nullifier, Output, PoWRewardParamsV1};

/// Debris produced by building a PoWReward call, containing the parameters
/// needed to assemble the plaintext call data (b6bf44f79 — no ZK proof).
pub struct PoWRewardCallDebris {
    /// The contract call parameters
    pub params: PoWRewardParamsV1,
}

/// Builder for creating PoWRewardV1 contract calls.
///
/// This is used to claim block rewards after successfully mining a block.
pub struct PoWRewardCallBuilder {
    /// Caller's secret key for commitment ownership
    pub secret: SecretKey,
    /// Ephemeral signature secret — MUST be fresh per reward claim
    pub ephemeral_signature_secret: SecretKey,
    /// Rewarded block height
    pub block_height: BlockHeight,
    /// Optional recipient's public key, in case we want to mint to a different address
    pub recipient: Option<PublicKey>,
    /// Optional contract spend hook to use in the output (as pallas::Base)
    pub spend_hook: Option<pallas::Base>,
    /// Optional user data to use in the output
    pub user_data: Option<pallas::Base>,
    /// Expected cumulative total supply at this block height (infinity-mint hardening)
    pub expected_cumulative_supply: u64,
    /// TOTAL_SUPPLY from sled before this block
    pub old_total_supply: u64,
    /// Previous cumulative value commitment (S_{H-1})
    pub old_cumulative_commit: pallas::Point,
    /// Previous cumulative blind
    pub old_cumulative_blind: pallas::Scalar,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl PoWRewardCallBuilder {
    /// Build the PoWReward call debris
    fn _build(&self, value: u64, effective_value: u64) -> Result<PoWRewardCallDebris> {
        debug!(target: "contract::native_token::client::pow_reward", "Building NativeToken::PoWRewardV1 contract call");

        // In this call, we will build one clear input and one anonymous output.
        // Only DRKW_ASSET_ID can be minted as PoW reward.
        let asset_id = DRKW_ASSET_ID.inner();

        // Deterministic blinds derived from sk_H + height + domain separator.
        // consensus-coinbase.md §2.7: "MUST use sk_H = derive_instance(...) — no
        // random keys." Extending this to blinds: every value that affects the
        // commitment and transaction hash MUST be deterministic.
        // Per type-system.md §2: commitment_blind, value_blind, token_blind are
        // distinct types (BaseBlind vs ScalarBlind) with distinct derivation
        // domain separators — two types SHALL NOT share derivation paths.
        const DOMAIN_VALUE_BLIND: u64 = 1;
        const DOMAIN_TOKEN_BLIND: u64 = 2;
        const DOMAIN_COMMITMENT_BLIND: u64 = 3;
        let sk_base = *self.secret.inner();
        let h_base = pallas::Base::from(self.block_height.get());
        // value_blind: Blind<pallas::Scalar> (ScalarBlind)
        let value_blind: ScalarBlind = Blind(
            Option::<pallas::Scalar>::from(pallas::Scalar::from_repr(
                poseidon_hash([sk_base, h_base, pallas::Base::from(DOMAIN_VALUE_BLIND)]).to_repr(),
            ))
            .ok_or_else(|| dwow_core::Error::Custom("Invalid scalar value_blind".into()))?,
        );
        // token_blind: Blind<pallas::Base> (BaseBlind)
        let token_blind: BaseBlind = Blind(poseidon_hash([
            sk_base, h_base, pallas::Base::from(DOMAIN_TOKEN_BLIND),
        ]));
        // commitment_blind: Blind<pallas::Base> (BaseBlind)
        let commitment_blind: BaseBlind = Blind(poseidon_hash([
            sk_base, h_base, pallas::Base::from(DOMAIN_COMMITMENT_BLIND),
        ]));
        let c_input = ClearInput {
            value,
            asset_id,
            value_blind: value_blind.clone(),
            token_blind: token_blind.clone(),
            signature_public: PublicKey::from_secret(self.ephemeral_signature_secret.clone()),
        };

        // Grab the spend hook and user data to use in the output
        let spend_hook = self.spend_hook.unwrap_or(pallas::Base::ZERO);
        let user_data = self.user_data.unwrap_or(pallas::Base::ZERO);

        // Building the anonymous output using CommitmentAttributes (TransferCallOutput)
        let output = CommitmentAttributes {
            version: 0,
            public_key: self.recipient.unwrap_or(PublicKey::from_secret(self.secret.clone())),
            value,
            asset_id: AssetId::from_base(asset_id),
            spend_hook: FuncId::from_base(spend_hook),
            user_data,
            blind: commitment_blind.clone(),
        };

        debug!(target: "contract::native_token::client::pow_reward", "Computing plaintext mint revealed values for output");
        // total_pin = value − effective_value = Σ pin (the uncle split). Public.
        let total_pin = value.saturating_sub(effective_value);
        let public_inputs = compute_transfer_mint_revealed(
            &output,
            effective_value,
            total_pin,
            self.secret.clone(),
            value_blind.clone(),
            token_blind.clone(),
            spend_hook,
            user_data,
            commitment_blind.clone(),
            self.old_total_supply, // from sled — actual TOTAL_SUPPLY before this block
            self.old_cumulative_blind,
            self.tx_commitment,
            self.tx_nonce,
        );

        // Spec: uncle_merkle.md §Uncle Minting & Maturity — the AEAD note carries
        // the REDUCED effective value so the wallet sees the canonical miner's actual
        // spendable share (base − Σ pin + fees).
        let note = NativeToken {
            value: effective_value,
            asset_id: output.asset_id.inner(),
            spend_hook,
            user_data,
            commitment_blind: commitment_blind.clone().inner(),
            spend_secret: *self.secret.inner(),
            value_blind: value_blind.clone().inner(),
            token_blind: token_blind.clone().inner(),
            memo: vec![],
        };

        // Deterministic AEAD encryption — uses the same ephemeral secret derived
        // from sk_H (consensus-coinbase.md §2.7: "no random keys"). The wallet
        // decrypts with sk_H via the standard decrypt path — no change needed
        // on the wallet side.
        let encrypted_note = AeadEncryptedNote::encrypt_deterministic(
            &note,
            &output.public_key,
            self.ephemeral_signature_secret.clone(),
        )?;

        let nf = Nullifier::new(self.secret.clone(), public_inputs.commitment.inner());

        let c_output = Output {
            value_commit: public_inputs.value_commit,
            token_commit: public_inputs.token_commit,
            commitment: public_inputs.commitment,
            nullifier: nf,
            note: encrypted_note,
        };

        let params = PoWRewardParamsV1 {
            input: c_input,
            total_pin,
            output: c_output,
            nullifier: nf,
            expected_cumulative_supply: self.expected_cumulative_supply,
            old_cumulative_commit: self.old_cumulative_commit,
            old_cumulative_blind: self.old_cumulative_blind,
            new_cumulative_commit: public_inputs.new_cumulative_commit,
            tx_binding: public_inputs.tx_binding,
            tx_nonce: public_inputs.tx_nonce,
        };
        let debris = PoWRewardCallDebris { params };
        Ok(debris)
    }

    /// Build with a full reward and a REDUCED effective reward (uncle split).
    /// Spec: uncle_merkle.md §Uncle Minting & Maturity — the coinbase mints the full
    /// `reward` into the cumulative supply chain (`value_commit`) while the spendable
    /// note commits to `effective_reward` (`C_effective = base − Σ pin`).
    pub fn build_with_custom_reward_and_effective(
        &self,
        reward: u64,
        effective_reward: u64,
    ) -> Result<PoWRewardCallDebris> {
        // The coinbase is reward-only: fees never enter it (entrypoint/mod.rs:968
        // exact equality). Fees are minted separately by FeeCollectV1 (0x06).
        self._build(reward, effective_reward)
    }
}