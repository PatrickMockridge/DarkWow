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

//! Swap module — Promissory Note atomic OTC swap (OtcSwapV1)
//!
//! OtcSwapV1 atomically swaps coins between two parties. Each party burns their
//! input coin and receives the counterparty's output coin in a single transaction.
//!
//! The swap is initiated by one party creating a `PartialSwapData` offer, which
//! is exchanged out-of-band. The counterparty joins by providing their own coin
//! info, and the transaction is built combining both sides.
//!
//! For single-wallet cross-token swaps, use `atomic_swap()` directly.

use dwow_core::{
    tx::{ContractCallLeaf, Transaction},
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
    tx::ContractCall,
};
use dwow_serial::AsyncEncodable;
use rand::rngs::OsRng;

use crate::contract_imports::{
    promissory_note::{
        PromissoryNoteFunction, TokenId, BALANCE_BASE10_DECIMALS,
        TransferCallInput, TransferCallOutput,
    },
    PROMISSORY_NOTE_CONTRACT_ID,
};
use crate::transfer::decode_bs58_field;
use crate::Dww;

/// Half of an OTC swap — one party's contribution.
#[derive(Debug, Clone)]
pub struct PartialSwapData {
    /// Our coin ID (base58-encoded) — for wallet lookup
    pub cap_id: String,
    /// Our coin's value
    pub value: u64,
    /// Our coin's token ID
    pub token_id: String,
    /// What we want to receive: token ID
    pub receive_token_id: String,
    /// What we want to receive: value
    pub receive_value: u64,
    /// Our public key (recipient of the swap output)
    pub recipient: String,
}

impl PartialSwapData {
    /// Serialize to JSON for out-of-band exchange.
    pub fn to_json(&self) -> String {
        format!(
            r#"{{"cap_id":"{}","value":{},"token_id":"{}","receive_token_id":"{}","receive_value":{},"recipient":"{}"}}"#,
            self.cap_id, self.value, self.token_id, self.receive_token_id, self.receive_value, self.recipient
        )
    }

    /// Deserialize from JSON.
    pub fn from_json(s: &str) -> Result<Self> {
        use serde_json::{self, Value};
        let v: Value = serde_json::from_str(s)
            .map_err(|e| Error::Custom(e.to_string()))?;

        let cap_id = v["cap_id"].as_str()
            .ok_or_else(|| Error::Custom("missing cap_id".to_string()))?
            .to_string();
        let value = v["value"].as_u64()
            .ok_or_else(|| Error::Custom("missing value".to_string()))?;
        let token_id = v["token_id"].as_str()
            .ok_or_else(|| Error::Custom("missing token_id".to_string()))?
            .to_string();
        let receive_token_id = v["receive_token_id"].as_str()
            .ok_or_else(|| Error::Custom("missing receive_token_id".to_string()))?
            .to_string();
        let receive_value = v["receive_value"].as_u64()
            .ok_or_else(|| Error::Custom("missing receive_value".to_string()))?;
        let recipient = v["recipient"].as_str()
            .ok_or_else(|| Error::Custom("missing recipient".to_string()))?
            .to_string();

        Ok(PartialSwapData {
            cap_id, value, token_id, receive_token_id, receive_value, recipient,
        })
    }

    /// Return a human-readable summary of this swap half.
    pub fn summary(&self) -> String {
        format!(
            "Send {} of token {} → Receive {} of token {} (recipient: {})",
            self.value, &self.token_id[..8],
            self.receive_value, &self.receive_token_id[..8],
            &self.recipient[..12]
        )
    }
}

