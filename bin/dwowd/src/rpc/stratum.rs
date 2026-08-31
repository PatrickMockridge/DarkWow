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
};

use async_trait::async_trait;
use smol::lock::MutexGuard;
use tinyjson::JsonValue;
use tracing::{debug, error, info};

use dwow_core::{
    rpc::{
        jsonrpc::{
            ErrorCode, ErrorCode::InvalidParams, JsonError, JsonRequest, JsonResponse, JsonResult,
            JsonSubscriber,
        },
        server::RequestHandler,
    },
    concurrency::{Publisher, StoppableTaskPtr},
};

use dwow_chain::fee_window::FeeWindowFlags;
use dwow_chain::PowSource;
use dwow_sdk::blockchain::{BlockHeight, BlockReward, BlockTimestamp, BlockVersion, MoneroBlockHeight};

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
    #[expect(clippy::unwrap_used, reason = "serialization of a JsonValue into a String is infallible")]
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
        if crate::SyncState::load(&self.mining_state.sync_state) != crate::SyncState::CaughtUp {
            return server_error(RpcError::NodeNotSynced, id, None);
        }

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

        let chain_state = match self.chain_state.as_ref() {
            Some(c) => c,
            None => return server_error(RpcError::MinerMissingPassword, id, None),
        };

        // Coinbase recipient is ALWAYS this node's own declared key (decision:
        // one miner, one key — no external/forwarded recipient). The stratum login
        // `wallet` parameter is NOT used as the reward target; rewards accrue to the
        // node's declared key and move elsewhere only via a later transfer.
        let _ = wallet; // login param retained for protocol compat; not a reward target
        if !wallet.trim().is_empty() {
            tracing::warn!(target: "dwowd::rpc::rpc_stratum",
                "Ignoring stratum login wallet '{}': node mines only to its own declared key (one miner, one key)",
                wallet);
        }
        let height = chain_state.get_height().succ();
        let config = match LinearMinerRewardsRecipientConfig::from_account(
            &*self.account_manager.read().await, height,
        ) {
            Ok(c) => c,
            Err(e) => return server_error(e, id, None),
        };

        // Generate unique client ID
        #[expect(clippy::unwrap_used, reason = "system clock is always after UNIX_EPOCH")]
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let client_id = format!("{}-{}", agent.replace("/", "-"), timestamp);

        // Lazily initialize ZK proving materials for linear coinbase
        let linear_zk = {
            let mut zk_lock = self.mining_state.linear_zk.lock().await;
            if zk_lock.is_none() {
                match crate::registry::model::LinearPowRewardZk::new(
                    chain_state.clone(),
                ).await {
                    Ok(zk) => *zk_lock = Some(crate::registry::model::RequiredLinearZk::new(Some(zk))),
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
        #[expect(clippy::expect_used, reason = "linear_zk is initialized before template generation (lazy-init invariant)")]
        let zk_ref = linear_zk.as_ref().expect("ZK must be initialized before template generation");

        // Drain mempool for transaction inclusion in this block template
        let mempool_txs = match &self.mempool {
            Some(mp) => mp.select_for_block(&self.mining_state.miner_config).await,
            None => vec![],
        };

        // Collect uncles from previous height — matches Python miner_cycle
        let uncles: Vec<dwow_chain::UncleBlock> = match chain_state.get_latest_block() {
            Ok(latest) => {
                let competing = chain_state.take_competing_blocks(latest.header.height);
                competing.iter().map(|block| {
                    dwow_chain::UncleBlock {
                        header: block.header.clone(),
                        transactions: block.transactions.clone(),
                        depth: 1,
                        pin_offered: false,
                        pin_accepted: false,
                        pin_confirmed: BlockReward::ZERO,
                    }
                }).collect()
            }
            Err(_) => vec![],
        };

        // Generate block template with collected uncles
        let template = match generate_linear_block_template(
            chain_state, &config, zk_ref, mempool_txs, uncles,
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
        *self.mining_state.current_linear_template.lock().await = Some(template.clone());
        self.mining_state.template_height.set(chain_state.get_height());
        *self.mining_state.linear_recipient_config.lock().await = Some(config);

        // Create or reuse shared publisher for push notifications
        #[expect(clippy::unwrap_used, reason = "publisher is Some after the is_none guard above")]
        let publisher = {
            let mut lock = self.mining_state.linear_stratum_publisher.lock().await;
            if lock.is_none() {
                *lock = Some(Publisher::new());
            }
            lock.as_ref().unwrap().clone()
        };

        let job_id = format!("linear-job-{}", template.height);
        let randomx_key = dwow_chain::Miner::derive_key_from_height(template.height);
        let seed_hash = hex::encode(randomx_key);

        // Build mining blob from block header (nonce=0 placeholder)
        let mining_header = dwow_chain::BlockHeader {
            version: BlockVersion::CURRENT,
            previous: blake3::Hash::from_bytes(template.previous),
            merkle_root: blake3::hash(&[]),
            timestamp: BlockTimestamp::new(template.timestamp),
            target: template.target,
            nonce: 0,
            height: template.height,
            uncle_merkle_root: [0u8; 32],
            total_reward: template.value,
            randomx_key,
            miner: [0u8; 32],
            commitment_merkle_root: template.commitment_merkle_root,
            nullifier_root: template.nullifier_root,
            anchor_tx_id: [0u8; 32],
            anchor_monero_height: MoneroBlockHeight::new(0),
            anchor_monero_hash: [0u8; 32],
            finality_flags: 0,
            fee_window_flags: FeeWindowFlags::default(),

        pow_source: PowSource::Native,

        };
        let blob_data = mining_header.to_mining_blob();
        let blob = hex::encode(&blob_data);
        info!(
            target: "dwowd::rpc::rpc_stratum::stratum_login",
            "[RPC-STRATUM] Login blob (to xmrig): {blob}",
        );

        // Target encoding bridge (32-bit daemon check ↔ 64-bit xmrig check):
        // - Daemon: u32_le(hash[0..4]) <= target
        // - xmrig:  u64_le(hash[0..8]) <= strtoull(target_hex)
        // Upper 32 bits = 0xFFFFFFFF so xmrig ignores bytes[4..7] (any u32
        // value fits); lower 32 bits = target for the precise check.
        let target = format!("FFFFFFFF{:08x}", template.target);

        info!(
            target: "dwowd::rpc::rpc_stratum::stratum_login",
            "[RPC-STRATUM] Created mining job for {client_id}: height={}, job_id={job_id}",
            template.height,
        );

        // --- Stratum login response ---
        //
        // xmrig's StratumClient::onLoginResponse checks:
        //   1. response["error"] is absent or null            → "error":null
        //   2. response["result"] exists                      → "result":{...}
        //   3. response["result"]["id"] exists and is string   → "id":"client-id"
        //   4. response["result"]["job"] exists                → "job":{...}
        //   5. response["result"]["status"] == "OK"            → "status":"OK"
        //
        // All integer fields use raw format (no ".0" suffix) because rapidjson
        // GetUint64/GetInt on a float value triggers an assertion or returns 0.
        //
        // The JSON-RPC id field wraps the stratum result. xmrig 6.22.2
        // accepts this because it routes responses by method, not by id.
        let raw_json = format!(
            concat!(
                r#"{{"jsonrpc":"2.0","id":{},"#,
                r#""result":{{"id":"{}","job":{{"blob":"{}","job_id":"{}","#,
                r#""target":"{}","algo":"rx/0","seed_hash":"{}","#,
                r#""height":{},"reserved_offset":39}},"status":"OK"}},"#,
                r#""error":null}}"#,
            ),
            id,         // JSON-RPC request id (u16 → integer)
            client_id,  // stratum client id (string)
            blob,       // hex-encoded 227-byte mining blob (string)
            job_id,     // "linear-job-{height}" (string)
            target,     // pool difficulty (decimal string)
            seed_hash,  // hex-encoded RandomX key (string)
            template.height, // block height (u64 → integer)
        );

        let subscriber = JsonSubscriber {
            method: "job",
            publisher,
        };
        JsonResult::StratumReply(raw_json.into_bytes(), subscriber)
    }

    /// Stratum submit — linear-only path.
    ///
    /// Parses xmrig solution, reconstructs the block with the found nonce,
    /// verifies PoW via the RandomX VM, and inserts the block if valid.
    pub async fn stratum_submit(&self, id: u16, params: JsonValue) -> JsonResult {
        if crate::SyncState::load(&self.mining_state.sync_state) != crate::SyncState::CaughtUp {
            return server_error(RpcError::NodeNotSynced, id, None);
        }

        use crate::registry::model::generate_linear_block_template;
        use dwow_chain::caribina::anchor_block;

        info!(
            target: "dwowd::rpc::rpc_stratum::stratum_submit",
            "[RPC-STRATUM] stratum_submit called id={}",
            id,
        );

        // Serialize submissions to prevent concurrent RandomX VM access
        let _submit_guard = self.mining_state.linear_submit_lock.lock().await;

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
        let nonce = match nonce_bytes.try_into() {
            Ok(arr) => u32::from_le_bytes(arr),
            Err(_) => return server_error(RpcError::MinerInvalidNonce, id, None),
        };

        // Parse result (RandomX hash) for logging
        let xmrig_result = params.get("result").and_then(|r| r.get::<String>());

        info!(
            target: "dwowd::rpc::rpc_stratum::stratum_submit",
            "[RPC-STRATUM] Got solution from client {client_id} for job: {job_id}",
        );

        let chain_state = match self.chain_state.as_ref() {
            Some(c) => c,
            None => return miner_status_response(id, "rejected"),
        };

        // Rate limit blocks
        #[expect(clippy::unwrap_used, reason = "system clock is always after UNIX_EPOCH")]
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let last_time = self.mining_state.last_block_time.get().get(); // G3: rate-limit comparison
        if last_time > 0 && now.saturating_sub(last_time) < self.min_block_interval {
            info!(
                target: "dwowd::rpc::rpc_stratum::stratum_submit",
                "[RPC-STRATUM] Rate-limited: {}s since last block (min {})",
                now.saturating_sub(last_time), self.min_block_interval
            );
            return miner_status_response(id, "stale")
        }

        // Validate job height
        let current_height = chain_state.get_height();
        let submitted_height: BlockHeight = job_id
            .trim_start_matches("linear-job-")
            .parse()
            .map(BlockHeight::new)
            .unwrap_or(current_height.succ());

        if submitted_height != current_height.succ() {
            info!(
                target: "dwowd::rpc::rpc_stratum::stratum_submit",
                "[RPC-STRATUM] Stale: submitted height {} != expected {}",
                submitted_height, current_height.succ()
            );
            return miner_status_response(id, "stale")
        }

        let randomx_key = dwow_chain::Miner::derive_key_from_height(submitted_height);
        let target = {
            #[expect(clippy::unwrap_used, reason = "mutex is never poisoned")]
            let consensus = chain_state.consensus.lock().unwrap();
            consensus.target()
        };

        // Build previous hash using previous block's RandomX key
        #[expect(clippy::expect_used, reason = "RandomX hash failure surfaces via panic (see safety.md C1)")]
        let previous_hash = if submitted_height == BlockHeight::GENESIS {
            blake3::Hash::from_bytes([0u8; 32])
        } else {
            match chain_state.get_latest_block() {
                Ok(block) => {
                    let _prev_key = block.header.randomx_key;
                    chain_state.hash_block_with_cached_vm(&block).expect("hash failed")
                }
                Err(_) => blake3::Hash::from_bytes([0u8; 32]),
            }
        };

        // Load stored template for ZK coinbase data and timestamp.
        // Timestamp MUST match the mining blob that xmrig hashed.
        let template = self.mining_state.current_linear_template.lock().await.clone();
        let template_timestamp = template.as_ref().map(|t| t.timestamp).unwrap_or(now);
        #[expect(clippy::expect_used, reason = "nullifier is Some when zk_proof is present (ZK circuit path)")]
        let (_coinbase, pow_reward_call_data, commitment_merkle_root, nullifier_root) = if let Some(ref tmpl) = template {
            let call_data = tmpl.pow_reward_call_data.clone();
            if !tmpl.zk_proof.is_empty() {
                let cb = dwow_chain::CoinbaseTransaction {
                    proof: tmpl.zk_proof.clone(),
                    public_inputs: dwow_chain::ZkPublicInputs(tmpl.zk_public_inputs),
                    commitment: tmpl.commitment,
                    value_commit_x: tmpl.value_commit_x,
                    value_commit_y: tmpl.value_commit_y,
                    token_commit: tmpl.token_commit,
                    nullifier: tmpl.nullifier
                        .expect("Nullifier must be set in ZK circuit path; None is dev-fallback only"),
                    new_cumulative_x: tmpl.new_cumulative_x,
                    new_cumulative_y: tmpl.new_cumulative_y,
                    encrypted_note: tmpl.encrypted_note.clone(),
                };
                (Some(cb), call_data, tmpl.commitment_merkle_root, tmpl.nullifier_root)
            } else {
                (None, vec![], [0u8; 32], [0u8; 32])
            }
        } else {
            (None, vec![], [0u8; 32], [0u8; 32])
        };

        let reward = dwow_sdk::blockchain::expected_reward(submitted_height);

        // Use template's merkle root and transactions (frozen at login time)
        let (merkle_root, template_txs) = template.as_ref()
            .map(|t| (t.merkle_root, t.transactions.clone()))
            .unwrap_or_else(|| (blake3::hash(&[]), vec![]));

        let header = dwow_chain::BlockHeader {
            fee_window_flags: FeeWindowFlags::default(),
            version: BlockVersion::CURRENT,
            previous: previous_hash,
            merkle_root,
            timestamp: BlockTimestamp::new(template_timestamp),
            target,
            nonce,
            height: submitted_height,
            uncle_merkle_root: [0u8; 32],
            total_reward: reward,
            randomx_key,
            miner: [0u8; 32],
            commitment_merkle_root,
            nullifier_root,
            anchor_tx_id: [0u8; 32],
            anchor_monero_height: MoneroBlockHeight::new(0),
            anchor_monero_hash: [0u8; 32],
            finality_flags: 0,

        pow_source: PowSource::Native,

        };

        let coinbase_tx = dwow_chain::Transaction {
            version: BlockVersion::CURRENT,
            inputs: vec![],
            outputs: vec![dwow_chain::TxOutput {
                value: reward.get(),
                script: vec![],
            }],
            contract_calls: if pow_reward_call_data.is_empty() {
                vec![]
            } else {
                vec![dwow_chain::ContractCall {
                    contract_id: *dwow_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID,
                    data: pow_reward_call_data,
                }]
            },
            lock_time: 0,
            ..Default::default()
        };

        // Combine template transactions with coinbase
        let mut all_txs = template_txs;
        all_txs.insert(0, coinbase_tx);

        let mut block = dwow_chain::Block {
            header,
            transactions: all_txs,
        };

        // Verify PoW before inserting
        {
            let submit_blob = block.header.to_mining_blob();
            #[expect(clippy::expect_used, reason = "RandomX hash failure surfaces via panic (see safety.md C1)")]
            let daemon_hash = chain_state.hash_block_with_cached_vm(&block).expect("hash failed");
            info!(
                target: "dwowd::rpc::rpc_stratum::stratum_submit",
                "[RPC-STRATUM] Submit — nonce={}, blob={}, daemon_hash={}, xmrig_hash={}",
                nonce,
                hex::encode(&submit_blob),
                hex::encode(daemon_hash.as_bytes()),
                xmrig_result.as_ref().map(|s| s.as_str()).unwrap_or("none"),
            );
            #[expect(clippy::expect_used, reason = "RandomX hash failure surfaces via panic (see safety.md C1)")]
            let vm = chain_state.get_vm(randomx_key)
                .expect("Failed to get RandomX VM for stratum");
            #[expect(clippy::unwrap_used, reason = "mutex is never poisoned")]
            let verify_result = {
                let guard = vm.lock().unwrap();
                chain_state.consensus.lock().unwrap().verify_proof(&block, &*guard)
            };
            match verify_result {
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
            let fc = &chain_state.finality_config;
            if fc.should_anchor() {
                #[expect(clippy::expect_used, reason = "RandomX hash failure surfaces via panic (see safety.md C1)")]
                let block_hash = chain_state.hash_block_with_cached_vm(&block).expect("hash failed");
                let mut block_hash_bytes = [0u8; 32];
                block_hash_bytes.copy_from_slice(block_hash.as_bytes());
                match anchor_block(&block_hash_bytes, block.header.timestamp.get(), block.header.height) {
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

        // Apply block with uncles from the stored template
        let uncles: Vec<dwow_chain::UncleBlock> = {
            let tmpl = self.mining_state.current_linear_template.lock().await;
            tmpl.as_ref().map(|t| t.uncles.clone()).unwrap_or_default()
        };

        // Accept block — single unified path (block_acceptor::accept_block).
        // Use pooled RandomXCache — 256 MB allocation reused.
        let flags = randomx::RandomXFlags::get_recommended_flags() & !randomx::RandomXFlags::JIT;
        #[expect(clippy::expect_used, reason = "RandomX hash failure surfaces via panic (see safety.md C1)")]
        let exec_rx_cache = chain_state.get_cache(randomx_key)
            .expect("Failed to get RandomX cache for stratum execution");
        #[expect(clippy::expect_used, reason = "RandomX hash failure surfaces via panic (see safety.md C1)")]
        let exec_vm = std::sync::Arc::new(
            randomx::RandomXVM::new(flags, Some(exec_rx_cache), None)
                .expect("Failed to create RandomX VM for stratum execution"),
        );

        let current_h = chain_state.get_height();
        match crate::block_acceptor::accept_block(
            &chain_state, &block, &uncles, &exec_vm, current_h, block.header.target, None,
        ) {
            Ok(dwow_chain::BlockConnectOutcome::CanonicalExtension { .. }) => {
                drop(exec_vm);
                self.mining_state.last_block_time.set_now();

                info!(
                    target: "dwowd::rpc::rpc_stratum::stratum_submit",
                    "[RPC-STRATUM] Block at height {} accepted!",
                    submitted_height
                );

                // HAZID F5: Remove mined transactions from mempool.
                // ONLY for canonical blocks — competing/uncle blocks do NOT advance the chain.
                if let Some(ref mp) = self.mempool {
                    let tx_hashes: Vec<blake3::Hash> = block.transactions.iter()
                        .map(|tx| tx.hash()).collect();
                    mp.mark_mined(&tx_hashes).await;
                }

                // Push new mining job to all connected miners
                if let Some(ref publisher) = *self.mining_state.linear_stratum_publisher.lock().await {
                    if let Some(ref recipient_config) = *self.mining_state.linear_recipient_config.lock().await {
                        let linear_zk = {
                            let zk_lock = self.mining_state.linear_zk.lock().await;
                            zk_lock.clone()
                        };
                        #[expect(clippy::expect_used, reason = "linear_zk is initialized before mining (lazy-init invariant)")]
                        let zk_ref = linear_zk.as_ref().expect("ZK must be initialized");

                        let effective_recipient = recipient_config.clone();

                        // Drain mempool for the next block template
                        let next_mempool_txs = match &self.mempool {
                            Some(mp) => mp.select_for_block(&self.mining_state.miner_config).await,
                            None => vec![],
                        };

                        match generate_linear_block_template(
                            chain_state,
                            &effective_recipient,
                            zk_ref,
                            next_mempool_txs,
                            vec![],
                        )
                        .await
                        {
                            Ok(new_template) => {
                                let new_height = new_template.height;
                                let new_job_id = format!("linear-job-{}", new_height);
                                let new_randomx_key =
                                    dwow_chain::Miner::derive_key_from_height(new_height);
                                let new_seed_hash = hex::encode(new_randomx_key);

                                let new_mining_header = dwow_chain::BlockHeader {
                                    version: BlockVersion::CURRENT,
                                    previous: blake3::Hash::from_bytes(new_template.previous),
                                    merkle_root: new_template.merkle_root,
                                    timestamp: BlockTimestamp::new(new_template.timestamp),
                                    target: new_template.target,
                                    nonce: 0,
                                    height: new_height,
                                    uncle_merkle_root: [0u8; 32],
                                    total_reward: new_template.value,
                                    randomx_key: new_randomx_key,
                                    miner: [0u8; 32],
                                    commitment_merkle_root: new_template.commitment_merkle_root,
                                    nullifier_root: new_template.nullifier_root,
                                    anchor_tx_id: [0u8; 32],
                                    anchor_monero_height: MoneroBlockHeight::new(0),
                                    anchor_monero_hash: [0u8; 32],
                                    finality_flags: 0,
                                    fee_window_flags: FeeWindowFlags::default(),

                                pow_source: PowSource::Native,

                                };
                                let new_blob_data = new_mining_header.to_mining_blob();
                                let new_blob = hex::encode(&new_blob_data);
                                info!(
                                    target: "dwowd::rpc::rpc_stratum::stratum_submit",
                                    "[RPC-STRATUM] Push blob (to xmrig): {new_blob}",
                                );
                                let new_target = format!("FFFFFFFF{:08x}", new_template.target);

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
                                            JsonValue::String(format!("{}", new_height)),
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

                                *self.mining_state.current_linear_template.lock().await =
                                    Some(new_template);
                                self.mining_state.template_height.set(chain_state.get_height());

                                let notification = dwow_core::rpc::jsonrpc::JsonNotification::new(
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
            Ok(_outcome) => {
                // Block was stored as competing or uncle extension —
                // peer beat us to this height. Mempool unchanged.
                info!(
                    target: "dwowd::rpc::rpc_stratum::stratum_submit",
                    "[RPC-STRATUM] Block at height {} stored as {:?} — peer beat us",
                    submitted_height, _outcome,
                );
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
