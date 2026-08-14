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

//! Escrow cancel_v1 ZK proof generation
//!
//! CancelV1 uses a ZK proof (CancelEscrow circuit) to prove the buyer knows
//! their secret key matching the escrow's stored buyer_pubkey, without exposing
//! the secret or requiring on-chain Schnorr signature verification.
//!
//! Replaces the previous Schnorr signature pattern with ZK proof authentication.

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

/// CancelEscrowV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct CancelEscrowPublicInputs {
    pub escrow_id: pallas::Base,
    pub buyer_pub_x: pallas::Base,
    pub buyer_pub_y: pallas::Base,
    pub cancel_nullifier: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl CancelEscrowPublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.escrow_id,
            self.buyer_pub_x,
            self.buyer_pub_y,
            self.tx_binding,
            self.tx_nonce,
            self.cancel_nullifier,
        ]
    }
}

/// Input data for cancel proof generation
#[derive(Debug, Clone)]
pub struct CancelEscrowCallData {
    pub buyer_secret: pallas::Base,
    // Public inputs
    pub buyer_public: PublicKey,
    pub escrow_id: pallas::Base,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl CancelEscrowCallData {
    pub fn new(
        buyer_secret: pallas::Base,
        buyer_public: PublicKey,
        escrow_id: pallas::Base,
    ) -> Self {
        Self {
            buyer_secret,
            buyer_public,
            escrow_id,
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        }
    }

    /// Compute cancel nullifier: H(DOMAIN_NULLIFIER, escrow_id, buyer_secret)
    pub fn compute_nullifier(&self) -> pallas::Base {
        poseidon_hash([pallas::Base::from(1u64), self.escrow_id, self.buyer_secret])
    }

    pub fn compute_public_inputs(&self) -> CancelEscrowPublicInputs {
        let (ix, iy) = self.buyer_public.xy().expect("pk not identity");
        CancelEscrowPublicInputs {
            escrow_id: self.escrow_id,
            buyer_pub_x: ix,
            buyer_pub_y: iy,
            cancel_nullifier: self.compute_nullifier(),
            tx_binding: pallas::Base::zero(),
            tx_nonce: self.tx_nonce,
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        let (ix, iy) = self.buyer_public.xy().expect("pk not identity");
        vec![
            // Must match circuit witness order:
            // escrow_id, buyer_secret, buyer_pub_x, buyer_pub_y
            Witness::Base(Value::known(self.escrow_id)),
            Witness::Base(Value::known(self.buyer_secret)),
            Witness::Base(Value::known(ix)),
            Witness::Base(Value::known(iy)),
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            Witness::Base(Value::known(pallas::Base::zero())), // tx_binding
        ]
    }
}

/// Create a CancelEscrow ZK proof
pub fn create_cancel_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &CancelEscrowCallData,
) -> Result<(Proof, CancelEscrowPublicInputs)> {
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

/// Builder for constructing a CancelEscrowV1 call.
pub struct CancelEscrowV1Builder {
    escrow_id: pallas::Base,
    buyer_pubkey: PublicKey,
    buyer_secret: pallas::Base,
    cancel_zkbin: ZkBinary,
    cancel_pk: ProvingKey,
}

impl CancelEscrowV1Builder {
    pub fn new(
        escrow_id: pallas::Base,
        buyer_pubkey: PublicKey,
        buyer_secret: pallas::Base,
        cancel_zkbin: ZkBinary,
        cancel_pk: ProvingKey,
    ) -> Self {
        Self { escrow_id, buyer_pubkey, buyer_secret, cancel_zkbin, cancel_pk }
    }

    pub fn build(self) -> Result<(Proof, CancelEscrowPublicInputs)> {
        let call_data = CancelEscrowCallData::new(
            self.buyer_secret,
            self.buyer_pubkey,
            self.escrow_id,
        );

        create_cancel_proof(&self.cancel_zkbin, &self.cancel_pk, &call_data)
    }
}
