/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

//! Transfer module - Money V3 API
//!
//! This module handles token transfers using the Money V3 contract.

use darkfi::{
    tx::{ContractCallLeaf, Transaction, TransactionBuilder},
    util::parse::decode_base10,
    zk::{proof::ProvingKey, vm::ZkCircuit, vm_heap::empty_witnesses},
    zkas::ZkBinary,
    Error, Result,
};
use darkfi_sdk::{
    crypto::{
        pasta_prelude::PrimeField,
        poseidon_hash, BaseBlind, ContractId, MerkleNode, PublicKey, SecretKey,
    },
    dark_tree::DarkTree,
    pasta::pallas,
    tx::ContractCall,
};
use darkfi_serial::AsyncEncodable;
use rand::rngs::OsRng;

use crate::contract_imports::{
    money::{
        BALANCE_BASE10_DECIMALS, MoneyV3Function, TokenId,
        MONEY_V3_CONTRACT_ZKAS_BURN_V1_BIN, MONEY_V3_CONTRACT_ZKAS_MINT_V1_BIN,
        TransferCallBuilder as MoneyTransferCallBuilder,
        TransferCallInput as MoneyTransferCallInput, TransferCallOutput as MoneyTransferCallOutput,
    },
    native_token::{
        DARK_TOKEN_ID, FeeCallBuilder, FeeCallInput, FeeCallOutput,
        NATIVE_TOKEN_CONTRACT_ZKAS_FEE_V1_BIN,
    },
    MONEY_V3_CONTRACT_ID, NATIVE_TOKEN_CONTRACT_ID,
};
use crate::Drk;

// ============================================================================
// SPEND HOOK HELPER
// ============================================================================

/// Create a child ContractCall for a spend_hook if spend_hook is non-zero.
///
/// When a transfer output coin has a spend_hook, after the transfer completes,
/// a child call is made to that contract with the user_data as parameters.
fn create_spend_hook_call(
    spend_hook: pallas::Base,
    user_data: pallas::Base,
) -> Option<ContractCall> {
    if spend_hook == pallas::Base::zero() {
        return None;
    }

    let hook_contract_id = ContractId::from(spend_hook);

    // Function code 0x00 is generic - the spend_hook contract interprets
    // the params based on its own function signatures
    let mut data = vec![0x00u8];
    data.extend_from_slice(&user_data.to_repr());

    Some(ContractCall { contract_id: hook_contract_id, data })
}

/// Default network fee in DARK
const DEFAULT_FEE: u64 = 42_000_000;

/// Helper to decode a bs58-encoded base field element
fn decode_bs58_field(s: &str) -> Result<pallas::Base> {
    let bytes = bs58::decode(s)
        .into_vec()
        .map_err(|e| Error::Custom(e.to_string()))?
        .try_into()
        .map_err(|_| Error::Custom("Invalid field encoding length".to_string()))?;
    pallas::Base::from_repr(bytes)
        .into_option()
        .ok_or_else(|| Error::Custom("Invalid field element".to_string()))
}

