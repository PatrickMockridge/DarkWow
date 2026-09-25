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

//! Witnesses for the VK cache's single entry point.
//!
//! `cached_verifying_key` exists because VK derivation costs `O(k · 2^k)` — the
//! derivations measured on 2026-09-25 were k=11 median 0.83 s, k=13 2.91 s, k=14
//! 5.77 s — and `zkas_db_set` derived one per deployed circuit *without* going
//! through the cache. In one contract-deploy sweep that was 1246 derivations for
//! 141 distinct circuits; 1105 of them re-derived a circuit the process already
//! held. These tests pin the two properties that make the cache safe to rely on:
//! that a repeat lookup does not derive again, and that what it returns is the same
//! key a direct derivation produces.

use std::sync::Arc;

use crate::{
    zk::{cached_verifying_key, empty_witnesses, VerifyingKey, ZkCircuit},
    zkas::ZkBinary,
};

/// Two real circuits, taken as the two smallest zkas binaries in the tree
/// (`roulette/settle_bet` 152 bytes, `stablecoin/init` 159) so the witness costs
/// milliseconds instead of the seconds a k=13+ derivation takes. They are real
/// artifacts rather than a fixture precisely so the decode path is exercised.
const CIRCUIT_A: &[u8] = include_bytes!("../../src/contract/roulette/proof/settle_bet.zk.bin");
const CIRCUIT_B: &[u8] = include_bytes!("../../src/contract/stablecoin/proof/init.zk.bin");

/// The witness for "no redundant derivation".
///
/// `Arc::ptr_eq` is the assertion rather than a wall-clock comparison: a second
/// derivation allocates a new `VerifyingKey`, so an identical pointer is proof that
/// the second lookup returned the stored value and did not derive it. A timing
/// assertion would be flaky; this one cannot pass while the cache is bypassed.
#[test]
fn a_second_lookup_of_the_same_circuit_does_not_derive_again() {
    let first = cached_verifying_key(CIRCUIT_A).expect("circuit A must derive a key");
    let second = cached_verifying_key(CIRCUIT_A).expect("circuit A must derive a key");
    assert!(
        Arc::ptr_eq(&first, &second),
        "the second lookup for identical zkas bytes returned a DIFFERENT allocation, \
         so it re-derived the key instead of hitting the cache — the defect this \
         function was introduced to remove"
    );
}

/// Negative control for the key: the cache is keyed on the bytes, so a different
/// circuit must not be served the first one's key.
#[test]
fn a_different_circuit_is_not_served_the_first_circuit_key() {
    let a = cached_verifying_key(CIRCUIT_A).expect("circuit A must derive a key");
    let b = cached_verifying_key(CIRCUIT_B).expect("circuit B must derive a key");
    assert!(
        !Arc::ptr_eq(&a, &b),
        "two different circuits were served the same VerifyingKey — the cache key \
         is not the circuit"
    );
}

/// Negative control for the value, and the reason this file is four tests rather
/// than two: the cached key must equal one derived directly. Without this, a cache
/// that returned a stale or mismatched key for the right bytes would pass every
/// other test here while making every proof verified against it meaningless.
#[test]
fn the_cached_key_equals_a_directly_derived_one() {
    let cached = cached_verifying_key(CIRCUIT_A).expect("circuit A must derive a key");

    let zkbin = ZkBinary::decode(CIRCUIT_A, false).expect("circuit A must decode");
    let witnesses = empty_witnesses(&zkbin).expect("circuit A must have empty witnesses");
    let circuit = ZkCircuit::new(witnesses, &zkbin);
    let direct = VerifyingKey::build(zkbin.k, &circuit).expect("circuit A must build directly");

    // Compare by the serialized form: it is what `zkas_db_set` stores, so a
    // mismatch here is a mismatch in persisted state, which is the thing that
    // would actually break.
    let mut cached_bytes = vec![];
    cached.write(&mut cached_bytes).expect("serialize cached key");
    let mut direct_bytes = vec![];
    direct.write(&mut direct_bytes).expect("serialize direct key");

    assert_eq!(
        cached_bytes, direct_bytes,
        "the cached key does not match a directly derived one for the same zkas \
         bytes ({} cached bytes vs {} direct)",
        cached_bytes.len(),
        direct_bytes.len()
    );
}

/// The failure path stays a `None` rather than a panic: `zkas_db_set` turns it into
/// a `DB_SET_FAILED` return, and a panic here would abort the node on a malformed
/// circuit.
#[test]
fn bytes_that_are_not_zkas_yield_none() {
    assert!(
        cached_verifying_key(b"these bytes are not a zkas binary").is_none(),
        "a non-zkas byte string produced a VerifyingKey"
    );
}
