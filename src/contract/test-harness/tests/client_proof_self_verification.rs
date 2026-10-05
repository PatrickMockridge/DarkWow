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
use dwow_tender_contract::client::submit_bid::{
    submit_bid_v1_proof, SubmitBidV1CallData,
};
use dwow_tender_contract::client::submit_bid_with_capability::{
    submit_bid_with_capability_v1_proof, SubmitBidWithCapabilityV1CallData,
};

const ZKBIN_BYTES: &[u8] = include_bytes!("../../tender/proof/create_tender.zk.bin");

/// `submit_bid`'s circuit binary — the one `manifest.toml:20` names for function code 1.
const SUBMIT_BID_ZKBIN_BYTES: &[u8] = include_bytes!("../../tender/proof/submit_bid.zk.bin");

/// And the capability endpoint's, which `manifest.toml` names `SubmitBidWithCapabilityV2`.
const SUBMIT_BID_WITH_CAP_ZKBIN_BYTES: &[u8] =
    include_bytes!("../../tender/proof/submit_bid_with_capability.zk.bin");

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

/// **The same question for `submit_bid`, added because it is where the fixture now fails and nothing
/// localised it.** `create_tender`'s two tests above pass and `test_heavyweight_tender` commits
/// block 2 — so the defect the register recorded against `create_tender` has since been fixed, and
/// the fixture's rejection has moved to block 3, `invalid proof: call[0] namespace 'SubmitBidV2'`.
///
/// **Why this case and not a guess about causes**: a failure here says the defect is *below* the
/// metadata — the client's witnesses, its instance vector, or the circuit — and a pass says the proof
/// is sound and the contract's metadata is what disagrees with it. Those are the two halves of the
/// same investigation, and this test picks the half without touching the host.
///
/// A note on the pairing, because it looks like a mismatch and is not: the client type is
/// `SubmitBidV1CallData` while the manifest names `SubmitBidV2`, and that is the shape the register
/// carries for several contracts. The **V1 names the client**; the zkbin is whatever the contract
/// deploys for the namespace, and this test takes the same `.zk.bin` the contract embeds.
#[test]
fn submit_bid_proof_verifies_against_its_own_circuit() {
    let zkbin = ZkBinary::decode(SUBMIT_BID_ZKBIN_BYTES, false)
        .expect("submit_bid.zk.bin decodes");
    let pk = proving_key(&zkbin);

    let secret = pallas::Base::from(20u64);
    let public = PublicKey::from_secret(SecretKey::from_base(secret));
    let call_data = SubmitBidV1CallData::new(
        // The circuit instances `tender_id`, so a canonical value keeps this test about the proof
        // rather than about which tender the fixture happened to create.
        pallas::Base::from(1u64),
        secret,
        pallas::Base::from(5000u64),
        pallas::Base::from(3u64),
        public,
    );

    let (proof, public_inputs) = submit_bid_v1_proof(&zkbin, &pk, &call_data)
        .expect("the client must build a proof");
    let inputs = public_inputs.to_vec();

    match verify_zkp(&proof, SUBMIT_BID_ZKBIN_BYTES, &inputs) {
        ZkVerifyResult::Ok => {}
        other => panic!(
            "the client's submit_bid proof does not verify against its own circuit with its own \
             public inputs ({other:?}). The defect is in the client, the witnesses or the zkbin — \
             not in the contract's metadata, which this test never touches."
        ),
    }
}

