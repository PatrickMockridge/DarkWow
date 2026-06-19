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

use std::{collections::HashMap, fs::create_dir_all, sync::Arc};

use bs58;
use hex;
use rand::rngs::OsRng;

use smol::lock::RwLock;
use tracing::info;

use dwow_core::{
    tx::{ContractCallLeaf, Transaction},
    util::path::expand_path,
    zk::{proof::ProvingKey, vm::ZkCircuit, vm_heap::empty_witnesses, Proof},
    zkas::ZkBinary,
    Error, Result,
};
use dwow_serial::AsyncEncodable;
use dwow_sdk::{
    crypto::{
        keypair::{Address, Keypair, Network, PublicKey, SecretKey},
        pasta_prelude::PrimeField,
        poseidon_hash, BaseBlind, ContractId, FuncId, MerkleNode, MerkleTree,
    },
    pasta::pallas,
    tx::ContractCall,
};
use dwow_promissory_note_contract::client::PromissoryNote;
use crate::contract_imports::{promissory_note::TokenId, PROMISSORY_NOTE_CONTRACT_ID, NATIVE_TOKEN_CONTRACT_ID};
use crate::walletdb::CapRecord;
use dwow_sdk::crypto::util::FieldElemAsStr;

/// CLI argument parsing — visible, testable
pub mod args;

/// Configuration loading — TOML + CLI merge, sync
pub mod config;

/// Subcommand dispatch — classify + route to wallet methods
pub mod dispatch;

/// Contract manifest resolver — on-chain ABI queries
pub mod manifest_resolver;

/// WASM manifest verification — mechanical, zero-trust
pub mod manifest_verify;

/// Error codes
pub mod error;
use error::{WalletDbError, WalletDbResult};

/// Common shared functions
pub mod common;

/// Local block scanning — coin discovery, AEAD decryption
pub mod scan;

/// Payment methods
pub mod transfer;

/// Swap methods
pub mod swap;

/// Token methods
pub mod token;

/// CLI utility functions
pub mod cli_util;

/// Drk interactive shell (TEMPORARILY DISABLED - DAO removal in progress)
// pub mod interactive;

/// Wallet functionality related to Deployooor
pub mod deploy;

// dao_escrow and drain_protection removed — wallet uses generic AEAD scan + manifest.
// All contracts (except Native Token for fees + Deployooor for deployment) are
// discovered via the generic capability path. No per-contract files.

/// Fee builder helper for contract transactions
pub mod fee_builder;

/// Wallet functionality related to transactions history
pub mod txs_history;

/// Wallet functionality related to scanned blocks
pub mod scanned_blocks;

/// Contract import graph - maps stale imports to actual crates
pub mod contract_imports;

/// Generic contract registry for dependency resolution and transaction building
pub mod contract_registry;

/// Contract metadata registry for universal contract interaction
pub mod contract_metadata;

/// P2P chain sync task — GetTip/GetBlocks, block sync, local scan
pub mod sync_task;

/// Wallet database operations handler
pub mod walletdb;
use walletdb::{WalletDb, WalletPtr};

/// Blockchain cache database operations handler
pub mod cache;
use cache::Cache;

/// Atomic pointer to a `Dww` structure.
pub type DwwPtr = Arc<RwLock<Dww>>;

/// Wallet struct — full node architecture.
///
/// Syncs the chain via P2P (connects to seeds, discovers peers via hostlist,
/// requests blocks). Scans its own synced chain locally. Does not use RPC
/// for chain sync. RPC is transitional for broadcast_tx() only.
pub struct Dww {
    /// Blockchain network (Testnet / Mainnet)
    pub network: Network,
    /// Chain block store — wallet's own synced blocks
    pub chain: dwow_chain::LinearStore,
    /// Blockchain cache database operations handler (Sled — SMT indices, scan progress)
    pub cache: Cache,
    /// Wallet database operations handler (SQLite — keys, coins, contracts)
    pub wallet: WalletPtr,
    /// P2P network instance (None until init_p2p is called)
    pub p2p: Option<dwow_core::net::P2pPtr>,
    /// P2P network settings from config [net] section
    pub p2p_settings: Option<dwow_core::net::Settings>,
    /// Async executor for P2P runtime
    pub executor: Option<dwow_core::system::ExecutorPtr>,
    /// Highest peer chain tip seen by sync task. Updated on each Tip response.
    pub highest_peer_tip: Arc<crate::sync_task::HighestPeerTip>,
}

impl Dww {
    pub fn new(
        network: Network,
        database: String,
        cache_path: String,
        wallet_path: String,
        wallet_pass: String,
        p2p_settings: Option<dwow_core::net::Settings>,
    ) -> Result<Self> {
        // Open chain block store (same sled DB that dwowd writes to)
        let chain_db_path = expand_path(&database)?;
        let chain_db = sled::open(&chain_db_path)?;
        let chain = dwow_chain::LinearStore::new(Arc::new(chain_db))
            .map_err(|e| Error::Custom(format!("Failed to open chain store: {}", e)))?;

        // Initialize blockchain cache database
        let db_path = expand_path(&cache_path)?;
        let sled_db = sled::open(&db_path)?;
        let Ok(cache) = Cache::new(&sled_db) else {
            return Err(Error::DatabaseError(format!("{}", WalletDbError::InitializationFailed)));
        };

        // Initialize wallet
        let wallet_path = expand_path(&wallet_path)?;
        if !wallet_path.exists() {
            if let Some(parent) = wallet_path.parent() {
                create_dir_all(parent)?;
            }
        }
        let Ok(wallet) = WalletDb::new(Some(wallet_path), Some(&wallet_pass)) else {
            return Err(Error::DatabaseError(format!("{}", WalletDbError::InitializationFailed)));
        };

        // Auto-load persisted contract registry into OnceLock values
        if let Ok(registry) = wallet.get_contract_registry() {
            for (name, cid_str) in registry {
                let cid_bytes: [u8; 32] = match bs58::decode(&cid_str).into_vec() {
                    Ok(v) => match v.try_into() {
                        Ok(b) => b,
                        Err(_) => continue,
                    },
                    Err(_) => continue,
                };
                let cid = match ContractId::from_bytes(cid_bytes) {
                    Ok(c) => c,
                    Err(_) => continue,
                };
                let _ = crate::contract_imports::register_contract_id(&name, cid);
            }
        }

        Ok(Self { network, chain, cache, wallet, p2p: None, p2p_settings, executor: None, highest_peer_tip: Arc::new(crate::sync_task::HighestPeerTip::new()) })
    }

    /// Get the current chain tip height from the local block store.
    pub fn chain_height(&self) -> Result<u64> {
        self.chain.get_height()
            .map_err(|e| Error::Custom(format!("chain height: {}", e)))
    }

    /// Get a block by height from the local block store.
    pub fn chain_block(&self, height: u64) -> Result<dwow_chain::Block> {
        self.chain.get_block(height)
            .map_err(|e| Error::Custom(format!("chain block {}: {}", height, e)))
    }

    /// Initialize P2P networking. Connects to seeds, discovers peers via
    /// hostlist. Must be called before scan/broadcast.
    /// Idempotent — returns immediately if P2P is already initialized.
    pub async fn init_p2p(&mut self, executor: &dwow_core::system::ExecutorPtr) -> Result<()> {
        if self.p2p.is_some() {
            return Ok(());
        }
        let settings = self.p2p_settings.clone()
            .ok_or_else(|| Error::Custom("P2P not configured — add [net] section to wallet config".into()))?;
        let p2p = dwow_core::net::P2p::new(settings, executor.clone()).await?;
        p2p.clone().start().await?;
        p2p.clone().seed().await;
        info!(target: "drk::wallet", "P2P initialized — connected to seeds, discovering peers");

        self.p2p = Some(p2p);
        self.executor = Some(executor.clone());
        Ok(())
    }

