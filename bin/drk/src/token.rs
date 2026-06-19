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

//! Token module - Promissory Note token management
//!
//! This module handles token creation and management using Promissory Note.

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
        poseidon_hash, BaseBlind, MerkleNode, PublicKey, SecretKey,
    },
    pasta::pallas,
    tx::ContractCall as SdkContractCall,
};
use dwow_serial::AsyncEncodable;
use rand::rngs::OsRng;

use crate::contract_imports::{
    promissory_note::{
        BALANCE_BASE10_DECIMALS, PromissoryNoteFunction, TokenId,
        TokenMintCallInput as MoneyTokenMintCallInput,
        MintCallInput,
    },
    native_token::{
        DRKW_TOKEN_ID, FeeCallBuilder, FeeCallInput, FeeCallOutput,
        NATIVE_TOKEN_CONTRACT_ZKAS_FEE_V1_BIN,
    },
    PROMISSORY_NOTE_CONTRACT_ID, NATIVE_TOKEN_CONTRACT_ID,
};
use crate::Dww;

use crate::fee_builder::DEFAULT_FEE;

impl Dww {
    /// Import a token mint authority into the wallet.
    ///
    /// In Promissory Note, token IDs are derived as:
    /// token_id = poseidon_hash(mint_authority, token_user_data, token_blind)
    ///
    /// This function stores the mint authority and derives the token ID.
    pub async fn import_mint_authority(
        &self,
        mint_authority: SecretKey,
        token_blind: dwow_sdk::crypto::BaseBlind,
    ) -> Result<TokenId> {
        // Derive token_id = poseidon_hash(mint_authority_public, token_blind)
        // The mint_authority is the secret; we need its "public" representation
        // which in Promissory Note is poseidon_hash(secret)
        let mint_authority_public = poseidon_hash([mint_authority.inner()]);

        // For token creation, token_user_data is typically empty (zero)
        let token_user_data = pallas::Base::zero();

        // Derive token_id = poseidon_hash(mint_authority_public, token_user_data, token_blind)
        let token_id = poseidon_hash([
            mint_authority_public,
            token_user_data,
            token_blind.inner(),
        ]);

        use crate::walletdb::TokenInfo;
        self.wallet.insert_token(&TokenInfo {
            token_id: bs58::encode(token_id.to_repr()).into_string(),
            name: None,
            symbol: None,
            decimals: BALANCE_BASE10_DECIMALS as u8,
            mint_authority: Some(bs58::encode(mint_authority.inner().to_repr()).into_string()),
            token_blind: bs58::encode(token_blind.inner().to_repr()).into_string(),
            is_frozen: false,
            freeze_height: None,
            created_at_height: 0,
        }).map_err(|e| Error::Custom(format!("{:?}", e)))?;

        Ok(token_id)
    }