impl Dww {
    /// Initialize a swap offer — creates [`PartialSwapData`] for out-of-band exchange.
    ///
    /// The caller specifies their coin and what they want in return.
    /// The resulting `PartialSwapData` is serialized to JSON and shared with the
    /// counterparty.
    pub async fn init_swap(
        &self,
        amount: &str,
        token_id: TokenId,
        receive_amount: &str,
        receive_token_id: TokenId,
    ) -> Result<PartialSwapData> {
        let transfer_amount = decode_base10(amount, BALANCE_BASE10_DECIMALS, false)?;
        let receive_amount_val =
            decode_base10(receive_amount, BALANCE_BASE10_DECIMALS, false)?;

        let token_id_str = bs58::encode(token_id.to_repr()).into_string();
        let receive_token_id_str = bs58::encode(receive_token_id.to_repr()).into_string();

        // Find a coin with enough value
        let coin_records = self.wallet.get_capabilities_for_token(&token_id_str, Some(false))
            .map_err(|e| Error::Custom(format!("Failed to get capabilities: {:?}", e)))?;

        let input_cap = coin_records.iter()
            .find(|c| c.value >= transfer_amount)
            .ok_or_else(|| Error::Custom(format!(
                "No capability with sufficient balance for {} of {}",
                transfer_amount, &token_id_str[..8]
            )))?;

        // Get our public key
        let addresses = self.wallet.get_addresses()
            .map_err(|e| Error::Custom(format!("Failed to get addresses: {:?}", e)))?;
        let our_pubkey = addresses.first()
            .map(|a| a.public_key.clone())
            .ok_or_else(|| Error::Custom("No wallet addresses found".to_string()))?;

        Ok(PartialSwapData {
            cap_id: input_cap.cap_id.clone(),
            value: transfer_amount,
            token_id: token_id_str,
            receive_token_id: receive_token_id_str,
            receive_value: receive_amount_val,
            recipient: our_pubkey,
        })
    }

