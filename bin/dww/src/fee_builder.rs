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

use dwow_chain::fee_window::{FeeWindowFlags, CongestionFactor, compute_fee};
use dwow_chain::opcode_cost::circuit_difficulty;
use dwow_core::{
    tx::{ContractCallLeaf, Transaction, TransactionBuilder},
    zk::{proof::ProvingKey, vm::ZkCircuit, vm_heap::empty_witnesses, Proof},
    zkas::ZkBinary,
};
use crate::wallet_error::{Error, Result};
use dwow_sdk::{
    blockchain::FeeAmount,
    crypto::{BaseBlind, PublicKey, SecretKey, MerkleNode},
    pasta::pallas,
    tx::ContractCall,
};
use rand::{rngs::StdRng, SeedableRng};

use crate::contract_imports::native_token::{
    DRKW_TOKEN_ID,
    FeeV2CallBuilder, FeeV2CallInput, FeeV2CallOutput,
    NATIVE_TOKEN_CONTRACT_ZKAS_FEE_V2_BIN,
    NATIVE_TOKEN_CONTRACT_ZKAS_FEE_THRESHOLD_V1_BIN,
};
use dwow_native_token_contract::model::fee::ThresholdTxBinding;
use crate::walletdb::WalletPtr;
use crate::NATIVE_TOKEN_CONTRACT_ID;

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
///
/// `circuit_costs`: per-opcode difficulty values for this transaction's circuits.
/// `wasm_kb`: deployed WASM size in kB (0 for non-deploy transactions).
/// `fee_window_flags`: from the latest block header, used to derive congestion
/// factors for the two-component fee formula.
///
/// Schnorr signatures removed per contract-standards.md §3.
pub fn build_fee_and_finalize_tx(
    wallet: &WalletPtr,
    account_mgr: &dwow_accounts::AccountManager,
    call_leaf: ContractCallLeaf,
    fee_proofs: Option<Vec<Proof>>,
    exclude_cap_id: Option<&str>,
    seed: [u8; 32],
    circuit_costs: &[u64],
    wasm_kb: u64,
    fee_window_flags: FeeWindowFlags,
) -> Result<Transaction> {
    // Derive congestion factors from the latest block header flags.
    // wallet.md §6.4.2 / fee-spec.md §8.2.
    let (circuit_cf, wasm_cf) = fee_window_flags.derive_cfs();

    // Compute the minimum admission fee via the two-component formula.
    // fee = (wasm_kB × BASELINE_STORAGE × WASM_CF) + (Σ opcode_difficulty × CIRCUIT_CF)
    // Always uses premium CF — this is the admission threshold.

    // Decode fee zkbins early to compute their circuit difficulty for the fee.
    // fee-spec.md §12.11: circuit_difficulty scales with k-value.
    let fee_zkbin_cost = ZkBinary::decode(NATIVE_TOKEN_CONTRACT_ZKAS_FEE_V2_BIN, false)
        .map(|zkbin| circuit_difficulty(&zkbin.opcodes, zkbin.k))
        .unwrap_or(0);
    let threshold_zkbin_cost = ZkBinary::decode(NATIVE_TOKEN_CONTRACT_ZKAS_FEE_THRESHOLD_V1_BIN, false)
        .map(|zkbin| circuit_difficulty(&zkbin.opcodes, zkbin.k))
        .unwrap_or(0);

    // Combine caller-provided main-call circuit costs with fee circuit costs.
    let all_circuit_costs: Vec<u64> = circuit_costs.iter()
        .copied()
        .chain([fee_zkbin_cost, threshold_zkbin_cost])
        .collect();

    let fee = compute_fee(&all_circuit_costs, wasm_kb, wasm_cf, circuit_cf);
    let fee_value = fee.get();
    let threshold = fee; // FeeThreshold_V1 proves: fee_paid >= this threshold
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
    // saturating_sub handles underflow safely, but a cap with value < fee
    // produces a zero-value change output — the transaction would be rejected.
    if fee_cap.value < fee_value {
        return Err(Error::Custom(format!(
            "Selected DRKW cap has insufficient value for fee ({} < {}). \
             The wallet needs DRKW tokens with at least the fee amount.",
            fee_cap.value, fee_value
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

    // Threshold is the computed fee — the FeeThreshold_V1 proof shows
    // fee_paid >= threshold. Threshold selection uses the two-component
    // formula with premium CFs (already computed above as `fee`).

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
        value: fee_cap.value.saturating_sub(fee_value),
        spend_hook: pallas::Base::zero(),
        user_data: pallas::Base::zero(),
        coin_blind: change_blind.inner(),
    };

    // Build FeeThreshold_V1 proof (wallet→mempool gate: fee >= threshold).
    // Fee lifecycle step 1 — delegated to contract crate client
    // (client/fee_threshold.rs, single source of truth per G7).
    // Per fee-spec.md §5.5.1: ThresholdTxBinding binds proof to a specific threshold.
    let threshold_tx_binding = ThresholdTxBinding::compute(
        fee_input.tx_commitment,
        threshold,
    );
    let threshold_proof = dwow_native_token_contract::client::fee_threshold::create_fee_threshold_proof(
        &threshold_zkbin,
        &threshold_pk,
        fee,
        threshold,
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
        fee_amount: fee,
        threshold,
        fee_zkbin: fee_zkbin.clone(),
        fee_pk,
        threshold_proof_bytes,
    };

    let fee_v2_result = fee_builder.build()
        .map_err(|e| Error::Custom(format!("Failed to build FeeV2: {:?}", e)))?;

    // G2 Phase 2 (red team): encrypt fee_amount to miner's public key.
    // FIXME: wire miner_public_key from wallet P2P config. When available:
    //   let mut params = fee_v2_result.params;
    //   params.encrypted_fee_value = encrypt_fee_for_miner(
    //       fee_v2_result.params.fee_amount,
    //       &miner_public_key,
    //   )?;
    //   let fee_call_data = [0x08][params.encode()]
    // For now, encrypted_fee_value is empty — min_fee check uses threshold proof,
    // and FeeCollectV1 total_fees falls back to estimate in prepare_block().

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

/// Encrypt a fee amount to the miner's public key using AEAD (ECDH + ChaCha20-Poly1305).
///
/// Per red-team guardrails G7 and fee-spec.md §5.6.3: the fee value crosses from
/// wallet-private to miner-known ONLY through this encryption. The miner decrypts
/// in `prepare_block()` to compute the correct `total_fees` for FeeCollectV1.
///
/// Ciphertext format: [ephemeral_public (32B) || nonce (12B) || ciphertext+tag (24B)] = 68 bytes.
/// Key cycling: the miner's key is per-block derived via `derive_instance(NATIVE_TOKEN, height)`,
/// so encrypted fees cannot be correlated across blocks by public key.
pub fn encrypt_fee_for_miner(
    fee_amount: FeeAmount,
    miner_public_key: &PublicKey,
) -> Result<Vec<u8>> {
    use dwow_sdk::crypto::diffie_hellman::{sapling_ka_agree, kdf_sapling};
    use dwow_sdk::crypto::SecretKey;
    use chacha20poly1305::{AeadInPlace, ChaCha20Poly1305, KeyInit};
    use rand::rngs::OsRng;

    // 1. Generate ephemeral keypair
    let ephem_secret = SecretKey::random(&mut OsRng);
    let ephem_public = PublicKey::from_secret(ephem_secret.clone());

    // 2. ECDH key agreement
    let shared_secret = sapling_ka_agree(&ephem_secret, miner_public_key)
        .map_err(|_| Error::Custom("fee encrypt: ECDH failed".into()))?;
    let key = kdf_sapling(&shared_secret, &ephem_public);

    // 3. Derive deterministic nonce from ephemeral public key
    let nonce_hash = blake3::hash(ephem_public.to_bytes().as_ref());
    let mut nonce = [0u8; 12];
    nonce.copy_from_slice(&nonce_hash.as_bytes()[..12]);

    // 4. Encrypt fee bytes with ChaCha20-Poly1305
    let fee_bytes = fee_amount.get().to_le_bytes();
    let mut buf = vec![0u8; 24]; // 8 data + 16 tag
    buf[..8].copy_from_slice(&fee_bytes);
    ChaCha20Poly1305::new(key.as_ref().into())
        .encrypt_in_place((&nonce).into(), b"darkfi_fee", &mut buf)
        .map_err(|e| Error::Custom(format!("fee encrypt: {:?}", e)))?;

    // 5. Format output: [ephemeral_public (32)] [nonce (12)] [ciphertext+tag (24)]
    let mut out = Vec::with_capacity(68);
    out.extend_from_slice(&ephem_public.to_bytes());
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&buf);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use dwow_chain::fee_window::{WindowSignalling, CongestionFactor, compute_fee, BASELINE_STORAGE};
    use dwow_sdk::crypto::{PublicKey, SecretKey};

    /// FeeWindowFlags.derive_cfs() — default (inactive) flags yield identity CFs.
    #[test]
    fn test_derive_cfs_default() {
        let flags = FeeWindowFlags::default();
        let (circuit_cf, wasm_cf) = flags.derive_cfs();
        assert_eq!(circuit_cf, CongestionFactor::default());
        assert_eq!(wasm_cf, CongestionFactor::default());
    }

    /// FeeWindowFlags.derive_cfs() — circuit active +10%, wasm inactive.
    #[test]
    fn test_derive_cfs_circuit_increase() {
        let flags = FeeWindowFlags::pack(
            WindowSignalling::encode_cm(0x01), // circuit: +10%
            WindowSignalling::LEGACY,           // wasm: inactive
        );
        let (circuit_cf, wasm_cf) = flags.derive_cfs();
        let expected_premium = ((CongestionFactor::SCALE as u64) * 110 / 100) as u32;
        assert_eq!(circuit_cf.premium(), expected_premium);
        assert_eq!(circuit_cf.standard(), CongestionFactor::SCALE);
        assert_eq!(wasm_cf, CongestionFactor::default());
    }

    /// FeeWindowFlags.derive_cfs() — circuit active -10%, wasm active hold.
    #[test]
    fn test_derive_cfs_circuit_decrease_wasm_hold() {
        let flags = FeeWindowFlags::pack(
            WindowSignalling::encode_cm(0x02), // circuit: -10%
            WindowSignalling::encode_cm(0x00), // wasm: hold
        );
        let (circuit_cf, wasm_cf) = flags.derive_cfs();
        let expected_premium = ((CongestionFactor::SCALE as u64) * 90 / 100) as u32;
        assert_eq!(circuit_cf.premium(), expected_premium);
        assert_eq!(circuit_cf.standard(), CongestionFactor::SCALE);
        assert_eq!(wasm_cf.premium(), CongestionFactor::SCALE, "wasm hold = identity");
    }

    /// compute_fee() — zero congestion, minimal circuit.
    #[test]
    fn test_compute_fee_zero_congestion() {
        let cf = CongestionFactor::default();
        // Single opcode with difficulty 1000 (average circuit), 1 kB WASM.
        let fee = compute_fee(&[1000], 1, cf, cf);
        // wasm = 1 * 1_000_000 * 1_000_000 / 1_000_000 = 1_000_000
        // circuit = 1000 * 1_000_000 / 1_000_000 = 1000
        // total = 1_001_000
        assert_eq!(fee.get(), 1_001_000);
    }

    /// compute_fee() — CF at +10%, circuit-heavy.
    #[test]
    fn test_compute_fee_congested() {
        let premium = ((CongestionFactor::SCALE as u64) * 110 / 100) as u32;
        let cf = CongestionFactor::new(premium, CongestionFactor::SCALE);
        let fee = compute_fee(&[5000], 1, cf, cf);
        // wasm = 1 * 1_000_000 * 1_100_000 / 1_000_000 = 1_100_000
        // circuit = 5000 * 1_100_000 / 1_000_000 = 5_500
        // total = 1_105_500
        assert_eq!(fee.get(), 1_105_500);
    }

    /// compute_fee() — WASM-heavy deploy (50 kB).
    #[test]
    fn test_compute_fee_wasm_heavy() {
        let cf = CongestionFactor::default();
        let fee = compute_fee(&[1000], 50, cf, cf);
        // wasm = 50 * 1_000_000 * 1_000_000 / 1_000_000 = 50_000_000
        // circuit = 1000 * 1_000_000 / 1_000_000 = 1000
        // total = 50_001_000
        assert_eq!(fee.get(), 50_001_000);
    }

    /// compute_fee() — empty circuit costs, no WASM.
    #[test]
    fn test_compute_fee_minimal() {
        let cf = CongestionFactor::default();
        let fee = compute_fee(&[], 0, cf, cf);
        assert_eq!(fee.get(), 0);
    }

    /// FeeWindowFlags flags roundtrip through typed API.
    #[test]
    fn test_flags_roundtrip() {
        let flags = FeeWindowFlags::pack(
            WindowSignalling::encode_cm(0x01), // circuit +10%
            WindowSignalling::encode_cm(0x02), // wasm -10%
        );
        assert!(flags.is_active());
        assert_eq!(flags.circuit_byte().congestion_multiplier(), 0x01);
        assert_eq!(flags.wasm_byte().congestion_multiplier(), 0x02);

        let bytes = flags.to_le_bytes();
        let decoded = FeeWindowFlags::from_le_bytes(bytes);
        assert_eq!(decoded, flags);
    }

    /// G6: encrypt_fee_for_miner produces valid AEAD ciphertext (pk + nonce + data + tag).
    #[test]
    fn test_encrypt_fee_for_miner_format() {
        let fee = FeeAmount::new(42_000_000);
        let miner_sk = SecretKey::random(&mut rand::rngs::OsRng);
        let miner_pk = PublicKey::from_secret(miner_sk);
        let ciphertext = encrypt_fee_for_miner(fee, &miner_pk)
            .expect("encrypt_fee_for_miner must succeed");
        // Format: [ephemeral_public] [nonce(12)] [ciphertext+tag(24)]
        assert!(ciphertext.len() >= 44,
            "AEAD ciphertext must be at least 44 bytes (32 pk + 12 nonce), got {}",
            ciphertext.len());
    }

    /// G6: Different fees produce different ciphertexts.
    #[test]
    fn test_encrypt_fee_different_ciphertext() {
        let miner_sk = SecretKey::random(&mut rand::rngs::OsRng);
        let miner_pk = PublicKey::from_secret(miner_sk);
        let c1 = encrypt_fee_for_miner(FeeAmount::new(42_000_000), &miner_pk).unwrap();
        let c2 = encrypt_fee_for_miner(FeeAmount::new(15_000_000), &miner_pk).unwrap();
        assert_ne!(c1, c2, "different fees must produce different ciphertexts");
    }
}