/// **And the capability endpoint's, which could not build a proof at all.** `test_heavyweight_tender`
/// reported `harness generate failed — halo2 plonk error: General synthesis error` for this client:
/// its `to_witnesses` supplied **twelve** witnesses to a circuit that declares **eleven**, in a
/// different order, `bid_id` among them — and `submit_bid_with_capability.zk:119` *computes* `bid_id`,
/// so it is an intermediate and not a witness. The vector is now the circuit's declaration verbatim.
///
/// This case exists so the next disagreement of this kind is a one-minute failure rather than a
/// heavyweight run that stops one row early: a witness-arity mismatch is not visible to any gate in
/// the tree, and the only thing that caught it was building it.
#[test]
fn submit_bid_with_capability_proof_verifies_against_its_own_circuit() {
    let zkbin = ZkBinary::decode(SUBMIT_BID_WITH_CAP_ZKBIN_BYTES, false)
        .expect("submit_bid_with_capability.zk.bin decodes");
    let pk = proving_key(&zkbin);

    let secret = pallas::Base::from(20u64);
    let public = PublicKey::from_secret(SecretKey::from_base(secret));
    let call_data = SubmitBidWithCapabilityV1CallData::new(
        pallas::Base::from(1u64),
        secret,
        pallas::Base::from(5000u64),
        pallas::Base::from(3u64),
        pallas::Base::from(7u64),
        // `submit_bid_with_capability.zk:108` constrains this witness equal to the constant `ONE`, so
        // anything else fails synthesis — which is the tautology the host check relied on, and is not
        // what this test is about.
        pallas::Base::one(),
        public,
    );

    let (proof, public_inputs) = submit_bid_with_capability_v1_proof(&zkbin, &pk, &call_data)
        .expect("the client must build a proof");
    let inputs = public_inputs.to_vec();

    match verify_zkp(&proof, SUBMIT_BID_WITH_CAP_ZKBIN_BYTES, &inputs) {
        ZkVerifyResult::Ok => {}
        other => panic!(
            "the client's submit_bid_with_capability proof does not verify against its own circuit \
             with its own public inputs ({other:?}). The defect is in the client, the witnesses or \
             the zkbin — not in the contract's metadata, which this test never touches."
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

/// github issue #3 — the attestation repair, localised.
///
/// `test_heavyweight_attestation` failed at height 10 with `invalid proof: call[0] namespace
/// 'AttestSlashV2'` after the circuits changed, and that message cannot say which of the two
/// homes the defect is in: a proof that disagrees with its circuit, or a proof that is fine
/// against an instance vector the contract does not publish. The endpoint list is also
/// sequential, so one failure there costs a full run (~900s) and hides every later endpoint.
/// These cases answer both: each proves against the **same zkbin the contract embeds**, with
/// the instance vector the metadata arm publishes, and never involves the host.
///
/// The two `*_fabricated` cases are the ones that failed. `attest_slash` and
/// `commit_fee_schedule` have no client module, so their proofs are built by the harness's own
/// witness construction — which hardcoded a secret of `1` beside whatever public key the
/// caller passed, and published a two-element vector. While the circuits left the secret and
/// the coordinates unconstrained that was inert; the moment they derive-and-expose them, the
/// fixture describes a proof the circuit cannot be satisfied by. The construction is repeated
/// here rather than imported so that a change to the harness cannot silently change what this
/// control is checking.
mod attestation_issue3 {
    use super::{proving_key, verify_zkp, ZkBinary, ZkVerifyResult, Proof, PublicKey, SecretKey, ZkCircuit, pallas};
    use dwow_core::zk::{halo2::Value, Witness};
    use rand::SeedableRng;

    const CREATE_ATTESTATION: &[u8] = include_bytes!("../../attestation/proof/create_attestation.zk.bin");
    const VERIFY_CLAIM: &[u8] = include_bytes!("../../attestation/proof/verify_claim.zk.bin");
    const REVOKE: &[u8] = include_bytes!("../../attestation/proof/revoke_attestation.zk.bin");
    const ATTEST_SLASH: &[u8] = include_bytes!("../../attestation/proof/attest_slash.zk.bin");
    const COMMIT_FEE: &[u8] = include_bytes!("../../attestation/proof/commit_fee_schedule.zk.bin");
    const EXPIRE: &[u8] = include_bytes!("../../attestation/proof/expire_attestation.zk.bin");
    const UPDATE_DELEGATION: &[u8] = include_bytes!("../../attestation/proof/update_delegation.zk.bin");

    fn zkbin(bytes: &'static [u8]) -> &'static ZkBinary {
        // Decoded per call rather than cached: these are five small circuits and the point is
        // to read the artifact the contract embeds, not a copy this file controls.
        Box::leak(Box::new(ZkBinary::decode(bytes, false).expect("circuit decodes")))
    }

    fn txb() -> pallas::Base {
        // DOMAIN_TX_BINDING = 3, and the fixtures' pair is (0, 0).
        dwow_sdk::crypto::poseidon_hash([
            pallas::Base::from(3u64),
            pallas::Base::zero(),
            pallas::Base::zero(),
        ])
    }

    #[test]
    fn create_attestation_proof_verifies_and_the_actor_is_bound() {
        use dwow_attestation_contract::client::create_attestation::{
            create_attestation_v1_proof, CreateAttestationV1CallData,
        };

        let zkbin = zkbin(CREATE_ATTESTATION);
        let pk = proving_key(zkbin);

        let secret = pallas::Base::from(10u64);
        let public = PublicKey::from_secret(SecretKey::from_base(secret));
        let (ax, ay) = public.xy().expect("pk not identity");

        let call_data = CreateAttestationV1CallData::new(secret, public);
        let (proof, public_inputs) =
            create_attestation_v1_proof(zkbin, &pk, &call_data).expect("the client must build a proof");
        let inputs = public_inputs.to_vec();

        assert_eq!(inputs.len(), 4, "create_attestation.zk instances tx_binding, tx_nonce, and \
                                     the two attestor coordinates");
        assert_eq!(inputs[2], ax);
        assert_eq!(inputs[3], ay);

        match verify_zkp(&proof, CREATE_ATTESTATION, &inputs) {
            ZkVerifyResult::Ok => {}
            other => panic!("create_attestation's proof does not verify against its own circuit ({other:?})"),
        }

        // The negative half, and it is the whole point of the repair: the same proof must NOT
        // verify when the instance vector names a different attestor. A circuit that left the
        // coordinates unconstrained would accept this, which is the forgery.
        let victim = PublicKey::from_secret(SecretKey::from_base(pallas::Base::from(11u64)));
        let (vx, vy) = victim.xy().expect("pk not identity");
        let forged = [inputs[0], inputs[1], vx, vy];
        match verify_zkp(&proof, CREATE_ATTESTATION, &forged) {
            ZkVerifyResult::Ok => panic!(
                "create_attestation's proof verifies under the instance vector of a DIFFERENT \
                 attestor — that is the forgery github issue #3 describes, arriving again."
            ),
            _ => {}
        }
    }

    #[test]
    fn verify_claim_proof_verifies_against_its_own_circuit() {
        use dwow_attestation_contract::client::verify_claim::{verify_claim_v1_proof, VerifyClaimV1CallData};

        let zkbin = zkbin(VERIFY_CLAIM);
        let pk = proving_key(zkbin);

        let call_data = VerifyClaimV1CallData::new(
            pallas::Base::from(1u64), pallas::Base::from(2u64), pallas::Base::from(60u64),
            pallas::Base::from(3u64), pallas::Base::from(4u64), pallas::Base::from(5u64),
            [pallas::Base::from(0u64); 255], pallas::Base::from(6u64),
        );
        let (proof, public_inputs) =
            verify_claim_v1_proof(zkbin, &pk, &call_data).expect("the client must build a proof");
        let inputs = public_inputs.to_vec();

        assert_eq!(inputs.len(), 2, "verify_claim.zk instances the tx pair and nothing else — the \
                                     three hashes it used to compute reached no constraint and its \
                                     verdict is now derived by the host");
        match verify_zkp(&proof, VERIFY_CLAIM, &inputs) {
            ZkVerifyResult::Ok => {}
            other => panic!("verify_claim's proof does not verify against its own circuit ({other:?})"),
        }
    }

    #[test]
    fn revoke_attestation_proof_verifies_and_the_actor_is_bound() {
        use dwow_attestation_contract::client::revoke_attestation::{
            revoke_attestation_v1_proof, RevokeAttestationV1CallData,
        };

        let zkbin = zkbin(REVOKE);
        let pk = proving_key(zkbin);

        let secret = pallas::Base::from(10u64);
        let public = PublicKey::from_secret(SecretKey::from_base(secret));
        let (ax, ay) = public.xy().expect("pk not identity");

        let call_data = RevokeAttestationV1CallData::new(secret, public);
        let (proof, public_inputs) =
            revoke_attestation_v1_proof(zkbin, &pk, &call_data).expect("the client must build a proof");
        let inputs = public_inputs.to_vec();
        assert_eq!(inputs.len(), 4);

        match verify_zkp(&proof, REVOKE, &inputs) {
            ZkVerifyResult::Ok => {}
            other => panic!("revoke_attestation's proof does not verify against its own circuit ({other:?})"),
        }

        // The instruction had no circuit at all before this change, and its host compared the
        // stored key against the wire's copy of itself. The control is that the proof is bound
        // to the key: a different attestor's instance vector must not verify.
        let victim = PublicKey::from_secret(SecretKey::from_base(pallas::Base::from(11u64)));
        let (vx, vy) = victim.xy().expect("pk not identity");
        match verify_zkp(&proof, REVOKE, &[inputs[0], inputs[1], vx, vy]) {
            ZkVerifyResult::Ok => panic!(
                "revoke_attestation's proof verifies under another attestor's coordinates — the \
                 public-against-public check this circuit replaced."
            ),
            _ => {}
        }
    }

    /// OBL-C196(i): `expire_attestation` gained a circuit. Its arm checked no caller, so any party
    /// could flip any attestation to `Expired`; the circuit derive-and-exposes the attestor and the
    /// handler compares the published coordinates against the stored key. The two-sided control is
    /// the same as revoke's: bound to the actor's own key, and NOT under another's.
    #[test]
    fn expire_attestation_proof_verifies_and_the_actor_is_bound() {
        use dwow_attestation_contract::client::expire_attestation::{
            expire_attestation_v1_proof, ExpireAttestationV1CallData,
        };

        let zkbin = zkbin(EXPIRE);
        let pk = proving_key(zkbin);

        let secret = pallas::Base::from(10u64);
        let public = PublicKey::from_secret(SecretKey::from_base(secret));
        let (ax, ay) = public.xy().expect("pk not identity");

        let call_data = ExpireAttestationV1CallData::new(secret, public);
        let (proof, public_inputs) =
            expire_attestation_v1_proof(zkbin, &pk, &call_data).expect("the client must build a proof");
        let inputs = public_inputs.to_vec();
        assert_eq!(inputs.len(), 4, "expire_attestation.zk instances the tx pair and the two attestor coordinates");
        assert_eq!(inputs[2], ax);
        assert_eq!(inputs[3], ay);

        match verify_zkp(&proof, EXPIRE, &inputs) {
            ZkVerifyResult::Ok => {}
            other => panic!("expire_attestation's proof does not verify against its own circuit ({other:?})"),
        }

        let victim = PublicKey::from_secret(SecretKey::from_base(pallas::Base::from(11u64)));
        let (vx, vy) = victim.xy().expect("pk not identity");
        match verify_zkp(&proof, EXPIRE, &[inputs[0], inputs[1], vx, vy]) {
            ZkVerifyResult::Ok => panic!(
                "expire_attestation's proof verifies under another attestor's coordinates — any \
                 party could expire any attestation, which is the hole this circuit closed."
            ),
            _ => {}
        }
    }

    /// OBL-C196(ii): `update_delegation`'s witnesses were restored with the host read. The
    /// delegator is the original attestation's attestor, so the same two-sided control applies.
    #[test]
    fn update_delegation_proof_verifies_and_the_actor_is_bound() {
        use dwow_attestation_contract::client::update_delegation::{
            update_delegation_v1_proof, UpdateDelegationV1CallData,
        };

        let zkbin = zkbin(UPDATE_DELEGATION);
        let pk = proving_key(zkbin);

        let secret = pallas::Base::from(10u64);
        let public = PublicKey::from_secret(SecretKey::from_base(secret));
        let (dx, dy) = public.xy().expect("pk not identity");

        let call_data = UpdateDelegationV1CallData {
            delegator_secret: secret,
            delegator_public: public,
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        };
        let (proof, public_inputs) =
            update_delegation_v1_proof(zkbin, &pk, &call_data).expect("the client must build a proof");
        let inputs = public_inputs.to_vec();
        assert_eq!(inputs.len(), 4, "update_delegation.zk instances tx_binding, tx_nonce and the two delegator coordinates");
        assert_eq!(inputs[2], dx);
        assert_eq!(inputs[3], dy);

        match verify_zkp(&proof, UPDATE_DELEGATION, &inputs) {
            ZkVerifyResult::Ok => {}
            other => panic!("update_delegation's proof does not verify against its own circuit ({other:?})"),
        }

        let victim = PublicKey::from_secret(SecretKey::from_base(pallas::Base::from(11u64)));
        let (vx, vy) = victim.xy().expect("pk not identity");
        match verify_zkp(&proof, UPDATE_DELEGATION, &[inputs[0], inputs[1], vx, vy]) {
            ZkVerifyResult::Ok => panic!(
                "update_delegation's proof verifies under another delegator's coordinates — the arm \
                 authorized no one before this."
            ),
            _ => {}
        }
    }

    /// The harness's construction for a circuit with no client, repeated here verbatim.
    fn fabricated_actor_proof(
        bytes: &'static [u8],
        secret: pallas::Base,
        public: PublicKey,
    ) -> (Proof, &'static ZkBinary, Vec<pallas::Base>) {
        let zkbin = zkbin(bytes);
        let pk = proving_key(zkbin);
        let (ax, ay) = public.xy().expect("pk not identity");
        let txb = txb();
        // Circuit witness order: attester_secret, attester_pub_x, attester_pub_y,
        // tx_commitment, tx_nonce, tx_binding
        let witnesses = vec![
            Witness::Base(Value::known(secret)),
            Witness::Base(Value::known(ax)),
            Witness::Base(Value::known(ay)),
            Witness::Base(Value::known(pallas::Base::zero())),
            Witness::Base(Value::known(pallas::Base::zero())),
            Witness::Base(Value::known(txb)),
        ];
        // Circuit constrain_instance order: tx_binding, tx_nonce, attester_pub_x, attester_pub_y
        let publics = vec![txb, pallas::Base::zero(), ax, ay];
        let circuit = ZkCircuit::new(witnesses, zkbin);
        let mut rng = rand::rngs::StdRng::seed_from_u64(0);
        let proof = Proof::create(&pk, &[circuit], &publics, &mut rng)
            .expect("an unsatisfied circuit still produces bytes, so this passing means nothing on its own");
        (proof, zkbin, publics)
    }

    #[test]
    fn attest_slash_fabricated_proof_verifies() {
        let secret = pallas::Base::from(10u64);
        let public = PublicKey::from_secret(SecretKey::from_base(secret));
        let (proof, _zkbin, publics) = fabricated_actor_proof(ATTEST_SLASH, secret, public);
        assert_eq!(publics.len(), 4);
        match verify_zkp(&proof, ATTEST_SLASH, &publics) {
            ZkVerifyResult::Ok => {}
            other => panic!(
                "attest_slash's fabricated proof does not verify against its own circuit ({other:?}) — \
                 this is the failure test_heavyweight_attestation reported at height 10, localised."
            ),
        }
    }

    #[test]
    fn commit_fee_schedule_fabricated_proof_verifies() {
        let secret = pallas::Base::from(10u64);
        let public = PublicKey::from_secret(SecretKey::from_base(secret));
        let (proof, _zkbin, publics) = fabricated_actor_proof(COMMIT_FEE, secret, public);
        assert_eq!(publics.len(), 4);
        match verify_zkp(&proof, COMMIT_FEE, &publics) {
            ZkVerifyResult::Ok => {}
            other => panic!(
                "commit_fee_schedule's fabricated proof does not verify against its own circuit ({other:?})"
            ),
        }
    }
}

// ============================================================================
// darktoshi_dice::settle_bet — the circuit whose pair MOVED, and whose vector therefore had to
// ============================================================================

const DICE_SETTLE_ZKBIN: &[u8] = include_bytes!("../../darktoshi_dice/proof/settle_bet.zk.bin");

/// `OBL-C198`: a `settle_bet` proof made against a non-zero tx pair verifies against the circuit the
/// contract embeds, with the vector the client publishes.
///
/// The assertion is on the pair's **position** as much as on its value, and both are needed.
/// `settle_bet` is the dice circuit whose pair moved — from 2,3 of 4 to last — so a `to_vec` left
/// behind by the move produces a proof the host refuses while every *count* still agrees, which is
/// the shape that cost a session a 907-second run. `verify_zkp` fails on the transposition; the
/// value assertion fails on a constant, which is what the arm published until this campaign.
///
/// This is the local instrument for a 900-second heavyweight run, and it is deliberately not a
/// substitute for it: it exercises the client and the circuit against each other and never touches
/// the host's metadata.
#[test]
fn dice_settle_bet_proof_verifies_with_a_non_zero_tx_pair() {
    use dwow_darktoshi_dice_contract::client::settle_bet::{
        create_settle_bet_v1_proof, SettleBetV1CallData,
    };

    let zkbin = ZkBinary::decode(DICE_SETTLE_ZKBIN, false).expect("settle_bet.zk.bin decodes");
    let pk = proving_key(&zkbin);

    let tx_commitment = pallas::Base::from(0xC0FFEEu64);
    let tx_nonce = pallas::Base::from(9u64);

    let mut input = SettleBetV1CallData::new(
        pallas::Base::from(1u64),    // player_pub_x
        pallas::Base::from(2u64),    // player_pub_y
        pallas::Base::from(1000u64), // bet_value
        pallas::Base::from(1u64),    // target
        pallas::Base::from(99u64),   // secret_nonce
        pallas::Base::from(3u64),    // blind
        pallas::Base::from(4u64),    // asset_id
        pallas::Base::from(42u64),   // block_hash
    );
    input.tx_commitment = tx_commitment;
    input.tx_nonce = tx_nonce;

    let (proof, public_inputs) =
        create_settle_bet_v1_proof(&zkbin, &pk, &input).expect("the client must build a proof");
    let inputs = public_inputs.to_vec();
    assert_eq!(inputs.len(), 4, "settle_bet instances four values");

    // The pair is the LAST two: the node reads `pubvals[len-2]`/`pubvals[len-1]`, because nothing
    // in `src/linear/` indexes this vector — the position is the interface.
    assert_eq!(inputs[3], tx_nonce, "the last instance is the nonce the proof was made with");
    assert_eq!(
        inputs[2],
        dwow_sdk::crypto::poseidon_hash([
            pallas::Base::from(3u64),
            tx_commitment,
            tx_nonce
        ]),
        "the instance before it is poseidon_hash([3, tx_commitment, tx_nonce]) for the pair the \
         proof was made with — not a constant"
    );

    match verify_zkp(&proof, DICE_SETTLE_ZKBIN, &inputs) {
        ZkVerifyResult::Ok => {}
        other => panic!(
            "OBL-C198: a settle_bet proof bound to a non-zero tx pair does not verify ({other:?}) — \
             the client's `to_vec` order and the circuit's `constrain_instance` order disagree."
        ),
    }
}

// ============================================================================
// slot::settle_bet — the same control, for the same move, one contract over
// ============================================================================

const SLOT_SETTLE_ZKBIN: &[u8] = include_bytes!("../../slot/proof/settle_bet.zk.bin");

/// `OBL-C198`: `slot`'s `settle_bet` had its pair moved from 1,2 of 4 to last on 2026-10-05, so it
/// gets the same control as `darktoshi_dice`'s — the two are the campaign's worked examples for a
/// circuit whose pair moved with a *value* instance after it, and a `to_vec` left behind fails
/// here in seconds rather than in a long run. The assertions are on the pair's position, because a
/// count-only move leaves every number agreeing.
#[test]
fn slot_settle_bet_proof_verifies_with_a_non_zero_tx_pair() {
    use dwow_slot_contract::client::settle_bet::{
        create_settle_bet_v1_proof, SettleBetV1CallData,
    };

    let zkbin = ZkBinary::decode(SLOT_SETTLE_ZKBIN, false).expect("settle_bet.zk.bin decodes");
    let pk = proving_key(&zkbin);

    let tx_commitment = pallas::Base::from(0xBEEFu64);
    let tx_nonce = pallas::Base::from(11u64);

    let player_secret = pallas::Base::from(3u64);
    let player_pub = PublicKey::from_secret(SecretKey::from_base(player_secret));

    let mut input = SettleBetV1CallData::new(
        player_pub,
        1000,                            // bet_value
        1,                               // paylines
        pallas::Base::from(99u64),       // secret_nonce
        pallas::Base::from(3u64),        // blind
        pallas::Base::from(1u64),        // asset_id
        [1u64, 2u64, 3u64],              // positions
        1,                               // match_count
        500,                             // payout
    );
    input.tx_commitment = tx_commitment;
    input.tx_nonce = tx_nonce;

    let (proof, public_inputs) =
        create_settle_bet_v1_proof(&zkbin, &pk, &input).expect("the client must build a proof");
    let inputs = public_inputs.to_vec();
    assert_eq!(inputs.len(), 4, "settle_bet instances four values");

    assert_eq!(inputs[3], tx_nonce, "the last instance is the nonce the proof was made with");
    assert_eq!(
        inputs[2],
        dwow_sdk::crypto::poseidon_hash([
            pallas::Base::from(3u64),
            tx_commitment,
            tx_nonce
        ]),
        "the instance before it is poseidon_hash([3, tx_commitment, tx_nonce]) for the pair the \
         proof was made with — not a constant"
    );

    match verify_zkp(&proof, SLOT_SETTLE_ZKBIN, &inputs) {
        ZkVerifyResult::Ok => {}
        other => panic!(
            "OBL-C198: a slot settle_bet proof bound to a non-zero tx pair does not verify \
             ({other:?}) — the client's `to_vec` order and the circuit's `constrain_instance` \
             order disagree."
        ),
    }
}

/// `bearer_bond`'s `ProveCoverage_V2` binary — the circuit whose pair was **added** here, not
/// merely moved: it carried none before `OBL-C198`.
const BEARER_BOND_PROVE_COVERAGE_ZKBIN: &[u8] =
    include_bytes!("../../bearer_bond/proof/prove_coverage.zk.bin");

/// A `ProveCoverage_V2` proof is bound to the transaction commitment it was made with, and the
/// binding is the **last two** instances (`OBL-C198`).
///
/// Two assertions, because they fail for different reasons. The **position** assertion fails if a
/// `to_vec` is left behind by the pair-last move — the value is right, the slot is wrong, and the
/// host reads `pubvals[len-2]`/`pubvals[len-1]`. The **value** assertion fails if the binding is
/// the constant `poseidon_hash([3, 0, 0])` the arm published before this campaign, rather than a
/// function of the commitment. The two-sided half then proves the binding is *load-bearing*: a
/// proof made over one commitment must not verify under a different one, which is the property the
/// whole of `OBL-C198` exists to give.
///
/// This is the local instrument for a ~900-second heavyweight run, and not a substitute for it: it
/// exercises the client and the circuit against each other and never touches the host's metadata.
#[test]
fn bearer_bond_prove_coverage_proof_is_bound_to_its_tx_commitment() {
    use dwow_bearer_bond_contract::client::prove_coverage::{
        ProveCoverageCallBuilder, ProveCoverageCallInput, ProveCoverageRevealed,
    };

    let zkbin = ZkBinary::decode(BEARER_BOND_PROVE_COVERAGE_ZKBIN, false)
        .expect("prove_coverage.zk.bin decodes");
    let pk = proving_key(&zkbin);

    let tx_commitment = pallas::Base::from(0xC0FFEEu64);
    let tx_nonce = pallas::Base::from(9u64);

    let input = ProveCoverageCallInput {
        series_asset_id: pallas::Base::from(1u64),
        total_outstanding: 500,
        total_interest_obligation: 50,
        reserve_amount: 100,
        report_block: 500,
        tx_commitment,
        tx_nonce,
    };
    let debris = ProveCoverageCallBuilder {
        input,
        prove_coverage_zkbin: zkbin.clone(),
        prove_coverage_pk: pk,
    }
    .build()
    .expect("the client must build a proof");

    // 100 * 10000 / (500 + 50) = 1818 bps — the ratio the builder derives.
    let revealed = |commitment: pallas::Base, nonce: pallas::Base| ProveCoverageRevealed {
        reserve_amount: pallas::Base::from(100u64),
        total_outstanding: pallas::Base::from(500u64),
        total_interest_obligation: pallas::Base::from(50u64),
        coverage_ratio_bps: pallas::Base::from(1818u64),
        tx_binding: dwow_sdk::crypto::poseidon_hash([
            pallas::Base::from(3u64),
            commitment,
            nonce,
        ]),
        tx_nonce: nonce,
    };

    let own = revealed(tx_commitment, tx_nonce).to_vec();
    assert_eq!(own.len(), 6, "prove_coverage instances six values");
    assert_eq!(own[5], tx_nonce, "the last instance is the nonce the proof was made with");
    assert_eq!(
        own[4],
        dwow_sdk::crypto::poseidon_hash([pallas::Base::from(3u64), tx_commitment, tx_nonce]),
        "the instance before it is poseidon_hash([3, tx_commitment, tx_nonce]) for the pair the \
         proof was made with — not a constant"
    );

    match verify_zkp(&debris.proofs[0], BEARER_BOND_PROVE_COVERAGE_ZKBIN, &own) {
        ZkVerifyResult::Ok => {}
        other => panic!(
            "OBL-C198: a prove_coverage proof bound to a non-zero tx pair does not verify \
             ({other:?}) — the client's `to_vec` order and the circuit's `constrain_instance` \
             order disagree."
        ),
    }

    // The negative control (R8): the same proof, verified against a *different* commitment's
    // instances, must be refused. If it verified, the binding would not be binding.
    let other_commitment = pallas::Base::from(0xDEADBEEFu64);
    let other = revealed(other_commitment, tx_nonce).to_vec();
    assert!(
        !matches!(
            verify_zkp(&debris.proofs[0], BEARER_BOND_PROVE_COVERAGE_ZKBIN, &other),
            ZkVerifyResult::Ok
        ),
        "a prove_coverage proof made over one tx_commitment must not verify under another — the \
         binding would not be load-bearing"
    );
}
