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

//! NativeToken BurnV1 Client API
//!
//! This module provides the ability to build Burn calls to destroy coins.

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    bridgetree::Hashable,
    crypto::{
        constants::{DRK_POSEIDON_DOMAIN_SIGNATURE_SECRET, DRK_POSEIDON_DOMAIN_TOKEN_COMMIT, DRK_POSEIDON_DOMAIN_TX_BINDING, DRK_POSEIDON_DOMAIN_USER_DATA_ENC},
        pasta_prelude::*, pedersen_commitment_u64, poseidon_hash, BaseBlind, Blind, FuncId,
        MerkleNode, PublicKey, ScalarBlind, SecretKey, AssetId,
    },
    error::ContractError,
    pasta::pallas,
};
use rand::{rngs::OsRng, SeedableRng};
use tracing::debug;

use crate::model::{BurnParamsV1, CoinAttributes, Input, Nullifier};

/// Public inputs revealed after burn proof creation
pub struct BurnRevealed {
    pub nullifier: Nullifier,
    pub value_commit: pallas::Point,
    pub token_commit: pallas::Base,
    pub merkle_root: MerkleNode,
    pub user_data_enc: pallas::Base,
    pub spend_hook: pallas::Base,
    pub signature_public: PublicKey,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl BurnRevealed {
    #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so x()/y() is always Some")]
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        let valcom_coords = self.value_commit.to_affine().coordinates().expect("Value commitment cannot be the identity element");
        vec![
            self.nullifier.inner(),
            *valcom_coords.x(),
            *valcom_coords.y(),
            self.token_commit,
            self.merkle_root.inner(),
            self.user_data_enc,
            self.spend_hook,
            self.signature_public.x().expect("pk not identity"),
            self.signature_public.y().expect("pk not identity"),
            self.tx_binding,
            self.tx_nonce,
        ]
    }
}

