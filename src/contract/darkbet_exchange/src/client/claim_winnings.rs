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

//! DarkBet Exchange ClaimWinnings ZK proof generation

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

/// ClaimWinningsV1 circuit public inputs (only 1 - matching what circuit exposes)
#[derive(Debug, Clone)]
pub struct ClaimWinningsV1PublicInputs {
    pub derived_claim_id: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl ClaimWinningsV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![self.derived_claim_id, self.tx_binding, self.tx_nonce]
    }
}

/// Input data for ClaimWinnings proof generation
#[derive(Debug, Clone)]
pub struct ClaimWinningsV1CallData {
    pub market_id: pallas::Base,
    pub position_id: pallas::Base,
    pub owner_pub_x: pallas::Base,
    pub owner_pub_y: pallas::Base,
    pub winning_outcome: u8,
    pub block_height: u64,
    pub nonce: u64,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl ClaimWinningsV1CallData {
    pub fn new(
        market_id: pallas::Base,
        position_id: pallas::Base,
        owner_public: PublicKey,
        winning_outcome: u8,
        block_height: u64,
        nonce: u64,
    ) -> Self {
        let (ox, oy) = owner_public.xy().expect("pk not identity");
        Self {
            market_id,
            position_id,
            owner_pub_x: ox,
            owner_pub_y: oy,
            winning_outcome,
            block_height,
            nonce,
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        }
    }

    pub fn compute_public_inputs(&self) -> ClaimWinningsV1PublicInputs {
        let derived_claim_id = poseidon_hash([
            pallas::Base::from(4u64),
            self.market_id,
            self.position_id,
            self.owner_pub_x,
            self.owner_pub_y,
            pallas::Base::from(self.winning_outcome as u64),
            pallas::Base::from(self.block_height),
        ]);
        ClaimWinningsV1PublicInputs { derived_claim_id, tx_binding: pallas::Base::zero(), tx_nonce: self.tx_nonce }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        vec![
            // Public inputs as witnesses
            Witness::Base(Value::known(self.market_id)),
            Witness::Base(Value::known(self.position_id)),
            Witness::Base(Value::known(self.owner_pub_x)),
            Witness::Base(Value::known(self.owner_pub_y)),
            Witness::Base(Value::known(pallas::Base::from(self.winning_outcome as u64))),
            Witness::Base(Value::known(pallas::Base::from(self.block_height))),
            Witness::Base(Value::known(pallas::Base::from(self.nonce))),
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            Witness::Base(Value::known(pallas::Base::zero())), // tx_binding
        ]
    }
}

/// Create a ClaimWinnings ZK proof
pub fn claim_winnings_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &ClaimWinningsV1CallData,
) -> Result<(Proof, ClaimWinningsV1PublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}