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

//! Bearer Bond BurnStakeV1 Client API
//!
//! Issuer retires a staking pool by burning remaining stake coins.
//! Uses Burn_V1 proofs to prove ownership and prevent double-spends.
//! No outputs are created — the coins are destroyed.

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    bridgetree::Hashable,
    crypto::{pedersen_commitment_u64, poseidon_hash, BaseBlind, MerkleNode, ScalarBlind},
    pasta::pallas,
};
use rand::rngs::OsRng;
use tracing::debug;

use crate::model::{BondInput, BurnStakeParamsV1, CoinAttributes, Nullifier};
use super::point_coords;

/// Public inputs revealed after Burn_V1 proof.
/// Order must match Burn_V1 circuit:
/// nullifier, value_commit_x, value_commit_y, token_commit, merkle_root,
/// user_data_enc, spend_hook, signature_public
pub struct BurnStakeRevealed {
    pub nullifier: Nullifier,
    pub value_commit: pallas::Point,
    pub token_commit: pallas::Base,
    pub merkle_root: MerkleNode,
    pub user_data_enc: pallas::Base,
    pub spend_hook: pallas::Base,
    pub signature_public: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl BurnStakeRevealed {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        let (vc_x, vc_y) = point_coords(self.value_commit);
        vec![
            self.nullifier.inner(),
            vc_x,
            vc_y,
            self.token_commit,
            self.merkle_root.inner(),
            self.user_data_enc,
            self.spend_hook,
            self.signature_public,
            self.tx_binding,
            self.tx_nonce,
        ]
    }
}

/// Input for burning a stake coin (retiring the staking pool).
pub struct BurnStakeCallInput {
    /// Principal value staked
    pub principal: u64,
    /// Token ID of the staking pool series
    pub token_id: pallas::Base,
    /// Spend hook
    pub spend_hook: pallas::Base,
    /// User data
    pub user_data: pallas::Base,
    /// Coin blinding factor
    pub coin_blind: pallas::Base,
    /// Block height when stake matures (ZK-committed, must match coin)
    pub maturity_block: u64,
    /// Merkle tree leaf position
    pub leaf_position: u64,
    /// Merkle path (siblings)
    pub merkle_path: Vec<MerkleNode>,
    /// Caller's secret key
    pub secret: pallas::Base,
    /// Ephemeral signature secret — MUST be fresh per transaction
    pub ephemeral_signature_secret: pallas::Base,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

/// Debris produced by building a BurnStake call.
pub struct BurnStakeCallDebris {
    /// The contract call parameters
    pub params: BurnStakeParamsV1,
    /// The ZK proofs (one Burn_V1 proof per input)
    pub proofs: Vec<Proof>,
}

/// Builder for `BearerBond::BurnStakeV1` contract call.
pub struct BurnStakeCallBuilder {
    /// Stake coins to retire
    pub inputs: Vec<BurnStakeCallInput>,
    /// `Burn_V1` zkas circuit ZkBinary
    pub burn_zkbin: ZkBinary,
    /// Proving key for Burn_V1
    pub burn_pk: ProvingKey,
}

impl BurnStakeCallBuilder {
    /// Build the BurnStake call debris.
    pub fn build(self) -> Result<BurnStakeCallDebris> {
        debug!(target: "contract::bearer_bond::client::burn_stake", "Building BearerBond::BurnStakeV1 contract call");

        if self.inputs.is_empty() {
            return Err(dwow_sdk::error::ContractError::Custom(
                crate::error::BearerBondError::MissingInputs.code(),
            )
            .into());
        }

        let mut proofs = vec![];
        let mut inputs = vec![];

        for input in self.inputs.into_iter() {
            let value_blind = ScalarBlind::random(&mut OsRng);
            let token_id_blind = BaseBlind::random(&mut OsRng);
            let user_data_blind = BaseBlind::random(&mut OsRng);

            let (proof, revealed) = create_burn_stake_proof(
                &self.burn_zkbin,
                &self.burn_pk,
                &input,
                value_blind.clone(),
                token_id_blind.clone(),
                user_data_blind.clone(),
            )?;

            proofs.push(proof);

            inputs.push(BondInput {
                value_commit: revealed.value_commit,
                token_commit: revealed.token_commit,
                nullifier: revealed.nullifier,
                merkle_root: revealed.merkle_root,
                user_data_enc: revealed.user_data_enc,
                spend_hook: input.spend_hook,
                signature_public: revealed.signature_public,
            });
        }

        Ok(BurnStakeCallDebris {
            params: BurnStakeParamsV1 { inputs },
            proofs,
        })
    }
}

/// Create a Burn_V1 proof for retiring a stake coin.
///
/// Witness order must match Burn_V1 circuit:
/// secret, value, token_id, spend_hook, user_data, coin_blind,
/// value_blind, token_id_blind, user_data_blind, leaf_position,
/// merkle_path, ephemeral_signature_secret
fn create_burn_stake_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &BurnStakeCallInput,
    value_blind: ScalarBlind,
    token_id_blind: BaseBlind,
    user_data_blind: BaseBlind,
) -> Result<(Proof, BurnStakeRevealed)> {
    let public_key = poseidon_hash([pallas::Base::from(7), input.secret]);

    let coin = CoinAttributes {
        public_key,
        value: input.principal,
        token_id: input.token_id,
        spend_hook: input.spend_hook,
        user_data: input.user_data,
        blind: input.coin_blind,
        maturity_block: input.maturity_block,
    }
    .to_coin();

    let nullifier = Nullifier::new(input.secret, coin);

    let merkle_root = {
        let position: u64 = input.leaf_position;
        let mut current = MerkleNode::from_base(coin);
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

    let value_commit = pedersen_commitment_u64(input.principal, value_blind.clone());
    let token_commit = poseidon_hash([pallas::Base::from(2), input.token_id, token_id_blind.inner()]);
    let user_data_enc = poseidon_hash([pallas::Base::from(6), input.user_data, user_data_blind.inner()]);
    let signature_public = poseidon_hash([pallas::Base::from(7), input.ephemeral_signature_secret]);

    let public_inputs = BurnStakeRevealed {
        nullifier,
        value_commit,
        token_commit,
        merkle_root,
        user_data_enc,
        spend_hook: input.spend_hook,
        signature_public,
        tx_binding: pallas::Base::zero(),
        tx_nonce: input.tx_nonce,
    };

    let prover_witnesses = vec![
        Witness::Base(Value::known(input.secret)),
        Witness::Base(Value::known(pallas::Base::from(input.principal))),
        Witness::Base(Value::known(input.token_id)),
        Witness::Base(Value::known(input.spend_hook)),
        Witness::Base(Value::known(input.user_data)),
        Witness::Base(Value::known(input.coin_blind)),
        Witness::Scalar(Value::known(value_blind.inner())),
        Witness::Base(Value::known(token_id_blind.inner())),
        Witness::Base(Value::known(user_data_blind.inner())),
        Witness::Uint32(Value::known(
            u64::from(input.leaf_position).try_into().unwrap(),
        )),
        Witness::MerklePath(Value::known(
            input.merkle_path.clone().try_into().unwrap(),
        )),
        Witness::Base(Value::known(input.ephemeral_signature_secret)),
        Witness::Base(Value::known(input.tx_commitment)),
        Witness::Base(Value::known(input.tx_nonce)),
        Witness::Base(Value::known(pallas::Base::zero())), // tx_binding
    ];

    let circuit = ZkCircuit::new(prover_witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}