impl Drk {
    /// Create a payment transaction using Money V3 TransferV1 with fee attachment.
    ///
    /// Returns the transaction object on success.
    ///
    /// This implements the full transfer flow:
    /// 1. Select token coin for the transfer
    /// 2. Build MoneyV3 TransferV1 proof (burn + mint)
    /// 3. Select DARK coin for fee payment
    /// 4. Build NativeToken FeeV1 proof
    /// 5. Combine into final transaction
    pub async fn transfer(
        &self,
        amount: &str,
        token_id: TokenId,
        recipient: PublicKey,
        spend_hook: Option<pallas::Base>,
        user_data: Option<pallas::Base>,
        half_split: bool,
    ) -> Result<Transaction> {
        // Decode the transfer amount
        let transfer_amount = decode_base10(amount, BALANCE_BASE10_DECIMALS, false)?;

        // Get token_id string for database lookup (bs58 encoded to match wallet storage)
        let token_id_str = bs58::encode(token_id.to_repr()).into_string();

        // =========================================================================
        // Step 1: Get token coin for the transfer
        // =========================================================================
        let coin_records = self.wallet.get_token_coins(&token_id_str, false)
            .map_err(|e| Error::Custom(format!("Failed to get coins: {:?}", e)))?;

        if coin_records.is_empty() {
            return Err(Error::Custom(format!(
                "Did not find any unspent coins with token ID: {:?}",
                token_id
            )));
        }

        // Find a coin with enough value
        let input_coin = coin_records.iter().find(|c| c.value >= transfer_amount);
        let input_coin_record = match input_coin {
            Some(coin) => coin,
            None => {
                return Err(Error::Custom(format!(
                    "Insufficient funds: needed {}, got {}",
                    transfer_amount,
                    coin_records.iter().map(|c| c.value).max().unwrap_or(0)
                )))
            }
        };

        // Get secret for this coin
        let secret_bytes = bs58::decode(&input_coin_record.secret)
            .into_vec()
            .map_err(|e| Error::Custom(e.to_string()))?
            .try_into()
            .map_err(|_| Error::Custom("Invalid secret key length".to_string()))?;
        let secret = SecretKey::from_bytes(secret_bytes)
            .map_err(|_| Error::Custom("Failed to parse secret key".to_string()))?;

        // Get Merkle proof from wallet
        let merkle_proof = self.wallet.get_merkle_proof(&input_coin_record.coin_id)
            .map_err(|e| Error::Custom(format!("Failed to get Merkle proof: {:?}", e)))?;

        // Convert Merkle proof siblings to MerkleNode
        let merkle_path: Vec<MerkleNode> = merkle_proof
            .siblings
            .iter()
            .map(|s| {
                let bytes: [u8; 32] = bs58::decode(s)
                    .into_vec()
                    .map_err(|e| Error::Custom(e.to_string()))?
                    .try_into()
                    .map_err(|_| Error::Custom("Invalid Merkle node length".to_string()))?;
                Ok(MerkleNode::from_bytes(bytes).ok_or_else(|| Error::Custom("Invalid Merkle node".to_string()))?)
            })
            .collect::<Result<Vec<_>>>()?;

        // Parse coin blind
        let coin_blind = decode_bs58_field(&input_coin_record.coin_blind)?;

        // Parse spend_hook and user_data from coin record
        let spend_hook_in = match input_coin_record.spend_hook {
            Some(ref s) => decode_bs58_field(s)?,
            None => pallas::Base::zero(),
        };

        let user_data_in = match input_coin_record.user_data {
            Some(ref s) => decode_bs58_field(s)?,
            None => pallas::Base::zero(),
        };

        // =========================================================================
        // Step 2: Build MoneyV3 TransferV1
        // =========================================================================
        // Build TransferCallInput
        let input = MoneyTransferCallInput {
            value: input_coin_record.value,
            token_id,
            spend_hook: spend_hook_in,
            user_data: user_data_in,
            coin_blind,
            leaf_position: input_coin_record.leaf_position,
            merkle_path,
            secret: secret.inner(),
            signature_secret: secret.inner(),
        };

        // Get spend_hook and user_data for output
        let spend_hook_out = spend_hook.unwrap_or(pallas::Base::zero());
        let user_data_out = user_data.unwrap_or(pallas::Base::zero());

        // Calculate change
        let change_value = input_coin_record.value - transfer_amount;

        // The recipient address in Money V3 is poseidon_hash(secret_key) as a field element
        let recipient_address = poseidon_hash([recipient.x()]);

        // Generate random blind for output coin
        let output_coin_blind = BaseBlind::random(&mut OsRng);

        // Build output
        let output = MoneyTransferCallOutput {
            recipient: recipient_address,
            value: transfer_amount,
            token_id,
            spend_hook: spend_hook_out,
            user_data: user_data_out,
            coin_blind: output_coin_blind.inner(),
        };

        // Create change output if there's change and half_split is false
        let outputs = if change_value > 0 && !half_split {
            let change_coin_blind = BaseBlind::random(&mut OsRng);
            vec![
                output,
                MoneyTransferCallOutput {
                    recipient: poseidon_hash([secret.inner()]),
                    value: change_value,
                    token_id,
                    spend_hook: pallas::Base::zero(),
                    user_data: pallas::Base::zero(),
                    coin_blind: change_coin_blind.inner(),
                },
            ]
        } else {
            vec![output]
        };

        // Load MoneyV3 ZK circuits
        let burn_zkbin = ZkBinary::decode(MONEY_V3_CONTRACT_ZKAS_BURN_V1_BIN, false)
            .map_err(|e| Error::Custom(format!("Failed to decode burn ZK binary: {:?}", e)))?;
        let mint_zkbin = ZkBinary::decode(MONEY_V3_CONTRACT_ZKAS_MINT_V1_BIN, false)
            .map_err(|e| Error::Custom(format!("Failed to decode mint ZK binary: {:?}", e)))?;

        // Create MoneyV3 proving keys
        let empty_wits = empty_witnesses(&burn_zkbin)?;
        let burn_circuit = ZkCircuit::new(empty_wits.clone(), &burn_zkbin);
        let burn_pk = ProvingKey::build(0, &burn_circuit);

        let mint_circuit = ZkCircuit::new(empty_wits, &mint_zkbin);
        let mint_pk = ProvingKey::build(0, &mint_circuit);

        // Build transfer call
        let builder = MoneyTransferCallBuilder {
            inputs: vec![input],
            outputs,
            burn_zkbin,
            burn_pk,
            mint_zkbin,
            mint_pk,
        };

        let debris = builder.build()
            .map_err(|e| Error::Custom(format!("Failed to build transfer: {:?}", e)))?;

        // Create MoneyV3 contract call
        let function = MoneyV3Function::TransferV1 as u8;
        let mut call_data = vec![function];
        debris.params.encode_async(&mut call_data).await
            .map_err(|e| Error::Custom(format!("Failed to encode params: {:?}", e)))?;

        let money_contract_id = MONEY_V3_CONTRACT_ID.get()
            .copied()
            .ok_or_else(|| Error::Custom("Money V3 contract ID not initialized".to_string()))?;

        let money_call = ContractCall {
            contract_id: money_contract_id,
            data: call_data,
        };

        // Collect proofs from transfer
        let mut all_proofs = debris.proofs;

        // =========================================================================
        // Step 3: Get DARK coin for fee payment
        // =========================================================================
        let dark_token_id_str = format!("{:?}", DARK_TOKEN_ID);
        let dark_coin_records = self.wallet.get_token_coins(&dark_token_id_str, false)
            .map_err(|e| Error::Custom(format!("Failed to get DARK coins: {:?}", e)))?;

        // If no DARK coin, we can't pay fee - return error
        if dark_coin_records.is_empty() {
            return Err(Error::Custom(
                "No DARK coins available for fee payment. \
                 The wallet needs DARK tokens to pay network fees.".to_string(),
            ));
        }

        // Use the first DARK coin for fee
        let dark_coin = &dark_coin_records[0];
        let dark_secret_bytes = bs58::decode(&dark_coin.secret)
            .into_vec()
            .map_err(|e| Error::Custom(e.to_string()))?
            .try_into()
            .map_err(|_| Error::Custom("Invalid DARK secret key length".to_string()))?;
        let dark_secret = SecretKey::from_bytes(dark_secret_bytes)
            .map_err(|_| Error::Custom("Failed to parse DARK secret key".to_string()))?;

        // Get DARK Merkle proof
        let dark_merkle_proof = self.wallet.get_merkle_proof(&dark_coin.coin_id)
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
                Ok(MerkleNode::from_bytes(bytes).ok_or_else(|| Error::Custom("Invalid Merkle node".to_string()))?)
            })
            .collect::<Result<Vec<_>>>()?;

        let dark_coin_blind = decode_bs58_field(&dark_coin.coin_blind)?;

        // =========================================================================
        // Step 4: Build NativeToken FeeV1
        // =========================================================================
        // Load fee ZK binary
        let fee_zkbin = ZkBinary::decode(NATIVE_TOKEN_CONTRACT_ZKAS_FEE_V1_BIN, false)
            .map_err(|e| Error::Custom(format!("Failed to decode fee ZK binary: {:?}", e)))?;

        // Create fee proving key
        let fee_empty_wits = empty_witnesses(&fee_zkbin)?;
        let fee_circuit = ZkCircuit::new(fee_empty_wits, &fee_zkbin);
        let fee_pk = ProvingKey::build(0, &fee_circuit);

        // Build fee input
        let fee_input = FeeCallInput {
            value: dark_coin.value,
            token_id: DARK_TOKEN_ID,
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

        // =========================================================================
        // Step 5: Combine into final transaction
        // =========================================================================
        let native_token_id = *NATIVE_TOKEN_CONTRACT_ID;

        let mut fee_call_data = vec![0x00u8]; // FeeV1 function code
        fee_debris.params.encode_async(&mut fee_call_data).await
            .map_err(|e| Error::Custom(format!("Failed to encode fee params: {:?}", e)))?;

        let fee_call = ContractCall {
            contract_id: native_token_id,
            data: fee_call_data,
        };

        // Combine all proofs
        all_proofs.extend(fee_debris.proofs);

        // Build MoneyV3 call leaf
        let money_leaf = ContractCallLeaf {
            call: money_call,
            proofs: all_proofs,
        };

        // Build fee call leaf (no proofs - they're already combined)
        let fee_leaf = ContractCallLeaf {
            call: fee_call,
            proofs: vec![],
        };

        // Create spend_hook child call if spend_hook is set
        let child_tree = if let Some(hook_call) = create_spend_hook_call(spend_hook_out, user_data_out) {
            let hook_leaf = ContractCallLeaf { call: hook_call, proofs: vec![] };
            let tree = DarkTree::new(hook_leaf, vec![], None, None);
            vec![tree]
        } else {
            vec![]
        };

        // Build final transaction using TransactionBuilder
        // Pass child_tree as children of the money_leaf (the transfer call)
        let mut tx_builder = TransactionBuilder::new(money_leaf, child_tree)
            .map_err(|e| Error::Custom(format!("Failed to create transaction builder: {:?}", e)))?;

        // Fee call is a sibling, not a child (no children_indexes relationship)
        tx_builder.append(fee_leaf, vec![])
            .map_err(|e| Error::Custom(format!("Failed to append fee call: {:?}", e)))?;

        let tx = tx_builder.build()
            .map_err(|e| Error::Custom(format!("Failed to build transaction: {:?}", e)))?;

        Ok(tx)
    }
}