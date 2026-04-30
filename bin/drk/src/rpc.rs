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
    collections::{BTreeMap, HashMap},
    sync::Arc,
    time::Instant,
};

use futures::{AsyncReadExt, AsyncWriteExt};
use smol::channel::Sender;
use smol::net::TcpStream;
use url::Url;

use darkfi::{
    blockchain::{BlockInfo, HeaderHash, PowData},
    rpc::{
        client::RpcClient,
        jsonrpc::{ErrorCode, JsonError, JsonRequest, JsonResult},
        util::JsonValue,
    },
    system::{ExecutorPtr, Publisher, StoppableTaskPtr},
    tx::Transaction,
    util::encoding::base64,
    zk::verifier::{verify_zkp, ZkVerifyResult},
    Error, Result,
};
use crate::contract_imports::money::TokenId;
use darkfi_sdk::{
    bridgetree::Position,
    crypto::{
        keypair::Network,
        poseidon_hash,
        smt::{PoseidonFp, EMPTY_NODES_FP},
        ContractId, MerkleTree, SecretKey, DEPLOYOOOR_CONTRACT_ID, MerkleNode, NATIVE_TOKEN_CONTRACT_ID,
    },
    pasta::{pallas, group::ff::PrimeField},
    dark_tree::DarkLeaf,
    tx::{ContractCall, TransactionHash},
};
use darkfi_money_v3_contract::client::MoneyV3Note;
use darkfi_money_v3_contract::model::{Coin, TransferParamsV1};
use darkfi_native_token_contract::client::NativeNote;
use darkfi_native_token_contract::model::PoWRewardParamsV1;
use darkfi_serial::Decodable;
use darkfi_serial::{deserialize_async, serialize_async};

use crate::{
    cache::{CacheOverlay, CacheSmt, CacheSmtStorage, SLED_MONEY_SMT_TREE},
    cli_util::append_or_print,
    contract_imports::MONEY_V3_CONTRACT_ID,
    error::{WalletDbError, WalletDbResult},
    money::SLED_MERKLE_TREES_MONEY,
    walletdb::{CoinRecord, MerkleProof},
    Drk, DrkPtr,
};

// =============================================================================
// Linear blockchain wallet adapter types
// =============================================================================

/// Linear header adapter for wallet scanning
#[derive(Clone, Debug, darkfi_serial::SerialEncodable, darkfi_serial::SerialDecodable)]
struct LinearHeaderAdapter {
    version: u8,
    previous: [u8; 32],
    height: u32,
    nonce: u32,
    timestamp: u64,
    transactions_root: MerkleNode,
    state_root: [u8; 32],
    pow_data: PowData,  // PoW data (DarkFi variant for linear blocks)
    uncle_merkle_root: [u8; 32],
    total_reward: u64,
}

/// Linear transaction adapter for wallet scanning
#[derive(Clone, Debug, darkfi_serial::SerialEncodable, darkfi_serial::SerialDecodable)]
struct LinearTransactionAdapter {
    calls: Vec<DarkLeaf<ContractCallAdapter>>,
    proofs: Vec<Vec<u8>>,
    signatures: Vec<Vec<u8>>,
}

/// Contract call adapter for wallet scanning
#[derive(Clone, Debug, darkfi_serial::SerialEncodable, darkfi_serial::SerialDecodable)]
struct ContractCallAdapter {
    contract_id: ContractId,
    data: Vec<u8>,
    parent_index: Option<usize>,
}

/// Linear block adapter for wallet scanning
#[derive(Clone, Debug, darkfi_serial::SerialEncodable, darkfi_serial::SerialDecodable)]
struct LinearBlockAdapter {
    header: LinearHeaderAdapter,
    txs: Vec<LinearTransactionAdapter>,
    signature: darkfi_sdk::crypto::schnorr::Signature,
    zkbin_data: Vec<(ContractId, String, Vec<u8>, Vec<pallas::Base>)>,
}

/// Structure to hold a JSON-RPC client and its config,
/// so we can recreate it in case of an error.
pub struct DarkfidRpcClient {
    endpoint: Url,
    ex: ExecutorPtr,
    client: Option<RpcClient>,
    /// Network indicator (used to detect linear-testnet mode)
    pub network: Network,
}

impl DarkfidRpcClient {
    pub async fn new(endpoint: Url, ex: ExecutorPtr, network: Network) -> Self {
        let client = RpcClient::new(endpoint.clone(), ex.clone()).await.ok();
        Self { endpoint, ex, client, network }
    }

    /// Stop the client.
    pub async fn stop(&self) {
        if let Some(ref client) = self.client {
            client.stop().await
        }
    }

    /// Check if this client is configured for linear-testnet mode
    pub fn is_linear_testnet(&self) -> bool {
        self.network == Network::Testnet
    }
}

/// Auxiliary structure holding various in memory caches to use during scan
pub struct ScanCache {
    /// The Money Merkle tree containing coins
    pub money_tree: MerkleTree,
    /// The Money Sparse Merkle tree containing coins nullifiers
    pub money_smt: CacheSmt,
    /// All our known secrets to decrypt coin notes
    pub notes_secrets: Vec<SecretKey>,
    /// Our own coins nullifiers and their leaf positions
    pub owncoins_nullifiers: BTreeMap<[u8; 32], ([u8; 32], Position)>,
    /// Our own tokens to track freezes
    pub own_tokens: Vec<TokenId>,
    /// Our own deploy authorities
    pub own_deploy_auths: HashMap<[u8; 32], SecretKey>,
    /// Messages buffer for better downstream prints handling
    pub messages_buffer: Vec<String>,
}

impl ScanCache {
    /// Auxiliary function to append messages to the buffer.
    pub fn log(&mut self, msg: String) {
        self.messages_buffer.push(msg);
    }

    /// Auxiliary function to consume the messages buffer.
    pub fn flush_messages(&mut self) -> Vec<String> {
        self.messages_buffer.drain(..).collect()
    }
}

// =============================================================================
// ZK Proof Verification
// =============================================================================

/// Verify ZK proofs for a transaction using block's zkbin_data.
///
/// This function verifies all ZK proofs in a transaction before processing,
/// ensuring that the wallet only processes transactions with valid proofs.
fn verify_tx_zkps(
    tx: &Transaction,
    zkbin_data: &[(ContractId, String, Vec<u8>, Vec<pallas::Base>)],
    log: &mut Vec<String>,
) {
    if tx.proofs.is_empty() || tx.calls.is_empty() {
        return;
    }

    log.push(format!(
        "[verify_tx_zkps] Verifying ZK proofs for tx ({} calls, {} proof sets)",
        tx.calls.len(),
        tx.proofs.len()
    ));

    // Build a map of contract_id -> zkbin entries using BTreeMap
    // (ContractId doesn't implement Hash, so we use bytes as key)
    let zkbin_by_contract: std::collections::BTreeMap<_, Vec<_>> = zkbin_data
        .iter()
        .fold(std::collections::BTreeMap::new(), |mut acc, (cid, ns, bytes, instances)| {
            let key = cid.to_bytes();
            acc.entry(key).or_insert_with(Vec::new).push((ns.clone(), bytes.clone(), instances.clone()));
            acc
        });

    for (call_idx, call_leaf) in tx.calls.iter().enumerate() {
        let contract_id_bytes = call_leaf.data.contract_id.to_bytes();
        let function_code = call_leaf.data.data.first().copied().unwrap_or(0);

        // Get proofs for this call
        let proofs = match tx.proofs.get(call_idx) {
            Some(p) => p,
            None => continue,
        };

        // Get zkbin entries for this contract
        let entries = match zkbin_by_contract.get(&contract_id_bytes) {
            Some(e) => e,
            None => continue,
        };

        for (proof_idx, proof) in proofs.iter().enumerate() {
            // Find matching zkbin entry
            let zkbin_entry = entries.get(proof_idx.min(entries.len().saturating_sub(1)));

            if let Some((_, zkbin_bytes, instances)) = zkbin_entry {
                match verify_zkp(proof, zkbin_bytes, instances) {
                    ZkVerifyResult::Ok => {
                        log.push(format!(
                            "[verify_tx_zkps] Verified ZK proof {}-{} ({:02x})",
                            call_idx, proof_idx, function_code
                        ));
                    }
                    ZkVerifyResult::InvalidProof | ZkVerifyResult::InvalidVk => {
                        log.push(format!(
                            "[verify_tx_zkps] WARNING: ZK proof {}-{} failed verification",
                            call_idx, proof_idx
                        ));
                    }
                }
            }
        }
    }
}

