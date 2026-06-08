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
    collections::{BTreeMap, HashMap},
    sync::Arc,
    time::Instant,
};

use futures::AsyncWriteExt;
use smol::channel::Sender;
use smol::net::TcpStream;
use url::Url;

use dwow_core::{
    blockchain::HeaderHash,
    rpc::{
        client::RpcClient,
        jsonrpc::{ErrorCode, JsonError, JsonRequest, JsonResult},
        util::JsonValue,
    },
    system::{ExecutorPtr, Publisher, StoppableTaskPtr},
    tx::Transaction,
    util::encoding::base64,
    Error, Result,
};
use crate::contract_imports::promissory_note::TokenId;
use dwow_sdk::{
    bridgetree::Position,
    crypto::{
        keypair::Network,
        poseidon_hash,
        smt::{PoseidonFp, EMPTY_NODES_FP},
        ContractId, MerkleTree, PublicKey, SecretKey, DEPLOYOOOR_CONTRACT_ID,
        NATIVE_TOKEN_CONTRACT_ID,
    },
    deploy::{ContractMetadata, DeployParamsV1},
    pasta::group::ff::PrimeField,
    dark_tree::DarkLeaf,
    tx::{ContractCall, TransactionHash},
};
use dwow_promissory_note_contract::client::PromissoryNote;
use dwow_promissory_note_contract::model::{Coin, MintParamsV1, RedeemParamsV1, TransferParamsV1};
use dwow_native_token_contract::client::NativeNote;
use dwow_native_token_contract::model::{CoinAttributes, PoWRewardParamsV1};
use dwow_bearer_bond_contract::client::BearerBondNote;
use dwow_bearer_bond_contract::model::{
    IssueStakeParamsV1, PayInterestParamsV1, TransferStakeParamsV1,
};
use dwow_sdk::crypto::note::AeadEncryptedNote;
use dwow_serial::Decodable;
use dwow_serial::{deserialize_async, serialize_async};

use crate::{
    cache::{BlockScanner, CacheSmt, PnSmtStorage},
    cli_util::append_or_print,
    contract_imports::{BEARER_BOND_CONTRACT_ID, PROMISSORY_NOTE_CONTRACT_ID},
    error::{WalletDbError, WalletDbResult},
    promissory_note::SLED_MERKLE_TREES_PROMISSORY_NOTE,
    walletdb::{BondCoinRecord, CoinRecord, MerkleProof},
    Drk, DrkPtr,
};

// The wallet uses dwow_chain::Block directly — no adapter types.
// Blocks are fetched from dwowd via blockchain.get_block_linear (JSON).

/// Structure to hold a JSON-RPC client and its config,
/// so we can recreate it in case of an error.
pub struct DwowdRpcClient {
    endpoint: Url,
    ex: ExecutorPtr,
    client: Option<RpcClient>,
    /// Network indicator (used to detect darkwow-devnet mode)
    pub network: Network,
}

impl DwowdRpcClient {
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

    /// Check if this client is configured for darkwow-devnet mode
    pub fn is_darkwow_devnet(&self) -> bool {
        self.network == Network::Testnet
    }
}

