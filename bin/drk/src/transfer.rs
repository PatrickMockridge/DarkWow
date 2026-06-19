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

//! Transfer module - Promissory Note API
//!
//! This module handles token transfers using the Promissory Note contract.

use dwow_core::{
    tx::{ContractCallLeaf, Transaction, TransactionBuilder},
    util::parse::decode_base10,
    zk::{proof::ProvingKey, vm::ZkCircuit, vm_heap::empty_witnesses},
    zkas::ZkBinary,
    Error, Result,
};
use dwow_sdk::{
    crypto::{
        pasta_prelude::PrimeField,
        poseidon_hash, BaseBlind, ContractId, MerkleNode, PublicKey, SecretKey,
    },
    dark_tree::DarkTree,
    pasta::pallas,
    tx::ContractCall,
};
use dwow_serial::AsyncEncodable;
use rand::rngs::OsRng;

use crate::contract_imports::{
    promissory_note::{
        BALANCE_BASE10_DECIMALS, PromissoryNoteFunction, TokenId,
        TransferCallInput as MoneyTransferCallInput, TransferCallOutput as MoneyTransferCallOutput,
    },
    native_token::{
        DRKW_TOKEN_ID, FeeCallBuilder, FeeCallInput, FeeCallOutput,
        NATIVE_TOKEN_CONTRACT_ZKAS_FEE_V1_BIN,
    },
    PROMISSORY_NOTE_CONTRACT_ID, NATIVE_TOKEN_CONTRACT_ID,
};
use crate::Dww;

// ============================================================================
// SPEND HOOK HELPER
// ============================================================================

/// Create a child ContractCall for a spend_hook if spend_hook is non-zero.
///
/// When a transfer output coin has a spend_hook, after the transfer completes,
/// a child call is made to that contract with the user_data as parameters.
///
/// `hook_func_code` allows the caller to specify which function the hook
/// contract should dispatch to. Defaults to 0x00 (generic callback).
fn create_spend_hook_call(
    spend_hook: pallas::Base,
    user_data: pallas::Base,
    hook_func_code: Option<u8>,
) -> Option<ContractCall> {
    if spend_hook == pallas::Base::zero() {
        return None;
    }

    let hook_contract_id = ContractId::from(spend_hook);
    let func_code = hook_func_code.unwrap_or(0x00);

    let mut data = vec![func_code];
    data.extend_from_slice(&user_data.to_repr());

    Some(ContractCall { contract_id: hook_contract_id, data })
}

use crate::fee_builder::DEFAULT_FEE;

/// Helper to decode a bs58-encoded base field element
pub(crate) fn decode_bs58_field(s: &str) -> Result<pallas::Base> {
    let bytes = bs58::decode(s)
        .into_vec()
        .map_err(|e| Error::Custom(e.to_string()))?
        .try_into()
        .map_err(|_| Error::Custom("Invalid field encoding length".to_string()))?;
    pallas::Base::from_repr(bytes)
        .into_option()
        .ok_or_else(|| Error::Custom("Invalid field element".to_string()))
}

impl Dww {
    /// Create a payment transaction using Promissory Note TransferV1 with fee attachment.
    ///
    /// Returns the transaction object on success.
    ///
    /// This implements the full transfer flow:
    /// 1. Select token coin for the transfer
    /// 2. Build PromissoryNote TransferV1 proof (burn + mint)
    /// 3. Select DRKW coin for fee payment
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
        let coin_records = self.wallet.get_capabilities_for_token(&token_id_str, Some(false))
            .map_err(|e| Error::Custom(format!("Failed to get capabilities: {:?}", e)))?;

        if coin_records.is_empty() {
            return Err(Error::Custom(format!(
                "Did not find any retained capabilities with token ID: {:?}",
                token_id
            )));
        }

