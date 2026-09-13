/* This file is part of DarkWow
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

//! NativeToken FeeCollectV1 Client API
//!
//! Builds the "collection plate" — the final transaction in every block that
//! forwards accumulated FeeV3 fees to the miner (consensus-coinbase.md §3).
//!
//! Plaintext since the FeeCollect_V2 proof was dropped (2026-09): the fee
//! amount is public (plain `fees_db[height]` sum), and the WASM entrypoint
//! verifies the claim in plaintext. No ZK proof is generated or verified.
//! The commitment recipient is ALWAYS PublicKey::from_secret(sk_H) — zero
//! public key exposure, identity proven via nullifier only.
//!
//! Fully deterministic per spec §3.6: blinds (domains 10/12) and the AEAD
//! ephemeral secret (domain 13) are derived from
//! poseidon_hash([sk_H, height, domain]). No ambient randomness.

use dwow_core::Result;
use dwow_sdk::{
    blockchain::{BlockHeight, FeeAmount},
    crypto::{
        constants::{DRK_POSEIDON_DOMAIN_TOKEN_COMMIT, DRK_POSEIDON_DOMAIN_TX_BINDING},
        note::AeadEncryptedNote, pasta_prelude::*, pedersen_commitment_u64, poseidon_hash,
        BaseBlind, Blind, FuncId, PublicKey, ScalarBlind, SecretKey, AssetId,
    },
    pasta::pallas,
};
use tracing::debug;

use super::NativeToken;
use crate::model::{CommitmentAttributes, DRKW_ASSET_ID, FeeCollectParamsV1, Nullifier, Output};

/// Domain separators for deterministic derivation — consensus-coinbase.md §3.6.
/// Distinct from coinbase domains (1/2/3) to prevent blind reuse.
/// (Domain 14 — the proof RNG seed — retired with the FeeCollect_V2 circuit.)
const DOMAIN_VALUE_BLIND: u64 = 10;
const DOMAIN_COMMITMENT_BLIND: u64 = 12;
const DOMAIN_AEAD_EPHEMERAL: u64 = 13;

/// Debris produced by building a FeeCollectV1 call
pub struct FeeCollectCallDebris {
    pub params: FeeCollectParamsV1,
}

/// Builder for creating FeeCollectV1 contract calls.
///
/// This is the "collection plate" — appended as the final transaction in
/// every block to forward accumulated fees to the miner (spec §3.1).
///
/// The recipient is ALWAYS PublicKey::from_secret(secret): the commitment is
/// constructed directly for pk_H, so the fee commitment can only be spent by
/// the miner holding sk_H (proven at spend time by SpendV1, not by a
/// FeeCollect proof).
pub struct FeeCollectCallBuilder {
    /// Caller's secret key (sk_H — per-block derived, same as coinbase §3.2)
    pub secret: SecretKey,
    /// Block height this fee collection targets
    pub block_height: BlockHeight,
    /// Total fees accumulated in fees_db[height] for this block
    pub total_fees: FeeAmount,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl FeeCollectCallBuilder {
    pub fn build(&self) -> Result<FeeCollectCallDebris> {
        debug!(target: "contract::native_token::client::fee_collect",
            "Building FeeCollectV1: {} fees at height {}", self.total_fees, self.block_height);

        let asset_id = DRKW_ASSET_ID.inner();
        // Fee commitment recipient is pk_H (spec §3.3).
        let public_key = PublicKey::from_secret(self.secret.clone());

        // Deterministic blinds — spec §3.6, domains 10/12.
        let sk_base = *self.secret.inner();
        let h_base = pallas::Base::from(self.block_height.get());
        let value_blind: ScalarBlind = Blind(
            Option::<pallas::Scalar>::from(pallas::Scalar::from_repr(
                poseidon_hash([sk_base, h_base, pallas::Base::from(DOMAIN_VALUE_BLIND)]).to_repr(),
            ))
            .ok_or_else(|| dwow_core::Error::Custom("Invalid scalar value_blind".into()))?,
        );
        let token_blind = BaseBlind::ZERO;  // Native DRKW: spec fee-spec.md §4.2 C5
        let commitment_blind: BaseBlind = Blind(poseidon_hash([
            sk_base, h_base, pallas::Base::from(DOMAIN_COMMITMENT_BLIND),
        ]));

        // Build the fee commitment output for the miner — spec §3.3
        let output = CommitmentAttributes {
            version: 0,
            public_key,
            value: self.total_fees.get(),
            asset_id: AssetId::from_base(asset_id),
            spend_hook: FuncId::none(),
            user_data: pallas::Base::ZERO,
            blind: commitment_blind.clone(),
        };

        // Plaintext revealed values — formerly the FeeCollect_V2 proof's
        // public inputs, now computed directly (byte-identical params, no
        // proof). The WASM entrypoint re-checks each value in plaintext.
        // UNVERIFIED(F2-1): needs cargo check -p dwow_native_token_contract && cargo test -p dwowd --lib
        let value_commit = pedersen_commitment_u64(output.value, value_blind.clone());
        let token_commit = poseidon_hash([
            DRK_POSEIDON_DOMAIN_TOKEN_COMMIT,
            output.asset_id.inner(),
            token_blind.clone().inner(),
        ]);
        let commitment = output.to_commitment();

        // Nullifier: nf = poseidon_hash(spend_secret, C) — spec §3.4
        let nullifier = Nullifier::new(self.secret.clone(), commitment.inner());

        // tx_binding = poseidon_hash(tx_commitment, tx_nonce) — spec §3.5 (D11).
        // MUST be the hash, not the raw tx_commitment: with (0, 0) inputs the
        // hash is nonzero, and declaring raw zero breaks verification.
        let tx_binding = poseidon_hash([
            DRK_POSEIDON_DOMAIN_TX_BINDING,
            self.tx_commitment,
            self.tx_nonce,
        ]);

        // Construct the output note for wallet discovery
        let output_attrs = NativeToken {
            value: self.total_fees.get(),
            asset_id,
            spend_hook: pallas::Base::ZERO,
            user_data: pallas::Base::ZERO,
            commitment_blind: commitment_blind.clone().inner(),
            spend_secret: *self.secret.inner(),
            value_blind: value_blind.clone().inner(),
            token_blind: token_blind.clone().inner(),
            memo: vec![],
        };

        // Deterministic AEAD encryption — spec §3.6 requirement 1, domain 13.
        // Ephemeral secret derived from (sk_H, height, domain) — never reused.
        let ephem_secret = SecretKey::from_base(poseidon_hash([
            sk_base, h_base, pallas::Base::from(DOMAIN_AEAD_EPHEMERAL),
        ]));
        let encrypted_note =
            AeadEncryptedNote::encrypt_deterministic(&output_attrs, &public_key, ephem_secret)
                .map_err(|e| {
                    dwow_core::Error::Custom(format!("fee collect note encryption: {:?}", e))
                })?;

        Ok(FeeCollectCallDebris {
            params: FeeCollectParamsV1 {
                total_fees: self.total_fees,
                output: Output {
                    value_commit,
                    token_commit,
                    commitment,
                    nullifier: Some(nullifier.clone()),
                    note: encrypted_note,
                },
                nullifier,
                tx_binding,
                tx_nonce: self.tx_nonce,
            },
        })
    }
}