    /// Returns true if the wallet has synced to the latest known peer tip.
    /// Compares local chain height against highest peer tip seen by sync task.
    /// If P2P is not configured, falls back to chain.height > 0.
    pub fn is_synced(&self) -> bool {
        let local = match self.chain.get_height() {
            Ok(h) => h,
            Err(_) => return false,
        };
        if local == 0 {
            return false;
        }
        if self.p2p.is_some() {
            // HAZOP #5: Must have at least one peer to be considered synced.
            // Falling through to 'local > 0' with zero peers is misleading.
            let peer_count = self.p2p.as_ref()
                .map(|p| p.hosts().peers().len())
                .unwrap_or(0);
            if peer_count == 0 {
                return false;
            }
            let peer_tip = self.highest_peer_tip.get();
            if peer_tip > 0 {
                return local >= peer_tip;
            }
        }
        // No peer tip yet — consider synced if we have any blocks
        local > 0
    }

    /// Insert a block synced from a P2P peer into the wallet's chain store.
    pub fn insert_synced_block(&self, block: &dwow_chain::Block) -> Result<()> {
        let height = block.header.height;
        self.chain.insert_block(height, block)
            .map_err(|e| Error::Custom(format!("insert block {}: {}", height, e)))
    }

    pub fn into_ptr(self) -> DwwPtr {
        Arc::new(RwLock::new(self))
    }

    /// Broadcast a transaction via P2P gossip.
    /// Serializes the tx and sends it to all connected peers.
    /// Returns the txid on success.
    pub async fn broadcast_tx(&self, tx: &dwow_core::tx::Transaction, output: &mut Vec<String>) -> Result<String> {
        let p2p = self.p2p.as_ref()
            .ok_or_else(|| Error::Custom("P2P not initialized — run 'sync init' first".into()))?;

        // Broadcast the raw Transaction via P2P gossip.
        // The Transaction type's P2P Message name is "tx" which matches
        // the ProtocolTxHandler on receiving nodes.
        p2p.broadcast(tx).await;

        let txid = tx.hash().to_string();
        output.push(format!("Transaction broadcast: {}", txid));

        // Store in history
        if let Err(e) = self.put_tx_history_record(tx, "Broadcasted", None) {
            output.push(format!("Warning: failed to record tx history: {e}"));
        }

        Ok(txid)
    }

    /// Initialize wallet with tables for `Dww`.
    pub fn initialize_wallet(&self) -> WalletDbResult<()> {
        // Initialize wallet schema
        self.wallet.exec_batch_sql(include_str!("../wallet.sql"))?;

        // Register default DRKW native token alias so `transfer 1.0 DRKW <addr>` works
        // on a fresh wallet without requiring a prior scan.
        let drkw_token = walletdb::TokenInfo {
            token_id: "11111111111111111111111111111111".to_string(), // bs58 of 32 zero bytes
            name: Some("DRKW".to_string()),
            symbol: Some("DRKW".to_string()),
            decimals: 8,
            mint_authority: None,
            token_blind: "11111111111111111111111111111111".to_string(),
            is_frozen: false,
            freeze_height: None,
            created_at_height: 0,
        };
        self.wallet.insert_token(&drkw_token)?;

        Ok(())
    }

    /// Auxiliary function to completely reset wallet state.
    pub fn reset(&self, output: &mut Vec<String>) -> WalletDbResult<()> {
        output.push(String::from("Resetting full wallet state"));
        self.reset_scanned_blocks(output)?;
        self.reset_deploy_authorities(output)?;
        self.reset_deploy_history(output)?;
        self.reset_tx_history(output)?;
        output.push(String::from("Successfully reset full wallet state"));
        Ok(())
    }

    // =============================================================================================
    // STUB METHODS - These are stubs that need proper implementation
    // =============================================================================================

    /// Get the Money contract Merkle tree from cache
    pub fn get_cap_tree(&self) -> Result<MerkleTree> {
        match self.cache.get_merkle_tree(b"promissory_note_merkle_trees") {
            Some(tree) => Ok(tree),
            None => {
                // Create an empty Merkle tree for darkwow-devnet (no previous state)
                let tree = MerkleTree::new(1);
                Ok(tree)
            }
        }
    }

    /// Get promissory note secrets from wallet
    pub fn get_secrets(&self) -> Result<Vec<SecretKey>> {
        let secret_strings = self.wallet.get_secrets().map_err(|e| Error::Custom(format!("{:?}", e)))?;
        let mut secrets = vec![];
        for s in secret_strings {
            let bytes = bs58::decode(&s).into_vec().map_err(|e| Error::Custom(e.to_string()))?;
            let key_array: [u8; 32] = bytes.try_into().map_err(|_| Error::Custom("Invalid secret key length".to_string()))?;
            let secret = SecretKey::from_bytes(key_array)
                .map_err(|_| Error::Custom("Failed to parse secret key".to_string()))?;
            secrets.push(secret);
        }
        Ok(secrets)
    }

    /// Get the Bearer Bond contract Merkle tree from cache

    /// Get held capabilities from wallet
    pub fn get_held_capabilities(&self, revoked: Option<bool>) -> Result<Vec<PromissoryNote>> {
        let coin_records = self.wallet.get_held_capabilities(revoked).map_err(|e| Error::Custom(format!("{:?}", e)))?;
        cap_records_to_pn_notes(&coin_records)
    }

    /// Get coins for a specific token
    pub fn get_capabilities_for_token(&self, token_id: &TokenId) -> Result<Vec<PromissoryNote>> {
        let token_id_str = token_id.to_string();
        let coin_records = self.wallet.get_capabilities_for_token(&token_id_str, Some(false)).map_err(|e| Error::Custom(format!("{:?}", e)))?;
        cap_records_to_pn_notes(&coin_records)
    }

    /// Get token by token ID or alias.
    ///
    /// The identifier can be:
    /// - A bs58-encoded token ID (pallas::Base)
    /// - A token name/alias stored in the wallet
    pub fn get_token(&self, identifier: String) -> Result<TokenId> {
        // First, try to parse identifier as a direct bs58-encoded token ID
        if let Ok(token_bytes) = bs58::decode(&identifier).into_vec() {
            if token_bytes.len() == 32 {
                let token_array: [u8; 32] = match token_bytes.clone().try_into() {
                    Ok(a) => a,
                    Err(_) => return Err(Error::Custom("Invalid token_id bytes".to_string())),
                };
                let token_id_opt = pallas::Base::from_repr(token_array);
                if bool::from(token_id_opt.is_some()) {
                    return Ok(token_id_opt.unwrap());
                }
            }
        }

        // Try to look up in database by token_id or name
        match self.wallet.get_token(&identifier) {
            Ok(Some(token_info)) => {
                // Parse the token_id from bs58
                let token_bytes = bs58::decode(&token_info.token_id)
                    .into_vec()
                    .map_err(|e| Error::Custom(format!("Invalid token_id in database: {}", e)))?;
                if token_bytes.len() == 32 {
                    let token_array: [u8; 32] = match token_bytes.try_into() {
                        Ok(a) => a,
                        Err(_) => return Err(Error::Custom("Invalid token_id length".to_string())),
                    };
                    let token_id_opt = pallas::Base::from_repr(token_array);
                    if bool::from(token_id_opt.is_some()) {
                        return Ok(token_id_opt.unwrap());
                    }
                    return Err(Error::Custom("Invalid token_id in database".to_string()));
                }
                Err(Error::Custom(format!("Invalid token_id length in database")))
            }
            Ok(None) => Err(Error::Custom(format!("Token not found: {}", identifier))),
            Err(e) => Err(Error::Custom(format!("Database error: {:?}", e))),
        }
    }

    /// Get aliases mapped by token
    pub fn get_aliases_mapped_by_token(&self) -> Result<HashMap<String, String>> {
        let aliases = self.wallet.get_aliases()
            .map_err(|e| Error::Custom(format!("Database error: {:?}", e)))?;

        let mut map = HashMap::new();
        for alias in aliases {
            map.insert(alias.token_id, alias.alias);
        }
        Ok(map)
    }

    /// Get default address
    pub fn default_address(&self) -> Result<Address> {
        let addresses = self.wallet.get_addresses()
            .map_err(|e| Error::Custom(format!("Database error: {:?}", e)))?;

        match addresses.first() {
            Some(addr) => {
                let secret_bytes: [u8; 32] = bs58::decode(&addr.secret)
                    .into_vec()
                    .expect("Invalid secret encoding")
                    .as_slice().try_into().unwrap();
                let secret = SecretKey::from_bytes(secret_bytes).unwrap();
                let public = PublicKey::from_secret(secret);
                let std_addr = dwow_sdk::crypto::keypair::StandardAddress::from_public(self.network, public);
                Ok(std_addr.into())
            }
            None => Err(Error::Custom("No addresses in wallet".to_string())),
        }
    }