/// Auxiliary structure holding various in memory caches to use during scan
pub struct ScanCache {
    /// The PromissoryNote Merkle tree containing coins
    pub promissory_note_tree: MerkleTree,
    /// The PromissoryNote Sparse Merkle tree containing coins nullifiers
    pub pn_smt: CacheSmt,
    /// All our known secrets to decrypt coin notes
    pub notes_secrets: Vec<SecretKey>,
    /// Our own coins nullifiers and their leaf positions
    pub owncoins_nullifiers: BTreeMap<[u8; 32], ([u8; 32], Position)>,
    /// Our own tokens to track freezes
    pub own_tokens: Vec<TokenId>,
    /// Our own deploy authorities
    pub own_deploy_auths: HashMap<[u8; 32], SecretKey>,
    /// Bearer Bond Merkle tree containing bond coins
    pub bearer_bond_tree: MerkleTree,
    /// Bearer Bond Sparse Merkle tree containing bond coin nullifiers
    pub bb_smt: CacheSmt,
    /// All our known secrets to decrypt bearer bond coin notes
    pub bb_notes_secrets: Vec<SecretKey>,
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

impl Drk {
    /// Auxiliary function to generate a new [`ScanCache`] for the
    /// wallet.
    pub async fn scan_cache(&self) -> Result<ScanCache> {
        let promissory_note_tree = self.get_promissory_note_tree().await?;

        // Create SMT storage and tree directly — no overlay
        let smt_store = PnSmtStorage::new(self.cache.pn_smt.clone());
        let pn_smt = CacheSmt::new(smt_store, PoseidonFp::new(), &EMPTY_NODES_FP);

        // Get our secrets
        let notes_secrets = self.get_promissory_note_secrets().await?;

        // Build nullifiers map from our coins
        let owncoins_nullifiers = BTreeMap::new();
        for coin in self.get_coins(false).await? {
            // TODO: Compute nullifier from coin attributes
            // For now, we can't compute nullifiers without the full note data
            let _ = coin;
        }

        // Bearer Bond: get Merkle tree and secrets
        let bearer_bond_tree = self.get_bearer_bond_tree().await?;
        let bb_notes_secrets = self.get_bearer_bond_secrets().await?;

        // Bearer Bond SMT for nullifiers
        let bb_smt_store = PnSmtStorage::new(self.cache.bb_smt.clone());
        let bb_smt = CacheSmt::new(bb_smt_store, PoseidonFp::new(), &EMPTY_NODES_FP);

        // TODO: Get mint authorities
        let own_tokens: Vec<TokenId> = vec![];

        // TODO: Get deploy auth keys
        let own_deploy_auths: HashMap<[u8; 32], SecretKey> = HashMap::new();

        Ok(ScanCache {
            promissory_note_tree,
            pn_smt,
            notes_secrets,
            owncoins_nullifiers,
            own_tokens,
            own_deploy_auths,
            bearer_bond_tree,
            bb_smt,
            bb_notes_secrets,
            messages_buffer: vec![],
        })
    }

