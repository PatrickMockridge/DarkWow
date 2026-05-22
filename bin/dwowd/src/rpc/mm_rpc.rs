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
//! Implements the Monero p2pool merge mining protocol so p2pool can connect
//! to dwowd as an aux chain. The two RPC methods are:
//!
//! - `merge_mining_get_aux_block` — p2pool requests aux chain mining data
//! - `merge_mining_submit_solution` — p2pool submits a solved aux block
//!
//! Reference: <https://github.com/SChernykh/p2pool>

use std::{
    collections::{HashMap, HashSet},
    sync::atomic::Ordering,
};

use async_trait::async_trait;
use smol::lock::MutexGuard;
use tinyjson::JsonValue;
use tracing::{error, info};

use dwow::{
    rpc::{
        jsonrpc::{
            ErrorCode, ErrorCode::InvalidParams, JsonError, JsonRequest, JsonResponse, JsonResult,
        },
        server::RequestHandler,
    },
    system::StoppableTaskPtr,
};

use crate::{error::miner_status_response, DwowNode};

/// JSON-RPC `RequestHandler` for Merge Mining (p2pool protocol)
pub struct MergeMiningRpcHandler;

#[async_trait]
impl RequestHandler<MergeMiningRpcHandler> for DwowNode {
    async fn handle_request(&self, req: JsonRequest) -> JsonResult {
        match req.method.as_str() {
            "merge_mining_get_chain_id" => {
                self.mm_get_chain_id(req.id, req.params).await
            }
            "merge_mining_get_aux_block" => {
                self.mm_get_aux_block(req.id, req.params).await
            }
            "merge_mining_submit_solution" => {
                self.mm_submit_solution(req.id, req.params).await
            }
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
    /// This is the FIRST method p2pool calls during merge mining setup.
    /// Without a successful response, p2pool will never call `get_aux_block`
    /// and the stratum server will not start.
    pub async fn mm_get_chain_id(&self, id: u16, _params: JsonValue) -> JsonResult {
        let hash = self.linear_genesis_hash.lock().await;
        let chain_id = match &*hash {
            Some(h) => hex::encode(h.0),
            None => return JsonError::new(
                ErrorCode::InternalError,
                Some("Genesis hash not yet initialized".to_string()),
                id,
            ).into(),
        };

        info!(
            target: "dwowd::rpc::mm_rpc::mm_get_chain_id",
            "[RPC-MM] get_chain_id: {}",
            chain_id,
        );

        let result = JsonValue::from(HashMap::from([
            ("chain_id".to_string(), JsonValue::String(chain_id)),
        ]));

        JsonResponse::new(result, id).into()
    }

    /// Handle `merge_mining_get_aux_block` — p2pool requests aux chain data.
    ///
    /// p2pool calls this to get the DarkWow block template that miners should
    /// merge-mine alongside Monero. Returns the 227-byte mining blob, target,
    /// height, and previous hash in the format p2pool expects.
    pub async fn mm_get_aux_block(&self, id: u16, params: JsonValue) -> JsonResult {
        use crate::registry::model::generate_linear_block_template;

        let Some(params) = params.get::<HashMap<String, JsonValue>>() else {
            return JsonError::new(InvalidParams, None, id).into()
        };

        // p2pool sends: address, aux_hash, height, prev_hash
        // These are Monero-side identifiers that we don't need for the aux
        // chain itself, but we validate they're present for protocol compliance.

        let Some(_address) = params.get("address").and_then(|v| v.get::<String>()) else {
            return JsonError::new(InvalidParams, Some("Missing 'address'".to_string()), id).into()
        };

        let Some(_aux_hash) = params.get("aux_hash").and_then(|v| v.get::<String>()) else {
            return JsonError::new(InvalidParams, Some("Missing 'aux_hash'".to_string()), id).into()
        };

        let linear_chain = match self.linear_blockchain.as_ref() {
            Some(c) => c,
            None => {
                return JsonError::new(
                    ErrorCode::InternalError,
                    Some("linear-testnet mode only".to_string()),
                    id,
                )
                .into()
            }
        };

        // Use an anonymous recipient config for template generation (p2pool
        // doesn't send a wallet address for the aux chain). The template is
        // only used for the block structure; rewards are handled by the
        // p2pool adaptor's own reward distribution.
        // p2pool controls reward distribution separately — we generate a
        // random keypair for the template's coinbase recipient since it
        // won't be used for actual rewards.
        let placeholder_kp = dwow_sdk::crypto::keypair::Keypair::random(&mut rand::rngs::OsRng);
        let recipient_config = crate::registry::model::LinearMinerRewardsRecipientConfig {
            recipient: placeholder_kp.public,
        };

        let linear_zk = {
            let zk_lock = self.linear_zk.lock().await;
            zk_lock.clone()
        };

        let template = match generate_linear_block_template(
            linear_chain,
            &recipient_config,
            linear_zk.as_ref(),
        )
        .await
        {
            Ok(t) => t,
            Err(e) => {
                error!(
                    target: "dwowd::rpc::mm_rpc::mm_get_aux_block",
                    "Failed to generate linear block template: {e}",
                );
                return JsonError::new(
                    ErrorCode::InternalError,
                    Some(format!("Failed to generate block template: {e}")),
                    id,
                )
                .into()
            }
        };

        // Build mining blob from the template
        let randomx_key = dwow_linear::Miner::derive_key_from_height(template.height);
        let mining_header = dwow_linear::BlockHeader {
            version: 1,
            previous: blake3::Hash::from_bytes(template.previous),
            merkle_root: blake3::hash(&[]),
            timestamp: template.timestamp,
            target: template.target,
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

        let template_height = template.height;
        let template_previous = template.previous;

        // Compute aux_hash = hash of the block template (used by p2pool to
        // detect when the aux chain work changes).
        let aux_hash = blake3::hash(&blob_data);
        let aux_hash_hex = aux_hash.to_hex().to_string();

        let prev_hash_hex = hex::encode(template_previous);

        // Target encoding: same bridge as stratum (64-bit format for p2pool).
        // Upper 32 bits = 0xFFFFFFFF so only lower 32 bits matter.
        let aux_target = format!("FFFFFFFF{:08x}", template.target);

        info!(
            target: "dwowd::rpc::mm_rpc::mm_get_aux_block",
            "[RPC-MM] get_aux_block: height={}, target={}, blob_len={}",
            template_height,
            template.target,
            blob_data.len(),
        );

        // Store template for submit validation
        *self.current_linear_template.lock().await = Some(template);

        // p2pool expects aux_diff as a decimal number (difficulty = MAX/target)
        let aux_difficulty = u32::MAX as u64 / template.target as u64;

        let result = JsonValue::from(HashMap::from([
            ("aux_blob".to_string(), JsonValue::String(blob)),
            ("aux_hash".to_string(), JsonValue::String(aux_hash_hex)),
            ("aux_diff".to_string(), JsonValue::Number(aux_difficulty as f64)),
            ("aux_target".to_string(), JsonValue::String(aux_target)),
            ("aux_height".to_string(), JsonValue::Number(template_height as f64)),
            ("aux_prev_hash".to_string(), JsonValue::String(prev_hash_hex)),
        ]));

        JsonResponse::new(result, id).into()
    }

    /// Handle `merge_mining_submit_solution` — p2pool submits a solved aux block.
    ///
    /// p2pool calls this when a Monero block is found that includes DarkWow
    /// merge-mining data. The submitted blob has the nonce filled in by the
    /// Monero miner. We verify RandomX PoW and insert the block if valid.
    pub async fn mm_submit_solution(&self, id: u16, params: JsonValue) -> JsonResult {
        use crate::registry::model::generate_linear_block_template;

        // Serialize submissions to prevent concurrent RandomX VM access
        let _submit_guard = self.linear_submit_lock.lock().await;

        let Some(params) = params.get::<HashMap<String, JsonValue>>() else {
            return JsonError::new(InvalidParams, None, id).into()
        };

        // Parse aux_blob (hex-encoded block with nonce filled in)
        let Some(blob_hex) = params.get("aux_blob").and_then(|v| v.get::<String>()) else {
            return JsonError::new(InvalidParams, Some("Missing 'aux_blob'".to_string()), id).into()
        };
        let blob_data = match hex::decode(blob_hex) {
            Ok(b) => b,
            Err(e) => {
                return JsonError::new(
                    InvalidParams,
                    Some(format!("Invalid 'aux_blob' hex: {e}")),
                    id,
                )
                .into()
            }
        };

        if blob_data.len() < dwow_linear::BlockHeader::MINING_BLOB_LEN {
            return JsonError::new(
                InvalidParams,
                Some(format!(
                    "Blob too short: {} < {}",
                    blob_data.len(),
                    dwow_linear::BlockHeader::MINING_BLOB_LEN,
                )),
                id,
            )
            .into()
        }

        // Parse aux_hash (optional, for validation)
        let _aux_hash = params.get("aux_hash").and_then(|v| v.get::<String>());

        // Parse aux_nonce (optional)
        let _aux_nonce = params.get("aux_nonce").and_then(|v| v.get::<f64>());

        // Parse Monero block height from the p2pool submit params
        let monero_height = params
            .get("height")
            .and_then(|v| v.get::<f64>())
            .map(|h| *h as u64);

        // Parse Monero block hash from the p2pool submit params (optional)
        let monero_block_hash: Option<[u8; 32]> = params
            .get("hash")
            .and_then(|v| v.get::<String>())
            .and_then(|s| {
                hex::decode(s)
                    .ok()
                    .and_then(|b| b.try_into().ok())
            });

        info!(
            target: "dwowd::rpc::mm_rpc::mm_submit_solution",
            "[RPC-MM] submit_solution: blob_len={}, monero_height={:?}, monero_hash={:?}",
            blob_data.len(),
            monero_height,
            monero_block_hash.as_ref().map(|h| hex::encode(h)),
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
                target: "dwowd::rpc::mm_rpc::mm_submit_solution",
                "[RPC-MM] Rate-limited: {}s since last block (min {})",
                now.saturating_sub(last_time),
                self.min_block_interval,
            );
            return miner_status_response(id, "stale")
        }

        // Deserialize the blob back into a BlockHeader
        let submitted_header = match mm_deserialize_header(&blob_data) {
            Some(h) => h,
            None => {
                error!(
                    target: "dwowd::rpc::mm_rpc::mm_submit_solution",
                    "[RPC-MM] Failed to deserialize submitted blob",
                );
                return miner_status_response(id, "rejected")
            }
        };

        let submitted_height = submitted_header.height;

        // Validate height matches current chain tip + 1
        let current_height = linear_chain.get_height();
        if submitted_height != current_height + 1 {
            info!(
                target: "dwowd::rpc::mm_rpc::mm_submit_solution",
                "[RPC-MM] Stale: submitted height {} != expected {}",
                submitted_height,
                current_height + 1,
            );
            return miner_status_response(id, "stale")
        }

        let randomx_key = submitted_header.randomx_key;
        let target = {
            let consensus = linear_chain.consensus.lock().unwrap();
            consensus.target()
        };

        // Load stored template for ZK coinbase data and timestamp
        let template = self.current_linear_template.lock().await.clone();
        let template_height = template.as_ref().map(|t| t.height).unwrap_or(0);

        // Reject if the submitted block doesn't match our current template height
        if submitted_height != template_height {
            info!(
                target: "dwowd::rpc::mm_rpc::mm_submit_solution",
                "[RPC-MM] Template mismatch: submitted={}, template={}",
                submitted_height,
                template_height,
            );
            return miner_status_response(id, "stale")
        }

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

        // Build the full block header with all fields from the template
        let mut header = submitted_header;
        header.target = target;
        header.total_reward = reward;
        header.coin_merkle_root = coin_merkle_root;
        header.nullifier_root = nullifier_root;

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

        // Verify RandomX PoW before inserting
        {
            let submit_blob = block.header.to_mining_blob();
            let vm = linear_chain.get_vm(randomx_key);
            let daemon_hash = block.hash(&vm);
            info!(
                target: "dwowd::rpc::mm_rpc::mm_submit_solution",
                "[RPC-MM] Submit — nonce={}, blob={}, daemon_hash={}",
                block.header.nonce,
                hex::encode(&submit_blob),
                hex::encode(daemon_hash.as_bytes()),
            );
            match linear_chain.consensus.lock().unwrap().verify_proof(&block, &vm) {
                Ok(true) => {}
                Ok(false) => {
                    info!(
                        target: "dwowd::rpc::mm_rpc::mm_submit_solution",
                        "[RPC-MM] Block at height {} rejected: PoW verification failed",
                        submitted_height,
                    );
                    return miner_status_response(id, "rejected")
                }
                Err(e) => {
                    info!(
                        target: "dwowd::rpc::mm_rpc::mm_submit_solution",
                        "[RPC-MM] Block at height {} rejected: PoW error: {}",
                        submitted_height,
                        e,
                    );
                    return miner_status_response(id, "rejected")
                }
            }
        }

        // --- Monero anchor population (best-effort, independent of Caribina) ---
        {
            let fc = &linear_chain.finality_config;
            if fc.should_anchor_monero() {
                if let Some(height) = monero_height {
                    block.header.anchor_monero_height = height;
                    if let Some(hash) = monero_block_hash {
                        block.header.anchor_monero_hash = hash;
                    }
                    info!(
                        target: "dwowd::rpc::mm_rpc::mm_submit_solution",
                        "[RPC-MM] Monero anchor set: height={}, hash={:?}",
                        height,
                        block.header.anchor_monero_hash,
                    );
                } else {
                    info!(
                        target: "dwowd::rpc::mm_rpc::mm_submit_solution",
                        "[RPC-MM] Monero anchor skipped: no height in p2pool params",
                    );
                }
            }
        }

        // --- Caribina anchor (best-effort, independent of Monero) ---
        {
            let fc = &linear_chain.finality_config;
            if fc.should_anchor() {
                let vm = linear_chain.get_vm(randomx_key);
                let block_hash = block.hash(&vm);
                let mut block_hash_bytes = [0u8; 32];
                block_hash_bytes.copy_from_slice(block_hash.as_bytes());
                match dwow_linear::caribina::anchor_block(
                    &block_hash_bytes,
                    block.header.timestamp,
                    block.header.height,
                ) {
                    Some(tx_id) => {
                        block.header.anchor_tx_id = tx_id;
                        info!(
                            target: "dwowd::rpc::mm_rpc::mm_submit_solution",
                            "[RPC-MM] Anchored block {} to Arweave",
                            block_hash,
                        );
                    }
                    None => {
                        info!(
                            target: "dwowd::rpc::mm_rpc::mm_submit_solution",
                            "[RPC-MM] Arweave anchor skipped (network/turbo unavailable)",
                        );
                    }
                }
            }
        }

        // --- Set finality_flags unconditionally (not gated on anchor success) ---
        block.header.finality_flags = linear_chain.finality_config.mine_flags();

        // Insert validated block
        match linear_chain.insert_validated_block(&block) {
            Ok(_) => {
                self.last_block_time.store(now, Ordering::SeqCst);

                info!(
                    target: "dwowd::rpc::mm_rpc::mm_submit_solution",
                    "[RPC-MM] Block at height {} accepted!",
                    submitted_height,
                );

                // Generate new template for the next round
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

                miner_status_response(id, "OK")
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

/// Deserialize a byte array back into a BlockHeader.
///
/// Layout must match `BlockHeader::to_mining_blob()` (227 bytes).
/// Duplicated from dwow-p2pool-adaptor/src/translate.rs to avoid a dependency
/// on the adaptor crate.
fn mm_deserialize_header(data: &[u8]) -> Option<dwow_linear::BlockHeader> {
    if data.len() < dwow_linear::BlockHeader::MINING_BLOB_LEN {
        return None
    }

    let previous = blake3::Hash::from_bytes(data[0..32].try_into().ok()?);
    let version = data[32];
    let target = u32::from_le_bytes(data[33..37].try_into().ok()?);
    // bytes 37..39 are reserved (zero-pad)
    let nonce = u32::from_le_bytes(data[39..43].try_into().ok()?);
    let height = u64::from_le_bytes(data[43..51].try_into().ok()?);
    let merkle_root = blake3::Hash::from_bytes(data[51..83].try_into().ok()?);
    let timestamp = u64::from_le_bytes(data[83..91].try_into().ok()?);
    let uncle_merkle_root: [u8; 32] = data[91..123].try_into().ok()?;
    let total_reward = u64::from_le_bytes(data[123..131].try_into().ok()?);
    let randomx_key: [u8; 32] = data[131..163].try_into().ok()?;
    let coin_merkle_root: [u8; 32] = data[163..195].try_into().ok()?;
    let nullifier_root: [u8; 32] = data[195..227].try_into().ok()?;

    Some(dwow_linear::BlockHeader {
        version,
        previous,
        merkle_root,
        timestamp,
        target,
        nonce,
        height,
        uncle_merkle_root,
        total_reward,
        randomx_key,
        coin_merkle_root,
        nullifier_root,
        anchor_tx_id: [0u8; 32],
        anchor_monero_height: 0,
        anchor_monero_hash: [0u8; 32],
        finality_flags: 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_header() -> dwow_linear::BlockHeader {
        dwow_linear::BlockHeader {
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
        }
    }

    #[test]
    fn test_deserialize_header_roundtrip() {
        let header = test_header();
        let blob = header.to_mining_blob();
        assert_eq!(blob.len(), dwow_linear::BlockHeader::MINING_BLOB_LEN);

        let deserialized = mm_deserialize_header(&blob).unwrap();
        assert_eq!(deserialized.version, header.version);
        assert_eq!(deserialized.previous, header.previous);
        assert_eq!(deserialized.timestamp, header.timestamp);
        assert_eq!(deserialized.target, header.target);
        assert_eq!(deserialized.nonce, header.nonce);
        assert_eq!(deserialized.height, header.height);
        assert_eq!(deserialized.uncle_merkle_root, header.uncle_merkle_root);
        assert_eq!(deserialized.total_reward, header.total_reward);
        assert_eq!(deserialized.randomx_key, header.randomx_key);
    }

    #[test]
    fn test_deserialize_header_matches_to_mining_blob() {
        let header = test_header();
        let blob = header.to_mining_blob();
        let deserialized = mm_deserialize_header(&blob).unwrap();

        // Re-serialize and compare
        let re_blob = deserialized.to_mining_blob();
        assert_eq!(blob, re_blob);
    }

    #[test]
    fn test_deserialize_header_nonce_offset() {
        let mut header = test_header();
        header.nonce = 0xCAFEBABE;

        let blob = header.to_mining_blob();
        let nonce_offset = dwow_linear::BlockHeader::NONCE_OFFSET;
        let nonce_bytes: [u8; 4] = blob[nonce_offset..nonce_offset + 4].try_into().unwrap();
        let nonce = u32::from_le_bytes(nonce_bytes);
        assert_eq!(nonce, 0xCAFEBABE);

        let deserialized = mm_deserialize_header(&blob).unwrap();
        assert_eq!(deserialized.nonce, 0xCAFEBABE);
    }

    #[test]
    fn test_deserialize_short_blob() {
        let result = mm_deserialize_header(&[0u8; 100]);
        assert!(result.is_none());
    }
}
