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

//! Fee builder helper for contract transactions
//!
//! Shared functionality for building fee calls and finalizing transactions.

use dwow_core::{
    tx::{ContractCallLeaf, Transaction, TransactionBuilder},
    zk::{proof::ProvingKey, vm::ZkCircuit, vm_heap::empty_witnesses, Proof},
    zkas::ZkBinary,
};
use crate::wallet_error::{Error, Result};
use dwow_sdk::{
    blockchain::FeeAmount,
    crypto::{BaseBlind, PublicKey, SecretKey, MerkleNode, constants::DRK_POSEIDON_DOMAIN_TX_BINDING, poseidon_hash},
    pasta::pallas,
    tx::ContractCall,
};
use dwow_serial::Encodable;
use rand::{rngs::StdRng, SeedableRng};

use crate::contract_imports::native_token::{
    DRKW_TOKEN_ID,
    FeeV2CallBuilder, FeeV2CallInput, FeeV2CallOutput,
    NATIVE_TOKEN_CONTRACT_ZKAS_FEE_V2_BIN,
    NATIVE_TOKEN_CONTRACT_ZKAS_FEE_THRESHOLD_V1_BIN,
};
use dwow_native_token_contract::client::fee::FeeV2CallBuilder as _;
use crate::walletdb::WalletPtr;
use crate::NATIVE_TOKEN_CONTRACT_ID;

/// Default network fee in DRKW (base units). Also the minimum fee per call.
pub const DEFAULT_FEE: u64 = 42_000_000;

/// Estimated fee per additional input beyond the first.
/// Each input adds a Merkle proof verification to the ZK circuit.
pub const FEE_PER_ADDITIONAL_INPUT: u64 = 10_000_000;

/// Estimate the transaction fee based on complexity.
///
/// Base fee (DEFAULT_FEE) + per-additional-input fees.
/// Additional outputs (change) do not increase the fee.
///
/// Returns the estimated fee, which is always >= DEFAULT_FEE.
///
/// TODO: query node's FeeEstimator (RPC: tx.calculate_fee) for dynamic
/// fee based on recent block gas utilization. Fall back to static formula
/// if node unreachable.
pub fn estimate_fee(num_inputs: usize, _num_outputs: usize) -> u64 {
    let extra_inputs = num_inputs.saturating_sub(1);
    DEFAULT_FEE + (extra_inputs as u64 * FEE_PER_ADDITIONAL_INPUT)
}