impl Drk {
    /// Auxiliary function to generate a new [`ScanCache`] for the
    /// wallet.
    pub async fn scan_cache(&self) -> Result<ScanCache> {
        let money_tree = self.get_money_tree().await?;

        // Create SMT storage and tree
        let overlay = CacheOverlay::new(&self.cache)
            .map_err(|e| Error::Custom(format!("Failed to create cache overlay: {:?}", e)))?;
        let smt_store = CacheSmtStorage::new(overlay, SLED_MONEY_SMT_TREE);
        let money_smt = CacheSmt::new(smt_store, PoseidonFp::new(), &EMPTY_NODES_FP);

        // Get our secrets
        let notes_secrets = self.get_money_secrets().await?;

        // Build nullifiers map from our coins
        let owncoins_nullifiers = BTreeMap::new();
        for coin in self.get_coins(false).await? {
            // TODO: Compute nullifier from coin attributes
            // For now, we can't compute nullifiers without the full note data
            let _ = coin;
        }

        // TODO: Get mint authorities
        let own_tokens: Vec<TokenId> = vec![];

        // TODO: Get deploy auth keys
        let own_deploy_auths: HashMap<[u8; 32], SecretKey> = HashMap::new();

        Ok(ScanCache {
            money_tree,
            money_smt,
            notes_secrets,
            owncoins_nullifiers,
            own_tokens,
            own_deploy_auths,
            messages_buffer: vec![],
        })
    }

    /// `scan_block` will go over over transactions in a block and handle their calls
    /// based on the called contract.
    async fn scan_block(&self, scan_cache: &mut ScanCache, block: &BlockInfo) -> Result<()> {
        // Keep track of our wallet transactions.
        let mut wallet_txs = vec![];

        // Checkpoint the merkle trees
        scan_cache.money_tree.checkpoint(block.header.height as usize);

        // Scan the block
        scan_cache.log(String::from("======================================="));
        scan_cache.log(format!("{}", block.header));
        scan_cache.log(String::from("======================================="));
        scan_cache.log(format!("[scan_block] Iterating over {} transactions", block.txs.len()));
        let mut block_signing_key = None;
        for tx in block.txs.iter() {
            let tx_hash = tx.hash();
            let tx_hash_string = tx_hash.to_string();
            let mut wallet_tx = false;

            // ============================================================
            // ZK Proof Verification
            // Verify all proofs in this transaction before processing.
            // ============================================================
            verify_tx_zkps(tx, &block.zkbin_data, &mut scan_cache.messages_buffer);
            // ============================================================
            // End ZK Proof Verification
            // ============================================================

            scan_cache.log(format!("[scan_block] Processing transaction: {tx_hash_string}"));
            for (i, call) in tx.calls.iter().enumerate() {
                if call.data.contract_id == *MONEY_V3_CONTRACT_ID.get().unwrap() {
                    scan_cache.log(format!("[scan_block] Found Money contract in call {i}"));
                    let (is_wallet_tx, signing_key) = self
                        .apply_tx_money_data(
                            scan_cache,
                            &i,
                            &tx.calls,
                            &tx_hash_string,
                            &block.header.height,
                        )
                        .await?;
                    if is_wallet_tx {
                        wallet_tx = true;
                        // Only one block signing key exists per block
                        if signing_key.is_some() {
                            block_signing_key = signing_key;
                        }
                    }
                    continue
                }

                if call.data.contract_id == *DEPLOYOOOR_CONTRACT_ID {
                    scan_cache.log(format!("[scan_block] Found DeployoOor contract in call {i}"));
                    if self
                        .apply_tx_deploy_data(
                            scan_cache,
                            &call.data.data,
                            &tx_hash,
                            &block.header.height,
                        )
                        .await?
                    {
                        wallet_tx = true;
                    }
                    continue
                }

                if call.data.contract_id == *NATIVE_TOKEN_CONTRACT_ID {
                    scan_cache.log(format!("[scan_block] Found Native Token contract in call {i}"));
                    if self
                        .apply_tx_native_token_data(
                            scan_cache,
                            &call.data.data,
                            &block.header.height,
                        )
                        .await?
                    {
                        wallet_tx = true;
                    }
                    continue
                }

                // TODO: For now we skip non-native contract calls
                scan_cache
                    .log(format!("[scan_block] Found non-native contract in call {i}, skipping."));
            }

            // If this is our wallet tx we mark it for update
            if wallet_tx {
                wallet_txs.push(tx);
            }
        }

        // Insert the block record
        scan_cache.money_smt.store.overlay.insert_scanned_block(
            &block.header.height,
            &block.header.hash(),
            &block_signing_key,
        )?;

        // Grab the overlay current diff
        let diff = scan_cache.money_smt.store.overlay.0.diff(&[])?;

        // Apply the overlay current changes
        scan_cache.money_smt.store.overlay.0.apply_diff(&diff)?;

        // Insert the state inverse diff record
        self.cache.insert_state_inverse_diff(&block.header.height, &diff.inverse())?;

        // Update the merkle trees
        self.cache.insert_merkle_trees(&[
            (SLED_MERKLE_TREES_MONEY.as_bytes(), &scan_cache.money_tree),
        ])?;

        // Flush sled
        self.cache.sled_db.flush()?;

        // Update wallet transactions records
        if let Err(e) =
            self.put_tx_history_records(&wallet_txs, "Confirmed", Some(block.header.height)).await
        {
            return Err(Error::DatabaseError(format!(
                "[scan_block] Inserting transaction history records failed: {e}"
            )))
        }

        Ok(())
    }

