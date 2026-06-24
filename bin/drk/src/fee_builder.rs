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
    crypto::{pasta_prelude::PrimeField, BaseBlind, PublicKey, SecretKey, MerkleNode},
    pasta::pallas,
    tx::ContractCall,
};
use dwow_serial::Encodable;
use rand::rngs::OsRng;

use crate::contract_imports::native_token::{
    DRKW_TOKEN_ID, FeeCallBuilder, FeeCallInput, FeeCallOutput,
    NATIVE_TOKEN_CONTRACT_ZKAS_FEE_V1_BIN,
};
use crate::walletdb::WalletPtr;
use crate::NATIVE_TOKEN_CONTRACT_ID;

/// Default network fee in DRKW
pub const DEFAULT_FEE: u64 = 42_000_000;

/// Build fee call and finalize transaction.
///
/// When `fee_proofs` is provided, the proofs are attached to the fee leaf.
/// This supports the transfer.rs/token.rs pattern where fee ZK proofs are
/// merged into the main call's proof bundle. When `fee_proofs` is None
/// (the default for swap.rs/lib.rs), the fee leaf carries empty proofs.
pub fn build_fee_and_finalize_tx(
    wallet: &WalletPtr,
    call_leaf: ContractCallLeaf,
    fee_proofs: Option<Vec<Proof>>,
    exclude_cap_id: Option<&str>,
) -> Result<Transaction> {
    // Get DRKW cap for fee
    let dark_token_id_str = bs58::encode(DRKW_TOKEN_ID.to_repr()).into_string();
    let drkw_cap_records = wallet.get_capabilities_for_token(&dark_token_id_str, Some(false))
        .map_err(|e| Error::Custom(format!("Failed to get DRKW capabilities: {:?}", e)))?;

    if drkw_cap_records.is_empty() {
        return Err(Error::Custom(
            "No DRKW capabilities available for fee payment. \
             The wallet needs DRKW tokens to pay network fees.".to_string(),
        ));
    }

    // Select a DRKW cap for fee, excluding the transfer input cap if specified.
    // Prevents the same cap from being consumed twice (duplicate nullifier).
    let drkw_cap = if let Some(exclude_id) = exclude_cap_id {
        drkw_cap_records.iter()
            .find(|c| c.cap_id != exclude_id)
            .ok_or_else(|| Error::Custom(
                "No DRKW capabilities available for fee (all held caps consumed as transfer inputs). \
                 The wallet needs additional DRKW tokens.".to_string(),
            ))?
    } else {
        &drkw_cap_records[0]
    };

    // Pre-validate: the selected cap must have enough value to pay the fee.
    // saturating_sub handles underflow safely, but a cap with value < DEFAULT_FEE
    // produces a zero-value change output — the transaction would be rejected.
    if drkw_cap.value < DEFAULT_FEE {
        return Err(Error::Custom(format!(
            "Selected DRKW cap has insufficient value for fee ({} < {}). \
             The wallet needs DRKW tokens with at least the fee amount.",
            drkw_cap.value, DEFAULT_FEE
        )));
    }

    let dark_secret_bytes = bs58::decode(&drkw_cap.secret)
        .into_vec()
        .map_err(|e| Error::Custom(e.to_string()))?
        .try_into()
        .map_err(|_| Error::Custom("Invalid DRKW secret key length".to_string()))?;
    let dark_secret = SecretKey::from_bytes(dark_secret_bytes)
        .map_err(|_| Error::Custom("Failed to parse DRKW secret key".to_string()))?;

    // Get DRKW Merkle proof
    let dark_merkle_proof = wallet.get_merkle_proof(&drkw_cap.cap_id)
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

    // Decode dark cap blind
    let dark_coin_blind_bytes = bs58::decode(&drkw_cap.cap_blind)
        .into_vec()
        .map_err(|e| Error::Custom(e.to_string()))?
        .try_into()
        .map_err(|_| Error::Custom("Invalid cap blind length".to_string()))?;
    let drkw_cap_blind = pallas::Base::from_repr(dark_coin_blind_bytes)
        .into_option()
        .ok_or_else(|| Error::Custom("Invalid cap blind".to_string()))?;

    // Load fee ZK binary and build fee proof
    let fee_zkbin = ZkBinary::decode(NATIVE_TOKEN_CONTRACT_ZKAS_FEE_V1_BIN, false)
        .map_err(|e| Error::Custom(format!("Failed to decode fee ZK binary: {:?}", e)))?;

    let fee_empty_wits = empty_witnesses(&fee_zkbin)?;
    let fee_circuit = ZkCircuit::new(fee_empty_wits, &fee_zkbin);
    let fee_pk = ProvingKey::build(fee_zkbin.k, &fee_circuit);

    // Build fee input
    // Pre-compute tx_commitment = hash(main_call_data || fee_function_code).
    // The fee function code 0x00 is the canonical "a fee is being paid" marker.
    // Fee params are excluded from the hash to avoid circular dependency
    // (the proof's public inputs include tx_commitment, which would change the
    // call data, which would change tx_commitment).
    let tx_commitment: pallas::Base = {
        use blake3::Hasher;
        use dwow_serial::Encodable;
        let mut hasher = Hasher::new();
        let _ = call_leaf.call.encode(&mut hasher);
        hasher.update(&[0x00u8]); // FeeV1 function code
        let hash = hasher.finalize();
        let mut bytes = [0u8; 32];
        bytes.copy_from_slice(hash.as_bytes());
        pallas::Base::from_repr(bytes).into_option()
            .unwrap_or(pallas::Base::zero())
    };

    let fee_input = FeeCallInput {
        value: drkw_cap.value,
        token_id: DRKW_TOKEN_ID,
        spend_hook: pallas::Base::zero(),
        user_data: pallas::Base::zero(),
        coin_blind: drkw_cap_blind,
        leaf_position: drkw_cap.leaf_position,
        merkle_path: dark_merkle_path,
        secret: dark_secret,
        ephemeral_signature_secret: SecretKey::random(&mut OsRng),
        tx_commitment,
        tx_nonce: pallas::Base::zero(),
    };

    // Fee output - change goes back to our public key
    let dark_public_key = PublicKey::from_secret(dark_secret);
    let change_blind = BaseBlind::random(&mut OsRng);
    let fee_output = FeeCallOutput {
        recipient: dark_public_key,
        value: drkw_cap.value.saturating_sub(DEFAULT_FEE),
        spend_hook: pallas::Base::zero(),
        user_data: pallas::Base::zero(),
        coin_blind: change_blind.inner(),
    };

    // Build fee call
    let fee_builder = FeeCallBuilder {
        input: fee_input,
        output: fee_output,
        fee_zkbin,
        fee_pk,
        fee: DEFAULT_FEE,
    };

    let fee_debris = fee_builder.build()
        .map_err(|e| Error::Custom(format!("Failed to build fee: {:?}", e)))?;

    // Create fee call data
    let mut fee_call_data = vec![0x00u8]; // FeeV1 function code
    fee_debris.params.encode(&mut fee_call_data)
        .map_err(|e| Error::Custom(format!("Failed to encode fee params: {:?}", e)))?;

    let fee_call = ContractCall {
        contract_id: *NATIVE_TOKEN_CONTRACT_ID,
        data: fee_call_data,
    };

    // Fee leaf — carries proofs when fee_proofs is provided (transfer/token path).
    // When fee_proofs is None, the fee leaf has empty proofs (swap/lib path).
    let fee_proofs = fee_proofs.unwrap_or_default();
    let fee_leaf = ContractCallLeaf { call: fee_call, proofs: fee_proofs };

    // Build final transaction
    let mut tx_builder = TransactionBuilder::new(call_leaf, vec![])
        .map_err(|e| Error::Custom(format!("Failed to create transaction builder: {:?}", e)))?;

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

    #[test]
    fn test_saturating_sub_underflow_protection() {
        // If cap value < DEFAULT_FEE, saturating_sub returns 0 instead of panicking
        assert_eq!(10u64.saturating_sub(DEFAULT_FEE), 0);
        assert_eq!((DEFAULT_FEE - 1).saturating_sub(DEFAULT_FEE), 0);
    }

    #[test]
    fn test_saturating_sub_normal_case() {
        assert_eq!((DEFAULT_FEE + 100).saturating_sub(DEFAULT_FEE), 100);
    }
}