    /// Create a new token using Promissory Note's TokenMintV1.
    ///
    /// This creates a new token type with an initial supply.
    /// The token ID is derived from the mint authority and token blind.
    pub async fn create_token(
        &self,
        name: String,
        supply: u64,
        decimals: u8,
    ) -> Result<Transaction> {
        // Generate mint authority (secret key) for this token
        let mint_authority = SecretKey::random(&mut OsRng);
        let mint_authority_public = poseidon_hash([mint_authority.inner()]);

        // Generate token blind
        let token_blind = BaseBlind::random(&mut OsRng);

        // Token user data (typically zero for basic tokens)
        let token_user_data = pallas::Base::zero();

        // Derive token_id = poseidon_hash(mint_authority_public, token_user_data, token_blind)
        let token_id = poseidon_hash([
            mint_authority_public,
            token_user_data,
            token_blind.inner(),
        ]);

        // Generate recipient (our own public key derived from mint_authority)
        let recipient = mint_authority_public;

        // Generate coin blind for initial coin
        let coin_blind = BaseBlind::random(&mut OsRng);

        // Decode supply amount
        let mint_amount = decode_base10(&supply.to_string(), BALANCE_BASE10_DECIMALS, false)?;

        // =========================================================================
        // Build TokenMintV1 via PromissoryNoteClient — ZK knowledge in contract crate
        let token_mint_input = MoneyTokenMintCallInput {
            token_auth_parent: mint_authority_public,
            token_user_data,
            token_blind: token_blind.inner(),
            recipient,
            value: mint_amount,
            spend_hook: pallas::Base::zero(),
            user_data: pallas::Base::zero(),
            coin_blind: coin_blind.inner(),
        };

        let (pn_call_data, pn_proof_bytes) =
            dwow_promissory_note_contract::client::PromissoryNoteClient::build_token_mint(
                token_mint_input,
            )
            .await
            .map_err(|e| Error::Custom(format!("Failed to build TokenMint: {}", e)))?;

        let function = PromissoryNoteFunction::TokenMintV1 as u8;
        let mut call_data = vec![function];
        call_data.extend_from_slice(&pn_call_data);

        let money_contract_id = *PROMISSORY_NOTE_CONTRACT_ID;

        let money_call = SdkContractCall {
            contract_id: money_contract_id,
            data: call_data,
        };

        // =========================================================================
        // Build fee call (NativeToken FeeV1)
        // =========================================================================
        // Get DRKW coin for fee payment
        let dark_token_id_str = bs58::encode(DRKW_TOKEN_ID.to_repr()).into_string();
        let drkw_cap_records = self.wallet.get_capabilities_for_token(&dark_token_id_str, Some(false))
            .map_err(|e| Error::Custom(format!("Failed to get DRKW capabilities: {:?}", e)))?;

        if drkw_cap_records.is_empty() {
            return Err(Error::Custom(
                "No DRKW capabilities available for fee payment. \
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

        let drkw_cap_blind = {
            let bytes: [u8; 32] = bs58::decode(&drkw_cap.cap_blind)
                .into_vec()
                .map_err(|e| Error::Custom(e.to_string()))?
                .try_into()
                .map_err(|_| Error::Custom("Invalid coin blind length".to_string()))?;
            pallas::Base::from_repr(bytes)
                .into_option()
                .ok_or_else(|| Error::Custom("Invalid field element".to_string()))?
        };

        // Load fee ZK binary
        let fee_zkbin = ZkBinary::decode(NATIVE_TOKEN_CONTRACT_ZKAS_FEE_V1_BIN, false)
            .map_err(|e| Error::Custom(format!("Failed to decode fee ZK binary: {:?}", e)))?;

        // Create fee proving key
        let fee_empty_wits = empty_witnesses(&fee_zkbin)?;
        let fee_circuit = ZkCircuit::new(fee_empty_wits, &fee_zkbin);
        let fee_pk = ProvingKey::build(0, &fee_circuit);

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
        // Combine into final transaction
        // =========================================================================
        let mut fee_call_data = vec![0x00u8]; // FeeV1 function code
        fee_debris.params.encode_async(&mut fee_call_data).await
            .map_err(|e| Error::Custom(format!("Failed to encode fee params: {:?}", e)))?;

        let native_token_id = *NATIVE_TOKEN_CONTRACT_ID;
        let fee_call = SdkContractCall {
            contract_id: native_token_id,
            data: fee_call_data,
        };

        // Combine all proofs
        let mut all_proofs: Vec<dwow_core::zk::Proof> =
            pn_proof_bytes.into_iter().map(|b| dwow_core::zk::Proof::new(b)).collect();
        all_proofs.extend(fee_debris.proofs);

        // Build PromissoryNote call leaf
        let money_leaf = ContractCallLeaf {
            call: money_call,
            proofs: all_proofs,
        };

        // Build fee call leaf
        let fee_leaf = ContractCallLeaf {
            call: fee_call,
            proofs: vec![],
        };

        // Build final transaction
        let mut tx_builder = TransactionBuilder::new(money_leaf, vec![])
            .map_err(|e| Error::Custom(format!("Failed to create transaction builder: {:?}", e)))?;

        tx_builder.append(fee_leaf, vec![])
            .map_err(|e| Error::Custom(format!("Failed to append fee call: {:?}", e)))?;

        let tx = tx_builder.build()
            .map_err(|e| Error::Custom(format!("Failed to build transaction: {:?}", e)))?;

        use crate::walletdb::TokenInfo;
        self.wallet.insert_token(&TokenInfo {
            token_id: bs58::encode(token_id.to_repr()).into_string(),
            name: Some(name),
            symbol: None,
            decimals,
            mint_authority: Some(bs58::encode(mint_authority.inner().to_repr()).into_string()),
            token_blind: bs58::encode(token_blind.inner().to_repr()).into_string(),
            is_frozen: false,
            freeze_height: None,
            created_at_height: 0,
        }).map_err(|e| Error::Custom(format!("{:?}", e)))?;

        Ok(tx)
    }

    /// Mint tokens of an existing token type.
    ///
    /// Requires the mint authority to be imported into the wallet.
    /// The mint_authority must correspond to the token_id (i.e., it was used when creating the token).
    pub async fn mint_tokens(
        &self,
        token_id: TokenId,
        amount: &str,
        mint_authority: SecretKey,
        token_leaf_pos: u64,
        token_path: Vec<MerkleNode>,
        recipient: Option<PublicKey>,
    ) -> Result<Transaction> {
        // Decode mint amount
        let mint_amount = decode_base10(amount, BALANCE_BASE10_DECIMALS, false)?;

        // Derive mint public key from authority
        let _mint_public = poseidon_hash([mint_authority.inner()]);

        // Default recipient is the mint authority's public key
        let recipient_pk = recipient.unwrap_or_else(|| PublicKey::from_secret(mint_authority));

        // =========================================================================
        // Build MintV1 via PromissoryNoteClient — ZK knowledge in contract crate
        // =========================================================================
        let coin_blind = BaseBlind::random(&mut OsRng);
        let recipient_base = poseidon_hash([recipient_pk.x()]);

        let mint_input = MintCallInput {
            mint_secret: mint_authority.inner(),
            token_leaf_pos: token_leaf_pos as u32,
            token_path: token_path.clone(),
            recipient: recipient_base,
            value: mint_amount,
            token_id,
            spend_hook: pallas::Base::zero(),
            user_data: pallas::Base::zero(),
            coin_blind: coin_blind.inner(),
        };

        let (mint_pn_data, mint_pn_proofs) =
            dwow_promissory_note_contract::client::PromissoryNoteClient::build_mint(
                mint_input,
            )
            .await
            .map_err(|e| Error::Custom(format!("Failed to build Mint: {}", e)))?;

        let money_contract_id = *PROMISSORY_NOTE_CONTRACT_ID;

        let mint_function = PromissoryNoteFunction::MintV1 as u8;
        let mut mint_call_data = vec![mint_function];
        mint_call_data.extend_from_slice(&mint_pn_data);

        let mint_call = SdkContractCall {
            contract_id: money_contract_id,
            data: mint_call_data,
        };

        // =========================================================================
        // Build fee call (NativeToken FeeV1)
        // =========================================================================
        let dark_token_id_str = bs58::encode(DRKW_TOKEN_ID.to_repr()).into_string();
        let drkw_cap_records = self.wallet.get_capabilities_for_token(&dark_token_id_str, Some(false))
            .map_err(|e| Error::Custom(format!("Failed to get DRKW capabilities: {:?}", e)))?;

        if drkw_cap_records.is_empty() {
            return Err(Error::Custom(
                "No DRKW capabilities available for fee payment. \
                 The wallet needs DRKW tokens to pay network fees.".to_string(),
            ));
        }

        let drkw_cap = &drkw_cap_records[0];
        let dark_secret_bytes = bs58::decode(&drkw_cap.secret)
            .into_vec()
            .map_err(|e| Error::Custom(e.to_string()))?
            .try_into()
            .map_err(|_| Error::Custom("Invalid DRKW secret key length".to_string()))?;
        let dark_secret = SecretKey::from_bytes(dark_secret_bytes)
            .map_err(|_| Error::Custom("Failed to parse DRKW secret key".to_string()))?;

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

        let drkw_cap_blind = {
            let bytes: [u8; 32] = bs58::decode(&drkw_cap.cap_blind)
                .into_vec()
                .map_err(|e| Error::Custom(e.to_string()))?
                .try_into()
                .map_err(|_| Error::Custom("Invalid coin blind length".to_string()))?;
            pallas::Base::from_repr(bytes)
                .into_option()
                .ok_or_else(|| Error::Custom("Invalid field element".to_string()))?
        };

        let fee_zkbin = ZkBinary::decode(NATIVE_TOKEN_CONTRACT_ZKAS_FEE_V1_BIN, false)
            .map_err(|e| Error::Custom(format!("Failed to decode fee ZK binary: {:?}", e)))?;

        let fee_empty_wits = empty_witnesses(&fee_zkbin)?;
        let fee_circuit = ZkCircuit::new(fee_empty_wits, &fee_zkbin);
        let fee_pk = ProvingKey::build(0, &fee_circuit);

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

        let dark_public_key = PublicKey::from_secret(dark_secret);
        let change_blind = BaseBlind::random(&mut OsRng);
        let fee_output = FeeCallOutput {
            recipient: dark_public_key,
            value: drkw_cap.value - DEFAULT_FEE,
            spend_hook: pallas::Base::zero(),
            user_data: pallas::Base::zero(),
            coin_blind: change_blind.inner(),
        };

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
        // Combine into final transaction
        // =========================================================================
        let mut fee_call_data = vec![0x00u8];
        fee_debris.params.encode_async(&mut fee_call_data).await
            .map_err(|e| Error::Custom(format!("Failed to encode fee params: {:?}", e)))?;

        let native_token_id = *NATIVE_TOKEN_CONTRACT_ID;
        let fee_call = SdkContractCall {
            contract_id: native_token_id,
            data: fee_call_data,
        };

        // Combine all proofs from all calls
        let mut all_proofs: Vec<dwow_core::zk::Proof> =
            mint_pn_proofs.into_iter().map(|b| dwow_core::zk::Proof::new(b)).collect();
        all_proofs.extend(fee_debris.proofs);

        // Mint call data
        let combined_call_data = mint_call.data.clone();

        // Build PromissoryNote call leaf with combined calls and all proofs
        let money_leaf = ContractCallLeaf {
            call: SdkContractCall {
                contract_id: money_contract_id,
                data: combined_call_data,
            },
            proofs: all_proofs,
        };

        // Build fee call leaf
        let fee_leaf = ContractCallLeaf {
            call: fee_call,
            proofs: vec![],
        };

        // Build final transaction
        let mut tx_builder = TransactionBuilder::new(money_leaf, vec![])
            .map_err(|e| Error::Custom(format!("Failed to create transaction builder: {:?}", e)))?;

        tx_builder.append(fee_leaf, vec![])
            .map_err(|e| Error::Custom(format!("Failed to append fee call: {:?}", e)))?;

        let tx = tx_builder.build()
            .map_err(|e| Error::Custom(format!("Failed to build transaction: {:?}", e)))?;

        Ok(tx)
    }
}