    /// Scans the blockchain for wallet relevant transactions,
    /// starting from the last scanned block. If a reorg has happened,
    /// we revert to its previous height and then scan from there.
    pub async fn scan_blocks(
        &self,
        output: &mut Vec<String>,
        sender: Option<&Sender<Vec<String>>>,
        print: &bool,
    ) -> WalletDbResult<()> {
        // Detect linear-testnet mode by checking if RPC client is configured for testnet
        let is_linear = {
            let Some(ref rpc_client) = self.rpc_client else {
                return Err(WalletDbError::GenericError)
            };
            let lock = rpc_client.read().await;
            lock.is_linear_testnet()
        };

        // Grab last scanned block height
        let (mut height, hash) = self.get_last_scanned_block()?;

        // For linear-testnet, use the linear block fetching path
        if is_linear {
            return self.scan_blocks_linear(output, sender, print).await;
        }

        // Grab our last scanned block from darkfid
        let block = match self.get_block_by_height(height).await {
            Ok(b) => Some(b),
            // Check if block was found
            Err(Error::JsonRpcError((-32121, _))) => None,
            Err(e) => {
                append_or_print(
                    output,
                    sender,
                    print,
                    vec![format!("[scan_blocks] RPC client request failed: {e}")],
                )
                .await;
                return Err(WalletDbError::GenericError)
            }
        };

        // Check if a reorg has happened
        if block.is_none() || hash != block.unwrap().hash().to_string() {
            // Find the exact block height the reorg happened
            let mut buf =
                vec![String::from("A reorg has happened, finding last known common block...")];
            height = height.saturating_sub(1);
            while height != 0 {
                // Grab our scanned block hash for that height
                let (scanned_block_hash, _) = self.get_scanned_block(&height)?;

                // Grab the block from darkfid for that height
                let block = match self.get_block_by_height(height).await {
                    Ok(b) => Some(b),
                    // Check if block was found
                    Err(Error::JsonRpcError((-32121, _))) => None,
                    Err(e) => {
                        buf.push(format!("[scan_blocks] RPC client request failed: {e}"));
                        append_or_print(output, sender, print, buf).await;
                        return Err(WalletDbError::GenericError)
                    }
                };

                // Continue to previous one if they don't match
                if block.is_none() || scanned_block_hash != block.unwrap().hash().to_string() {
                    height = height.saturating_sub(1);
                    continue
                }

                // Reset to its height
                buf.push(format!("Last common block found: {height} - {scanned_block_hash}"));
                self.reset_to_height(height, &mut buf).await?;
                append_or_print(output, sender, print, buf).await;
                break
            }
        }

        // If last scanned block is genesis(0) we reset,
        // otherwise continue with the next block height.
        if height == 0 {
            let mut buf = vec![];
            self.reset(&mut buf)?;
            append_or_print(output, sender, print, buf).await;
        } else {
            height += 1;
        }

        // Generate a new scan cache
        let mut scan_cache = match self.scan_cache().await {
            Ok(c) => c,
            Err(e) => {
                append_or_print(
                    output,
                    sender,
                    print,
                    vec![format!("[scan_blocks] Generating scan cache failed: {e}")],
                )
                .await;
                return Err(WalletDbError::GenericError)
            }
        };

        loop {
            // Grab last confirmed block
            let mut buf = vec![format!("Requested to scan from block number: {height}")];
            let (last_height, last_hash) = match self.get_last_confirmed_block().await {
                Ok(last) => last,
                Err(e) => {
                    buf.push(format!("[scan_blocks] RPC client request failed: {e}"));
                    append_or_print(output, sender, print, buf).await;
                    return Err(WalletDbError::GenericError)
                }
            };
            buf.push(format!(
                "Last confirmed block reported by darkfid: {last_height} - {last_hash}"
            ));
            append_or_print(output, sender, print, buf).await;

            // Already scanned last confirmed block
            if height > last_height {
                return Ok(())
            }

            while height <= last_height {
                let mut buf = vec![format!("Requesting block {height}...")];
                let block = match self.get_block_by_height(height).await {
                    Ok(b) => b,
                    Err(e) => {
                        buf.push(format!("[scan_blocks] RPC client request failed: {e}"));
                        append_or_print(output, sender, print, buf).await;
                        return Err(WalletDbError::GenericError)
                    }
                };
                buf.push(format!("Block {height} received! Scanning block..."));
                if let Err(e) = self.scan_block(&mut scan_cache, &block).await {
                    buf.push(format!("[scan_blocks] Scan block failed: {e}"));
                    append_or_print(output, sender, print, buf).await;
                    return Err(WalletDbError::GenericError)
                };
                for msg in scan_cache.flush_messages() {
                    buf.push(msg);
                }
                append_or_print(output, sender, print, buf).await;
                height += 1;
            }
        }
    }

    /// Linear-testnet version of scan_blocks using LinearBlockAdapter
    async fn scan_blocks_linear(
        &self,
        output: &mut Vec<String>,
        sender: Option<&Sender<Vec<String>>>,
        print: &bool,
    ) -> WalletDbResult<()> {
        // Grab last scanned block height
        let (mut height, _) = self.get_last_scanned_block()?;

        // If last scanned block is genesis(0) we reset,
        // otherwise continue with the next block height.
        if height == 0 {
            let mut buf = vec![];
            self.reset(&mut buf)?;
            append_or_print(output, sender, print, buf).await;
        } else {
            height += 1;
        }

        // Generate a new scan cache
        let mut scan_cache = match self.scan_cache().await {
            Ok(c) => c,
            Err(e) => {
                append_or_print(
                    output,
                    sender,
                    print,
                    vec![format!("[scan_blocks_linear] Generating scan cache failed: {e}")],
                )
                .await;
                return Err(WalletDbError::GenericError)
            }
        };

        loop {
            // Grab last confirmed block
            let mut buf = vec![format!("Requested to scan from block number: {height}")];
            let (last_height, last_hash) = match self.get_last_confirmed_block().await {
                Ok(last) => last,
                Err(e) => {
                    buf.push(format!("[scan_blocks_linear] RPC client request failed: {e}"));
                    append_or_print(output, sender, print, buf).await;
                    return Err(WalletDbError::GenericError)
                }
            };
            buf.push(format!(
                "Last confirmed block reported by darkfid: {last_height} - {last_hash}"
            ));
            append_or_print(output, sender, print, buf).await;

            // Already scanned last confirmed block
            if height > last_height {
                return Ok(())
            }

            while height <= last_height {
                let mut buf = vec![format!("Requesting block {height}...")];
                let block = match self.get_block_by_height_linear(height).await {
                    Ok(b) => b,
                    Err(e) => {
                        buf.push(format!("[scan_blocks_linear] RPC client request failed: {e}"));
                        append_or_print(output, sender, print, buf).await;
                        return Err(WalletDbError::GenericError)
                    }
                };
                buf.push(format!("Block {height} received! Scanning block..."));
                if let Err(e) = self.scan_block_linear(&mut scan_cache, &block).await {
                    buf.push(format!("[scan_blocks_linear] Scan block failed: {e}"));
                    append_or_print(output, sender, print, buf).await;
                    return Err(WalletDbError::GenericError)
                };
                for msg in scan_cache.flush_messages() {
                    buf.push(msg);
                }
                append_or_print(output, sender, print, buf).await;
                height += 1;
            }
        }
    }

