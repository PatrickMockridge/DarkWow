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

use std::{
    collections::{HashMap, HashSet},
    sync::{
        atomic::Ordering,
        Arc,
    },
};

use async_trait::async_trait;
use smol::lock::MutexGuard;
use tinyjson::JsonValue;
use tracing::{debug, error, info};

use dwow::{
    rpc::{
        jsonrpc::{
            ErrorCode, ErrorCode::InvalidParams, JsonError, JsonRequest, JsonResponse, JsonResult,
            JsonSubscriber,
        },
        server::RequestHandler,
    },
    system::{Publisher, StoppableTaskPtr},
};

use dwow_linear::caribina::anchor_block;
use crate::{
    error::{miner_status_response, server_error, RpcError},
    registry::model::{LinearMinerRewardsRecipientConfig, MinerRewardsRecipientConfig},
    DarkfiNode,
};

// https://github.com/xmrig/xmrig-proxy/blob/master/doc/STRATUM.md
// https://github.com/xmrig/xmrig-proxy/blob/master/doc/STRATUM_EXT.md

/// JSON-RPC `RequestHandler` for Stratum
pub struct StratumRpcHandler;

#[async_trait]
#[rustfmt::skip]
impl RequestHandler<StratumRpcHandler> for DarkfiNode {
	async fn handle_request(&self, req: JsonRequest) -> JsonResult {
		debug!(target: "dwowd::rpc::stratum_rpc", "--> {}", req.stringify().unwrap());

		match req.method.as_str() {
			// ======================
			// Stratum mining methods
			// ======================
			"login" => self.stratum_login(req.id, req.params).await,
			"submit" => self.stratum_submit(req.id, req.params).await,
			"keepalived" => self.stratum_keepalived(req.id, req.params).await,
			_ => JsonError::new(ErrorCode::MethodNotFound, None, req.id).into(),
		}
	}

    async fn connections_mut(&self) -> MutexGuard<'life0, HashSet<StoppableTaskPtr>> {
        self.registry.stratum_rpc_connections.lock().await
    }
}

