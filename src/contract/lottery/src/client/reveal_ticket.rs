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

//! Lottery reveal_ticket_v1 ZK proof generation (RevealTicketV2 circuit).
//!
//! Circuit witness (8): lottery_id, ticket_secret, ticket_pub_x, ticket_pub_y, ticket_number,
//! tx_commitment, tx_nonce, tx_binding.
//! Only `tx_binding`/`tx_nonce` are constrained — the numbers→commitment binding is OFF-circuit
//! (in `reveal_ticket.rs` exec). instances (2): tx_binding, tx_nonce.

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    crypto::poseidon_hash,
    pasta::pallas,
};
use rand::rngs::OsRng;
use rand::SeedableRng;

/// RevealTicketV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct RevealTicketV1PublicInputs {
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl RevealTicketV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![self.tx_binding, self.tx_nonce]
    }
}

/// Input data for reveal_ticket proof generation
#[derive(Debug, Clone)]
pub struct RevealTicketV1CallData {
    pub lottery_id: pallas::Base,
    pub ticket_secret: pallas::Base,
    pub ticket_pub_x: pallas::Base,
    pub ticket_pub_y: pallas::Base,
    pub ticket_number: pallas::Base,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl RevealTicketV1CallData {
    pub fn new() -> Self {
        Self {
            lottery_id: pallas::Base::zero(),
            ticket_secret: pallas::Base::zero(),
            ticket_pub_x: pallas::Base::zero(),
            ticket_pub_y: pallas::Base::zero(),
            ticket_number: pallas::Base::zero(),
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        }
    }

    pub fn compute_public_inputs(&self) -> RevealTicketV1PublicInputs {
        RevealTicketV1PublicInputs {
            tx_binding: poseidon_hash([
                pallas::Base::from(3u64),
                self.tx_commitment,
                self.tx_nonce,
            ]),
            tx_nonce: self.tx_nonce,
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        vec![
            Witness::Base(Value::known(self.lottery_id)),
            Witness::Base(Value::known(self.ticket_secret)),
            Witness::Base(Value::known(self.ticket_pub_x)),
            Witness::Base(Value::known(self.ticket_pub_y)),
            Witness::Base(Value::known(self.ticket_number)),
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            Witness::Base(Value::known(poseidon_hash([
                pallas::Base::from(3u64),
                self.tx_commitment,
                self.tx_nonce,
            ]))), // tx_binding
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
    #[cfg(not(target_arch = "wasm32"))]
    let proof = if crate::deterministic_zk_enabled() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(0);
        Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut rng)?
    } else {
        Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?
    };
    #[cfg(target_arch = "wasm32")]
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}