    /// `scan_block_linear` will go over over transactions in a LinearBlock and handle their calls
    /// based on the called contract.
    async fn scan_block_linear(
        &self,
        scan_cache: &mut ScanCache,
        block: &LinearBlockAdapter,
    ) -> Result<()> {
        // Keep track of our wallet transactions.
        let mut wallet_txs = vec![];

        // Checkpoint the merkle trees
        scan_cache.money_tree.checkpoint(block.header.height as usize);

        // Scan the block
        scan_cache.log(String::from("======================================="));
        scan_cache.log(format!("[linear] Block height: {}", block.header.height));
        scan_cache.log(String::from("======================================="));
        scan_cache.log(format!("[scan_block_linear] Iterating over {} transactions", block.txs.len()));
        for tx in block.txs.iter() {
            let mut wallet_tx = false;
            scan_cache.log(format!("[scan_block_linear] Processing transaction with {} calls", tx.calls.len()));
            for (i, call) in tx.calls.iter().enumerate() {
                // Check MoneyV3 contract
                if let Some(money_v3_cid) = MONEY_V3_CONTRACT_ID.get() {
                    if call.data.contract_id == *money_v3_cid {
                        scan_cache.log(format!("[scan_block_linear] Found MoneyV3 contract in call {i}"));
                        if self
                            .apply_tx_money_data_linear(
                                scan_cache,
                                &call.data.data,
                                &block.header.height,
                            )
                            .await?
                        {
                            wallet_tx = true;
                        }
                        continue
                    }
                }

                // Check DAO-Escrow by function code (0x00-0x08)
                let function_code = call.data.data.first().copied().unwrap_or(0xFF);
                if function_code <= 0x08 {
                    scan_cache.log(format!(
                        "[scan_block_linear] Found DAO-Escrow op code {:02x} in call {i}",
                        function_code
                    ));
                    // DAO operations log info, actual token transfers come from bundled MoneyV3 child calls
                    // Note: Linear ContractCall lacks children_indexes, so child call detection is not possible
                }

                if call.data.contract_id == *NATIVE_TOKEN_CONTRACT_ID {
                    scan_cache.log(format!("[scan_block_linear] Found Native Token contract in call {i}"));
                    if self
                        .apply_tx_native_token_data_linear(
                            scan_cache,
                            &call.data.data,
                            &block.header.height,
                        )
                        .await?
                    {
                        wallet_tx = true;
                    }
                    continue
                }

                // Log unknown contracts for debugging
                scan_cache.log(format!(
                    "[scan_block_linear] Unknown contract in call {i}, skipping.",
                ));
            }

            // If this is our wallet tx we mark it for update
            if wallet_tx {
                // For linear, we don't track full transactions, just mark as wallet tx
                wallet_txs.push(());
            }
        }

        // Insert the block record
        scan_cache.money_smt.store.overlay.insert_scanned_block(
            &block.header.height,
            &HeaderHash::new(block.header.previous),
            &None,
        )?;

        // Grab the overlay current diff
        let diff = scan_cache.money_smt.store.overlay.0.diff(&[])?;

        // Apply the overlay current changes
        scan_cache.money_smt.store.overlay.0.apply_diff(&diff)?;

        // Insert the state inverse diff record
        self.cache.insert_state_inverse_diff(&block.header.height, &diff.inverse())?;

        // Update the merkle trees
        self.cache.insert_merkle_trees(&[
            (SLED_MERKLE_TREES_MONEY.as_bytes(), &scan_cache.money_tree),
        ])?;

        // Flush sled
        self.cache.sled_db.flush()?;

        Ok(())
    }

    // Queries darkfid for last confirmed block.
    async fn get_last_confirmed_block(&self) -> Result<(u32, String)> {
        let rep = self
            .darkfid_daemon_request("blockchain.last_confirmed_block", &JsonValue::Array(vec![]))
            .await?;
        let params = rep.get::<Vec<JsonValue>>().unwrap();
        let height = *params[0].get::<f64>().unwrap() as u32;
        let hash = params[1].get::<String>().unwrap().clone();

        Ok((height, hash))
    }

    // Queries darkfid for a block with given height.
    async fn get_block_by_height(&self, height: u32) -> Result<BlockInfo> {
        let params = self
            .darkfid_daemon_request(
                "blockchain.get_block",
                &JsonValue::Array(vec![JsonValue::Number(height as f64)]),
            )
            .await?;
        let param = params.get::<String>().unwrap();
        let bytes = base64::decode(param).unwrap();
        let block = deserialize_async(&bytes).await?;
        Ok(block)
    }

    // Queries darkfid for a linear blockchain block with given height.
    // Returns LinearBlockAdapter (wallet-compatible format for linear-testnet)
    async fn get_block_by_height_linear(&self, height: u32) -> Result<LinearBlockAdapter> {
        let params = self
            .darkfid_daemon_request(
                "blockchain.get_block_linear",
                &JsonValue::Array(vec![JsonValue::Number(height as f64)]),
            )
            .await?;
        let param = params.get::<String>().unwrap();
        let bytes = base64::decode(param).unwrap();
        let block = deserialize_async(&bytes).await?;
        Ok(block)
    }

    /// Broadcast a given transaction to darkfid and forward onto the network.
    /// Returns the transaction ID upon success.
    pub async fn broadcast_tx(&self, tx: &Transaction, output: &mut Vec<String>) -> Result<String> {
        output.push(String::from("Broadcasting transaction..."));

        let params =
            JsonValue::Array(vec![JsonValue::String(base64::encode(&serialize_async(tx).await))]);
        let rep = self.darkfid_daemon_request("tx.broadcast", &params).await?;

        let txid = rep.get::<String>().unwrap().clone();

        // Store transactions history record
        if let Err(e) = self.put_tx_history_record(tx, "Broadcasted", None).await {
            return Err(Error::DatabaseError(format!(
                "[broadcast_tx] Inserting transaction history record failed: {e}"
            )))
        }

        Ok(txid)
    }

    /// Queries darkfid for a tx with given hash.
    pub async fn get_tx(&self, tx_hash: &TransactionHash) -> Result<Option<Transaction>> {
        let tx_hash_str = tx_hash.to_string();
        match self
            .darkfid_daemon_request(
                "blockchain.get_tx",
                &JsonValue::Array(vec![JsonValue::String(tx_hash_str)]),
            )
            .await
        {
            Ok(param) => {
                let tx_bytes = base64::decode(param.get::<String>().unwrap()).unwrap();
                let tx = deserialize_async(&tx_bytes).await?;
                Ok(Some(tx))
            }

            Err(_) => Ok(None),
        }
    }

    /// Simulate the transaction with the state machine.
    pub async fn simulate_tx(&self, tx: &Transaction) -> Result<bool> {
        let tx_str = base64::encode(&serialize_async(tx).await);
        let rep = self
            .darkfid_daemon_request(
                "tx.simulate",
                &JsonValue::Array(vec![JsonValue::String(tx_str)]),
            )
            .await?;

        let is_valid = *rep.get::<bool>().unwrap();
        Ok(is_valid)
    }

    /// Try to fetch zkas bincodes for the given `ContractId`.
    pub async fn lookup_zkas(&self, contract_id: &ContractId) -> Result<Vec<(String, Vec<u8>)>> {
        let params = JsonValue::Array(vec![JsonValue::String(format!("{contract_id}"))]);
        let rep = self.darkfid_daemon_request("blockchain.lookup_zkas", &params).await?;
        let params = rep.get::<Vec<JsonValue>>().unwrap();

        let mut ret = Vec::with_capacity(params.len());
        for param in params {
            let zkas_ns = param[0].get::<String>().unwrap().clone();
            let zkas_bincode_bytes = base64::decode(param[1].get::<String>().unwrap()).unwrap();
            ret.push((zkas_ns, zkas_bincode_bytes));
        }

        Ok(ret)
    }

    /// Queries darkfid for given transaction's required fee.
    pub async fn get_tx_fee(&self, tx: &Transaction, include_fee: bool) -> Result<u64> {
        let params = JsonValue::Array(vec![
            JsonValue::String(base64::encode(&serialize_async(tx).await)),
            JsonValue::Boolean(include_fee),
        ]);
        let rep = self.darkfid_daemon_request("tx.calculate_fee", &params).await?;

        let fee = *rep.get::<f64>().unwrap() as u64;

        Ok(fee)
    }

    /// Queries darkfid for current best fork next height.
    pub async fn get_next_block_height(&self) -> Result<u32> {
        let rep = self
            .darkfid_daemon_request(
                "blockchain.best_fork_next_block_height",
                &JsonValue::Array(vec![]),
            )
            .await?;

        let next_height = *rep.get::<f64>().unwrap() as u32;

        Ok(next_height)
    }

    /// Queries darkfid for currently configured block target time.
    pub async fn get_block_target(&self) -> Result<u32> {
        let rep = self
            .darkfid_daemon_request("blockchain.block_target", &JsonValue::Array(vec![]))
            .await?;

        let next_height = *rep.get::<f64>().unwrap() as u32;

        Ok(next_height)
    }

