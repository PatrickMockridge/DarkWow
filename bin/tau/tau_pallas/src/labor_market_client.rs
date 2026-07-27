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

//! Labor Market contract client for tau_pallas
//!
//! This module provides utilities for building Labor Market contract calls,
//! enabling tau_pallas to submit deliverables on-chain.

use dwow_core::tx::Transaction;
use dwow_sdk::crypto::pasta_prelude::PrimeField;
use dwow_sdk::dark_tree::{DarkForest, DarkLeaf, DarkTree};
use dwow_sdk::pasta::pallas;
use dwow_sdk::tx::ContractCall;
use dwow_serial::Encodable;

use crate::error::{TauPallasError, TauPallasResult};

/// Function codes for Labor Market contract
pub const LABOR_MARKET_FUNCTION_SUBMIT_DELIVERABLE_V1: u8 = 0x02;
pub const LABOR_MARKET_FUNCTION_SUBMIT_GIT_DELIVERABLE_V1: u8 = 0x03;

/// Function code for Attestation contract VerifyClaimV1
pub const ATTESTATION_FUNCTION_VERIFY_CLAIM_V1: u8 = 0x04;

/// Build call data for Labor Market's SubmitDeliverableV1
///
/// Serializes SubmitDeliverableParamsV1 for the labor market contract.
/// Function code: 0x02
pub fn build_submit_deliverable_calldata(
    proof: &[u8],
    job_id: pallas::Base,
    claim_id: pallas::Base,
    worker_pub_x: pallas::Base,
    worker_pub_y: pallas::Base,
    spent_nullifier: pallas::Base,
) -> TauPallasResult<Vec<u8>> {
    let mut call_data = Vec::new();
    call_data.push(LABOR_MARKET_FUNCTION_SUBMIT_DELIVERABLE_V1);

    // proof: Vec<u8> — length-prefixed
    call_data.extend_from_slice(&(proof.len() as u32).to_le_bytes());
    call_data.extend_from_slice(proof);

    // job_id, claim_id, worker_pub_x, worker_pub_y, spent_nullifier — all pallas::Base (32 bytes)
    call_data.extend_from_slice(&job_id.to_repr());
    call_data.extend_from_slice(&claim_id.to_repr());
    call_data.extend_from_slice(&worker_pub_x.to_repr());
    call_data.extend_from_slice(&worker_pub_y.to_repr());
    call_data.extend_from_slice(&spent_nullifier.to_repr());

    Ok(call_data)
}

/// Build call data for Attestation contract's VerifyClaimV1
///
/// Function code: 0x04
pub fn build_verify_claim_calldata(
    claim_id: pallas::Base,
    attestation_id: pallas::Base,
    evidence_commitment: pallas::Base,
    revealed_result: pallas::Base,
    revocation_root: pallas::Base,
    attestation_data: pallas::Base,
) -> TauPallasResult<Vec<u8>> {
    let mut call_data = Vec::new();
    call_data.push(ATTESTATION_FUNCTION_VERIFY_CLAIM_V1);

    call_data.extend_from_slice(&claim_id.to_repr());
    call_data.extend_from_slice(&attestation_id.to_repr());
    call_data.extend_from_slice(&evidence_commitment.to_repr());
    call_data.extend_from_slice(&revealed_result.to_repr());
    call_data.extend_from_slice(&revocation_root.to_repr());
    call_data.extend_from_slice(&attestation_data.to_repr());

    Ok(call_data)
}

/// Build a Transaction for submitting a deliverable to the labor market.
///
/// The transaction includes:
/// 1. The labor market SubmitDeliverableV1 call (parent)
/// 2. An Attestation::VerifyClaimV1 child call for on-chain attestation verification
/// 3. Optionally, an Identity::VerifyCapabilityV1 child call if capability is required
pub fn build_submit_deliverable_tx(
    labor_market_contract_id: [u8; 32],
    attestation_contract_id: [u8; 32],
    identity_contract_id: Option<[u8; 32]>,
    deliverable_calldata: Vec<u8>,
    verify_claim_calldata: Vec<u8>,
    verify_capability_calldata: Option<Vec<u8>>,
) -> TauPallasResult<Transaction> {
    use dwow_sdk::crypto::ContractId;

    let lm_cid = ContractId::from_bytes(labor_market_contract_id)
        .map_err(|e| TauPallasError::TransactionError(format!("Invalid labor_market contract id: {:?}", e)))?;
    let att_cid = ContractId::from_bytes(attestation_contract_id)
        .map_err(|e| TauPallasError::TransactionError(format!("Invalid attestation contract id: {:?}", e)))?;

    // Build child DarkTrees
    let mut children: Vec<DarkTree<ContractCall>> = Vec::new();

    // Always add attestation verification child call
    let attestation_call = ContractCall {
        contract_id: att_cid,
        data: verify_claim_calldata,
    };
    children.push(DarkTree::new(attestation_call, vec![], None, None));

    // Optionally add identity verification child call
    if let (Some(id_bytes), Some(verify_cap_calldata)) = (identity_contract_id, verify_capability_calldata) {
        let id_cid = ContractId::from_bytes(id_bytes)
            .map_err(|e| TauPallasError::TransactionError(format!("Invalid identity contract id: {:?}", e)))?;
        let identity_call = ContractCall {
            contract_id: id_cid,
            data: verify_cap_calldata,
        };
        children.push(DarkTree::new(identity_call, vec![], None, None));
    }

    // Build parent call (labor market submit deliverable)
    let parent_call = ContractCall {
        contract_id: lm_cid,
        data: deliverable_calldata,
    };

    let parent_tree = DarkTree::new(parent_call, children, None, None);

    let mut forest: DarkForest<ContractCall> = DarkForest::new(Some(1), Some(20));
    forest.append(parent_tree)
        .map_err(|e| TauPallasError::TransactionError(format!("Failed to append call tree: {:?}", e)))?;

    let calls: Vec<DarkLeaf<ContractCall>> = forest.build_vec()
        .map_err(|e| TauPallasError::TransactionError(format!("Failed to build call tree: {:?}", e)))?;

    // Compute transaction commitment: Blake3 hash of all call data
    let tx_commitment = {
        use blake3::Hasher;
        let mut hasher = Hasher::new();
        for call in &calls {
            let _ = call.data.encode(&mut hasher);
        }
        let hash = hasher.finalize();
        let mut bytes = [0u8; 32];
        bytes.copy_from_slice(hash.as_bytes());
        bytes
    };

    Ok(Transaction {
        calls,
        proofs: vec![],
        tx_commitment,
        nullifiers: vec![],
    })
}
