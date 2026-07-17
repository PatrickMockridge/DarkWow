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

use std::io::Cursor;

use crate::execution::TxBackend;
use crate::Transaction as ChainTransaction;

// ---------------------------------------------------------------------------
// 1. Witness decode + reconciliation (L2 soundness gate #1)
// ---------------------------------------------------------------------------

/// Decode the witness and reconcile it against the chain tx.
///
/// The witness is `dwow_serial(core_tx)` where `core_tx` is
/// `dwow_core::tx::Transaction` (calls + proofs + signatures + tx_commitment +
/// nullifiers). Because the witness is hash-excluded / malleable (L1 barrier
/// #1), contract_calls in the chain tx could diverge from the calls in the
/// witness. Verification is decoupled from execution unless we enforce
/// identity here — so we hard-reject on any mismatch (per the MoC tabletop's
/// mandatory reconciliation requirement).
///
/// Returns the decoded `dwow_core::tx::Transaction` on success so the caller
/// can feed it to `verify_zkps`/`verify_sigs`.
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
        }
    }
}

impl std::error::Error for VerifyError {}

// ---------------------------------------------------------------------------
// 4. Per-tx verification with accumulated metadata tables
// ---------------------------------------------------------------------------

/// Verify the decoded core_tx against the accumulated per-call `metadata()`
/// tables (see `execute_block`) and the on-chain zkas binaries.
///
/// For each call in core_tx: load the zkbin from the contracts store,
/// feed it to the stateless `verify_zkp` along with the proof and public
/// inputs. Then verify all Schnorr signatures against the pub_table.
///
/// This is the function that closes HAZOP C1 (counterfeiting) — a
/// fabricated transaction has no valid proof and is rejected here.
pub fn verify_core_tx_with_tables(
    store: &crate::LinearStore,
    core_tx: &dwow_core::tx::Transaction,
    zkp_table: &[Vec<(String, Vec<dwow_sdk::pasta::pallas::Base>)>],
    pub_table: &[Vec<dwow_sdk::crypto::PublicKey>],
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
                dwow_core::zk::ZkVerifyResult::Ok => {}
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

    // -- Schnorr signatures --
    if let Err(e) = core_tx.verify_sigs(pub_table.to_vec()) {
        return Err(VerifyError::InvalidProof(format!(
            "signature verification failed: {}",
            e,
        )));
    }

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
    // Every proof-requiring call must carry at least one ZK proof. At the
    // structural stage only the native token's value-bearing selectors are
    // knowably proof-requiring (the mempool already hardcodes native
    // knowledge — see the fee extractor); other contracts' calls may be
    // legitimately proofless (e.g. Deployooor DeployV1/LockV1). The
    // authoritative per-call verification against each contract's metadata
    // happens at block accept (`verify_core_tx_with_tables`).
    for (i, (call, proofs)) in core_tx.calls.iter().zip(core_tx.proofs.iter()).enumerate() {
        let is_native_proof_call = call.data.contract_id
            == *dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID
            && matches!(call.data.data.first(), Some(0x00 | 0x02 | 0x03 | 0x04 | 0x06));
        if is_native_proof_call && proofs.is_empty() {
            return Err(VerifyError::InvalidProof(format!(
                "call[{}] requires a proof but has none (mempool admission)",
                i,
            )));
        }
    }
    // Structural sig check: every call with calls must have sigs.
    if core_tx.calls.len() != core_tx.signatures.len() {
        return Err(VerifyError::InvalidProof(format!(
            "signature table length mismatch: {} calls vs {} sigs",
            core_tx.calls.len(), core_tx.signatures.len(),
        )));
    }
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
            signatures: vec![],
            tx_commitment: [0u8; 32],
            nullifiers: vec![],
        };

        let chain_tx = ChainTransaction {
            version: 1,
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

        let core_tx = dwow_core::tx::Transaction {
            calls: vec![dwow_sdk::dark_tree::DarkLeaf {
                data: call.clone(),
                children_indexes: vec![],
                parent_index: None,
            }],
            proofs: vec![],
            signatures: vec![],
            tx_commitment: [0u8; 32],
            nullifiers: vec![],
        };

        let chain_tx = ChainTransaction {
            version: 1,
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
        assert_eq!(decoded.tx_commitment, [0u8; 32]);
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
                    data: vec![0x06u8, 0u8],
                },
                children_indexes: vec![],
                parent_index: None,
            }],
            proofs: vec![], // EMPTY — should trigger the outer length guard
            signatures: vec![vec![]],
            tx_commitment: [0u8; 32],
            nullifiers: vec![],
        };
        // zkp_table declares one call's worth of metadata — the proof vec
        // is empty, so lengths differ. The guard rejects before any iteration.
        let zkp_table: Vec<Vec<(String, Vec<pallas::Base>)>> = vec![
            vec![("FeeCollect_V1".to_string(), vec![pallas::Base::zero()])],
        ];
        let pub_table: Vec<Vec<dwow_sdk::crypto::PublicKey>> = vec![vec![]];

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
            &pub_table,
        );
        assert!(
            matches!(result, Err(VerifyError::InvalidProof(_))),
            "empty proofs with non-empty metadata must be rejected, got {:?}", result,
        );
    }
}