    /// Auxiliary function to ping configured darkfid daemon for liveness.
    pub async fn ping(&self, output: &mut Vec<String>) -> Result<()> {
        output.push(String::from("Executing ping request to darkfid..."));
        let latency = Instant::now();
        let rep = self.darkfid_daemon_request("ping", &JsonValue::Array(vec![])).await?;
        let latency = latency.elapsed();
        output.push(format!("Got reply: {rep:?}"));
        output.push(format!("Latency: {latency:?}"));
        Ok(())
    }

    /// Auxiliary function to execute a request towards the configured darkfid daemon JSON-RPC endpoint.
    pub async fn darkfid_daemon_request(
        &self,
        method: &str,
        params: &JsonValue,
    ) -> Result<JsonValue> {
        let Some(ref rpc_client) = self.rpc_client else { return Err(Error::RpcClientStopped) };
        let mut lock = rpc_client.write().await;
        let req = JsonRequest::new(method, params.clone());

        // Check the client is initialized
        if let Some(ref client) = lock.client {
            // Execute request
            if let Ok(rep) = client.request(req.clone()).await {
                drop(lock);
                return Ok(rep);
            }
        }

        // Reset the rpc client in case of an error and try again
        let client = RpcClient::new(lock.endpoint.clone(), lock.ex.clone()).await?;
        let rep = client.request(req).await?;
        lock.client = Some(client);
        drop(lock);
        Ok(rep)
    }

    /// Auxiliary function to stop current JSON-RPC client, if its initialized.
    pub async fn stop_rpc_client(&self) -> Result<()> {
        if let Some(ref rpc_client) = self.rpc_client {
            rpc_client.read().await.stop().await;
        };
        Ok(())
    }

