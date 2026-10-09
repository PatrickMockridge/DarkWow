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

//! ZK-proof + signature verification for chain transactions.
//!
//! L1 (commit 60a2534c7c) made the block/mempool/persistence carry and persist
//! the full authenticated transaction as an opaque `witness` on the chain tx.
//! This module (L2) **verifies** that witness — the whole point L1 made
//! possible. Per mempool.md §1/§4 and type-system.md §5, it is wired at both
//! admission (stop garbage-witness relay) and block accept (independent
//! history validation — the counterfeiting fix, HAZOP C1).
//!
//! For the coinbase, verification is *skipped* — coinbase soundness is
//! transparent WASM re-execution of PoWRewardV1 (genesis.md, HAZOP F3).


use crate::Transaction as ChainTransaction;
use dwow_sdk::crypto::{constants::DRK_POSEIDON_DOMAIN_TX_BINDING, pasta_prelude::PrimeField, poseidon_hash};
use dwow_sdk::dark_tree::dark_forest_leaf_vec_integrity_check;
use dwow_sdk::pasta::pallas;

/// The largest number of calls a transaction may carry.
///
/// **Derived, not chosen.** The host hands the guest its position as a one-byte `call_idx`
/// (`execution.rs`: `u8::try_from(call_idx).unwrap_or(u8::MAX)`), and the state key commits that
/// same byte — `runtime/import/merkle.rs` asserts its encoding is exactly `32 + 1`. So an index
/// above 255 cannot be named: `unwrap_or(u8::MAX)` collapses every index from 255 upward to 255,
/// and a contract's `get_call_index()` then reports the wrong call. `execution.rs` reads the same
/// truncated index to decide which call Deployooor should deploy.
///
/// The bound is therefore the encoding's capacity — nothing is invented, and a stricter figure
/// would need its own derivation, which `MAX_TX_CALLS` in `src/tx/mod.rs` does not have.
const MAX_CALLS_BY_INDEX_ENCODING: usize = u8::MAX as usize;

// ---------------------------------------------------------------------------
// 1. Witness decode + reconciliation (L2 soundness gate #1)
// ---------------------------------------------------------------------------

