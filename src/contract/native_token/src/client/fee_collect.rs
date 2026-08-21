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
//! forwards accumulated FeeV1 fees to the miner (consensus-coinbase.md §3).
//!
//! Uses the dedicated FeeCollect_V1 circuit (12 witnesses, 7 public inputs,
//! NO cumulative supply chain — fees are redistribution, not minting).
//! The circuit derives pk_H from coin_secret (constraints C1-C3), so the fee
//! coin recipient is ALWAYS PublicKey::from_secret(sk_H) — zero public key
//! exposure, identity proven via nullifier only.
//!
//! Fully deterministic per spec §3.6: blinds (domains 10-12), AEAD ephemeral
//! secret (domain 13), and proof RNG seed (domain 14) are all derived from
//! poseidon_hash([sk_H, height, domain]). No ambient randomness.

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    blockchain::{BlockHeight, FeeAmount},
    crypto::{
        constants::{DRK_POSEIDON_DOMAIN_TOKEN_COMMIT, DRK_POSEIDON_DOMAIN_TX_BINDING},
        note::AeadEncryptedNote, pasta_prelude::*, pedersen_commitment_u64, poseidon_hash,
        BaseBlind, Blind, FuncId, PublicKey, ScalarBlind, SecretKey, AssetId,
    },
    pasta::pallas,
};
use rand::SeedableRng;
use tracing::debug;

use super::NativeToken;
use crate::circuit::CircuitPublicInputs;
use crate::model::{Coin, CoinAttributes, DRKW_ASSET_ID, FeeCollectParamsV1, Nullifier, Output};

/// Domain separators for deterministic derivation — consensus-coinbase.md §3.6.
/// Distinct from coinbase domains (1/2/3) to prevent blind reuse.
const DOMAIN_VALUE_BLIND: u64 = 10;
const DOMAIN_TOKEN_BLIND: u64 = 11;
const DOMAIN_COIN_BLIND: u64 = 12;
const DOMAIN_AEAD_EPHEMERAL: u64 = 13;
const DOMAIN_PROOF_RNG: u64 = 14;

/// Debris produced by building a FeeCollectV1 call
pub struct FeeCollectCallDebris {
    pub params: FeeCollectParamsV1,
    pub proofs: Vec<Proof>,
}

/// Public inputs revealed after FeeCollect_V1 proof creation.
/// Order matches the circuit's constrain_instance calls exactly (spec §3.5):
/// [C, nf, vc.x, vc.y, tc, tx_binding, tx_nonce]
pub struct FeeCollectRevealed {
    pub coin: Coin,
    /// Nullifier: nf = poseidon_hash(coin_secret, coin) — capability claim
    pub nullifier: pallas::Base,
    pub value_commit: pallas::Point,
    pub token_commit: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl FeeCollectRevealed {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        self.to_public_inputs()
    }
}

impl crate::circuit::CircuitPublicInputs for FeeCollectRevealed {
    const COUNT: usize = 7;

    fn to_public_inputs(&self) -> Vec<pallas::Base> {
        let valcom_coords = self.value_commit.to_affine().coordinates()
            .expect("Value commitment cannot be the identity element");
        vec![
            self.coin.inner(),      // 1: C
            self.nullifier,         // 2: nf
            *valcom_coords.x(),     // 3: vc.x
            *valcom_coords.y(),     // 4: vc.y
            self.token_commit,      // 5: tc
            self.tx_binding,        // 6: tx_binding = poseidon_hash(tx_commitment, tx_nonce)
            self.tx_nonce,          // 7: tx_nonce
        ]
    }
}

