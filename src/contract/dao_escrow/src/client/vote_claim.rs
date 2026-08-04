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

//! DAO-Escrow VoteClaim ZK proof generation

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    crypto::{poseidon_hash, PublicKey, pasta_prelude::PrimeField},
    pasta::pallas,
};
use rand::rngs::OsRng;

/// VoteClaimV2 circuit public inputs (3 — matching VoteClaimV2 circuit constrain_instance order)
/// Circuit order: [tx_binding, tx_nonce, vote_nullifier]
#[derive(Debug, Clone)]
pub struct VoteClaimV1PublicInputs {
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
    pub vote_nullifier: pallas::Base,
}

impl VoteClaimV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![self.tx_binding, self.tx_nonce, self.vote_nullifier]
    }
}

/// Input data for VoteClaim proof generation
#[derive(Debug, Clone)]
pub struct VoteClaimV1CallData {
    pub nullifier_k: pallas::Scalar,
    pub vote_commit_value: pallas::Point,
    pub vote_commit_random: pallas::Point,
    pub proposal_id: pallas::Base,
    pub capability_id: pallas::Base,
    pub capability_secret: pallas::Base,
    pub voter_secret: pallas::Base,
    pub vote_type: pallas::Base,
    pub vote_blind: pallas::Scalar,
    pub voter_pub_x: pallas::Base,
    pub voter_pub_y: pallas::Base,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl VoteClaimV1CallData {
    pub fn new(
        nullifier_k: pallas::Scalar,
        vote_commit_value: pallas::Point,
        vote_commit_random: pallas::Point,
        proposal_id: pallas::Base,
        capability_id: pallas::Base,
        capability_secret: pallas::Base,
        voter_secret: pallas::Base,
        vote_yes: bool,
        vote_blind: pallas::Scalar,
    ) -> Self {
        let voter_pub = PublicKey::from_secret(
            dwow_sdk::crypto::SecretKey::from_bytes(voter_secret.to_repr()).unwrap()
        );
        let (vx, vy) = voter_pub.xy().expect("pk not identity");
        Self {
            nullifier_k,
            vote_commit_value,
            vote_commit_random,
            proposal_id,
            capability_id,
            capability_secret,
            voter_secret,
            vote_type: if vote_yes { pallas::Base::one() } else { pallas::Base::zero() },
            vote_blind,
            voter_pub_x: vx,
            voter_pub_y: vy,
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        }
    }

    pub fn compute_public_inputs(&self) -> VoteClaimV1PublicInputs {
        // vote_nullifier = poseidon_hash(DOMAIN_NULLIFIER, capability_secret,
        //                                 proposal_id, voter_pub_x, voter_pub_y)
        let vote_nullifier = poseidon_hash([
            pallas::Base::from(1u64), // DOMAIN_NULLIFIER
            self.capability_secret,
            self.proposal_id,
            self.voter_pub_x,
            self.voter_pub_y,
        ]);

        // Circuit constrain_instance order: [tx_binding, tx_nonce, vote_nullifier]
        VoteClaimV1PublicInputs {
            tx_binding: pallas::Base::zero(),
            tx_nonce: self.tx_nonce,
            vote_nullifier,
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        vec![
            Witness::Base(Value::known(self.proposal_id)),
            Witness::Base(Value::known(self.capability_id)),
            Witness::Base(Value::known(self.capability_secret)),
            Witness::Base(Value::known(self.voter_secret)),
            Witness::Base(Value::known(self.vote_type)),
            Witness::Scalar(Value::known(self.vote_blind)),
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            Witness::Base(Value::known(pallas::Base::zero())), // tx_binding
        ]
    }
}

/// Create a VoteClaim ZK proof
pub fn vote_claim_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &VoteClaimV1CallData,
) -> Result<(Proof, VoteClaimV1PublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}
