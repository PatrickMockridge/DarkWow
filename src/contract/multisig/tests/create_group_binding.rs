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

//! `create_group.zk` binds the exposed `group_id` to the group it creates.
//!
//! Before 2026-10-22 the circuit witnessed `group_id`, `threshold` and `total_keys` as free values,
//! exposed them, and constrained nothing; the host recomputed `group_id` and ignored the circuit's
//! copy, so a proof over any bytes verified. The circuit now derives
//! `H(14, first_commitment, threshold, total_keys)` and constrains it equal to the instance. This
//! is the same prove+verify pattern as `drain_protection/tests/exit_client_proof.rs`: an
//! unsatisfied circuit still yields proof bytes, so the assertion is on verification, not on
//! `Proof::create` returning.

#![cfg(feature = "client")]

use dwow_core::zk::{halo2::Value, verify_zkp, Proof, ProvingKey, Witness, ZkCircuit, ZkVerifyResult};
use dwow_core::zkas::ZkBinary;
use dwow_sdk::crypto::poseidon_hash;
use dwow_sdk::pasta::pallas;

/// The zkbin the contract embeds (`entrypoint/mod.rs`: `zkas_db_set(include_bytes!(...))`).
const CREATE_GROUP_ZKBIN: &[u8] = include_bytes!("../proof/create_group.zk.bin");

const DOMAIN_TX_BINDING: u64 = 3;
const DOMAIN_GROUP_ID: u64 = 14;

#[test]
fn create_group_members_are_bound_to_the_exposed_group_id() {
    let zkbin = ZkBinary::decode(CREATE_GROUP_ZKBIN, false).expect("create_group.zk.bin decodes");
    let circuit = ZkCircuit::new(
        dwow_core::zk::empty_witnesses(&zkbin).expect("witnesses"),
        &zkbin,
    );
    let pk = ProvingKey::build(zkbin.k, &circuit).expect("ProvingKey::build");

    let threshold = pallas::Base::from(2u64);
    let total_keys = pallas::Base::from(3u64);
    let first_commitment = pallas::Base::from(11u64);
    let tx_commitment = pallas::Base::from(44u64);
    let tx_nonce = pallas::Base::from(55u64);
    let group_id = poseidon_hash([
        pallas::Base::from(DOMAIN_GROUP_ID),
        first_commitment,
        threshold,
        total_keys,
    ]);
    let tx_binding = poseidon_hash([
        pallas::Base::from(DOMAIN_TX_BINDING),
        tx_commitment,
        tx_nonce,
    ]);
    let public_inputs = vec![group_id, threshold, total_keys, tx_binding, tx_nonce];

    let honest = vec![
        Witness::Base(Value::known(group_id)),
        Witness::Base(Value::known(threshold)),
        Witness::Base(Value::known(total_keys)),
        Witness::Base(Value::known(first_commitment)),
        Witness::Base(Value::known(tx_commitment)),
        Witness::Base(Value::known(tx_nonce)),
    ];
    let proof = Proof::create(
        &pk,
        &[ZkCircuit::new(honest, &zkbin)],
        &public_inputs,
        rand::rngs::OsRng,
    )
    .expect("the honest witness proves");
    match verify_zkp(&proof, CREATE_GROUP_ZKBIN, &public_inputs) {
        ZkVerifyResult::Ok => {}
        other => panic!("the honest group proof must verify; got {other:?}"),
    }

    // The control. A different first member cannot produce the same group id, so the derivation
    // constraint is unsatisfiable for these public inputs. Before the constraint existed, this
    // proof verified: `group_id` was related to nothing.
    let wrong = vec![
        Witness::Base(Value::known(group_id)),
        Witness::Base(Value::known(threshold)),
        Witness::Base(Value::known(total_keys)),
        Witness::Base(Value::known(pallas::Base::from(12u64))),
        Witness::Base(Value::known(tx_commitment)),
        Witness::Base(Value::known(tx_nonce)),
    ];
    if let Ok(bad_proof) = Proof::create(
        &pk,
        &[ZkCircuit::new(wrong, &zkbin)],
        &public_inputs,
        rand::rngs::OsRng,
    ) {
        match verify_zkp(&bad_proof, CREATE_GROUP_ZKBIN, &public_inputs) {
            ZkVerifyResult::InvalidProof | ZkVerifyResult::InvalidVk => {}
            other => panic!(
                "a proof whose first member does not derive the exposed group_id must not verify; got {other:?}"
            ),
        }
    }
}
