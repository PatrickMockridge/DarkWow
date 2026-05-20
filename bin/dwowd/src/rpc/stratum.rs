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
    sync::atomic::Ordering,
    time::Duration,
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

use crate::{
    error::{miner_status_response, server_error, RpcError},
    registry::model::LinearMinerRewardsRecipientConfig,
    DwowNode,
};

// https://github.com/xmrig/xmrig-proxy/blob/master/doc/STRATUM.md
// https://github.com/xmrig/xmrig-proxy/blob/master/doc/STRATUM_EXT.md

/// JSON-RPC `RequestHandler` for Stratum
pub struct StratumRpcHandler;

#[async_trait]
impl RequestHandler<StratumRpcHandler> for DwowNode {
    async fn handle_request(&self, req: JsonRequest) -> JsonResult {
        debug!(target: "dwowd::rpc::stratum_rpc", "--> {}", req.stringify().unwrap());

        match req.method.as_str() {
            "login" => self.stratum_login(req.id, req.params).await,
            "submit" => self.stratum_submit(req.id, req.params).await,
            _ => JsonError::new(ErrorCode::MethodNotFound, None, req.id).into(),
        }
    }

    async fn connections_mut(&self) -> MutexGuard<'life0, HashSet<StoppableTaskPtr>> {
        self.registry.stratum_rpc_connections.lock().await
    }
}