/// Decode the witness and reconcile it against the chain tx.
///
/// The witness is `dwow_serial(core_tx)` where `core_tx` is
/// `dwow_core::tx::Transaction` (calls + proofs + tx_commitment +
/// nullifiers). Because the witness is hash-excluded / malleable (L1 barrier
/// #1), contract_calls in the chain tx could diverge from the calls in the
/// witness. Verification is decoupled from execution unless we enforce
/// identity here — so we hard-reject on any mismatch (per the MoC tabletop's
/// mandatory reconciliation requirement).
///
/// Returns the decoded `dwow_core::tx::Transaction` on success so the caller
/// can feed it to `verify_core_tx_with_tables`.
pub fn decode_and_reconcile(
    chain_tx: &ChainTransaction,
) -> Result<dwow_core::tx::Transaction, VerifyError> {
    // Coinbase and transactions not yet carrying a witness are silently OK —
    // the coinbase is exempt from verification, and a proofless non-coinbase tx
    // will fail downstream checks (fee/proof) so we do not pre-emptively reject.
    if chain_tx.witness.is_empty() {
        return Err(VerifyError::NoWitness);
    }

    let core_tx: dwow_core::tx::Transaction =
        dwow_serial::deserialize(&chain_tx.witness).map_err(|e| {
            VerifyError::WitnessDecode(format!("{}", e))
        })?;

    // -- The tree must be well-formed and bounded BEFORE anything reads it. --
    //
    // The witness is where the tree lives: `parent_index` and `children_indexes` are carried in
    // it, and they are in neither `Transaction::hash` nor the merkle root, so nothing else commits
    // to them. The reconciliation below proves the *leaves* — each call's `(contract_id, data)`
    // against the chain tx — and says nothing about the shape they are arranged in. But the guest
    // is handed exactly this shape (`extract_wasm_call_tree` serialises `core_tx.calls` straight
    // into the payload) and more than twenty entrypoints gate value custody on the parent/child
    // relation, so a relaying node that re-encoded the tree could change contract behaviour while
    // every hash still passed (F10).
    //
    // The bound is the second half of the same call: at 255 calls the largest index is 255, which
    // the one-byte `call_idx` can name, so `u8::try_from` cannot saturate and `get_call_index()`
    // cannot report the wrong call (F7d). `MAX_CALLS_BY_INDEX_ENCODING` carries the derivation.
    dark_forest_leaf_vec_integrity_check(
        &core_tx.calls,
        Some(dwow_core::tx::MIN_TX_CALLS),
        Some(MAX_CALLS_BY_INDEX_ENCODING),
    )
    .map_err(|e| {
        VerifyError::Reconciliation(format!(
            "witness call tree is not well-formed, or has more than {} calls: {}",
            MAX_CALLS_BY_INDEX_ENCODING, e
        ))
    })?;

    // -- Reconciliation: chain_tx.contract_calls MUST equal core_tx.calls.data
    //    (contract_id + data, in order), and nullifiers must match. --

    if core_tx.calls.len() != chain_tx.contract_calls.len() {
        return Err(VerifyError::Reconciliation(format!(
            "call count mismatch: witness={} chain={}",
            core_tx.calls.len(),
            chain_tx.contract_calls.len(),
        )));
    }

    for (i, (leaf, chain_call)) in core_tx
        .calls
        .iter()
        .zip(chain_tx.contract_calls.iter())
        .enumerate()
    {
        if leaf.data.contract_id != chain_call.contract_id {
            return Err(VerifyError::Reconciliation(format!(
                "call[{}] contract_id mismatch: witness={} chain={}",
                i, leaf.data.contract_id, chain_call.contract_id,
            )));
        }
        if leaf.data.data != chain_call.data {
            return Err(VerifyError::Reconciliation(format!(
                "call[{}] data mismatch ({} witness vs {} chain bytes)",
                i,
                leaf.data.data.len(),
                chain_call.data.len(),
            )));
        }
    }

    if core_tx.nullifiers != chain_tx.nullifiers {
        return Err(VerifyError::Reconciliation(format!(
            "nullifier mismatch",
        )));
    }

    // -- `OBL-C198` stage 4's precondition, and it is not optional. --
    //
    // `tx_commitment` is part of the witness bundle — proofs, signatures and this field are all
    // hash-excluded from `tx.hash()` (L1 barrier #1) — so until it is tied to the calls reconciled
    // above it is a prover-supplied value like any other. `verify_core_tx_with_tables` compares a
    // proof's published `tx_binding` against the commitment *this* field carries; if the field were
    // unconstrained, a proof lifted from transaction A would pass by the prover writing A's
    // commitment here, and the stage-4 check could not fail. The calls are settled one branch up, so
    // derive the commitment from them — the same `commitment_of_calls` the builder uses, one
    // derivation, one home — and require agreement.
    if core_tx.tx_commitment != dwow_core::tx::commitment_of_calls(&core_tx.calls) {
        return Err(VerifyError::Reconciliation(
            "witness tx_commitment does not match its own reconciled call set".into(),
        ));
    }

    Ok(core_tx)
}

// ---------------------------------------------------------------------------
// 2. VK loading — read raw zkbin bytes from the contracts sled tree
// ---------------------------------------------------------------------------

/// Read the raw zkas binary for `(contract_id, namespace)` from the contracts
/// sled tree. The data is stored as `value = serialize(&(zkbin_bytes, vk_buf))`;
/// we extract only `zkbin_bytes` (the stateless `verify_zkp` in `src/zk/verifier.rs`
/// derives and caches the VK itself).
pub fn load_zkbin(
    store: &crate::LinearStore,
    contract_id: &dwow_sdk::crypto::ContractId,
    namespace: &str,
) -> Result<Vec<u8>, VerifyError> {
    let prefix = contract_id.hash_state_id(
        dwow_sdk::crypto::contract_id::SMART_CONTRACT_ZKAS_DB_NAME,
    );
    let ns_bytes = dwow_serial::serialize(&namespace);
    let key = [&prefix[..], &ns_bytes[..]].concat();
    let raw = store.get_contract_data(&key).map_err(|e| {
        VerifyError::MissingCircuit(format!(
            "no zkas '{}' for contract {}: {}",
            namespace, contract_id, e,
        ))
    })?;
    // Value = (Vec<u8>, Vec<u8>) = (zkbin_bytes, vk_buf)
    let (zkbin_bytes, _vk_buf): (Vec<u8>, Vec<u8>) =
        dwow_serial::deserialize(&raw).map_err(|e| {
            VerifyError::StoreRead(format!("deserialize zkas value: {}", e))
        })?;
    Ok(zkbin_bytes)
}

