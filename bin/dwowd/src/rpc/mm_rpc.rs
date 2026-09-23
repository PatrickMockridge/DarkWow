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

//! Merge mining JSON-RPC handler.
//!
//! Implements the Monero p2pool merge mining protocol.
//!
//! Reference: <https://github.com/SChernykh/p2pool/blob/master/docs/MERGE_MINING.MD>

use std::{
    collections::{HashMap, HashSet},
    sync::atomic::Ordering,
};

use async_trait::async_trait;
use smol::lock::MutexGuard;
use tinyjson::JsonValue;
use tracing::{debug, error, info};

use dwow_core::{
    rpc::{
        jsonrpc::{
            ErrorCode, ErrorCode::InvalidParams, JsonError, JsonRequest, JsonResponse, JsonResult,
        },
        server::RequestHandler,
    },
    concurrency::StoppableTaskPtr,
};
use dwow_chain::fee_window::FeeWindowFlags;
use dwow_chain::{
    monero::{
        fixed_array::FixedByteArray,
        extract_aux_merkle_root_from_block,
        merkle_proof::MerkleProof,
        monero_block_deserialize, JobId,
        MoneroPowData,
    },
    PowSource,
};
use dwow_sdk::blockchain::{BlockHeight, BlockReward, BlockTimestamp, BlockVersion, MoneroBlockHeight};

use crate::{error::{miner_status_response, server_error, RpcError}, DwowNode};

/// Evict the **oldest** entry from a merge-mining job table at capacity (`OBL-C55`).
///
/// `merge-mining-ffi.md` §4.4 requires the oldest entry to be evicted, not the whole table and not an
/// arbitrary one. Both job tables therefore carry an insertion sequence as their value — `u64` from
/// `MiningState::mm_jobs_seq` — because a `HashMap`'s and a `HashSet`'s iteration order is arbitrary:
/// `keys().next()` (and `iter().next()`) evicted *some* job, and the comment beside one of those sites
/// said so while the site next to it, and the test that named this rule, both claimed FIFO.
///
/// Returns whether an entry was evicted, so a caller can assert on it.
fn evict_oldest<K: std::hash::Hash + Eq + Clone>(table: &mut HashMap<K, u64>, capacity: usize) -> bool {
    if table.len() < capacity {
        return false;
    }
    let Some(oldest) = table.iter().min_by_key(|(_, seq)| **seq).map(|(k, _)| k.clone()) else {
        return false;
    };
    table.remove(&oldest);
    true
}

/// JSON-RPC `RequestHandler` for Merge Mining (p2pool protocol)
pub struct MergeMiningRpcHandler;

#[async_trait]
impl RequestHandler<MergeMiningRpcHandler> for DwowNode {
    #[expect(clippy::unwrap_used, reason = "serialization of a JsonValue into a String is infallible")]
    async fn handle_request(&self, req: JsonRequest) -> JsonResult {
        debug!(target: "dwowd::rpc::mm_rpc", "--> {}", req.stringify().unwrap());

        match req.method.as_str() {
            "merge_mining_get_chain_id" => self.mm_get_chain_id(req.id, req.params).await,
            "merge_mining_get_aux_block" => self.mm_get_aux_block(req.id, req.params).await,
            "merge_mining_submit_solution" => self.mm_submit_solution(req.id, req.params).await,
            _ => JsonError::new(ErrorCode::MethodNotFound, None, req.id).into(),
        }
    }

    async fn connections_mut(&self) -> MutexGuard<'life0, HashSet<StoppableTaskPtr>> {
        self.registry.mm_rpc_connections.lock().await
    }
}

impl DwowNode {
    /// Handle `merge_mining_get_chain_id` — p2pool discovers the aux chain identity.
    ///
    /// Returns: H(genesis_hash || "testnet" || 0u32.to_le_bytes())
    pub async fn mm_get_chain_id(&self, id: u16, params: JsonValue) -> JsonResult {
        // Verify request params
        let Some(params) = params.get::<Vec<JsonValue>>() else {
            return JsonError::new(InvalidParams, None, id).into()
        };
        if !params.is_empty() {
            return JsonError::new(InvalidParams, None, id).into()
        }

        // Grab genesis block hash
        let genesis_hash = {
            let hash = self.mining_state.linear_genesis_hash.lock().await;
            match &*hash {
                Some(h) => *h,
                None => {
                    return JsonError::new(
                        ErrorCode::InternalError,
                        Some("Genesis hash not yet initialized".to_string()),
                        id,
                    )
                    .into()
                }
            }
        };

        // Generate the chain id: blake3(genesis_hash || "testnet" || 0u32.to_le_bytes())
        let mut hasher = blake3::Hasher::new();
        hasher.update(&genesis_hash.0);
        hasher.update("testnet".as_bytes());
        hasher.update(&0u32.to_le_bytes());
        let chain_id = hasher.finalize().to_string();

        info!(
            target: "dwowd::rpc::mm_rpc::mm_get_chain_id",
            "[RPC-MM] get_chain_id: {}",
            chain_id,
        );

        let response = HashMap::from([("chain_id".to_string(), JsonValue::from(chain_id))]);
        JsonResponse::new(JsonValue::from(response), id).into()
    }

