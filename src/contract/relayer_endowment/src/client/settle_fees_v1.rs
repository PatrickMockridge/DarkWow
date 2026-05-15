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

//! Relayer Endowment settle_fees_v1 proof generation
//!
//! SettleFees v0.1 uses signature-based authentication (no ZK circuit yet).
//! The relayer signs the allocation list with their keypair, and the contract
//! verifies signature_public matches relayer_pub.

use dwow::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    crypto::PublicKey,
    pasta::pallas,
};
use rand::rngs::OsRng;

/// SettleFeesV1 public inputs
#[derive(Debug, Clone)]
pub struct SettleFeesV1PublicInputs {
    pub relayer_pub_x: pallas::Base,
    pub relayer_pub_y: pallas::Base,
    pub total_fees: pallas::Base,
}

impl SettleFeesV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![self.relayer_pub_x, self.relayer_pub_y, self.total_fees]
    }
}

/// Input data for settle_fees proof generation
#[derive(Debug, Clone)]
pub struct SettleFeesV1CallData {
    pub relayer_pub_x: pallas::Base,
    pub relayer_pub_y: pallas::Base,
    pub total_fees: pallas::Base,
    pub total_fees_u64: u64,
}

impl SettleFeesV1CallData {
    pub fn new(
        relayer_public: PublicKey,
        total_fees: u64,
    ) -> Self {
        let (rx, ry) = relayer_public.xy();
        Self {
            relayer_pub_x: rx,
            relayer_pub_y: ry,
            total_fees: pallas::Base::from(total_fees),
            total_fees_u64: total_fees,
        }
    }

    pub fn compute_public_inputs(&self) -> SettleFeesV1PublicInputs {
        SettleFeesV1PublicInputs {
            relayer_pub_x: self.relayer_pub_x,
            relayer_pub_y: self.relayer_pub_y,
            total_fees: self.total_fees,
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        vec![
            Witness::Base(Value::known(self.relayer_pub_x)),
            Witness::Base(Value::known(self.relayer_pub_y)),
            Witness::Base(Value::known(self.total_fees)),
        ]
    }
}

/// Create a SettleFees proof
pub fn settle_fees_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &SettleFeesV1CallData,
) -> Result<(Proof, SettleFeesV1PublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}