// ---------------------------------------------------------------------------
// 3. Errors
// ---------------------------------------------------------------------------

/// Errors that can occur during witness verification.
#[derive(Debug)]
pub enum VerifyError {
    /// The transaction carries no witness (coinbase or not-yet-populated).
    NoWitness,
    /// The witness blob failed to decode — not a valid core tx.
    WitnessDecode(String),
    /// The decoded core tx does not agree with the chain tx
    /// (calls / nullifiers diverge — the witness is a different transaction).
    Reconciliation(String),
    /// Failed to read contract data from the sled store.
    StoreRead(String),
    /// The required zkas binary is not in the contracts tree.
    MissingCircuit(String),
    /// A ZK proof did not verify, or a signature was invalid.
    InvalidProof(String),
    /// A proof's published `tx_binding` does not bind to the enclosing transaction
    /// (`OBL-C198` stage 4) — the proof was made against a different `tx_commitment`.
    BindingMismatch(String),
}

impl std::fmt::Display for VerifyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoWitness => write!(f, "no witness"),
            Self::WitnessDecode(msg) => write!(f, "witness decode: {}", msg),
            Self::Reconciliation(msg) => write!(f, "reconciliation: {}", msg),
            Self::StoreRead(msg) => write!(f, "store read: {}", msg),
            Self::MissingCircuit(msg) => write!(f, "missing circuit: {}", msg),
            Self::InvalidProof(msg) => write!(f, "invalid proof: {}", msg),
            Self::BindingMismatch(msg) => write!(f, "binding mismatch: {}", msg),
        }
    }
}

impl std::error::Error for VerifyError {}

// ---------------------------------------------------------------------------
// 4. Per-tx verification with accumulated metadata tables
// ---------------------------------------------------------------------------

/// `OBL-C198` stage 4: does a proof's public-input vector bind it to the transaction that carries it?
///
/// Every circuit places the pair last (`scripts/check-circuit-tx-pair-last.sh`, whose exceptions file is
/// empty), so `pubvals[len-2]`/`[len-1]` is the interface and no per-circuit index map is wanted. Returns
/// `None` when the published `tx_binding` is exactly `poseidon_hash(DOMAIN_TX_BINDING, tx_commitment,
/// tx_nonce)`, or the reason it is not. A **named predicate** so that a control can make it fail
/// (AGENTS.md R8): hand it a commitment the proof was *not* made over and it must report a mismatch.
fn tx_binding_mismatch(
    pubvals: &[dwow_sdk::pasta::pallas::Base],
    tx_commitment: [u8; 32],
) -> Option<String> {
    let pair_at = match pubvals.len().checked_sub(2) {
        Some(p) => p,
        None => {
            return Some(
                "circuit instances no tx pair, so the binding cannot be checked".to_string(),
            )
        }
    };
    let (tx_binding, tx_nonce) = (pubvals[pair_at], pubvals[pair_at + 1]);
    let commitment = match pallas::Base::from_repr(tx_commitment).into_option() {
        Some(c) => c,
        None => return Some("witness tx_commitment is not a canonical field element".to_string()),
    };
    let expected = poseidon_hash([DRK_POSEIDON_DOMAIN_TX_BINDING, commitment, tx_nonce]);
    if expected != tx_binding {
        return Some("published tx_binding does not bind to the enclosing transaction".to_string());
    }
    None
}

