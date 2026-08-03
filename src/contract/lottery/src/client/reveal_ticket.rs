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

//! Lottery reveal_ticket_v1 ZK proof generation

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    crypto::PublicKey,
    pasta::pallas,
};
use rand::rngs::OsRng;

/// RevealTicketV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct RevealTicketV1PublicInputs {
    pub ticket_id: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl RevealTicketV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![self.ticket_id, self.tx_binding, self.tx_nonce]
    }
}

/// Input data for reveal_ticket proof generation
#[derive(Debug, Clone)]
pub struct RevealTicketV1CallData {
    pub player_pub_x: pallas::Base,
    pub player_pub_y: pallas::Base,
    pub ticket_price: pallas::Base,
    pub secret_nonce: pallas::Base,
    pub blind: pallas::Base,
    pub nonce: pallas::Base,
    pub random: pallas::Base,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl RevealTicketV1CallData {
    pub fn new(
        player_pub: PublicKey,
        ticket_price: u64,
        secret_nonce: pallas::Base,
        blind: pallas::Base,
        nonce: pallas::Base,
        random: pallas::Base,
    ) -> Self {
        let (px, py) = player_pub.xy().expect("pk not identity");
        Self {
            player_pub_x: px,
            player_pub_y: py,
            ticket_price: pallas::Base::from(ticket_price),
            secret_nonce,
            blind,
            nonce,
            random,
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        }
    }

    pub fn compute_public_inputs(&self) -> RevealTicketV1PublicInputs {
        RevealTicketV1PublicInputs { ticket_id: pallas::Base::zero(), tx_binding: pallas::Base::zero(), tx_nonce: self.tx_nonce }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        vec![
            // Public inputs as witnesses
            Witness::Base(Value::known(self.player_pub_x)),
            Witness::Base(Value::known(self.player_pub_y)),
            Witness::Base(Value::known(self.ticket_price)),
            // Private inputs
            Witness::Base(Value::known(self.secret_nonce)),
            Witness::Base(Value::known(self.blind)),
            Witness::Base(Value::known(self.nonce)),
            Witness::Base(Value::known(self.random)),
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            Witness::Base(Value::known(pallas::Base::zero())), // tx_binding
        ]
    }
}

/// Create a RevealTicket ZK proof
pub fn create_reveal_ticket_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &RevealTicketV1CallData,
) -> Result<(Proof, RevealTicketV1PublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}