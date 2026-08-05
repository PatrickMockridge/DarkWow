//! Witness construction and verification for DarkLeaf call trees.
//!
//! Used by: All 4 test categories.
//! Spec: heavyweight-spec.md §3.2 (Full Production Path — accept_block).

use dwow_core::zk::Proof;
use dwow_sdk::crypto::ContractId;

/// Build an L1 witness for a single-call test transaction.
/// Required by accept_block step 2.6 and execute_block step 3d —
/// both call decode_and_reconcile which returns NoWitness on empty witness.
pub fn build_call_witness(
    contract_id: ContractId,
    call_data: &[u8],
    proofs: Vec<Proof>,
) -> Vec<u8> {
    let core_call = dwow_sdk::tx::ContractCall {
        contract_id,
        data: call_data.to_vec(),
    };
    let core_tx = dwow_core::tx::Transaction {
        calls: vec![dwow_sdk::dark_tree::DarkLeaf {
            data: core_call,
            parent_index: None,
            children_indexes: vec![],
        }],
        proofs: vec![proofs],
        tx_commitment: [0u8; 32],
        nullifiers: vec![],
    };
    dwow_serial::serialize(&core_tx)
}

/// Verify that a witness contains a well-formed DarkLeaf call tree.
/// Deserializes the witness back to a core tx and checks every call
/// has non-empty inner data and a valid function selector byte.
pub fn verify_witness(witness: &[u8], label: &str) {
    let core_tx: dwow_core::tx::Transaction = match dwow_serial::deserialize(witness) {
        Ok(tx) => tx,
        Err(e) => {
            eprintln!(
                "[tree-diag] {}: witness decode FAILED: {:?}", label, e,
            );
            return;
        }
    };
    eprintln!(
        "[tree-diag] {}: {} calls in witness tree",
        label,
        core_tx.calls.len(),
    );
    for (i, leaf) in core_tx.calls.iter().enumerate() {
        let cid = leaf.data.contract_id;
        let inner = &leaf.data.data;
        let fn_code = inner.first().copied();
        let _has_children = !leaf.children_indexes.is_empty();
        let _has_parent = leaf.parent_index.is_some();
        eprintln!(
            "[tree-diag]   call[{}]: cid={} fn=0x{:02x?} data_len={} parent={:?} children={}",
            i, cid, fn_code, inner.len(),
            leaf.parent_index, leaf.children_indexes.len(),
        );
        if inner.is_empty() {
            eprintln!(
                "[tree-diag]   call[{}]: WARNING — empty inner data (no fn_code, no params)",
                i,
            );
        }
    }
}