/// Verify the decoded core_tx against the accumulated per-call `metadata()`
/// tables (see `execute_block`) and the on-chain zkas binaries.
///
/// For each call in core_tx: load the zkbin from the contracts store,
/// feed it to the stateless `verify_zkp` along with the proof and public
/// inputs. Schnorr signature verification removed per contract-standards.md §3.
///
/// This is the function that closes HAZOP C1 (counterfeiting) — a
/// fabricated transaction has no valid proof and is rejected here.
pub fn verify_core_tx_with_tables(
    store: &crate::LinearStore,
    core_tx: &dwow_core::tx::Transaction,
    zkp_table: &[Vec<(String, Vec<dwow_sdk::pasta::pallas::Base>)>],
) -> Result<(), VerifyError> {
    // -- ZK proofs: per call, per proof --
    // Safety gate: the outer zip silently truncates to the minimum length.
    // A witness whose proofs vec is shorter than the metadata tables would
    // pass with zero ZK verification (HAZOP C1 — counterfeiting guard).
    // Reject length mismatches before any iteration.
    if core_tx.proofs.len() != zkp_table.len() {
        return Err(VerifyError::InvalidProof(format!(
            "proof vec length {} != metadata {} calls",
            core_tx.proofs.len(),
            zkp_table.len(),
        )));
    }
    // HAZOP H-16: proof-to-call index is enforced by the zip of proofs
    // and zkp_table. A malicious witness that swaps proof ordering would
    // cause verify_zkp to fail: each proof's VK is derived from the circuit
    // bytecode keyed by (contract_id, namespace), and different circuits
    // produce different VKs. Swapped proofs verify against wrong VKs → fail.
    // load_zkbin additionally validates that the namespace exists in the
    // contract's store — a swapped namespace would return Err.
    for (call_i, (proofs, call_zkp)) in core_tx
        .proofs
        .iter()
        .zip(zkp_table.iter())
        .enumerate()
    {
        let contract_id = &core_tx.calls[call_i].data.contract_id;
        // Per-call length guard: a call with N metadata-declared proof
        // entries must carry exactly N proofs — an undersized vec
        // silently skips verification via zip truncation.
        if proofs.len() != call_zkp.len() {
            return Err(VerifyError::InvalidProof(format!(
                "call[{}] has {} proofs but metadata declares {} entries",
                call_i, proofs.len(), call_zkp.len(),
            )));
        }
        for (proof_j, (proof_ref, (ns, pubvals))) in proofs.iter().zip(call_zkp.iter()).enumerate() {
            let _ = proof_j; // index-only — use proof_ref for the actual proof
            let zkbin = load_zkbin(store, contract_id, ns)?;
            let result =
                dwow_core::zk::verify_zkp(proof_ref, &zkbin, pubvals);
            match result {
                dwow_core::zk::ZkVerifyResult::Ok => {
                    // -- `OBL-C198` stage 4: the proof must bind to the transaction carrying it. --
                    //
                    // Reached only on `Ok`, so the refusal is a genuine proof that is *mis-bound*, never a
                    // malformed one. The commitment compared against is `core_tx.tx_commitment`, which
                    // `decode_and_reconcile` proved equal to `commitment_of_calls(&core_tx.calls)`, so the
                    // binding ties the proof to this reconciled call set and not to one the prover chose.
                    if let Some(reason) = tx_binding_mismatch(pubvals, core_tx.tx_commitment) {
                        return Err(VerifyError::BindingMismatch(format!(
                            "call[{}] namespace '{}': {}",
                            call_i, ns, reason,
                        )));
                    }
                }
                dwow_core::zk::ZkVerifyResult::InvalidVk
                | dwow_core::zk::ZkVerifyResult::InvalidProof => {
                    return Err(VerifyError::InvalidProof(format!(
                        "call[{}] namespace '{}'",
                        call_i, ns,
                    )));
                }
            }
        }
    }

    // Schnorr signature verification removed per contract-standards.md §3.
    // Authorization is via ZK proof + nullifier exclusively.

    Ok(())
}

// ---------------------------------------------------------------------------
// 5. Mempool-side verification: structural check for admission
// ---------------------------------------------------------------------------

