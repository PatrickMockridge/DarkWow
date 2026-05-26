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

//! AcceptSwap ZK proof generation

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    crypto::{poseidon_hash, pasta_prelude::Field, PublicKey, SecretKey},
    pasta::pallas,
};
use rand::rngs::OsRng;

/// AcceptSwap circuit public inputs (in order of constrain_instance)
#[derive(Debug, Clone)]
pub struct AcceptSwapPublicInputs {
    /// Acceptor's lock commitment = poseidon_hash([secret, offer_token, offer_amount, token_blind, amount_blind])
    pub acceptor_lock_commitment: pallas::Base,
    /// Acceptor's nullifier = poseidon_hash([secret, acceptor_lock])
    pub acceptor_nullifier: pallas::Base,
    /// Signature public key X coordinate
    pub signature_public_x: pallas::Base,
    /// Signature public key Y coordinate
    pub signature_public_y: pallas::Base,
}

impl AcceptSwapPublicInputs {
    /// Convert to vector for ZK proof creation
    /// Order must match constrain_instance calls in accept_swap_v1.zk
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.acceptor_lock_commitment,
            self.acceptor_nullifier,
            self.signature_public_x,
            self.signature_public_y,
        ]
    }
}

/// Input data for AcceptSwap proof generation
#[derive(Debug, Clone)]
pub struct AcceptSwapCallData {
    /// Swap ID being accepted
    pub swap_id: pallas::Base,
    /// Proposer's lock commitment (for verification)
    pub proposer_lock_commitment: pallas::Base,
    /// Acceptor's secret for their lock
    pub acceptor_secret: pallas::Base,
    /// Token being offered
    pub offer_token: pallas::Base,
    /// Amount being offered
    pub offer_amount: pallas::Base,
    /// Blinding factor for token
    pub token_blind: pallas::Base,
    /// Blinding factor for amount
    pub amount_blind: pallas::Base,
    /// Secret key for signature
    pub signature_secret: SecretKey,
    /// Signature public key (derived from signature_secret)
    pub signature_public: PublicKey,
}

impl AcceptSwapCallData {
    /// Create new call data with random blinds
    pub fn new(
        swap_id: pallas::Base,
        proposer_lock_commitment: pallas::Base,
        acceptor_secret: pallas::Base,
        offer_token: pallas::Base,
        offer_amount: u64,
        signature_secret: SecretKey,
    ) -> Self {
        let signature_public = PublicKey::from_secret(signature_secret);
        let token_blind = pallas::Base::random(&mut OsRng);
        let amount_blind = pallas::Base::random(&mut OsRng);

        Self {
            swap_id,
            proposer_lock_commitment,
            acceptor_secret,
            offer_token,
            offer_amount: pallas::Base::from(offer_amount),
            token_blind,
            amount_blind,
            signature_secret,
            signature_public,
        }
    }

    /// Compute public inputs for this call
    pub fn compute_public_inputs(&self) -> AcceptSwapPublicInputs {
        // Compute acceptor's lock commitment
        // lock = poseidon_hash([secret, offer_token, offer_amount, token_blind, amount_blind])
        let acceptor_lock_commitment = poseidon_hash([
            self.acceptor_secret,
            self.offer_token,
            self.offer_amount,
            self.token_blind,
            self.amount_blind,
        ]);

        // Compute acceptor's nullifier
        // nullifier = poseidon_hash([secret, lock])
        let acceptor_nullifier = poseidon_hash([self.acceptor_secret, acceptor_lock_commitment]);

        // Get signature public key coordinates
        let (sig_x, sig_y) = self.signature_public.xy();

        AcceptSwapPublicInputs {
            acceptor_lock_commitment,
            acceptor_nullifier,
            signature_public_x: sig_x,
            signature_public_y: sig_y,
        }
    }

    /// Generate prover witnesses for the circuit
    pub fn to_witnesses(&self) -> Vec<Witness> {
        vec![
            // Base swap_id
            Witness::Base(Value::known(self.swap_id)),
            // Base proposer_lock_commitment
            Witness::Base(Value::known(self.proposer_lock_commitment)),
            // Base acceptor_secret
            Witness::Base(Value::known(self.acceptor_secret)),
            // Base offer_token
            Witness::Base(Value::known(self.offer_token)),
            // Base offer_amount
            Witness::Base(Value::known(self.offer_amount)),
            // Base token_blind
            Witness::Base(Value::known(self.token_blind)),
            // Base amount_blind
            Witness::Base(Value::known(self.amount_blind)),
            // Base signature_secret
            Witness::Base(Value::known(self.signature_secret.inner())),
            // Base signature_public_x
            Witness::Base(Value::known(self.signature_public.x())),
            // Base signature_public_y
            Witness::Base(Value::known(self.signature_public.y())),
        ]
    }
}

/// Create an AcceptSwap ZK proof
///
/// # Arguments
///
/// * `zkbin` - The compiled ZK binary for AcceptSwap circuit
/// * `pk` - The proving key for the circuit
/// * `input` - The call data containing secrets and parameters
///
/// # Returns
///
/// * `(Proof, AcceptSwapPublicInputs)` - The ZK proof and public inputs
pub fn create_accept_swap_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &AcceptSwapCallData,
) -> Result<(Proof, AcceptSwapPublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    //dwow_core::zk::export_witness_json("proof/witness/accept_swap_v1.json", &witnesses, &public_inputs.to_vec());
    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}
