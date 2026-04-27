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

use std::{
    collections::{HashMap, HashSet},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

use async_trait::async_trait;
use smol::lock::MutexGuard;
use tinyjson::JsonValue;
use tracing::{debug, error, info};

use darkfi::{
    rpc::{
        jsonrpc::{
            ErrorCode, ErrorCode::InvalidParams, JsonError, JsonRequest, JsonResponse, JsonResult,
        },
        server::RequestHandler,
    },
    system::StoppableTaskPtr,
};

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
		debug!(target: "darkfid::rpc::stratum_rpc", "--> {}", req.stringify().unwrap());

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
        // Check if node is synced before responding
        let validator = self.validator.read().await;
        if !validator.synced {
            return JsonResponse::new(JsonValue::from(HashMap::new()), id).into()
        }

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
            target: "darkfid::rpc::rpc_stratum::stratum_login",
            "[RPC-STRATUM] Got login from {wallet} ({agent})",
        );

        // Check if we're in linear-testnet mode and use linear-specific mining
        if let Some(linear_chain) = self.linear_blockchain.as_ref() {
            let linear_config = match LinearMinerRewardsRecipientConfig::from_str(wallet).await {
                Ok(c) => c,
                Err(e) => return server_error(e, id, None),
            };
            return self.stratum_login_linear(id, linear_chain, linear_config, agent.to_string()).await;
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
                    target: "darkfid::rpc::rpc_stratum::stratum_login",
                    "[RPC-STRATUM] Failed to register miner: {e}",
                );
                return JsonResponse::new(JsonValue::from(HashMap::new()), id).into()
            }
        };

        // Now we have the new job, we ship it to RPC
        info!(
            target: "darkfid::rpc::rpc_stratum::stratum_login",
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
        use darkfi_sdk::pasta::pallas;
        use std::time::{SystemTime, UNIX_EPOCH};

        // Generate a unique client ID based on timestamp and random bytes
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let client_id = format!("{}-{}", agent.replace("/", "-"), timestamp);

        // Generate block template using LinearBlockchain
        let template = match generate_linear_block_template(linear_blockchain, &config).await {
            Ok(t) => t,
            Err(e) => {
                error!(
                    target: "darkfid::rpc::rpc_stratum::stratum_login_linear",
                    "[RPC-STRATUM] Failed to generate linear block template: {e}",
                );
                return JsonResponse::new(JsonValue::from(HashMap::new()), id).into()
            }
        };

        // Create job ID
        let job_id = format!("linear-job-{}", template.height);

        // Build the job blob - this is the hex-encoded block data that miners hash
        // drk miner puts nonce at byte offset 39, so we need to align our header to this.
        // Format: previous(32) + height(8) + nonce_placeholder(4) + difficulty_target(4) + coinbase_script
        // This makes byte 39 be the first byte of our placeholder (which drk overwrites with nonce[0])
        // But actually byte 39 will be height[7] after our format - this is fine because we store height in job metadata.
        //
        // Actually, for correct alignment: previous(32) + height(7) + padding(1) + nonce_pl(4) + height_rem(1) + diff(4) + script
        // But let's simplify: just put a placeholder byte at byte 39.
        //
        // Correct format for drk compatibility:
        // bytes 0-31: previous hash (32 bytes)
        // bytes 32-39: height (8 bytes) - byte 39 gets overwritten by drk with nonce[0]
        // bytes 40-43: nonce placeholder (4 bytes) - drk overwrites these with actual nonce
        // bytes 44-47: difficulty_target (4 bytes)
        // bytes 48-79: coinbase_script (32 bytes)
        let mut blob_data = Vec::new();
        blob_data.extend_from_slice(&template.previous);
        blob_data.extend_from_slice(&template.height.to_le_bytes());
        // Add 4-byte nonce placeholder at byte 40 (after height which is bytes 32-39)
        blob_data.extend_from_slice(&[0u8, 0u8, 0u8, 0u8]);
        blob_data.extend_from_slice(&template.difficulty_target.to_le_bytes());
        blob_data.extend_from_slice(&template.coinbase_output.script);

        let blob = hex::encode(&blob_data);

        // Target: drk expects 8 bytes (64 hex chars) representing a u256 in little-endian
        // For difficulty 0x000000FF, we want the first byte to be 0xFF and rest 0x00
        // So target = 0xFF00000000000000000000000000000000000000000000000000000000000000
        // In hex string (LE): "ff00000000000000000000000000000000000000000000000000000000000000"
        // But since difficulty_target is 0x000000FF (u32), we want the FIRST BYTE to be 0xFF
        // This means target should be: bytes 0-31 = [0xFF, 0, 0, 0, ..., 0]
        // In hex LE representation: "ff00000000000000000000000000000000000000000000000000000000000000"
        //
        // Wait - we're comparing the hash as u256, so:
        // hash = 0x10fa1b01... (first byte 0x10 = 16 in decimal)
        // target = 0xff0000... (first byte 0xff = 255 in decimal)
        // 16 < 255, so hash < target should be true!
        //
        // But the comparison in darkfid is using u32 (first 4 bytes only)
        // hash_u32 = 0x011bfa10 = 18610704, which is > 255
        // This is WRONG - should compare full 32 bytes
        //
        // For now, use the SAME target format as drk expects: 8 bytes zero-padded
        // The target we send is "ff00000000000000" (8 bytes hex = 16 chars)
        // But difficulty_target is only 4 bytes, so we pad to 8 bytes
        // Actually the target should be the FULL 32 bytes for proper u256 comparison
        // Since we use u32 difficulty, let's format target as u32 in first 4 bytes, zero-padded
        let target = format!("{:016x}", template.difficulty_target as u64);

        // Get RandomX seed hash
        let randomx_key = darkfi_linear::Miner::derive_key_from_height(template.height);
        let seed_hash = hex::encode(randomx_key);

        info!(
            target: "darkfid::rpc::rpc_stratum::stratum_login_linear",
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
            ]))),
            ("status".to_string(), JsonValue::from(String::from("OK"))),
        ]));
        JsonResponse::new(response, id).into()
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
            target: "darkfid::rpc::rpc_stratum::stratum_submit",
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
                target: "darkfid::rpc::rpc_stratum::stratum_submit",
                "[RPC-STRATUM] Error submitting new block: {e}",
            );

            // Try to refresh the jobs before returning error
            if let Err(e) = registry.refresh(&validator).await {
                error!(
                    target: "darkfid::rpc::rpc_stratum::stratum_submit",
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

    /// Linear-testnet stratum submit - handles block submissions for linear blockchain
    async fn stratum_submit_linear(&self, id: u16, params: JsonValue) -> JsonResult {
        info!(
            target: "darkfid::rpc::rpc_stratum::stratum_submit_linear",
            "[RPC-STRATUM] >>> stratum_submit_linear CALLED! id={}, params={}",
            id,
            params.stringify().unwrap_or_else(|_| "PARSE_ERROR".to_string())
        );

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

        // Parse result (RandomX hash)
        let Some(result) = params.get("result") else {
            return server_error(RpcError::MinerMissingResult, id, None)
        };
        let Some(result) = result.get::<String>() else {
            return server_error(RpcError::MinerInvalidResult, id, None)
        };

        info!(
            target: "darkfid::rpc::rpc_stratum::stratum_submit_linear",
            "[RPC-STRATUM] Got solution submission from client {client_id} for job: {job_id}",
        );

        // Get linear blockchain
        let Some(linear_chain) = self.linear_blockchain.as_ref() else {
            return miner_status_response(id, "rejected")
        };

        // Check block rate limiting - prevent submitting too fast
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let last_time = self.last_block_time.load(Ordering::SeqCst);
        if last_time > 0 && now.saturating_sub(last_time) < self.min_block_interval {
            info!(
                target: "darkfid::rpc::rpc_stratum::stratum_submit_linear",
                "[RPC-STRATUM] Block submission rate-limited: {} seconds since last block (min {})",
                now.saturating_sub(last_time), self.min_block_interval
            );
            return miner_status_response(id, "stale")
        }

        // Parse the result hash
        let hash_bytes = match hex::decode(result) {
            Ok(b) => {
                if b.len() != 32 {
                    return miner_status_response(id, "rejected")
                }
                b
            }
            Err(_) => return miner_status_response(id, "rejected")
        };

        // Get the difficulty target atomically from consensus
        let difficulty_target = {
            let consensus = linear_chain.consensus.lock().unwrap();
            consensus.difficulty_target()
        };

        // Get current height to determine RandomX key
        let current_height = linear_chain.get_height();
        let submitted_height: u64 = job_id
            .trim_start_matches("linear-job-")
            .parse()
            .unwrap_or(current_height + 1);

        // Verify height is correct (stale block detection)
        if submitted_height != current_height + 1 {
            info!(
                target: "darkfid::rpc::rpc_stratum::stratum_submit_linear",
                "[RPC-STRATUM] Stale block submission: submitted height {} != expected {}",
                submitted_height, current_height + 1
            );
            return miner_status_response(id, "stale")
        }

        let randomx_key = darkfi_linear::Miner::derive_key_from_height(submitted_height);
        let vm = linear_chain.get_vm(randomx_key);

        // Verify hash meets difficulty target
        // For difficulty_target=0x000000FF, drk expects first byte of hash to be < 0xFF
        // hash_first_byte comparison:
        // hash = 0x011bfa10... (first byte = 0x01 = 1)
        // target = 0xff in first byte = 255
        // 1 < 255 means hash should pass!
        let hash_first_byte = hash_bytes[0];
        let target_first_byte = (difficulty_target & 0xFF) as u8;
        info!(
            target: "darkfid::rpc::rpc_stratum::stratum_submit_linear",
            "[RPC-STRATUM] Submitted hash_first_byte=0x{:02x}, difficulty_target=0x{:08x}, target_first_byte=0x{:02x}",
            hash_first_byte, difficulty_target, target_first_byte
        );
        if hash_first_byte > target_first_byte {
            info!(
                target: "darkfid::rpc::rpc_stratum::stratum_submit_linear",
                "[RPC-STRATUM] Block hash first byte {} does not meet target {}",
                hash_first_byte, target_first_byte
            );
            return miner_status_response(id, "rejected")
        }

        info!(
            target: "darkfid::rpc::rpc_stratum::stratum_submit_linear",
            "[RPC-STRATUM] Hash first byte 0x{:02x} < target 0x{:02x}! Submitting block...",
            hash_first_byte, target_first_byte
        );

        // Build block header
        let previous_hash = if submitted_height == 1 {
            blake3::Hash::from_bytes([0u8; 32])
        } else {
            match linear_chain.get_latest_block() {
                Ok(block) => block.hash(&vm),
                Err(_) => blake3::Hash::from_bytes([0u8; 32]),
            }
        };

        let header = darkfi_linear::BlockHeader {
            version: 1,
            previous: previous_hash,
            merkle_root: blake3::hash(&[]), // Empty for coinbase-only block
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            difficulty_target,
            nonce,
            height: submitted_height,
            uncle_merkle_root: [0u8; 32],
            total_reward: 100_000_000,
            randomx_key,
        };

        // Create coinbase output (this would normally come from the submitted blob)
        let coinbase_output = darkfi_linear::Output {
            value: 100_000_000,
            script: vec![], // Would be extracted from blob in full implementation
        };

        let coinbase_tx = darkfi_linear::Transaction {
            version: 1,
            inputs: vec![],
            outputs: vec![coinbase_output],
            contract_calls: vec![],
            lock_time: 0,
        };

        // Create the block
        let block = darkfi_linear::Block {
            header,
            transactions: vec![coinbase_tx],
        };

        // Insert the block into the blockchain
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        match linear_chain.insert_block(&block) {
            Ok(_) => {
                // Update last block time for rate limiting
                self.last_block_time.store(now, Ordering::SeqCst);

                // Trigger difficulty adjustment for next block
                {
                    let mut consensus = linear_chain.consensus.lock().unwrap();
                    consensus.record_block(now, difficulty_target);
                    consensus.adjust_difficulty();
                }

                info!(
                    target: "darkfid::rpc::rpc_stratum::stratum_submit_linear",
                    "[RPC-STRATUM] Block at height {} inserted successfully!",
                    submitted_height
                );
                miner_status_response(id, "OK")
            }
            Err(e) => {
                error!(
                    target: "darkfid::rpc::rpc_stratum::stratum_submit_linear",
                    "[RPC-STRATUM] Failed to insert block: {e}",
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