    /// Get all addresses
    pub fn addresses(&self) -> Result<Vec<(u64, PublicKey, SecretKey, u64)>> {
        let addrs = self.wallet.get_addresses()
            .map_err(|e| Error::Custom(format!("Database error: {:?}", e)))?;

        let mut result: Vec<(u64, PublicKey, SecretKey, u64)> = vec![];
        for a in addrs {
            let secret_bytes: [u8; 32] = bs58::decode(&a.secret)
                .into_vec()
                .expect("Invalid secret encoding")
                .as_slice().try_into().unwrap();
            let secret = SecretKey::from_bytes(secret_bytes).unwrap();
            let public = PublicKey::from_secret(secret);
            result.push((a.id as u64, public, secret, a.created_at_height as u64));
        }

        Ok(result)
    }

    /// Get default secret key for the wallet
    pub fn default_secret(&self) -> Result<SecretKey> {
        let addresses = self.wallet.get_addresses()
            .map_err(|e| Error::Custom(format!("Database error: {:?}", e)))?;

        match addresses.first() {
            Some(addr) => {
                let secret_bytes: [u8; 32] = bs58::decode(&addr.secret)
                    .into_vec()
                    .expect("Invalid secret encoding")
                    .as_slice().try_into().unwrap();
                Ok(SecretKey::from_bytes(secret_bytes).unwrap())
            }
            None => Err(Error::Custom("No addresses in wallet".to_string())),
        }
    }

    /// Get the default secret as a bs58 string (for WalletStateProvider).
    fn default_secret_bs58(&self) -> std::result::Result<String, String> {
        let secrets = self.wallet.get_secrets()
            .map_err(|e| format!("{:?}", e))?;
        secrets.first()
            .cloned()
            .ok_or_else(|| "No secrets in wallet".to_string())
    }
}

// ============================================================================
// WalletStateProvider impl — provides wallet state to ContractClient::build()
// ============================================================================

use dwow_sdk::contract_client::{CapInfo, MerkleProofInfo, WalletStateProvider};

impl WalletStateProvider for Dww {
    fn default_address(&self) -> std::result::Result<String, String> {
        let addresses = self.wallet.get_addresses()
            .map_err(|e| format!("{:?}", e))?;
        match addresses.first() {
            Some(addr) => Ok(addr.public_key.clone()),
            None => Err("No addresses in wallet".to_string()),
        }
    }

    fn held_capabilities_for_token(&self, token_id: &str) -> std::result::Result<Vec<CapInfo>, String> {
        let cap_records = self.wallet.get_held_capabilities(Some(false))
            .map_err(|e| format!("{:?}", e))?;
        Ok(cap_records.iter()
            .filter(|c| c.token_id == token_id)
            .map(|c| CapInfo {
                cap_id: c.cap_id.clone(),
                value: c.value,
                token_id: c.token_id.clone(),
                leaf_position: c.leaf_position,
                secret: c.secret.clone(),
                cap_blind: c.cap_blind.clone(),
                value_blind: c.value_blind.clone(),
                token_blind: c.token_blind.clone(),
                spend_hook: c.spend_hook.clone(),
                user_data: c.user_data.clone(),
            })
            .collect())
    }

    fn get_merkle_proof(&self, cap_id: &str) -> std::result::Result<MerkleProofInfo, String> {
        // Get leaf position from the capability record
        let caps = self.wallet.get_held_capabilities(None)  // include exercised
            .map_err(|e| format!("{:?}", e))?;
        let cap = caps.iter()
            .find(|c| c.cap_id == cap_id)
            .ok_or_else(|| format!("Capability not found: {}", cap_id))?;
        let leaf_position = cap.leaf_position;

        // Get Merkle proof siblings
        let proof = self.wallet.get_merkle_proof(cap_id)
            .map_err(|e| format!("{:?}", e))?;
        Ok(MerkleProofInfo {
            siblings: proof.siblings,
            leaf_position,
        })
    }

    fn get_secret(&self) -> std::result::Result<String, String> {
        let secrets = self.wallet.get_secrets()
            .map_err(|e| format!("{:?}", e))?;
        secrets.first()
            .cloned()
            .ok_or_else(|| "No secrets in wallet".to_string())
    }
}

// original Dww impl continues below
impl Dww {
    /// Append fee call to transaction using NativeToken::FeeV1
    pub async fn append_fee_call(
        &self,
        _tx: &Transaction,
        _tree: &MerkleTree,
        _fee_pk: &ProvingKey,
        _fee_zkbin: &ZkBinary,
        _spent_coins: Option<&[PromissoryNote]>,
    ) -> Result<(ContractCall, Vec<Proof>, Vec<SecretKey>)> {
        Err(Error::Custom(
            "append_fee_call not yet implemented for Promissory Note. \
             Fee payment requires NativeToken::FeeV1 integration with DRKW capabilities.".to_string(),
        ))
    }

    /// Attach fee to transaction
    ///
    /// Builds a NativeToken::FeeV1 call using the wallet's first DRKW coin
    /// and appends it as a root-level call in the transaction.
    pub async fn attach_fee(&self, tx: &mut Transaction, _fee: u64) -> Result<()> {
        use crate::contract_imports::native_token::{
            DRKW_TOKEN_ID, FeeCallBuilder, FeeCallInput, FeeCallOutput,
            NATIVE_TOKEN_CONTRACT_ZKAS_FEE_V1_BIN,
        };
        use dwow_core::zk::{proof::ProvingKey, vm::ZkCircuit, vm_heap::empty_witnesses};
        use dwow_sdk::crypto::{BaseBlind, MerkleNode};
        use dwow_serial::Encodable;
        use rand::rngs::OsRng;

        const DEFAULT_FEE: u64 = 42_000_000;

        // Get DRKW coin for fee
        let dark_token_id_str = bs58::encode(DRKW_TOKEN_ID.to_repr()).into_string();
        let drkw_cap_records = self.wallet.get_capabilities_for_token(&dark_token_id_str, Some(false))
            .map_err(|e| Error::Custom(format!("Failed to get DRKW capabilities: {:?}", e)))?;

        if drkw_cap_records.is_empty() {
            return Err(Error::Custom(
                "No DRKW capabilities available for fee payment.".to_string(),
            ));
        }

        let drkw_cap = &drkw_cap_records[0];
        let dark_secret_bytes = bs58::decode(&drkw_cap.secret)
            .into_vec()
            .map_err(|e| Error::Custom(e.to_string()))?
            .try_into()
            .map_err(|_| Error::Custom("Invalid DRKW secret key length".to_string()))?;
        let dark_secret = SecretKey::from_bytes(dark_secret_bytes)
            .map_err(|_| Error::Custom("Failed to parse DRKW secret key".to_string()))?;

        let dark_merkle_proof = self.wallet.get_merkle_proof(&drkw_cap.cap_id)
            .map_err(|e| Error::Custom(format!("Failed to get DRKW Merkle proof: {:?}", e)))?;

        let dark_merkle_path: Vec<MerkleNode> = dark_merkle_proof
            .siblings
            .iter()
            .map(|s| {
                let bytes: [u8; 32] = bs58::decode(s)
                    .into_vec()
                    .map_err(|e| Error::Custom(e.to_string()))?
                    .try_into()
                    .map_err(|_| Error::Custom("Invalid Merkle node length".to_string()))?;
                Ok(MerkleNode::from_bytes(bytes)
                    .ok_or_else(|| Error::Custom("Invalid Merkle node".to_string()))?)
            })
            .collect::<Result<Vec<_>>>()?;

        let dark_coin_blind_bytes = bs58::decode(&drkw_cap.cap_blind)
            .into_vec()
            .map_err(|e| Error::Custom(e.to_string()))?
            .try_into()
            .map_err(|_| Error::Custom("Invalid capability blind length".to_string()))?;
        let drkw_cap_blind = pallas::Base::from_repr(dark_coin_blind_bytes)
            .into_option()
            .ok_or_else(|| Error::Custom("Invalid capability blind".to_string()))?;

        // Load fee ZK binary and build fee proof
        let fee_zkbin = ZkBinary::decode(NATIVE_TOKEN_CONTRACT_ZKAS_FEE_V1_BIN, false)
            .map_err(|e| Error::Custom(format!("Failed to decode fee ZK binary: {:?}", e)))?;

        let fee_empty_wits = empty_witnesses(&fee_zkbin)?;
        let fee_circuit = ZkCircuit::new(fee_empty_wits, &fee_zkbin);
        let fee_pk = ProvingKey::build(fee_zkbin.k, &fee_circuit);

        let fee_input = FeeCallInput {
            value: drkw_cap.value,
            token_id: DRKW_TOKEN_ID,
            spend_hook: pallas::Base::zero(),
            user_data: pallas::Base::zero(),
            coin_blind: drkw_cap_blind,
            leaf_position: drkw_cap.leaf_position,
            merkle_path: dark_merkle_path,
            secret: dark_secret,
            ephemeral_signature_secret: SecretKey::random(&mut OsRng),
        };

        let dark_public_key = PublicKey::from_secret(dark_secret);
        let change_blind = BaseBlind::random(&mut OsRng);
        let fee_output = FeeCallOutput {
            recipient: dark_public_key,
            value: drkw_cap.value.saturating_sub(DEFAULT_FEE),
            spend_hook: pallas::Base::zero(),
            user_data: pallas::Base::zero(),
            coin_blind: change_blind.inner(),
        };

        let fee_builder = FeeCallBuilder {
            input: fee_input,
            output: fee_output,
            fee_zkbin,
            fee_pk,
            fee: DEFAULT_FEE,
        };

        let fee_debris = fee_builder.build()
            .map_err(|e| Error::Custom(format!("Failed to build fee: {:?}", e)))?;

        // Encode fee params into call data (FeeV1 = 0x00)
        let mut fee_call_data = vec![0x00u8];
        fee_debris.params.encode(&mut fee_call_data)
            .map_err(|e| Error::Custom(format!("Failed to encode fee params: {:?}", e)))?;

        let fee_call = ContractCall {
            contract_id: *NATIVE_TOKEN_CONTRACT_ID,
            data: fee_call_data,
        };

        // Append fee as root-level call (no parent, no children)
        tx.calls.push(dwow_core::tx::DarkLeaf {
            data: fee_call,
            parent_index: None,
            children_indexes: vec![],
        });
        tx.proofs.push(vec![]);

        Ok(())
    }

