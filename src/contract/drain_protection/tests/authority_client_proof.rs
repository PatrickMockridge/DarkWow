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

//! The shared authority client's proofs verify against the circuits they name (`OBL-C78`).
//!
//! Eight of this contract's nine circuits — `execute`, `initialize`, `lock`, `propose`, `transfer`,
//! `unlock`, `update_config` and `vote` — instance the same five values and declare the same witness
//! list, so `client::create_authority_proof` serves all eight. This checks two of them (`lock` and
//! `vote`), and it checks them the way `exit_client_proof.rs` checks `exit`: proving **and**
//! verifying, against **the same zkbin the contract embeds**, with the inputs `to_vec()` produces.
//!
//! Two of the eight rather than all eight, stated rather than implied: the witness list and the
//! instance vector are *compared* across all eight by
//! `scripts/check-circuit-metadata-alignment.sh` (which reads each circuit's `constrain_instance`
//! order and each arm's pushes), and what this test adds is the half no static check can reach — that
//! the client's derivation satisfies the circuit's *constraints*.

#![cfg(feature = "client")]

use dwow_core::{
    zk::{empty_witnesses, verify_zkp, Proof, ProvingKey, ZkCircuit, ZkVerifyResult},
    zkas::ZkBinary,
};
use dwow_drain_protection_contract::client::{create_authority_proof, AuthorityCallData};
use dwow_sdk::pasta::pallas;

/// The zkbins the contract embeds (`entrypoint.rs`: `zkas_db_set(include_bytes!(...))`).
const LOCK_ZKBIN: &[u8] = include_bytes!("../proof/lock.zk.bin");
const VOTE_ZKBIN: &[u8] = include_bytes!("../proof/vote.zk.bin");

fn proving_key(zkbin: &ZkBinary) -> ProvingKey {
    let circuit = ZkCircuit::new(empty_witnesses(zkbin).expect("witnesses"), zkbin);
    ProvingKey::build(zkbin.k, &circuit).expect("ProvingKey::build")
}

fn prove_and_verify(name: &str, zkbin_bytes: &[u8]) {
    let zkbin = ZkBinary::decode(zkbin_bytes, false).expect("zkbin decodes");
    let pk = proving_key(&zkbin);

    // A real secret and a real fund: the circuit derives the point and the nullifier from both, so a
    // zero secret would exercise the derivation at its least interesting input.
    let call = AuthorityCallData::new(pallas::Base::from(1234u64), pallas::Base::from(5678u64))
        .tx_pair(pallas::Base::from(11u64), pallas::Base::from(22u64));

    let (proof, public_inputs) = create_authority_proof(&zkbin, &pk, &call)
        .unwrap_or_else(|e| panic!("{name}: the client must build a proof for the circuit it names: {e}"));
    let inputs = public_inputs.to_vec();

    match verify_zkp(&proof, zkbin_bytes, &inputs) {
        ZkVerifyResult::Ok => {}
        other => panic!(
            "{name}: the client's proof must verify against its own circuit with the inputs its own \
             to_vec() produces; got {other:?}"
        ),
    }

    // The control, without which a verifier that accepted anything would pass the assertion above.
    let mut wrong = inputs.clone();
    wrong[3] = pallas::Base::from(99u64); // tx_binding
    match verify_zkp(&proof, zkbin_bytes, &wrong) {
        ZkVerifyResult::InvalidProof | ZkVerifyResult::InvalidVk => {}
        other => panic!("{name}: a proof must not verify against a public input it was not made with; got {other:?}"),
    }

    assert_eq!(public_inputs.tx_binding, call.tx_binding(), "{name}: to_vec must carry the derived binding");
    assert_eq!(public_inputs.authority_nullifier, call.authority_nullifier(), "{name}: and the derived nullifier");
}

#[test]
fn lock_client_proof_verifies_against_its_own_circuit() {
    prove_and_verify("lock", LOCK_ZKBIN);
}

#[test]
fn vote_client_proof_verifies_against_its_own_circuit() {
    prove_and_verify("vote", VOTE_ZKBIN);
}
