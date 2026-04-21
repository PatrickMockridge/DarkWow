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

//! Miner RPC methods for local development.
//!
//! WARNING: These methods are ONLY available in localnet mode and should
//! NEVER be deployed to mainnet or testnet.

use std::collections::HashMap;

use darkfi::{
    blockchain::BlockInfo,
    rpc::jsonrpc::{
        ErrorCode::InternalError,
        JsonError, JsonResponse, JsonResult,
    },
    tx::{ContractCallLeaf, TransactionBuilder},
    zk::{empty_witnesses, ZkCircuit, ProvingKey},
};
use darkfi_native_token_contract::{client::pow_reward_v1::PoWRewardCallBuilder, NativeTokenFunction};
use darkfi_sdk::{
    crypto::{keypair::Keypair, pasta_prelude::PrimeField, MerkleTree, NATIVE_TOKEN_CONTRACT_ID},
    tx::ContractCall,
};
use tinyjson::JsonValue;
use darkfi_serial::Encodable;
use rand::rngs::OsRng;
use tracing::{error, info};

use crate::{server_error, DarkfiNode, RpcError};

impl DarkfiNode {
    // RPCAPI:
    // Mine a block and send PoW reward to a recipient (LOCALNET ONLY).
    //
    // This is a development-only method that mines a single block
    // with a PoW reward to the specified address.
    //
    // --> {"jsonrpc": "2.0", "method": "miner.mine",
    //      "params": ["recipient_base58"], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "txHash...", "id": 1}
    pub async fn miner_mine(&self, id: u16, params: JsonValue) -> JsonResult {
        let params = match params.get::<Vec<JsonValue>>() {
            Some(v) => v,
            None => return JsonError::new(InternalError, None, id).into(),
        };

        if params.len() != 1 || !params[0].is_string() {
            return JsonError::new(InternalError, Some("Expected [recipient]".to_string()), id).into()
        }

        let recipient = params[0].get::<String>().unwrap();

        info!(target: "darkfid::rpc::miner", "miner.mine called for recipient {}", recipient);

        // Check that we're in localnet mode
        if !self.is_localnet() {
            error!(target: "darkfid::rpc::miner", "miner.mine is only available in localnet mode");
            return JsonError::new(
                InternalError,
                Some("miner.mine is only available in localnet mode".to_string()),
                id,
            )
            .into();
        }

        // Decode recipient address (bs58 encoded public key)
        let recipient_bytes = match bs58::decode(recipient).with_check(None).into_vec() {
            Ok(v) => v,
            Err(_) => {
                error!(target: "darkfid::rpc::miner", "Invalid recipient base58");
                return JsonError::new(
                    InternalError,
                    Some("Invalid recipient address".to_string()),
                    id,
                )
                .into()
            }
        };

        // DarkFi address format: [prefix(1)][public_key(32)][checksum(4)] = 37 bytes
        if recipient_bytes.len() != 37 {
            error!(
                target: "darkfid::rpc::miner",
                "Invalid address length: {}",
                recipient_bytes.len()
            );
            return JsonError::new(InternalError, Some("Invalid address length".to_string()), id)
                .into()
        }

        // NOTE: We validate the recipient address format but currently ignore it
        // and use recipient: None so the reward goes to the block signing keypair.
        // The block signing secret key is exported in the tx_hash response.

        // Get validator state
        let mut validator = self.validator.write().await;

        // Get ZK proving keys (needs validator immutable borrow)
        let zkbin = match validator.blockchain.contracts.get_zkas(
            &validator.blockchain.sled_db,
            &NATIVE_TOKEN_CONTRACT_ID,
            darkfi_native_token_contract::NATIVE_TOKEN_CONTRACT_ZKAS_MINT_NS_V1,
        ) {
            Ok(z) => z.0,
            Err(e) => {
                error!(target: "darkfid::rpc::miner", "Failed to get zkas: {}", e);
                return JsonError::new(
                    InternalError,
                    Some(format!("Failed to get zkas: {}", e)),
                    id,
                )
                .into()
            }
        };

        // Copy verify_fees before getting mutable fork
        let verify_fees = validator.verify_fees;

        // Get current fork (needs validator mutable borrow)
        let fork = match validator.consensus.forks.first_mut() {
            Some(f) => f,
            None => {
                error!(target: "darkfid::rpc::miner", "No fork available");
                return JsonError::new(InternalError, Some("No fork available".to_string()), id)
                    .into()
            }
        };

        let previous = match fork.overlay.lock().unwrap().last_block() {
            Ok(p) => p,
            Err(e) => {
                error!(target: "darkfid::rpc::miner", "Failed to get last block: {}", e);
                return JsonError::new(
                    InternalError,
                    Some(format!("Failed to get last block: {}", e)),
                    id,
                )
                .into()
            }
        };

        let block_height = previous.header.height + 1;

        let witnesses = match empty_witnesses(&zkbin) {
            Ok(w) => w,
            Err(e) => {
                error!(target: "darkfid::rpc::miner", "Failed to create circuit: {}", e);
                return JsonError::new(
                    InternalError,
                    Some(format!("Failed to create circuit: {}", e)),
                    id,
                )
                .into()
            }
        };
        let circuit = ZkCircuit::new(witnesses, &zkbin);

        let pk = ProvingKey::build(zkbin.k, &circuit);

        // Create a block signing keypair
        let block_signing_keypair = Keypair::random(&mut OsRng);

        // Build the PoWReward transaction
        // Note: recipient is None so the reward goes to the block signing keypair
        // The secret key will be exported so it can be imported to wallet
        let debris = match (PoWRewardCallBuilder {
            signature_keypair: block_signing_keypair,
            block_height,
            fees: 0,
            recipient: None,
            spend_hook: None,
            user_data: None,
            mint_zkbin: zkbin.clone(),
            mint_pk: pk.clone(),
        })
        .build()
        {
            Ok(d) => d,
            Err(e) => {
                error!(target: "darkfid::rpc::miner", "Failed to build PoWReward: {}", e);
                return JsonError::new(
                    InternalError,
                    Some(format!("Failed to build PoWReward: {}", e)),
                    id,
                )
                .into()
            }
        };

        let mut data = vec![NativeTokenFunction::PoWRewardV1 as u8];
        if let Err(e) = debris.params.encode(&mut data) {
            error!(target: "darkfid::rpc::miner", "Failed to encode params: {}", e);
            return JsonError::new(InternalError, Some(format!("Failed to encode: {}", e)), id)
                .into()
        }

        let call = ContractCall { contract_id: *NATIVE_TOKEN_CONTRACT_ID, data };

        let mut tx_builder = match TransactionBuilder::new(
            ContractCallLeaf { call, proofs: debris.proofs },
            vec![],
        ) {
            Ok(t) => t,
            Err(e) => {
                error!(target: "darkfid::rpc::miner", "Failed to create tx builder: {}", e);
                return JsonError::new(
                    InternalError,
                    Some(format!("Failed to create tx builder: {}", e)),
                    id,
                )
                .into()
            }
        };

        let mut tx = match tx_builder.build() {
            Ok(t) => t,
            Err(e) => {
                error!(target: "darkfid::rpc::miner", "Failed to build tx: {}", e);
                return JsonError::new(InternalError, Some(format!("Failed to build tx: {}", e)), id)
                    .into()
            }
        };

        let sigs = match tx.create_sigs(&[block_signing_keypair.secret]) {
            Ok(s) => s,
            Err(e) => {
                error!(target: "darkfid::rpc::miner", "Failed to create signatures: {}", e);
                return JsonError::new(
                    InternalError,
                    Some(format!("Failed to create signatures: {}", e)),
                    id,
                )
                .into()
            }
        };
        tx.signatures = vec![sigs];

        // Increment timestamp
        let timestamp = match previous.header.timestamp.checked_add(1.into()) {
            Ok(t) => t,
            Err(e) => {
                error!(target: "darkfid::rpc::miner", "Failed to increment timestamp: {}", e);
                return JsonError::new(
                    InternalError,
                    Some(format!("Failed to increment timestamp: {}", e)),
                    id,
                )
                .into()
            }
        };

        // Generate header
        let header =
            darkfi::blockchain::Header::new(previous.hash(), block_height, 0, timestamp);

        // Create block
        let mut block = BlockInfo::new_empty(header);
        block.append_txs(vec![tx.clone()]);

        // Apply the producer transaction
        let overlay = match fork.overlay.lock().unwrap().full_clone() {
            Ok(o) => o,
            Err(e) => {
                error!(target: "darkfid::rpc::miner", "Failed to clone overlay: {}", e);
                return JsonError::new(
                    InternalError,
                    Some(format!("Failed to clone overlay: {}", e)),
                    id,
                )
                .into()
            }
        };

        let _ = match darkfi::validator::verification::apply_producer_transaction(
            &overlay,
            block.header.height,
            fork.module.target,
            block.txs.last().unwrap(),
            &mut MerkleTree::new(1),
        )
        .await
        {
            Ok(o) => o,
            Err(e) => {
                error!(target: "darkfid::rpc::miner", "Failed to apply producer tx: {}", e);
                return JsonError::new(
                    InternalError,
                    Some(format!("Failed to apply producer tx: {}", e)),
                    id,
                )
                .into()
            }
        };

        let diff = match overlay.lock().unwrap().overlay.lock().unwrap().diff(&fork.diffs) {
            Ok(d) => d,
            Err(e) => {
                error!(target: "darkfid::rpc::miner", "Failed to get diff: {}", e);
                return JsonError::new(
                    InternalError,
                    Some(format!("Failed to get diff: {}", e)),
                    id,
                )
                .into()
            }
        };

        block.header.state_root = match overlay
            .lock()
            .unwrap()
            .contracts
            .update_state_monotree(&diff)
        {
            Ok(s) => s,
            Err(e) => {
                error!(target: "darkfid::rpc::miner", "Failed to update state monotree: {}", e);
                return JsonError::new(
                    InternalError,
                    Some(format!("Failed to update state monotree: {}", e)),
                    id,
                )
                .into()
            }
        };

        // Sign the block
        block.sign(&block_signing_keypair.secret);

        // Verify and append the block
        let proposal = darkfi::validator::consensus::Proposal::new(block.clone());

        // First verify the block
        if let Err(e) = darkfi::validator::verification::verify_block(
            &fork.overlay,
            &fork.diffs,
            &mut fork.module,
            &block,
            &previous,
            true,
            verify_fees,
            &block.zkbin_data,
        )
        .await
        {
            error!(target: "darkfid::rpc::miner", "Failed to verify block: {}", e);
            return JsonError::new(InternalError, Some(format!("Failed to verify block: {}", e)), id)
                .into()
        }

        // Append to fork
        if let Err(e) = fork.append_proposal(&proposal).await {
            error!(target: "darkfid::rpc::miner", "Failed to append proposal: {}", e);
            return JsonError::new(
                InternalError,
                Some(format!("Failed to append proposal: {}", e)),
                id,
            )
                .into()
        }

        let tx_hash = tx.hash().to_string();
        info!(target: "darkfid::rpc::miner", "Mined block with reward tx: {}", tx_hash);

        // Export the block signing secret key so wallet can spend the coins
        // The note is encrypted to the output public key, and the memo contains
        // the signing secret key. With the signing secret key, the wallet can
        // decrypt the note and access the coins.
        let secret_key_b58 = bs58::encode(block_signing_keypair.secret.inner().to_repr()).into_string();

        // Broadcast
        self.p2p_handler.p2p.broadcast(&tx).await;

        // Return tx hash and secret key for coin redemption
        let result = JsonValue::from(HashMap::from([
            ("tx_hash".to_string(), JsonValue::String(tx_hash)),
            ("secret_key".to_string(), JsonValue::String(secret_key_b58)),
        ]));
        JsonResponse::new(result, id).into()
    }

