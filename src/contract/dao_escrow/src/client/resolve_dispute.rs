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

//! DAO-Escrow ResolveDispute ZK proof generation

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

/// ResolveDisputeV2 circuit public inputs (3 — matching V2 circuit constrain_instance order)
/// Circuit order: [tx_binding, tx_nonce, resolution_commit]
#[derive(Debug, Clone)]
pub struct ResolveDisputeV1PublicInputs {
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
    pub resolution_commit: pallas::Base,
}

impl ResolveDisputeV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![self.tx_binding, self.tx_nonce, self.resolution_commit]
    }
}

/// Input data for ResolveDispute proof generation
#[derive(Debug, Clone)]
pub struct ResolveDisputeV1CallData {
    pub nullifier_k: pallas::Scalar,
    pub capability_id: pallas::Base,
    pub dao_escrow_bulla: pallas::Base,
    pub dispute_id: pallas::Base,
    pub capability_secret: pallas::Base,
    pub arbitrator_secret: pallas::Base,
    pub attestation_count: u64,
    pub threshold: u64,
    pub resolution_result: bool,
    pub payout_amount: u64,
    pub recipient_pub_x: pallas::Base,
    pub recipient_pub_y: pallas::Base,
    pub attestation_root: pallas::Base,
    /// Arbitrator's public key coordinates — `ResolveDisputeV2` constrains both as witnesses and
    /// derives them in-circuit from `arbitrator_secret`.
    pub arbitrator_pub_x: pallas::Base,
    pub arbitrator_pub_y: pallas::Base,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl ResolveDisputeV1CallData {
    pub fn new(
        nullifier_k: pallas::Scalar,
        capability_id: pallas::Base,
        dao_escrow_bulla: pallas::Base,
        dispute_id: pallas::Base,
        capability_secret: pallas::Base,
        arbitrator_secret: pallas::Base,
        attestation_count: u64,
        threshold: u64,
        resolution_result: bool,
        payout_amount: u64,
        payout_recipient: PublicKey,
        attestation_root: pallas::Base,
    ) -> Self {
        #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy()/x()/y() is always Some")]
        let (rx, ry) = payout_recipient.xy().expect("pk not identity");
        let arbitrator_pub =
            PublicKey::from_secret(dwow_sdk::crypto::SecretKey::from_base(arbitrator_secret));
        #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy()/x()/y() is always Some")]
        let (ax, ay) = arbitrator_pub.xy().expect("pk not identity");
        Self {
            nullifier_k,
            capability_id,
            dao_escrow_bulla,
            dispute_id,
            capability_secret,
            arbitrator_secret,
            attestation_count,
            threshold,
            resolution_result,
            payout_amount,
            recipient_pub_x: rx,
            recipient_pub_y: ry,
            attestation_root,
            arbitrator_pub_x: ax,
            arbitrator_pub_y: ay,
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        }
    }

    pub fn compute_public_inputs(&self) -> ResolveDisputeV1PublicInputs {
        // resolution_commit = poseidon_hash(DOMAIN_COIN_COMMIT, dispute_id,
        //                                    resolution_type, resolution_blind)
        let resolution_commit = poseidon_hash([
            pallas::Base::from(4u64), // DOMAIN_COIN_COMMIT
            self.dispute_id,
            pallas::Base::from(self.resolution_result as u64),
            pallas::Base::from(self.payout_amount),
        ]);

        // Circuit constrain_instance order: [tx_binding, tx_nonce, resolution_commit]
        ResolveDisputeV1PublicInputs {
            tx_binding: self.compute_tx_binding(),
            tx_nonce: self.tx_nonce,
            resolution_commit,
        }
    }

    /// `tx_binding = poseidon_hash(DOMAIN_TX_BINDING, tx_commitment, tx_nonce)`, domain 3 — the
    /// value `ResolveDisputeV2` constrains at instance 1 (register OBL-C78).
    pub fn compute_tx_binding(&self) -> pallas::Base {
        poseidon_hash([pallas::Base::from(3u64), self.tx_commitment, self.tx_nonce])
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        // Must match `resolve_dispute.zk`'s `witness` block exactly:
        //   dao_escrow_bulla, arbitrator_secret, arbitrator_pub_x, arbitrator_pub_y, capability_id,
        //   capability_secret, dispute_id, resolution_type, resolution_blind, tx_commitment,
        //   tx_nonce, tx_binding
        // The circuit names two of these differently from this struct, and `compute_public_inputs`
        // above already pairs them the same way: `resolution_type` is `resolution_result as u64` and
        // `resolution_blind` is `payout_amount`. `attestation_count`, `threshold`,
        // `attestation_root` and the recipient coordinates are record fields exec needs — they were
        // five of the six extra entries (register OBL-C78).
        vec![
            Witness::Base(Value::known(self.dao_escrow_bulla)),
            Witness::Base(Value::known(self.arbitrator_secret)),
            Witness::Base(Value::known(self.arbitrator_pub_x)),
            Witness::Base(Value::known(self.arbitrator_pub_y)),
            Witness::Base(Value::known(self.capability_id)),
            Witness::Base(Value::known(self.capability_secret)),
            Witness::Base(Value::known(self.dispute_id)),
            Witness::Base(Value::known(pallas::Base::from(self.resolution_result as u64))),
            Witness::Base(Value::known(pallas::Base::from(self.payout_amount))),
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            Witness::Base(Value::known(self.compute_tx_binding())), // tx_binding
        ]
    }
}

/// Create a ResolveDispute ZK proof
pub fn resolve_dispute_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &ResolveDisputeV1CallData,
) -> Result<(Proof, ResolveDisputeV1PublicInputs)> {
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
