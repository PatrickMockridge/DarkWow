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

use dwow::{
    tx::{ContractCallLeaf, Transaction, TransactionBuilder},
    zk::{proof::ProvingKey, vm::ZkCircuit, vm_heap::empty_witnesses},
    zkas::ZkBinary,
    Error, Result,
};
use dwow_sdk::{
    crypto::{pasta_prelude::PrimeField, BaseBlind, PublicKey, SecretKey, MerkleNode},
    pasta::pallas,
    tx::ContractCall,
};
use dwow_serial::Encodable;
use rand::{rngs::OsRng, Rng};

use crate::contract_imports::native_token::{
    DRKW_TOKEN_ID, FeeCallBuilder, FeeCallInput, FeeCallOutput,
    NATIVE_TOKEN_CONTRACT_ZKAS_FEE_V1_BIN,
};
use crate::walletdb::WalletPtr;
use crate::NATIVE_TOKEN_CONTRACT_ID;

/// Default network fee in DARK
const DEFAULT_FEE: u64 = 42_000_000;

/// Build fee call and finalize transaction
pub async fn build_fee_and_finalize_tx(
    wallet: &WalletPtr,
    call_leaf: ContractCallLeaf,
) -> Result<Transaction> {
    // Get DARK coin for fee
    let dark_token_id_str = format!("{:?}", DRKW_TOKEN_ID);
    let dark_coin_records = wallet.get_token_coins(&dark_token_id_str, false)
        .map_err(|e| Error::Custom(format!("Failed to get DARK coins: {:?}", e)))?;

    if dark_coin_records.is_empty() {
        return Err(Error::Custom(
            "No DARK coins available for fee payment. \
             The wallet needs DARK tokens to pay network fees.".to_string(),
        ));
    }

    // Use first DARK coin for fee
    let dark_coin = &dark_coin_records[0];
    let dark_secret_bytes = bs58::decode(&dark_coin.secret)
        .into_vec()
        .map_err(|e| Error::Custom(e.to_string()))?
        .try_into()
        .map_err(|_| Error::Custom("Invalid DARK secret key length".to_string()))?;
    let dark_secret = SecretKey::from_bytes(dark_secret_bytes)
        .map_err(|_| Error::Custom("Failed to parse DARK secret key".to_string()))?;

    // Get DARK Merkle proof
    let dark_merkle_proof = wallet.get_merkle_proof(&dark_coin.coin_id)
        .map_err(|e| Error::Custom(format!("Failed to get DARK Merkle proof: {:?}", e)))?;

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

    // Decode dark coin blind
    let dark_coin_blind_bytes = bs58::decode(&dark_coin.coin_blind)
        .into_vec()
        .map_err(|e| Error::Custom(e.to_string()))?
        .try_into()
        .map_err(|_| Error::Custom("Invalid coin blind length".to_string()))?;
    let dark_coin_blind = pallas::Base::from_repr(dark_coin_blind_bytes)
        .into_option()
        .ok_or_else(|| Error::Custom("Invalid coin blind".to_string()))?;

    // Load fee ZK binary and build fee proof
    let fee_zkbin = ZkBinary::decode(NATIVE_TOKEN_CONTRACT_ZKAS_FEE_V1_BIN, false)
        .map_err(|e| Error::Custom(format!("Failed to decode fee ZK binary: {:?}", e)))?;

    let fee_empty_wits = empty_witnesses(&fee_zkbin)?;
    let fee_circuit = ZkCircuit::new(fee_empty_wits, &fee_zkbin);
    let fee_pk = ProvingKey::build(0, &fee_circuit);

    // Build fee input
    let fee_input = FeeCallInput {
        value: dark_coin.value,
        token_id: DRKW_TOKEN_ID,
        spend_hook: pallas::Base::zero(),
        user_data: pallas::Base::zero(),
        coin_blind: dark_coin_blind,
        leaf_position: dark_coin.leaf_position,
        merkle_path: dark_merkle_path,
        secret: dark_secret,
        signature_secret: dark_secret,
    };

    // Fee output - change goes back to our public key
    let dark_public_key = PublicKey::from_secret(dark_secret);
    let change_blind = BaseBlind::random(&mut OsRng);
    let fee_output = FeeCallOutput {
        recipient: dark_public_key,
        value: dark_coin.value - DEFAULT_FEE,
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

    // Fee leaf has no proofs
    let fee_leaf = ContractCallLeaf { call: fee_call, proofs: vec![] };

    // Build final transaction
    let mut tx_builder = TransactionBuilder::new(call_leaf, vec![])
        .map_err(|e| Error::Custom(format!("Failed to create transaction builder: {:?}", e)))?;

    tx_builder.append(fee_leaf, vec![])
        .map_err(|e| Error::Custom(format!("Failed to append fee call: {:?}", e)))?;

    let tx = tx_builder.build()
        .map_err(|e| Error::Custom(format!("Failed to build transaction: {:?}", e)))?;

    Ok(tx)
}