    // RPCAPI:
    // Mine a block on the linear blockchain (LINEAR-TESTNET ONLY).
    //
    // This is a development-only method that mines a single block
    // with a PoW reward to the specified address.
    //
    // --> {"jsonrpc": "2.0", "method": "miner.mine_linear",
    //      "params": ["recipient_base58", reward_value], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "blockHash...", "id": 1}
    pub async fn miner_mine_linear(&self, id: u16, params: JsonValue) -> JsonResult {
        let params = match params.get::<Vec<JsonValue>>() {
            Some(v) => v,
            None => return JsonError::new(InternalError, None, id).into(),
        };

        if params.len() != 2 || !params[0].is_string() || !params[1].is_number() {
            return JsonError::new(InternalError, Some("Expected [recipient, value]".to_string()), id).into()
        }

        let recipient = params[0].get::<String>().unwrap();
        let reward_value = *params[1].get::<f64>().unwrap() as u64;

        info!(target: "darkfid::rpc::miner", "miner.mine_linear called for recipient {} with reward {}", recipient, reward_value);

        // Check that we're in linear-testnet mode (linear_blockchain is set)
        let linear_blockchain = match &self.linear_blockchain {
            Some(lb) => lb.clone(),
            None => {
                error!(target: "darkfid::rpc::miner", "miner.mine_linear is only available in linear-testnet mode");
                return JsonError::new(
                    InternalError,
                    Some("miner.mine_linear is only available in linear-testnet mode".to_string()),
                    id,
                )
                .into();
            }
        };

        // Decode recipient address (bs58 encoded public key)
        let recipient_bytes = match bs58::decode(recipient).with_check(None).into_vec() {
            Ok(v) => v,
            Err(_) => {
                error!(target: "darkfid::rpc::miner", "Invalid recipient base58");
                return JsonError::new(
                    InternalError,
                    Some("Invalid recipient address".to_string()),
                    id,
                )
                .into()
            }
        };

        // DarkFi address format: [prefix(1)][public_key(32)][checksum(4)] = 37 bytes
        if recipient_bytes.len() != 37 {
            error!(
                target: "darkfid::rpc::miner",
                "Invalid address length: {}",
                recipient_bytes.len()
            );
            return JsonError::new(InternalError, Some("Invalid address length".to_string()), id)
                .into()
        }

        // Extract public key from address (bytes 1-32)
        let public_key_bytes: [u8; 32] = recipient_bytes[1..33].try_into().unwrap();
        use darkfi_sdk::crypto::PublicKey;
        let public_key = match PublicKey::from_bytes(public_key_bytes) {
            Ok(pk) => pk,
            Err(_) => {
                error!(target: "darkfid::rpc::miner", "Invalid public key in address");
                return JsonError::new(InternalError, Some("Invalid public key".to_string()), id)
                    .into()
            }
        };

        // Get latest block info
        let latest_block = match linear_blockchain.get_latest_block() {
            Ok(block) => block,
            Err(e) => {
                error!(target: "darkfid::rpc::miner", "Failed to get latest block: {}", e);
                return JsonError::new(InternalError, Some(format!("Failed to get latest block: {}", e)), id)
                    .into()
            }
        };

        let height = latest_block.header.height + 1;
        let previous = latest_block.hash();
        let difficulty_target = latest_block.header.difficulty_target;

        // Create coinbase output
        let coinbase_output = darkfi_linear::Output {
            value: reward_value,
            script: public_key.to_bytes().to_vec(),
        };

        // Create coinbase transaction (no inputs for coinbase)
        let coinbase_tx = darkfi_linear::Transaction {
            version: 1,
            inputs: vec![],
            outputs: vec![coinbase_output],
            lock_time: height,
        };

        // Create miner and mine a block
        let consensus = darkfi_linear::PoWConsensus::default();
        let miner = darkfi_linear::Miner::new(std::sync::Arc::new(consensus));

        let mined_block = match miner.mine(previous, height, vec![coinbase_tx], difficulty_target) {
            Ok(block) => block,
            Err(e) => {
                error!(target: "darkfid::rpc::miner", "Mining failed: {}", e);
                return JsonError::new(InternalError, Some(format!("Mining failed: {}", e)), id)
                    .into()
            }
        };

        let block_hash = format!("{}", mined_block.hash());

        // Apply the mined block to the blockchain
        match linear_blockchain.apply_block(&mined_block).await {
            Ok(()) => {
                info!(target: "darkfid::rpc::miner", "Mined and applied block {} at height {}", block_hash, height);
            }
            Err(e) => {
                error!(target: "darkfid::rpc::miner", "Failed to apply block: {}", e);
                return JsonError::new(InternalError, Some(format!("Failed to apply block: {}", e)), id)
                    .into()
            }
        }

        // Return block hash
        let result = JsonValue::from(HashMap::from([
            ("block_hash".to_string(), JsonValue::String(block_hash)),
            ("height".to_string(), JsonValue::Number(height as f64)),
        ]));
        JsonResponse::new(result, id).into()
    }
}
