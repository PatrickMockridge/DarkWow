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

use dwow_chain::fee_window::{FeeWindowFlags, compute_fee_v3, compute_storage_fee};
use dwow_chain::opcode_cost::circuit_difficulty;
use dwow_core::{
    tx::{ContractCallLeaf, Transaction, TransactionBuilder},
    zk::{proof::ProvingKey, vm::ZkCircuit, vm_heap::empty_witnesses, Proof},
    zkas::ZkBinary,
};
use crate::wallet_error::{Error, Result};
use dwow_sdk::{
    blockchain::{FeeAmount, FeeTier, RiskFactor, WasmKb},
    crypto::{BaseBlind, PublicKey, SecretKey, MerkleNode},
    mass_balance_call_data::MassBalanceFeeV2CallData,
    pasta::pallas,
    tx::ContractCall,
};
use rand::{rngs::StdRng, SeedableRng};

use crate::contract_imports::native_token::{
    DRKW_ASSET_ID,
    FeeV2CallBuilder, FeeV2CallInput, FeeV2CallOutput,
    NATIVE_TOKEN_CONTRACT_ZKAS_FEE_V2_BIN,
};
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
    risk_factor: RiskFactor,
    wasm_kb: WasmKb,
    fee_window_flags: FeeWindowFlags,
    tier: FeeTier,
) -> Result<Transaction> {
    // Derive the circuit congestion factor from the latest block header flags.
    // wallet.md §6.4.2 / fee-spec.md §8.2. (The WASM CF was only used for the
    // removed storage term; storage is flat per §12.4.3.)
    let (circuit_cf, _wasm_cf) = fee_window_flags.derive_cfs();

    // Compute the admission fee via the FeeV3 gas-framing formula:
    // fee = gas × CF × tier × risk  (+ storage fee for DeployV1 only).
    // gas is the fee in wow — no base-price multiplier. fee-spec.md §12.4.1/§12.4.3.

    // Decode the fee zkbin to compute the fee circuit's gas (Σ rows).
    // These binaries are embedded at compile time — decode failure is a build bug.
    #[expect(clippy::expect_used, reason = "embedded zkbin is valid at compile time — decode failure is a build bug")]
    let fee_zkbin_cost = ZkBinary::decode(NATIVE_TOKEN_CONTRACT_ZKAS_FEE_V2_BIN, false)
        .map(|zkbin| circuit_difficulty(&zkbin.opcodes))
        .expect("FeeV2 zkbin decode failed — embedded binary corrupted at build time");

    // gas = Σ main-call circuit rows + fee circuit rows. Risk is a single
    // multiplier (dynamic ContractRiskTracker), not per-circuit.
    let gas: u64 = circuit_costs.iter().sum::<u64>().saturating_add(fee_zkbin_cost);
    let circuit_fee = compute_fee_v3(gas, circuit_cf, tier, risk_factor);
    // Deploy storage fee — 0 for non-deploy (wasm_kb = WasmKb::ZERO).
    let storage_fee = compute_storage_fee(wasm_kb);
    let fee = FeeAmount::new(circuit_fee.get().saturating_add(storage_fee.get()));
    let fee_value = fee.get();
    // wallet.md §6.1: Seed-derived randomness — no OsRng.
    let mut rng = StdRng::from_seed(seed);
    // Get DRKW cap for fee
    let fee_cap_records = wallet.get_capabilities_by_asset(&DRKW_ASSET_ID, Some(false))
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
    // wallet.md §6.4.0: prefer the persisted spend_secret (fresh for received
    // TransferV1/SpendV1 outputs); fall back to key_coords for self-issued
    // coinbase/fee coins (derivable via resolve_key).
    let coords = fee_cap.key_coords.as_ref();
    let dark_secret = if let Some(s) = &fee_cap.spend_secret {
        s.clone()
    } else {
        let coords = coords.ok_or_else(|| Error::Custom(format!(
            "fee cap {} has no key_coords or spend_secret", fee_cap.cap_id,
        )))?;
        let owned = account_mgr.resolve_key(coords)
            .map_err(|e| Error::Custom(format!("resolve_key fee cap: {}", e)))?;
        owned.expose_secret().clone()
    };
    // Change-output key: ALWAYS the account's MASTER key (not the cap's
    // per-block key). Path 1 scan trials master keys at every height but
    // per-block keys only at their own height, so change sent to an old
    // per-block key would never be rediscovered (locked change).
    let change_secret = {
        let coords = coords.ok_or_else(|| Error::Custom(format!(
            "fee cap {} has no key_coords", fee_cap.cap_id,
        )))?;
        let owned = account_mgr.resolve_key(&dwow_accounts::KeyCoordinates {
            account_index: coords.account_index,
            derivation: dwow_accounts::KeyDerivation::Master,
        }).map_err(|e| Error::Custom(format!("resolve_key fee change: {}", e)))?;
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

    // Fee output - change goes back to our MASTER key (rediscoverable at any height).
    let dark_public_key = PublicKey::from_secret(change_secret.clone());
    let change_blind = BaseBlind::random(&mut rng);

    let fee_input = FeeV2CallInput {
        value: fee_cap.value,
        asset_id: DRKW_ASSET_ID.inner(),
        spend_hook: pallas::Base::zero(),
        user_data: pallas::Base::zero(),
        commitment_blind: fee_cap_blind,
        leaf_position: dark_merkle_proof.leaf_position,
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
        commitment_blind: change_blind.inner(),
    };

    // Build FeeV3 call — plaintext fee + tier (no threshold proof, no encrypt).
    // The Fee_V2 mass-balance proof (Pedersen input = output + fee) is constructed
    // by build() and retained verbatim.
    let fee_builder = FeeV2CallBuilder {
        input: fee_input,
        output: fee_output,
        fee_amount: fee,
        tier,
        fee_zkbin: fee_zkbin.clone(),
        fee_pk,
    };

    let mut fee_v2_result = fee_builder.build()
        .map_err(|e| Error::Custom(format!("Failed to build FeeV3: {:?}", e)))?;

    // FeeV3 call data via nominal MassBalanceFeeV2CallData (type-system.md §8.2.3, §10.5).
    // The selector (0x08) is unchanged; the payload is now FeeParamsV3.
    // This is the SINGLE constructor — no raw vec![0x08u8] anywhere.
    let fee_call_data = MassBalanceFeeV2CallData::new(fee_v2_result.params.encode()).encode();

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
    use dwow_chain::fee_window::{WindowSignalling, CongestionFactor, CfValue, BASELINE_STORAGE};
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
        assert_eq!(circuit_cf.premium(), CfValue::new(expected_premium));
        assert_eq!(circuit_cf.standard(), CfValue::new(CongestionFactor::SCALE));
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
        assert_eq!(circuit_cf.premium(), CfValue::new(expected_premium));
        assert_eq!(circuit_cf.standard(), CfValue::new(CongestionFactor::SCALE));
        assert_eq!(wasm_cf.premium(), CfValue::new(CongestionFactor::SCALE), "wasm hold = identity");
    }

    /// compute_storage_fee() — deploy storage fee is flat per-kB.
    #[test]
    fn test_compute_storage_fee() {
        assert_eq!(compute_storage_fee(WasmKb::new(0)).get(), 0);
        assert_eq!(compute_storage_fee(WasmKb::new(1)).get(), BASELINE_STORAGE);
        assert_eq!(compute_storage_fee(WasmKb::new(50)).get(), 50 * BASELINE_STORAGE);
    }

    /// compute_fee_v3() — transaction fee is gas (no storage).
    #[test]
    fn test_compute_fee_v3_no_storage() {
        let cf = CongestionFactor::default();
        let fee = compute_fee_v3(1000, cf, FeeTier::LOW, RiskFactor::BASELINE);
        assert_eq!(fee.get(), 1000);
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
}