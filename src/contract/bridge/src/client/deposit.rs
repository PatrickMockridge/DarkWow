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

//! DepositV1 ZK proof generation
//!
//! Bridge-core: the deposit proof binds the depositor's commitment. There is no
//! bridge-side Sinsemilla merkle proof — the external-chain deposit is verified
//! via `verify_chain_proof` (bridge-verify feature) and the wrapped promissory
//! note is issued by the child `promissory_note::issue_v1` call.

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    crypto::{poseidon_hash, PublicKey, SecretKey},
    pasta::pallas,
};
use rand::rngs::OsRng;
use rand::SeedableRng;

/// DepositV1 circuit public inputs (in order of constrain_instance)
#[derive(Debug, Clone)]
pub struct DepositPublicInputs {
    /// Commitment the user claims this deposit creates
    pub commitment: pallas::Base,
    /// Recipient's DarkWow public key X coordinate
    pub recipient_pub_x: pallas::Base,
    /// Recipient's DarkWow public key Y coordinate
    pub recipient_pub_y: pallas::Base,
    /// Fresh nonce for this deposit (unlinkability)
    pub bridge_nonce: pallas::Base,
    /// Hash of external chain block containing deposit
    pub external_block_hash: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl DepositPublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.commitment,
            self.tx_binding,
            self.tx_nonce,
        ]
    }
}

/// Input data for Deposit proof generation
#[derive(Debug, Clone)]
pub struct DepositCallData {
    /// User's secret for this deposit
    pub secret: pallas::Base,
    /// Deposit amount in external chain unit
    pub amount: u64,
    /// Recipient's public key on DarkWow
    pub recipient_public: PublicKey,
    /// Fresh nonce for temporal privacy
    pub bridge_nonce: u64,
    /// External block hash containing deposit
    pub external_block_hash: pallas::Base,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl DepositCallData {
    /// Create new call data
    pub fn new(
        secret: pallas::Base,
        amount: u64,
        recipient_public: PublicKey,
        bridge_nonce: u64,
        external_block_hash: pallas::Base,
    ) -> Self {
        Self {
            secret,
            amount,
            recipient_public,
            bridge_nonce,
            external_block_hash,
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        }
    }

    /// Derive bridge address from recipient identity and nonce
    pub fn derive_bridge_address(&self) -> pallas::Base {
        #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy()/x()/y() is always Some")]
        let (pub_x, pub_y) = self.recipient_public.xy().expect("pk not identity");
        let bridge_secret = poseidon_hash([pallas::Base::from(7u64), pub_x, pub_y, pallas::Base::from(self.bridge_nonce)]);
        let bridge_pub = PublicKey::from_secret(SecretKey::from_base(bridge_secret));
        #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy()/x()/y() is always Some")]
        let (bridge_pub_x, bridge_pub_y) = bridge_pub.xy().expect("pk not identity");
        poseidon_hash([pallas::Base::from(4u64), bridge_pub_x, bridge_pub_y])
    }

    /// Compute commitment: H(DOMAIN_COIN_COMMIT, secret, amount, bridge_address)
    pub fn compute_commitment(&self) -> pallas::Base {
        let bridge_address = self.derive_bridge_address();
        poseidon_hash([pallas::Base::from(4u64), self.secret, pallas::Base::from(self.amount), bridge_address])
    }

    /// Compute public inputs for this call
    pub fn compute_public_inputs(&self) -> DepositPublicInputs {
        let commitment = self.compute_commitment();
        #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy()/x()/y() is always Some")]
        let (recipient_pub_x, recipient_pub_y) = self.recipient_public.xy().expect("pk not identity");

        DepositPublicInputs {
            commitment,
            recipient_pub_x,
            recipient_pub_y,
            bridge_nonce: pallas::Base::from(self.bridge_nonce),
            external_block_hash: self.external_block_hash,
            tx_binding: poseidon_hash([pallas::Base::from(3u64), self.tx_commitment, self.tx_nonce]),
            tx_nonce: self.tx_nonce,
        }
    }

    /// Generate prover witnesses for the circuit.
    /// Order matches the deposit.zk witness block:
    ///   recipient_pub_x, recipient_pub_y, bridge_nonce, secret, amount,
    ///   tx_commitment, tx_nonce, tx_binding
    pub fn to_witnesses(&self) -> Vec<Witness> {
        let public_inputs = self.compute_public_inputs();

        vec![
            Witness::Base(Value::known(public_inputs.recipient_pub_x)),
            Witness::Base(Value::known(public_inputs.recipient_pub_y)),
            Witness::Base(Value::known(public_inputs.bridge_nonce)),
            Witness::Base(Value::known(self.secret)),
            Witness::Base(Value::known(pallas::Base::from(self.amount))),
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            Witness::Base(Value::known(poseidon_hash([pallas::Base::from(3u64), self.tx_commitment, self.tx_nonce]))), // tx_binding
        ]
    }
}

/// Create a Deposit ZK proof
pub fn create_deposit_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &DepositCallData,
) -> Result<(Proof, DepositPublicInputs)> {
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