/// Create the FeeCollect_V1 ZK proof (spec §3.5).
///
/// Witness order matches fee_collect_v1.zk exactly (12 witnesses).
/// Proof RNG is seeded from poseidon_hash([sk_H, H, domain=14]) — the
/// RFC 6979 pattern: deterministic proof bytes without weakening ZK
/// (spec §3.6 requirement 1).
#[allow(clippy::too_many_arguments)]
fn create_fee_collect_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    output: &CoinAttributes,
    coin_secret: SecretKey,
    value_blind: ScalarBlind,
    token_blind: BaseBlind,
    block_height: BlockHeight,
    tx_commitment: pallas::Base,
    tx_nonce: pallas::Base,
) -> Result<(Proof, FeeCollectRevealed)> {
    let value_commit = pedersen_commitment_u64(output.value, value_blind.clone());
    let token_commit = poseidon_hash([DRK_POSEIDON_DOMAIN_TOKEN_COMMIT, output.asset_id.inner(), token_blind.clone().inner()]);
    #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy() is always Some")]
    let (pub_x, pub_y) = output.public_key.xy().expect("pk not identity");

    let coin = output.to_coin();

    // Nullifier: nf = poseidon_hash(DOMAIN_NULLIFIER, coin_secret, C) — spec §3.4
    let nf = Nullifier::new(coin_secret.clone(), coin.inner()).inner();

    // tx_binding = poseidon_hash(tx_commitment, tx_nonce) — spec §3.5 (D11).
    // MUST be the hash, not the raw tx_commitment: with (0, 0) inputs the
    // hash is nonzero, and declaring raw zero breaks proof verification.
    let tx_binding = poseidon_hash([DRK_POSEIDON_DOMAIN_TX_BINDING, tx_commitment, tx_nonce]);

    let public_inputs = FeeCollectRevealed {
        coin,
        nullifier: nf,
        value_commit,
        token_commit,
        tx_binding,
        tx_nonce,
    };

    // Witness order matches fee_collect_v1.zk declaration order exactly.
    let prover_witnesses = vec![
        Witness::Base(Value::known(pub_x)),                                  // 1: coin_public_x
        Witness::Base(Value::known(pub_y)),                                  // 2: coin_public_y
        Witness::Base(Value::known(pallas::Base::from(output.value))),      // 3: coin_value
        Witness::Base(Value::known(output.asset_id.inner())),               // 4: coin_asset_id
        Witness::Base(Value::known(output.spend_hook.inner())),             // 5: coin_spend_hook
        Witness::Base(Value::known(output.user_data)),                      // 6: coin_user_data
        Witness::Base(Value::known(output.blind.inner())),                  // 7: coin_blind
        Witness::Base(Value::known(*coin_secret.inner())),                   // 8: coin_secret
        Witness::Scalar(Value::known(value_blind.clone().inner())),                 // 9: value_blind
        Witness::Base(Value::known(token_blind.clone().inner())),                   // 10: token_blind
        Witness::Base(Value::known(tx_commitment)),                         // 11: tx_commitment
        Witness::Base(Value::known(tx_nonce)),                              // 12: tx_nonce
    ];

    let circuit = ZkCircuit::new(prover_witnesses, zkbin);

    // Deterministic proof RNG — spec §3.6 requirement 1. Seed derived from
    // (sk_H, height, domain=14). Same seed → identical proof bytes on every
    // validator re-execution.
    let seed: [u8; 32] = poseidon_hash([
        *coin_secret.inner(),
        pallas::Base::from(block_height.get()),
        pallas::Base::from(DOMAIN_PROOF_RNG),
    ])
    .to_repr();
    let mut rng = rand::rngs::StdRng::from_seed(seed);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut rng)?;

    Ok((proof, public_inputs))
}

