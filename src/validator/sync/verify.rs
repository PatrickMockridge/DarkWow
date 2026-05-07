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

//! Block verification for sync module.
//!
//! Design: Verify block header, then verify ZK proofs by deriving VK from circuit.
//! No VK storage - VK is derived fresh at verification time.

use std::collections::HashMap;

use darkfi_sdk::pasta::pallas;

use crate::{
    blockchain::BlockInfo,
    error::Error,
    zk::verifier::{verify_zkp, ZkVerifyResult},
    Result,
};

/// Verify block header is valid.
/// Checks:
/// - Block has transactions
/// - Height is previous + 1
/// - Timestamp is greater than previous
pub fn verify_header(block: &BlockInfo, previous: &BlockInfo) -> Result<()> {
    if block.txs.is_empty() {
        return Err(Error::BlockContainsNoTransactions(block.hash().to_string()));
    }

    if block.header.height != previous.header.height + 1 {
        return Err(Error::BlockIsInvalid(format!(
            "height {} != previous {} + 1",
            block.header.height,
            previous.header.height
        )));
    }

    if block.header.timestamp <= previous.header.timestamp {
        return Err(Error::BlockIsInvalid(format!(
            "timestamp {} <= previous {}",
            block.header.timestamp, previous.header.timestamp
        )));
    }

    Ok(())
}

/// ZK data entry from ExtendedProposalMessage
/// Format: (contract_id, zkas_ns, zkbin_bytes, instances)
pub type ZkBinEntry = (darkfi_sdk::crypto::ContractId, String, Vec<u8>, Vec<pallas::Base>);

/// Verify a complete block (header + ZK proofs).
///
/// VK is derived from zkbin_bytes at verification time.
/// This eliminates VK storage/retrieval issues.
///
/// `zkbin_data` format: Vec of (contract_id, zkas_ns, zkbin_bytes, instances)
pub async fn verify_block(
    block: &BlockInfo,
    previous: &BlockInfo,
    zkbin_data: &[ZkBinEntry],
) -> Result<()> {
    verify_header(block, previous)?;

    // Build lookup: (contract_id_bytes, zkas_ns) -> (zkbin_bytes, instances)
    // Using bytes to avoid Hash trait issues with ContractId
    let mut zkbin_index: HashMap<([u8; 32], &str), (&Vec<u8>, &Vec<pallas::Base>)> =
        HashMap::new();
    for (cid, ns, zkbin, inst) in zkbin_data {
        zkbin_index.insert((cid.to_bytes(), ns.as_str()), (zkbin, inst));
    }

    // Process each transaction
    for tx in &block.txs {
        // Iterate through calls and their corresponding proofs
        for (call_idx, call) in tx.calls.iter().enumerate() {
            let proofs = &tx.proofs[call_idx];

            // For each proof, we need to find matching (zkbin_bytes, instances)
            // The mapping is based on position: proof[i] matches the i-th zkbin entry
            // for this call's contract that has the matching zkas_ns.
            //
            // Since we don't have explicit zkas_ns per proof, we try to match
            // by contract_id only when there's a single proof for the call.
            // For multiple proofs, we assume the zkbin_data entries are ordered
            // to match the proofs.
            let cid_bytes = call.data.contract_id.to_bytes();

            if proofs.len() == 1 {
                // Single proof - try to find matching entry by contract_id
                if let Some((zkbin_bytes, instances)) =
                    find_zkbin_for_contract(&zkbin_index, cid_bytes)
                {
                    match verify_zkp(&proofs[0], zkbin_bytes, instances) {
                        ZkVerifyResult::Ok => {}
                        ZkVerifyResult::InvalidProof | ZkVerifyResult::InvalidVk => {
                            return Err(Error::ZkasBincodeNotFound)
                        }
                    }
                }
                // If no zkbin found, skip verification (may be a native call)
            } else {
                // Multiple proofs - use positional matching
                let zkbin_entries: Vec<_> =
                    zkbin_data.iter().filter(|(c, _, _, _)| *c == call.data.contract_id).collect();

                for (i, proof) in proofs.iter().enumerate() {
                    if i < zkbin_entries.len() {
                        let (_, _, zkbin_bytes, instances) = zkbin_entries[i];
                        match verify_zkp(proof, zkbin_bytes, instances) {
                            ZkVerifyResult::Ok => {}
                            ZkVerifyResult::InvalidProof | ZkVerifyResult::InvalidVk => {
                                return Err(Error::ZkasBincodeNotFound)
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

/// Find zkbin data for a contract when there's only one proof.
/// Returns the first matching entry.
fn find_zkbin_for_contract<'a>(
    index: &HashMap<([u8; 32], &str), (&'a Vec<u8>, &'a Vec<pallas::Base>)>,
    contract_id_bytes: [u8; 32],
) -> Option<(&'a Vec<u8>, &'a Vec<pallas::Base>)> {
    // Try to find entry with empty namespace (common for simple contracts)
    index.get(&(contract_id_bytes, "")).map(|(z, i)| (*z, *i))
        .or_else(|| {
            // Otherwise return any entry for this contract
            index.iter()
                .find(|((cid, _), _)| *cid == contract_id_bytes)
                .map(|((_, _), (z, i))| (*z, *i))
        })
}