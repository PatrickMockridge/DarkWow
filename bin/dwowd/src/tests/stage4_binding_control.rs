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

//! `OBL-C198` stage 4's end-to-end control — the loop's wiring, with a real proof.
//!
//! The unit controls in `src/linear/src/zk_verifier.rs` exercise the predicate
//! (`tx_binding_mismatch`) and the precondition (`decode_and_reconcile`), but neither drives a
//! real proof through `verify_core_tx_with_tables` and makes the new `BindingMismatch` branch
//! fire — nothing in `bin/` or `src/contract/` named the binding at all. A check is not a check
//! until something can make it fail (AGENTS.md R8), so this test does: a **valid** proof made
//! over commitment A, presented in a transaction whose reconciled commitment is B, must be
//! refused naming the binding.
//!
//! **Why the metadata table is built here and not taken from a contract's arm.** A migrated arm
//! derives its published `tx_binding` from `get_tx_commitment()` — the witness's own
//! `tx_commitment` field, which stage 3a has already forced equal to `commitment_of_calls`. So
//! for a migrated contract the published binding and the reconciled commitment cannot disagree,
//! and a proof made over a *different* commitment is refused earlier, by `verify_zkp` as
//! `InvalidProof` (the supplied instance would not match the proof's own public input). The
//! branch this test drives is the one that fires when an arm publishes a binding for a
//! commitment the transaction does not carry — precisely the echo/constant arm `OBL-C198`
//! exists to close, and the shape `auction` still ships (`OBL-C212`). Constructing that table
//! directly is the only way to reach the branch without an un-migrated contract in the tree.

use std::sync::Arc;

use dwow_bearer_bond_contract::client::prove_coverage::{
    ProveCoverageCallBuilder, ProveCoverageCallInput, ProveCoverageRevealed,
};
use dwow_bearer_bond_contract::BEARER_BOND_CONTRACT_ZKAS_PROVE_COVERAGE_NS_V2;
use dwow_chain::zk_verifier::{verify_core_tx_with_tables, VerifyError};
use dwow_core::zk::{empty_witnesses, Proof, ProvingKey, ZkCircuit};
use dwow_core::zkas::ZkBinary;
use dwow_sdk::crypto::constants::DRK_POSEIDON_DOMAIN_TX_BINDING;
use dwow_sdk::crypto::contract_id::SMART_CONTRACT_ZKAS_DB_NAME;
use dwow_sdk::crypto::pasta_prelude::PrimeField;
use dwow_sdk::crypto::{poseidon_hash, ContractId};
use dwow_sdk::dark_tree::DarkLeaf;
use dwow_sdk::pasta::pallas;
use dwow_sdk::tx::ContractCall;

const PROVE_COVERAGE_ZKBIN: &[u8] =
    include_bytes!("../../../../src/contract/bearer_bond/proof/prove_coverage.zk.bin");

fn proving_key(zkbin: &ZkBinary) -> ProvingKey {
    let circuit = ZkCircuit::new(empty_witnesses(zkbin).expect("witnesses"), zkbin);
    ProvingKey::build(zkbin.k, &circuit).expect("ProvingKey::build")
}

/// A real `ProveCoverage_V2` proof over `tx_commitment`, and the public-input vector the circuit
/// instances for it (the tx pair last).
fn prove_coverage(
    zkbin: &ZkBinary,
    tx_commitment: pallas::Base,
    tx_nonce: pallas::Base,
) -> (Proof, Vec<pallas::Base>) {
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
        prove_coverage_pk: proving_key(zkbin),
    }
    .build()
    .expect("the client must build a proof");

    // 100 * 10000 / (500 + 50) = 1818 bps — the ratio the builder derives.
    let pubvals = ProveCoverageRevealed {
        reserve_amount: pallas::Base::from(100u64),
        total_outstanding: pallas::Base::from(500u64),
        total_interest_obligation: pallas::Base::from(50u64),
        coverage_ratio_bps: pallas::Base::from(1818u64),
        tx_binding: poseidon_hash([DRK_POSEIDON_DOMAIN_TX_BINDING, tx_commitment, tx_nonce]),
        tx_nonce,
    }
    .to_vec();
    (debris.proofs[0].clone(), pubvals)
}

/// A store holding the bearer_bond `ProveCoverage_V2` zkas under the key `load_zkbin` reads.
fn store_with_zkas(cid: &ContractId) -> dwow_chain::LinearStore {
    let tmp = sled::Config::new().temporary(true).open().expect("temp sled");
    let store = dwow_chain::LinearStore::new(Arc::new(tmp)).expect("LinearStore");
    let prefix = cid.hash_state_id(SMART_CONTRACT_ZKAS_DB_NAME);
    let ns = dwow_serial::serialize(&BEARER_BOND_CONTRACT_ZKAS_PROVE_COVERAGE_NS_V2);
    let key = [&prefix[..], &ns[..]].concat();
    let value = dwow_serial::serialize(&(PROVE_COVERAGE_ZKBIN.to_vec(), Vec::<u8>::new()));
    store.set_contract_data(&key, &value).expect("store the zkas");
    store
}

/// A valid proof made over commitment A must verify when the transaction carries A, and must be
/// refused — naming the binding, not the proof — when the transaction carries B.
#[test]
fn stage_4_refuses_a_valid_proof_bound_to_a_different_commitment() {
    let zkbin =
        ZkBinary::decode(PROVE_COVERAGE_ZKBIN, false).expect("prove_coverage.zk.bin decodes");
    let nonce = pallas::Base::from(9u64);

    // The commitment the proof is made over, and a *different* one a transaction may carry.
    let proved_for = pallas::Base::from(0xC0FFEEu64);
    let carried = pallas::Base::from(0xC0FFEFu64);

    let (proof, pubvals) = prove_coverage(&zkbin, proved_for, nonce);
    let cid = crate::tests::blockchain::derive_contract_id_from_name("bearer_bond");
    let store = store_with_zkas(&cid);
    let ns = BEARER_BOND_CONTRACT_ZKAS_PROVE_COVERAGE_NS_V2;

    let tx_carrying = |commitment: pallas::Base| dwow_core::tx::Transaction {
        calls: vec![DarkLeaf {
            data: ContractCall { contract_id: cid, data: vec![] },
            children_indexes: vec![],
            parent_index: None,
        }],
        proofs: vec![vec![proof.clone()]],
        tx_commitment: commitment.to_repr(),
        nullifiers: vec![],
    };
    let table = vec![vec![(ns.to_string(), pubvals.clone())]];

    // Positive control: the transaction *is* the one the proof was made over → accepted. Without
    // this, the refusal below could pass on a proof that simply never verifies.
    let ok = verify_core_tx_with_tables(&store, &tx_carrying(proved_for), &table);
    assert!(ok.is_ok(), "a proof bound to the transaction carrying it must verify, got {ok:?}");

    // Planted defect: the same valid proof, in a transaction whose commitment differs.
    let err = verify_core_tx_with_tables(&store, &tx_carrying(carried), &table)
        .expect_err("a proof bound to a different transaction must be refused");
    assert!(
        matches!(
            err,
            VerifyError::BindingMismatch(ref m)
                if m.contains("does not bind to the enclosing transaction")
        ),
        "the refusal must name the binding, not the proof, got {err:?}",
    );
    assert!(
        err.to_string().starts_with("binding mismatch:"),
        "the loop's error must carry the binding reason, got {err}",
    );
}