impl DarkfiNode {
    // RPCAPI:
    // Register a new mining client to the registry and generate a new
    // job.
    //
    // **Request:**
    // * `login` : A wallet address or its base-64 encoded mining configuration
    // * `pass`  : Unused client password field
    // * `agent` : Client agent description
    // * `algo`  : Client supported mining algorithms
    //
    // **Response:**
    // * `id`     : Registry client ID
    // * `job`    : The generated mining job
    // * `status` : Response status
    //
    // The generated mining job map consists of the following fields:
    // * `blob`      : The hex encoded block hashing blob of the job block
    // * `job_id`    : Registry mining job ID
    // * `height`    : The job block height
    // * `target`    : Current mining target
    // * `algo`      : The mining algorithm - RandomX
    // * `seed_hash` : Current RandomX key
    // * `next_seed_hash`: (optional) Next RandomX key if it is known
    //
    // --> {
    //       "jsonrpc": "2.0",
    //       "method": "login",
    //       "params": {
    //         "login": "WALLET_ADDRESS",
    //         "pass": "x",
    //         "agent": "XMRig",
    //         "algo": ["rx/0"]
    //       },
    //       "id": 1
    //     }
    // <-- {
    //       "jsonrpc": "2.0",
    //       "result": {
    //         "id": "unique_connection-id",
    //         "job": {
    //           "blob": "abcdef...001234",
    //           "job_id": "unique_job-id",
    //           "height": 1234,
    //           "target": "abcd1234",
    //           "algo": "rx/0",
    //           "seed_hash": "deadbeef...0234",
    //           "next_seed_hash": "c0fefe...1243"
    //         },
    //         "status": "OK"
    //       },
    //       "id": 1
    //     }
    pub async fn stratum_login(&self, id: u16, params: JsonValue) -> JsonResult {
        // Parse request params
        let Some(params) = params.get::<HashMap<String, JsonValue>>() else {
            return JsonError::new(InvalidParams, None, id).into()
        };

        // Parse login
        let Some(wallet) = params.get("login") else {
            return server_error(RpcError::MinerMissingLogin, id, None)
        };
        let Some(wallet) = wallet.get::<String>() else {
            return server_error(RpcError::MinerInvalidLogin, id, None)
        };

        // Parse password
        let Some(pass) = params.get("pass") else {
            return server_error(RpcError::MinerMissingPassword, id, None)
        };
        let Some(_pass) = pass.get::<String>() else {
            return server_error(RpcError::MinerInvalidPassword, id, None)
        };

        // Parse agent
        let Some(agent) = params.get("agent") else {
            return server_error(RpcError::MinerMissingAgent, id, None)
        };
        let Some(agent) = agent.get::<String>() else {
            return server_error(RpcError::MinerInvalidAgent, id, None)
        };

        // Parse algo
        let Some(algo) = params.get("algo") else {
            return server_error(RpcError::MinerMissingAlgo, id, None)
        };
        let Some(algo) = algo.get::<Vec<JsonValue>>() else {
            return server_error(RpcError::MinerInvalidAlgo, id, None)
        };

        // Iterate through `algo` to see if "rx/0" is supported.
        // rx/0 is RandomX.
        let mut found_rx0 = false;
        for i in algo {
            let Some(algo) = i.get::<String>() else {
                return server_error(RpcError::MinerInvalidAlgo, id, None)
            };
            if algo == "rx/0" {
                found_rx0 = true;
                break
            }
        }
        if !found_rx0 {
            return server_error(RpcError::MinerRandomXNotSupported, id, None)
        }

        // Register the new miner
        info!(
            target: "dwowd::rpc::rpc_stratum::stratum_login",
            "[RPC-STRATUM] Got login from {wallet} ({agent})",
        );

        // Linear-testnet path: uses LinearBlockchain, no DAG validator needed
        if let Some(linear_chain) = self.linear_blockchain.as_ref() {
            let linear_config = match LinearMinerRewardsRecipientConfig::from_str(wallet).await {
                Ok(c) => c,
                Err(e) => return server_error(e, id, None),
            };
            return self.stratum_login_linear(id, linear_chain, linear_config, agent.to_string()).await;
        }

        // DAG path: check if node is synced before responding
        let validator = self.validator.read().await;
        if !validator.synced {
            return JsonResponse::new(JsonValue::from(HashMap::new()), id).into()
        }

        let config =
            match MinerRewardsRecipientConfig::from_str(&self.registry.network, wallet).await {
                Ok(c) => c,
                Err(e) => return server_error(e, id, None),
            };

        let (client_id, job_id, job, publisher) = match self
            .registry
            .state
            .write()
            .await
            .register_miner(&validator, wallet, &config)
            .await
        {
            Ok(p) => p,
            Err(e) => {
                error!(
                    target: "dwowd::rpc::rpc_stratum::stratum_login",
                    "[RPC-STRATUM] Failed to register miner: {e}",
                );
                return JsonResponse::new(JsonValue::from(HashMap::new()), id).into()
            }
        };

        // Now we have the new job, we ship it to RPC
        info!(
            target: "dwowd::rpc::rpc_stratum::stratum_login",
            "[RPC-STRATUM] Created new mining job for client {client_id}: {job_id}"
        );
        let response = JsonValue::from(HashMap::from([
            ("id".to_string(), JsonValue::from(client_id)),
            ("job".to_string(), job),
            ("status".to_string(), JsonValue::from(String::from("OK"))),
        ]));
        (publisher, JsonResponse::new(response, id)).into()
    }