    /// Mark coins from a transaction as spent in the wallet database.
    pub fn mark_tx_exercise(&self, tx: &Transaction, output: &mut Vec<String>) -> Result<()> {
        use dwow_sdk::contract_client::CapabilityInfo;

        // Get all unspent coins as held capabilities
        let unspent_coins = self.wallet.get_held_capabilities(Some(false))
            .map_err(|e| Error::Custom(format!("Failed to get capabilities: {:?}", e)))?;

        let held_capabilities: Vec<CapabilityInfo> = unspent_coins.iter()
            .map(|c| CapabilityInfo {
                capability_id: c.cap_id.clone(),
                secret: c.secret.clone(),
            })
            .collect();

        let client_registry = crate::contract_imports::get_client_registry();

        // For each call in the transaction, dispatch to the contract's client
        for call in &tx.calls {
            let contract_id = call.data.contract_id;

            // Look up the contract name from its ID for registry dispatch
            // For genesis contracts, use known IDs
            let contract_name: Option<&str> =
                if contract_id == *PROMISSORY_NOTE_CONTRACT_ID {
                    Some("promissory_note")
                } else if contract_id == *NATIVE_TOKEN_CONTRACT_ID {
                    Some("native_token")
                } else if contract_id == *crate::contract_imports::DEPLOYOOOR_CONTRACT_ID {
                    Some("deployooor")
                } else if crate::contract_imports::ATTESTATION_CONTRACT_ID.get().map_or(false, |id| contract_id == *id) {
                    Some("attestation")
                } else if crate::contract_imports::IDENTITY_CONTRACT_ID.get().map_or(false, |id| contract_id == *id) {
                    Some("identity")
                } else if crate::contract_imports::ORACLE_CONTRACT_ID.get().map_or(false, |id| contract_id == *id) {
                    Some("oracle")
                } else {
                    // Future: reverse-lookup from registered contract IDs
                    None
                };

            let Some(name) = contract_name else { continue; };
            let Some(client) = client_registry.get(name) else { continue; };

            let Some(_function_code) = call.data.data.first() else { continue; };
            let params_data = &call.data.data[1..];

            // Generic dispatch: each contract's client knows how to decode
            // its own call data and detect which capabilities were transferred
            let transferred = client.detect_transferred(params_data, &held_capabilities);

            for capability_id in &transferred {
                if let Err(e) = self.wallet.mark_revoked(capability_id, 0) {
                    output.push(format!(
                        "Failed to mark capability {} as revoked: {:?}", capability_id, e
                    ));
                } else {
                    output.push(format!(
                        "Marked capability {} as revoked", capability_id
                    ));
                }
            }
        }
        Ok(())
    }

    /// Unspent promissory note coins after block height
    pub fn retained_pn_caps_after(
        &self,
        height: &u32,
        _output: &mut Vec<String>,
    ) -> WalletDbResult<Vec<PromissoryNote>> {
        let all_coins = self.wallet.get_held_capabilities(Some(false))?;

        let filtered: Vec<&CapRecord> = all_coins
            .iter()
            .filter(|c| c.created_at_height > *height)
            .collect();

        if filtered.is_empty() {
            return Ok(vec![]);
        }

        match cap_records_to_pn_notes(&filtered.iter().map(|c| (*c).clone()).collect::<Vec<_>>()) {
            Ok(notes) => Ok(notes),
            Err(_) => Ok(vec![]),
        }
    }

    /// Remove promissory note coins after block height
    pub fn remove_pn_caps_after(
        &self,
        height: &u32,
        _output: &mut Vec<String>,
    ) -> WalletDbResult<()> {
        self.wallet.remove_capabilities_after(*height)?;
        Ok(())
    }

    /// Check if call is a money fee (NativeToken::FeeV1)
    pub fn is_native_token_fee(&self, call: &ContractCall) -> bool {
        call.contract_id == *NATIVE_TOKEN_CONTRACT_ID &&
            call.data.first() == Some(&0x00) // FeeV1 function code
    }

    /// Look up a spend_hook contract in the registry and return metadata.
    ///
    /// Returns `Some((name, category))` if the contract is registered, or `None`
    /// if the contract is unknown.  Callers should warn the user before sending
    /// coins to an unknown spend_hook target.
    pub fn check_spend_hook(&self, hook_contract_id: &ContractId) -> Option<(String, String)> {
        let registry = match self.wallet.get_contract_registry() {
            Ok(r) => r,
            Err(_) => return None,
        };

        let cid_str = hook_contract_id.to_string();
        for (name, stored_cid) in &registry {
            if stored_cid == &cid_str {
                let category = self.wallet
                    .get_contract_metadata(name)
                    .ok()
                    .map(|m| m.category)
                    .unwrap_or_else(|| "unknown".to_string());
                return Some((name.clone(), category));
            }
        }
        None
    }

    /// Initialize promissory note functionality.
    ///
    /// PN is a genesis contract — its WASM and manifest are embedded in the
    /// chain at genesis (see bin/dwowd/src/lib.rs::init_linear). The wallet
    /// auto-registers the manifest from the embedded TOML so it's available
    /// immediately without requiring a chain query or manual registration.
    pub fn initialize_promissory_note(&self, output: &mut Vec<String>) -> Result<()> {
        // Embed the PN manifest TOML at compile time (same file stored in
        // genesis by dwowd). The wallet resolves capabilities from this
        // manifest without needing to query the chain.
        let manifest_toml = include_str!("../../../src/contract/promissory_note/manifest.toml");
        let manifest = dwow_sdk::manifest::ContractManifest::from_toml(manifest_toml)
            .map_err(|e| Error::Custom(format!("Failed to parse PN manifest: {}", e)))?;

        // Store in wallet DB for manifest-based capability resolution
        let contract_id_hex = hex::encode(dwow_sdk::crypto::PROMISSORY_NOTE_CONTRACT_ID.to_bytes());
        self.wallet.store_manifest(&contract_id_hex, manifest_toml)
            .map_err(|e| Error::Custom(format!("Failed to store PN manifest: {:?}", e)))?;

        output.push(format!(
            "Promissory Note initialized — {} functions, {} circuits, {} actions (genesis manifest)",
            manifest.functions.len(), manifest.circuits.len(), manifest.actions.len()
        ));
        Ok(())
    }

