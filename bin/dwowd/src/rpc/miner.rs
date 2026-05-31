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

//! Miner RPC methods for local development.
//!
//! WARNING: These methods are ONLY available in localnet mode and should
//! NEVER be deployed to mainnet or testnet.

use std::{collections::HashMap, sync::atomic::Ordering};

use dwow_core::{
    rpc::jsonrpc::{
        ErrorCode::InternalError,
        JsonError, JsonResponse, JsonResult,
    },
};
use tinyjson::JsonValue;
use tracing::{error, info};

use dwow_chain::caribina::anchor_block;
use crate::error::{server_error, RpcError};
use crate::{proto::linear_broadcast::broadcast_block, DwowNode};

impl DwowNode {
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
        if !self.sync_complete.load(Ordering::SeqCst) {
            return server_error(RpcError::NodeNotSynced, id, None);
        }

        let params = match params.get::<Vec<JsonValue>>() {
            Some(v) => v,
            None => return JsonError::new(InternalError, None, id).into(),
        };

        if params.len() != 2 || !params[0].is_string() || !params[1].is_number() {
            return JsonError::new(InternalError, Some("Expected [recipient, value]".to_string()), id).into()
        }

        let recipient = params[0].get::<String>().unwrap();
        let reward_value = *params[1].get::<f64>().unwrap() as u64;

        info!(target: "dwowd::rpc::miner", "miner.mine_linear called for recipient {} with reward {}", recipient, reward_value);

        // Check that we're in darkwow-devnet mode (linear_blockchain is set)
        let linear_blockchain = match &self.linear_blockchain {
            Some(lb) => lb.clone(),
            None => {
                error!(target: "dwowd::rpc::miner", "miner.mine_linear is only available in darkwow-devnet mode");
                return JsonError::new(
                    InternalError,
                    Some("miner.mine_linear is only available in darkwow-devnet mode".to_string()),
                    id,
                )
                .into();
            }
        };

        // Decode recipient address (bs58 encoded public key)
        let recipient_bytes = match bs58::decode(recipient).with_check(None).into_vec() {
            Ok(v) => v,
            Err(_) => {
                error!(target: "dwowd::rpc::miner", "Invalid recipient base58");
                return JsonError::new(
                    InternalError,
                    Some("Invalid recipient address".to_string()),
                    id,
                )
                .into()
            }
        };

        // DarkWow address format: [prefix(1)][public_key(32)][checksum(4)] = 37 bytes
        if recipient_bytes.len() != 37 {
            error!(
                target: "dwowd::rpc::miner",
                "Invalid address length: {}",
                recipient_bytes.len()
            );
            return JsonError::new(InternalError, Some("Invalid address length".to_string()), id)
                .into()
        }

        // Extract public key from address (bytes 1-32)
        let public_key_bytes: [u8; 32] = recipient_bytes[1..33].try_into().unwrap();
        use dwow_sdk::crypto::PublicKey;
        let public_key = match PublicKey::from_bytes(public_key_bytes) {
            Ok(pk) => pk,
            Err(_) => {
                error!(target: "dwowd::rpc::miner", "Invalid public key in address");
                return JsonError::new(InternalError, Some("Invalid public key".to_string()), id)
                    .into()
            }
        };

        // Get latest block info
        let latest_block = match linear_blockchain.get_latest_block() {
            Ok(block) => block,
            Err(e) => {
                error!(target: "dwowd::rpc::miner", "Failed to get latest block: {}", e);
                return JsonError::new(InternalError, Some(format!("Failed to get latest block: {}", e)), id)
                    .into()
            }
        };

        let height = latest_block.header.height + 1;
        // Hash the PREVIOUS block with its own stored RandomX key, not the
        // new height's key. RandomX is a keyed hash — wrong key = garbage hash
        // that will be rejected as InvalidPreviousHash on apply.
        let previous_vm = linear_blockchain.get_vm(latest_block.header.randomx_key);
        let previous = latest_block.hash_with_vm(&previous_vm);
        // VM for the NEW block's PoW — keyed to the new height.
        let randomx_key = dwow_chain::Miner::derive_key_from_height(height);
        let vm = linear_blockchain.get_vm(randomx_key);
        let target = linear_blockchain.consensus.lock().unwrap().target();
        info!(target: "dwowd::rpc::miner",
            "Mining block at height {} (target={}, previous={})",
            height, target, previous);