        // Find a coin with enough value
        let input_cap = coin_records.iter().find(|c| c.value >= transfer_amount);
        let input_cap_record = match input_cap {
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
        let secret_bytes = bs58::decode(&input_cap_record.secret)
            .into_vec()
            .map_err(|e| Error::Custom(e.to_string()))?
            .try_into()
            .map_err(|_| Error::Custom("Invalid secret key length".to_string()))?;
        let secret = SecretKey::from_bytes(secret_bytes)
            .map_err(|_| Error::Custom("Failed to parse secret key".to_string()))?;

        // Get Merkle proof from wallet
        let merkle_proof = self.wallet.get_merkle_proof(&input_cap_record.cap_id)
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
        let coin_blind = decode_bs58_field(&input_cap_record.cap_blind)?;

        // Parse spend_hook and user_data from coin record
        let spend_hook_in = match input_cap_record.spend_hook {
            Some(ref s) => decode_bs58_field(s)?,
            None => pallas::Base::zero(),
        };

        let user_data_in = match input_cap_record.user_data {
            Some(ref s) => decode_bs58_field(s)?,
            None => pallas::Base::zero(),
        };

        // =========================================================================
        // Step 2: Build PromissoryNote TransferV1
        // =========================================================================
        // Build TransferCallInput
        let input = MoneyTransferCallInput {
            value: input_cap_record.value,
            token_id,
            spend_hook: spend_hook_in,
            user_data: user_data_in,
            coin_blind,
            leaf_position: input_cap_record.leaf_position,
            merkle_path,
            secret: secret.inner(),
            ephemeral_signature_secret: SecretKey::random(&mut OsRng).inner(),
        };

        // Get spend_hook and user_data for output
        let spend_hook_out = spend_hook.unwrap_or(pallas::Base::zero());
        let user_data_out = user_data.unwrap_or(pallas::Base::zero());

        // Calculate change
        let change_value = input_cap_record.value - transfer_amount;

        // The recipient address in Promissory Note is poseidon_hash(secret_key) as a field element
        let recipient_address = poseidon_hash([recipient.x()]);

        // Generate random blind for output coin
        let output_coin_blind = BaseBlind::random(&mut OsRng);

        // Build output
        let output = MoneyTransferCallOutput {
            recipient: recipient_address,
            recipient_pub: recipient,
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
                    recipient_pub: PublicKey::from_secret(secret),
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

        // Build transfer via PromissoryNoteClient — ZK knowledge lives in the contract crate
        let (pn_call_data, pn_proof_bytes) =
            dwow_promissory_note_contract::client::PromissoryNoteClient::build_transfer(
                vec![input], outputs,
            )
            .await
            .map_err(|e| Error::Custom(format!("Failed to build transfer: {}", e)))?;

        let mut all_proofs: Vec<dwow_core::zk::Proof> =
            pn_proof_bytes.into_iter().map(|b| dwow_core::zk::Proof::new(b)).collect();

        // Prepend function code byte
        let function = PromissoryNoteFunction::TransferV1 as u8;
        let mut call_data = vec![function];
        call_data.extend_from_slice(&pn_call_data);

        let money_contract_id = *PROMISSORY_NOTE_CONTRACT_ID;

        let money_call = ContractCall {
            contract_id: money_contract_id,
            data: call_data,
        };

        // =========================================================================
        // Step 3: Get DRKW coin for fee payment
        // =========================================================================
        let dark_token_id_str = bs58::encode(DRKW_TOKEN_ID.to_repr()).into_string();
        let drkw_cap_records = self.wallet.get_capabilities_for_token(&dark_token_id_str, Some(false))
            .map_err(|e| Error::Custom(format!("Failed to get DRKW coins: {:?}", e)))?;

        // If no DRKW coin, we can't pay fee - return error
        if drkw_cap_records.is_empty() {
            return Err(Error::Custom(
                "No DRKW coins available for fee payment. \
                 The wallet needs DRKW tokens to pay network fees.".to_string(),
            ));
        }

        // Use the first DRKW coin for fee
        let drkw_cap = &drkw_cap_records[0];
        let dark_secret_bytes = bs58::decode(&drkw_cap.secret)
            .into_vec()
            .map_err(|e| Error::Custom(e.to_string()))?
            .try_into()
            .map_err(|_| Error::Custom("Invalid DRKW secret key length".to_string()))?;
        let dark_secret = SecretKey::from_bytes(dark_secret_bytes)
            .map_err(|_| Error::Custom("Failed to parse DRKW secret key".to_string()))?;

        // Get DRKW Merkle proof
        let dark_merkle_proof = self.wallet.get_merkle_proof(&drkw_cap.cap_id)
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
                Ok(MerkleNode::from_bytes(bytes).ok_or_else(|| Error::Custom("Invalid Merkle node".to_string()))?)
            })
            .collect::<Result<Vec<_>>>()?;

        let drkw_cap_blind = decode_bs58_field(&drkw_cap.cap_blind)?;

        // =========================================================================
        // Step 4: Build NativeToken FeeV1
        // =========================================================================
        // Load fee ZK binary
        let fee_zkbin = ZkBinary::decode(NATIVE_TOKEN_CONTRACT_ZKAS_FEE_V1_BIN, false)
            .map_err(|e| Error::Custom(format!("Failed to decode fee ZK binary: {:?}", e)))?;

        // Create fee proving key
        let fee_empty_wits = empty_witnesses(&fee_zkbin)?;
        let fee_circuit = ZkCircuit::new(fee_empty_wits, &fee_zkbin);
        let fee_pk = ProvingKey::build(fee_zkbin.k, &fee_circuit);

        // Build fee input
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
        };

        // Fee output - change goes back to our public key
        let dark_public_key = PublicKey::from_secret(dark_secret);
        let change_blind = BaseBlind::random(&mut OsRng);
        let fee_output = FeeCallOutput {
            recipient: dark_public_key,
            value: drkw_cap.value - DEFAULT_FEE,
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

        // Build PromissoryNote call leaf
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
        let child_tree = if let Some(hook_call) = create_spend_hook_call(spend_hook_out, user_data_out, None) {
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