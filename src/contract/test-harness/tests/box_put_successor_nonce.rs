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

//! `Put`'s successor nonce is derived, not chosen — the falsifier for that constraint.
//!
//! `box/proof/put.zk` pins `new_state_nonce` to `base_add(old_state_nonce, ONE)`, the idiom
//! `purse/proof/deposit.zk` uses for the same value (assign to a name, then constrain). Before that
//! existed the successor was a free witness, so a `Put` could name the nonce it had just consumed: the
//! new leaf would equal the old one, whose nullifier the same call already spent, and the box could
//! never be consumed again.
//!
//! **Every arm here verifies, because creating a proof is not evidence.** halo2's `create_proof` — and
//! so `Proof::create` — produces a transcript for an *unsatisfying* assignment; `MockProver` is what
//! checks satisfaction. An arm that asserted on `Proof::create` would report success for exactly the
//! circuits it exists to catch. This file's first draft did that and all three arms "passed", including
//! a control that zeroes the nullifier witness; `tests/client_proof_self_verification.rs` had already
//! recorded the rule, and `stablecoin_governance_report.rs` before it. The instrument is
//! `verify_zkp(proof, zkbin, inputs)` — the same call the chain makes.
//!
//! The control arm is first among the negatives for that reason: it violates a *hash*-derived
//! constraint, so if it ever verifies, no arm below it means anything.

use dwow_contract_test_harness::harness::BoxHarness;
use dwow_core::zk::{verify_zkp, ZkVerifyResult};
use dwow_sdk::crypto::poseidon_hash;
use dwow_sdk::pasta::pallas;

/// The same artifact the contract embeds and the harness proves against.
const ZKBIN_BYTES: &[u8] = include_bytes!("../../box/proof/put.zk.bin");

fn contents() -> pallas::Base {
    poseidon_hash([pallas::Base::from(100u64)])
}

/// The derived successor verifies. `old_state_nonce` is zero in the fixture, so this is `old + 1`.
#[test]
fn a_put_with_the_derived_successor_nonce_verifies() {
    let h = BoxHarness::spawn();
    let r = h
        .put_contents_with_successor(contents(), pallas::Base::from(1u64))
        .expect("the honest put must build");
    assert_eq!(
        verify_zkp(&r.proof, ZKBIN_BYTES, &r.inputs),
        ZkVerifyResult::Ok,
        "the derived successor (old_state_nonce + 1) must verify"
    );
}

/// The control: a long-standing, hash-derived constraint violated on purpose. `Proof::create` will
/// return `Ok` here — that is the point — and verification must reject it. If this ever returns `Ok`,
/// the arms below are not evidence of anything.
#[test]
fn a_put_with_a_zeroed_nullifier_witness_does_not_verify() {
    let h = BoxHarness::spawn();
    let r = h
        .put_with_zeroed_nullifier_witness()
        .expect("halo2 still produces proof bytes for an unsatisfying assignment");
    assert_eq!(
        verify_zkp(&r.proof, ZKBIN_BYTES, &r.inputs),
        ZkVerifyResult::InvalidProof,
        "the nullifier constraint must reject a zeroed witness"
    );
}

/// The footgun itself: the nonce this same `Put` consumes. The leaf would equal the old leaf, whose
/// nullifier the call spends — an unspendable box, self-inflicted.
#[test]
fn a_put_reusing_the_consumed_nonce_does_not_verify() {
    let h = BoxHarness::spawn();
    let r = h
        .put_contents_with_successor(contents(), pallas::Base::from(0u64))
        .expect("halo2 still produces proof bytes for an unsatisfying assignment");
    assert_eq!(
        verify_zkp(&r.proof, ZKBIN_BYTES, &r.inputs),
        ZkVerifyResult::InvalidProof,
        "a put that re-uses the nonce it consumes must not verify"
    );
}

/// Any other successor: the constraint is `old + 1`, not "not the old one".
#[test]
fn a_put_with_an_unrelated_successor_nonce_does_not_verify() {
    let h = BoxHarness::spawn();
    let r = h
        .put_contents_with_successor(contents(), pallas::Base::from(3u64))
        .expect("halo2 still produces proof bytes for an unsatisfying assignment");
    assert_eq!(
        verify_zkp(&r.proof, ZKBIN_BYTES, &r.inputs),
        ZkVerifyResult::InvalidProof,
        "only old_state_nonce + 1 is provable"
    );
}
