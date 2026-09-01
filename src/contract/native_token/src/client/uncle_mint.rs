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

//! UncleMintV1 Client API
//!
//! Spec: uncle_merkle.md §Uncle Minting & Maturity — "Per-uncle note mint".
//! Mints one spendable note per accepted uncle, carved out of the coinbase's
//! full base reward. Reuses the transfer-v1 mint path (`create_transfer_mint_proof`
//! with `old_cumulative_value = 0`) so the cumulative supply chain is NOT touched.

use dwow_core::{
    zk::{Proof, ProvingKey},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    blockchain::BlockHeight,
    crypto::{
        note::AeadEncryptedNote, pasta_prelude::*, poseidon_hash,
        BaseBlind, Blind, FuncId, PublicKey, ScalarBlind, SecretKey,
    },
    pasta::pallas,
};
use tracing::debug;

use super::{transfer::proof::create_transfer_mint_proof, NativeToken};
use crate::model::{ClearInput, CommitmentAttributes, DRKW_ASSET_ID, Nullifier, Output, UncleMintParamsV1};

/// Debris produced by building an UncleMintV1 call.
pub struct UncleMintCallDebris {
    pub params: UncleMintParamsV1,
    pub proofs: Vec<Proof>,
}

/// Build an UncleMintV1 call — one spendable note for one accepted uncle.
///
/// `uncle_hash` is the blake3 hash of the uncle's mining blob (32 bytes); `height`
/// is the canonical block height; `uncle_miner` is the uncle miner's public key
/// (`uncle.header.miner`), which the note is AEAD-encrypted to.
///
/// All keys and blinds are derived deterministically from the uncle hash and
/// height so the uncle miner's wallet can independently reconstruct + decrypt
/// the note (same pure-function derivation as the coinbase, §2.7 "no random keys").
#[allow(clippy::too_many_arguments)]
pub fn build_uncle_mint(
    value: u64,
    uncle_miner: PublicKey,
    uncle_hash: [u8; 32],
    height: BlockHeight,
    mint_zkbin: &ZkBinary,
    mint_pk: &ProvingKey,
    tx_commitment: pallas::Base,
    tx_nonce: pallas::Base,
) -> Result<UncleMintCallDebris> {
    let asset_id = DRKW_ASSET_ID.inner();
    let h_base = pallas::Base::from(height.get());
    let uncle_hash_base = Option::<pallas::Base>::from(pallas::Base::from_repr(uncle_hash))
        .unwrap_or(pallas::Base::ZERO);

    // Deterministic per-uncle spend_secret (domain-separated from coinbase/fee).
    const DOMAIN_SPEND_SECRET: u64 = 20;
    const DOMAIN_EPHEMERAL: u64 = 21;
    const DOMAIN_VALUE_BLIND: u64 = 22;
    const DOMAIN_TOKEN_BLIND: u64 = 23;
    const DOMAIN_COMMITMENT_BLIND: u64 = 24;

    let spend_secret = SecretKey::from_base(poseidon_hash([
        uncle_hash_base,
        h_base,
        pallas::Base::from(DOMAIN_SPEND_SECRET),
    ]));
    let ephemeral_secret = SecretKey::from_base(poseidon_hash([
        *spend_secret.inner(),
        h_base,
        pallas::Base::from(DOMAIN_EPHEMERAL),
    ]));

    let sk_base = *spend_secret.inner();
    let value_blind: ScalarBlind = Blind(
        Option::<pallas::Scalar>::from(pallas::Scalar::from_repr(
            poseidon_hash([sk_base, h_base, pallas::Base::from(DOMAIN_VALUE_BLIND)]).to_repr(),
        ))
        .ok_or_else(|| dwow_core::Error::Custom("Invalid scalar value_blind".into()))?,
    );
    let token_blind: BaseBlind = Blind(poseidon_hash([
        sk_base, h_base, pallas::Base::from(DOMAIN_TOKEN_BLIND),
    ]));
    let commitment_blind: BaseBlind = Blind(poseidon_hash([
        sk_base, h_base, pallas::Base::from(DOMAIN_COMMITMENT_BLIND),
    ]));

    let c_input = ClearInput {
        value,
        asset_id,
        value_blind: value_blind.clone(),
        token_blind: token_blind.clone(),
        signature_public: PublicKey::from_secret(spend_secret.clone()),
    };

    let spend_hook = pallas::Base::ZERO;
    let user_data = pallas::Base::ZERO;

    let output = CommitmentAttributes {
        version: 0,
        public_key: uncle_miner.clone(),
        value,
        asset_id: dwow_sdk::crypto::AssetId::from_base(asset_id),
        spend_hook: FuncId::from_base(spend_hook),
        user_data,
        blind: commitment_blind.clone(),
    };

    let (proof, public_inputs) = create_transfer_mint_proof(
        mint_zkbin,
        mint_pk,
        &output,
        value, // effective_value == value (no further split on the uncle note)
        spend_secret.clone(),
        value_blind.clone(),
        token_blind.clone(),
        spend_hook,
        user_data,
        commitment_blind.clone(),
        0,                    // old_cumulative_value (identity — no supply bump)
        pallas::Scalar::zero(), // old_cumulative_blind
        tx_commitment,
        tx_nonce,
    )?;

    debug!(target: "contract::native_token::client::uncle_mint", "Minted uncle note: value={value}");

    let note = NativeToken {
        value,
        asset_id: output.asset_id.inner(),
        spend_hook,
        user_data,
        commitment_blind: commitment_blind.clone().inner(),
        spend_secret: *spend_secret.inner(),
        value_blind: value_blind.clone().inner(),
        token_blind: token_blind.clone().inner(),
        memo: vec![],
    };
    let encrypted_note = AeadEncryptedNote::encrypt_deterministic(&note, &uncle_miner, ephemeral_secret)?;

    let nf = Nullifier::new(spend_secret.clone(), public_inputs.commitment.inner());

    let c_output = Output {
        value_commit: public_inputs.value_commit,
        token_commit: public_inputs.token_commit,
        commitment: public_inputs.commitment,
        nullifier: nf,
        note: encrypted_note,
    };

    let params = UncleMintParamsV1 {
        input: c_input,
        effective_value: value,
        output: c_output,
        nullifier: nf,
        tx_binding: public_inputs.tx_binding,
        tx_nonce: public_inputs.tx_nonce,
    };
    Ok(UncleMintCallDebris { params, proofs: vec![proof] })
}
