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

//! Tender select_winner_v1 ZK proof generation

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

/// SelectWinnerV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct SelectWinnerV1PublicInputs {
    pub tender_id: pallas::Base,
    pub winner_bid_id: pallas::Base,
    pub requester_pub_x: pallas::Base,
    pub requester_pub_y: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl SelectWinnerV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.tender_id,
            self.winner_bid_id,
            self.requester_pub_x,
            self.requester_pub_y,
            self.tx_binding,
            self.tx_nonce,
        ]
    }
}

/// Input data for select_winner proof generation
#[derive(Debug, Clone)]
pub struct SelectWinnerV1CallData {
    pub tender_id: pallas::Base,
    pub winner_bid_id: pallas::Base,
    pub requester_secret: pallas::Base,
    // Public inputs
    pub requester_public: PublicKey,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl SelectWinnerV1CallData {
    pub fn new(
        tender_id: pallas::Base,
        winner_bid_id: pallas::Base,
        requester_secret: pallas::Base,
        requester_public: PublicKey,
    ) -> Self {
        Self { tender_id, winner_bid_id, requester_secret, requester_public, tx_commitment: pallas::Base::zero(), tx_nonce: pallas::Base::zero() }
    }

    pub fn compute_public_inputs(&self) -> SelectWinnerV1PublicInputs {
        #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy() is always Some")]
        let (ix, iy) = self.requester_public.xy().expect("pk not identity");
        SelectWinnerV1PublicInputs {
            tender_id: self.tender_id,
            winner_bid_id: self.winner_bid_id,
            requester_pub_x: ix,
            requester_pub_y: iy,
            tx_binding: pallas::Base::zero(),
            tx_nonce: self.tx_nonce,
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy() is always Some")]
        let (ix, iy) = self.requester_public.xy().expect("pk not identity");
        vec![
            // Must match circuit witness order:
            // tender_id, winner_bid_id, requester_secret, requester_pub_x, requester_pub_y
            Witness::Base(Value::known(self.tender_id)),
            Witness::Base(Value::known(self.winner_bid_id)),
            Witness::Base(Value::known(self.requester_secret)),
            Witness::Base(Value::known(ix)),
            Witness::Base(Value::known(iy)),
            // tx_commitment, tx_nonce, tx_binding
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            Witness::Base(Value::known(pallas::Base::zero())), // tx_binding
        ]
    }
}

/// Create a SelectWinner ZK proof
pub fn select_winner_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &SelectWinnerV1CallData,
) -> Result<(Proof, SelectWinnerV1PublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}