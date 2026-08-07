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

//! FeeThreshold_V1 proof construction — wallet→mempool admission gate.
//!
//! # Position in the Fee Lifecycle
//!
//! ```text
//! Wallet                    Mempool                  Miner                    Chain
//!   │                         │                       │                        │
//!   ├─ FeeThreshold_V1 ──────►│                       │                        │
//!   │  (fee >= threshold)     ├─ premium/general/     │                        │
//!   │                         │  reject               │                        │
//!   │                         │                       │                        │
//!   │                         │     transactions ────►│                        │
//!   │                         │     + fees            ├─ Build block ──────────►│
//!   │                         │                       │  + PoWReward            │
//!   │                         │                       │  + FeeCollectV1         │
//!   │                         │                       │                        │
//!   │                         │                       │                        ├─ Fee_V2
//!   │                         │                       │                        │  (no inflation)
//!   │                         │                       │                        ├─ FeeCollectV1
//!   │                         │                       │                        │  (claim + reset)
//! ```
//!
//! FeeThreshold_V1 is step 1: the wallet constructs this proof to satisfy the
//! mempool's admission requirement. The mempool verifies it to determine which
//! tier (premium/general) the transaction qualifies for. This is an **active
//! consensus payment pathway**, not defensive contract logic.
//!
//! # Why This Lives in the Wallet Crate
//!
//! | Proof | Category | Location | Reason |
//! |-------|----------|----------|--------|
//! | Fee_V2 | Defensive | `src/contract/native_token/` | Pedersen mass balance — proves no secret inflation. ZCash Orchard exploit defense-in-depth. Verified during accept_block via WASM. |
//! | FeeThreshold_V1 | Active payment | `bin/dww/src/` (this file) | Wallet→mempool gate — proves fee >= threshold for admission tier selection. |
//! | FeeCollectV1 | Active payment | `src/contract/native_token/` | Transfers accumulated fee pot to miner, resets accumulator. Contract logic. |
//!
//! # Circuit (fee_threshold_v1.zk)
//!
//! 4 witnesses: fee, threshold, tx_commitment, tx_binding
//! 2 public inputs: threshold, tx_binding
//!
//! The circuit constrains `fee >= threshold` without revealing the actual
//! fee amount. The mempool sees only the public inputs — it learns that
//! the fee meets the minimum but not the exact value. Privacy-preserving
//! fee admission: the mempool can enforce fee minimums without de-anonymizing
//! transaction values.

use rand::SeedableRng;

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
};
use dwow_sdk::blockchain::FeeAmount;
use dwow_sdk::pasta::pallas;

/// Convert a FeeAmount to a pallas::Base field element for ZK witness construction.
///
/// The wallet uses this to encode fee/threshold values into the FeeThreshold_V1
/// circuit's public inputs and witnesses. Private fee amounts are never exposed
/// on-chain — only the threshold is public (visible to the mempool for tier
/// selection).
pub(crate) fn fee_to_base(amount: FeeAmount) -> pallas::Base {
    pallas::Base::from(amount.get())
}

/// Create a FeeThreshold_V1 ZK proof: fee >= threshold.
///
/// This is step 1 of the fee flow (wallet → mempool gate). The wallet constructs
/// the transaction, builds this proof, and submits it to the mempool. The mempool
/// verifies the proof to determine admission tier (premium/general/reject).
///
/// # Circuit (fee_threshold_v1.zk)
///
/// - 4 witnesses: fee, threshold, tx_commitment, tx_binding
/// - 2 public inputs: threshold, tx_binding
/// - Constraint: fee >= threshold (combinatorial, no bit decomposition)
///
/// The fee amount remains private — the mempool learns only that the fee meets
/// or exceeds the threshold, not the exact value.
///
/// # Determinism
///
/// When `deterministic_zk_enabled()` is true (test mode), uses StdRng seeded
/// with 43 for reproducible proofs. In production, uses OsRng for cryptographic
/// security.
pub fn create_fee_threshold_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    fee_amount: FeeAmount,
    threshold: FeeAmount,
    tx_commitment: pallas::Base,
    tx_binding: pallas::Base,
) -> Result<Proof, dwow_core::Error> {
    // FeeThreshold_V1 witnesses (4): fee, threshold, tx_commitment, tx_binding
    let witnesses: Vec<Witness> = vec![
        Witness::Base(Value::known(fee_to_base(fee_amount))),
        Witness::Base(Value::known(fee_to_base(threshold))),
        Witness::Base(Value::known(tx_commitment)),
        Witness::Base(Value::known(tx_binding)),
    ];

    // Public inputs (2): threshold, tx_binding
    let public_inputs = vec![
        fee_to_base(threshold),
        tx_binding,
    ];

    let circuit = ZkCircuit::new(witnesses, zkbin);
    let rng: Box<dyn rand::RngCore + Send> = if dwow_native_token_contract::deterministic_zk_enabled() {
        Box::new(rand::rngs::StdRng::seed_from_u64(43))
    } else {
        Box::new(rand::rngs::OsRng)
    };
    Ok(Proof::create(pk, &[circuit], &public_inputs, rng)
        .map_err(|e| dwow_core::Error::Custom(format!("FeeThreshold_V1 proof synthesis: {}", e)))?)
}