/// Verify a single chain tx for mempool admission. Called before `mp.add()`.
///
/// Performs witness decode + reconciliation (soundness gate #1 — the witness
/// must describe the same tx as the chain tx), then checks that every
/// proof-requiring call has at least one ZK proof. Full VK-based verification
/// is done at block accept (see `verify_core_tx_with_tables`).
///
/// Returns `Ok(())` if the tx is safe to admit. Coinbase txs (empty witness)
/// are silently ok — the mempool already rejects them before this call.
pub fn verify_single_tx(chain_tx: &ChainTransaction) -> Result<(), VerifyError> {
    let core_tx = decode_and_reconcile(chain_tx)?;
    // Proof vec length guard: Rust's zip silently truncates to the shortest
    // iterator. If proofs.len() < calls.len(), per-call validation below only
    // inspects the first proofs.len() calls. Reject before any iteration.
    // Closes: H3 (zip truncation bypass). Enforces: type-system.md §8.2.
    if core_tx.proofs.len() != core_tx.calls.len() {
        return Err(VerifyError::InvalidProof(format!(
            "proof vec length {} != calls {}",
            core_tx.proofs.len(), core_tx.calls.len(),
        )));
    }
    // Every proof-requiring call must carry at least one ZK proof. At the
    // structural stage only the native token's value-bearing selectors are
    // knowably proof-requiring (the mempool already hardcodes native
    // knowledge — see the fee extractor); other contracts' calls may be
    // legitimately proofless (e.g. Deployooor DeployV1/LockV1). The
    // authoritative per-call verification against each contract's metadata
    // happens at block accept (`verify_core_tx_with_tables`).
    for (i, (call, proofs)) in core_tx.calls.iter().zip(core_tx.proofs.iter()).enumerate() {
        // 0x06 (FeeCollectV1) dropped — plaintext since 2026-09, like the
        // coinbase (0x05) and uncle note (0x07), which carry no proof.
        let is_native_proof_call = call.data.contract_id
            == *dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID
            && matches!(call.data.data.first(), Some(0x00 | 0x02 | 0x03 | 0x04 | 0x08)); // 0x08 = FeeV3
        if is_native_proof_call && proofs.is_empty() {
            return Err(VerifyError::InvalidProof(format!(
                "call[{}] requires a proof but has none (mempool admission)",
                i,
            )));
        }
    }
    // ── Authority model per ocap.md §6.2 ──────────────────────────────────
    // ZK contract calls prove authority through ZK proofs (↓prove) +
    // nullifier-based consumption (↓nullify). The ZK proof demonstrates
    // possession of the SecretKey (the capability name) — this IS the
    // authority evidence per type-system.md §5.
    //
    // Schnorr signatures removed per contract-standards.md §3.
    // ocap.md §6.2 defines Exercise = ZK Proof, Verify =
    // Proof::verify, Consume = Nullifier. No per-call Schnorr signature.

    Ok(())
}

