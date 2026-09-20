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

//! Adversarial tests for `stablecoin/proof/governance_report.zk`.
//!
//! One defect, repaired on 2026-09-20: the ratio divided by `total_debt` where the specification
//! says `outstanding = total_debt - total_redeemed` (`doc/src/contract/stablecoin.md:172`,
//! `model/mod.rs:1190`). Redeemed debt is not a liability, so the debt-based ratio *understates*
//! coverage — the direction that hides insolvency. The case below is chosen so the two differ
//! sharply: 1500 collateral against 1000 debt with 400 redeemed is `25000` bps over the outstanding
//! 600, and `15000` over the debt.
//!
//! Proving *and verifying*, as in `governance_ratio_bounds.rs`: an unsatisfied circuit still
//! produces proof bytes, so an assertion on `Proof::create` alone reports success for exactly the
//! circuits it is meant to catch.

use dwow_core::zk::{
    empty_witnesses, halo2::Value, verify_zkp, Proof, ProvingKey, Witness, ZkCircuit,
    ZkVerifyResult,
};
use dwow_core::zkas::ZkBinary;
use dwow_sdk::crypto::{poseidon_hash, PublicKey, SecretKey};
use dwow_sdk::pasta::pallas;
use rand::rngs::OsRng;

/// `BP_PRECISION` in the model.
const BPS: u64 = 10000;

/// `DENOM` in the circuit: `365 * 86400 * 10000`.
const DENOM: u64 = 315_360_000_000;

const ZKBIN_BYTES: &[u8] = include_bytes!("../../stablecoin/proof/governance_report.zk.bin");

fn governance_report_zkbin() -> ZkBinary {
    ZkBinary::decode(ZKBIN_BYTES, false).expect("governance_report.zk.bin decodes")
}

fn proving_key(zkbin: &ZkBinary) -> ProvingKey {
    let circuit = ZkCircuit::new(empty_witnesses(zkbin).expect("witnesses"), zkbin);
    ProvingKey::build(zkbin.k, &circuit).expect("ProvingKey::build")
}

/// Witness order is `GovernanceReportV2`'s: total_collateral, total_debt, outstanding, ratio,
/// interest_accrued, reporter_pub_x, reporter_pub_y, reporter_secret, rate_per_second,
/// time_elapsed, tx_commitment, tx_nonce, tx_binding. The public inputs are the first five of
/// those plus tx_binding and tx_nonce, in that order.
struct Report {
    total_collateral: u64,
    total_debt: u64,
    outstanding: u64,
    coverage_ratio_bps: u64,
    reporter_secret: pallas::Base,
    rate_per_second: u64,
    time_elapsed: u64,
}

impl Report {
    fn interest_accrued(&self) -> u64 {
        ((u128::from(self.total_debt) * u128::from(self.rate_per_second)
            * u128::from(self.time_elapsed))
            / u128::from(DENOM)) as u64
    }

    fn honest_ratio(&self) -> u64 {
        ((u128::from(self.total_collateral) * u128::from(BPS)) / u128::from(self.outstanding)) as u64
    }

    /// Prove and verify with `ratio` in place of the honest one.
    fn attempt(&self, ratio: u64, pk: &ProvingKey) -> Result<(), String> {
        let reporter = PublicKey::from_secret(SecretKey::from_base(self.reporter_secret));
        #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity")]
        let (rx, ry) = reporter.xy().expect("pk not identity");

        let (tx_commitment, tx_nonce) = (pallas::Base::zero(), pallas::Base::zero());
        let tx_binding = poseidon_hash([pallas::Base::from(3u64), tx_commitment, tx_nonce]);
        let interest = self.interest_accrued();

        let witnesses = vec![
            Witness::Base(Value::known(pallas::Base::from(self.total_collateral))),
            Witness::Base(Value::known(pallas::Base::from(self.total_debt))),
            Witness::Base(Value::known(pallas::Base::from(self.outstanding))),
            Witness::Base(Value::known(pallas::Base::from(ratio))),
            Witness::Base(Value::known(pallas::Base::from(interest))),
            Witness::Base(Value::known(rx)),
            Witness::Base(Value::known(ry)),
            Witness::Base(Value::known(self.reporter_secret)),
            Witness::Base(Value::known(pallas::Base::from(self.rate_per_second))),
            Witness::Base(Value::known(pallas::Base::from(self.time_elapsed))),
            Witness::Base(Value::known(tx_commitment)),
            Witness::Base(Value::known(tx_nonce)),
            Witness::Base(Value::known(tx_binding)),
        ];
        let inputs = [
            pallas::Base::from(self.total_collateral),
            pallas::Base::from(self.total_debt),
            pallas::Base::from(self.outstanding),
            pallas::Base::from(ratio),
            pallas::Base::from(interest),
            tx_binding,
            tx_nonce,
        ];

        let zkbin = governance_report_zkbin();
        let circuit = ZkCircuit::new(witnesses, &zkbin);
        let proof =
            Proof::create(pk, &[circuit], &inputs, OsRng).map_err(|e| format!("synthesis: {e:?}"))?;
        match verify_zkp(&proof, ZKBIN_BYTES, &inputs) {
            ZkVerifyResult::Ok => Ok(()),
            other => Err(format!("verification: {other:?}")),
        }
    }
}

fn report() -> Report {
    Report {
        total_collateral: 1500,
        total_debt: 1000,
        outstanding: 600, // 1000 issued, 400 redeemed
        coverage_ratio_bps: 0, // set by the caller
        reporter_secret: pallas::Base::from(7u64),
        rate_per_second: 10,
        time_elapsed: 3600,
    }
}

#[test]
fn governance_report_accepts_the_outstanding_ratio() {
    let zkbin = governance_report_zkbin();
    let pk = proving_key(&zkbin);
    let r = report();
    assert_eq!(r.interest_accrued(), 0, "36e6 / 315.36e9 truncates to zero");
    assert_eq!(r.honest_ratio(), 25000, "1500 over the outstanding 600");
    r.attempt(r.honest_ratio(), &pk).expect("the honest ratio must prove and verify");
}

#[test]
fn governance_report_rejects_the_debt_denominator() {
    // The pre-fix value: collateral over *debt*, 15000 bps where the outstanding-denominated
    // answer is 25000. Understating coverage is what hides insolvency, so this must not prove.
    let zkbin = governance_report_zkbin();
    let pk = proving_key(&zkbin);
    let r = report();
    let debt_denominated = (1500u128 * u128::from(BPS) / 1000u128) as u64;
    assert_eq!(debt_denominated, 15000);
    assert_ne!(debt_denominated, r.honest_ratio());
    assert!(
        r.attempt(debt_denominated, &pk).is_err(),
        "the ratio must be over `outstanding`, not over `total_debt`",
    );
}

#[test]
fn governance_report_rejects_a_denominator_of_zero() {
    // `outstanding = 0` (everything redeemed) has no ratio; the quotient-remainder's `+ 1` upper
    // bound makes it unsatisfiable rather than admitting an arbitrary quotient, and `ratio = 0` is
    // the value an attacker would try.
    let zkbin = governance_report_zkbin();
    let pk = proving_key(&zkbin);
    let mut r = report();
    r.outstanding = 0;
    assert!(
        r.attempt(0, &pk).is_err(),
        "a zero denominator must not be provable",
    );
}