    /// Initialize bearer bond functionality

    /// Initialize Identity genesis contract — embeds manifest at compile time.
    pub fn initialize_identity(&self, output: &mut Vec<String>) -> Result<()> {
        let manifest_toml = include_str!("../../../src/contract/identity/manifest.toml");
        let manifest = dwow_sdk::manifest::ContractManifest::from_toml(manifest_toml)
            .map_err(|e| Error::Custom(format!("Failed to parse Identity manifest: {}", e)))?;
        let manifest_json = serde_json::to_string(&manifest)
            .map_err(|e| Error::Custom(format!("Failed to serialize Identity manifest: {}", e)))?;
        let cid_str = bs58::encode(dwow_sdk::crypto::IDENTITY_CONTRACT_ID.to_bytes()).into_string();
        self.wallet.store_manifest(&cid_str, &manifest_json)
            .map_err(|e| Error::Custom(format!("{:?}", e)))?;
        output.push(format!(
            "Identity initialized — {} functions, {} circuits, {} actions (genesis manifest)",
            manifest.functions.len(), manifest.circuits.len(), manifest.actions.len()
        ));
        Ok(())
    }

    /// Initialize Oracle genesis contract — embeds manifest at compile time.
    pub fn initialize_oracle(&self, output: &mut Vec<String>) -> Result<()> {
        let manifest_toml = include_str!("../../../src/contract/oracle/manifest.toml");
        let manifest = dwow_sdk::manifest::ContractManifest::from_toml(manifest_toml)
            .map_err(|e| Error::Custom(format!("Failed to parse Oracle manifest: {}", e)))?;
        let manifest_json = serde_json::to_string(&manifest)
            .map_err(|e| Error::Custom(format!("Failed to serialize Oracle manifest: {}", e)))?;
        let cid_str = bs58::encode(dwow_sdk::crypto::ORACLE_CONTRACT_ID.to_bytes()).into_string();
        self.wallet.store_manifest(&cid_str, &manifest_json)
            .map_err(|e| Error::Custom(format!("{:?}", e)))?;
        output.push(format!(
            "Oracle initialized — {} functions, {} circuits, {} actions (genesis manifest)",
            manifest.functions.len(), manifest.circuits.len(), manifest.actions.len()
        ));
        Ok(())
    }

    /// Initialize Attestation genesis contract — embeds manifest at compile time.
    pub fn initialize_attestation(&self, output: &mut Vec<String>) -> Result<()> {
        let manifest_toml = include_str!("../../../src/contract/attestation/manifest.toml");
        let manifest = dwow_sdk::manifest::ContractManifest::from_toml(manifest_toml)
            .map_err(|e| Error::Custom(format!("Failed to parse Attestation manifest: {}", e)))?;
        let manifest_json = serde_json::to_string(&manifest)
            .map_err(|e| Error::Custom(format!("Failed to serialize Attestation manifest: {}", e)))?;
        let cid_str = bs58::encode(dwow_sdk::crypto::ATTESTATION_CONTRACT_ID.to_bytes()).into_string();
        self.wallet.store_manifest(&cid_str, &manifest_json)
            .map_err(|e| Error::Custom(format!("{:?}", e)))?;
        output.push(format!(
            "Attestation initialized — {} functions, {} circuits, {} actions (genesis manifest)",
            manifest.functions.len(), manifest.circuits.len(), manifest.actions.len()
        ));
        Ok(())
    }

    /// PromissoryNote keygen
    pub fn keygen(&self, output: &mut Vec<String>) -> Result<Keypair> {
        use dwow_sdk::crypto::Keypair;
        use rand::rngs::OsRng;

        // Generate new keypair
        let keypair = Keypair::random(&mut OsRng);

        // Encode to base58
        let public_str = bs58::encode(keypair.public.to_bytes()).into_string();
        let secret_bytes: [u8; 32] = keypair.secret.inner().to_repr();
        let secret_str = bs58::encode(secret_bytes).into_string();

        // Check if this is the first address (set as default)
        let addresses = self.wallet.get_addresses()
            .map_err(|e| Error::Custom(format!("Database error: {:?}", e)))?;
        let is_default = addresses.is_empty();

        // Store in database
        self.wallet.insert_address(&public_str, &secret_str, is_default, 0)
            .map_err(|e| Error::Custom(format!("Failed to store address: {:?}", e)))?;

        // Also insert secret into coin_secrets so wallet can decrypt notes sent to this address
        // Use empty string "" for cap_id (same pattern as import_secrets)
        self.wallet.insert_secret(&secret_str, "")
            .map_err(|e| Error::Custom(format!("Failed to insert secret: {:?}", e)))?;

        output.push(format!("Generated new address: {}", &public_str[..16]));
        output.push(format!("Address (bs58): {public_str}"));
        output.push(format!("Secret (hex): {}", hex::encode(secret_bytes)));

        Ok(keypair)
    }

    /// Money balance
    pub fn token_balance(&self) -> Result<HashMap<String, u64>> {
        let mut balances: HashMap<String, u64> = HashMap::new();

        // Get all unspent coins
        let coin_records = self.wallet.get_held_capabilities(Some(false)).map_err(|e| Error::Custom(format!("{:?}", e)))?;

        for record in coin_records {
            *balances.entry(record.token_id).or_insert(0) += record.value;
        }

        Ok(balances)
    }

    /// Set default address

    /// Mining config

    /// Import promissory note secrets
    pub fn import_secrets(&self, secrets: Vec<SecretKey>, output: &mut Vec<String>) -> Result<Vec<SecretKey>> {
        // Check if any addresses exist already (first import sets default)
        let addresses = self.wallet.get_addresses()
            .map_err(|e| Error::Custom(format!("Database error: {:?}", e)))?;
        let mut is_default = addresses.is_empty();

        for secret in &secrets {
            let secret_bytes: [u8; 32] = secret.inner().to_repr();
            let secret_str = bs58::encode(secret_bytes).into_string();
            let public = dwow_sdk::crypto::PublicKey::from_secret(*secret);
            let public_str = bs58::encode(public.to_bytes()).into_string();

            // Store address (needed for wallet address, default_address, balance display)
            self.wallet.insert_address(&public_str, &secret_str, is_default, 0)
                .map_err(|e| Error::Custom(format!("Failed to store address: {:?}", e)))?;
            // Store secret for AEAD decryption during block scanning
            self.wallet.insert_secret(&secret_str, "")
                .map_err(|e| Error::Custom(format!("Failed to insert secret: {:?}", e)))?;

            output.push(format!("Imported secret: {}", &secret_str[..8]));
            is_default = false; // only first imported key is default
        }
        Ok(secrets)
    }

    /// Unspent coin
    pub fn retain_cap(&self, coin: &pallas::Base) -> Result<()> {
        let cap_id = bs58::encode(coin.to_repr()).into_string();
        self.wallet.mark_retained(&cap_id).map_err(|e| Error::Custom(format!("{:?}", e)))
    }

    /// Get aliases
    pub fn get_aliases(
        &self,
        _alias: Option<String>,
        _token_id: Option<TokenId>,
    ) -> Result<HashMap<String, TokenId>> {
        let aliases = self.wallet.get_aliases()
            .map_err(|e| Error::Custom(format!("Database error: {:?}", e)))?;

        // If specific alias requested, return just that one
        if let Some(alias_str) = _alias {
            for a in aliases {
                if a.alias == alias_str {
                    if let Ok(tid) = TokenId::from_str(&a.token_id) {
                        let mut result = HashMap::new();
                        result.insert(alias_str, tid);
                        return Ok(result)
                    }
                }
            }
            return Ok(HashMap::new())
        }

        // If specific token_id requested, find its alias
        if let Some(token_id) = _token_id {
            for a in aliases {
                if a.token_id == token_id.to_string() {
                    let mut result = HashMap::new();
                    result.insert(a.alias, token_id);
                    return Ok(result)
                }
            }
            return Ok(HashMap::new())
        }

        // Return all aliases
        let mut result = HashMap::new();
        for a in aliases {
            // Parse the token_id string to TokenId
            if let Ok(tid) = TokenId::from_str(&a.token_id) {
                result.insert(a.alias, tid);
            }
        }
        Ok(result)
    }

