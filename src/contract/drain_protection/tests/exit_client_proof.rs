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

//! The `exit` client's proof verifies against the circuit it names (`OBL-C78`).
//!
//! `exit.zk` instances `[tx_binding, tx_nonce]` and the metadata arm used to answer with two literal
//! zeros, so the instruction could not verify as built — and because
//! `src/contract/drain_protection/src/client/` held no proof module at all, nothing in the tree could
//! produce a proof to check that against. Both halves are now in place and this is the check:
//! proving **and** verifying, following `stablecoin_governance_report.rs` — an unsatisfied circuit
//! still yields proof bytes, so an assertion on `Proof::create` alone would report success for
//! exactly the circuits this is meant to catch.
//!
//! The verification uses the **same zkbin the contract embeds** and the inputs `to_vec()` produces,
//! so it says the two halves agree with each other rather than with a transcription of either.
//! Gated on the `client` feature because the proof modules need `dwow_core`, which the wasm build does
//! not enable; `make test` runs the workspace with `--all-features`.

#![cfg(feature = "client")]

use dwow_core::{
    zk::{empty_witnesses, verify_zkp, Proof, ProvingKey, ZkCircuit, ZkVerifyResult},
    zkas::ZkBinary,
};
use dwow_drain_protection_contract::client::exit::{create_exit_proof, ExitCallData};
use dwow_sdk::pasta::pallas;

/// The zkbin the contract embeds (`entrypoint.rs`: `zkas_db_set(include_bytes!("../proof/exit.zk.bin"))`).
const EXIT_ZKBIN: &[u8] = include_bytes!("../proof/exit.zk.bin");

fn proving_key(zkbin: &ZkBinary) -> ProvingKey {
    let circuit = ZkCircuit::new(empty_witnesses(zkbin).expect("witnesses"), zkbin);
    ProvingKey::build(zkbin.k, &circuit).expect("ProvingKey::build")
}

#[test]
fn exit_client_proof_verifies_against_its_own_circuit() {
    let zkbin = ZkBinary::decode(EXIT_ZKBIN, false).expect("exit.zk.bin decodes");
    let pk = proving_key(&zkbin);

    let call = ExitCallData::new();
    let (proof, public_inputs) = create_exit_proof(&zkbin, &pk, &call)
        .expect("the client must build a proof for the circuit it names");
    let inputs = public_inputs.to_vec();

    match verify_zkp(&proof, EXIT_ZKBIN, &inputs) {
        ZkVerifyResult::Ok => {}
        other => panic!(
            "the client's proof must verify against exit.zk with the inputs its own to_vec() \
             produces; got {other:?}"
        ),
    }

    // The control, without which a verifier that accepted anything would pass the assertion above:
    // the same proof against a *different* nonce must be refused. This is what makes the pass above
    // evidence about the pair rather than about the verifier's willingness.
    let mut wrong = inputs.clone();
    wrong[1] = pallas::Base::from(1u64);
    match verify_zkp(&proof, EXIT_ZKBIN, &wrong) {
        ZkVerifyResult::InvalidProof | ZkVerifyResult::InvalidVk => {}
        other => panic!("a proof must not verify against a public input it was not made with; got {other:?}"),
    }

    // And the binding itself: the pair the params carry is the one the circuit derives from the
    // witnesses, which is what the metadata arm's two published values have to equal.
    let expected = call.tx_binding();
    assert_eq!(public_inputs.tx_binding, expected, "tx_binding must be the derivation of the call's pair");
    assert_eq!(public_inputs.tx_nonce, call.tx_nonce);
}