    /// Scans the linear blockchain for wallet relevant transactions,
    /// starting from the last scanned block. If a reorg has happened,
    /// we revert to its previous height and then scan from there.
    pub async fn scan_blocks(
        &self,
        output: &mut Vec<String>,
        sender: Option<&Sender<Vec<String>>>,
        print: &bool,
    ) -> WalletDbResult<()> {
        // Grab last scanned block height (stored as u32 in wallet db, convert to u64)
        let (last_scanned_u32, _) = self.get_last_scanned_block()?;
        let mut height: u64 = if last_scanned_u32 == 0 {
            let mut buf = vec![];
            self.reset(&mut buf)?;
            append_or_print(output, sender, print, buf).await;
            1 // Start scanning from genesis block (height 1)
        } else {
            (last_scanned_u32 + 1) as u64
        };

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
            let (last_height_u32, last_hash) = match self.get_last_confirmed_block().await {
                Ok(last) => last,
                Err(e) => {
                    buf.push(format!("[scan_blocks] RPC client request failed: {e}"));
                    append_or_print(output, sender, print, buf).await;
                    return Err(WalletDbError::GenericError)
                }
            };
            let last_height = last_height_u32 as u64;
            buf.push(format!(
                "Last confirmed block reported by dwowd: {last_height} - {last_hash}"
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
                        buf.push(format!("[scan_blocks] RPC client request failed: {e}"));
                        append_or_print(output, sender, print, buf).await;
                        return Err(WalletDbError::GenericError)
                    }
                };
                buf.push(format!("Block {height} received! Scanning block..."));
                if let Err(e) = self.scan_block_linear(&mut scan_cache, &block).await {
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

    /// `scan_block_linear` processes a linear block directly from dwow_chain::Block.
    /// Handles contract calls AND coinbase transactions (mining rewards).
    async fn scan_block_linear(
        &self,
        scan_cache: &mut ScanCache,
        block: &dwow_chain::Block,
    ) -> Result<()> {
        use dwow_sdk::pasta::{pallas, group::ff::PrimeField};

        let height_u32 = block.header.height as u32;

        // Checkpoint the merkle trees
        scan_cache.promissory_note_tree.checkpoint(block.header.height as usize);
        scan_cache.bearer_bond_tree.checkpoint(block.header.height as usize);

        // Scan the block
        scan_cache.log(String::from("======================================="));
        scan_cache.log(format!("[linear] Block height: {}", block.header.height));
        scan_cache.log(String::from("======================================="));
        scan_cache.log(format!("[scan_block_linear] Iterating over {} transactions", block.transactions.len()));
        for tx in block.transactions.iter() {
            let mut wallet_tx = false;

            // Process contract calls (transfers, etc.)
            scan_cache.log(format!("[scan_block_linear] Processing transaction with {} calls", tx.contract_calls.len()));
            for (i, call) in tx.contract_calls.iter().enumerate() {
                // Convert linear [u8; 32] contract_id to ContractId for comparison
                let cid = ContractId::from(
                    pallas::Base::from_repr(call.contract_id).unwrap_or(pallas::Base::zero()),
                );

                // Check PromissoryNote contract
                if let Some(promissory_note_cid) = PROMISSORY_NOTE_CONTRACT_ID.get() {
                    if cid == *promissory_note_cid {
                        scan_cache.log(format!("[scan_block_linear] Found PromissoryNote contract in call {i}"));
                        if self
                            .apply_tx_promissory_note_data_linear(
                                scan_cache,
                                &call.data,
                                &height_u32,
                            )
                            .await?
                        {
                            wallet_tx = true;
                        }
                        continue
                    }
                }

                // Check Bearer Bond contract
                if let Some(bb_cid) = BEARER_BOND_CONTRACT_ID.get() {
                    if cid == *bb_cid {
                        scan_cache.log(format!("[scan_block_linear] Found BearerBond contract in call {i}"));
                        if self
                            .apply_tx_bearer_bond_data_linear(
                                scan_cache,
                                &call.data,
                                &height_u32,
                            )
                            .await?
                        {
                            wallet_tx = true;
                        }
                        continue
                    }
                }

                // Check DAO-Escrow by function code (0x00-0x08)
                let function_code = call.data.first().copied().unwrap_or(0xFF);
                if function_code <= 0x08 {
                    scan_cache.log(format!(
                        "[scan_block_linear] Found DAO-Escrow op code {:02x} in call {i}",
                        function_code
                    ));
                }

                if cid == *NATIVE_TOKEN_CONTRACT_ID {
                    scan_cache.log(format!("[scan_block_linear] Found Native Token contract in call {i}"));
                    if self
                        .apply_tx_native_token_data_linear(
                            scan_cache,
                            &call.data,
                            &height_u32,
                        )
                        .await?
                    {
                        wallet_tx = true;
                    }
                    continue
                }

                // Check Deployooor contract
                if cid == *DEPLOYOOOR_CONTRACT_ID {
                    let function_code = call.data.first().copied().unwrap_or(0xFF);
                    if function_code == 0x00 {
                        scan_cache.log(format!("[scan_block_linear] Found Deployooor::DeployV1 in call {i}"));
                        if let Ok(params) = DeployParamsV1::decode(&mut std::io::Cursor::new(&call.data[1..])) {
                            let contract_id = ContractId::derive_public(params.public_key);
                            let contract_id_str = bs58::encode(contract_id.to_bytes()).into_string();
                            let deployer_pubkey_str = bs58::encode(params.public_key.to_bytes()).into_string();

                            if let Some(metadata) = ContractMetadata::from_ix_bytes(&params.ix) {
                                let record = crate::walletdb::ContractMetadataRecord {
                                    contract_id: contract_id_str.clone(),
                                    name: metadata.name,
                                    symbol: metadata.symbol,
                                    category: format!("{:?}", metadata.category),
                                    description: metadata.description,
                                    public: metadata.public,
                                    deployer_pubkey: deployer_pubkey_str,
                                    deploy_height: height_u32,
                                    attestations_json: "[]".to_string(),
                                    lock_status: "unlocked".to_string(),
                                };
                                if self.wallet.insert_contract_metadata(&record).is_ok() {
                                    scan_cache.log(format!(
                                        "[scan_block_linear] Recorded contract metadata for {} at height {}",
                                        &contract_id_str[..8], height_u32
                                    ));
                                }
                            } else {
                                // Deployment without metadata — still record it
                                let record = crate::walletdb::ContractMetadataRecord {
                                    contract_id: contract_id_str.clone(),
                                    name: format!("Contract-{}", &contract_id_str[..8]),
                                    symbol: None,
                                    category: "Other".to_string(),
                                    description: None,
                                    public: false,
                                    deployer_pubkey: deployer_pubkey_str,
                                    deploy_height: height_u32,
                                    attestations_json: "[]".to_string(),
                                    lock_status: "unlocked".to_string(),
                                };
                                if self.wallet.insert_contract_metadata(&record).is_ok() {
                                    scan_cache.log(format!(
                                        "[scan_block_linear] Recorded anonymous contract {} at height {}",
                                        &contract_id_str[..8], height_u32
                                    ));
                                }
                            }
                            wallet_tx = true;
                        }
                    }
                    continue
                }

                // Log unknown contracts for debugging
                scan_cache.log(format!(
                    "[scan_block_linear] Unknown contract in call {i}, skipping.",
                ));
            }

            // Process coinbase transaction (mining reward with ZK privacy)
            if let Some(ref coinbase) = tx.coinbase {
                scan_cache.log(format!("[scan_block_linear] Found coinbase tx, attempting note decryption..."));
                // Deserialize the encrypted note and try to decrypt with wallet secrets
                if let Ok(aes_note) = AeadEncryptedNote::decode(
                    &mut std::io::Cursor::new(&coinbase.encrypted_note),
                ) {
                    for secret in &scan_cache.notes_secrets {
                        if let Ok(decrypted_note) = aes_note.decrypt::<NativeNote>(secret) {
                            let public_key = PublicKey::from_secret(*secret);
                            let coin_attrs = CoinAttributes {
                                version: 0,
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

                            let merkle_root = scan_cache.promissory_note_tree.root(0)
                                .map(|n| n.inner().to_repr())
                                .unwrap();
                            let merkle_proof = MerkleProof {
                                siblings: vec![],
                                root: bs58::encode(merkle_root).into_string(),
                            };

                            let token_id_str = bs58::encode(
                                decrypted_note.token_id.to_repr()
                            ).into_string();
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
                                created_at_height: height_u32,
                            };

                            if self.wallet.insert_coin(&coin_record, &merkle_proof).is_ok() {
                                scan_cache.log(format!(
                                    "[scan_block_linear] Inserted coinbase coin {} at height {}",
                                    &coin_id[..8], block.header.height
                                ));
                            }
                            wallet_tx = true;
                            break;
                        }
                    }
                }
            }

            // Record transaction history for wallet-relevant transactions
            if wallet_tx {
                let tx_hash = tx.hash();
                let tx_hash_str = tx_hash.to_hex().to_string();
                let tx_blob = serde_json::to_vec(tx).unwrap_or_default();
                if self.wallet.insert_transaction_history(
                    &tx_hash_str,
                    "confirmed",
                    Some(height_u32),
                    &tx_blob,
                ).is_ok() {
                    scan_cache.log(format!(
                        "[scan_block_linear] Recorded tx history {} at height {}",
                        &tx_hash_str[..8], height_u32
                    ));
                }
            }
        }

        // Insert the block record — direct sled write, no overlay
        let block_scanner = BlockScanner::new(&self.cache);
        block_scanner.insert_scanned_block(
            &height_u32,
            &HeaderHash(*block.header.previous.as_bytes()),
            &None,
        )?;

        // Update the merkle trees
        self.cache.insert_merkle_trees(&[
            (SLED_MERKLE_TREES_PROMISSORY_NOTE.as_bytes(), &scan_cache.promissory_note_tree),
        ])?;

        // Flush sled
        self.cache.db.flush()?;

        Ok(())
    }

    // Queries dwowd for last confirmed block height.
    async fn get_last_confirmed_block(&self) -> Result<(u32, String)> {
        let rep = self
            .dwowd_rpc_request("blockchain.get_height", &JsonValue::Array(vec![]))
            .await?;
        let height = *rep.get::<f64>().unwrap() as u32;
        // Fetch the block to compute a unique fingerprint for reorg detection
        let block = self.get_block_by_height_linear(height as u64).await?;
        let hash = hex::encode(block.header.merkle_root.as_bytes());

        Ok((height, hash))
    }

    // Queries dwowd for a linear blockchain block with given height.
    // Returns LinearBlockAdapter (wallet-compatible format for darkwow-devnet)
    async fn get_block_by_height_linear(&self, height: u64) -> Result<dwow_chain::Block> {
        let params = self
            .dwowd_rpc_request(
                "blockchain.get_block_linear",
                &JsonValue::Array(vec![JsonValue::Number(height as f64)]),
            )
            .await?;
        let json_str = params.get::<String>().unwrap();
        let block: dwow_chain::Block = serde_json::from_str(json_str)
            .map_err(|e| Error::Custom(format!("Failed to parse linear block: {}", e)))?;
        Ok(block)
    }

    /// Broadcast a given transaction to dwowd and forward onto the network.
    /// Returns the transaction ID upon success.
    pub async fn broadcast_tx(&self, tx: &Transaction, output: &mut Vec<String>) -> Result<String> {
        output.push(String::from("Broadcasting transaction..."));

        // Convert dwow_core::tx::Transaction to dwow_chain::Transaction
        let chain_contract_calls: Vec<dwow_chain::ContractCall> = tx.calls.iter().map(|leaf| {
            dwow_chain::ContractCall {
                contract_id: leaf.data.contract_id.to_bytes(),
                data: leaf.data.data.clone(),
            }
        }).collect();

        let chain_tx = dwow_chain::Transaction {
            version: 1,
            inputs: vec![],
            outputs: vec![],
            contract_calls: chain_contract_calls,
            lock_time: 0,
            coinbase: None,
        };

        let params =
            JsonValue::Array(vec![JsonValue::String(base64::encode(&serde_json::to_vec(&chain_tx).unwrap()))]);
        let rep = self.dwowd_rpc_request("tx.submit_linear", &params).await?;

        let txid = rep.get::<String>().unwrap().clone();

        // Store transactions history record
        if let Err(e) = self.put_tx_history_record(tx, "Broadcasted", None).await {
            return Err(Error::DatabaseError(format!(
                "[broadcast_tx] Inserting transaction history record failed: {e}"
            )))
        }

        // Record contract interactions for each contract call in the tx
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        for call in &tx.calls {
            let contract_id_str = bs58::encode(call.data.contract_id.to_bytes()).into_string();
            let function_code = call.data.data.first().copied().unwrap_or(0xFF);
            let function_name = format!("fc_{:02x}", function_code);
            if let Err(e) = self.wallet.insert_contract_interaction(
                &contract_id_str,
                &function_name,
                &txid,
                None,
                now,
            ) {
                // Non-fatal: log but don't fail the broadcast
                output.push(format!(
                    "[broadcast_tx] Failed to record contract interaction: {e}"
                ));
            }
        }

        Ok(txid)
    }

    /// Queries dwowd for a tx with given hash.
    pub async fn get_tx(&self, tx_hash: &TransactionHash) -> Result<Option<Transaction>> {
        let tx_hash_str = tx_hash.to_string();
        match self
            .dwowd_rpc_request(
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
            .dwowd_rpc_request(
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
        let rep = self.dwowd_rpc_request("blockchain.lookup_zkas", &params).await?;
        let params = rep.get::<Vec<JsonValue>>().unwrap();

        let mut ret = Vec::with_capacity(params.len());
        for param in params {
            let zkas_ns = param[0].get::<String>().unwrap().clone();
            let zkas_bincode_bytes = base64::decode(param[1].get::<String>().unwrap()).unwrap();
            ret.push((zkas_ns, zkas_bincode_bytes));
        }

        Ok(ret)
    }

    /// Queries dwowd for given transaction's required fee.
    pub async fn get_tx_fee(&self, tx: &Transaction, include_fee: bool) -> Result<u64> {
        let params = JsonValue::Array(vec![
            JsonValue::String(base64::encode(&serialize_async(tx).await)),
            JsonValue::Boolean(include_fee),
        ]);
        let rep = self.dwowd_rpc_request("tx.calculate_fee", &params).await?;

        let fee = *rep.get::<f64>().unwrap() as u64;

        Ok(fee)
    }

    /// Queries dwowd for current best fork next height.
    pub async fn get_next_block_height(&self) -> Result<u32> {
        let rep = self
            .dwowd_rpc_request(
                "blockchain.last_confirmed_block",
                &JsonValue::Array(vec![]),
            )
            .await?;
        let params = rep.get::<Vec<JsonValue>>().unwrap();
        let height = *params[0].get::<f64>().unwrap() as u32;

        Ok(height + 1)
    }

    /// Queries dwowd for currently configured block target time.
    pub async fn get_block_target(&self) -> Result<u32> {
        let rep = self
            .dwowd_rpc_request("blockchain.get_target", &JsonValue::Array(vec![]))
            .await?;

        // dwowd returns {"target": N}, wallet expects bare f64
        let target = *rep.get::<f64>().unwrap() as u32;

        Ok(target)
    }

    /// Auxiliary function to ping configured dwowd daemon for liveness.
    pub async fn ping(&self, output: &mut Vec<String>) -> Result<()> {
        output.push(String::from("Executing ping request to dwowd..."));
        let latency = Instant::now();
        let rep = self.dwowd_rpc_request("ping", &JsonValue::Array(vec![])).await?;
        let latency = latency.elapsed();
        output.push(format!("Got reply: {rep:?}"));
        output.push(format!("Latency: {latency:?}"));
        Ok(())
    }

    /// Auxiliary function to execute a request towards the configured dwowd daemon JSON-RPC endpoint.
    pub async fn dwowd_rpc_request(
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
    /// Connects to dwowd's stratum server and mines blocks using RandomX.
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
                    match dwow_core::system::io_timeout(
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
    /// 1. Parses PromissoryNote contract calls (TransferV1)
    /// 2. For each output, tries to decrypt the note using our secrets
    /// 3. If we own the coin, stores it in the wallet with Merkle proof
    ///
    /// Note: MintV1 scanning requires additional work as the note encryption
    /// is handled at the application layer, not in the contract params.
    pub async fn apply_tx_promissory_note_data(
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
                    let mut decrypted_note_opt: Option<PromissoryNote> = None;
                    let mut found_secret: Option<SecretKey> = None;

                    // Try to decrypt with each of our secrets
                    for secret in &scan_cache.notes_secrets {
                        if let Ok(decrypted_note) = note.decrypt::<PromissoryNote>(secret) {
                            decrypted_note_opt = Some(decrypted_note);
                            found_secret = Some(*secret);
                            break
                        }
                    }

                    if let (Some(decrypted_note), Some(secret)) = (decrypted_note_opt, found_secret) {
                        found_coin = true;
                        // Calculate coin ID from the note
                        
                        // In Promissory Note, public_key = poseidon_hash(secret) as a field element
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

                        // Get the Merkle proof from the promissory_note_tree
                        let merkle_proof = {
                            let siblings: Vec<String> = vec![];
                            let root = scan_cache.promissory_note_tree.root(0).map(|n| n.inner()).unwrap();
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
                                "[apply_tx_promissory_note_data] Inserted coin {} at height {}",
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
            // RedeemV1 (0x01) - Redeem coin, create zero-value receipt
            0x01 => {
                let mut cursor = std::io::Cursor::new(&data[1..]);
                match RedeemParamsV1::decode(&mut cursor) {
                    Ok(params) => {
                        let note = &params.output.note;
                        let mut found_coin = false;
                        let mut decrypted_note_opt: Option<PromissoryNote> = None;
                        let mut found_secret: Option<SecretKey> = None;

                        // Try to decrypt the receipt note
                        for secret in &scan_cache.notes_secrets {
                            if let Ok(decrypted_note) = note.decrypt::<PromissoryNote>(secret) {
                                decrypted_note_opt = Some(decrypted_note);
                                found_secret = Some(*secret);
                                break
                            }
                        }

                        if let (Some(decrypted_note), Some(secret)) = (decrypted_note_opt, found_secret) {
                            found_coin = true;
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

                            let merkle_proof = {
                                let siblings: Vec<String> = vec![];
                                let root = scan_cache.promissory_note_tree.root(0).map(|n| n.inner()).unwrap();
                                let root_bytes = root.to_repr();
                                MerkleProof {
                                    siblings,
                                    root: bs58::encode(root_bytes).into_string(),
                                }
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
                                    "[apply_tx_promissory_note_data] Inserted RedeemV1 receipt coin {} at height {}",
                                    &coin_id[..8],
                                    height
                                ));
                                is_wallet_tx = true;
                            }
                        }
                    }
                    Err(e) => {
                        scan_cache.log(format!(
                            "[apply_tx_promissory_note_data] Failed to decode RedeemV1 params: {:?}", e
                        ));
                    }
                }
            }
            // MintV1 (0x02) - Mint tokens
            // The note encryption is not in the params, so we skip for now
            0x02 => {
                scan_cache.log(String::from("[apply_tx_promissory_note_data] MintV1 detected - note decryption not in params, skipping"));
            }
            _ => {
                // Other function codes (TokenMintV1, BurnV1)
                scan_cache.log(format!(
                    "[apply_tx_promissory_note_data] Skipping PromissoryNote function code: {:02x}",
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
                        use dwow_sdk::crypto::PublicKey;
                        use dwow_native_token_contract::model::CoinAttributes;
                        let public_key = PublicKey::from_secret(*secret);
                        let coin_attrs = CoinAttributes {
                            version: 0,
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

                        // Get merkle proof from promissory_note_tree (same merkle tree for native token coins)
                        let merkle_root = scan_cache.promissory_note_tree.root(0).map(|n| n.inner().to_repr()).unwrap();
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
    /// For darkwow-devnet, mining rewards are directly sent to the wallet's public key
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
                        use dwow_sdk::crypto::PublicKey;
                        use dwow_native_token_contract::model::CoinAttributes;
                        let public_key = PublicKey::from_secret(*secret);
                        let coin_attrs = CoinAttributes {
                            version: 0,
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

                        // Get merkle proof from promissory_note_tree
                        let merkle_root = scan_cache.promissory_note_tree.root(0).map(|n| n.inner().to_repr()).unwrap();
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

    /// Apply Bearer Bond transaction data from linear blockchain
    ///
    /// Handles IssueStakeV1 (0x00), TransferStakeV1 (0x01), and PayInterestV1 (0x08)
    /// with AEAD note decryption for BlindOutput_V1 outputs.
    async fn apply_tx_bearer_bond_data_linear(
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
            // IssueStakeV1 (0x00) — issuer creates staking pool, mints stake coin
            0x00 => {
                let mut cursor = std::io::Cursor::new(&data[1..]);
                let params = IssueStakeParamsV1::decode(&mut cursor)
                    .map_err(|e| Error::Custom(format!("Failed to decode IssueStakeV1 params: {:?}", e)))?;

                let mut found_our_coin = false;
                let mut log_messages = vec![];

                let merkle_root = scan_cache.bearer_bond_tree.root(0)
                    .map(|n| n.inner().to_repr())
                    .unwrap();
                let merkle_proof = MerkleProof {
                    siblings: vec![],
                    root: bs58::encode(merkle_root).into_string(),
                };

                // The IssueStakeV1 params contain a single output coin (BlindOutput_V1).
                // We don't have a direct note to decrypt here — the note is embedded
                // in the BlindOutput_V1 proof. For now, we log and move on.
                // The actual coin discovery happens at the transfer level.
                let _ = params;
                let _ = merkle_proof;

                for msg in log_messages {
                    scan_cache.log(msg);
                }
                Ok(found_our_coin)
            }
            // TransferStakeV1 (0x01) — transfer stake position
            0x01 => {
                let mut cursor = std::io::Cursor::new(&data[1..]);
                let params = TransferStakeParamsV1::decode(&mut cursor)
                    .map_err(|e| Error::Custom(format!("Failed to decode TransferStakeV1 params: {:?}", e)))?;

                let mut found_our_coin = false;
                let mut log_messages = vec![];

                let merkle_root = scan_cache.bearer_bond_tree.root(0)
                    .map(|n| n.inner().to_repr())
                    .unwrap();
                let merkle_proof = MerkleProof {
                    siblings: vec![],
                    root: bs58::encode(merkle_root).into_string(),
                };

                // Try to decrypt each output note
                for output in params.outputs.iter() {
                    // BlindOutput_V1 outputs carry AeadEncryptedNote bearer bond data
                    // For now we track the outputs — full note decryption is Phase 3b
                    let _ = output;
                    let _ = merkle_proof;
                    let _ = &scan_cache.bb_notes_secrets;
                }

                for msg in log_messages {
                    scan_cache.log(msg);
                }
                Ok(found_our_coin)
            }
            // PayInterestV1 (0x08) — issuer pays a pending interest claim
            0x08 => {
                let mut cursor = std::io::Cursor::new(&data[1..]);
                let params = PayInterestParamsV1::decode(&mut cursor)
                    .map_err(|e| Error::Custom(format!("Failed to decode PayInterestV1 params: {:?}", e)))?;

                let mut found_our_coin = false;
                let mut log_messages = vec![];

                let merkle_root = scan_cache.bearer_bond_tree.root(0)
                    .map(|n| n.inner().to_repr())
                    .unwrap();
                let merkle_proof = MerkleProof {
                    siblings: vec![],
                    root: bs58::encode(merkle_root).into_string(),
                };

                // The interest_coin is a BlindOutput_V1 addressed to the holder's
                // payment_key from the claim. Full decryption is Phase 3b.
                let _ = params;
                let _ = merkle_proof;

                for msg in log_messages {
                    scan_cache.log(msg);
                }
                Ok(found_our_coin)
            }
            _ => {
                scan_cache.log(format!(
                    "[apply_tx_bearer_bond_data_linear] Skipping BearerBond function code: {:02x}",
                    function_code
                ));
                Ok(false)
            }
        }
    }

    /// Apply PromissoryNote transaction data from linear blockchain
    ///
    /// Handles TransferV1 (0x04) for token transfers with note decryption
    async fn apply_tx_promissory_note_data_linear(
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
                let merkle_root = scan_cache.promissory_note_tree.root(0)
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
                        if let Ok(decrypted_note) = output.note.decrypt::<PromissoryNote>(secret) {
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
                                    "[apply_tx_promissory_note_data_linear] Inserted PromissoryNote coin {} at height {}",
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
            // RedeemV1 (0x01) - Redeem coin, create zero-value receipt
            0x01 => {
                let mut cursor = std::io::Cursor::new(&data[1..]);
                match RedeemParamsV1::decode(&mut cursor) {
                    Ok(params) => {
                        let mut found_our_coin = false;
                        let mut log_messages = vec![];

                        let merkle_root = scan_cache.promissory_note_tree.root(0)
                            .map(|n| n.inner().to_repr())
                            .unwrap();
                        let merkle_proof = MerkleProof {
                            siblings: vec![],
                            root: bs58::encode(merkle_root).into_string(),
                        };

                        // Try to decrypt the receipt note
                        for secret in &scan_cache.notes_secrets {
                            if let Ok(decrypted_note) = params.output.note.decrypt::<PromissoryNote>(secret) {
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
                                        "[apply_tx_promissory_note_data_linear] Inserted RedeemV1 receipt coin {} at height {}",
                                        &coin_id[..8],
                                        height
                                    ));
                                    found_our_coin = true;
                                }
                            }
                        }

                        for msg in log_messages {
                            scan_cache.log(msg);
                        }
                        Ok(found_our_coin)
                    }
                    Err(e) => {
                        scan_cache.log(format!(
                            "[apply_tx_promissory_note_data_linear] Failed to decode RedeemV1 params: {:?}", e
                        ));
                        Ok(false)
                    }
                }
            }
            // MintV1 (0x02) - mint tokens of existing type
            0x02 => {
                let mut cursor = std::io::Cursor::new(&data[1..]);
                match MintParamsV1::decode(&mut cursor) {
                    Ok(params) => {
                        // Check if we own the mint authority used
                        for secret in &scan_cache.notes_secrets {
                            if poseidon_hash([secret.inner()]) == params.mint_public {
                                let coin_id = bs58::encode(params.coin.to_bytes()).into_string();
                                let token_id_str = bs58::encode(params.token_id.to_repr()).into_string();
                                scan_cache.log(format!(
                                    "[apply_tx_promissory_note_data_linear] Found our MintV1: coin={}, token={}",
                                    &coin_id[..8], &token_id_str[..8]
                                ));
                                // TODO: insert coin into wallet. We know token_id
                                // and mint_public is ours, but value and coin_blind
                                // are hidden behind the coin commitment. The minted
                                // coin is tracked during tx building; scanning
                                // cross-checks are deferred.
                                return Ok(true);
                            }
                        }
                        scan_cache.log(format!(
                            "[apply_tx_promissory_note_data_linear] MintV1 for token {} (not ours)",
                            bs58::encode(params.token_id.to_repr()).into_string()
                        ));
                    }
                    Err(e) => {
                        scan_cache.log(format!(
                            "[apply_tx_promissory_note_data_linear] Failed to decode MintV1 params: {:?}", e
                        ));
                    }
                }
                Ok(false)
            }
            _ => {
                scan_cache.log(format!(
                    "[apply_tx_promissory_note_data_linear] Skipping PromissoryNote function code: {:02x}",
                    function_code
                ));
                Ok(false)
            }
        }
    }
}

/// Subscribes to dwowd's JSON-RPC notification endpoint that serves
/// new confirmed blocks. Upon receiving them, all the transactions are
/// scanned and we check if any of them call the money contract, and if
/// the payments are intended for us. If so, we decrypt them and append
/// the metadata to our wallet. If a reorg block is received, we revert
/// to its previous height and then scan it. We assume that the blocks
/// up to that point are unchanged, since dwowd will just broadcast
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
                    vec![String::from("Got Block notification from dwowd subscription")];
                if n.method != "blockchain.subscribe_blocks" {
                    shell_sender.send(shell_message).await?;
                    break Error::UnexpectedJsonRpc(format!(
                        "Got foreign notification from dwowd: {}",
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

                    // Linear blocks are sent as JSON strings
                    let block: dwow_chain::Block = serde_json::from_str(&param)
                        .map_err(|e| Error::Custom(format!(
                            "[subscribe_blocks] Failed to parse linear block: {e}"
                        )))?;
                    shell_message
                        .push(String::from("Deserialized successfully. Scanning block..."));

                    // Check if a reorg block was received, to reset to its previous
                    let lock = drk.read().await;
                    if (block.header.height as u32) <= last_scanned_height {
                        let reset_height = (block.header.height as u32).saturating_sub(1);
                        if let Err(e) = lock.reset_to_height(reset_height, &mut shell_message).await
                        {
                            shell_sender.send(shell_message).await?;
                            break 'outer Error::Custom(format!(
                                "[subscribe_blocks] Wallet state reset failed: {e}"
                            ))
                        }

                        // Scan genesis again if needed
                        if reset_height == 0 {
                            let genesis = match lock.get_block_by_height_linear(0).await {
                                Ok(b) => b,
                                Err(e) => {
                                    shell_sender.send(shell_message).await?;
                                    break 'outer Error::Custom(format!(
                                        "[subscribe_blocks] RPC client request failed: {e}"
                                    ))
                                }
                            };
                            let mut scan_cache = lock.scan_cache().await?;
                            if let Err(e) = lock.scan_block_linear(&mut scan_cache, &genesis).await {
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
                    if let Err(e) = lock.scan_block_linear(&mut scan_cache, &block).await {
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
                    last_scanned_height = block.header.height as u32;
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