    /// Remove alias

    /// Add alias
    pub fn add_alias(
        &self,
        alias: String,
        token_id: TokenId,
        output: &mut Vec<String>,
    ) -> Result<()> {
        self.wallet.insert_alias(&alias, &token_id.to_string(), 0)
            .map_err(|e| Error::Custom(format!("Failed to store alias: {:?}", e)))?;
        output.push(format!("Added alias {} for token (stored)", alias));
        Ok(())
    }

    /// Unfreeze mint authorities after height (stub)
    pub fn unfreeze_mint_authorities_after(&self, _height: &u32, _output: &mut Vec<String>) -> WalletDbResult<()> {
        Err(WalletDbError::GenericError)
    }

    /// Unlock deploy authorities after height (stub)
    pub fn unlock_deploy_authorities_after(&self, _height: &u32, _output: &mut Vec<String>) -> WalletDbResult<()> {
        Err(WalletDbError::GenericError)
    }

    /// Remove deploy history after height (stub)
    pub fn remove_deploy_history_after(&self, _height: &u32, _output: &mut Vec<String>) -> WalletDbResult<()> {
        Err(WalletDbError::GenericError)
    }

    /// Reset deploy authorities
    pub fn reset_deploy_authorities(&self, _output: &mut Vec<String>) -> WalletDbResult<()> {
        self.wallet.remove_deploy_authorities()
    }

    /// Reset deploy history (stub)
    pub fn reset_deploy_history(&self, _output: &mut Vec<String>) -> WalletDbResult<()> {
        // Stub - deploy history not yet implemented
        Ok(())
    }

    /// Initialize deployooor
    pub fn initialize_deployooor(&self, output: &mut Vec<String>) -> Result<()> {
        output.push("Deployooor initialized".to_string());
        Ok(())
    }

    /// Get mint authorities (stub)
    /// Look up the mint authority secret for a given token_id
    pub fn get_mint_authority_for_token(&self, token_id: &TokenId) -> Result<SecretKey> {
        let token_id_str = token_id.to_string();
        let token_info = self.wallet.get_token(&token_id_str)
            .map_err(|e| Error::Custom(format!("{:?}", e)))?
            .ok_or_else(|| Error::Custom(format!("Token {} not found in wallet", token_id_str)))?;
        let mint_auth_str = token_info.mint_authority
            .ok_or_else(|| Error::Custom(format!("No mint authority stored for token {}", token_id_str)))?;
        let bytes: [u8; 32] = bs58::decode(&mint_auth_str)
            .into_vec()
            .map_err(|e| Error::Custom(e.to_string()))?
            .try_into()
            .map_err(|_| Error::Custom("Invalid mint authority length".to_string()))?;
        Ok(SecretKey::from_bytes(bytes)
            .map_err(|_| Error::Custom("Failed to parse mint authority secret key".to_string()))?)
    }

    /// List all tokens with mint authorities in the wallet
    pub fn get_mint_authorities(&self) -> Result<Vec<(TokenId, SecretKey)>> {
        let all_tokens = self.wallet.get_all_tokens()
            .map_err(|e| Error::Custom(format!("{:?}", e)))?;
        let mut result = Vec::new();
        for token in all_tokens {
            if let Some(ref mint_auth_str) = token.mint_authority {
                let token_id_bytes: [u8; 32] = bs58::decode(&token.token_id)
                    .into_vec()
                    .map_err(|e| Error::Custom(e.to_string()))?
                    .try_into()
                    .map_err(|_| Error::Custom("Invalid token_id length".to_string()))?;
                let token_id = pallas::Base::from_repr(token_id_bytes)
                    .into_option()
                    .ok_or_else(|| Error::Custom("Invalid token_id field element".to_string()))?;
                let auth_bytes: [u8; 32] = bs58::decode(mint_auth_str)
                    .into_vec()
                    .map_err(|e| Error::Custom(e.to_string()))?
                    .try_into()
                    .map_err(|_| Error::Custom("Invalid mint authority length".to_string()))?;
                let auth = SecretKey::from_bytes(auth_bytes)
                    .map_err(|_| Error::Custom("Failed to parse mint authority secret key".to_string()))?;
                result.push((token_id, auth));
            }
        }
        Ok(result)
    }

    /// Deploy auth keygen — generates a new deploy authority keypair
    /// and persists it to the wallet database.

    /// List deploy authorities stored in the wallet database.

    /// Register a contract ID for runtime use. Persists to wallet DB so
    /// subsequent `drk` invocations automatically load it.
    pub fn register_contract_id(&self, name: &str, cid: ContractId) -> Result<()> {
        let cid_str = bs58::encode(cid.to_bytes()).into_string();
        self.wallet.register_contract(name, &cid_str)
            .map_err(|e| Error::Custom(format!("Failed to persist contract registry: {:?}", e)))?;
        crate::contract_imports::register_contract_id(name, cid)
            .map_err(|e| Error::Custom(e))
    }

    /// Retrieve a stored contract manifest from the wallet DB.
    /// Returns None if no manifest was stored for this contract.
    pub fn get_contract_manifest(
        &self,
        contract_id: &str,
    ) -> Result<Option<dwow_sdk::manifest::ContractManifest>> {
        self.wallet.get_contract_manifest(contract_id)
            .map_err(|e| Error::Custom(format!("DB error: {e:?}")))
    }

    /// Redeem a Promissory Note coin via RedeemV1 (0x01).
    ///
    /// Destroys the coin's monetary value and creates a zero-value receipt coin
    /// as cryptographic proof of redemption. The receipt is permanent, verifiable,
    /// and non-transferable.
    pub async fn redeem(
        &self,
        cap_id: String,
        spend_hook: Option<pallas::Base>,
    ) -> Result<Transaction> {
        // Look up coin in wallet
        let coin_records = self.wallet.get_held_capabilities(Some(false))
            .map_err(|e| Error::Custom(format!("Failed to get capabilities: {:?}", e)))?;
        let coin_record = coin_records.iter()
            .find(|c| c.cap_id == cap_id)
            .ok_or_else(|| Error::Custom(format!("Capability not found: {}", cap_id)))?;

        // Get secret for this coin
        let secret_bytes: [u8; 32] = bs58::decode(&coin_record.secret)
            .into_vec()
            .map_err(|e| Error::Custom(e.to_string()))?
            .try_into()
            .map_err(|_| Error::Custom("Invalid secret key length".to_string()))?;
        let secret = SecretKey::from_bytes(secret_bytes)
            .map_err(|_| Error::Custom("Failed to parse secret key".to_string()))?;

        // Get Merkle proof
        let merkle_proof = self.wallet.get_merkle_proof(&coin_record.cap_id)
            .map_err(|e| Error::Custom(format!("Failed to get Merkle proof: {:?}", e)))?;
        let merkle_path: Vec<MerkleNode> = merkle_proof.siblings.iter()
            .map(|s| {
                let bytes: [u8; 32] = bs58::decode(s).into_vec()
                    .map_err(|e| Error::Custom(e.to_string()))?
                    .try_into()
                    .map_err(|_| Error::Custom("Invalid Merkle node length".to_string()))?;
                Ok(MerkleNode::from_bytes(bytes)
                    .ok_or_else(|| Error::Custom("Invalid Merkle node".to_string()))?)
            })
            .collect::<Result<Vec<_>>>()?;

        // Parse coin fields
        let coin_blind = crate::transfer::decode_bs58_field(&coin_record.cap_blind)?;
        let token_id = crate::transfer::decode_bs58_field(&coin_record.token_id)?;
        let spend_hook_in = match coin_record.spend_hook {
            Some(ref s) => crate::transfer::decode_bs58_field(s)?,
            None => pallas::Base::zero(),
        };
        let user_data_in = match coin_record.user_data {
            Some(ref s) => crate::transfer::decode_bs58_field(s)?,
            None => pallas::Base::zero(),
        };
        let spend_hook_out = spend_hook.unwrap_or(spend_hook_in);

        // Build RedeemV1 via PromissoryNoteClient — ZK knowledge in contract crate
        let input = crate::contract_imports::promissory_note::RedeemCallInput {
            value: coin_record.value,
            token_id,
            spend_hook: spend_hook_in,
            user_data: user_data_in,
            coin_blind,
            leaf_position: coin_record.leaf_position,
            merkle_path,
            secret: secret.inner(),
            ephemeral_signature_secret: SecretKey::random(&mut OsRng).inner(),
        };

        let recipient_pub = PublicKey::from_secret(secret);
        let receipt_coin_blind = BaseBlind::random(&mut OsRng);
        let output = crate::contract_imports::promissory_note::RedeemCallOutput {
            recipient: poseidon_hash([recipient_pub.x()]),
            recipient_pub,
            token_id,
            spend_hook: spend_hook_out,
            user_data: pallas::Base::zero(),
            coin_blind: receipt_coin_blind.inner(),
        };

        let (pn_call_data, pn_proof_bytes) =
            dwow_promissory_note_contract::client::PromissoryNoteClient::build_redeem(
                input, output,
            )
            .await
            .map_err(|e| Error::Custom(format!("Failed to build Redeem: {}", e)))?;

        let pn_cid = Some(*PROMISSORY_NOTE_CONTRACT_ID)
            .ok_or_else(|| Error::Custom("Promissory Note contract ID not initialized".to_string()))?;
        let mut call_data = vec![crate::contract_imports::promissory_note::PromissoryNoteFunction::RedeemV1 as u8];
        call_data.extend_from_slice(&pn_call_data);
        let redeem_call = ContractCall { contract_id: pn_cid, data: call_data };

        let redeem_proofs: Vec<Proof> =
            pn_proof_bytes.into_iter().map(|b| Proof::new(b)).collect();
        let redeem_leaf = ContractCallLeaf { call: redeem_call, proofs: redeem_proofs };
        crate::fee_builder::build_fee_and_finalize_tx(
            &self.wallet, redeem_leaf, None,
        )
    }

