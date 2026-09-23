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
use tracing::{debug, error, info, warn};

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

        // No ZK materials needed: coinbase/uncle/fee-collect are all plaintext.
        // UNVERIFIED(F2-8): needs cargo test -p dwowd --lib

        // Drain mempool for transaction inclusion in this block template
        let mempool_txs = match &self.mempool {
            Some(mp) => mp.select_for_block(&self.mining_state.miner_config).await,
            None => vec![],
        };

        // Collect uncles from previous height — matches Python miner_cycle.
        // Save original blocks for error recovery (validate-then-mutate pattern).
        let (uncles, competing_originals) = match chain_state.get_latest_block() {
            Ok(latest) => {
                let latest_height = latest.header.height;
                let competing = chain_state.take_competing_blocks(latest_height);
                let base_reward = dwow_sdk::blockchain::expected_reward(latest_height.succ());
                let uncle_blocks: Vec<dwow_chain::UncleBlock> = competing.iter().map(|block| {
                    let depth = dwow_chain::UncleBlock::depth_for(latest_height.succ(), block.header.height);
                    let mut uncle = dwow_chain::create_uncle(block.clone(), depth, base_reward);
                    uncle.accept_pin(); // "rejection is strictly dominated" — always accept
                    uncle
                }).collect();
                (uncle_blocks, competing)
            }
            Err(_) => (vec![], vec![]),
        };

        // Generate block template with collected uncles
        let template = match generate_linear_block_template(
            chain_state, &config, mempool_txs, uncles,
        ).await {
            Ok(t) => t,
            Err(e) => {
                error!(
                    target: "dwowd::rpc::rpc_stratum::stratum_login",
                    "[RPC-STRATUM] Failed to generate linear block template: {e}",
                );
                // UNVERIFIED(HYG-5-1): needs cargo check -p dwowd -j 2 && cargo test
                // -p dwowd --lib --test-threads=2 (competing originals taken above are
                // re-inserted on template failure, matching mm_rpc's HAZOP #5 —
                // previously the take was lost, forfeiting the competing miner's
                // uncle reward)
                if !competing_originals.is_empty() {
                    let latest = chain_state.get_height();
                    chain_state.put_competing_blocks(latest, competing_originals);
                }
                return JsonResponse::new(JsonValue::from(HashMap::new()), id).into()
            }
        };

        // Store template, height and recipient as ONE value for the submit handler (`OBL-C43`/`OBL-C54`):
        // three separate assignments let a submit observe a new template with the previous round's
        // recipient or height.
        self.mining_state
            .store_template(template.clone(), chain_state.get_height(), config)
            .await;

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
            miner: template.miner,
            // The per-block anchor key, from the same template the submit path reads. It MUST appear
            // here as well as there: it is inside the mining blob, so a value that differed between
            // this blob and the submitted header would make the found nonce fail PoW verification.
            anchor_owner: template.anchor_wallet.public_key(),
            // Zeroed roots (see submit path); recomputed by WASM execution.
            commitment_merkle_root: [0u8; 32],
            nullifier_root: [0u8; 32],
            anchor_tx_id: [0u8; 32],
            // No proof yet — it is built at submit, after the nonce is known.
            caribina_anchor: None,
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
            blob,       // hex-encoded 292-byte mining blob (string)
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

        info!(
            target: "dwowd::rpc::rpc_stratum::stratum_submit",
            "[RPC-STRATUM] stratum_submit called id={}",
            id,
        );

        // Serialize submissions to prevent concurrent RandomX VM access.
        //
        // `mut` because the guard is released and re-taken around the Arweave anchor POST — see the
        // comment at the `drop` below (OBL-C68). Everything that touches the VM must run under it.
        let mut submit_guard = self.mining_state.linear_submit_lock.lock().await;

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
                    chain_state.hash_block_with_cached_vm(&block).expect("hash failed")
                }
                Err(_) => blake3::Hash::from_bytes([0u8; 32]),
            }
        };

        // Load the active template for the PoWRewardV1 call data and timestamp.
        // Timestamp MUST match the mining blob that xmrig hashed.
        //
        // **One snapshot for the whole submission** (`OBL-C43`/`OBL-C54`). The height, the recipient and
        // the per-block anchor key travel with the template, and everything below — the PoW data, the
        // anchor wallet, the uncles, the next round's recipient — reads this one clone rather than
        // re-acquiring the lock. Three separate acquisitions were what let a login land between two of
        // them and pair one round's template with another round's key, which the removed branch further
        // down used to report as "the live template moved between login and submit".
        //
        // Cloned rather than held: the guard must not live across the awaits below.
        let active = self.mining_state.active_template.lock().await.clone();
        let template = active.as_ref().map(|active| active.template.clone());
        let template_timestamp = template.as_ref().map(|t| t.timestamp).unwrap_or(now);
        // Since b6bf44f79 the coinbase is a plaintext contract call — no ZK
        // proof rides in the template; only the pre-built call data is needed.
        let pow_reward_call_data = template
            .as_ref()
            .map(|tmpl| tmpl.pow_reward_call_data.clone())
            .unwrap_or_default();

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
            total_reward: template.as_ref().map(|t| t.value).unwrap_or(reward),
            randomx_key,
            miner: template.as_ref().map(|t| t.miner).unwrap_or([0u8; 32]),
            // Same source as the login blob's: both read the live template, so the value xmrig
            // hashed and the value submitted agree.
            anchor_owner: template.as_ref().map(|t| t.anchor_wallet.public_key()).unwrap_or([0u8; 32]),
            // Zeroed roots match the built-in miner path (lib.rs): header roots
            // are not validated pre-commit; the WASM execution recomputes them.
            commitment_merkle_root: [0u8; 32],
            nullifier_root: [0u8; 32],
            anchor_tx_id: [0u8; 32],
            // Built below, once the nonce is known and PoW has been verified: the proof binds the
            // header minus its post-mining fields, so it cannot exist before the nonce does.
            caribina_anchor: None,
            anchor_monero_height: MoneroBlockHeight::new(0),
            anchor_monero_hash: [0u8; 32],
            finality_flags: 0,

        pow_source: PowSource::Native,

        };

        let coinbase_tx =
            crate::registry::model::plaintext_coinbase_transaction(pow_reward_call_data, reward);

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

        // Build the Caribina anchor proof, then publish it best-effort.
        //
        // **Order matters, and it changed on 2026-09-22.** The proof is built first — a pure,
        // local, infallible step — and *attached to the header* before `accept_block`, because
        // `chain_state`'s enforcement requires a verified proof to confer finality (OBL-C63).
        // Publication is then attempted separately: it is a network call that may fail, and a block
        // whose publication failed is still valid, merely unanchored. Previously this block set only
        // `anchor_tx_id` — a field nothing now consults — so an anchored block was indistinguishable
        // from an unanchored one.
        {
            let fc = &chain_state.finality_config;
            if fc.should_anchor() {
                // The template's per-block key, whose public half the header already commits. Taken from
                // the submission's snapshot, so it is the key of the template this block was built from
                // (`OBL-C43`/`OBL-C54`) — it used to be a second lock acquisition, and *that* was the
                // whole of the race the branch below described.
                let anchor_wallet = active
                    .as_ref()
                    .map(|active| active.template.anchor_wallet.clone());
                if let Some(wallet) = anchor_wallet {
                    if block.header.anchor_owner == wallet.public_key() {
                        block.header.caribina_anchor = Some(
                            dwow_chain::caribina::build_anchor_proof(&block.header, &wallet),
                        );
                        block.header.finality_flags = fc.mine_flags();
                        debug_assert!(
                            dwow_chain::caribina::verify_anchor_proof(&block.header),
                            "a proof this node just built must verify — if not, the miner and the \
                             verifier disagree about the commitment and no block will ever be final"
                        );
                    } else {
                        // Unreachable from this handler now, and kept as a guard rather than deleted: the
                        // snapshot and the block were built from the same template, so the owner can only
                        // differ if a block arrived from somewhere else. It used to fire whenever a login
                        // landed between the template read and this one; leaving such a block unanchored
                        // is still the right answer, and `accept_block` rejects it as stale in any case.
                        info!(
                            target: "dwowd::rpc::rpc_stratum::stratum_submit",
                            "[RPC-STRATUM] Block {} commits anchor_owner {:?} but the snapshot's template \
                             holds a different key — leaving it unanchored",
                            block.header.height, block.header.anchor_owner
                        );
                    }
                }
            }
        }

        // Publish the proof to Arweave (best-effort), with the submission lock released.
        {
            let fc = &chain_state.finality_config;
            if fc.should_anchor() {
                let proof = block.header.caribina_anchor.clone();
                if let Some(proof) = proof {
                let proof_len = proof.len();

                // Release the submission lock for the network call (OBL-C68).
                //
                // The lock exists to serialise RandomX VM access, and the hashing above needed it —
                // but publishing touches no VM. It performs a blocking HTTP POST with a 30-second
                // end-to-end deadline (`ureq`'s `timeout_global`, which the crate documents as covering
                // DNS through response body), and holding the lock across it serialised *every* other
                // stratum submission behind one stuck anchor while also blocking the async executor
                // thread. Do not move this call back inside the lock.
                //
                // Releasing it here is safe: between the drop and the re-acquire only the POST and a
                // local `anchor_tx_id` assignment run, and a submission that interleaves is rejected by
                // `accept_block`'s own height check — the same outcome serialisation would have produced.
                drop(submit_guard);

                match dwow_chain::caribina::publish_anchor_proof(&proof) {
                    Some(tx_id) => {
                        // Informational: the id is a convenience handle for looking the DataItem up on
                        // a gateway. It is *not* what confers finality — `caribina_anchor` above is.
                        block.header.anchor_tx_id = tx_id;
                        info!(
                            target: "dwowd::rpc::rpc_stratum::stratum_submit",
                            "[RPC-STRATUM] Published anchor proof for block {} (tx {})",
                            block.header.height, hex::encode(tx_id)
                        );
                    }
                    None => {
                        // The block keeps its proof, and therefore its finality; only the Arweave
                        // publication is missing. Logged at warn because it is a real degradation of
                        // the external anchor, not a routine skip.
                        warn!(
                            target: "dwowd::rpc::rpc_stratum::stratum_submit",
                            "[RPC-STRATUM] Block {} is anchored but not published ({} bytes) — Arweave \
                             publication failed; finality is local-only until it is re-published",
                            block.header.height, proof_len
                        );
                    }
                }

                // Re-take it before `accept_block`, which needs the VM.
                submit_guard = self.mining_state.linear_submit_lock.lock().await;
                }
            }
        }

        // Apply block with uncles from the submission's snapshot (`OBL-C43`/`OBL-C54`)
        let uncles: Vec<dwow_chain::UncleBlock> = active
            .as_ref()
            .map(|active| active.template.uncles.clone())
            .unwrap_or_default();

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

        match crate::block_acceptor::accept_block(
            &chain_state, &block, &uncles, &exec_vm, block.header.target, None,
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

                // Broadcast the accepted block to peers (OBL-C35 / HAZID H-C2).
                //
                // This path committed blocks locally and never propagated them, so the network
                // learned of a stratum-mined block only when a peer's 30-second poll happened to
                // fetch it. Merge mining (`rpc/mm_rpc.rs`) and both other miner paths already
                // broadcast; stratum was the last one that did not. The uncles travel with the
                // block, as they do on the built-in miner's path (`rpc/miner.rs`) rather than as
                // the empty vector `mm_rpc` passes — they are the ones this block's template
                // carried, loaded above for `accept_block`.
                crate::proto::linear_broadcast::broadcast_block(
                    &self.p2p_handler.p2p, block.clone(), uncles.clone()).await;

                // Push new mining job to all connected miners.
                //
                // The recipient comes from the **submission's own snapshot** (`OBL-C43`/`OBL-C54`), so the
                // next round pays the same recipient as the round just submitted. The `if let` can only
                // fail before a first login, when there is no template to have just submitted against; it
                // used to depend on whether some login had happened to store a config elsewhere.
                if let Some(ref publisher) = *self.mining_state.linear_stratum_publisher.lock().await {
                    if let Some(effective_recipient) = active
                        .as_ref()
                        .map(|active| active.recipient_config.clone())
                    {

                        // Drain mempool for the next block template
                        let next_mempool_txs = match &self.mempool {
                            Some(mp) => mp.select_for_block(&self.mining_state.miner_config).await,
                            None => vec![],
                        };

                        // No ZK materials needed — plaintext template.
                        // UNVERIFIED(F2-9): needs cargo test -p dwowd --lib
                        match generate_linear_block_template(
                            chain_state,
                            &effective_recipient,
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
                                    miner: new_template.miner,
                                    // The refreshed template's own anchor key — a new block gets a new
                                    // one, and the submit path will read whichever template is live.
                                    anchor_owner: new_template.anchor_wallet.public_key(),
                                    // Zeroed roots (see submit path); recomputed by WASM execution.
                                    commitment_merkle_root: [0u8; 32],
                                    nullifier_root: [0u8; 32],
                                    anchor_tx_id: [0u8; 32],
                                    caribina_anchor: None,
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

                                // Published together with the recipient it was built from, so the next
                                // submission observes one round's snapshot (`OBL-C43`/`OBL-C54`).
                                self.mining_state
                                    .store_template(
                                        new_template,
                                        chain_state.get_height(),
                                        effective_recipient,
                                    )
                                    .await;

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
