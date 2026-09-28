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

//! Tender submit_bid_v1 ZK proof generation

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    crypto::{poseidon_hash, PublicKey},
    pasta::pallas,
};
use rand::{rngs::OsRng, SeedableRng};

/// SubmitBidV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct SubmitBidV1PublicInputs {
    pub tender_id: pallas::Base,
    pub bid_id: pallas::Base,
    pub bidder_pub_x: pallas::Base,
    pub bidder_pub_y: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl SubmitBidV1PublicInputs {
    /// **The order is the circuit's, and it was not.**
    ///
    /// `submit_bid.zk` constrains six values and the order it constrains them in is
    /// `bidder_pub_x`, `bidder_pub_y`, `tender_id`, `bid_id`, `tx_binding`, `tx_nonce`
    /// (`:76`, `:77`, `:100`, `:101`, `:115`, `:116`) — which is also the order the contract's
    /// metadata arm pushes, and its own comment says so. This vector emitted `tender_id` and
    /// `bid_id` **first** and the two key coordinates after — the same six values, permuted — so
    /// the proof was verified against a different instance vector than the one it was built for and
    /// every `submit_bid` failed `InvalidProof`.
    ///
    /// Localised by `test-harness/tests/client_proof_self_verification.rs`'s
    /// `submit_bid_proof_verifies_against_its_own_circuit`, which builds this client's proof and
    /// verifies it against the same `.zk.bin` the contract embeds, with this vector's instances and
    /// no host involved — `create_tender`'s and `slot`'s cases passing in the same run is what
    /// showed the defect was here rather than in a test that could never pass.
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.bidder_pub_x,
            self.bidder_pub_y,
            self.tender_id,
            self.bid_id,
            self.tx_binding,
            self.tx_nonce,
        ]
    }
}

/// Input data for submit_bid proof generation
#[derive(Debug, Clone)]
pub struct SubmitBidV1CallData {
    pub tender_id: pallas::Base,
    pub bidder_secret: pallas::Base,
    pub amount: pallas::Base,
    pub bid_nonce: pallas::Base,
    // Public inputs
    pub bidder_public: PublicKey,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl SubmitBidV1CallData {
    pub fn new(
        tender_id: pallas::Base,
        bidder_secret: pallas::Base,
        amount: pallas::Base,
        bid_nonce: pallas::Base,
        bidder_public: PublicKey,
    ) -> Self {
        Self { tender_id, bidder_secret, amount, bid_nonce, bidder_public, tx_commitment: pallas::Base::zero(), tx_nonce: pallas::Base::zero() }
    }

    /// Compute bid ID from bid parameters
    pub fn compute_bid_id(&self) -> pallas::Base {
        #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy() is always Some")]
        let (ix, iy) = self.bidder_public.xy().expect("pk not identity");
        poseidon_hash([pallas::Base::from(4), self.tender_id, ix, iy, self.amount, self.bid_nonce])
    }

    pub fn compute_public_inputs(&self) -> SubmitBidV1PublicInputs {
        #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy() is always Some")]
        let (ix, iy) = self.bidder_public.xy().expect("pk not identity");
        SubmitBidV1PublicInputs {
            tender_id: self.tender_id,
            bid_id: self.compute_bid_id(),
            bidder_pub_x: ix,
            bidder_pub_y: iy,
            tx_binding: super::tx_binding_of(&self.tx_commitment, &self.tx_nonce),
            tx_nonce: self.tx_nonce,
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy() is always Some")]
        let (ix, iy) = self.bidder_public.xy().expect("pk not identity");
        vec![
            // Must match circuit witness order:
            // tender_id, bidder_secret, bidder_pub_x, bidder_pub_y, amount, bid_nonce
            // (bid_id is computed by the circuit, not provided as witness)
            Witness::Base(Value::known(self.tender_id)),
            Witness::Base(Value::known(self.bidder_secret)),
            Witness::Base(Value::known(ix)),
            Witness::Base(Value::known(iy)),
            Witness::Base(Value::known(self.amount)),
            Witness::Base(Value::known(self.bid_nonce)),
            // tx_commitment, tx_nonce, tx_binding
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            // OBL-C78: the circuit *assigns* `tx_binding` —
            // `tx_binding = poseidon_hash(DOMAIN_TX_BINDING, tx_commitment, tx_nonce)` — and an
            // assignment to a declared witness constrains it rather than shadowing it, so the zero
            // that stood here was a constraint no proof could satisfy. `select_winner.rs` carries the
            // same note with the full reasoning; this is the value `compute_public_inputs` publishes.
            Witness::Base(Value::known(super::tx_binding_of(&self.tx_commitment, &self.tx_nonce))), // tx_binding
        ]
    }
}

/// Create a SubmitBid ZK proof
pub fn submit_bid_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &SubmitBidV1CallData,
) -> Result<(Proof, SubmitBidV1PublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    // As `create_tender.rs`: `OsRng` unless deterministic mode is on.
    let proof = if crate::deterministic_zk_enabled() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(0);
        Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut rng)?
    } else {
        Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?
    };

    Ok((proof, public_inputs))
}