    /// Handle `merge_mining_get_aux_block` — p2pool requests aux chain data.
    ///
    /// Returns an empty aux_blob (merge mining data goes in the Monero
    /// coinbase tx_extra, not in a blob). The aux_hash is a job ID that
    /// p2pool sends back on submit.
    pub async fn mm_get_aux_block(&self, id: u16, params: JsonValue) -> JsonResult {
        // Parse request params
        let Some(params) = params.get::<HashMap<String, JsonValue>>() else {
            return JsonError::new(InvalidParams, None, id).into()
        };

        // Parse aux_hash (Monero block hash being merge-mined)
        let Some(aux_hash) = params.get("aux_hash") else {
            return server_error(RpcError::MinerMissingAuxHash, id, None)
        };
        let Some(aux_hash) = aux_hash.get::<String>() else {
            return server_error(RpcError::MinerInvalidAuxHash, id, None)
        };
        let aux_hash = aux_hash.to_string();

        // Convert aux_hash to JobId for dedup table lookup.
        // Monero block hashes are 32-byte Keccak values — any non-zero
        // 32-byte value is a valid JobId per merge-mining-ffi.md §2.1.
        let aux_job_id = {
            let Ok(bytes) = hex::decode(&aux_hash) else {
                return server_error(RpcError::MinerInvalidAuxHash, id, None)
            };
            if bytes.len() != 32 {
                return server_error(RpcError::MinerInvalidAuxHash, id, None)
            };
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&bytes);
            match JobId::from_bytes(arr) {
                Some(j) => j,
                None => return server_error(RpcError::MinerInvalidAuxHash, id, None),
            }
        };

        // Skip duplicate jobs — p2pool polls with the same aux_hash until
        // a solution is found
        {
            let mm_jobs = self.mining_state.mm_jobs.lock().await;
            if mm_jobs.contains_key(&aux_job_id) {
                return JsonResponse::new(JsonValue::from(HashMap::new()), id).into()
            }
        }

        // Parse address (wallet/mining config)
        let Some(wallet) = params.get("address") else {
            return server_error(RpcError::MinerMissingAddress, id, None)
        };
        let Some(wallet) = wallet.get::<String>() else {
            return server_error(RpcError::MinerInvalidAddress, id, None)
        };
        let _wallet = wallet.to_string();
        if !wallet.trim().is_empty() {
            tracing::warn!(target: "dwowd::rpc::mm_rpc",
                "Ignoring caller-supplied wallet '{}': node mines only to its own declared key (one miner, one key)",
                wallet);
        }

        // Parse height (Monero block height)
        let Some(height) = params.get("height") else {
            return server_error(RpcError::MinerMissingHeight, id, None)
        };
        let Some(height) = height.get::<f64>() else {
            return server_error(RpcError::MinerInvalidHeight, id, None)
        };
        // json! macro can't express u64 natively. This value is unused.
        let _height = *height as u64;

        // Parse prev_id (Monero previous block hash)
        let Some(prev_id) = params.get("prev_id") else {
            return server_error(RpcError::MinerMissingPrevId, id, None)
        };
        let Some(prev_id) = prev_id.get::<String>() else {
            return server_error(RpcError::MinerInvalidPrevId, id, None)
        };
        let Ok(_prev_id) = hex::decode(prev_id) else {
            return server_error(RpcError::MinerInvalidPrevId, id, None)
        };

        let chain_state = match self.chain_state.as_ref() {
            Some(c) => c,
            None => {
                return JsonError::new(
                    ErrorCode::InternalError,
                    Some("darkwow-devnet mode only".to_string()),
                    id,
                )
                .into()
            }
        };

        // Coinbase recipient is ALWAYS this node's own declared key (decision:
        // one miner, one key — no external/forwarded recipient). If a config was
        // stored (e.g. by a stratum login) it already holds the node's own key;
        // otherwise resolve it directly from the node's AccountManager. Never a
        // random key.
        let height = chain_state.get_height().succ();
        let recipient_config = {
            // The recipient of the template currently published, if any — this is a cache of the node's
            // own declared key, not an independently-written setting any more (`OBL-C43`/`OBL-C54`).
            let stored = self
                .mining_state
                .active_template
                .lock()
                .await
                .as_ref()
                .map(|active| active.recipient_config.clone());
            match stored {
                Some(config) => config,
                None => match crate::registry::model::LinearMinerRewardsRecipientConfig::from_account(
                    &*self.account_manager.read().await, height,
                ) {
                    Ok(c) => c,
                    Err(_) => {
                        error!(target: "dwowd::rpc::mm_rpc",
                            "Cannot resolve mining recipient from declared key");
                        return JsonError::new(
                            ErrorCode::InternalError,
                            Some("Cannot resolve mining recipient from the node's declared key.".to_string()),
                            id,
                        )
                        .into()
                    }
                },
            }
        };

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

        // No ZK materials needed — plaintext template.
        // UNVERIFIED(F2-10): needs cargo test -p dwowd --lib
        let template = match crate::registry::model::generate_linear_block_template(
            chain_state,
            &recipient_config,
            mempool_txs,
            uncles,
        )
        .await
        {
            Ok(t) => t,
            Err(e) => {
                error!(
                    target: "dwowd::rpc::mm_rpc::mm_get_aux_block",
                    "[RPC-MM] Failed to generate block template: {e}",
                );
                // HAZOP #5: Re-insert competing blocks that were destructively
                // consumed above. Without this, a template generation failure
                // permanently loses the competing miner's uncle reward.
                if !competing_originals.is_empty() {
                    let latest = chain_state.get_height();
                    chain_state.put_competing_blocks(
                        latest,
                        competing_originals,
                    );
                }
                return JsonResponse::new(JsonValue::from(HashMap::new()), id).into()
            }
        };

