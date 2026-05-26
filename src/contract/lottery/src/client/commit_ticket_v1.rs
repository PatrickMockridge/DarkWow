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

//! Lottery commit_ticket_v1 ZK proof generation

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    crypto::{pasta_prelude::PrimeField, poseidon_hash, PublicKey},
    pasta::pallas,
};
use rand::rngs::OsRng;

/// CommitTicketV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct CommitTicketV1PublicInputs {
    pub ticket_id: pallas::Base,
    pub value_commit_x: pallas::Base,
    pub value_commit_y: pallas::Base,
}

impl CommitTicketV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![self.ticket_id, self.value_commit_x, self.value_commit_y]
    }
}

/// Input data for commit_ticket proof generation
#[derive(Debug, Clone)]
pub struct CommitTicketV1CallData {
    pub player_pub_x: pallas::Base,
    pub player_pub_y: pallas::Base,
    pub ticket_price: pallas::Base,
    pub secret_nonce: pallas::Base,
    pub blind: pallas::Base,
    pub token_id: pallas::Base,
}

impl CommitTicketV1CallData {
    pub fn new(
        player_pub: PublicKey,
        ticket_price: u64,
        secret_nonce: pallas::Base,
        blind: pallas::Base,
        token_id: pallas::Base,
    ) -> Self {
        let (px, py) = player_pub.xy();
        Self {
            player_pub_x: px,
            player_pub_y: py,
            ticket_price: pallas::Base::from(ticket_price),
            secret_nonce,
            blind,
            token_id,
        }
    }

    pub fn compute_public_inputs(&self) -> CommitTicketV1PublicInputs {
        let ticket_id = poseidon_hash([
            self.player_pub_x,
            self.player_pub_y,
            self.secret_nonce,
            self.token_id,
            self.ticket_price,
        ]);
        CommitTicketV1PublicInputs { ticket_id, value_commit_x: pallas::Base::zero(), value_commit_y: pallas::Base::zero() }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        let ticket_id = poseidon_hash([
            self.player_pub_x,
            self.player_pub_y,
            self.secret_nonce,
            self.token_id,
            self.ticket_price,
        ]);
        let blind_bytes = self.blind.to_repr();
        let value_blind = pallas::Scalar::from_repr(blind_bytes).unwrap_or(pallas::Scalar::zero());
        vec![
            Witness::Base(Value::known(self.player_pub_x)),
            Witness::Base(Value::known(self.player_pub_y)),
            Witness::Base(Value::known(ticket_id)),
            Witness::Base(Value::known(self.secret_nonce)),
            Witness::Base(Value::known(self.token_id)),
            Witness::Base(Value::known(self.ticket_price)),
            Witness::Scalar(Value::known(value_blind)),
        ]
    }
}

/// Create a CommitTicket ZK proof
pub fn create_commit_ticket_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &CommitTicketV1CallData,
) -> Result<(Proof, CommitTicketV1PublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}