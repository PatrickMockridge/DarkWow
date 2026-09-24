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

//! OBL-C78 — do the clients' proofs verify against their own circuits, with the public inputs the
//! client actually produces?
//!
//! This exists to localise a failure, and the distinction it draws is the point. `test_heavyweight_tender`
//! reports `invalid proof: call[0] namespace 'CreateTenderV2'`, which means the **host**'s verification
//! failed — and the host verifies against the *metadata's* instance vector. So the failure has two
//! possible homes: the proof and the circuit disagree (a client, witness or zkbin problem), or the
//! proof is fine and the instance the contract publishes is not the one the proof was made with (a
//! metadata problem). Reading cannot tell them apart; these tests can, because each verifies the
//! client's proof against the **same zkbin the contract embeds**, with the inputs `to_vec()` produces,
//! and never involves the host at all.
//!
//! Proving *and* verifying, following `stablecoin_governance_report.rs`: an unsatisfied circuit still
//! produces proof bytes, so an assertion on `Proof::create` alone reports success for exactly the
//! circuits it is meant to catch.
//!
//! `create_tender` **fails** (both tests) while `slot`'s `commit_bet` passes — so this is not one
//! systemic defect in the client-proof path. The difference between the two is instructive: `slot`'s
//! client derives `tx_binding = poseidon_hash([3, tx_commitment, tx_nonce])` from its own call data,
//! while `tender`'s instanced a literal zero until 2026-09-23 and its witnesses carry values the
//! circuit's `NULLIFIER_K` derivation has to reproduce. The remaining tender defect is a **constraint**
//! the proof does not satisfy, and it is below `OBL-C78`'s layer: this file is the instrument for
//! finding it, not the fix.

use dwow_core::zk::{
    empty_witnesses, verify_zkp, Proof, ProvingKey, ZkCircuit, ZkVerifyResult,
};
use dwow_core::zkas::ZkBinary;
use dwow_sdk::crypto::{PublicKey, SecretKey};
use dwow_sdk::pasta::pallas;
use dwow_tender_contract::client::create_tender::{
    create_tender_v1_proof, CreateTenderV1CallData,
};

const ZKBIN_BYTES: &[u8] = include_bytes!("../../tender/proof/create_tender.zk.bin");

fn create_tender_zkbin() -> ZkBinary {
    ZkBinary::decode(ZKBIN_BYTES, false).expect("create_tender.zk.bin decodes")
}

fn proving_key(zkbin: &ZkBinary) -> ProvingKey {
    let circuit = ZkCircuit::new(empty_witnesses(zkbin).expect("witnesses"), zkbin);
    ProvingKey::build(zkbin.k, &circuit).expect("ProvingKey::build")
}

/// The client's own proof must verify against the client's own public inputs.
///
/// A failure here says the defect is *below* the metadata: the witnesses, the instance vector or the
/// circuit. A pass says the proof is sound and the metadata is what disagrees with it — which is the
/// other half of the same investigation.
#[test]
fn create_tender_proof_verifies_against_its_own_circuit() {
    let zkbin = create_tender_zkbin();
    let pk = proving_key(&zkbin);

    let secret = pallas::Base::from(10u64);
    let public = PublicKey::from_secret(SecretKey::from_base(secret));
    let call_data = CreateTenderV1CallData::new(secret, public);

    let (proof, public_inputs) = create_tender_v1_proof(&zkbin, &pk, &call_data)
        .expect("the client must build a proof");
    let inputs = public_inputs.to_vec();

    assert_eq!(
        inputs.len(),
        4,
        "create_tender.zk instances four values: requester_pub_x, requester_pub_y, tx_binding, tx_nonce"
    );

    match verify_zkp(&proof, ZKBIN_BYTES, &inputs) {
        ZkVerifyResult::Ok => {}
        other => panic!(
            "OBL-C78: the client's create_tender proof does not verify against its own circuit with \
             its own public inputs ({other:?}). The defect is in the client, the witnesses or the \
             zkbin — not in the contract's metadata, which this test never touches."
        ),
    }
}