/// Build fee call and finalize transaction.
///
/// When `fee_proofs` is provided, the proofs are attached to the fee leaf.
/// This supports the transfer.rs/token.rs pattern where fee ZK proofs are
/// merged into the main call's proof bundle. When `fee_proofs` is None
/// (the default for swap.rs/lib.rs), the fee leaf carries empty proofs.
/// P0.1c: Delegation through AccountManager. The fee cap's key is resolved
/// via `account_mgr.resolve_key(cap.key_coords)` — never assumes `secrets[0]`
/// (which is the master key, unable to witness per-block-derived coinbase caps).
/// `fee_proofs` is optional for callers that merge fee ZK proofs into the main
/// proof bundle.
/// Schnorr signatures removed per contract-standards.md §3.
pub fn build_fee_and_finalize_tx(
    wallet: &WalletPtr,
    account_mgr: &dwow_accounts::AccountManager,
    call_leaf: ContractCallLeaf,
    fee_proofs: Option<Vec<Proof>>,
    exclude_cap_id: Option<&str>,
    seed: [u8; 32],
    // Fee window flags from latest block header (fee-spec.md §12.6).
    // 0 = legacy static fees. Feature-gated — ignored without feature.
    #[allow(unused_variables)]
    fee_window_flags: u16,
) -> Result<Transaction> {
    // wallet.md §6.1: Seed-derived randomness — no OsRng.
    let mut rng = StdRng::from_seed(seed);
    // Get DRKW cap for fee
    let fee_cap_records = wallet.get_capabilities_by_asset(&DRKW_TOKEN_ID, Some(false))
        .map_err(|e| Error::Custom(format!("Failed to get DRKW capabilities: {:?}", e)))?;

    if fee_cap_records.is_empty() {
        return Err(Error::Custom(
            "No DRKW capabilities available for fee payment. \
             The wallet needs DRKW tokens to pay network fees.".to_string(),
        ));
    }

    // Select a DRKW cap for fee, excluding the transfer input cap if specified.
    // Prevents the same cap from being consumed twice (duplicate nullifier).
    let fee_cap = if let Some(exclude_id) = exclude_cap_id {
        fee_cap_records.iter()
            .find(|c| c.cap_id != exclude_id)
            .ok_or_else(|| Error::Custom(
                "No DRKW capabilities available for fee (all held caps consumed as transfer inputs). \
                 The wallet needs additional DRKW tokens.".to_string(),
            ))?
    } else {
        &fee_cap_records[0]
    };

    // Pre-validate: the selected cap must have enough value to pay the fee.
    // saturating_sub handles underflow safely, but a cap with value < DEFAULT_FEE
    // produces a zero-value change output — the transaction would be rejected.
    if fee_cap.value < DEFAULT_FEE {
        return Err(Error::Custom(format!(
            "Selected DRKW cap has insufficient value for fee ({} < {}). \
             The wallet needs DRKW tokens with at least the fee amount.",
            fee_cap.value, DEFAULT_FEE
        )));
    }

    // P0.1c: resolve the cap's OWN key through AccountManager delegation.
    // The fee cap carries key_coords (set at scan time); resolve_key
    // re-derives the per-block or master key that actually owns this cap.
    // SecretKey is Copy — dereference the borrow from expose_secret().
    let dark_secret = {
        let coords = fee_cap.key_coords.as_ref()
            .ok_or_else(|| Error::Custom(format!(
                "fee cap {} has no key_coords — cannot determine owning secret", fee_cap.cap_id,
            )))?;
        let owned = account_mgr.resolve_key(coords)
            .map_err(|e| Error::Custom(format!("resolve_key fee cap: {}", e)))?;
        owned.expose_secret().clone()
    };

    // Get DRKW Merkle proof
    let dark_merkle_proof = wallet.get_merkle_proof(&fee_cap.cap_id)
        .map_err(|e| Error::Custom(format!("Failed to get DRKW Merkle proof: {:?}", e)))?;

    let dark_merkle_path: Vec<MerkleNode> = dark_merkle_proof
        .siblings
        .iter()
        .map(|s| {
            let bytes: [u8; 32] = bs58::decode(s)
                .into_vec()
                .map_err(|e| Error::Custom(e.to_string()))?
                .try_into()
                .map_err(|_| Error::Custom("Invalid Merkle node length".to_string()))?;
            Ok(MerkleNode::from_bytes(bytes)
                .ok_or_else(|| Error::Custom("Invalid Merkle node".to_string()))?)
        })
        .collect::<Result<Vec<_>>>()?;

    // Decode merkle root from the wallet's production tree (already stored in DB).
    // SHALL NOT be recomputed manually in the proof builder.
    let dark_merkle_root = {
        let root_bytes: [u8; 32] = bs58::decode(&dark_merkle_proof.root)
            .into_vec()
            .map_err(|e| Error::Custom(e.to_string()))?
            .try_into()
            .map_err(|_| Error::Custom("Invalid Merkle root length".to_string()))?;
        MerkleNode::from_bytes(root_bytes)
            .ok_or_else(|| Error::Custom("Invalid Merkle root".to_string()))?
    };

    // CapBlind is now BaseBlind — typed, no from_repr round-trip needed
    let fee_cap_blind = fee_cap.cap_blind.inner();

    // Load fee ZK binary and build fee proof
    let fee_zkbin = ZkBinary::decode(NATIVE_TOKEN_CONTRACT_ZKAS_FEE_V2_BIN, false)
        .map_err(|e| Error::Custom(format!("Failed to decode fee ZK binary: {:?}", e)))?;

    let fee_empty_wits = empty_witnesses(&fee_zkbin)?;
    let fee_circuit = ZkCircuit::new(fee_empty_wits, &fee_zkbin);
    let fee_pk = ProvingKey::build(fee_zkbin.k, &fee_circuit)
        .map_err(|e| Error::Custom(format!("ProvingKey::build fee: {:?}", e)))?;

    // HAZOP C7 fix: per-transaction random nonces make tx_binding unique.
    let tx_commitment: pallas::Base = BaseBlind::random(&mut rng).inner();
    let tx_nonce: pallas::Base = BaseBlind::random(&mut rng).inner();

    // Fee output - change goes back to our public key
    let dark_public_key = PublicKey::from_secret(dark_secret.clone());
    let change_blind = BaseBlind::random(&mut rng);

    // Load FeeThreshold_V1 circuit for threshold proof
    let threshold_zkbin = ZkBinary::decode(
        NATIVE_TOKEN_CONTRACT_ZKAS_FEE_THRESHOLD_V1_BIN, false,
    ).map_err(|e| Error::Custom(format!("Failed to decode threshold ZK binary: {:?}", e)))?;
    let threshold_empty_wits = empty_witnesses(&threshold_zkbin)?;
    let threshold_circuit = ZkCircuit::new(threshold_empty_wits, &threshold_zkbin);
    let threshold_pk = ProvingKey::build(threshold_zkbin.k, &threshold_circuit)
        .map_err(|e| Error::Custom(format!("ProvingKey::build threshold: {:?}", e)))?;

    // Threshold selection per wallet.md §6.4.2 / fee-spec.md §8.2.
    // When fee window is active, thresholds are decoded from the latest
    // block header's fee_window_flags. Otherwise, legacy static constants.
    #[cfg(feature = "fee-window")]
    let (premium_threshold, general_threshold) = {
        if fee_window_flags & 0x01 != 0 {
            // Fee window active — decode congestion_multiplier from bits [4:8]
            let cm = (fee_window_flags >> 4) & 0x0F;
            let base_premium = 420_000_000u64; // CF=1.0, rate=10
            let base_general = 42_000_000u64;  // CF=1.0, rate=1
            let premium = match cm {
                0x01 => ((base_premium as u128) * 110 / 100) as u64, // +10%
                0x02 => ((base_premium as u128) * 90 / 100) as u64,  // -10%
                _ => base_premium, // hold or legacy
            };
            (premium, base_general)
        } else {
            (42_000_000u64, 1_000_000u64) // legacy static
        }
    };
    #[cfg(not(feature = "fee-window"))]
    let premium_threshold: u64 = 42_000_000;
    #[cfg(not(feature = "fee-window"))]
    let general_threshold: u64 = 1_000_000;

    let threshold = if DEFAULT_FEE >= premium_threshold {
        premium_threshold
    } else {
        general_threshold
    };

    let fee_input = FeeV2CallInput {
        value: fee_cap.value,
        token_id: DRKW_TOKEN_ID.inner(),
        spend_hook: pallas::Base::zero(),
        user_data: pallas::Base::zero(),
        coin_blind: fee_cap_blind,
        leaf_position: fee_cap.leaf_position,
        merkle_path: dark_merkle_path,
        merkle_root: dark_merkle_root,
        secret: dark_secret.clone(),
        ephemeral_signature_secret: SecretKey::random(&mut rng),
        tx_commitment,
        tx_nonce,
    };

    let fee_output = FeeV2CallOutput {
        recipient: dark_public_key,
        value: fee_cap.value.saturating_sub(DEFAULT_FEE),
        spend_hook: pallas::Base::zero(),
        user_data: pallas::Base::zero(),
        coin_blind: change_blind.inner(),
    };

    // Build FeeThreshold_V1 proof (wallet→mempool gate: fee >= threshold).
    // Fee lifecycle step 1 — constructed here in the wallet crate, not in
    // the contract crate. See fee_threshold_proof.rs for the full lifecycle.
    let threshold_tx_binding = poseidon_hash([
        DRK_POSEIDON_DOMAIN_TX_BINDING,
        fee_input.tx_commitment,
        crate::fee_threshold_proof::fee_to_base(FeeAmount::new(threshold)),
    ]);
    let threshold_proof = crate::fee_threshold_proof::create_fee_threshold_proof(
        &threshold_zkbin,
        &threshold_pk,
        FeeAmount::new(DEFAULT_FEE),
        FeeAmount::new(threshold),
        fee_input.tx_commitment,
        threshold_tx_binding,
    ).map_err(|e| Error::Custom(format!("FeeThreshold_V1 proof: {}", e)))?;

    // Serialize threshold proof for embedding in FeeParamsV2
    let mut threshold_proof_bytes = vec![];
    dwow_serial::Encodable::encode(&threshold_proof, &mut threshold_proof_bytes)
        .map_err(|e| Error::Custom(format!("threshold proof encode: {:?}", e)))?;

    // Build FeeV2 call — privacy-preserving fee payment.
    // Fee_V2 proof (Pedersen mass balance) is constructed by build().
    // FeeThreshold_V1 proof is provided externally (built above).
    let fee_builder = FeeV2CallBuilder {
        input: fee_input,
        output: fee_output,
        fee_amount: FeeAmount::new(DEFAULT_FEE),
        threshold: FeeAmount::new(threshold),
        fee_zkbin: fee_zkbin.clone(),
        fee_pk,
        threshold_proof_bytes,
    };

    let fee_v2_result = fee_builder.build()
        .map_err(|e| Error::Custom(format!("Failed to build FeeV2: {:?}", e)))?;

    // Create FeeV2 call data: [0x08 selector][FeeParamsV2 encoded]
    // NO clear-text fee bytes — spec: fee-spec.md §5.2
    let mut fee_call_data = vec![0x08u8];
    fee_call_data.extend_from_slice(&fee_v2_result.params.encode());

    let fee_call = ContractCall {
        contract_id: *NATIVE_TOKEN_CONTRACT_ID,
        data: fee_call_data,
    };

    // P2.2: use the REAL fee proofs from the ZK builder (not empty).
    // fee_proofs param is for callers that merge proofs externally; default
    // to the proofs the builder just produced.
    let fee_leaf_proofs = if let Some(ext) = fee_proofs {
        if ext.is_empty() { fee_v2_result.proofs } else { ext }
    } else {
        fee_v2_result.proofs
    };
    let fee_leaf = ContractCallLeaf { call: fee_call, proofs: fee_leaf_proofs };

    // Collect nullifiers for mempool double-spend detection.
    let nf = fee_v2_result.params.input.nullifier;

    // Build final transaction
    let mut tx_builder = TransactionBuilder::new(call_leaf, vec![])
        .map_err(|e| Error::Custom(format!("Failed to create transaction builder: {:?}", e)))?;
    tx_builder.nullifiers.push(nf);

    tx_builder.append(fee_leaf, vec![])
        .map_err(|e| Error::Custom(format!("Failed to append fee call: {:?}", e)))?;

    let tx = tx_builder.build()
        .map_err(|e| Error::Custom(format!("Failed to build transaction: {:?}", e)))?;

    Ok(tx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_fee_value() {
        assert_eq!(DEFAULT_FEE, 42_000_000);
    }

}