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

//! Lottery commit_ticket_v1 ZK proof generation (CommitTicketV2 circuit).
//!
//! Circuit witness (9): lottery_id, ticket_secret, ticket_pub_x, ticket_pub_y, amount, nonce,
//! tx_commitment, tx_nonce, tx_binding.
//! `computed_ticket_id = poseidon_hash(4, lottery_id, ticket_pub_x, ticket_pub_y, amount, nonce)`.
//! instances (3): computed_ticket_id, tx_binding, tx_nonce.
//! (`ticket_secret` is a declared witness but is not constrained in the circuit body.)

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
use rand::SeedableRng;

/// CommitTicketV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct CommitTicketV1PublicInputs {
    pub ticket_id: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl CommitTicketV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![self.ticket_id, self.tx_binding, self.tx_nonce]
    }
}

/// Input data for commit_ticket proof generation
#[derive(Debug, Clone)]
pub struct CommitTicketV1CallData {
    pub lottery_id: pallas::Base,
    pub ticket_secret: pallas::Base,
    pub ticket_pub_x: pallas::Base,
    pub ticket_pub_y: pallas::Base,
    pub amount: pallas::Base,
    pub nonce: pallas::Base,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl CommitTicketV1CallData {
    pub fn new(
        lottery_id: pallas::Base,
        player_pub: PublicKey,
        amount: u64,
        nonce: pallas::Base,
    ) -> Self {
        #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy()/x()/y() is always Some")]
        let (px, py) = player_pub.xy().expect("pk not identity");
        Self {
            lottery_id,
            ticket_secret: pallas::Base::zero(),
            ticket_pub_x: px,
            ticket_pub_y: py,
            amount: pallas::Base::from(amount),
            nonce,
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        }
    }

    pub fn compute_public_inputs(&self) -> CommitTicketV1PublicInputs {
        let ticket_id = poseidon_hash([
            pallas::Base::from(4u64),
            self.lottery_id,
            self.ticket_pub_x,
            self.ticket_pub_y,
            self.amount,
            self.nonce,
        ]);
        CommitTicketV1PublicInputs {
            ticket_id,
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
            Witness::Base(Value::known(self.amount)),
            Witness::Base(Value::known(self.nonce)),
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

/// Create a CommitTicket ZK proof
pub fn create_commit_ticket_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &CommitTicketV1CallData,
) -> Result<(Proof, CommitTicketV1PublicInputs)> {
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