    /// Burn Promissory Note coins via BurnV1 (0x03).
    ///
    /// Destroys coins and publishes nullifiers. If any input coin has a non-zero
    /// spend_hook, the PN contract will dispatch a callback to the target contract.
    pub async fn burn(
        &self,
        coin_ids: Vec<String>,
    ) -> Result<Transaction> {
        if coin_ids.is_empty() {
            return Err(Error::Custom("At least one cap ID is required for burn".to_string()));
        }

        let unspent_coins = self.wallet.get_held_capabilities(Some(false))
            .map_err(|e| Error::Custom(format!("Failed to get capabilities: {:?}", e)))?;

        let mut inputs: Vec<crate::contract_imports::promissory_note::BurnCallInput> = vec![];

        for cap_id in &coin_ids {
            let coin_record = unspent_coins.iter()
                .find(|c| &c.cap_id == cap_id)
                .ok_or_else(|| Error::Custom(format!("Capability not found: {}", cap_id)))?;

            let secret_bytes: [u8; 32] = bs58::decode(&coin_record.secret)
                .into_vec().map_err(|e| Error::Custom(e.to_string()))?
                .try_into().map_err(|_| Error::Custom("Invalid secret key length".to_string()))?;
            let secret = SecretKey::from_bytes(secret_bytes)
                .map_err(|_| Error::Custom("Failed to parse secret key".to_string()))?;

            let merkle_proof = self.wallet.get_merkle_proof(&coin_record.cap_id)
                .map_err(|e| Error::Custom(format!("Failed to get Merkle proof: {:?}", e)))?;
            let merkle_path: Vec<MerkleNode> = merkle_proof.siblings.iter()
                .map(|s| {
                    let bytes: [u8; 32] = bs58::decode(s).into_vec()
                        .map_err(|e| Error::Custom(e.to_string()))?
                        .try_into()
                        .map_err(|_| Error::Custom("Invalid Merkle node length".to_string()))?;
                    Ok(MerkleNode::from_bytes(bytes)
                        .ok_or_else(|| Error::Custom("Invalid Merkle node".to_string()))?)
                })
                .collect::<Result<Vec<_>>>()?;

            let coin_blind = crate::transfer::decode_bs58_field(&coin_record.cap_blind)?;
            let token_id = crate::transfer::decode_bs58_field(&coin_record.token_id)?;
            let spend_hook = match coin_record.spend_hook {
                Some(ref s) => crate::transfer::decode_bs58_field(s)?,
                None => pallas::Base::zero(),
            };
            let user_data = match coin_record.user_data {
                Some(ref s) => crate::transfer::decode_bs58_field(s)?,
                None => pallas::Base::zero(),
            };

            inputs.push(crate::contract_imports::promissory_note::BurnCallInput {
                value: coin_record.value,
                token_id,
                spend_hook,
                user_data,
                coin_blind,
                leaf_position: coin_record.leaf_position,
                merkle_path,
                secret: secret.inner(),
                ephemeral_signature_secret: SecretKey::random(&mut OsRng).inner(),
            });
        }

        // Build BurnV1 via PromissoryNoteClient — ZK knowledge in contract crate
        let (pn_call_data, pn_proof_bytes) =
            dwow_promissory_note_contract::client::PromissoryNoteClient::build_burn(
                inputs,
            )
            .await
            .map_err(|e| Error::Custom(format!("Failed to build Burn: {}", e)))?;

        let pn_cid = Some(*PROMISSORY_NOTE_CONTRACT_ID)
            .ok_or_else(|| Error::Custom("Promissory Note contract ID not initialized".to_string()))?;
        let mut call_data = vec![crate::contract_imports::promissory_note::PromissoryNoteFunction::BurnV1 as u8];
        call_data.extend_from_slice(&pn_call_data);
        let burn_call = ContractCall { contract_id: pn_cid, data: call_data };

        let burn_proofs: Vec<Proof> = pn_proof_bytes.into_iter().map(|b| Proof::new(b)).collect();
        let burn_leaf = ContractCallLeaf { call: burn_call, proofs: burn_proofs };
        crate::fee_builder::build_fee_and_finalize_tx(
            &self.wallet, burn_leaf, None,
        )
    }

