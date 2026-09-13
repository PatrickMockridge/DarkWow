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
//! full base reward. Plaintext (no Mint_V2 proof): the note's value is public
//! (`pin_confirmed_i`), so it is built with plaintext Pedersen/Poseidon with
//! `old_cumulative_value = 0` — the cumulative supply chain is NOT touched.

use dwow_core::Result;
use dwow_sdk::{
    blockchain::BlockHeight,
    crypto::{
        constants::{DRK_POSEIDON_DOMAIN_TOKEN_COMMIT, DRK_POSEIDON_DOMAIN_TX_BINDING},
        note::AeadEncryptedNote, pasta_prelude::*, pedersen_commitment_u64, poseidon_hash,
        BaseBlind, Blind, FuncId, PublicKey, ScalarBlind, SecretKey,
    },
    pasta::pallas,
};
use tracing::debug;

use super::NativeToken;
use crate::model::{ClearInput, CommitmentAttributes, DRKW_ASSET_ID, Output, UncleMintParamsV1};

/// Debris produced by building an UncleMintV1 call — parameters only
/// (b6bf44f79: uncle mints are plaintext contract calls, no ZK proof).
pub struct UncleMintCallDebris {
    pub params: UncleMintParamsV1,
}

/// Build an UncleMintV1 call — one spendable note for one accepted uncle.
///
/// `uncle_hash` is the blake3 hash of the uncle's mining blob (32 bytes); `height`
/// is the canonical block height; `uncle_miner` is the uncle miner's public key
/// (`uncle.header.miner`), which the note is AEAD-encrypted to.
///
/// # Spend authority
///
/// The note's commitment is bound to `uncle_miner`, and the burn/spend circuit
/// derives the commitment's public key from the witness `spend_secret`
/// in-circuit (`burn.zk`: `pub = ec_mul_base(spend_secret, NULLIFIER_K)`, then the
/// coin hash is built from `pub`'s coordinates). So spending requires the secret
/// behind `uncle_miner` — which only the uncle miner holds, and which the
/// canonical miner building this call does NOT have.
///
/// Two consequences, both deliberate:
///
/// * This function cannot compute the note's nullifier, so the mint publishes
///   none (`Output::nullifier == None`). The nullifier is revealed at spend time,
///   as it is for every note in this system.
/// * The note payload's `spend_secret` field is a placeholder. The uncle miner's
///   wallet spends this note with its OWN secret; the note is fully
///   reconstructible from the plaintext commitment preimage carried in the call
///   data, so the wallet does not need this field.
///
/// All blinds are derived deterministically from the uncle hash and height, so
/// every node (and an auditor) can recompute them. They are PUBLIC inputs — the
/// pin amount is public anyway, so the note hides nothing.
#[allow(clippy::too_many_arguments)]
pub fn build_uncle_mint(
    value: u64,
    uncle_miner: PublicKey,
    uncle_hash: [u8; 32],
    height: BlockHeight,
    tx_commitment: pallas::Base,
    tx_nonce: pallas::Base,
) -> Result<UncleMintCallDebris> {
    let asset_id = DRKW_ASSET_ID.inner();
    let h_base = pallas::Base::from(height.get());
    let uncle_hash_base = Option::<pallas::Base>::from(pallas::Base::from_repr(uncle_hash))
        .unwrap_or(pallas::Base::ZERO);

    // Deterministic per-uncle blinds (domain-separated from coinbase/fee).
    // Domain 20 was the old `DOMAIN_SPEND_SECRET`; it is gone because the note's
    // spend authority is `uncle_miner`, not a publicly-derived secret.
    const DOMAIN_EPHEMERAL: u64 = 21;
    const DOMAIN_VALUE_BLIND: u64 = 22;
    const DOMAIN_TOKEN_BLIND: u64 = 23;
    const DOMAIN_COMMITMENT_BLIND: u64 = 24;

    let value_blind: ScalarBlind = Blind(
        Option::<pallas::Scalar>::from(pallas::Scalar::from_repr(
            poseidon_hash([uncle_hash_base, h_base, pallas::Base::from(DOMAIN_VALUE_BLIND)])
                .to_repr(),
        ))
        .ok_or_else(|| dwow_core::Error::Custom("Invalid scalar value_blind".into()))?,
    );
    let token_blind: BaseBlind = Blind(poseidon_hash([
        uncle_hash_base, h_base, pallas::Base::from(DOMAIN_TOKEN_BLIND),
    ]));
    let commitment_blind: BaseBlind = Blind(poseidon_hash([
        uncle_hash_base, h_base, pallas::Base::from(DOMAIN_COMMITMENT_BLIND),
    ]));
    let ephemeral_secret = SecretKey::from_base(poseidon_hash([
        uncle_hash_base, h_base, pallas::Base::from(DOMAIN_EPHEMERAL),
    ]));

    let spend_hook = pallas::Base::ZERO;
    let user_data = pallas::Base::ZERO;

    // All three are computable from public data: `uncle_miner` is the header's
    // public reward key, so no secret is needed to build the commitment.
    let value_commit = pedersen_commitment_u64(value, value_blind.clone());
    let token_commit = poseidon_hash([
        DRK_POSEIDON_DOMAIN_TOKEN_COMMIT, asset_id, token_blind.inner(),
    ]);
    let commitment = CommitmentAttributes {
        version: 0,
        public_key: uncle_miner.clone(),
        value,
        asset_id: dwow_sdk::crypto::AssetId::from_base(asset_id),
        spend_hook: FuncId::from_base(spend_hook),
        user_data,
        blind: commitment_blind.clone(),
    }
    .to_commitment();
    let tx_binding = poseidon_hash([DRK_POSEIDON_DOMAIN_TX_BINDING, tx_commitment, tx_nonce]);

    // The spender is the uncle miner, so the clear input's signature public is
    // the uncle miner's key rather than a minter-derived one.
    let c_input = ClearInput {
        value,
        asset_id,
        value_blind: value_blind.clone(),
        token_blind: token_blind.clone(),
        signature_public: uncle_miner.clone(),
    };

    debug!(target: "contract::native_token::client::uncle_mint", "Minted uncle note: value={value}");

    let note = NativeToken {
        value,
        asset_id,
        spend_hook,
        user_data,
        commitment_blind: commitment_blind.clone().inner(),
        // Placeholder — the producer does not know the spend key. The wallet
        // spends with its own secret (see the doc comment above).
        spend_secret: uncle_hash_base,
        value_blind: value_blind.clone().inner(),
        token_blind: token_blind.clone().inner(),
        memo: vec![],
    };
    let encrypted_note = AeadEncryptedNote::encrypt_deterministic(&note, &uncle_miner, ephemeral_secret)?;

    let c_output = Output {
        value_commit,
        token_commit,
        commitment,
        // Unbound: see the doc comment above.
        nullifier: None,
        note: encrypted_note,
    };

    let params = UncleMintParamsV1 {
        input: c_input,
        total_pin: 0,
        output: c_output,
        // Value binding for the mint: the note commits to exactly this value.
        effective_value: value,
        commitment_attrs: CommitmentAttributes {
            version: 0,
            public_key: uncle_miner,
            value,
            asset_id: dwow_sdk::crypto::AssetId::from_base(asset_id),
            spend_hook: FuncId::from_base(spend_hook),
            user_data,
            blind: commitment_blind,
        },
        tx_binding,
        tx_nonce,
    };
    Ok(UncleMintCallDebris { params })
}