impl DwowNode {
    /// Stratum login — linear-only path.
    ///
    /// Parses xmrig login request, generates a block template, and returns
    /// a mining job. The response is a flat stratum JSON object written inside
    /// the JSON-RPC response envelope.
    pub async fn stratum_login(&self, id: u16, params: JsonValue) -> JsonResult {
        use crate::registry::model::generate_linear_block_template;
        use std::time::{SystemTime, UNIX_EPOCH};

        // Parse request params
        let Some(params) = params.get::<HashMap<String, JsonValue>>() else {
            return JsonError::new(InvalidParams, None, id).into()
        };

        // Parse login (wallet address)
        let Some(wallet) = params.get("login") else {
            return server_error(RpcError::MinerMissingLogin, id, None)
        };
        let Some(wallet) = wallet.get::<String>() else {
            return server_error(RpcError::MinerInvalidLogin, id, None)
        };

        // Parse password (unused but required by protocol)
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

        // Parse algo — must support rx/0 (RandomX)
        let Some(algo) = params.get("algo") else {
            return server_error(RpcError::MinerMissingAlgo, id, None)
        };
        let Some(algo) = algo.get::<Vec<JsonValue>>() else {
            return server_error(RpcError::MinerInvalidAlgo, id, None)
        };
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

        info!(
            target: "dwowd::rpc::rpc_stratum::stratum_login",
            "[RPC-STRATUM] Got login from {wallet} ({agent})",
        );

        let linear_chain = match self.linear_blockchain.as_ref() {
            Some(c) => c,
            None => return server_error(RpcError::MinerMissingPassword, id, None),
        };

        let config = match LinearMinerRewardsRecipientConfig::from_str(wallet).await {
            Ok(c) => c,
            Err(e) => return server_error(e, id, None),
        };

        // Generate unique client ID
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
                    linear_chain.clone(),
                ).await {
                    Ok(zk) => *zk_lock = Some(zk),
                    Err(e) => {
                        tracing::warn!(
                            target: "dwowd::rpc::rpc_stratum::stratum_login",
                            "[RPC-STRATUM] Failed to init ZK: {e}, using transparent coinbase",
                        );
                    }
                }
            }
            zk_lock.clone()
        };

        // Generate block template
        let template = match generate_linear_block_template(
            linear_chain, &config, linear_zk.as_ref(),
        ).await {
            Ok(t) => t,
            Err(e) => {
                error!(
                    target: "dwowd::rpc::rpc_stratum::stratum_login",
                    "[RPC-STRATUM] Failed to generate linear block template: {e}",
                );
                return JsonResponse::new(JsonValue::from(HashMap::new()), id).into()
            }
        };

        // Store template and config for submit handler
        *self.current_linear_template.lock().await = Some(template.clone());
        *self.linear_recipient_config.lock().await = Some(config);

        // Create or reuse shared publisher for push notifications
        let publisher = {
            let mut lock = self.linear_stratum_publisher.lock().await;
            if lock.is_none() {
                *lock = Some(Publisher::new());
            }
            lock.as_ref().unwrap().clone()
        };

        // Spawn keepalive to prevent xmrig disconnects.
        // xmrig reconnects after ~6s of TCP silence, resetting mining progress.
        // Pushing the current job every 5s keeps the connection alive while
        // xmrig searches for a valid nonce.
        let was_active = self.stratum_keepalive_active.swap(true, Ordering::SeqCst);
        if !was_active {
            let kp_publisher = publisher.clone();
            let kp_template = self.current_linear_template.clone();
            self.ex.spawn(async move {
                loop {
                    smol::Timer::after(Duration::from_secs(5)).await;
                    let tmpl = kp_template.lock().await;
                    if let Some(ref t) = *tmpl {
                        let kp_header = dwow_linear::BlockHeader {
                            version: 1,
                            previous: blake3::Hash::from_bytes(t.previous),
                            merkle_root: blake3::hash(&[]),
                            timestamp: t.timestamp,
                            difficulty_target: t.difficulty_target,
                            nonce: 0,
                            height: t.height,
                            uncle_merkle_root: [0u8; 32],
                            total_reward: t.value,
                            randomx_key: dwow_linear::Miner::derive_key_from_height(t.height),
                            coin_merkle_root: t.coin_merkle_root,
                            nullifier_root: t.nullifier_root,
                            anchor_tx_id: [0u8; 32],
                            anchor_monero_height: 0,
                            anchor_monero_hash: [0u8; 32],
                            finality_flags: 0,
                        };
                        let kp_blob = hex::encode(&kp_header.to_mining_blob());
                        let kp_pool_diff =
                            0xFFFFFFFFu64 / t.difficulty_target as u64;
                        let kp_job_id = format!("linear-job-{}", t.height);
                        let kp_seed_hash = hex::encode(kp_header.randomx_key);
                        let kp_params = JsonValue::from(HashMap::from([
                            ("blob".to_string(), JsonValue::from(kp_blob)),
                            ("job_id".to_string(), JsonValue::from(kp_job_id)),
                            ("height".to_string(), JsonValue::from(t.height as f64)),
                            ("target".to_string(), JsonValue::from(format!("{}", kp_pool_diff))),
                            ("algo".to_string(), JsonValue::from(String::from("rx/0"))),
                            ("seed_hash".to_string(), JsonValue::from(kp_seed_hash)),
                            ("reserved_offset".to_string(), JsonValue::from(39_f64)),
                        ]));
                        let kp_notif =
                            dwow::rpc::jsonrpc::JsonNotification::new("job", kp_params);
                        kp_publisher.notify(kp_notif).await;
                    }
                }
            })
            .detach();
        }

        let job_id = format!("linear-job-{}", template.height);
        let randomx_key = dwow_linear::Miner::derive_key_from_height(template.height);
        let seed_hash = hex::encode(randomx_key);

        // Build mining blob from block header (nonce=0 placeholder)
        let mining_header = dwow_linear::BlockHeader {
            version: 1,
            previous: blake3::Hash::from_bytes(template.previous),
            merkle_root: blake3::hash(&[]),
            timestamp: template.timestamp,
            difficulty_target: template.difficulty_target,
            nonce: 0,
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
            target: "dwowd::rpc::rpc_stratum::stratum_login",
            "[RPC-STRATUM] Login blob (to xmrig): {blob}",
        );

        // Pool difficulty as decimal string (xmrig parses with strtoull base 10)
        let pool_diff = 0xFFFFFFFFu64 / template.difficulty_target as u64;
        let target = format!("{}", pool_diff);

        info!(
            target: "dwowd::rpc::rpc_stratum::stratum_login",
            "[RPC-STRATUM] Created mining job for {client_id}: height={}, job_id={job_id}",
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
                ("reserved_offset".to_string(), JsonValue::from(39_f64)),
            ]))),
            ("status".to_string(), JsonValue::from(String::from("OK"))),
        ]));

        let subscriber = JsonSubscriber {
            method: "job",
            publisher,
        };
        (subscriber, JsonResponse::new(response, id)).into()
    }

    /// Stratum submit — linear-only path.
    ///
    /// Parses xmrig solution, reconstructs the block with the found nonce,
    /// verifies PoW via the RandomX VM, and inserts the block if valid.
    pub async fn stratum_submit(&self, id: u16, params: JsonValue) -> JsonResult {
        use crate::registry::model::generate_linear_block_template;
        use dwow_linear::caribina::anchor_block;

        info!(
            target: "dwowd::rpc::rpc_stratum::stratum_submit",
            "[RPC-STRATUM] stratum_submit called id={}",
            id,
        );

        // Serialize submissions to prevent concurrent RandomX VM access
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
            target: "dwowd::rpc::rpc_stratum::stratum_submit",
            "[RPC-STRATUM] Got solution from client {client_id} for job: {job_id}",
        );

        let linear_chain = match self.linear_blockchain.as_ref() {
            Some(c) => c,
            None => return miner_status_response(id, "rejected"),
        };

        // Rate limit blocks
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let last_time = self.last_block_time.load(Ordering::SeqCst);
        if last_time > 0 && now.saturating_sub(last_time) < self.min_block_interval {
            info!(
                target: "dwowd::rpc::rpc_stratum::stratum_submit",
                "[RPC-STRATUM] Rate-limited: {}s since last block (min {})",
                now.saturating_sub(last_time), self.min_block_interval
            );
            return miner_status_response(id, "stale")
        }

        // Validate job height
        let current_height = linear_chain.get_height();
        let submitted_height: u64 = job_id
            .trim_start_matches("linear-job-")
            .parse()
            .unwrap_or(current_height + 1);

        if submitted_height != current_height + 1 {
            info!(
                target: "dwowd::rpc::rpc_stratum::stratum_submit",
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

        // Build previous hash using previous block's RandomX key
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

        // Load stored template for ZK coinbase data and timestamp.
        // Timestamp MUST match the mining blob that xmrig hashed.
        let template = self.current_linear_template.lock().await.clone();
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

        let reward = dwow_sdk::blockchain::expected_reward(submitted_height as u32);

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

        // Verify PoW before inserting
        {
            let submit_blob = block.header.to_mining_blob();
            let vm = linear_chain.get_vm(randomx_key);
            let daemon_hash = block.hash(&vm);
            info!(
                target: "dwowd::rpc::rpc_stratum::stratum_submit",
                "[RPC-STRATUM] Submit — nonce={}, blob={}, daemon_hash={}, xmrig_hash={}",
                nonce,
                hex::encode(&submit_blob),
                hex::encode(daemon_hash.as_bytes()),
                xmrig_result.as_ref().map(|s| s.as_str()).unwrap_or("none"),
            );
            match linear_chain.consensus.lock().unwrap().verify_proof(&block, &vm) {
                Ok(true) => {}
                Ok(false) => {
                    info!(
                        target: "dwowd::rpc::rpc_stratum::stratum_submit",
                        "[RPC-STRATUM] Block at height {} rejected: PoW verification failed",
                        submitted_height
                    );
                    return miner_status_response(id, "rejected");
                }
                Err(e) => {
                    info!(
                        target: "dwowd::rpc::rpc_stratum::stratum_submit",
                        "[RPC-STRATUM] Block at height {} rejected: PoW error: {}",
                        submitted_height, e
                    );
                    return miner_status_response(id, "rejected");
                }
            }
        }

        // Anchor to Arweave via Caribina (best-effort)
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
                            target: "dwowd::rpc::rpc_stratum::stratum_submit",
                            "[RPC-STRATUM] Anchored block {} to Arweave",
                            block_hash
                        );
                    }
                    None => {
                        info!(
                            target: "dwowd::rpc::rpc_stratum::stratum_submit",
                            "[RPC-STRATUM] Arweave anchor skipped (network/turbo unavailable)"
                        );
                    }
                }
            }
        }

        // Insert validated block
        match linear_chain.insert_validated_block(&block) {
            Ok(_) => {
                self.last_block_time.store(now, Ordering::SeqCst);

                info!(
                    target: "dwowd::rpc::rpc_stratum::stratum_submit",
                    "[RPC-STRATUM] Block at height {} accepted!",
                    submitted_height
                );

                // Push new mining job to all connected miners
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
                                let new_job_id = format!("linear-job-{}", new_height);
                                let new_randomx_key =
                                    dwow_linear::Miner::derive_key_from_height(new_height);
                                let new_seed_hash = hex::encode(new_randomx_key);

                                let new_mining_header = dwow_linear::BlockHeader {
                                    version: 1,
                                    previous: blake3::Hash::from_bytes(new_template.previous),
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
                                    target: "dwowd::rpc::rpc_stratum::stratum_submit",
                                    "[RPC-STRATUM] Push blob (to xmrig): {new_blob}",
                                );
                                let new_pool_diff = 0xFFFFFFFFu64
                                    / new_template.difficulty_target as u64;
                                let new_target = format!("{}", new_pool_diff);

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
                                            JsonValue::from(39_f64),
                                        ),
                                    ]));

                                *self.current_linear_template.lock().await =
                                    Some(new_template);

                                let notification = dwow::rpc::jsonrpc::JsonNotification::new(
                                    "job", job_params,
                                );
                                publisher.notify(notification).await;

                                info!(
                                    target: "dwowd::rpc::rpc_stratum::stratum_submit",
                                    "[RPC-STRATUM] Pushed new mining job: height={}",
                                    new_height,
                                );
                            }
                            Err(e) => {
                                error!(
                                    target: "dwowd::rpc::rpc_stratum::stratum_submit",
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
                    target: "dwowd::rpc::rpc_stratum::stratum_submit",
                    "[RPC-STRATUM] Block rejected: {e}",
                );
                miner_status_response(id, "rejected")
            }
        }
    }
}