        // Generate job ID: blake3(template_hash || timestamp)
        let mut job_hasher = blake3::Hasher::new();
        job_hasher.update(&template.previous);
        job_hasher.update(&template.height.to_le_bytes());
        job_hasher.update(template.merkle_root.as_bytes());
        job_hasher.update(&template.timestamp.to_le_bytes());
        let job_hash = job_hasher.finalize();
        #[expect(clippy::expect_used, reason = "blake3 hash is never zero (probability 2^-256)")]
        let job_id = JobId::from_bytes(job_hash.into())
            .expect("blake3 hash is never zero (probability 2^-256)");
        let job_id_hex = job_id.to_hex();

        // Derive difficulty
        let difficulty = {
            #[expect(clippy::unwrap_used, reason = "mutex is never poisoned")]
            let consensus = chain_state.consensus.lock().unwrap();
            let target = consensus.target();
            target.difficulty()
        };

        // Register the job with bounded capacity — prevent unbounded
        // growth in long-running nodes. The oldest job is evicted.
        {
            const MAX_MM_JOBS: usize = 100;
            let mut mm_jobs = self.mining_state.mm_jobs.lock().await;
            // HAZID H-M11: single-entry eviction, not bulk clear(). Previously clear() wiped 100 jobs at
            // once, evicting recently-created valid jobs alongside expired ones. The evicted entry is the
            // **oldest by insertion** (`OBL-C55`): `evict_oldest` reads the sequence value rather than
            // `keys().next()`, which on a HashMap was arbitrary while the comment here, and
            // merge-mining-ffi.md §4.4, both said FIFO.
            evict_oldest(&mut mm_jobs, MAX_MM_JOBS);
            mm_jobs.insert(job_id, self.mining_state.mm_jobs_seq.fetch_add(1, Ordering::Relaxed));
        }

        // Publish the template with the height it was generated at **and the recipient it was built
        // from** (`OBL-C43`/`OBL-C54`). The config used to be dropped here — this call site stored the
        // template and the height and nothing else — which left the submit path's next-round template
        // regeneration (`:780`) guarding on a config that only a stratum login would ever have stored.
        self.mining_state
            .store_template(template, chain_state.get_height(), recipient_config)
            .await;

        info!(
            target: "dwowd::rpc::mm_rpc::mm_get_aux_block",
            "[RPC-MM] Created new merge mining job: aux_hash={}", job_id_hex,
        );

