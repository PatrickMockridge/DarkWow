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
    system::StoppableTaskPtr,
};
use dwow_chain::{
    monero::{
        fixed_array::FixedByteArray,
        extract_aux_merkle_root_from_block,
        merkle_proof::MerkleProof,
        monero_block_deserialize,
        MoneroPowData,
    },
    PowSource,
};

use crate::{error::{miner_status_response, server_error, RpcError}, DwowNode};

/// JSON-RPC `RequestHandler` for Merge Mining (p2pool protocol)
pub struct MergeMiningRpcHandler;

#[async_trait]
impl RequestHandler<MergeMiningRpcHandler> for DwowNode {
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
            let hash = self.linear_genesis_hash.lock().await;
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

        // Skip duplicate jobs — p2pool polls with the same aux_hash until
        // a solution is found
        {
            let mm_jobs = self.mm_jobs.lock().await;
            if mm_jobs.contains_key(&aux_hash) {
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

        // Parse height (Monero block height)
        let Some(height) = params.get("height") else {
            return server_error(RpcError::MinerMissingHeight, id, None)
        };
        let Some(height) = height.get::<f64>() else {
            return server_error(RpcError::MinerInvalidHeight, id, None)
        };
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

        let linear_chain = match self.linear_blockchain.as_ref() {
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

        // Generate block template
        let placeholder_kp = dwow_sdk::crypto::keypair::Keypair::random(&mut rand::rngs::OsRng);
        let recipient_config = crate::registry::model::LinearMinerRewardsRecipientConfig {
            recipient: placeholder_kp.public,
        };

        let linear_zk = {
            let zk_lock = self.linear_zk.lock().await;
            zk_lock.clone()
        };

        let mempool_txs = match &self.mempool {
            Some(mp) => mp.take_all().await,
            None => vec![],
        };

        let template = match crate::registry::model::generate_linear_block_template(
            linear_chain,
            &recipient_config,
            linear_zk.as_ref(),
            mempool_txs,
        )
        .await
        {
            Ok(t) => t,
            Err(e) => {
                error!(
                    target: "dwowd::rpc::mm_rpc::mm_get_aux_block",
                    "[RPC-MM] Failed to generate block template: {e}",
                );
                return JsonResponse::new(JsonValue::from(HashMap::new()), id).into()
            }
        };

        // Generate job ID: blake3(template_hash || timestamp)
        let mut job_hasher = blake3::Hasher::new();
        job_hasher.update(&template.previous);
        job_hasher.update(&template.height.to_le_bytes());
        job_hasher.update(template.merkle_root.as_bytes());
        job_hasher.update(&template.timestamp.to_le_bytes());
        let job_id = job_hasher.finalize().to_string();

        // Derive difficulty
        let difficulty = {
            let consensus = linear_chain.consensus.lock().unwrap();
            let target = consensus.target();
            u32::MAX as u64 / target as u64
        };

        // Register the job
        {
            let mut mm_jobs = self.mm_jobs.lock().await;
            mm_jobs.insert(job_id.clone(), ());
        }

        // Store template in current_linear_template
        *self.current_linear_template.lock().await = Some(template);

        info!(
            target: "dwowd::rpc::mm_rpc::mm_get_aux_block",
            "[RPC-MM] Created new merge mining job: aux_hash={}", job_id,
        );

        let response = JsonValue::from(HashMap::from([
            ("aux_blob".to_string(), JsonValue::from(hex::encode(vec![]))),
            ("aux_diff".to_string(), JsonValue::Number(difficulty as f64)),
            ("aux_hash".to_string(), JsonValue::from(job_id)),
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
        let _submit_guard = self.linear_submit_lock.lock().await;

        // Parse request params
        let Some(params) = params.get::<HashMap<String, JsonValue>>() else {
            return JsonError::new(InvalidParams, None, id).into()
        };

        // Parse aux_hash
        let Some(aux_hash) = params.get("aux_hash") else {
            return server_error(RpcError::MinerMissingAuxHash, id, None)
        };
        let Some(aux_hash) = aux_hash.get::<String>() else {
            return server_error(RpcError::MinerInvalidAuxHash, id, None)
        };
        let aux_hash = aux_hash.to_string();

        // Check we know about this job
        {
            let mm_jobs = self.mm_jobs.lock().await;
            if !mm_jobs.contains_key(&aux_hash) {
                return miner_status_response(id, "rejected")
            }
        }

        // Check not already submitted
        {
            let submitted = self.mm_jobs_submitted.lock().await;
            if submitted.contains(&aux_hash) {
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
            "[RPC-MM] Got solution submission: aux_hash={}", aux_hash,
        );

        // Construct the Merkle proof
        let Some(merkle_proof) = MerkleProof::try_construct(merkle_proof, path) else {
            return server_error(RpcError::MinerMerkleProofConstructionFailed, id, None)
        };

        // ── Cryptographic receipt #1: aux_hash committed in Monero coinbase ──
        // Decode our aux_hash (blake3 hex string → 32 bytes → monero::Hash)
        let aux_hash_bytes = match hex::decode(&aux_hash) {
            Ok(b) if b.len() == 32 => b,
            _ => return server_error(RpcError::MinerInvalidAuxHash, id, None),
        };
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

        // Get the block template
        let template = {
            let tmpl = self.current_linear_template.lock().await;
            match &*tmpl {
                Some(t) => t.clone(),
                None => return miner_status_response(id, "rejected"),
            }
        };

        // Build the DarkWow block
        let randomx_key: [u8; 32] = seed_hash_bytes_clone.try_into().unwrap_or([0u8; 32]);

        // Rate limit
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let last_time = self.last_block_time.load(Ordering::SeqCst);
        if last_time > 0 && now.saturating_sub(last_time) < self.min_block_interval {
            return miner_status_response(id, "stale")
        }

        // Build coinbase
        let reward = dwow_sdk::blockchain::expected_reward(template.height as u32);
        let (coinbase_tx_data, coin_merkle_root, nullifier_root) = if !template.zk_proof.is_empty() {
            let cb = dwow_chain::CoinbaseTransaction {
                proof: template.zk_proof.clone(),
                public_inputs: template.zk_public_inputs,
                coin: template.coin,
                value_commit_x: template.value_commit_x,
                value_commit_y: template.value_commit_y,
                token_commit: template.token_commit,
                encrypted_note: template.encrypted_note.clone(),
            };
            (Some(cb), template.coin_merkle_root, template.nullifier_root)
        } else {
            (None, [0u8; 32], [0u8; 32])
        };

        let mut header = dwow_chain::BlockHeader {
            version: 1,
            previous: blake3::Hash::from_bytes(template.previous),
            merkle_root: template.merkle_root,
            timestamp: template.timestamp,
            target: template.target,
            nonce: 0,
            height: template.height,
            uncle_merkle_root: [0u8; 32],
            total_reward: reward,
            randomx_key,
            coin_merkle_root,
            nullifier_root,
            anchor_tx_id: [0u8; 32],
            anchor_monero_height: 0,
            anchor_monero_hash: [0u8; 32],
            finality_flags: 0,
            pow_source: PowSource::Monero(monero_pow_data),
        };

        let coinbase_tx = dwow_chain::Transaction {
            version: 1,
            inputs: vec![],
            outputs: vec![dwow_chain::Output {
                value: reward,
                script: vec![],
            }],
            contract_calls: vec![],
            lock_time: 0,
            coinbase: coinbase_tx_data,
        };

        let mut all_txs = template.transactions.clone();
        all_txs.push(coinbase_tx);

        // Recompute merkle root to include the coinbase transaction.
        // The template merkle_root only covers mempool transactions.
        let tx_hashes: Vec<blake3::Hash> = all_txs.iter().map(|tx| tx.hash()).collect();
        let merkle_root = if tx_hashes.is_empty() {
            blake3::hash(&[])
        } else {
            let mut layer = tx_hashes.clone();
            while layer.len() > 1 {
                if layer.len() % 2 != 0 {
                    layer.push(*layer.last().unwrap());
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

        let linear_chain = match self.linear_blockchain.as_ref() {
            Some(c) => c,
            None => return miner_status_response(id, "rejected"),
        };

        // Set finality flags
        block.header.finality_flags = linear_chain.finality_config.mine_flags();

        // Apply block
        match linear_chain.apply_block(&block).await {
            Ok(()) => {
                self.last_block_time.store(now, Ordering::SeqCst);

                info!(
                    target: "dwowd::rpc::mm_rpc::mm_submit_solution",
                    "[RPC-MM] Merge-mined block at height {} accepted!",
                    template.height,
                );

                // Mark job as submitted
                {
                    let mut submitted = self.mm_jobs_submitted.lock().await;
                    submitted.insert(aux_hash.clone());
                }

                // Generate new template for next round
                if let Some(ref recipient_config) = *self.linear_recipient_config.lock().await {
                    let linear_zk = {
                        let zk_lock = self.linear_zk.lock().await;
                        zk_lock.clone()
                    };

                    let next_mempool_txs = match &self.mempool {
                        Some(mp) => mp.take_all().await,
                        None => vec![],
                    };

                    match crate::registry::model::generate_linear_block_template(
                        linear_chain,
                        recipient_config,
                        linear_zk.as_ref(),
                        next_mempool_txs,
                    )
                    .await
                    {
                        Ok(new_template) => {
                            *self.current_linear_template.lock().await = Some(new_template);
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

    fn test_header() -> dwow_chain::BlockHeader {
        dwow_chain::BlockHeader {
            version: 1,
            previous: blake3::Hash::from_bytes([0xAA; 32]),
            merkle_root: blake3::Hash::from_bytes([0xBB; 32]),
            timestamp: 1234567890,
            target: 0x00FFFFFF,
            nonce: 0xDEADBEEF,
            height: 42,
            uncle_merkle_root: [0xCC; 32],
            total_reward: 1000000000,
            randomx_key: [0xDD; 32],
            coin_merkle_root: [0xEE; 32],
            nullifier_root: [0xFF; 32],
            anchor_tx_id: [0u8; 32],
            anchor_monero_height: 0,
            anchor_monero_hash: [0u8; 32],
            finality_flags: 0,
            pow_source: PowSource::Native,
        }
    }

    #[test]
    fn test_mining_blob_len() {
        let header = test_header();
        let blob = header.to_mining_blob();
        assert_eq!(blob.len(), dwow_chain::BlockHeader::MINING_BLOB_LEN);
        assert_eq!(blob.len(), 228);
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
        // Last byte is the discriminator
        assert_eq!(blob[227], 0, "Native pow_source should write discriminator 0");
        assert_eq!(blob.len(), 228);
    }
}