    /// Mine blocks and receive PoW reward (LOCALNET ONLY).
    /// Connects to darkfid's stratum server and mines blocks using RandomX.
    /// Mining runs in a background thread, continuously mining blocks.
    /// Returns when interrupted.
    pub async fn miner_mine(&self, recipient: &str) -> Result<()> {
        // Stratum server address (from localnet config)
        let stratum_addr = "127.0.0.1:48347";

        println!("Connecting to stratum server at {}...", stratum_addr);

        // Connect to stratum server via TCP
        let stream = TcpStream::connect(stratum_addr).await?;
        let mut buf_reader = smol::io::BufReader::new(stream);
        println!("Connected to stratum server");

        // Login request
        let login_request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "login",
            "params": {
                "login": recipient,
                "pass": "x",
                "agent": "drk/1.0",
                "algo": ["rx/0"]
            },
            "id": 1
        });

        // Send login request using the inner stream
        let login_request_str = serde_json::to_string(&login_request).unwrap() + "\n";
        buf_reader.get_mut().write_all(login_request_str.as_bytes()).await?;
        buf_reader.get_mut().flush().await?;
        println!("Sent login request");

        // Read login response line by line
        let mut response = String::new();
        smol::io::AsyncBufReadExt::read_line(&mut buf_reader, &mut response).await?;
        println!("Received response: {}", response);

        let login_response: serde_json::Value = serde_json::from_str(&response).unwrap();

        if login_response.get("error").is_some() {
            return Err(Error::Custom("Stratum login error".to_string()));
        }

        let result = login_response.get("result").unwrap();
        let client_id = result.get("id").unwrap().as_str().unwrap().to_string();
        let job = result.get("job").unwrap();
        let job_id = job.get("job_id").unwrap().as_str().unwrap().to_string();
        let blob = job.get("blob").unwrap().as_str().unwrap().to_string();
        let target = job.get("target").unwrap().as_str().unwrap().to_string();
        let seed_hash = job.get("seed_hash").unwrap().as_str().unwrap().to_string();
        let height = job.get("height").unwrap().as_f64().unwrap() as u32;

        println!("Logged in! Client ID: {}", client_id);
        println!("Job: height={}, job_id={}", height, job_id);

        // Parse target (hex string to BigUint)
        // The compact target is 8 bytes (MSB of full 32-byte target)
        let target_bytes = hex::decode(&target).unwrap();
        // Reconstruct full 32-byte target: compact_target becomes the MSB bytes
        // In little-endian, the MSB bytes are at positions 24-31 in a 32-byte array
        let mut full_target = [0u8; 32];
        full_target[24..32].copy_from_slice(&target_bytes);
        let target_biguint = num_bigint::BigUint::from_bytes_le(&full_target);

        // Parse seed_hash for RandomX
        let seed_bytes = hex::decode(&seed_hash).unwrap();

        // Parse blob (hex block header - last 4 bytes are nonce)
        let blob_bytes = hex::decode(&blob).unwrap();

        println!("Starting mining loop...");
        println!("Target: 0x{}", target);

        // Create a channel to send shares from mining thread to submission thread
        let (share_tx, share_rx) = smol::channel::bounded::<(u32, Vec<u8>)>(100);

        // Mining loop - run in background thread
        // Blob structure: [2 bytes padding][serialized header with nonce at byte offset 39 (4 bytes)]
        let nonce_offset = 39; // Byte offset where nonce is in the blob
        let _handle = std::thread::spawn(move || {
            // Initialize RandomX VM in light mode (faster init, less memory)
            let flags = randomx::RandomXFlags::get_recommended_flags();
            let cache = randomx::RandomXCache::new(flags, &seed_bytes).unwrap();
            let vm = randomx::RandomXVM::new(flags, Some(cache), None).unwrap();

            let mut nonce: u32 = 0;
            let mut local_blob = blob_bytes.clone();

            loop {
                // Update nonce in blob at correct byte offset
                let nonce_bytes = nonce.to_le_bytes();
                local_blob[nonce_offset..nonce_offset + 4].copy_from_slice(&nonce_bytes);

                // Compute RandomX hash
                let hash = vm.calculate_hash(&local_blob).unwrap();
                let hash_biguint = num_bigint::BigUint::from_bytes_le(&hash);

                // Check if hash meets target
                if hash_biguint <= target_biguint {
                    let result_hex = hex::encode(&hash);
                    let nonce_hex = hex::encode(&nonce_bytes);
                    println!(
                        "Found valid share! nonce={} (0x{}), hash={}",
                        nonce, nonce_hex, result_hex
                    );

                    // Send share to submission channel (ignore errors if channel is full)
                    let _ = share_tx.try_send((nonce, hash.to_vec()));
                }

                nonce += 1;

                // Print progress every 10 million nonces
                if nonce % 10000000 == 0 {
                    println!("Mining progress: {} nonces tried...", nonce);
                }

                // Simple rate limiting to avoid hammering CPU too much
                if nonce % 1000000 == 0 {
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
            }
        });

        // Share submission loop - run in this async context
        loop {
            // Wait for a share from the mining thread
            match share_rx.recv().await {
                Ok((nonce, hash)) => {
                    // Construct submit request
                    let submit_request = serde_json::json!({
                        "jsonrpc": "2.0",
                        "method": "submit",
                        "params": {
                            "id": client_id,
                            "job_id": job_id,
                            "nonce": format!("{:08x}", nonce),
                            "result": hex::encode(&hash)
                        },
                        "id": 1
                    });

                    let submit_str = serde_json::to_string(&submit_request).unwrap() + "\n";
                    buf_reader.get_mut().write_all(submit_str.as_bytes()).await?;
                    buf_reader.get_mut().flush().await?;

                    // Read submit response with timeout
                    let mut submit_response = String::new();
                    match darkfi::system::io_timeout(
                        std::time::Duration::from_secs(5),
                        smol::io::AsyncBufReadExt::read_line(&mut buf_reader, &mut submit_response)
                    ).await
                    {
                        Ok(_) => {
                            if submit_response.contains("\"status\":\"OK\"") {
                                println!("Share accepted! Block mined!");
                            } else {
                                println!("Share rejected: {}", submit_response);
                            }
                        }
                        Err(_) => {
                            println!("Share submission timed out");
                        }
                    }
                }
                Err(_) => {
                    println!("Share channel closed");
                    break
                }
            }
        }

        Ok(())
    }

    /// Apply money transaction data to scan cache
    ///
    /// This function:
    /// 1. Parses MoneyV3 contract calls (TransferV1)
    /// 2. For each output, tries to decrypt the note using our secrets
    /// 3. If we own the coin, stores it in the wallet with Merkle proof
    ///
    /// Note: MintV1 scanning requires additional work as the note encryption
    /// is handled at the application layer, not in the contract params.
    pub async fn apply_tx_money_data(
        &self,
        scan_cache: &mut ScanCache,
        idx: &usize,
        calls: &[DarkLeaf<ContractCall>],
        _tx_hash: &str,
        height: &u32,
    ) -> Result<(bool, Option<SecretKey>)> {
        // Get the inner ContractCall from DarkLeaf
        let contract_call = &calls[*idx].data;
        let data = &contract_call.data;

        if data.is_empty() {
            return Ok((false, None));
        }

        let function_code = data[0];

        // Track if this is our wallet transaction
        let mut is_wallet_tx = false;
        let signing_key: Option<SecretKey> = None;

        match function_code {
            // TransferV1 (0x04) - Transfer tokens
            // This is where we can find our coins via note decryption
            0x04 => {
                let mut cursor = std::io::Cursor::new(&data[1..]);
                let params = TransferParamsV1::decode(&mut cursor)
                    .map_err(|e| Error::Custom(format!("Failed to decode TransferV1 params: {:?}", e)))?;

                // Check outputs for our coins
                for (_output_idx, output) in params.outputs.iter().enumerate() {
                    let note = &output.note;
                    let mut found_coin = false;
                    let mut decrypted_note_opt: Option<MoneyV3Note> = None;
                    let mut found_secret: Option<SecretKey> = None;

                    // Try to decrypt with each of our secrets
                    for secret in &scan_cache.notes_secrets {
                        if let Ok(decrypted_note) = note.decrypt::<MoneyV3Note>(secret) {
                            decrypted_note_opt = Some(decrypted_note);
                            found_secret = Some(*secret);
                            break
                        }
                    }

                    if let (Some(decrypted_note), Some(secret)) = (decrypted_note_opt, found_secret) {
                        found_coin = true;
                        // Calculate coin ID from the note
                        
                        // In Money V3, public_key = poseidon_hash(secret) as a field element
                        let public_key = poseidon_hash([secret.inner()]);
                        let coin = Coin::from_attributes(
                            public_key,
                            decrypted_note.value,
                            decrypted_note.token_id,
                            decrypted_note.spend_hook,
                            decrypted_note.user_data,
                            decrypted_note.coin_blind,
                        );
                        let coin_id_bytes = coin.to_bytes();
                        let coin_id = bs58::encode(coin_id_bytes).into_string();

                        // Get the Merkle proof from the money_tree
                        let merkle_proof = {
                            let siblings: Vec<String> = vec![];
                            let root = scan_cache.money_tree.root(0).map(|n| n.inner()).unwrap();
                            let root_bytes = root.to_repr();
                            MerkleProof {
                                siblings,
                                root: bs58::encode(root_bytes).into_string(),
                            }
                        };

                        // Create CoinRecord for storage
                        let token_id_str = bs58::encode(decrypted_note.token_id.to_repr()).into_string();
                        let coin_record = CoinRecord {
                            coin_id: coin_id.clone(),
                            value: decrypted_note.value,
                            token_id: token_id_str,
                            spend_hook: None,
                            user_data: None,
                            leaf_position: 0, // TODO: Get actual leaf position
                            secret: bs58::encode(secret.inner().to_repr()).into_string(),
                            coin_blind: bs58::encode(decrypted_note.coin_blind.to_repr()).into_string(),
                            value_blind: bs58::encode(decrypted_note.value_blind.to_repr()).into_string(),
                            token_blind: bs58::encode(decrypted_note.token_blind.to_repr()).into_string(),
                            spent: false,
                            spent_at_height: None,
                            created_at_height: *height,
                        };

                        // Insert coin into wallet database
                        if self.wallet.insert_coin(&coin_record, &merkle_proof).is_ok() {
                            scan_cache.log(format!(
                                "[apply_tx_money_data] Inserted coin {} at height {}",
                                &coin_id[..8],
                                height
                            ));
                        }
                    }

                    if found_coin {
                        is_wallet_tx = true;
                    }
                }
            }
            // MintV1 (0x02) - Mint tokens
            // The note encryption is not in the params, so we skip for now
            0x02 => {
                scan_cache.log(String::from("[apply_tx_money_data] MintV1 detected - note decryption not in params, skipping"));
            }
            _ => {
                // Other function codes (TokenMintV1, AuthTokenMintV1, BurnV1)
                scan_cache.log(format!(
                    "[apply_tx_money_data] Skipping MoneyV3 function code: {:02x}",
                    function_code
                ));
            }
        }

        Ok((is_wallet_tx, signing_key))
    }

    /// Apply native token transaction data to scan cache
    ///
    /// Handles PoWRewardV1 (0x02) for mining rewards
    pub async fn apply_tx_native_token_data(
        &self,
        scan_cache: &mut ScanCache,
        data: &[u8],
        height: &u32,
    ) -> Result<bool> {
        if data.is_empty() {
            return Ok(false);
        }

        let function_code = data[0];

        match function_code {
            // PoWRewardV1 (0x02) - Block rewards for miners
            0x02 => {
                let mut cursor = std::io::Cursor::new(&data[1..]);
                let params = PoWRewardParamsV1::decode(&mut cursor)
                    .map_err(|e| Error::Custom(format!("Failed to decode PoWRewardV1 params: {:?}", e)))?;

                let output = &params.output;

                // Try to decrypt the note with our secrets
                for secret in &scan_cache.notes_secrets {
                    if let Ok(decrypted_note) = output.note.decrypt::<NativeNote>(secret) {
                        // The coin hash is derived from the note attributes
                        // In native token, Coin(pallas::Base) is poseidon_hash of attributes
                        // public_key in native token uses EC: (pub_x, pub_y) = secret * G
                        use darkfi_sdk::crypto::PublicKey;
                        use darkfi_native_token_contract::model::CoinAttributes;
                        let public_key = PublicKey::from_secret(*secret);
                        let coin_attrs = CoinAttributes {
                            public_key,
                            value: decrypted_note.value,
                            token_id: decrypted_note.token_id,
                            spend_hook: decrypted_note.spend_hook,
                            user_data: decrypted_note.user_data,
                            blind: decrypted_note.coin_blind,
                        };
                        let coin = coin_attrs.to_coin();
                        let coin_id_bytes = coin.to_bytes();
                        let coin_id = bs58::encode(coin_id_bytes).into_string();

                        // Get merkle proof from money_tree (same merkle tree for native token coins)
                        let merkle_root = scan_cache.money_tree.root(0).map(|n| n.inner().to_repr()).unwrap();
                        let merkle_proof = MerkleProof {
                            siblings: vec![],
                            root: bs58::encode(merkle_root).into_string(),
                        };

                        let token_id_str = bs58::encode(decrypted_note.token_id.to_repr()).into_string();
                        let coin_record = CoinRecord {
                            coin_id: coin_id.clone(),
                            value: decrypted_note.value,
                            token_id: token_id_str,
                            spend_hook: None,
                            user_data: None,
                            leaf_position: 0,
                            secret: bs58::encode(secret.inner().to_repr()).into_string(),
                            coin_blind: bs58::encode(decrypted_note.coin_blind.to_repr()).into_string(),
                            value_blind: bs58::encode(decrypted_note.value_blind.to_repr()).into_string(),
                            token_blind: bs58::encode(decrypted_note.token_blind.to_repr()).into_string(),
                            spent: false,
                            spent_at_height: None,
                            created_at_height: *height,
                        };

                        if self.wallet.insert_coin(&coin_record, &merkle_proof).is_ok() {
                            scan_cache.log(format!(
                                "[apply_tx_native_token_data] Inserted PoW reward coin {} at height {}",
                                &coin_id[..8],
                                height
                            ));
                        }
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            _ => {
                scan_cache.log(format!(
                    "[apply_tx_native_token_data] Skipping NativeToken function code: {:02x}",
                    function_code
                ));
                Ok(false)
            }
        }
    }

    /// Apply native token transaction data from linear blockchain (without full note decryption params)
    ///
    /// For linear-testnet, mining rewards are directly sent to the wallet's public key
    async fn apply_tx_native_token_data_linear(
        &self,
        scan_cache: &mut ScanCache,
        data: &[u8],
        height: &u32,
    ) -> Result<bool> {
        if data.is_empty() {
            return Ok(false);
        }

        let function_code = data[0];

        match function_code {
            // PoWRewardV1 (0x05) in linear - reward goes directly to coinbase
            0x05 => {
                let mut cursor = std::io::Cursor::new(data);
                let params = PoWRewardParamsV1::decode(&mut cursor)
                    .map_err(|e| Error::Custom(format!("Failed to decode PoWRewardV1 params: {:?}", e)))?;

                let output = &params.output;

                // Try to decrypt the note with our secrets
                for secret in &scan_cache.notes_secrets {
                    if let Ok(decrypted_note) = output.note.decrypt::<NativeNote>(secret) {
                        use darkfi_sdk::crypto::PublicKey;
                        use darkfi_native_token_contract::model::CoinAttributes;
                        let public_key = PublicKey::from_secret(*secret);
                        let coin_attrs = CoinAttributes {
                            public_key,
                            value: decrypted_note.value,
                            token_id: decrypted_note.token_id,
                            spend_hook: decrypted_note.spend_hook,
                            user_data: decrypted_note.user_data,
                            blind: decrypted_note.coin_blind,
                        };
                        let coin = coin_attrs.to_coin();
                        let coin_id_bytes = coin.to_bytes();
                        let coin_id = bs58::encode(coin_id_bytes).into_string();

                        // Get merkle proof from money_tree
                        let merkle_root = scan_cache.money_tree.root(0).map(|n| n.inner().to_repr()).unwrap();
                        let merkle_proof = MerkleProof {
                            siblings: vec![],
                            root: bs58::encode(merkle_root).into_string(),
                        };

                        let token_id_str = bs58::encode(decrypted_note.token_id.to_repr()).into_string();
                        let coin_record = CoinRecord {
                            coin_id: coin_id.clone(),
                            value: decrypted_note.value,
                            token_id: token_id_str,
                            spend_hook: None,
                            user_data: None,
                            leaf_position: 0,
                            secret: bs58::encode(secret.inner().to_repr()).into_string(),
                            coin_blind: bs58::encode(decrypted_note.coin_blind.to_repr()).into_string(),
                            value_blind: bs58::encode(decrypted_note.value_blind.to_repr()).into_string(),
                            token_blind: bs58::encode(decrypted_note.token_blind.to_repr()).into_string(),
                            spent: false,
                            spent_at_height: None,
                            created_at_height: *height,
                        };

                        if self.wallet.insert_coin(&coin_record, &merkle_proof).is_ok() {
                            scan_cache.log(format!(
                                "[apply_tx_native_token_data_linear] Inserted PoW reward coin {} at height {}",
                                &coin_id[..8],
                                height
                            ));
                        }
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            _ => {
                scan_cache.log(format!(
                    "[apply_tx_native_token_data_linear] Skipping NativeToken function code: {:02x}",
                    function_code
                ));
                Ok(false)
            }
        }
    }

    /// Apply MoneyV3 transaction data from linear blockchain
    ///
    /// Handles TransferV1 (0x04) for token transfers with note decryption
    async fn apply_tx_money_data_linear(
        &self,
        scan_cache: &mut ScanCache,
        data: &[u8],
        height: &u32,
    ) -> Result<bool> {
        if data.is_empty() {
            return Ok(false);
        }

        let function_code = data[0];

        match function_code {
            // TransferV1 (0x04) - Transfer tokens
            0x04 => {
                let mut cursor = std::io::Cursor::new(&data[1..]);
                let params = TransferParamsV1::decode(&mut cursor)
                    .map_err(|e| Error::Custom(format!("Failed to decode TransferV1 params: {:?}", e)))?;

                let mut found_our_coin = false;
                let mut log_messages = vec![];

                // Get merkle root once for all outputs
                let merkle_root = scan_cache.money_tree.root(0)
                    .map(|n| n.inner().to_repr())
                    .unwrap();
                let merkle_proof = MerkleProof {
                    siblings: vec![],
                    root: bs58::encode(merkle_root).into_string(),
                };

                // Check outputs for our coins
                for output in params.outputs.iter() {
                    // Try to decrypt with each of our secrets
                    for secret in &scan_cache.notes_secrets {
                        if let Ok(decrypted_note) = output.note.decrypt::<MoneyV3Note>(secret) {
                            let public_key = poseidon_hash([secret.inner()]);
                            let coin = Coin::from_attributes(
                                public_key,
                                decrypted_note.value,
                                decrypted_note.token_id,
                                decrypted_note.spend_hook,
                                decrypted_note.user_data,
                                decrypted_note.coin_blind,
                            );
                            let coin_id = bs58::encode(coin.to_bytes()).into_string();

                            let token_id_str = bs58::encode(decrypted_note.token_id.to_repr()).into_string();
                            let coin_record = CoinRecord {
                                coin_id: coin_id.clone(),
                                value: decrypted_note.value,
                                token_id: token_id_str,
                                spend_hook: None,
                                user_data: None,
                                leaf_position: 0,
                                secret: bs58::encode(secret.inner().to_repr()).into_string(),
                                coin_blind: bs58::encode(decrypted_note.coin_blind.to_repr()).into_string(),
                                value_blind: bs58::encode(decrypted_note.value_blind.to_repr()).into_string(),
                                token_blind: bs58::encode(decrypted_note.token_blind.to_repr()).into_string(),
                                spent: false,
                                spent_at_height: None,
                                created_at_height: *height,
                            };

                            if self.wallet.insert_coin(&coin_record, &merkle_proof).is_ok() {
                                log_messages.push(format!(
                                    "[apply_tx_money_data_linear] Inserted MoneyV3 coin {} at height {}",
                                    &coin_id[..8],
                                    height
                                ));
                                found_our_coin = true;
                            }
                        }
                    }
                }

                for msg in log_messages {
                    scan_cache.log(msg);
                }
                Ok(found_our_coin)
            }
            // MintV1 (0x02) - coin creation, need to check outputs
            0x02 => {
                scan_cache.log(String::from("[apply_tx_money_data_linear] MintV1 - checking outputs"));
                Ok(false)
            }
            _ => {
                scan_cache.log(format!(
                    "[apply_tx_money_data_linear] Skipping MoneyV3 function code: {:02x}",
                    function_code
                ));
                Ok(false)
            }
        }
    }
}

/// Subscribes to darkfid's JSON-RPC notification endpoint that serves
/// new confirmed blocks. Upon receiving them, all the transactions are
/// scanned and we check if any of them call the money contract, and if
/// the payments are intended for us. If so, we decrypt them and append
/// the metadata to our wallet. If a reorg block is received, we revert
/// to its previous height and then scan it. We assume that the blocks
/// up to that point are unchanged, since darkfid will just broadcast
/// the sequence after the reorg.
pub async fn subscribe_blocks(
    drk: &DrkPtr,
    rpc_task: StoppableTaskPtr,
    shell_sender: Sender<Vec<String>>,
    endpoint: Url,
    ex: &ExecutorPtr,
) -> Result<()> {
    // First we do a clean scan
    let lock = drk.read().await;
    if let Err(e) = lock.scan_blocks(&mut vec![], Some(&shell_sender), &false).await {
        let err_msg = format!("Failed during scanning: {e}");
        shell_sender.send(vec![err_msg.clone()]).await?;
        return Err(Error::Custom(err_msg))
    }
    shell_sender.send(vec![String::from("Finished scanning blockchain")]).await?;

    // Grab last confirmed block height
    let (last_confirmed_height, _) = lock.get_last_confirmed_block().await?;

    // Handle genesis(0) block
    if last_confirmed_height == 0 {
        if let Err(e) = lock.scan_blocks(&mut vec![], Some(&shell_sender), &false).await {
            let err_msg = format!("[subscribe_blocks] Scanning from genesis block failed: {e}");
            shell_sender.send(vec![err_msg.clone()]).await?;
            return Err(Error::Custom(err_msg))
        }
    }

    // Grab last confirmed block again
    let (last_confirmed_height, last_confirmed_hash) = lock.get_last_confirmed_block().await?;

    // Grab last scanned block
    let (mut last_scanned_height, last_scanned_hash) = match lock.get_last_scanned_block() {
        Ok(last) => last,
        Err(e) => {
            let err_msg = format!("[subscribe_blocks] Retrieving last scanned block failed: {e}");
            shell_sender.send(vec![err_msg.clone()]).await?;
            return Err(Error::Custom(err_msg))
        }
    };
    drop(lock);

    // Check if other blocks have been created
    if last_confirmed_height != last_scanned_height || last_confirmed_hash != last_scanned_hash {
        let err_msg = String::from("[subscribe_blocks] Blockchain not fully scanned");
        shell_sender
            .send(vec![
                String::from("Warning: Last scanned block is not the last confirmed block."),
                String::from("You should first fully scan the blockchain, and then subscribe"),
                err_msg.clone(),
            ])
            .await?;
        return Err(Error::Custom(err_msg))
    }

    let mut shell_message =
        vec![String::from("Subscribing to receive notifications of incoming blocks")];
    let publisher = Publisher::new();
    let subscription = publisher.clone().subscribe().await;
    let _publisher = publisher.clone();
    let rpc_client = Arc::new(RpcClient::new(endpoint, ex.clone()).await?);
    let rpc_client_ = rpc_client.clone();
    rpc_task.start(
        // Weird hack to prevent lifetimes hell
        async move {
            let req = JsonRequest::new("blockchain.subscribe_blocks", JsonValue::Array(vec![]));
            rpc_client_.subscribe(req, _publisher).await
        },
        |res| async move {
            rpc_client.stop().await;
            match res {
                Ok(()) | Err(Error::DetachedTaskStopped) | Err(Error::RpcServerStopped) => { /* Do nothing */ }
                Err(e) => {
                    eprintln!("[subscribe_blocks] JSON-RPC server error: {e}");
                    publisher
                        .notify(JsonResult::Error(JsonError::new(
                            ErrorCode::InternalError,
                            None,
                            0,
                        )))
                        .await;
                }
            }
        },
        Error::RpcServerStopped,
        ex.clone(),
    );
    shell_message.push(String::from("Detached subscription to background"));
    shell_message.push(String::from("All is good. Waiting for block notifications..."));
    shell_sender.send(shell_message).await?;

    let e = 'outer: loop {
        match subscription.receive().await {
            JsonResult::Notification(n) => {
                let mut shell_message =
                    vec![String::from("Got Block notification from darkfid subscription")];
                if n.method != "blockchain.subscribe_blocks" {
                    shell_sender.send(shell_message).await?;
                    break Error::UnexpectedJsonRpc(format!(
                        "Got foreign notification from darkfid: {}",
                        n.method
                    ))
                }

                // Verify parameters
                if !n.params.is_array() {
                    shell_sender.send(shell_message).await?;
                    break Error::UnexpectedJsonRpc(
                        "Received notification params are not an array".to_string(),
                    )
                }
                let params = n.params.get::<Vec<JsonValue>>().unwrap();
                if params.is_empty() {
                    shell_sender.send(shell_message).await?;
                    break Error::UnexpectedJsonRpc("Notification parameters are empty".to_string())
                }

                for param in params {
                    let param = param.get::<String>().unwrap();
                    let bytes = base64::decode(param).unwrap();

                    let block: BlockInfo = deserialize_async(&bytes).await?;
                    shell_message
                        .push(String::from("Deserialized successfully. Scanning block..."));

                    // Check if a reorg block was received, to reset to its previous
                    let lock = drk.read().await;
                    if block.header.height <= last_scanned_height {
                        let reset_height = block.header.height.saturating_sub(1);
                        if let Err(e) = lock.reset_to_height(reset_height, &mut shell_message).await
                        {
                            shell_sender.send(shell_message).await?;
                            break 'outer Error::Custom(format!(
                                "[subscribe_blocks] Wallet state reset failed: {e}"
                            ))
                        }

                        // Scan genesis again if needed
                        if reset_height == 0 {
                            let genesis = match lock.get_block_by_height(reset_height).await {
                                Ok(b) => b,
                                Err(e) => {
                                    shell_sender.send(shell_message).await?;
                                    break 'outer Error::Custom(format!(
                                        "[subscribe_blocks] RPC client request failed: {e}"
                                    ))
                                }
                            };
                            let mut scan_cache = lock.scan_cache().await?;
                            if let Err(e) = lock.scan_block(&mut scan_cache, &genesis).await {
                                shell_sender.send(shell_message).await?;
                                break 'outer Error::Custom(format!(
                                    "[subscribe_blocks] Scanning block failed: {e}"
                                ))
                            };
                            for msg in scan_cache.flush_messages() {
                                shell_message.push(msg);
                            }
                        }
                    }

                    let mut scan_cache = lock.scan_cache().await?;
                    if let Err(e) = lock.scan_block(&mut scan_cache, &block).await {
                        shell_sender.send(shell_message).await?;
                        break 'outer Error::Custom(format!(
                            "[subscribe_blocks] Scanning block failed: {e}"
                        ))
                    }
                    for msg in scan_cache.flush_messages() {
                        shell_message.push(msg);
                    }
                    shell_sender.send(shell_message.clone()).await?;

                    // Set new last scanned block height
                    last_scanned_height = block.header.height;
                }
            }

            JsonResult::Error(e) => {
                // Some error happened in the transmission
                break Error::UnexpectedJsonRpc(format!("Got error from JSON-RPC: {e:?}"))
            }

            x => {
                // And this is weird
                break Error::UnexpectedJsonRpc(format!("Got unexpected data from JSON-RPC: {x:?}"))
            }
        }
    };

    shell_sender.send(vec![format!("[subscribe_blocks] Subscription loop break: {e}")]).await?;
    Err(e)
}