    /// Linear-testnet stratum login - uses LinearBlockchain instead of Validator
    async fn stratum_login_linear(
        &self,
        id: u16,
        linear_blockchain: &Arc<crate::blockchain::LinearBlockchain>,
        config: LinearMinerRewardsRecipientConfig,
        agent: String,
    ) -> JsonResult {
        use crate::registry::model::generate_linear_block_template;
        
        use std::time::{SystemTime, UNIX_EPOCH};

        // Generate a unique client ID based on timestamp and random bytes
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let client_id = format!("{}-{}", agent.replace("/", "-"), timestamp);

        // Lazily initialize ZK proving materials for linear coinbase
        let linear_zk = {
            let mut zk_lock = self.linear_zk.lock().await;
            if zk_lock.is_none() {
                match crate::registry::model::LinearPowRewardZk::new(
                    linear_blockchain.clone(),
                ).await {
                    Ok(zk) => *zk_lock = Some(zk),
                    Err(e) => {
                        tracing::warn!(
                            target: "dwowd::rpc::rpc_stratum::stratum_login_linear",
                            "[RPC-STRATUM] Failed to init ZK: {e}, using transparent coinbase",
                        );
                    }
                }
            }
            // Clone out of the lock
            zk_lock.clone()
        };

        // Generate block template using LinearBlockchain with ZK coinbase
        let template = match generate_linear_block_template(
            linear_blockchain, &config, linear_zk.as_ref(),
        ).await {
            Ok(t) => t,
            Err(e) => {
                error!(
                    target: "dwowd::rpc::rpc_stratum::stratum_login_linear",
                    "[RPC-STRATUM] Failed to generate linear block template: {e}",
                );
                return JsonResponse::new(JsonValue::from(HashMap::new()), id).into()
            }
        };

        // Store template for use in stratum_submit_linear
        *self.current_linear_template.lock().await = Some(template.clone());

        // Create or reuse a shared publisher for push notifications.
        // All stratum connections share one publisher so that when a block
        // is accepted, every connected miner gets the new job.
        let publisher = {
            let mut lock = self.linear_stratum_publisher.lock().await;
            if lock.is_none() {
                *lock = Some(Publisher::new());
            }
            lock.as_ref().unwrap().clone()
        };
        // Store recipient config for generating new block templates on submit
        *self.linear_recipient_config.lock().await = Some(config);

        // Create job ID
        let job_id = format!("linear-job-{}", template.height);

        // Get RandomX seed hash for this height
        let randomx_key = dwow_linear::Miner::derive_key_from_height(template.height);
        let seed_hash = hex::encode(randomx_key);

        // Build the mining blob using the standard compact header format.
        // This is the same format used by Block::hash() so the miner's hash
        // matches the block validation hash.
        // Create a temporary header with nonce=0 for the mining blob.
        let mining_header = dwow_linear::BlockHeader {
            version: 1,
            previous: blake3::Hash::from_bytes(template.previous),
            merkle_root: blake3::hash(&[]),
            timestamp: template.timestamp,
            difficulty_target: template.difficulty_target,
            nonce: 0, // placeholder - miner will find this
            height: template.height,
            uncle_merkle_root: [0u8; 32],
            total_reward: template.value,
            randomx_key,
            coin_merkle_root: template.coin_merkle_root,
            nullifier_root: template.nullifier_root,
            anchor_tx_id: [0u8; 32],
            anchor_monero_height: 0,
            anchor_monero_hash: [0u8; 32],
            finality_flags: 0,
        };
        let blob_data = mining_header.to_mining_blob();
        let blob = hex::encode(&blob_data);
        info!(
            target: "dwowd::rpc::rpc_stratum::stratum_login_linear",
            "[RPC-STRATUM] Login blob (to xmrig): {blob}",
        );

        // Target: u32 difficulty sent as 8 hex bytes (for stratum compatibility).
        // The consensus check compares u32::from_le_bytes(hash[0..4]) <= difficulty_target.
        let target = format!("{:016x}", template.difficulty_target as u64);

        info!(
            target: "dwowd::rpc::rpc_stratum::stratum_login_linear",
            "[RPC-STRATUM] Created linear mining job for client {client_id}: height={}, job_id={job_id}",
            template.height,
        );

        let response = JsonValue::from(HashMap::from([
            ("id".to_string(), JsonValue::from(client_id.clone())),
            ("job".to_string(), JsonValue::from(HashMap::from([
                ("blob".to_string(), JsonValue::from(blob)),
                ("job_id".to_string(), JsonValue::from(job_id)),
                ("height".to_string(), JsonValue::from(template.height as f64)),
                ("target".to_string(), JsonValue::from(target)),
                ("algo".to_string(), JsonValue::from(String::from("rx/0"))),
                ("seed_hash".to_string(), JsonValue::from(seed_hash)),
                ("reserved_offset".to_string(), JsonValue::from(40_f64)),
            ]))),
            ("status".to_string(), JsonValue::from(String::from("OK"))),
        ]));

        // Return SubscriberWithReply so the RPC server spawns a background task
        // that pushes JsonNotification messages to this client via the publisher.
        let subscriber = JsonSubscriber {
            method: "job",
            publisher,
        };
        (subscriber, JsonResponse::new(response, id)).into()
    }