        // Lazily initialize ZK proving materials for coinbase privacy
        let linear_zk = {
            let mut zk_lock = self.linear_zk.lock().await;
            if zk_lock.is_none() {
                match crate::registry::model::LinearPowRewardZk::new(
                    linear_blockchain.clone(),
                )
                .await
                {
                    Ok(zk) => *zk_lock = Some(zk),
                    Err(e) => {
                        error!(target: "dwowd::rpc::miner", "Failed to init linear ZK: {}", e);
                        return JsonError::new(
                            InternalError,
                            Some(format!("Failed to init linear ZK: {}", e)),
                            id,
                        )
                        .into()
                    }
                }
            }
            zk_lock.clone()
        };

        // Build ZK coinbase transaction with AEAD-encrypted note
        let (coinbase, _public_inputs) = match crate::registry::model::build_linear_coinbase(
            public_key,
            reward_value,
            linear_zk.as_ref().unwrap(),
        )
        .await
        {
            Ok(cb) => cb,
            Err(e) => {
                error!(target: "dwowd::rpc::miner", "Failed to build ZK coinbase: {}", e);
                return JsonError::new(
                    InternalError,
                    Some(format!("Failed to build ZK coinbase: {}", e)),
                    id,
                )
                .into()
            }
        };

        // Create coinbase transaction with ZK privacy data
        let coinbase_tx = dwow_chain::Transaction {
            version: 1,
            inputs: vec![],
            outputs: vec![],
            contract_calls: vec![],
            lock_time: height,
            coinbase: Some(coinbase),
        };

        // Get transactions from mempool
        let mempool_txs = match &self.mempool {
            Some(mp) => mp.take_all().await,
            None => vec![],
        };

        // Combine coinbase with mempool transactions
        let mut all_txs = mempool_txs.clone();
        all_txs.push(coinbase_tx);

        // Create miner and mine a block
        let consensus = dwow_chain::PoWConsensus::new(60, target, 1, u32::MAX);
        let miner = dwow_chain::Miner::new(std::sync::Arc::new(consensus));

        let mut mined_block = match miner.mine(&vm, previous, height, all_txs, target) {
            Ok(block) => block,
            Err(e) => {
                error!(target: "dwowd::rpc::miner", "Mining failed: {}", e);
                // Re-insert transactions on mining failure
                if let Some(ref mp) = self.mempool {
                    for tx in mempool_txs {
                        let _ = mp.add(tx).await;
                    }
                }
                return JsonError::new(InternalError, Some(format!("Mining failed: {}", e)), id)
                    .into()
            }
        };

        // Anchor the block to Arweave via Caribina (best-effort, configurable)
        let fc = &linear_blockchain.finality_config;
        if fc.should_anchor() {
            let block_hash = mined_block.hash_with_vm(&vm);
            let mut block_hash_bytes = [0u8; 32];
            block_hash_bytes.copy_from_slice(block_hash.as_bytes());
            match anchor_block(&block_hash_bytes, mined_block.header.timestamp, height) {
                Some(tx_id) => {
                    mined_block.header.anchor_tx_id = tx_id;
                    mined_block.header.finality_flags = fc.mine_flags();
                    info!(target: "dwowd::rpc::miner", "Anchored block {} to Arweave", block_hash);
                }
                None => {
                    info!(target: "dwowd::rpc::miner", "Arweave anchor skipped (network/turbo unavailable)");
                }
            }
        }

        let block_hash = format!("{}", mined_block.hash_with_vm(&vm));

        // Apply the mined block to the blockchain
        info!(target: "dwowd::rpc::miner",
            "Block {} mined (nonce={}), applying to chain...", block_hash, mined_block.header.nonce);
        match linear_blockchain.apply_block(&mined_block).await {
            Ok(()) => {
                info!(target: "dwowd::rpc::miner", "Mined and applied block {} at height {}", block_hash, height);
            }
            Err(e) => {
                error!(target: "dwowd::rpc::miner",
                    "Failed to apply mined block {} at height {}: {}",
                    block_hash, height, e);
                // Re-insert transactions on apply failure
                if let Some(ref mp) = self.mempool {
                    for tx in mempool_txs {
                        let _ = mp.add(tx).await;
                    }
                }
                return JsonError::new(InternalError, Some(format!("Failed to apply block: {}", e)), id)
                    .into()
            }
        }

        // Broadcast the mined block to peers
        broadcast_block(&self.p2p_handler.p2p, mined_block).await;

        // Return block hash
        let result = JsonValue::from(HashMap::from([
            ("block_hash".to_string(), JsonValue::String(block_hash)),
            ("height".to_string(), JsonValue::Number(height as f64)),
        ]));
        JsonResponse::new(result, id).into()
    }
}
