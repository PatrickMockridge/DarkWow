/* This file is part of DarkWow
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

//! FeeThreshold_V1 proof construction — wallet→mempool admission gate.
//!
//! # Domain
//!
//! `[domain: fee_signalling]` — NOT consensus-critical. This proof gates
//! mempool admission; it is verified at mempool admission time and re-verified
//! by miners before block inclusion. It is NOT verified at `accept_block`.
//!
//! # Architecture
//!
//! Two WASM widgets, one zkas circuit (fee-spec.md §0):
//! - **Proving widget** (wallet-side): the circuit IS the ground truth —
//!   witness count, order, and types come from `fee_threshold_v1.zk`, never
//!   hardcoded in Rust. This module uses `empty_witnesses()` to derive
//!   witness structure from the compiled circuit.
//! - **Verification widget** (mempool/miner-side): the contract entrypoint's
//!   `fee_v2_get_metadata()` already returns `[(FeeThreshold_V1,
//!   [threshold, tx_binding])]`. The mempool loads the WASM module, calls
//!   `__metadata`, and calls `verify_zkp()` with the extracted public inputs.
//!
//! # Circuit (fee_threshold_v1.zk)
//!
//! k = 11, field = "pallas"
//! 4 witnesses: fee, threshold, tx_commitment, tx_binding
//! 2 public inputs: threshold, tx_binding
//! Constraint: range_check(64, fee - threshold)
//! tx_binding is internally constrained to poseidon(3, tx_commitment, threshold)
//!
//! Spec: wallet.md §6.4.3, fee-spec.md §5.5.

use rand::SeedableRng;

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
};
use dwow_sdk::{
    blockchain::FeeAmount,
    pasta::pallas,
};

use crate::model::fee::ThresholdTxBinding;

/// Convert a FeeAmount to a pallas::Base field element for ZK witness construction.
///
/// Per fee-spec.md §6.1: consensus numeric domains SHALL be nominal types.
/// This function is the single transition point where FeeAmount crosses into
/// the ZK field. All call sites use this rather than bare `.get()` + `from()`.
pub fn fee_to_base(amount: FeeAmount) -> pallas::Base {
    pallas::Base::from(amount.get())
}

/// Create a FeeThreshold_V1 ZK proof: fee >= threshold.
///
/// This is the wallet→mempool admission gate. The wallet constructs the
/// proof and submits it with the transaction. The mempool verifies it via
/// the verification WASM widget to determine admission tier
/// (premium/general/reject).
///
/// # Circuit-Grounded Witness Binding
///
/// Witness order is derived FROM THE CIRCUIT, never hardcoded:
/// 1. `empty_witnesses(&zkbin)` returns witnesses in circuit declaration order
/// 2. Each witness is set by position matching the circuit's witness table
/// 3. The witness count is verified against the circuit's expected count
///
/// Circuit witness order (fee_threshold_v1.zk witness block):
///   [0] fee           — Base, private fee amount
///   [1] threshold     — Base, tier threshold being proved against
///   [2] tx_commitment — Base, binds proof to specific transaction
///   [3] tx_binding    — Base, poseidon(3, tx_commitment, threshold)
///
/// # Determinism
///
/// When `deterministic_zk_enabled()` is true (test mode), uses StdRng seeded
/// with 43 for reproducible proofs. In production, uses OsRng.
pub fn create_fee_threshold_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    fee_amount: FeeAmount,
    threshold: FeeAmount,
    tx_commitment: pallas::Base,
    threshold_tx_binding: ThresholdTxBinding,
) -> Result<Proof, dwow_core::Error> {
    // Circuit-grounded witness binding: get witness array in circuit order.
    // empty_witnesses() returns witnesses matching zkbin.witnesses (the
    // circuit's witness block declarations). This guarantees correct count,
    // types, and order — no manual vec![] construction.
    let mut witnesses = dwow_core::zk::vm_heap::empty_witnesses(zkbin)
        .map_err(|e| dwow_core::Error::Custom(format!(
            "FeeThreshold_V1 empty_witnesses failed: {}", e
        )))?;

    // Safety: verify witness count matches circuit expectation
    assert_eq!(
        witnesses.len(),
        4,
        "FeeThreshold_V1 circuit expects 4 witnesses, got {}",
        witnesses.len()
    );

    // Bind witnesses by position, matching circuit witness order in
    // fee_threshold_v1.zk witness block:
    //   [0] fee, [1] threshold, [2] tx_commitment, [3] tx_binding
    witnesses[0] = Witness::Base(Value::known(fee_to_base(fee_amount)));
    witnesses[1] = Witness::Base(Value::known(fee_to_base(threshold)));
    witnesses[2] = Witness::Base(Value::known(tx_commitment));
    witnesses[3] = Witness::Base(Value::known(threshold_tx_binding.inner()));

    // Public inputs (2): threshold, tx_binding
    // Order matches constrain_instance calls in fee_threshold_v1.zk circuit block
    let public_inputs = vec![
        fee_to_base(threshold),
        threshold_tx_binding.inner(),
    ];

    let circuit = ZkCircuit::new(witnesses, zkbin);

    let proof = if crate::deterministic_zk_enabled() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(43);
        Proof::create(pk, &[circuit], &public_inputs, &mut rng)
            .map_err(|e| dwow_core::Error::Custom(format!(
                "FeeThreshold_V1 proof synthesis: {}", e
            )))?
    } else {
        Proof::create(pk, &[circuit], &public_inputs, &mut rand::rngs::OsRng)
            .map_err(|e| dwow_core::Error::Custom(format!(
                "FeeThreshold_V1 proof synthesis: {}", e
            )))?
    };
    Ok(proof)
}

#[cfg(test)]
mod tests {
    use super::*;
    use dwow_core::zk::{ProvingKey, VerifyingKey, ZkCircuit};
    use dwow_core::zk::vm_heap::empty_witnesses;
    use dwow_core::zkas::ZkBinary;
    use dwow_sdk::crypto::{poseidon_hash, constants::DRK_POSEIDON_DOMAIN_TX_BINDING};


    /// L1.5-FW-3d: MockProver diagnostic — Mudra pattern for constraint-level
    /// validation BEFORE Proof::create. This catches constraint-system errors
    /// (wrong row, missing gate) with specific messages instead of the opaque
    /// "General synthesis error" that Proof::create produces.
    #[test]
    fn test_mock_prover_fee_threshold() {
        let zkbin = ZkBinary::decode(
            super::super::zkbins::NATIVE_TOKEN_CONTRACT_ZKAS_FEE_THRESHOLD_V1_BIN,
            false,
        ).expect("decode threshold zkbin");

        let fee = FeeAmount::new(150_000_000);
        let threshold = FeeAmount::new(1);
        let tx_commitment = pallas::Base::from(12345u64);
        let tx_binding = poseidon_hash([
            DRK_POSEIDON_DOMAIN_TX_BINDING,
            tx_commitment,
            fee_to_base(threshold),
        ]);

        let witnesses = vec![
            Witness::Base(Value::known(fee_to_base(fee))),
            Witness::Base(Value::known(fee_to_base(threshold))),
            Witness::Base(Value::known(tx_commitment)),
            Witness::Base(Value::known(tx_binding)),
        ];
        let public_inputs = vec![fee_to_base(threshold), tx_binding];
        let circuit = ZkCircuit::new(witnesses, &zkbin);

        let prover = halo2_proofs::dev::MockProver::run(
            zkbin.k as u32, &circuit, vec![public_inputs.clone()]
        ).expect("[L1.5-FW-3d] MockProver::run should succeed");

        prover.assert_satisfied();
    }

    /// Load the fee_threshold_v1 ZK binary and build its proving + verifying keys.
    ///
    /// Uses the contract crate's zkbins constant — zero `include_bytes!`
    /// crossing crate boundaries (G3).
    fn load_threshold_zk_materials() -> (ZkBinary, ProvingKey, VerifyingKey) {
        let zkbin = ZkBinary::decode(
            super::super::zkbins::NATIVE_TOKEN_CONTRACT_ZKAS_FEE_THRESHOLD_V1_BIN,
            false,
        ).expect("decode threshold zkbin");
        let empty_wits = empty_witnesses(&zkbin).expect("empty_witnesses");
        let circuit = ZkCircuit::new(empty_wits, &zkbin);
        let pk = ProvingKey::build(zkbin.k, &circuit).expect("ProvingKey::build");
        let vk = VerifyingKey::build(zkbin.k, &circuit).expect("VerifyingKey::build");
        (zkbin, pk, vk)
    }

    /// L1.5-FW-3a: deterministic_zk mode produces byte-identical proofs
    /// for identical inputs (seed 43). Verifies proof against public inputs.
    #[test]
    fn test_fee_threshold_proof_determinism() {
        crate::enable_deterministic_zk();
        let (zkbin, pk, vk) = load_threshold_zk_materials();

        let fee = FeeAmount::new(150_000_000);
        let threshold = FeeAmount::new(1);
        let tx_commitment = pallas::Base::from(12345u64);
        let threshold_tx_binding = ThresholdTxBinding::compute(tx_commitment, threshold);

        let proof1 = create_fee_threshold_proof(
            &zkbin, &pk, fee, threshold, tx_commitment, threshold_tx_binding,
        ).expect("first proof");

        // Verify the proof against public inputs
        let public_inputs = vec![fee_to_base(threshold), threshold_tx_binding.inner()];
        proof1.verify(&vk, &public_inputs).expect("[L1.5-FW-3a] proof must verify");

        let proof2 = create_fee_threshold_proof(
            &zkbin, &pk, fee, threshold, tx_commitment, threshold_tx_binding,
        ).expect("second proof");

        // Same inputs → byte-identical proofs
        assert_eq!(proof1, proof2,
            "[L1.5-FW-3a] deterministic proofs must be identical for identical inputs");
        assert_eq!(proof1.as_ref(), proof2.as_ref(),
            "[L1.5-FW-3a] proof bytes must match");
    }

    /// L1.5-FW-3b: tampered input → proof differs.
    /// Different threshold produces a different proof (different public input).
    /// Each proof verifies against its own VerifyingKey.
    #[test]
    fn test_fee_threshold_proof_different_threshold_yields_different_proof() {
        crate::enable_deterministic_zk();
        let (zkbin, pk, vk) = load_threshold_zk_materials();

        let fee = FeeAmount::new(150_000_000);
        let tx_commitment = pallas::Base::from(12345u64);
        let threshold_42 = FeeAmount::new(1);
        let threshold_100 = FeeAmount::new(100_000_000);

        let binding_42 = ThresholdTxBinding::compute(tx_commitment, threshold_42);
        let binding_100 = ThresholdTxBinding::compute(tx_commitment, threshold_100);

        let proof_t42 = create_fee_threshold_proof(
            &zkbin, &pk, fee, threshold_42, tx_commitment, binding_42,
        ).expect("proof with threshold 42M");
        proof_t42.verify(&vk, &[fee_to_base(threshold_42), binding_42.inner()])
            .expect("[L1.5-FW-3b] proof_t42 must verify");

        let proof_t100 = create_fee_threshold_proof(
            &zkbin, &pk, fee, threshold_100, tx_commitment, binding_100,
        ).expect("proof with threshold 100M");
        proof_t100.verify(&vk, &[fee_to_base(threshold_100), binding_100.inner()])
            .expect("[L1.5-FW-3b] proof_t100 must verify");

        assert_ne!(proof_t42, proof_t100,
            "[L1.5-FW-3b] different threshold must produce different proof");
    }

    /// L1.5-FW-3c: tampered witness (tx_commitment) → proof differs.
    /// Each proof verifies against its own VerifyingKey.
    #[test]
    fn test_fee_threshold_proof_different_witness_yields_different_proof() {
        crate::enable_deterministic_zk();
        let (zkbin, pk, vk) = load_threshold_zk_materials();

        let fee = FeeAmount::new(150_000_000);
        let threshold = FeeAmount::new(1);
        let tx_commitment_a = pallas::Base::from(12345u64);
        let tx_commitment_b = pallas::Base::from(54321u64);

        let binding_a = ThresholdTxBinding::compute(tx_commitment_a, threshold);
        let binding_b = ThresholdTxBinding::compute(tx_commitment_b, threshold);

        let proof_a = create_fee_threshold_proof(
            &zkbin, &pk, fee, threshold, tx_commitment_a, binding_a,
        ).expect("proof with commitment A");
        proof_a.verify(&vk, &[fee_to_base(threshold), binding_a.inner()])
            .expect("[L1.5-FW-3c] proof_a must verify");

        let proof_b = create_fee_threshold_proof(
            &zkbin, &pk, fee, threshold, tx_commitment_b, binding_b,
        ).expect("proof with commitment B");
        proof_b.verify(&vk, &[fee_to_base(threshold), binding_b.inner()])
            .expect("[L1.5-FW-3c] proof_b must verify");

        assert_ne!(proof_a, proof_b,
            "[L1.5-FW-3c] different tx_commitment must produce different proof");
    }
}