    // RPCAPI:
    // Miner submits a job solution.
    //
    // **Request:**
    // * `id`     : Registry client ID
    // * `job_id` : Registry mining job ID
    // * `nonce`  : The hex encoded solution header nonce.
    // * `result` : RandomX calculated hash
    //
    // **Response:**
    // * `status`: Block submit status
    //
    // --> {
    //       "jsonrpc": "2.0",
    //       "method": "submit",
    //       "params": {
    //         "id": "unique_connection-id",
    //         "job_id": "unique_job-id",
    //         "nonce": "d0030040",
    //         "result": "e1364b8782719d7683e2ccd3d8f724bc59dfa780a9e960e7c0e0046acdb40100"
    //       },
    //       "id": 1
    //     }
    // <-- {"jsonrpc": "2.0", "result": {"status": "OK"}, "id": 1}
    pub async fn stratum_submit(&self, id: u16, params: JsonValue) -> JsonResult {
        // Check if this is a linear-testnet submission
        if self.linear_blockchain.is_some() {
            return self.stratum_submit_linear(id, params).await;
        }

        // Check if node is synced before responding
        let mut validator = self.validator.write().await;
        if !validator.synced {
            return miner_status_response(id, "rejected")
        }

        // Parse request params
        let Some(params) = params.get::<HashMap<String, JsonValue>>() else {
            return JsonError::new(InvalidParams, None, id).into()
        };

        // Parse client id
        let Some(client_id) = params.get("id") else {
            return server_error(RpcError::MinerMissingClientId, id, None)
        };
        let Some(client_id) = client_id.get::<String>() else {
            return server_error(RpcError::MinerInvalidClientId, id, None)
        };

        // If we don't know about this client, we can just abort here
        let mut registry = self.registry.state.write().await;
        let Some(client) = registry.jobs.get(client_id) else {
            return miner_status_response(id, "rejected")
        };

        // Parse job id
        let Some(job_id) = params.get("job_id") else {
            return server_error(RpcError::MinerMissingJobId, id, None)
        };
        let Some(job_id) = job_id.get::<String>() else {
            return server_error(RpcError::MinerInvalidJobId, id, None)
        };

        // If this job doesn't match the client one, we can just abort
        // here.
        if &client.job != job_id {
            return miner_status_response(id, "rejected")
        }
        let wallet = client.wallet.clone();

        // If this client job wallet template doesn't exist, we can
        // just abort here.
        let Some(block_template) = registry.block_templates.get(&wallet) else {
            return miner_status_response(id, "rejected")
        };

        // If this template has been already submitted, reject this
        // submission.
        if block_template.submitted {
            return miner_status_response(id, "rejected")
        }

        // Parse nonce
        let Some(nonce) = params.get("nonce") else {
            return server_error(RpcError::MinerMissingNonce, id, None)
        };
        let Some(nonce) = nonce.get::<String>() else {
            return server_error(RpcError::MinerInvalidNonce, id, None)
        };
        let Ok(nonce_bytes) = hex::decode(nonce) else {
            return server_error(RpcError::MinerInvalidNonce, id, None)
        };
        if nonce_bytes.len() != 4 {
            return server_error(RpcError::MinerInvalidNonce, id, None)
        }
        let nonce = u32::from_le_bytes(nonce_bytes.try_into().unwrap());

        // Parse result
        let Some(result) = params.get("result") else {
            return server_error(RpcError::MinerMissingResult, id, None)
        };
        let Some(_result) = result.get::<String>() else {
            return server_error(RpcError::MinerInvalidResult, id, None)
        };

        info!(
            target: "dwowd::rpc::rpc_stratum::stratum_submit",
            "[RPC-STRATUM] Got solution submission from client {client_id} for job: {job_id}",
        );

        // Update the block nonce and sign it
        let mut block = block_template.block.clone();
        block.header.nonce = nonce;
        block.sign(&block_template.secret);

        // Keep the template in memory so we can safely refernce the
        // registry.
        let mut block_template = block_template.clone();

        // Submit the new block through the registry
        if let Err(e) =
            registry.submit(&mut validator, &self.subscribers, &self.p2p_handler, block).await
        {
            error!(
                target: "dwowd::rpc::rpc_stratum::stratum_submit",
                "[RPC-STRATUM] Error submitting new block: {e}",
            );

            // Try to refresh the jobs before returning error
            if let Err(e) = registry.refresh(&validator).await {
                error!(
                    target: "dwowd::rpc::rpc_stratum::stratum_submit",
                    "[RPC-STRATUM] Error refreshing registry jobs: {e}",
                );
            }

            return miner_status_response(id, "rejected")
        }

        // Mark block as submitted
        block_template.submitted = true;
        registry.block_templates.insert(wallet, block_template);

        miner_status_response(id, "OK")
    }