    /// Lock a deployed contract — marks it as immutable via Deployooor LockV1.
    /// `deploy_auth` is the hex-encoded secret key of the deployer.
    pub async fn lock_contract(&self, deploy_auth: &str) -> Result<Transaction> {
        // Parse deploy key
        let secret_bytes = hex::decode(deploy_auth)
            .map_err(|e| Error::Custom(format!("Invalid deploy auth hex: {}", e)))?;
        let mut key_bytes = [0u8; 32];
        key_bytes.copy_from_slice(&secret_bytes);
        let deploy_key = dwow_sdk::crypto::SecretKey::from_bytes(key_bytes)
            .map_err(|e| Error::Custom(format!("Invalid deploy key: {}", e)))?;
        let keypair = dwow_sdk::crypto::Keypair::new(deploy_key);
        let public_key_bs58 = bs58::encode(keypair.public.to_bytes()).into_string();

        // Build LockV1 params — just the deployer's public key
        let params = format!(r#"{{"public_key":"{}"}}"#, public_key_bs58);

        // Route through generic invoke — uses Deployooor's ContractClient
        self.invoke_contract("deployooor", "LockV1", Some(&params), vec![]).await
    }

    /// Get deploy auth history (stub)

    /// Get deploy history record data (stub)

    /// Invoke a smart contract function
    ///
    /// This is a universal contract invocation method that can call any function
    /// on any deployed contract without needing contract-specific CLI code.
    ///
    /// # Arguments
    /// * `contract_id_or_name` - Contract ID (Base58 encoded) or name (e.g., "dao_escrow")
    /// * `function` - Function name to call (e.g., "enable_drain_protection")
    /// * `params` - JSON string with function parameters
    /// * `proofs` - ZK proofs for functions that require them; use `vec![]` for non-ZK functions
    pub async fn invoke_contract(
        &self,
        contract_id_or_name: &str,
        function: &str,
        params: Option<&str>,
        proofs: Vec<Vec<u8>>,
    ) -> Result<Transaction> {
        // First try to look up as contract name (e.g., "dao_escrow")
        // If not found, try to parse as Base58 contract ID
        let metadata = crate::contract_metadata::CONTRACT_METADATA_REGISTRY
            .get(contract_id_or_name)
            .or_else(|| {
                // Path B: Parse as Base58 ContractId, then reverse-lookup
                // the contract name from the runtime OnceLock registry.
                let cid = bs58::decode(contract_id_or_name)
                    .into_vec()
                    .ok()
                    .and_then(|v| v.try_into().ok())
                    .and_then(|bytes: [u8; 32]| ContractId::from_bytes(bytes).ok());
                cid.and_then(|id| {
                    crate::contract_metadata::CONTRACT_METADATA_REGISTRY
                        .find_by_contract_id(&id)
                })
                .and_then(|name| {
                    crate::contract_metadata::CONTRACT_METADATA_REGISTRY.get(name)
                })
            })
            .ok_or_else(|| Error::Custom(format!("Unknown contract: {}", contract_id_or_name)))?;

        let func_sig = metadata.get_function(function)
            .ok_or_else(|| Error::Custom(format!("Unknown function: {} on contract {}", function, metadata.name)))?;

        // Get the actual contract ID from the runtime registry
        let contract_id = crate::contract_imports::get_contract_id(metadata.name)
            .ok_or_else(|| {
                Error::Custom(format!("Contract {} not registered in runtime", metadata.name))
            })?;

        // Build call data based on contract and function.
        // Generic dispatch: try the contract's own client module first
        // (in its crate), then fall through to wallet-side handling.
        let mut call_data = vec![func_sig.code];

        // Try the ContractClient trait dispatch first.
        // Each contract implements ContractClient in its own crate.
        // The wallet does NOT contain per-contract logic.
        {
            let client_registry = crate::contract_imports::get_client_registry();
            if let Some(client) = client_registry.get(metadata.name) {
                let (contract_call_data, proofs) = client
                    .build(function, params.unwrap_or("{}"), self)
                    .map_err(|e| Error::Custom(e))?;
                call_data.extend_from_slice(&contract_call_data);

                // Create contract call leaf with proofs
                let contract_call = ContractCall {
                    contract_id,
                    data: call_data,
                };
                let leaf = ContractCallLeaf {
                    call: contract_call,
                    proofs: proofs.into_iter().map(|p| Proof::new(p)).collect(),
                };
                return crate::fee_builder::build_fee_and_finalize_tx(&self.wallet, leaf, None);
            }
        }

        // All contract parameter encoding goes through the ContractClient trait
        // (in each contract's own crate). The wallet has no per-contract logic.
        // If we reach here, the contract is known (in metadata registry) but
        // no client is registered for it — the fallback is generic ZK/non-ZK handling.
        match () {
            // Pre-flight ZK proof validation: ensure ZK-requiring functions
            // have proofs attached before building the transaction. This catches
            // the common mistake of calling a ZK function without generating a
            // proof — which would fail consensus silently.
            _ if func_sig.requires_proof => {
                if proofs.is_empty() {
                    return Err(Error::Custom(format!(
                        "ZK proof required: {} function '{}' requires a ZK proof \
                         (circuit: {:?}) but no proofs were provided. \
                         Generate a proof using the contract's client module \
                         before calling this function.",
                        metadata.name,
                        function,
                        func_sig.proof_circuit,
                    )));
                }
                for (i, proof) in proofs.iter().enumerate() {
                    if proof.is_empty() {
                        return Err(Error::Custom(format!(
                            "ZK proof {i} for {}::{} is empty. \
                             Each proof must contain valid Halo2 proof bytes.",
                            metadata.name, function,
                        )));
                    }
                }
            }

            // Non-ZK functions with no specific parameter encoding:
            // use only the function code byte (no additional params).
            _ if !func_sig.requires_proof => {
                // call_data already has func_sig.code; nothing more to encode
            }

            // ZK function with no specific parameter encoding
            _ => {
                return Err(Error::Custom(format!(
                    "Unsupported function: {} on contract: {}. \
                     This function requires a ZK proof, but parameter encoding \
                     is not yet implemented for this contract/function pair.",
                    function, metadata.name
                )));
            }
        }

        // Create contract call
        let contract_call = ContractCall {
            contract_id,
            data: call_data,
        };

        // Create contract call leaf with ZK proofs
        let leaf = ContractCallLeaf {
            call: contract_call,
            proofs: proofs.into_iter().map(Proof::new).collect(),
        };

        // Build transaction with fee
        let tx = crate::fee_builder::build_fee_and_finalize_tx(&self.wallet, leaf, None)?;

        Ok(tx)
    }
}

// =============================================================================================
// HELPER FUNCTIONS
// =============================================================================================

/// Convert CapRecord database records to PromissoryNote structs.
fn cap_records_to_pn_notes(records: &[CapRecord]) -> Result<Vec<PromissoryNote>> {
    use dwow_sdk::pasta::pallas;

    let mut notes = vec![];
    for record in records {
        let token_id_bytes = bs58::decode(&record.token_id)
            .into_vec()
            .map_err(|e| Error::Custom(e.to_string()))?
            .try_into()
            .map_err(|_| Error::Custom("Invalid token_id length".to_string()))?;
        let token_id = pallas::Base::from_repr(token_id_bytes)
            .into_option()
            .ok_or_else(|| Error::Custom("Invalid token_id".to_string()))?;

        let spend_hook = match &record.spend_hook {
            Some(s) if s != "11111111111111111111111111111" => {
                let bytes = bs58::decode(s)
                    .into_vec()
                    .map_err(|e| Error::Custom(e.to_string()))?
                    .try_into()
                    .map_err(|_| Error::Custom("Invalid spend_hook length".to_string()))?;
                pallas::Base::from_repr(bytes)
                    .into_option()
                    .ok_or_else(|| Error::Custom("Invalid spend_hook".to_string()))?
            }
            _ => pallas::Base::zero(),
        };

        let user_data = match &record.user_data {
            Some(s) if !s.is_empty() => {
                let bytes = bs58::decode(s)
                    .into_vec()
                    .map_err(|e| Error::Custom(e.to_string()))?
                    .try_into()
                    .map_err(|_| Error::Custom("Invalid user_data length".to_string()))?;
                pallas::Base::from_repr(bytes)
                    .into_option()
                    .ok_or_else(|| Error::Custom("Invalid user_data".to_string()))?
            }
            _ => pallas::Base::zero(),
        };

        let coin_blind_bytes = bs58::decode(&record.cap_blind)
            .into_vec()
            .map_err(|e| Error::Custom(e.to_string()))?
            .try_into()
            .map_err(|_| Error::Custom("Invalid cap_blind length".to_string()))?;
        let coin_blind = pallas::Base::from_repr(coin_blind_bytes)
            .into_option()
            .ok_or_else(|| Error::Custom("Invalid cap_blind".to_string()))?;

        let value_blind_bytes = bs58::decode(&record.value_blind)
            .into_vec()
            .map_err(|e| Error::Custom(e.to_string()))?
            .try_into()
            .map_err(|_| Error::Custom("Invalid value_blind length".to_string()))?;
        let value_blind = pallas::Scalar::from_repr(value_blind_bytes)
            .into_option()
            .ok_or_else(|| Error::Custom("Invalid value_blind".to_string()))?;

        let token_blind_bytes = bs58::decode(&record.token_blind)
            .into_vec()
            .map_err(|e| Error::Custom(e.to_string()))?
            .try_into()
            .map_err(|_| Error::Custom("Invalid token_blind length".to_string()))?;
        let token_blind = pallas::Base::from_repr(token_blind_bytes)
            .into_option()
            .ok_or_else(|| Error::Custom("Invalid token_blind".to_string()))?;

        notes.push(PromissoryNote {
            value: record.value,
            token_id,
            spend_hook,
            user_data,
            coin_blind,
            value_blind,
            token_blind,
            memo: vec![],
        });
    }
    Ok(notes)
}