/// And the same for a non-zero transaction pair: the binding must survive the round trip, because a
/// binding that only works at zero is not a binding (`OBL-C78`).
#[test]
fn create_tender_proof_verifies_with_a_non_zero_tx_pair() {
    let zkbin = create_tender_zkbin();
    let pk = proving_key(&zkbin);

    let secret = pallas::Base::from(11u64);
    let public = PublicKey::from_secret(SecretKey::from_base(secret));
    let mut call_data = CreateTenderV1CallData::new(secret, public);
    call_data.tx_commitment = pallas::Base::from(0xC0FFEEu64);
    call_data.tx_nonce = pallas::Base::from(7u64);

    let (proof, public_inputs) = create_tender_v1_proof(&zkbin, &pk, &call_data)
        .expect("the client must build a proof");
    let inputs = public_inputs.to_vec();

    // The published binding is the one the circuit derives from the pair — not a constant.
    let expected = dwow_tender_contract::client::tx_binding_of(&call_data.tx_commitment, &call_data.tx_nonce);
    assert_eq!(
        inputs[2], expected,
        "the instance's tx_binding must be poseidon_hash([3, tx_commitment, tx_nonce]) for the pair the \
         proof was made with"
    );

    match verify_zkp(&proof, ZKBIN_BYTES, &inputs) {
        ZkVerifyResult::Ok => {}
        other => panic!(
            "OBL-C78: a create_tender proof bound to a non-zero tx pair does not verify ({other:?}) — \
             the tx binding is not carried through the circuit."
        ),
    }
}

// ============================================================================
// slot::commit_bet — the control for the sentence above
// ============================================================================

const SLOT_COMMIT_ZKBIN: &[u8] = include_bytes!("../../slot/proof/commit_bet.zk.bin");

/// `slot`'s `commit_bet` client derives its `tx_binding` from the pair it is given, so its proof should
/// verify. **This is the control that keeps the `create_tender` failures from being read as systemic**:
/// if this test passed while the other failed for a reason common to every client, the distinction
/// would be worthless; it fails or passes on the same code path — `ZkCircuit::new(witnesses, zkbin)` and
/// `Proof::create` — as `create_tender`'s.
#[test]
fn slot_commit_bet_proof_verifies_against_its_own_circuit() {
    use dwow_slot_contract::client::commit_bet::{create_commit_bet_v1_proof, CommitBetV1CallData};

    let zkbin = ZkBinary::decode(SLOT_COMMIT_ZKBIN, false).expect("commit_bet.zk.bin decodes");
    let pk = proving_key(&zkbin);

    let player_secret = pallas::Base::from(20u64);
    let player_public = PublicKey::from_secret(SecretKey::from_base(player_secret));
    // `CommitBetV1CallData::new` takes seven arguments — `player_pub, bet_value, paylines,
    // secret_nonce, blind, asset_id, value_blind` (`slot/src/client/commit_bet.rs:71`) — and this
    // call passed eight until 2026-09-24, which is why `make test` could not compile at all. The
    // surplus was an unlabelled `player_secret`; the labelled `from(30) // secret_nonce` below is the
    // real `secret_nonce`, and `player_secret` is what derived `player_public` above, so nothing is
    // lost by dropping it here.
    let call_data = CommitBetV1CallData::new(
        player_public,
        /* bet_value */ 100,
        /* paylines */ 5,
        pallas::Base::from(30u64), // secret_nonce
        pallas::Base::from(40u64), // blind
        pallas::Base::from(50u64), // asset_id
        pallas::Scalar::from(60u64), // value_blind
    );

    let (proof, public_inputs) = create_commit_bet_v1_proof(&zkbin, &pk, &call_data)
        .expect("the client must build a proof");
    let inputs = public_inputs.to_vec();

    assert_eq!(
        inputs.len(),
        5,
        "commit_bet.zk instances five values: spin_id, value_commit x/y, tx_binding, tx_nonce"
    );

    match verify_zkp(&proof, SLOT_COMMIT_ZKBIN, &inputs) {
        ZkVerifyResult::Ok => {}
        other => panic!(
            "control: slot's commit_bet proof does not verify against its own circuit ({other:?}) — \
             which would make the tender failures systemic rather than contract-specific."
        ),
    }
}