    /// Linear-testnet stratum submit - handles block submissions for linear blockchain.
    /// Reconstructs the block from stored template, applies ZK coinbase, and validates
    /// via apply_block() (which verifies PoW, merkle roots, ZK proofs, and nullifiers).
    async fn stratum_submit_linear(&self, id: u16, params: JsonValue) -> JsonResult {
        use crate::registry::model::generate_linear_block_template;

        info!(
            target: "dwowd::rpc::rpc_stratum::stratum_submit_linear",
            "[RPC-STRATUM] stratum_submit_linear called id={}",
            id,
        );

        // Serialize submissions to prevent concurrent access to the
        // RandomX VM (which is not thread-safe) from multiple XMRig
        // solutions arriving in rapid succession.
        let _submit_guard = self.linear_submit_lock.lock().await;

        // Parse request params
        let Some(params) = params.get::<HashMap<String, JsonValue>>() else {
            return JsonError::new(InvalidParams, None, id).into()
        };

        // Parse client id
        let Some(client_id) = params.get("id") else {
            return server_error(RpcError::MinerMissingClientId, id, None)
        };
        let Some(client_id) = client_id.get::<String>() else {
            return server_error(RpcError::MinerInvalidClientId, id, None)
        };

        // Parse job id
        let Some(job_id) = params.get("job_id") else {
            return server_error(RpcError::MinerMissingJobId, id, None)
        };
        let Some(job_id) = job_id.get::<String>() else {
            return server_error(RpcError::MinerInvalidJobId, id, None)
        };

        // Parse nonce
        let Some(nonce) = params.get("nonce") else {
            return server_error(RpcError::MinerMissingNonce, id, None)
        };
        let Some(nonce) = nonce.get::<String>() else {
            return server_error(RpcError::MinerInvalidNonce, id, None)
        };
        let Ok(nonce_bytes) = hex::decode(nonce) else {
            return server_error(RpcError::MinerInvalidNonce, id, None)
        };
        if nonce_bytes.len() != 4 {
            return server_error(RpcError::MinerInvalidNonce, id, None)
        }
        let nonce = u32::from_le_bytes(nonce_bytes.try_into().unwrap());

        // Parse result (RandomX hash) for logging
        let xmrig_result = params.get("result").and_then(|r| r.get::<String>());

        info!(
            target: "dwowd::rpc::rpc_stratum::stratum_submit_linear",
            "[RPC-STRATUM] Got solution from client {client_id} for job: {job_id}",
        );

        // Get linear blockchain
        let Some(linear_chain) = self.linear_blockchain.as_ref() else {
            return miner_status_response(id, "rejected")
        };

        // Check block rate limiting
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let last_time = self.last_block_time.load(Ordering::SeqCst);
        if last_time > 0 && now.saturating_sub(last_time) < self.min_block_interval {
            info!(
                target: "dwowd::rpc::rpc_stratum::stratum_submit_linear",
                "[RPC-STRATUM] Rate-limited: {}s since last block (min {})",
                now.saturating_sub(last_time), self.min_block_interval
            );
            return miner_status_response(id, "stale")
        }

        // Get current height and validate job height
        let current_height = linear_chain.get_height();
        let submitted_height: u64 = job_id
            .trim_start_matches("linear-job-")
            .parse()
            .unwrap_or(current_height + 1);

        if submitted_height != current_height + 1 {
            info!(
                target: "dwowd::rpc::rpc_stratum::stratum_submit_linear",
                "[RPC-STRATUM] Stale: submitted height {} != expected {}",
                submitted_height, current_height + 1
            );
            return miner_status_response(id, "stale")
        }

        let randomx_key = dwow_linear::Miner::derive_key_from_height(submitted_height);
        let difficulty_target = {
            let consensus = linear_chain.consensus.lock().unwrap();
            consensus.difficulty_target()
        };

        // Build previous hash using the previous block's own RandomX key,
        // not the current block's key.
        let previous_hash = if submitted_height == 1 {
            blake3::Hash::from_bytes([0u8; 32])
        } else {
            match linear_chain.get_latest_block() {
                Ok(block) => {
                    let prev_key = block.header.randomx_key;
                    let prev_vm = linear_chain.get_vm(prev_key);
                    block.hash(&prev_vm)
                }
                Err(_) => blake3::Hash::from_bytes([0u8; 32]),
            }
        };

        // Load stored template to get the ZK coinbase data and timestamp.
        // Timestamp MUST match the mining blob that xmrig hashed — PoW will fail otherwise.
        let template = self.current_linear_template.lock().await.take();
        let template_timestamp = template.as_ref().map(|t| t.timestamp).unwrap_or(now);
        let (coinbase, coin_merkle_root, nullifier_root) = if let Some(ref tmpl) = template {
            if !tmpl.zk_proof.is_empty() {
                let cb = dwow_linear::CoinbaseTransaction {
                    proof: tmpl.zk_proof.clone(),
                    public_inputs: tmpl.zk_public_inputs,
                    coin: tmpl.coin,
                    value_commit_x: tmpl.value_commit_x,
                    value_commit_y: tmpl.value_commit_y,
                    token_commit: tmpl.token_commit,
                    encrypted_note: tmpl.encrypted_note.clone(),
                };
                (Some(cb), tmpl.coin_merkle_root, tmpl.nullifier_root)
            } else {
                (None, [0u8; 32], [0u8; 32])
            }
        } else {
            (None, [0u8; 32], [0u8; 32])
        };

        // Compute block reward from the exponential-decay emission schedule
        let reward = dwow_sdk::blockchain::expected_reward(submitted_height as u32);

        // Build block header with privacy fields
        let header = dwow_linear::BlockHeader {
            version: 1,
            previous: previous_hash,
            merkle_root: blake3::hash(&[]),
            timestamp: template_timestamp,
            difficulty_target,
            nonce,
            height: submitted_height,
            uncle_merkle_root: [0u8; 32],
            total_reward: reward,
            randomx_key,
            coin_merkle_root,
            nullifier_root,
            anchor_tx_id: [0u8; 32],
            anchor_monero_height: 0,
            anchor_monero_hash: [0u8; 32],
            finality_flags: 0,
        };

        // Create coinbase transaction with ZK privacy data
        let coinbase_tx = dwow_linear::Transaction {
            version: 1,
            inputs: vec![],
            outputs: vec![dwow_linear::Output {
                value: reward,
                script: vec![],
            }],
            contract_calls: vec![],
            lock_time: 0,
            coinbase,
        };

        let mut block = dwow_linear::Block {
            header,
            transactions: vec![coinbase_tx],
        };

        // Verify PoW before inserting the block. This catches invalid
        // nonces (e.g. from adaptor blob layout mismatches) before they
        // corrupt chain state.
        {
            let submit_blob = block.header.to_mining_blob();
            let vm = linear_chain.get_vm(randomx_key);
            let daemon_hash = block.hash(&vm);
            info!(
                target: "dwowd::rpc::rpc_stratum::stratum_submit_linear",
                "[RPC-STRATUM] Submit — nonce={}, blob={}, daemon_hash={}, xmrig_hash={}",
                nonce,
                hex::encode(&submit_blob),
                hex::encode(daemon_hash.as_bytes()),
                xmrig_result.as_deref().unwrap_or("none"),
            );
            match linear_chain.consensus.lock().unwrap().verify_proof(&block, &vm) {
                Ok(true) => {}
                Ok(false) => {
                    info!(
                        target: "dwowd::rpc::rpc_stratum::stratum_submit_linear",
                        "[RPC-STRATUM] Block at height {} rejected: PoW verification failed",
                        submitted_height
                    );
                    return miner_status_response(id, "rejected");
                }
                Err(e) => {
                    info!(
                        target: "dwowd::rpc::rpc_stratum::stratum_submit_linear",
                        "[RPC-STRATUM] Block at height {} rejected: PoW error: {}",
                        submitted_height, e
                    );
                    return miner_status_response(id, "rejected");
                }
            }
        }

        // Anchor the block to Arweave via Caribina (best-effort, configurable)
        {
            let fc = &linear_chain.finality_config;
            if fc.should_anchor() {
                let vm = linear_chain.get_vm(randomx_key);
                let block_hash = block.hash(&vm);
                let mut block_hash_bytes = [0u8; 32];
                block_hash_bytes.copy_from_slice(block_hash.as_bytes());
                match anchor_block(&block_hash_bytes, block.header.timestamp, block.header.height) {
                    Some(tx_id) => {
                        block.header.anchor_tx_id = tx_id;
                        block.header.finality_flags = fc.mine_flags();
                        info!(
                            target: "dwowd::rpc::rpc_stratum::stratum_submit_linear",
                            "[RPC-STRATUM] Anchored block {} to Arweave",
                            block_hash
                        );
                    }
                    None => {
                        info!(
                            target: "dwowd::rpc::rpc_stratum::stratum_submit_linear",
                            "[RPC-STRATUM] Arweave anchor skipped (network/turbo unavailable)"
                        );
                    }
                }
            }
        }

        // Insert the validated block into the chain
        match linear_chain.insert_block(&block) {
            Ok(_) => {
                // Trigger difficulty adjustment
                {
                    let mut consensus = linear_chain.consensus.lock().unwrap();
                    consensus.record_block(now, difficulty_target);
                    consensus.adjust_difficulty();
                }

                self.last_block_time.store(now, Ordering::SeqCst);

                info!(
                    target: "dwowd::rpc::rpc_stratum::stratum_submit_linear",
                    "[RPC-STRATUM] Block at height {} accepted!",
                    submitted_height
                );

                // Generate and push new mining job to all connected miners
                // so they can start mining the next block immediately.
                if let Some(ref publisher) = *self.linear_stratum_publisher.lock().await {
                    if let Some(ref recipient_config) = *self.linear_recipient_config.lock().await {
                        let linear_zk = {
                            let zk_lock = self.linear_zk.lock().await;
                            zk_lock.clone()
                        };

                        match generate_linear_block_template(
                            linear_chain,
                            recipient_config,
                            linear_zk.as_ref(),
                        )
                        .await
                        {
                            Ok(new_template) => {
                                let new_height = new_template.height;
                                let new_job_id =
                                    format!("linear-job-{}", new_height);
                                let new_randomx_key =
                                    dwow_linear::Miner::derive_key_from_height(
                                        new_height,
                                    );
                                let new_seed_hash = hex::encode(new_randomx_key);

                                let new_mining_header = dwow_linear::BlockHeader {
                                    version: 1,
                                    previous: blake3::Hash::from_bytes(
                                        new_template.previous,
                                    ),
                                    merkle_root: blake3::hash(&[]),
                                    timestamp: new_template.timestamp,
                                    difficulty_target: new_template.difficulty_target,
                                    nonce: 0,
                                    height: new_height,
                                    uncle_merkle_root: [0u8; 32],
                                    total_reward: new_template.value,
                                    randomx_key: new_randomx_key,
                                    coin_merkle_root: new_template.coin_merkle_root,
                                    nullifier_root: new_template.nullifier_root,
                                    anchor_tx_id: [0u8; 32],
                                    anchor_monero_height: 0,
                                    anchor_monero_hash: [0u8; 32],
                                    finality_flags: 0,
                                };
                                let new_blob_data = new_mining_header.to_mining_blob();
                                let new_blob = hex::encode(&new_blob_data);
                                info!(
                                    target: "dwowd::rpc::rpc_stratum::stratum_submit_linear",
                                    "[RPC-STRATUM] Push blob (to xmrig): {new_blob}",
                                );
                                let new_target = format!(
                                    "{:016x}",
                                    new_template.difficulty_target as u64
                                );

                                let job_params =
                                    JsonValue::from(HashMap::from([
                                        (
                                            "blob".to_string(),
                                            JsonValue::from(new_blob),
                                        ),
                                        (
                                            "job_id".to_string(),
                                            JsonValue::from(new_job_id),
                                        ),
                                        (
                                            "height".to_string(),
                                            JsonValue::from(new_height as f64),
                                        ),
                                        (
                                            "target".to_string(),
                                            JsonValue::from(new_target),
                                        ),
                                        (
                                            "algo".to_string(),
                                            JsonValue::from(String::from("rx/0")),
                                        ),
                                        (
                                            "seed_hash".to_string(),
                                            JsonValue::from(new_seed_hash),
                                        ),
                                        (
                                            "reserved_offset".to_string(),
                                            JsonValue::from(40_f64),
                                        ),
                                    ]));

                                // Store template for the next submit call
                                *self.current_linear_template.lock().await =
                                    Some(new_template);

                                // Push notification to all subscribed miners
                                let notification = dwow::rpc::jsonrpc::JsonNotification::new("job", job_params);
                                publisher.notify(notification).await;

                                info!(
                                    target: "dwowd::rpc::rpc_stratum::stratum_submit_linear",
                                    "[RPC-STRATUM] Pushed new mining job to miners: height={}",
                                    new_height,
                                );
                            }
                            Err(e) => {
                                error!(
                                    target: "dwowd::rpc::rpc_stratum::stratum_submit_linear",
                                    "[RPC-STRATUM] Failed to generate new block template: {e}",
                                );
                            }
                        }
                    }
                }

                miner_status_response(id, "OK")
            }
            Err(e) => {
                error!(
                    target: "dwowd::rpc::rpc_stratum::stratum_submit_linear",
                    "[RPC-STRATUM] Block rejected: {e}",
                );
                miner_status_response(id, "rejected")
            }
        }
    }

    // RPCAPI:
    // Miner sends `keepalived` to prevent connection timeout.
    //
    // **Request:**
    // * `id` : Registry client ID
    //
    // **Response:**
    // * `status`: Response status
    //
    // --> {"jsonrpc": "2.0", "method": "keepalived", "params": {"id": "foo"}, "id": 1}
    // <-- {"jsonrpc": "2.0", "result": {"status": "KEEPALIVED"}, "id": 1}
    pub async fn stratum_keepalived(&self, id: u16, params: JsonValue) -> JsonResult {
        // Parse request params
        let Some(params) = params.get::<HashMap<String, JsonValue>>() else {
            return JsonError::new(InvalidParams, None, id).into()
        };

        // Parse client id
        let Some(client_id) = params.get("id") else {
            return server_error(RpcError::MinerMissingClientId, id, None)
        };
        let Some(client_id) = client_id.get::<String>() else {
            return server_error(RpcError::MinerInvalidClientId, id, None)
        };

        // If we don't know about this client job, we can just abort here
        if !self.registry.state.read().await.jobs.contains_key(client_id) {
            return server_error(RpcError::MinerUnknownClient, id, None)
        };

        // Respond with keepalived message
        miner_status_response(id, "KEEPALIVED")
    }
}