/// Builder for creating FeeCollectV1 contract calls.
///
/// This is the "collection plate" — appended as the final transaction in
/// every block to forward accumulated fees to the miner (spec §3.1).
///
/// The recipient is ALWAYS PublicKey::from_secret(secret): the FeeCollect_V1
/// circuit derives pk_H from coin_secret (constraints C1-C3), so any other
/// recipient makes the proof unsatisfiable.
pub struct FeeCollectCallBuilder {
    /// Caller's secret key (sk_H — per-block derived, same as coinbase §3.2)
    pub secret: SecretKey,
    /// Block height this fee collection targets
    pub block_height: BlockHeight,
    /// Total fees accumulated in fees_db[height] for this block
    pub total_fees: FeeAmount,
    /// Sum of fee_value_blind from each FeeV2 call — for Pedersen verification.
    /// Spec: fee-spec.md §5.6.4.
    pub total_blind: pallas::Scalar,
    /// FeeCollect_V1 zkas circuit ZkBinary
    pub fee_collect_zkbin: ZkBinary,
    /// Proving key for the FeeCollect_V1 zk circuit
    pub fee_collect_pk: ProvingKey,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl FeeCollectCallBuilder {
    pub fn build(&self) -> Result<FeeCollectCallDebris> {
        debug!(target: "contract::native_token::client::fee_collect",
            "Building FeeCollectV1: {} fees at height {}", self.total_fees, self.block_height);

        let asset_id = DRKW_ASSET_ID.inner();
        // Circuit-enforced: fee coin recipient is pk_H (spec §3.3).
        let public_key = PublicKey::from_secret(self.secret.clone());

        // Deterministic blinds — spec §3.6, domains 10-12.
        let sk_base = *self.secret.inner();
        let h_base = pallas::Base::from(self.block_height.get());
        let value_blind: ScalarBlind = Blind(
            Option::<pallas::Scalar>::from(pallas::Scalar::from_repr(
                poseidon_hash([sk_base, h_base, pallas::Base::from(DOMAIN_VALUE_BLIND)]).to_repr(),
            ))
            .ok_or_else(|| dwow_core::Error::Custom("Invalid scalar value_blind".into()))?,
        );
        let token_blind = BaseBlind::ZERO;  // Native DRKW: spec fee-spec.md §4.2 C5
        let coin_blind: BaseBlind = Blind(poseidon_hash([
            sk_base, h_base, pallas::Base::from(DOMAIN_COIN_BLIND),
        ]));

        // Build the fee coin output for the miner — spec §3.3
        let output = CoinAttributes {
            version: 0,
            public_key,
            value: self.total_fees.get(),
            asset_id: AssetId::from_base(asset_id),
            spend_hook: FuncId::none(),
            user_data: pallas::Base::ZERO,
            blind: coin_blind.clone(),
        };

        // Dedicated FeeCollect_V1 circuit — spec §3.5. No cumulative supply.
        let (proof, public_inputs) = create_fee_collect_proof(
            &self.fee_collect_zkbin,
            &self.fee_collect_pk,
            &output,
            self.secret.clone(),
            value_blind.clone(),
            token_blind.clone(),
            self.block_height,
            self.tx_commitment,
            self.tx_nonce,
        )?;

        // Construct the output note for wallet discovery
        let output_attrs = NativeToken {
            value: self.total_fees.get(),
            asset_id,
            spend_hook: pallas::Base::ZERO,
            user_data: pallas::Base::ZERO,
            coin_blind: coin_blind.clone().inner(),
            coin_secret: *self.secret.inner(),
            value_blind: value_blind.clone().inner(),
            token_blind: token_blind.clone().inner(),
            memo: vec![],
        };

        // Deterministic AEAD encryption — spec §3.6 requirement 2, domain 13.
        // Ephemeral secret derived from (sk_H, height, domain) — never reused.
        let ephem_secret = SecretKey::from_base(poseidon_hash([
            sk_base, h_base, pallas::Base::from(DOMAIN_AEAD_EPHEMERAL),
        ]));
        let encrypted_note =
            AeadEncryptedNote::encrypt_deterministic(&output_attrs, &public_key, ephem_secret)
                .map_err(|e| {
                    dwow_core::Error::Custom(format!("fee collect note encryption: {:?}", e))
                })?;

        let nullifier = Nullifier::new(self.secret.clone(), public_inputs.coin.inner());

        Ok(FeeCollectCallDebris {
            params: FeeCollectParamsV1 {
                total_fees: self.total_fees,
                total_blind: self.total_blind,
                output: Output {
                    value_commit: public_inputs.value_commit,
                    token_commit: public_inputs.token_commit,
                    coin: public_inputs.coin,
                    nullifier,
                    note: encrypted_note,
                },
                nullifier,
                tx_binding: public_inputs.tx_binding,
                tx_nonce: public_inputs.tx_nonce,
            },
            proofs: vec![proof],
        })
    }
}