    /// Complete an OTC swap given both parties' swap data.
    ///
    /// This builds the full OtcSwapV1 transaction combining both sides:
    /// - Our input coin is burned, producing output for the counterparty
    /// - Counterparty's input is expected to be burned by them
    ///
    /// For a true P2P swap, the counterparty must separately create their proofs.
    /// This implementation supports single-wallet cross-token swaps where both
    /// coins belong to the same wallet.
    pub async fn join_swap(
        &self,
        our_swap: &PartialSwapData,
        their_swap: &PartialSwapData,
    ) -> Result<Transaction> {
        // Get our coin details
        let our_coin_records = self.wallet.get_capabilities_for_token(&our_swap.token_id, Some(false))
            .map_err(|e| Error::Custom(format!("Failed to get our coins: {:?}", e)))?;

        let our_cap = our_coin_records.iter()
            .find(|c| c.cap_id == our_swap.cap_id)
            .ok_or_else(|| Error::Custom("Our swap coin not found in wallet".to_string()))?;

        let our_secret = self.load_coin_secret(our_cap)?;
        let our_merkle_path = self.load_merkle_path(our_cap)?;
        let our_coin_blind = decode_bs58_field(&our_cap.cap_blind)?;

        let our_spend_hook = match &our_cap.spend_hook {
            Some(s) => decode_bs58_field(s)?,
            None => pallas::Base::zero(),
        };
        let our_user_data = match &our_cap.user_data {
            Some(s) => decode_bs58_field(s)?,
            None => pallas::Base::zero(),
        };

        // Get their coin details (also in our wallet for single-wallet swap)
        let their_coin_records = self.wallet.get_capabilities_for_token(&their_swap.token_id, Some(false))
            .map_err(|e| Error::Custom(format!("Failed to get their coins: {:?}", e)))?;

        let their_cap = their_coin_records.iter()
            .find(|c| c.cap_id == their_swap.cap_id)
            .ok_or_else(|| Error::Custom("Counterparty coin not found in wallet — \
                for P2P swaps both coins must be in the same wallet".to_string()))?;

        let their_secret = self.load_coin_secret(their_cap)?;
        let their_merkle_path = self.load_merkle_path(their_cap)?;
        let their_coin_blind = decode_bs58_field(&their_cap.cap_blind)?;

        let their_spend_hook = match &their_cap.spend_hook {
            Some(s) => decode_bs58_field(s)?,
            None => pallas::Base::zero(),
        };
        let their_user_data = match &their_cap.user_data {
            Some(s) => decode_bs58_field(s)?,
            None => pallas::Base::zero(),
        };

        // Build inputs: our coin + their coin
        let our_input = TransferCallInput {
            value: our_cap.value,
            token_id: decode_bs58_field(&our_swap.token_id)?,
            spend_hook: our_spend_hook,
            user_data: our_user_data,
            coin_blind: our_coin_blind,
            leaf_position: our_cap.leaf_position,
            merkle_path: our_merkle_path,
            secret: our_secret.inner(),
            ephemeral_signature_secret: SecretKey::random(&mut OsRng).inner(),
        };

        let their_input = TransferCallInput {
            value: their_cap.value,
            token_id: decode_bs58_field(&their_swap.token_id)?,
            spend_hook: their_spend_hook,
            user_data: their_user_data,
            coin_blind: their_coin_blind,
            leaf_position: their_cap.leaf_position,
            merkle_path: their_merkle_path,
            secret: their_secret.inner(),
            ephemeral_signature_secret: SecretKey::random(&mut OsRng).inner(),
        };

        // Build outputs: our coin → their recipient, their coin → our recipient
        let their_recipient_pub = PublicKey::from_bytes(
            bs58::decode(&their_swap.recipient).into_vec()
                .map_err(|e| Error::Custom(e.to_string()))?
                .try_into()
                .map_err(|_| Error::Custom("invalid recipient pubkey".to_string()))?,
        ).map_err(|_| Error::Custom("invalid recipient public key".to_string()))?;

        let our_recipient_pub = PublicKey::from_bytes(
            bs58::decode(&our_swap.recipient).into_vec()
                .map_err(|e| Error::Custom(e.to_string()))?
                .try_into()
                .map_err(|_| Error::Custom("invalid recipient pubkey".to_string()))?,
        ).map_err(|_| Error::Custom("invalid recipient public key".to_string()))?;

        let output_for_them = TransferCallOutput {
            recipient: poseidon_hash([their_recipient_pub.x()]),
            recipient_pub: their_recipient_pub,
            value: our_cap.value,
            token_id: decode_bs58_field(&our_swap.token_id)?,
            spend_hook: pallas::Base::zero(),
            user_data: pallas::Base::zero(),
            coin_blind: BaseBlind::random(&mut OsRng).inner(),
        };

        let output_for_us = TransferCallOutput {
            recipient: poseidon_hash([our_recipient_pub.x()]),
            recipient_pub: our_recipient_pub,
            value: their_cap.value,
            token_id: decode_bs58_field(&their_swap.token_id)?,
            spend_hook: pallas::Base::zero(),
            user_data: pallas::Base::zero(),
            coin_blind: BaseBlind::random(&mut OsRng).inner(),
        };

        // Build swap via PromissoryNoteClient (same proof structure as TransferV1)
        let (pn_call_data, pn_proof_bytes) =
            dwow_promissory_note_contract::client::PromissoryNoteClient::build_transfer(
                vec![our_input, their_input],
                vec![output_for_them, output_for_us],
            )
            .await
            .map_err(|e| Error::Custom(format!("Failed to build swap: {}", e)))?;

        let function = PromissoryNoteFunction::OtcSwapV1 as u8;
        let mut call_data = vec![function];
        call_data.extend_from_slice(&pn_call_data);

        let pn_cid = *PROMISSORY_NOTE_CONTRACT_ID;

        let swap_proofs: Vec<dwow_core::zk::Proof> =
            pn_proof_bytes.into_iter().map(|b| dwow_core::zk::Proof::new(b)).collect();
        let swap_call = ContractCall { contract_id: pn_cid, data: call_data };
        let swap_leaf = ContractCallLeaf { call: swap_call, proofs: swap_proofs };

        // Attach fee
        crate::fee_builder::build_fee_and_finalize_tx(&self.wallet, swap_leaf)
    }