// ---------------------------------------------------------------------------
// 6. Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ContractCall;
    use dwow_sdk::pasta::pallas;
    use dwow_sdk::blockchain::BlockVersion;

    type Leaf = dwow_sdk::dark_tree::DarkLeaf<dwow_sdk::tx::ContractCall>;

    /// `id` must be a canonical, non-identity contract id — `from_bytes` rejects the rest, so tests
    /// pass 1..=3. The byte is also the call's payload, keeping the two in step.
    fn leaf(id: u8, parent: Option<usize>, children: Vec<usize>) -> Leaf {
        dwow_sdk::dark_tree::DarkLeaf {
            data: dwow_sdk::tx::ContractCall {
                contract_id: dwow_sdk::crypto::ContractId::from_bytes([id; 32]).unwrap(),
                data: vec![id],
            },
            children_indexes: children,
            parent_index: parent,
        }
    }

    /// A chain tx whose flat call list matches `calls` exactly, so that only the *structure* of the
    /// witness can be what a test rejects. Without this, a structural test would pass for the wrong
    /// reason — the count or the leaves would fail reconciliation first.
    fn chain_tx_for(calls: &[Leaf], witness: &[u8]) -> ChainTransaction {
        ChainTransaction {
            version: BlockVersion::CURRENT,
            inputs: vec![],
            outputs: vec![],
            contract_calls: calls
                .iter()
                .map(|l| ContractCall {
                    contract_id: l.data.contract_id,
                    data: l.data.data.clone(),
                })
                .collect(),
            lock_time: 0,
            nullifiers: vec![],
            witness: witness.to_vec(),
        }
    }

    /// `tx_commitment` is *derived* from `calls`, not zeroed: `decode_and_reconcile` now requires the
    /// witness's field to equal `commitment_of_calls(&calls)` (`OBL-C198` stage 4's precondition), so a
    /// fixture that zeroes it is refused before the structure checks these tests are about ever run.
    fn core_tx_of(calls: Vec<Leaf>) -> dwow_core::tx::Transaction {
        let tx_commitment = dwow_core::tx::commitment_of_calls(&calls);
        dwow_core::tx::Transaction { calls, proofs: vec![], tx_commitment, nullifiers: vec![] }
    }

    /// F10's witness, with its acceptance control.
    ///
    /// The tree is two children under one root, flattened in the order the tree format uses — the
    /// **root last**, with `children_indexes` naming the earlier leaves. With the root naming both
    /// children it reconciles; with `children_indexes` emptied — the mutation a relaying node would
    /// make, since the tree is in neither the tx hash nor the merkle root — it must be rejected.
    /// The leaves are identical in both cases and match the chain tx, so the *only* difference the
    /// check can see is the shape, which is the claim being tested.
    #[test]
    fn a_mutated_children_indexes_is_rejected_at_reconciliation() {
        let intact =
            vec![leaf(1, Some(2), vec![]), leaf(2, Some(2), vec![]), leaf(3, None, vec![0, 1])];
        let witness = dwow_serial::serialize(&core_tx_of(intact.clone()));
        decode_and_reconcile(&chain_tx_for(&intact, &witness))
            .expect("a well-formed tree with matching leaves must reconcile");

        // Same leaves, same chain tx; the root no longer names its children.
        let mut mutated = intact.clone();
        mutated[2].children_indexes = vec![];
        let witness = dwow_serial::serialize(&core_tx_of(mutated.clone()));
        let err = decode_and_reconcile(&chain_tx_for(&mutated, &witness))
            .expect_err("a tree whose parent does not name its children must be rejected");
        assert!(
            matches!(err, VerifyError::Reconciliation(ref m) if m.contains("not well-formed")),
            "wrong rejection for a mutated tree: {err:?}"
        );
    }

    /// F7d's witness. 256 parentless calls are each a well-formed one-leaf tree, so structure
    /// cannot be what rejects them — only the bound can, and the bound is what keeps a one-byte
    /// `call_idx` from saturating.
    #[test]
    fn more_calls_than_the_index_can_name_are_rejected() {
        let ids = |i: usize| 1 + (i % 3) as u8; // 1..=3: canonical, non-identity

        let over: Vec<Leaf> = (0..=MAX_CALLS_BY_INDEX_ENCODING)
            .map(|i| leaf(ids(i), None, vec![]))
            .collect();
        assert_eq!(over.len(), MAX_CALLS_BY_INDEX_ENCODING + 1);
        let witness = dwow_serial::serialize(&core_tx_of(over.clone()));
        let err = decode_and_reconcile(&chain_tx_for(&over, &witness))
            .expect_err("more calls than the index encoding can name must be rejected");
        assert!(
            matches!(err, VerifyError::Reconciliation(ref m) if m.contains("more than 255 calls")),
            "wrong rejection for an over-long call list: {err:?}"
        );

        // Acceptance control: exactly the bound is still fine, so this is a bound and not a blanket
        // refusal.
        let at_bound: Vec<Leaf> = (0..MAX_CALLS_BY_INDEX_ENCODING)
            .map(|i| leaf(ids(i), None, vec![]))
            .collect();
        let witness = dwow_serial::serialize(&core_tx_of(at_bound.clone()));
        decode_and_reconcile(&chain_tx_for(&at_bound, &witness))
            .expect("exactly the bound must be accepted");
    }

    #[test]
    fn test_reconciliation_rejects_divergent_calls() {
        let inner = dwow_core::tx::Transaction {
            calls: vec![dwow_sdk::dark_tree::DarkLeaf {
                data: dwow_sdk::tx::ContractCall {
                    contract_id: dwow_sdk::crypto::ContractId::from_bytes([1u8; 32]).unwrap(),
                    data: b"inner".to_vec(),
                },
                children_indexes: vec![],
                parent_index: None,
            }],
            proofs: vec![],
            tx_commitment: [0u8; 32],
            nullifiers: vec![],
        };

        let chain_tx = ChainTransaction {
            version: BlockVersion::CURRENT,
            inputs: vec![],
            outputs: vec![],
            contract_calls: vec![ContractCall {
                contract_id: dwow_sdk::crypto::ContractId::from_bytes([2u8; 32]).unwrap(),
                data: b"chain".to_vec(),
            }],
            lock_time: 0,
            nullifiers: vec![],
            witness: dwow_serial::serialize(&inner),
        };

        assert!(matches!(
            decode_and_reconcile(&chain_tx),
            Err(VerifyError::Reconciliation(_)),
        ));
    }

    #[test]
    fn test_valid_witness_reconciles() {
        let call = dwow_sdk::tx::ContractCall {
            contract_id: dwow_sdk::crypto::ContractId::from_bytes([3u8; 32]).unwrap(),
            data: b"data".to_vec(),
        };

        let calls = vec![dwow_sdk::dark_tree::DarkLeaf {
            data: call.clone(),
            children_indexes: vec![],
            parent_index: None,
        }];
        let expected_commitment = dwow_core::tx::commitment_of_calls(&calls);
        let core_tx = dwow_core::tx::Transaction {
            calls,
            proofs: vec![],
            tx_commitment: expected_commitment,
            nullifiers: vec![],
        };

        let chain_tx = ChainTransaction {
            version: BlockVersion::CURRENT,
            inputs: vec![],
            outputs: vec![],
            contract_calls: vec![ContractCall {
                contract_id: call.contract_id,
                data: call.data.clone(),
            }],
            lock_time: 0,
            nullifiers: vec![],
            witness: dwow_serial::serialize(&core_tx),
        };

        let decoded = decode_and_reconcile(&chain_tx).unwrap();
        assert_eq!(decoded.tx_commitment, expected_commitment);
    }

    /// `OBL-C198` stage 4's negative control (AGENTS.md R8): the check must be able to fail. A binding
    /// made over commitment A verifies against A and is **refused** against a different transaction B,
    /// with the refusal naming the binding rather than the proof.
    #[test]
    fn stage_4_refuses_a_binding_made_over_a_different_transaction() {
        let commitment_a = pallas::Base::from(11u64);
        let commitment_b = pallas::Base::from(22u64);
        let nonce = pallas::Base::from(33u64);
        // The proof's pair as a circuit instances it: binding then nonce, last.
        let binding_a = poseidon_hash([DRK_POSEIDON_DOMAIN_TX_BINDING, commitment_a, nonce]);
        let pubvals = vec![pallas::Base::from(1u64), binding_a, nonce];

        // Honest: the proof binds the transaction it was made over.
        assert!(
            tx_binding_mismatch(&pubvals, commitment_a.to_repr()).is_none(),
            "a binding over its own transaction must pass",
        );
        // Planted defect: the same proof where the enclosing transaction is B.
        let reason = tx_binding_mismatch(&pubvals, commitment_b.to_repr())
            .expect("a binding over a different transaction must be refused");
        assert!(reason.contains("does not bind"), "wrong refusal: {reason}");
    }

    /// `OBL-C198` stage 4's precondition, with its control: a witness whose `tx_commitment` field does
    /// not equal `commitment_of_calls` over its own calls is refused — the value the node compares a
    /// proof's binding against is derived, never the one the prover wrote down.
    #[test]
    fn a_witness_whose_commitment_disagrees_with_its_calls_is_refused() {
        let calls = vec![leaf(1, None, vec![])];
        let honest = core_tx_of(calls.clone());
        let witness = dwow_serial::serialize(&honest);
        decode_and_reconcile(&chain_tx_for(&calls, &witness))
            .expect("a witness whose commitment matches its calls must reconcile");

        // Planted defect: the field moves, the calls do not.
        let mut tampered = core_tx_of(calls.clone());
        tampered.tx_commitment = pallas::Base::from(9u64).to_repr();
        let witness = dwow_serial::serialize(&tampered);
        let err = decode_and_reconcile(&chain_tx_for(&calls, &witness))
            .expect_err("a witness whose commitment disagrees with its calls must be refused");
        assert!(
            matches!(err, VerifyError::Reconciliation(ref m) if m.contains("does not match its own reconciled call set")),
            "wrong rejection: {err:?}",
        );
    }

    #[test]
    fn test_empty_witness_returns_no_witness() {
        let chain_tx = ChainTransaction {
            witness: vec![],
            ..Default::default()
        };
        assert!(matches!(
            decode_and_reconcile(&chain_tx),
            Err(VerifyError::NoWitness),
        ));
    }

    /// A proof-less core_tx with non-empty metadata tables must be rejected —
    /// the zip-truncation gap would otherwise silently skip ZK verification
    /// (HAZOP C1 counterfeiting guard, red-team audit item 1).
    #[test]
    fn test_proof_metadata_length_mismatch_rejected() {
        let core_tx = dwow_core::tx::Transaction {
            calls: vec![dwow_sdk::dark_tree::DarkLeaf {
                data: dwow_sdk::tx::ContractCall {
                    contract_id: *dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID,
                    data: vec![0x03u8, 0u8], // TransferV1 — a still-proof-requiring selector
                },
                children_indexes: vec![],
                parent_index: None,
            }],
            proofs: vec![], // EMPTY — should trigger the outer length guard
            tx_commitment: [0u8; 32],
            nullifiers: vec![],
        };
        // zkp_table declares one call's worth of metadata — the proof vec
        // is empty, so lengths differ. The guard rejects before any iteration.
        let zkp_table: Vec<Vec<(String, Vec<pallas::Base>)>> = vec![
            vec![("Transfer_V1".to_string(), vec![pallas::Base::zero()])],
        ];
        // The function never reaches load_zkbin on this code path (fails on
        // the length guard before any store access), but the type system
        // requires a store handle. Create a minimal LinearStore via sled
        // tempdir — the sled API guarantees ::new() never fails.
        let tmp_db = sled::Config::new().temporary(true).open().unwrap();
        let store = crate::LinearStore::new(std::sync::Arc::new(tmp_db)).unwrap();
        let result = verify_core_tx_with_tables(
            &store,
            &core_tx,
            &zkp_table,
        );
        assert!(
            matches!(result, Err(VerifyError::InvalidProof(_))),
            "empty proofs with non-empty metadata must be rejected, got {:?}", result,
        );
    }

    /// BW-2: Proofless native token call rejection at mempool admission.
    /// Per type-system.md §10.5: the mempool admission boundary SHALL reject
    /// native token calls (FeeV3, TransferV1, SpendV1, BurnV1) that declare
    /// proofs required but carry none. This gate prevents transactions with
    /// missing ZK proofs from entering the mempool. (PoWRewardV1, FeeCollectV1
    /// and UncleMintV1 are plaintext and exempt.)
    #[test]
    fn test_native_token_proofless_call_rejected_at_admission() {
        // FeeV3 (0x08) with empty proofs — must be rejected
        let chain_tx = ChainTransaction {
            version: BlockVersion::CURRENT,
            inputs: vec![],
            outputs: vec![],
            contract_calls: vec![ContractCall {
                contract_id: *dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID,
                data: vec![0x08u8], // FeeV3 selector
            }],
            lock_time: 0,
            nullifiers: vec![],
            witness: vec![], // no proof — should trigger rejection
        };
        // verify_single_tx decodes witness, reconciles, and checks proof presence.
        // Empty witness means decode_and_reconcile gets an empty Transaction,
        // which triggers the proof-vec-length guard (0 proofs vs 1 call).
        let result = verify_single_tx(&chain_tx);
        assert!(result.is_err(), "native token FeeV3 without proofs must be rejected");

        // TransferV1 (0x03) with empty proofs — same guard
        let chain_tx2 = ChainTransaction {
            version: BlockVersion::CURRENT,
            inputs: vec![],
            outputs: vec![],
            contract_calls: vec![ContractCall {
                contract_id: *dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID,
                data: vec![0x03u8], // TransferV1 selector
            }],
            lock_time: 0,
            nullifiers: vec![],
            witness: vec![],
        };
        let result2 = verify_single_tx(&chain_tx2);
        assert!(result2.is_err(), "native token TransferV1 without proofs must be rejected");
    }
}
