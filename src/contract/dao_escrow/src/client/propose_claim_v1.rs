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

//! DAO-Escrow ProposeClaim ZK proof generation

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    crypto::{poseidon_hash, PublicKey},
    pasta::pallas,
};
use rand::rngs::OsRng;

/// ProposeClaimV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct ProposeClaimV1PublicInputs {
    pub dao_escrow_bulla: pallas::Base,
    pub claim_id: pallas::Base,
    pub capability_id: pallas::Base,
    pub proposal_nullifier: pallas::Base,
    pub claim_commit: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl ProposeClaimV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.dao_escrow_bulla,
            self.claim_id,
            self.capability_id,
            self.proposal_nullifier,
            self.claim_commit,
            self.tx_binding,
            self.tx_nonce,
        ]
    }
}

/// Input data for ProposeClaim proof generation
#[derive(Debug, Clone)]
pub struct ProposeClaimV1CallData {
    pub nullifier_k: pallas::Scalar,
    pub dao_escrow_bulla: pallas::Base,
    pub claim_id: pallas::Base,
    pub capability_id: pallas::Base,
    pub capability_secret: pallas::Base,
    pub proposer_secret: pallas::Base,
    pub value: u64,
    pub description_hash: pallas::Base,
    pub recipient_pub_x: pallas::Base,
    pub recipient_pub_y: pallas::Base,
    pub proposal_blind: pallas::Base,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl ProposeClaimV1CallData {
    pub fn new(
        nullifier_k: pallas::Scalar,
        dao_escrow_bulla: pallas::Base,
        claim_id: pallas::Base,
        capability_id: pallas::Base,
        capability_secret: pallas::Base,
        proposer_secret: pallas::Base,
        value: u64,
        description_hash: pallas::Base,
        recipient_pubkey: PublicKey,
        proposal_blind: pallas::Base,
    ) -> Self {
        let (rx, ry) = recipient_pubkey.xy();
        Self {
            nullifier_k,
            dao_escrow_bulla,
            claim_id,
            capability_id,
            capability_secret,
            proposer_secret,
            value,
            description_hash,
            recipient_pub_x: rx,
            recipient_pub_y: ry,
            proposal_blind,
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        }
    }

    pub fn compute_public_inputs(&self) -> ProposeClaimV1PublicInputs {
        let proposal_nullifier = poseidon_hash([
            self.capability_secret,
            self.dao_escrow_bulla,
            self.claim_id,
        ]);

        let claim_commit = poseidon_hash([
            self.dao_escrow_bulla,
            self.claim_id,
            pallas::Base::from(self.value),
            self.description_hash,
            self.recipient_pub_x,
            self.recipient_pub_y,
        ]);

        ProposeClaimV1PublicInputs {
            dao_escrow_bulla: self.dao_escrow_bulla,
            claim_id: self.claim_id,
            capability_id: self.capability_id,
            proposal_nullifier,
            claim_commit,
            tx_binding: pallas::Base::zero(),
            tx_nonce: self.tx_nonce,
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        vec![
            Witness::Base(Value::known(self.dao_escrow_bulla)),
            Witness::Base(Value::known(self.claim_id)),
            Witness::Base(Value::known(self.capability_id)),
            Witness::Base(Value::known(self.capability_secret)),
            Witness::Base(Value::known(self.proposer_secret)),
            Witness::Base(Value::known(pallas::Base::from(self.value))),
            Witness::Base(Value::known(self.description_hash)),
            Witness::Base(Value::known(self.recipient_pub_x)),
            Witness::Base(Value::known(self.recipient_pub_y)),
            Witness::Base(Value::known(self.proposal_blind)),
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            Witness::Base(Value::known(pallas::Base::zero())), // tx_binding
        ]
    }
}

/// Create a ProposeClaim ZK proof
pub fn propose_claim_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &ProposeClaimV1CallData,
) -> Result<(Proof, ProposeClaimV1PublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}