/// Create a ZK proof for burning (destroying) a coin.
#[allow(clippy::too_many_arguments)]
pub fn create_burn_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &BurnCallInput,
    value_blind: ScalarBlind,
    token_blind: BaseBlind,
    user_data_blind: BaseBlind,
    secret: SecretKey,
) -> Result<(Proof, BurnRevealed, SecretKey)> {
    let public_key = PublicKey::from_secret(secret.clone());

    // Reconstruct coin from the input
    let coin = CoinAttributes {
            version: 0,
        public_key,
        value: input.value,
        asset_id: AssetId::from_base(input.asset_id),
        spend_hook: FuncId::from_base(input.spend_hook),
        user_data: input.user_data,
        blind: Blind(input.coin_blind),
    }
    .to_coin();

    // Calculate nullifier: poseidon_hash(secret, coin)
    let nullifier = Nullifier::new(secret.clone(), coin.inner());

    // Derive per-burn unique signature_secret from coin_secret + nullifier.
    // This binds the signer to the coin owner (fixes H2) while keeping
    // signature_public unlinkable across burns (nullifier is unique per coin).
    let signature_secret = SecretKey::from_base(poseidon_hash([DRK_POSEIDON_DOMAIN_SIGNATURE_SECRET, *secret.inner(), nullifier.inner()]));
    let signature_public = PublicKey::from_secret(signature_secret.clone());

    // Calculate merkle root from coin and merkle path
    let merkle_root = {
        let position: u64 = input.leaf_position.into();
        let mut current = MerkleNode::from_base(coin.inner());
        for (level, sibling) in input.merkle_path.iter().enumerate() {
            let level = level as u8;
            current = if position & (1 << level) == 0 {
                MerkleNode::combine(level.into(), &current, sibling)
            } else {
                MerkleNode::combine(level.into(), sibling, &current)
            };
        }
        current
    };

    let user_data_enc = poseidon_hash([DRK_POSEIDON_DOMAIN_USER_DATA_ENC, input.user_data, user_data_blind.clone().inner()]);
    let value_commit = pedersen_commitment_u64(input.value, value_blind.clone());
    let token_commit = poseidon_hash([DRK_POSEIDON_DOMAIN_TOKEN_COMMIT, input.asset_id, token_blind.clone().inner()]);
    let tx_binding = poseidon_hash([
        DRK_POSEIDON_DOMAIN_TX_BINDING,
        input.tx_commitment,
        input.tx_nonce,
    ]);

    let public_inputs = BurnRevealed {
        nullifier,
        value_commit,
        token_commit,
        merkle_root,
        spend_hook: input.spend_hook,
        user_data_enc,
        signature_public,
        tx_binding,
        tx_nonce: input.tx_nonce,
    };

    #[expect(clippy::unwrap_used, reason = "leaf position fits u32")]
    let leaf_position: u32 = u64::from(input.leaf_position).try_into().unwrap();
    #[expect(clippy::unwrap_used, reason = "merkle path length equals fixed tree depth")]
    let merkle_path = {
        let mut path = input.merkle_path.clone();
        if path.is_empty() {
            path.push(MerkleNode::from_bytes([0u8; 32])
                .unwrap_or_else(|| MerkleNode::new(pallas::Base::zero())));
        }
        path.try_into().unwrap()
    };
    #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so x()/y() is always Some")]
    let sig_pub_x = signature_public.x().expect("pk not identity");
    #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so x()/y() is always Some")]
    let sig_pub_y = signature_public.y().expect("pk not identity");
    let prover_witnesses = vec![
        Witness::Base(Value::known(*secret.inner())),
        Witness::Base(Value::known(pallas::Base::from(input.value))),
        Witness::Base(Value::known(input.asset_id)),
        Witness::Base(Value::known(input.spend_hook)),
        Witness::Base(Value::known(input.user_data)),
        Witness::Base(Value::known(input.coin_blind)),
        Witness::Scalar(Value::known(value_blind.clone().inner())),
        Witness::Base(Value::known(token_blind.clone().inner())),
        Witness::Base(Value::known(user_data_blind.clone().inner())),
        Witness::Uint32(Value::known(leaf_position)),
        Witness::MerklePath(Value::known(merkle_path)),
        // Per-burn signature_secret = poseidon_hash(coin_secret, nullifier).
        // Cryptographically bound to coin_secret (fixes H2) but unique per burn
        // (different nullifier → different signature_public — unlinkable).
        Witness::Base(Value::known(*signature_secret.inner())),
        Witness::Base(Value::known(sig_pub_x)),
        Witness::Base(Value::known(sig_pub_y)),
        Witness::Base(Value::known(input.tx_commitment)),
        Witness::Base(Value::known(input.tx_nonce)),
        Witness::Base(Value::known(tx_binding)),
    ];

    let circuit = ZkCircuit::new(prover_witnesses, zkbin);
    let proof = if crate::deterministic_zk_enabled() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(0);
        Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut rng)?
    } else {
        Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?
    };

    Ok((proof, public_inputs, signature_secret))
}

/// Struct holding necessary information to build a `NativeToken::BurnV1`
/// contract call.
pub struct BurnCallBuilder {
    /// Anonymous inputs
    pub inputs: Vec<BurnCallInput>,
    /// `Burn_V1` zkas circuit ZkBinary
    pub burn_zkbin: ZkBinary,
    /// Proving key for the `Burn_V1` zk circuit
    pub burn_pk: ProvingKey,
}