    /// Inspect a swap offer — print its details.
    pub fn inspect_swap(&self, swap_data: &str) -> Result<()> {
        let swap = PartialSwapData::from_json(swap_data)?;
        println!("=== Swap Offer ===");
        println!("Send:     {} of token {}", swap.value, &swap.token_id[..8]);
        println!("Receive:  {} of token {}", swap.receive_value, &swap.receive_token_id[..8]);
        println!("Cap ID:  {}", &swap.cap_id[..16]);
        println!("Recipient: {}", &swap.recipient[..12]);
        Ok(())
    }

    /// Sign a swap — create our signed side of a swap offer.
    ///
    /// This is the same as [`init_swap`] but with explicit values rather
    /// than deriving them from wallet state. Useful for re-signing or
    /// counter-signing an existing proposal.
    pub async fn sign_swap(
        &self,
        cap_id: String,
        value: u64,
        token_id: TokenId,
        receive_value: u64,
        receive_token_id: TokenId,
    ) -> Result<PartialSwapData> {
        let token_id_str = bs58::encode(token_id.to_repr()).into_string();
        let receive_token_id_str = bs58::encode(receive_token_id.to_repr()).into_string();

        let addresses = self.wallet.get_addresses()
            .map_err(|e| Error::Custom(format!("Failed to get addresses: {:?}", e)))?;
        let our_pubkey = addresses.first()
            .map(|a| a.public_key.clone())
            .ok_or_else(|| Error::Custom("No wallet addresses found".to_string()))?;

        Ok(PartialSwapData {
            cap_id,
            value,
            token_id: token_id_str,
            receive_token_id: receive_token_id_str,
            receive_value,
            recipient: our_pubkey,
        })
    }

    /// Cross-token atomic swap — swap two of your own coins in a single transaction.
    ///
    /// Convenience wrapper around [`join_swap`] that creates the swap data
    /// for both sides. Both coins must be in the same wallet.
    pub async fn atomic_swap(
        &self,
        our_swap: PartialSwapData,
        their_swap: PartialSwapData,
    ) -> Result<Transaction> {
        self.join_swap(&our_swap, &their_swap).await
    }

    // ── Helpers ─────────────────────────────────────────────────────────

    /// Load the secret key for a coin record.
    fn load_coin_secret(&self, coin: &crate::walletdb::CapRecord) -> Result<SecretKey> {
        let secret_bytes = bs58::decode(&coin.secret)
            .into_vec()
            .map_err(|e| Error::Custom(e.to_string()))?
            .try_into()
            .map_err(|_| Error::Custom("Invalid secret key length".to_string()))?;
        SecretKey::from_bytes(secret_bytes)
            .map_err(|_| Error::Custom("Failed to parse secret key".to_string()))
    }

    /// Load the Merkle path for a coin record.
    fn load_merkle_path(&self, coin: &crate::walletdb::CapRecord) -> Result<Vec<MerkleNode>> {
        let merkle_proof = self.wallet.get_merkle_proof(&coin.cap_id)
            .map_err(|e| Error::Custom(format!("Failed to get Merkle proof: {:?}", e)))?;

        merkle_proof
            .siblings
            .iter()
            .map(|s| {
                let bytes: [u8; 32] = bs58::decode(s)
                    .into_vec()
                    .map_err(|e| Error::Custom(e.to_string()))?
                    .try_into()
                    .map_err(|_| Error::Custom("Invalid Merkle node length".to_string()))?;
                MerkleNode::from_bytes(bytes)
                    .ok_or_else(|| Error::Custom("Invalid Merkle node".to_string()))
            })
            .collect::<Result<Vec<_>>>()
    }
}