        let response = JsonValue::from(HashMap::from([
            ("aux_blob".to_string(), JsonValue::from(hex::encode(vec![]))),
            ("aux_diff".to_string(), JsonValue::Number(difficulty as f64)),
            ("aux_hash".to_string(), JsonValue::from(job_id_hex)),
        ]));
        JsonResponse::new(response, id).into()
    }

    /// Handle `merge_mining_submit_solution` — p2pool submits a solved Monero block
    /// containing the DarkWow merge mining tag.
    ///
    /// Verifies:
    /// 1. The aux_blob is empty (upstream returns empty)
    /// 2. The aux_hash matches a registered job
    /// 3. The Monero block contains the merge mining tag in coinbase tx_extra
    /// 4. The merkle proof verifies the aux_hash was committed in the Monero block
    /// 5. The coinbase merkle root is valid
    pub async fn mm_submit_solution(&self, id: u16, params: JsonValue) -> JsonResult {
        // Serialize submissions
        let _submit_guard = self.mining_state.linear_submit_lock.lock().await;

        // Parse request params
        let Some(params) = params.get::<HashMap<String, JsonValue>>() else {
            return JsonError::new(InvalidParams, None, id).into()
        };

        // Parse aux_hash (our job ID, returned from mm_get_aux_block)
        let Some(aux_hash) = params.get("aux_hash") else {
            return server_error(RpcError::MinerMissingAuxHash, id, None)
        };
        let Some(aux_hash_hex) = aux_hash.get::<String>() else {
            return server_error(RpcError::MinerInvalidAuxHash, id, None)
        };
        let aux_hash_hex = aux_hash_hex.to_string();

        // Convert aux_hash to JobId — validates hex and non-zero length.
        let job_id = {
            let bytes = match hex::decode(&aux_hash_hex) {
                Ok(b) if b.len() == 32 => b,
                _ => return server_error(RpcError::MinerInvalidAuxHash, id, None),
            };
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&bytes);
            match JobId::from_bytes(arr) {
                Some(j) => j,
                None => return server_error(RpcError::MinerInvalidAuxHash, id, None),
            }
        };

        // Check we know about this job
        {
            let mm_jobs = self.mining_state.mm_jobs.lock().await;
            if !mm_jobs.contains_key(&job_id) {
                return miner_status_response(id, "rejected")
            }
        }

        // Check not already submitted
        {
            let submitted = self.mining_state.mm_jobs_submitted.lock().await;
            if submitted.contains_key(&job_id) {
                return miner_status_response(id, "rejected")
            }
        }

        // Parse aux_blob (must be empty — upstream returns empty)
        let Some(aux_blob) = params.get("aux_blob") else {
            return server_error(RpcError::MinerMissingAuxBlob, id, None)
        };
        let Some(aux_blob) = aux_blob.get::<String>() else {
            return server_error(RpcError::MinerInvalidAuxBlob, id, None)
        };
        let Ok(aux_blob) = hex::decode(aux_blob) else {
            return server_error(RpcError::MinerInvalidAuxBlob, id, None)
        };
        if !aux_blob.is_empty() {
            return server_error(RpcError::MinerInvalidAuxBlob, id, None)
        }

        // Parse blob (the Monero block)
        let Some(blob) = params.get("blob") else {
            return server_error(RpcError::MinerMissingBlob, id, None)
        };
        let Some(blob) = blob.get::<String>() else {
            return server_error(RpcError::MinerInvalidBlob, id, None)
        };
        let Ok(block) = monero_block_deserialize(blob) else {
            return server_error(RpcError::MinerInvalidBlob, id, None)
        };

        // Parse merkle_proof
        let Some(merkle_proof_j) = params.get("merkle_proof") else {
            return server_error(RpcError::MinerMissingMerkleProof, id, None)
        };
        let Some(merkle_proof_j) = merkle_proof_j.get::<Vec<JsonValue>>() else {
            return server_error(RpcError::MinerInvalidMerkleProof, id, None)
        };
        let mut merkle_proof: Vec<monero::Hash> = Vec::with_capacity(merkle_proof_j.len());
        for hash in merkle_proof_j.iter() {
            match hash.get::<String>() {
                Some(v) => {
                    let Ok(bytes) = hex::decode(v) else {
                        return server_error(RpcError::MinerInvalidMerkleProof, id, None)
                    };
                    if bytes.len() != 32 {
                        return server_error(RpcError::MinerInvalidMerkleProof, id, None)
                    }
                    let mut arr = [0u8; 32];
                    arr.copy_from_slice(&bytes);
                    merkle_proof.push(monero::Hash::from_slice(&arr));
                }
                None => return server_error(RpcError::MinerInvalidMerkleProof, id, None),
            }
        }

        // Parse path
        let Some(path) = params.get("path") else {
            return server_error(RpcError::MinerMissingPath, id, None)
        };
        let Some(path) = path.get::<f64>() else {
            return server_error(RpcError::MinerInvalidPath, id, None)
        };
        let path = *path as u32;

        // Parse seed_hash
        let Some(seed_hash) = params.get("seed_hash") else {
            return server_error(RpcError::MinerMissingSeedHash, id, None)
        };
        let Some(seed_hash) = seed_hash.get::<String>() else {
            return server_error(RpcError::MinerInvalidSeedHash, id, None)
        };
        let Ok(seed_hash_bytes) = hex::decode(seed_hash) else {
            return server_error(RpcError::MinerInvalidSeedHash, id, None)
        };
        let seed_hash_bytes_clone = seed_hash_bytes.clone();
        let Ok(seed_hash) = FixedByteArray::from_bytes(&seed_hash_bytes) else {
            return server_error(RpcError::MinerInvalidSeedHash, id, None)
        };

        info!(
            target: "dwowd::rpc::mm_rpc::mm_submit_solution",
            "[RPC-MM] Got solution submission: aux_hash={}", aux_hash_hex,
        );

        // Construct the Merkle proof
        let Some(merkle_proof) = MerkleProof::try_construct(merkle_proof, path) else {
            return server_error(RpcError::MinerMerkleProofConstructionFailed, id, None)
        };

        // ── Cryptographic receipt #1: aux_hash committed in Monero coinbase ──
        // Use the already-validated JobId bytes (blake3 hash → monero::Hash)
        let aux_hash_bytes = job_id.to_bytes();
        let aux_hash_monero = monero::Hash::from_slice(&aux_hash_bytes);

        // Extract the merge mining tag from the Monero coinbase tx_extra
        let extracted_root = match extract_aux_merkle_root_from_block(&block) {
            Ok(Some(root)) => root,
            Ok(None) => {
                error!(
                    target: "dwowd::rpc::mm_rpc::mm_submit_solution",
                    "[RPC-MM] No merge mining tag found in Monero coinbase tx_extra",
                );
                return miner_status_response(id, "rejected")
            }
            Err(e) => {
                error!(
                    target: "dwowd::rpc::mm_rpc::mm_submit_solution",
                    "[RPC-MM] Failed to extract aux merkle root: {e}",
                );
                return miner_status_response(id, "rejected")
            }
        };

        // Verify: our aux_hash is a leaf in the merkle tree whose root
        // is the merge mining tag embedded in the Monero coinbase
        let calculated_root = merkle_proof.calculate_root(&aux_hash_monero);
        if calculated_root != extracted_root {
            error!(
                target: "dwowd::rpc::mm_rpc::mm_submit_solution",
                "[RPC-MM] Aux merkle proof failed: calculated={calculated_root}, expected={extracted_root}",
            );
            return miner_status_response(id, "rejected")
        }

        info!(
            target: "dwowd::rpc::mm_rpc::mm_submit_solution",
            "[RPC-MM] Aux merkle proof verified: aux_hash committed in Monero coinbase",
        );

        // ── Cryptographic receipt #2: MoneroPowData with coinbase proof ──
        // Construct MoneroPowData from the Monero block
        let monero_pow_data = match MoneroPowData::new(block, seed_hash, merkle_proof) {
            Ok(v) => v,
            Err(e) => {
                error!(
                    target: "dwowd::rpc::mm_rpc::mm_submit_solution",
                    "[RPC-MM] Failed constructing MoneroPowData: {e}",
                );
                return server_error(
                    RpcError::MinerMoneroPowDataConstructionFailed,
                    id,
                    None,
                )
            }
        };

        // Verify the coinbase merkle root is valid
        if !monero_pow_data.is_coinbase_valid_merkle_root() {
            error!(
                target: "dwowd::rpc::mm_rpc::mm_submit_solution",
                "[RPC-MM] Invalid coinbase merkle root",
            );
            return miner_status_response(id, "rejected")
        }

        info!(
            target: "dwowd::rpc::mm_rpc::mm_submit_solution",
            "[RPC-MM] Coinbase merkle proof verified — MoneroPowData is valid",
        );

        // TODO(HAZOP F3): Verify Monero block was accepted by Monero network.
        // FFI spec §5.2 delegates PoW security to Monero consensus — but
        // DarkWow never queries monerod to confirm. The security model trusts
        // p2pool (which runs alongside monerod) to only submit verified blocks.
        // Defense-in-depth: query monerod get_block_by_hash to confirm the
        // Monero block exists on the Monero chain before accepting it here.
        // Requires: monerod_url from FinalityConfig, get_block_by_hash RPC
        // method in src/linear/src/monero/rpc.rs, Monero block hash computation
        // from the submitted blob (RandomX over block header).

        // Get the block template — and with it the height and recipient it was published with
        // (`OBL-C43`/`OBL-C54`). **One snapshot for the whole submission**: this handler used to read the
        // template, then the height from a separate atomic, then the template again, so a login landing
        // between two of those reads could pair one round's template with another round's height or
        // recipient.
        let active = {
            let tmpl = self.mining_state.active_template.lock().await;
            match &*tmpl {
                Some(a) => a.clone(),
                None => return miner_status_response(id, "rejected"),
            }
        };
        let template = active.template.clone();

        // Build the DarkWow block
        let randomx_key: [u8; 32] = seed_hash_bytes_clone.try_into().unwrap_or([0u8; 32]);

        // Rate limit
        #[expect(clippy::unwrap_used, reason = "system clock is always after UNIX_EPOCH")]
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let last_time = self.mining_state.last_block_time.get().get(); // G3: rate-limit comparison
        if last_time > 0 && now.saturating_sub(last_time) < self.min_block_interval {
            return miner_status_response(id, "stale")
        }

        // Build coinbase — plaintext contract call (b6bf44f79): the template
        // carries pre-built PoWRewardV1 call data; no ZK proof rides along.
        let reward = dwow_sdk::blockchain::expected_reward(template.height);
        let pow_reward_call_data = template.pow_reward_call_data.clone();

        let mut header = dwow_chain::BlockHeader {
            fee_window_flags: FeeWindowFlags::default(),
            version: BlockVersion::CURRENT,
            previous: blake3::Hash::from_bytes(template.previous),
            merkle_root: template.merkle_root,
            timestamp: BlockTimestamp::new(template.timestamp),
            target: template.target,
            nonce: 0,
            height: template.height,
            uncle_merkle_root: [0u8; 32],
            total_reward: template.value,
            randomx_key,
            miner: template.miner,
            // Zeroed roots match the built-in miner path (lib.rs): header roots
            // are not validated pre-commit; the WASM execution recomputes them.
            commitment_merkle_root: [0u8; 32],
            nullifier_root: [0u8; 32],
            // A merge-mined block commits no anchor *author*: there is no DarkWow preimage of its
            // own to bind one into — its proof-of-work is Monero's, and the blob is never hashed.
            // Caribina anchoring is therefore unavailable to this path by construction, not by
            // omission; a merge-mined block is unanchored and confers no Caribina finality.
            anchor_owner: [0u8; 32],
            anchor_tx_id: [0u8; 32],
            caribina_anchor: None,
            // The Monero anchor is *derived* from the `PowSource::Monero` proof below, not read from
            // these fields, so they stay zero (OBL-C67). Setting them by hand would confer nothing.
            anchor_monero_height: MoneroBlockHeight::new(0),
            anchor_monero_hash: [0u8; 32],
            finality_flags: 0,
            pow_source: PowSource::Monero(monero_pow_data),
        };

        let coinbase_tx =
            crate::registry::model::plaintext_coinbase_transaction(pow_reward_call_data, reward);

        let mut all_txs = template.transactions.clone();
        all_txs.insert(0, coinbase_tx);

        // Recompute merkle root to include the coinbase transaction.
        // The template merkle_root only covers mempool transactions.
        let tx_hashes: Vec<blake3::Hash> = all_txs.iter().map(|tx| tx.hash()).collect();
        let merkle_root = if tx_hashes.is_empty() {
            blake3::hash(&[])
        } else {
            let mut layer = tx_hashes.clone();
            while layer.len() > 1 {
                if layer.len() % 2 != 0 {
                    #[expect(clippy::unwrap_used, reason = "layer is non-empty inside while layer.len() > 1")]
                    let last = *layer.last().unwrap();
                    layer.push(last);
                }
                layer = layer
                    .chunks(2)
                    .map(|pair| {
                        let mut combined = pair[0].as_bytes().to_vec();
                        combined.extend_from_slice(pair[1].as_bytes());
                        blake3::hash(&combined)
                    })
                    .collect();
            }
            layer[0]
        };
        header.merkle_root = merkle_root;

        let mut block = dwow_chain::Block {
            header,
            transactions: all_txs,
        };

        let chain_state = match self.chain_state.as_ref() {
            Some(c) => c,
            None => return miner_status_response(id, "rejected"),
        };

        // Check template staleness before PoW verification.
        // Per type-system.md §9.3: submissions against stale templates SHALL
        // be rejected before PoW verification.
        //
        // The height comes from the submission's snapshot, not from a re-read of a separate atomic, so it
        // is the height **of the template being submitted**. The old `template_h != 0` guard existed
        // because the atomic read 0 before any template was stored; a snapshot only exists once a template
        // has been published with a real height, so that state is now unrepresentable and the guard is
        // gone with it (`OBL-C43`/`OBL-C54`).
        {
            let chain_h = chain_state.get_height().get();
            if active.height.get() != chain_h {
                info!(
                    target: "dwowd::rpc::mm_rpc::mm_submit_solution",
                    "[RPC-MM] Template height {} != current {} — rejecting stale submission",
                    active.height.get(), chain_h,
                );
                return miner_status_response(id, "rejected");
            }
        }

        // Set finality flags
        block.header.finality_flags = chain_state.finality_config.mine_flags();

        // Apply block with uncles from the submission's snapshot — the same template the height and
        // recipient above came from (`OBL-C43`/`OBL-C54`).
        let uncles: Vec<dwow_chain::UncleBlock> = active.template.uncles.clone();

        // Accept block — single unified path (block_acceptor::accept_block).
        // Use pooled RandomXCache — 256 MB allocation reused.
        let flags = randomx::RandomXFlags::get_recommended_flags() & !randomx::RandomXFlags::JIT;
        #[expect(clippy::expect_used, reason = "RandomX hash failure surfaces via panic (see safety.md C1)")]
        let exec_rx_cache = chain_state.get_cache(randomx_key)
            .expect("Failed to get RandomX cache for mm execution");
        #[expect(clippy::expect_used, reason = "RandomX hash failure surfaces via panic (see safety.md C1)")]
        let exec_vm = std::sync::Arc::new(
            randomx::RandomXVM::new(flags, Some(exec_rx_cache), None)
                .expect("Failed to create RandomX VM for mm execution"),
        );

        match crate::block_acceptor::accept_block_with_mempool(
            &chain_state, &block, &uncles, &exec_vm,
            template.target, None,
            // The reorg this may trigger returns displaced transactions here (`OBL-C44`).
            self.mempool.as_ref(),
        ) {
            Ok(dwow_chain::BlockConnectOutcome::CanonicalExtension { .. }) => {
                drop(exec_vm);
                self.mining_state.last_block_time.set_now();

                info!(
                    target: "dwowd::rpc::mm_rpc::mm_submit_solution",
                    "[RPC-MM] Merge-mined block at height {} accepted!",
                    template.height,
                );

                // HAZID F5: Remove mined transactions from mempool.
                // ONLY for canonical blocks — competing/uncle blocks do NOT advance the chain.
                if let Some(ref mp) = self.mempool {
                    let tx_hashes: Vec<blake3::Hash> = block.transactions.iter()
                        .map(|tx| tx.hash()).collect();
                    mp.mark_mined(&tx_hashes).await;
                }

                // HAZID H-C2: broadcast merge-mined block to P2P peers.
                // Previously merge-mined blocks were committed locally but
                // never propagated — the network only discovered them via
                // 30-second sync poll. Now broadcast immediately.
                crate::proto::linear_broadcast::broadcast_block(
                    &self.p2p_handler.p2p, block.clone(), vec![]).await;

                // Mark job as submitted with bounded capacity
                {
                    const MAX_MM_SUBMITTED: usize = 1000;
                    let mut submitted = self.mining_state.mm_jobs_submitted.lock().await;
                    // HAZID H-M11: FIFO eviction for submitted set too — and it now is FIFO
                    // (`OBL-C55`). The set is keyed the same way as `mm_jobs`, and this site's comment
                    // claimed FIFO while `iter().next()` on a HashSet picked arbitrarily.
                    evict_oldest(&mut submitted, MAX_MM_SUBMITTED);
                    submitted.insert(job_id, self.mining_state.mm_jobs_seq.fetch_add(1, Ordering::Relaxed));
                }

                // Generate new template for next round, paid to the same recipient as the template just
                // submitted — taken from this submission's own snapshot, not from a separately-locked
                // config (`OBL-C43`/`OBL-C54`).
                //
                // The guard this used to carry is gone with the bundle, and that is a behaviour change
                // worth naming: the regeneration was wrapped in `if let Some(config)` over a field that
                // **only a stratum login ever wrote**. On a node serving merge mining alone, the next
                // round's template was therefore never generated, and every later merge-mining submission
                // was rejected against a stale template. The recipient is no longer optional, so the
                // branch cannot be skipped for that reason.
                {
                    let effective_recipient = active.recipient_config.clone();

                    let next_mempool_txs = match &self.mempool {
                        Some(mp) => mp.select_for_block(&self.mining_state.miner_config).await,
                        None => vec![],
                    };

                    // No ZK materials needed — plaintext template.
                    // UNVERIFIED(F2-10): needs cargo test -p dwowd --lib
                    match crate::registry::model::generate_linear_block_template(
                        chain_state,
                        &effective_recipient,
                        next_mempool_txs,
                        vec![],
                    )
                    .await
                    {
                        Ok(new_template) => {
                            self.mining_state
                                .store_template(
                                    new_template,
                                    chain_state.get_height(),
                                    effective_recipient,
                                )
                                .await;
                        }
                        Err(e) => {
                            error!(
                                target: "dwowd::rpc::mm_rpc::mm_submit_solution",
                                "[RPC-MM] Failed to generate new block template: {e}",
                            );
                        }
                    }
                }

                miner_status_response(id, "accepted")
            }
            Ok(_outcome) => {
                // Block was stored as competing or uncle extension —
                // peer beat us to this height. Mempool unchanged.
                info!(
                    target: "dwowd::rpc::mm_rpc::mm_submit_solution",
                    "[RPC-MM] Merge-mined block at height {} stored as {:?} — peer beat us",
                    template.height, _outcome,
                );
                miner_status_response(id, "accepted")
            }
            Err(e) => {
                error!(
                    target: "dwowd::rpc::mm_rpc::mm_submit_solution",
                    "[RPC-MM] Block rejected: {e}",
                );
                miner_status_response(id, "rejected")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dwow_sdk::blockchain::BlockTarget;

    fn test_header() -> dwow_chain::BlockHeader {
        dwow_chain::BlockHeader {
            version: BlockVersion::CURRENT,
            previous: blake3::Hash::from_bytes([0xAA; 32]),
            merkle_root: blake3::Hash::from_bytes([0xBB; 32]),
            timestamp: BlockTimestamp::new(1234567890),
            target: BlockTarget::new(0x00FFFFFF),
            nonce: 0xDEADBEEF,
            height: BlockHeight::new(42),
            uncle_merkle_root: [0xCC; 32],
            total_reward: BlockReward::new(1000000000),
            randomx_key: [0xDD; 32],
            miner: [0xDD; 32],
            commitment_merkle_root: [0xEE; 32],
            nullifier_root: [0xFF; 32],
            anchor_owner: [0u8; 32],
            anchor_tx_id: [0u8; 32],
            caribina_anchor: None,
            anchor_monero_height: MoneroBlockHeight::new(0),
            anchor_monero_hash: [0u8; 32],
            finality_flags: 0,
            fee_window_flags: FeeWindowFlags::default(),
            pow_source: PowSource::Native,
        }
    }

    #[test]
    fn test_mining_blob_len() {
        let header = test_header();
        let blob = header.to_mining_blob();
        assert_eq!(blob.len(), dwow_chain::BlockHeader::MINING_BLOB_LEN);
    }

    #[test]
    fn test_nonce_offset() {
        let mut header = test_header();
        header.nonce = 0xCAFEBABE;

        let blob = header.to_mining_blob();
        let nonce_offset = dwow_chain::BlockHeader::NONCE_OFFSET;
        let nonce_bytes: [u8; 4] = blob[nonce_offset..nonce_offset + 4].try_into().unwrap();
        let nonce = u32::from_le_bytes(nonce_bytes);
        assert_eq!(nonce, 0xCAFEBABE);
    }

    #[test]
    fn test_pow_source_discriminator() {
        let native_header = test_header();
        let blob = native_header.to_mining_blob();
        // The discriminator sits at POW_SOURCE_OFFSET, not at the end of the
        // blob: the miner pubkey follows it. Reading it by named offset keeps
        // this test honest if the blob layout changes again.
        assert_eq!(
            blob[dwow_chain::BlockHeader::POW_SOURCE_OFFSET], 0,
            "Native pow_source should write discriminator 0"
        );
        assert_eq!(blob.len(), dwow_chain::BlockHeader::MINING_BLOB_LEN);
    }

    // ═══════════════════════════════════════════════════════════════════════
    // Pre-pipeline integration tests — merge mining (Phase 3)
    // ═══════════════════════════════════════════════════════════════════════
    // These tests verify the merge mining FFI contract without Docker.
    // Each test maps to a SHALL/MUST in merge-mining-ffi.md.

    /// P0: FIFO eviction — oldest entry evicted, newest survives.
    /// merge-mining-ffi.md §4.4: "When the table reaches capacity, the oldest
    /// entry SHALL be evicted, not all entries."
    ///
    /// **Rewritten 2026-09-23 (`OBL-C55`) to call the code the daemon calls.** As written this test built
    /// its own `HashMap` and `Vec` pair and evicted from *them* — it asserted FIFO of a local model while
    /// the two real eviction sites picked arbitrarily with `keys().next()`, so it could not have failed for
    /// the reason it named. It now drives `evict_oldest`, which is what `mm_get_aux_block` calls.
    #[test]
    fn test_fifo_eviction_removes_oldest_not_newest() {
        let mut table: HashMap<String, u64> = HashMap::new();
        for seq in 0..100u64 {
            table.insert(format!("job_{seq:03}"), seq);
        }
        assert_eq!(table.len(), 100, "table at capacity");

        assert!(evict_oldest(&mut table, 100), "a table at capacity must evict one entry");
        assert_eq!(table.len(), 99, "one entry evicted");

        table.insert("job_new".to_string(), 100);
        assert_eq!(table.len(), 100, "new entry fits after FIFO eviction");

        // The implementation's answer, not a model's: `keys().next()` — what the daemon did before
        // `OBL-C55` — picks an arbitrary key, so `job_000` surviving is the failure this asserts against.
        assert!(!table.contains_key("job_000"), "the OLDEST entry (job_000) must be the one evicted");
        assert!(table.contains_key("job_099"), "job_099 is newer and must survive");
        assert!(table.contains_key("job_new"), "new entry present");

        // Below capacity evicts nothing — a control that the helper is not a blanket clear.
        assert!(!evict_oldest(&mut table, 1000), "under capacity there is nothing to evict");
        assert_eq!(table.len(), 100, "and nothing is removed");
    }

    /// P0: Submitted set FIFO eviction — same pattern as job table.
    /// merge-mining-ffi.md §4.4
    ///
    /// **Rewritten 2026-09-23 (`OBL-C55`)**, for the same reason as the table test above: the version
    /// before it built a local `HashSet`, removed an entry with the comment "arbitrary pick from set", and
    /// asserted only the resulting *length* — so it passed against the arbitrary behaviour it was named
    /// after. This one asserts which entry goes.
    #[test]
    fn test_submitted_fifo_eviction() {
        let mut submitted: HashMap<String, u64> = HashMap::new();
        for seq in 0..1000u64 {
            submitted.insert(format!("sub_{seq:04}"), seq);
        }
        assert_eq!(submitted.len(), 1000, "submitted table at capacity");

        assert!(evict_oldest(&mut submitted, 1000));
        assert_eq!(submitted.len(), 999, "one entry evicted");

        submitted.insert("sub_new".into(), 1000);
        assert_eq!(submitted.len(), 1000, "new entry fits");

        assert!(!submitted.contains_key("sub_0000"), "the OLDEST submission must be the one evicted");
        assert!(submitted.contains_key("sub_0999"), "the newest pre-existing entry survives");
        assert!(submitted.contains_key("sub_new"), "new entry present");
    }

    /// P3: Job ID determinism — same template contents produce same job ID.
    /// merge-mining-ffi.md §4.3: "job_id SHALL be a deterministic function
    /// of template contents."
    #[test]
    fn test_job_id_deterministic() {
        let prev = [0xAAu8; 32];
        let height: u64 = 42;
        let merkle = [0xBBu8; 32];
        let ts: u64 = 1234567890;

        // Compute job_id twice with same inputs
        let mut hasher1 = blake3::Hasher::new();
        hasher1.update(&prev);
        hasher1.update(&height.to_le_bytes());
        hasher1.update(&merkle);
        hasher1.update(&ts.to_le_bytes());
        let job1 = hasher1.finalize();

        let mut hasher2 = blake3::Hasher::new();
        hasher2.update(&prev);
        hasher2.update(&height.to_le_bytes());
        hasher2.update(&merkle);
        hasher2.update(&ts.to_le_bytes());
        let job2 = hasher2.finalize();

        assert_eq!(job1.as_bytes(), job2.as_bytes(), "same inputs → same job_id");
    }

    /// P3: Job ID changes with different inputs.
    /// merge-mining-ffi.md §4.3
    #[test]
    fn test_job_id_changes_with_height() {
        let prev = [0xAAu8; 32];
        let merkle = [0xBBu8; 32];
        let ts: u64 = 1234567890;

        let mut hasher1 = blake3::Hasher::new();
        hasher1.update(&prev);
        hasher1.update(&42u64.to_le_bytes());
        hasher1.update(&merkle);
        hasher1.update(&ts.to_le_bytes());
        let job_h42 = hasher1.finalize();

        let mut hasher2 = blake3::Hasher::new();
        hasher2.update(&prev);
        hasher2.update(&43u64.to_le_bytes());
        hasher2.update(&merkle);
        hasher2.update(&ts.to_le_bytes());
        let job_h43 = hasher2.finalize();

        assert_ne!(job_h42.as_bytes(), job_h43.as_bytes(), "different height → different job_id");
    }

    /// Newtype validation: MoneroHash rejects zero.
    /// merge-mining-ffi.md §2.1
    #[test]
    fn test_monero_hash_rejects_zero_in_mm_context() {
        use dwow_chain::monero::MoneroHash;
        assert!(MoneroHash::from_bytes([0u8; 32]).is_none(), "zero hash rejected");
    }

    /// Newtype validation: RandomXKey rejects non-32-byte.
    /// merge-mining-ffi.md §2.1
    #[test]
    fn test_randomx_key_rejects_bad_length() {
        use dwow_chain::monero::RandomXKey;
        assert!(RandomXKey::from_bytes(&[0x42u8; 31]).is_none(), "31 bytes rejected");
        assert!(RandomXKey::from_bytes(&[0x42u8; 33]).is_none(), "33 bytes rejected");
        assert!(RandomXKey::from_bytes(&[0x42u8; 32]).is_some(), "32 bytes accepted");
    }
}
