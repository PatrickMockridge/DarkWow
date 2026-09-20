/* This file is part of DarkWow
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * DarkWow is a tool for people and nations to establish sovereignty
 * according to human rights law. See the UN Declaration on the Rights
 * of Indigenous Peoples and associated documents:
 * https://documents.un.org/doc/undoc/g26/031/70/pdf/g2603170.pdf
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

//! Adversarial tests for `bearer_bond/proof/prove_coverage.zk`.
//!
//! Three defects were repaired in that circuit on 2026-09-20, and each one is a *rejected proof*
//! that used to be accepted:
//!
//! 1. **the bound.** The quotient-remainder's upper bound was `+ 10000` instead of `+ 1`, so
//!    `q·D ≤ R·BPS < (q+10000)·D` admitted a 10000-bps-wide window. For `R = D = 100` the honest
//!    ratio is 10000 and `q = 9999` was accepted — an understated solvency report, and
//!    understating is what trips `is_coverage_voided` and forces an emergency unstake.
//! 2. **the denominator.** The circuit divided by `total_outstanding` alone and dropped the accrued
//!    interest, overstating coverage. The model (`sim/contracts/bearer_bond.py:271-275`) says the
//!    divisor is `total_outstanding + total_interest_obligation`.
//! 3. **the wrap.** With no `range_check` on `coverage_ratio_bps`, a prover could choose a quotient
//!    whose products wrap modulo `p` back into the accepted region. `(p + 49999) / 2` does exactly
//!    that for `R = 5, D = 2`; the kernel-checked counterexample is `BaseDivGadget.qr_needs_bound`
//!    in `proofs/lean/src/DarkFi/BaseDivGadget.lean`, and `range_check(64, …)` is what rejects it.
//!
//! Each assertion is that proving *fails*, with the honest witness as the positive control: a
//! circuit that rejected everything would satisfy the negative assertions and fail the control.

use dwow_core::zk::{
    empty_witnesses, halo2::Value, verify_zkp, Proof, ProvingKey, Witness, ZkCircuit,
    ZkVerifyResult,
};
use dwow_core::zkas::ZkBinary;
use dwow_sdk::pasta::{group::ff::Field, pallas};
use rand::rngs::OsRng;

/// `BP_PRECISION` in the model.
const BPS: u64 = 10000;

const ZKBIN_BYTES: &[u8] = include_bytes!("../../bearer_bond/proof/prove_coverage.zk.bin");

fn prove_coverage_zkbin() -> ZkBinary {
    ZkBinary::decode(ZKBIN_BYTES, false).expect("prove_coverage.zk.bin decodes")
}

fn proving_key(zkbin: &ZkBinary) -> ProvingKey {
    let circuit = ZkCircuit::new(empty_witnesses(zkbin).expect("witnesses"), zkbin);
    ProvingKey::build(zkbin.k, &circuit).expect("ProvingKey::build")
}

/// `coverage_ratio_bps = floor(reserve * BPS / (outstanding + interest))` — the model's formula.
fn honest_ratio(reserve: u64, outstanding: u64, interest: u64) -> u64 {
    let obligation = u128::from(outstanding) + u128::from(interest);
    ((u128::from(reserve) * u128::from(BPS)) / obligation) as u64
}

/// Prove **and verify**. Witness order is `ProveCoverage_V2`'s: reserve, outstanding, interest
/// obligation, ratio; the public inputs are the same four values in the same order.
///
/// The verification step is not decoration. An unsatisfied circuit still produces proof bytes —
/// `Proof::create` runs the synthesis and commits; it does not check satisfaction, which is what
/// the verifier's pairing check does. Asserting on `Proof::create` alone would have passed every
/// negative case below, and did, before this was written.
fn attempt(pk: &ProvingKey, w: [pallas::Base; 4]) -> Result<(), String> {
    let zkbin = prove_coverage_zkbin();
    let witnesses = w
        .iter()
        .map(|x| Witness::Base(Value::known(*x)))
        .collect::<Vec<_>>();
    let circuit = ZkCircuit::new(witnesses, &zkbin);
    let proof =
        Proof::create(pk, &[circuit], &w, OsRng).map_err(|e| format!("synthesis: {e:?}"))?;
    match verify_zkp(&proof, ZKBIN_BYTES, &w) {
        ZkVerifyResult::Ok => Ok(()),
        other => Err(format!("verification: {other:?}")),
    }
}

fn attempt_u64(
    pk: &ProvingKey,
    reserve: u64,
    outstanding: u64,
    interest: u64,
    ratio: u64,
) -> Result<(), String> {
    attempt(
        pk,
        [
            pallas::Base::from(reserve),
            pallas::Base::from(outstanding),
            pallas::Base::from(interest),
            pallas::Base::from(ratio),
        ],
    )
}

#[test]
fn prove_coverage_accepts_the_honest_ratio() {
    // Positive control, with a non-zero interest term on purpose: that is the case whose
    // denominator the pre-fix circuit got wrong, so a circuit that still divided by
    // `total_outstanding` would fail here rather than in one of the negative tests.
    let zkbin = prove_coverage_zkbin();
    let pk = proving_key(&zkbin);
    let (reserve, outstanding, interest) = (150u64, 100u64, 40u64);
    let ratio = honest_ratio(reserve, outstanding, interest);
    assert_eq!(ratio, 10714, "150 over 140 in basis points");
    attempt_u64(&pk, reserve, outstanding, interest, ratio)
        .expect("the honest ratio must prove and verify");
}

#[test]
fn prove_coverage_rejects_an_understated_ratio() {
    // `9999` against `R = D = 100`, whose honest ratio is 10000, sat inside the `+ 10000` window.
    let zkbin = prove_coverage_zkbin();
    let pk = proving_key(&zkbin);
    assert!(
        attempt_u64(&pk, 100, 100, 0, 9999).is_err(),
        "a ratio of 9999 bps must not be provable when the honest ratio is 10000",
    );
}

#[test]
fn prove_coverage_rejects_a_dropped_interest_term() {
    // The denominator: `reserve / outstanding` with the interest dropped gives 15000 here against
    // an honest 10714 — an issuer with unpaid interest reporting better coverage than it has.
    let zkbin = prove_coverage_zkbin();
    let pk = proving_key(&zkbin);
    let dropped = (150u128 * u128::from(BPS) / 100u128) as u64;
    assert_eq!(dropped, 15000);
    assert!(
        attempt_u64(&pk, 150, 100, 40, dropped).is_err(),
        "the ratio must be over the total obligation, not over outstanding principal alone",
    );
}

#[test]
fn prove_coverage_rejects_a_wrapped_quotient() {
    // `(p + 49999) / 2` for `R = 5, D = 2`: the honest quotient is 25000, and the wrapped one
    // satisfies *both* comparisons modulo `p`, which is what the pre-fix circuit checked. Built
    // here as `-1/2 + 25000`, which is `(p - 1)/2 + 25000` — the same field element as
    // `(p + 49999)/2`, without a 77-digit literal in the test.
    let zkbin = prove_coverage_zkbin();
    let pk = proving_key(&zkbin);
    #[expect(clippy::unwrap_used, reason = "2 is invertible in the Pallas field")]
    let half = pallas::Base::from(2u64).invert().unwrap();
    let wrapped = pallas::Base::from(25000u64) - half;

    // The property that makes it a wrap: doubling it gives 49999 ≤ 5 * 10000, so the lower
    // comparison holds, and doubling the successor gives 50001 > 50000, so the upper one does too.
    assert_eq!(wrapped * pallas::Base::from(2u64), pallas::Base::from(49999u64));
    assert_eq!(
        (wrapped + pallas::Base::from(1u64)) * pallas::Base::from(2u64),
        pallas::Base::from(50001u64),
    );

    attempt_u64(&pk, 5, 2, 0, 25000).expect("positive control: the honest ratio for 5 over 2 is 25000");
    assert!(
        attempt(
            &pk,
            [
                pallas::Base::from(5u64),
                pallas::Base::from(2u64),
                pallas::Base::from(0u64),
                wrapped,
            ],
        )
        .is_err(),
        "a quotient whose products wrap the field must not be provable",
    );
}
