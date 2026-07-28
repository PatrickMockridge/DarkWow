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

//! Bearer Bond EmergencyUnstakeV1 Client API
//!
//! Allows unstaking before maturity when coverage falls below the minimum
//! threshold (10000 bps = 100%). The holder submits a coverage report
//! proving the series is under-collateralized. Burns the stake coin
//! (Burn_V1 proof) and creates a zero-value receipt coin (Redeem_V1 proof).

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    bridgetree::Hashable,
    crypto::{
        pedersen_commitment_u64, poseidon_hash, BaseBlind, MerkleNode, ScalarBlind,
    },
    pasta::pallas,
};
use rand::rngs::OsRng;
use tracing::debug;

use crate::model::{BondInput, CoinAttributes, CoverageReport, EmergencyUnstakeParamsV1, Nullifier};
use super::point_coords;

/// Public inputs revealed after Burn_V1 proof (emergency unstake input side).
pub struct EmergencyUnstakeBurnRevealed {
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

impl EmergencyUnstakeBurnRevealed {
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

/// Public inputs revealed after Redeem_V1 receipt proof.
pub struct EmergencyUnstakeReceiptRevealed {
    pub coin: pallas::Base,
    pub value_commit: pallas::Point,
    pub token_commit: pallas::Base,
    pub coin_value: pallas::Base,
    pub spend_hook: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl EmergencyUnstakeReceiptRevealed {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        let (vc_x, vc_y) = point_coords(self.value_commit);
        vec![
            self.coin,
            vc_x,
            vc_y,
            self.token_commit,
            self.coin_value,
            self.spend_hook,
            self.tx_binding,
            self.tx_nonce,
        ]
    }
}

/// Input for emergency unstaking a coin.
pub struct EmergencyUnstakeCallInput {
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
    /// Block height when stake matures (ZK-committed)
    pub maturity_block: u64,
    /// Merkle tree leaf position
    pub leaf_position: u64,
    /// Merkle path (siblings)
    pub merkle_path: Vec<MerkleNode>,
    /// Caller's secret key
    pub secret: pallas::Base,
    /// Ephemeral signature secret — MUST be fresh per transaction
    pub ephemeral_signature_secret: pallas::Base,
    /// Coverage report proving under-collateralization
    pub coverage_report: CoverageReport,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

/// Output for the receipt coin.
pub struct EmergencyUnstakeCallOutput {
    /// Redeemer's address (poseidon_hash of public key)
    pub recipient: pallas::Base,
    /// Token ID (same as unstaked coin)
    pub token_id: pallas::Base,
    /// Spend hook (issuer contract)
    pub spend_hook: pallas::Base,
    /// User data (emergency unstaking metadata)
    pub user_data: pallas::Base,
    /// Coin blinding factor (fresh random)
    pub coin_blind: pallas::Base,
}

/// Debris produced by building an EmergencyUnstake call.
pub struct EmergencyUnstakeCallDebris {
    pub params: EmergencyUnstakeParamsV1,
    pub proofs: Vec<Proof>,
}

/// Builder for `BearerBond::EmergencyUnstakeV1` contract call.
pub struct EmergencyUnstakeCallBuilder {
    pub input: EmergencyUnstakeCallInput,
    pub output: EmergencyUnstakeCallOutput,
    pub burn_zkbin: ZkBinary,
    pub burn_pk: ProvingKey,
    pub redeem_zkbin: ZkBinary,
    pub redeem_pk: ProvingKey,
}

impl EmergencyUnstakeCallBuilder {
    pub fn build(self) -> Result<EmergencyUnstakeCallDebris> {
        debug!(target: "contract::bearer_bond::client::emergency_unstake", "Building BearerBond::EmergencyUnstakeV1 contract call");

        let mut proofs = vec![];

        let value_blind = ScalarBlind::random(&mut OsRng);
        let token_id_blind = BaseBlind::random(&mut OsRng);
        let user_data_blind = BaseBlind::random(&mut OsRng);

        let (burn_proof, burn_revealed) = create_emergency_unstake_burn_proof(
            &self.burn_zkbin,
            &self.burn_pk,
            &self.input,
            value_blind.clone(),
            token_id_blind.clone(),
            user_data_blind.clone(),
        )?;

        proofs.push(burn_proof);

        let bond_input = BondInput {
            value_commit: burn_revealed.value_commit,
            token_commit: burn_revealed.token_commit,
            nullifier: burn_revealed.nullifier,
            merkle_root: burn_revealed.merkle_root,
            user_data_enc: burn_revealed.user_data_enc,
            spend_hook: self.input.spend_hook,
            signature_public: burn_revealed.signature_public,
        };

        // Build Redeem_V1 proof for the zero-value receipt coin
        let receipt_value_blind = ScalarBlind::random(&mut OsRng);
        let receipt_token_id_blind = BaseBlind::random(&mut OsRng);

        let (receipt_proof, _receipt_revealed) = create_emergency_unstake_receipt_proof(
            &self.redeem_zkbin,
            &self.redeem_pk,
            &self.output,
            receipt_value_blind,
            receipt_token_id_blind,
        )?;

        proofs.push(receipt_proof);

        Ok(EmergencyUnstakeCallDebris {
            params: EmergencyUnstakeParamsV1 {
                bond_input,
                coverage_report: self.input.coverage_report,
            },
            proofs,
        })
    }
}

fn create_emergency_unstake_burn_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &EmergencyUnstakeCallInput,
    value_blind: ScalarBlind,
    token_id_blind: BaseBlind,
    user_data_blind: BaseBlind,
) -> Result<(Proof, EmergencyUnstakeBurnRevealed)> {
    let public_key = poseidon_hash([input.secret]);

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
    let token_commit = poseidon_hash([input.token_id, token_id_blind.inner()]);
    let user_data_enc = poseidon_hash([input.user_data, user_data_blind.inner()]);
    let signature_public = poseidon_hash([input.ephemeral_signature_secret]);

    let public_inputs = EmergencyUnstakeBurnRevealed {
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

fn create_emergency_unstake_receipt_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    output: &EmergencyUnstakeCallOutput,
    value_blind: ScalarBlind,
    token_id_blind: BaseBlind,
) -> Result<(Proof, EmergencyUnstakeReceiptRevealed)> {
    let coin_value = pallas::Base::zero();
    let attrs = CoinAttributes {
        public_key: output.recipient,
        value: 0,
        token_id: output.token_id,
        spend_hook: output.spend_hook,
        user_data: output.user_data,
        blind: output.coin_blind,
        maturity_block: 0,
    };
    let coin = attrs.to_coin();

    let value_commit = pedersen_commitment_u64(0, value_blind.clone());
    let token_commit = poseidon_hash([output.token_id, token_id_blind.inner()]);

    let public_inputs = EmergencyUnstakeReceiptRevealed {
        coin,
        value_commit,
        token_commit,
        coin_value,
        spend_hook: output.spend_hook,
        tx_binding: pallas::Base::zero(),
        tx_nonce: pallas::Base::zero(),
    };

    let prover_witnesses = vec![
        Witness::Base(Value::known(output.recipient)),
        Witness::Base(Value::known(coin_value)),
        Witness::Base(Value::known(output.token_id)),
        Witness::Base(Value::known(output.spend_hook)),
        Witness::Base(Value::known(output.user_data)),
        Witness::Base(Value::known(output.coin_blind)),
        Witness::Scalar(Value::known(value_blind.inner())),
        Witness::Base(Value::known(token_id_blind.inner())),
        Witness::Base(Value::known(pallas::Base::zero())), // tx_commitment
        Witness::Base(Value::known(pallas::Base::zero())), // tx_nonce
        Witness::Base(Value::known(pallas::Base::zero())), // tx_binding
    ];

    let circuit = ZkCircuit::new(prover_witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}