/// Input for building a burn call
pub struct BurnCallInput {
    /// Value of the coin being burned
    pub value: u64,
    /// Token ID
    pub asset_id: pallas::Base,
    /// Spend hook
    pub spend_hook: pallas::Base,
    /// User data
    pub user_data: pallas::Base,
    /// Coin blind
    pub coin_blind: pallas::Base,
    /// Merkle tree leaf position
    pub leaf_position: u64,
    /// Merkle path (siblings)
    pub merkle_path: Vec<dwow_sdk::crypto::MerkleNode>,
    /// Caller's secret key for coin ownership
    pub secret: SecretKey,
    /// Ephemeral signature secret — MUST be fresh per transaction.
    /// Never reuse the wallet secret here; doing so links all
    /// transactions to the same on-chain signature_public.
    pub ephemeral_signature_secret: SecretKey,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

/// Debris produced by building a Burn call, containing the parameters
/// and ZK proofs needed to execute the transaction.
pub struct BurnCallDebris {
    /// The contract call parameters
    pub params: BurnParamsV1,
    /// The ZK proofs for the burn operation
    pub proofs: Vec<Proof>,
    /// The ephemeral secret keys created for signing
    pub signature_secrets: Vec<SecretKey>,
}

impl BurnCallBuilder {
    /// Build the Burn call debris
    pub fn build(self) -> Result<BurnCallDebris> {
        debug!(target: "contract::native_token::client::burn", "Building NativeToken::BurnV1 contract call");

        if self.inputs.is_empty() {
            return Err(ContractError::Custom(1).into());
        }

        let mut proofs = vec![];
        let mut signature_secrets = vec![];
        let mut inputs = vec![];

        // Capture tx_binding values before consuming self.inputs
        let tx_commitment = self.inputs.first().map(|i| i.tx_commitment).unwrap_or(pallas::Base::zero());
        let tx_nonce = self.inputs.first().map(|i| i.tx_nonce).unwrap_or(pallas::Base::zero());

        for input in self.inputs.into_iter() {
            let secret = input.secret.clone();

            // Generate burn proof
            // DZ-4: single seeded RNG for all three blinds in deterministic mode
            // so PI-7 replay produces identical proof bytes.
            let (value_blind, token_blind, user_data_blind) =
                if crate::deterministic_zk_enabled() {
                    let mut rng = rand::rngs::StdRng::seed_from_u64(0);
                    (ScalarBlind::random(&mut rng), BaseBlind::random(&mut rng),
                     BaseBlind::random(&mut rng))
                } else {
                    (ScalarBlind::random(&mut OsRng), BaseBlind::random(&mut OsRng),
                     BaseBlind::random(&mut OsRng))
                };

            // create_burn_proof derives the per-burn signature_secret from
            // (coin_secret, nullifier) — the params MUST use the same derived
            // signature_public (revealed) as the proof, not the ephemeral input.
            let (proof, revealed, sig_secret) = create_burn_proof(
                &self.burn_zkbin,
                &self.burn_pk,
                &input,
                value_blind.clone(),
                token_blind.clone(),
                user_data_blind.clone(),
                secret.clone(),
            )?;

            proofs.push(proof);
            signature_secrets.push(sig_secret);

            // Create the Input model for params
            let coin = CoinAttributes {
            version: 0,
                public_key: PublicKey::from_secret(secret.clone()),
                value: input.value,
                asset_id: AssetId::from_base(input.asset_id),
                spend_hook: FuncId::from_base(input.spend_hook),
                user_data: input.user_data,
                blind: Blind(input.coin_blind),
            }
            .to_coin();

            let value_commit = pedersen_commitment_u64(input.value, value_blind.clone());
            let token_commit = poseidon_hash([DRK_POSEIDON_DOMAIN_TOKEN_COMMIT, input.asset_id, token_blind.clone().inner()]);
            let nullifier = Nullifier::new(secret.clone(), coin.inner());

            // Calculate merkle root
            let merkle_root = {
                let position: u64 = input.leaf_position.into();
                let mut current = MerkleNode::from_base(coin.inner());
                for (level, sibling) in input.merkle_path.iter().enumerate() {
                    let level = level as u8;
                    current = if position & (1 << level) == 0 {
                        MerkleNode::combine(level.into(), &current, sibling)
                    } else {
                        MerkleNode::combine(level.into(), sibling, &current)
                    };
                }
                current
            };

            let user_data_enc = poseidon_hash([DRK_POSEIDON_DOMAIN_USER_DATA_ENC, input.user_data, user_data_blind.clone().inner()]);

            inputs.push(Input {
                value_commit,
                token_commit,
                nullifier,
                merkle_root,
                user_data_enc,
                spend_hook: FuncId::from_base(input.spend_hook),
                signature_public: revealed.signature_public,
            });
        }

        Ok(BurnCallDebris {
            params: BurnParamsV1 {
                inputs,
                tx_binding: poseidon_hash([
                    DRK_POSEIDON_DOMAIN_TX_BINDING,
                    tx_commitment,
                    tx_nonce,
                ]),
                tx_nonce,
            },
            proofs,
            signature_secrets,
        })
    }
}
