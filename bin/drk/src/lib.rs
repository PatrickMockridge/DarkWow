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

use std::{collections::HashMap, fs::create_dir_all, sync::Arc, time::{Duration, Instant}};

use bs58;
use hex;
use rand::rngs::OsRng;

use smol::lock::RwLock;
use tracing::{error, info};

use dwow_core::{
    net::hosts::HostColor,
    tx::{ContractCallLeaf, Transaction},
    zk::Proof,
    zkas::ZkBinary,
};
use crate::wallet_error::{Error, Result};
use crate::wallet_util::expand_path;
use dwow_serial::AsyncEncodable;
use dwow_sdk::{
    crypto::{
        keypair::{Address, Keypair, Network, PublicKey, SecretKey},
        pasta_prelude::PrimeField,
        poseidon_hash, BaseBlind, ContractId, MerkleNode, MerkleTree,
    },
    pasta::pallas,
    tx::ContractCall,
};
use dwow_promissory_note_contract::client::PromissoryNote;
use crate::contract_imports::{promissory_note::TokenId, PROMISSORY_NOTE_CONTRACT_ID, NATIVE_TOKEN_CONTRACT_ID};
use dwow_sdk::crypto::util::FieldElemAsStr;
use crate::walletdb::CapRecord;

/// CLI argument parsing — visible, testable
pub mod args;

/// Configuration loading — TOML + CLI merge, sync
pub mod config;

/// Subcommand dispatch — classify + route to wallet methods
pub mod dispatch;

/// Capability resolver — wallet capability browser (ocap.md)
pub mod capability;

/// Contract manifest resolver — on-chain ABI queries
pub mod manifest_resolver;

/// WASM manifest verification — mechanical, zero-trust
pub mod manifest_verify;

/// Error codes
pub mod error;
use error::{WalletDbError, WalletDbResult};

/// Common shared functions
pub mod common;

/// Coin selection — multi-input, fee-aware, dust threshold
pub mod coin_selection;

/// Local block scanning — cap discovery, AEAD decryption
pub mod scan;

/// Payment methods
pub mod transfer;

/// Swap methods
pub mod swap;

/// Token methods
pub mod token;

/// Wallet-owned error types — replaces dwow_core::Error
pub mod wallet_error;

/// Wallet-owned P2P networking — replaces dwow_core::net
pub mod p2p_wallet;

/// Wallet-owned utility functions — replaces dwow_core::util
pub mod wallet_util;

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
pub mod rpc_server;
pub mod sled_checksum;
pub mod local_wallet;
pub mod wallet_rpc_client;

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
    /// Wallet database operations handler (SQLite — keys, capabilities, contracts)
    pub wallet: WalletPtr,
    /// P2P network instance — dwow_core::net::P2p, same as mining nodes.
    pub p2p: Option<dwow_core::net::P2pPtr>,
    /// Async executor for P2p sessions.
    pub executor: Option<dwow_core::system::ExecutorPtr>,
    /// P2P network settings from config [net] section
    pub p2p_settings: Option<crate::p2p_wallet::P2pWalletConfig>,
    /// Highest peer chain tip seen by sync task. Updated on each Tip response.
    pub highest_peer_tip: Arc<crate::sync_task::HighestPeerTip>,
    /// Highest block height with a verified Caribina (Arweave) anchor.
    /// Blocks below this height are cryptographically final — cannot be reorged.
    /// The chain state rejects AnchoredBlockConflict for anchored blocks.
    pub verified_anchor_height: smol::lock::Mutex<u32>,
}

impl Dww {
    pub fn new(
        network: Network,
        chain_path: String,
        cache_path: String,
        wallet_path: String,
        wallet_pass: String,
        production_mode: bool,
        p2p_settings: Option<crate::p2p_wallet::P2pWalletConfig>,
    ) -> Result<Self> {
        // Open wallet's own chain block store (wallet syncs independently via P2P).
        // Retry on lock contention — a previous wallet process may not have
        // released the sled lock before this process starts.
        let chain_db_path = expand_path(&chain_path)?;
        let chain_db = sled::Config::new()
            .path(&chain_db_path)
            .cache_capacity(256 * 1024 * 1024) // 256MB — blocks + txs
            // TODO: .checksumming(true) — requires sled 0.35+
            .open()
            .map_err(|e| {
                if e.to_string().contains("WouldBlock") {
                    Error::DatabaseError(
                        "sled locked by another process — ensure no other wallet daemon is running"
                            .into(),
                    )
                } else {
                    Error::DatabaseError(format!("sled open: {e}"))
                }
            })?;
        let chain = dwow_chain::LinearStore::new(Arc::new(chain_db))
            .map_err(|e| Error::Custom(format!("Failed to open chain store: {}", e)))?;

        // Initialize blockchain cache database
        let db_path = expand_path(&cache_path)?;
        let sled_db = sled::Config::new()
            .path(&db_path)
            .cache_capacity(128 * 1024 * 1024) // 128MB — merkle trees, nullifier SMT
            // TODO: .checksumming(true) — requires sled 0.35+
            .open()
            .map_err(|e| {
                if e.to_string().contains("WouldBlock") {
                    Error::DatabaseError(
                        "sled locked by another process — ensure no other wallet daemon is running"
                            .into(),
                    )
                } else {
                    Error::DatabaseError(format!("sled open cache: {e}"))
                }
            })?;
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
        let Ok(wallet) = WalletDb::new(Some(wallet_path), Some(&wallet_pass), production_mode) else {
            return Err(Error::DatabaseError(format!("{}", WalletDbError::InitializationFailed)));
        };

        Ok(Self { network, chain, cache, wallet, p2p: None, executor: None, p2p_settings, highest_peer_tip: Arc::new(crate::sync_task::HighestPeerTip::new()), verified_anchor_height: smol::lock::Mutex::new(0) })
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
    /// Initialize P2P networking using dwow_core::net::P2p.
    /// Same stack as the mining nodes: P2p::new() → start() → seed().
    /// Connects to seeds, exchanges hostlist, discovers mining peers.
    /// Idempotent — returns immediately if P2P is already initialized.
    pub async fn init_p2p(
        &mut self,
        executor: std::sync::Arc<smol::Executor<'static>>,
    ) -> Result<()> {
        if self.p2p.is_some() {
            return Ok(());
        }
        let config = self.p2p_settings.clone()
            .ok_or_else(|| Error::Custom("P2P not configured — add [net] section to wallet config".into()))?;

        let settings = crate::config::build_p2p_settings(&config)?;

        self.executor = Some(executor.clone());

        let p2p = dwow_core::net::P2p::new(settings, executor).await
            .map_err(|e| Error::Custom(format!("P2p::new: {e}")))?;
        p2p.clone().start().await
            .map_err(|e| Error::Custom(format!("P2p::start: {e}")))?;

        // Connect to seed nodes — same unconditional call as mining node
        // at bin/dwowd/src/proto/mod.rs:137
        eprintln!("[dww] Connecting to seed nodes...");
        p2p.clone().seed().await;

        let peer_count = p2p.hosts().peers().len();
        let greylist_count = p2p.hosts().container.fetch_all(HostColor::Grey).len();
        eprintln!("[dww] Seed complete: peers={} greylist={}", peer_count, greylist_count);
        info!(target: "drk::wallet",
            "Seed complete: peer_count={}, greylist_count={}", peer_count, greylist_count);

        self.p2p = Some(p2p);
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
        // If P2P is not configured, fall back to chain.height > 0
        let Some(ref p2p) = self.p2p else {
            return local > 0;
        };
        // With P2P: need at least one peer
        if p2p.hosts().peers().is_empty() {
            return false;
        }
        let peer_tip = self.highest_peer_tip.get();
        if peer_tip > 0 {
            return local >= peer_tip;
        }
        // P2P connected but no peer tip yet — sync task hasn't queried tips.
        // Consider synced if we have peers and blocks (tip will arrive).
        local > 0
    }

    /// Insert a block synced from a P2P peer into the wallet's chain store.
    /// Insert a synced block into the local chain store with defense-in-depth
    /// verification: after insert, read back and verify the header hash matches.
    /// Detects torn writes and sled-level corruption before the block is trusted.
    pub fn insert_synced_block(&self, block: &dwow_chain::Block) -> Result<()> {
        let height = block.header.height;
        self.chain.insert_block(height, block)
            .map_err(|e| Error::Custom(format!("insert block {}: {}", height, e)))?;
        // Defense in depth: verify the write by reading it back.
        // Sled batch writes are atomic (all-or-nothing), but a torn page
        // from a crash would be caught here before the block is scanned.
        let stored = self.chain.get_block(height)
            .map_err(|e| Error::Custom(format!("verify block {} after insert: {}", height, e)))?;
        if stored.header.merkle_root != block.header.merkle_root {
            return Err(Error::Custom(format!(
                "Block {} height mismatch after insert — possible sled corruption", height
            )));
        }
        Ok(())
    }

    pub fn into_ptr(self) -> DwwPtr {
        Arc::new(RwLock::new(self))
    }

    /// Broadcast a transaction via P2P gossip.
    /// Serializes the tx and sends it to all connected peers.
    /// Returns the txid on success.
    ///
    /// When `confirm` is true, waits for the local chain to advance past
    /// the broadcast height, indicating the tx was included in a block.
    /// The wallet sync task polls peers every 10s and inserts new blocks
    /// into this wallet's LinearStore — we poll chain.get_height() until
    /// it advances or timeout is reached. No RPC — wallet is a full node.
    ///
    /// Matches SpecWallet.broadcast_tx() and _poll_for_confirmation()
    /// in contrib/model/wallet_model.py.
    pub async fn broadcast_tx(
        &self,
        tx: &dwow_core::tx::Transaction,
        output: &mut Vec<String>,
        confirm: bool,
        timeout_secs: Option<u64>,
        poll_interval_secs: Option<u64>,
    ) -> Result<String> {
        let p2p = self.p2p.as_ref()
            .ok_or_else(|| Error::Custom(
                "P2P not initialized. The daemon broadcasts automatically; from docker exec, pipe tx to 'broadcast' or run 'sync init' first.".into()
            ))?;

        // Record chain height before broadcast for confirmation polling
        let start_height = if confirm {
            self.chain.get_height().unwrap_or(0) as u32
        } else {
            0
        };

        // Verify at least one connected peer before broadcasting.
        let txid = tx.hash().to_string();
        let peer_count = p2p.hosts().peers().len();
        if peer_count == 0 {
            return Err(Error::Custom(
                "No P2P peers connected — cannot broadcast transaction. \
                 Run 'sync init' first or wait for peer connections.".into()
            ));
        }

        // Broadcast with retry: 3 attempts × 2s delay.
        // Transient P2P drops should not cause permanent tx loss.
        let mut broadcast_ok = false;
        for attempt in 1..=3 {
            p2p.broadcast(tx).await;
            // Verify peers still connected after broadcast
            if p2p.hosts().peers().len() > 0 {
                broadcast_ok = true;
                break;
            }
            if attempt < 3 {
                output.push(format!(
                    "Broadcast attempt {}/3: no peers after send, retrying...", attempt));
                smol::Timer::after(std::time::Duration::from_secs(2)).await;
            }
        }

        if !broadcast_ok {
            return Err(Error::Custom(
                "Transaction broadcast failed after 3 attempts — all peers disconnected. \
                 Transaction NOT stored as broadcasted. Retry when P2P reconnects.".into()
            ));
        }

        output.push(format!("Transaction broadcast (P2P, {} peers): {txid}", peer_count));

        // Store in history
        if let Err(e) = self.put_tx_history_record(tx, "Broadcasted", None) {
            output.push(format!("Warning: failed to record tx history: {e}"));
        }

        // Optional confirmation: wait for chain to advance via sync task
        if confirm {
            return self.poll_for_confirmation(
                &txid,
                start_height,
                timeout_secs.unwrap_or(30),
                poll_interval_secs.unwrap_or(5),
            ).await;
        }

        Ok(txid)
    }

    /// Wait for the local chain to advance past the broadcast height.
    /// The sync task polls peers every 10s and inserts new blocks into
    /// this wallet's LinearStore. We poll chain.get_height() until it
    /// exceeds start_height or timeout is reached.
    ///
    /// Matches SpecWallet._poll_for_confirmation() in wallet_model.py.
    async fn poll_for_confirmation(
        &self,
        txid: &str,
        start_height: u32,
        timeout_secs: u64,
        interval_secs: u64,
    ) -> Result<String> {
        let start = Instant::now();
        let timeout = Duration::from_secs(timeout_secs);
        let interval = Duration::from_secs(interval_secs);

        loop {
            smol::Timer::after(interval).await;
            let current_height = self.chain.get_height().unwrap_or(0) as u32;
            if current_height > start_height {
                return Ok(txid.to_string());
            }
            if start.elapsed() >= timeout {
                return Err(Error::Custom(format!(
                    "Transaction {} not confirmed after {}s (chain at height {})",
                    &txid[..8], timeout_secs, current_height
                )));
            }
        }
    }

    /// Initialize wallet with tables for `Dww`.
    pub fn initialize_wallet(&self) -> WalletDbResult<()> {
        // Initialize wallet schema
        self.wallet.exec_batch_sql(include_str!("../wallet.sql"))?;

        // Migration: add manifest_json column to existing contract_metadata tables.
        // Ignore error if column already exists (SQLite lacks IF NOT EXISTS for ALTER TABLE).
        let _ = self.wallet.exec_batch_sql(
            "ALTER TABLE contract_metadata ADD COLUMN manifest_json TEXT DEFAULT '';"
        );
        // Migration: add externally_revoked column for issuer-side revocation detection.
        let _ = self.wallet.exec_batch_sql(
            "ALTER TABLE held_capabilities ADD COLUMN externally_revoked INTEGER DEFAULT 0;"
        );

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
    /// Best-effort: each step logs errors and continues rather than
    /// aborting mid-reset, preventing partial/inconsistent DB state.
    pub fn reset(&self, output: &mut Vec<String>) -> WalletDbResult<()> {
        output.push(String::from("Resetting full wallet state"));
        if let Err(e) = self.reset_scanned_blocks(output) {
            output.push(format!("Warning: reset_scanned_blocks failed: {e}"));
        }
        if let Err(e) = self.reset_deploy_authorities(output) {
            output.push(format!("Warning: reset_deploy_authorities failed: {e}"));
        }
        if let Err(e) = self.reset_tx_history(output) {
            output.push(format!("Warning: reset_tx_history failed: {e}"));
        }
        output.push(String::from("Successfully reset full wallet state"));
        Ok(())
    }

    /// Get the Native Token capability Merkle tree from cache
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

    /// Get secrets for AEAD decryption from AccountManager — the SINGLE key authority.
    /// No longer reads from SQLite capability_secrets (dual-store anti-pattern).
    /// AccountManager is the canonical key store; scan reads directly from it.
    pub fn get_secrets(&self) -> Result<Vec<SecretKey>> {
        // Open AccountManager from the wallet's sled cache (transitional — will
        // move to SQLite backend in Step 5).
        let accounts_tree = self.cache.db.open_tree("accounts")
            .map_err(|e| Error::Custom(format!("sled open_tree: {e}")))?;
        let cached_json = accounts_tree.get("accounts_json")
            .map_err(|e| Error::Custom(format!("sled get: {e}")))?
            .map(|v| String::from_utf8(v.to_vec())
                .map_err(|e| Error::Custom(format!("utf8: {e}"))))
            .transpose()?;
        let mgr = dwow_accounts::AccountManager::open(
            cached_json.as_deref(),
            true,   // localnet
            None::<&std::path::Path>,
            dwow_sdk::crypto::keypair::Network::Testnet,
            Some("default"),
        ).map_err(|e| Error::Custom(format!("AccountManager::open: {e}")))?;

        let secrets = mgr.secrets();
        if secrets.is_empty() {
            tracing::error!(
                target: "drk::wallet",
                "get_secrets: AccountManager has ZERO secrets — AEAD decryption will FAIL. \
                 Run 'wallet keygen' or 'wallet import-from-toml <name>' to add a secret key."
            );
            return Err(Error::Custom(
                "No secrets in AccountManager — wallet cannot decrypt. \
                 Run 'wallet keygen' or 'wallet import-secrets'.".into()
            ));
        }

        tracing::info!(
            target: "drk::wallet",
            "get_secrets: loaded {} secret(s) from AccountManager", secrets.len(),
        );

        Ok(secrets)
    }

    /// Get held capabilities from wallet
    pub fn get_held_capabilities(&self, revoked: Option<bool>) -> Result<Vec<PromissoryNote>> {
        let cap_records = self.wallet.get_held_capabilities(revoked).map_err(|e| Error::Custom(format!("{:?}", e)))?;
        cap_records_to_pn_notes(&cap_records)
    }

    /// Get caps for a specific token
    pub fn get_capabilities_for_token(&self, token_id: &TokenId) -> Result<Vec<PromissoryNote>> {
        let token_id_str = token_id.to_string();
        let cap_records = self.wallet.get_capabilities_for_token(&token_id_str, Some(false)).map_err(|e| Error::Custom(format!("{:?}", e)))?;
        cap_records_to_pn_notes(&cap_records)
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
                    .map_err(|e| Error::Custom(format!("Invalid secret encoding: {}", e)))?
                    .as_slice().try_into()
                    .map_err(|_| Error::Custom("Invalid secret length".into()))?;
                let secret = SecretKey::from_bytes(secret_bytes)
                    .map_err(|_| Error::Custom("Failed to parse secret key".into()))?;
                let public = PublicKey::from_secret(secret);
                let std_addr = dwow_sdk::crypto::keypair::StandardAddress::from_public(self.network, public);
                Ok(std_addr.into())
            }
            None => Err(Error::Custom(
                "No addresses in wallet. Run 'wallet keygen' to create one.".into()
            )),
        }
    }

    /// Get all addresses
    pub fn addresses(&self) -> Result<Vec<(u64, PublicKey, SecretKey, u64)>> {
        let addrs = self.wallet.get_addresses()
            .map_err(|e| Error::Custom(format!("Database error: {:?}", e)))?;

        // Return empty vec if no addresses (no auto-keygen — RC3 fix)
        let mut result: Vec<(u64, PublicKey, SecretKey, u64)> = vec![];
        for a in addrs {
            let secret_bytes: [u8; 32] = bs58::decode(&a.secret)
                .into_vec()
                .map_err(|e| Error::Custom(format!("Invalid secret encoding: {}", e)))?
                .as_slice().try_into()
                .map_err(|_| Error::Custom("Invalid secret length".into()))?;
            let secret = SecretKey::from_bytes(secret_bytes)
                .map_err(|_| Error::Custom("Failed to parse secret key".into()))?;
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
                    .map_err(|e| Error::Custom(format!("Invalid secret encoding: {}", e)))?
                    .as_slice().try_into()
                    .map_err(|_| Error::Custom("Invalid secret length".into()))?;
                SecretKey::from_bytes(secret_bytes)
                    .map_err(|_| Error::Custom("Failed to parse secret key".into()))
            }
            None => Err(Error::Custom("No addresses in wallet".to_string())),
        }
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
            None => Err("No addresses in wallet. Run 'wallet keygen' to create one.".into()),
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

    /// Get the default secret from the addresses table (single key authority).
    fn get_secret(&self) -> std::result::Result<String, String> {
        let addresses = self.wallet.get_addresses()
            .map_err(|e| format!("{:?}", e))?;
        addresses.first()
            .map(|a| a.secret.clone())
            .ok_or_else(|| "No secrets in wallet".to_string())
    }
}

impl Dww {
    /// Attach fee to transaction
    ///
    /// Builds a NativeToken::FeeV1 call using the wallet's first DRKW cap
    /// and appends it as a root-level call in the transaction.
    ///
    /// TODO(arch): Extract ZK proof building to NativeTokenClient::build_fee()
    /// in the native_token contract crate. The wallet should call the client,
    /// not build FeeV1 proofs directly. Native Token is the sole special citizen
    /// per wallet.md — its ZK logic belongs in its contract crate like all others.
    pub async fn attach_fee(&self, tx: &mut Transaction, _fee: u64) -> Result<()> {
        use crate::contract_imports::native_token::{
            DRKW_TOKEN_ID, FeeCallBuilder, FeeCallInput, FeeCallOutput,
            NATIVE_TOKEN_CONTRACT_ZKAS_FEE_V1_BIN,
        };
        use dwow_core::zk::{proof::ProvingKey, vm::ZkCircuit, vm_heap::empty_witnesses};
        use dwow_sdk::crypto::{BaseBlind, MerkleNode};
        use dwow_serial::Encodable;
        use rand::rngs::OsRng;

        use crate::fee_builder::DEFAULT_FEE;

        // Get DRKW cap for fee
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
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
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

    /// Mark caps from a transaction as revoked in the wallet database.
    pub fn mark_tx_exercise(&self, tx: &Transaction, output: &mut Vec<String>) -> Result<()> {
        use dwow_sdk::contract_client::CapabilityInfo;

        // Get all unspent caps as held capabilities
        let unspent_caps = self.wallet.get_held_capabilities(Some(false))
            .map_err(|e| Error::Custom(format!("Failed to get capabilities: {:?}", e)))?;

        let held_capabilities: Vec<CapabilityInfo> = unspent_caps.iter()
            .map(|c| CapabilityInfo {
                capability_id: c.cap_id.clone(),
                secret: c.secret.clone(),
            })
            .collect();

        let client_registry = crate::contract_imports::get_client_registry();

        // For each call in the transaction, dispatch to the contract's client
        for call in &tx.calls {
            let contract_id = call.data.contract_id;

            // Look up the contract name from its ID for registry dispatch.
            // Try the wallet's contract_metadata table first (populated during
            // chain scan for ALL contracts — genesis and deployed). Falls back
            // to hardcoded genesis mapping for bootstrap before first scan.
            let contract_id_str = bs58::encode(contract_id.to_bytes()).into_string();
            let contract_name: Option<String> = self.wallet
                .get_contract_name_by_id(&contract_id_str)
                .ok()
                .flatten();

            let Some(name) = contract_name else { continue; };
            let Some(client) = client_registry.get(&name) else { continue; };

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

    /// Unspent promissory note caps after block height
    pub fn retained_pn_caps_after(
        &self,
        height: &u32,
        _output: &mut Vec<String>,
    ) -> WalletDbResult<Vec<PromissoryNote>> {
        let all_caps = self.wallet.get_held_capabilities(Some(false))?;

        let filtered: Vec<&CapRecord> = all_caps
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

    /// Remove promissory note caps after block height
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


    /// Generate a new capability root secret.
    ///
    /// Per ocap.md: the wallet is a capability browser, not an identity manager.
    /// The secret key is a capability root — it proves the holder can discover and
    /// exercise capabilities. It is NOT an "account" or "identity." The wallet
    /// never links a secret to a real-world person.
    ///
    /// For per-contract unlinkability, use `derive_contract_key()` after keygen
    /// to create cryptographically independent keys for each contract instance.
    pub fn keygen(&self, output: &mut Vec<String>) -> Result<Keypair> {
        use dwow_sdk::crypto::Keypair;
        use rand::rngs::OsRng;

        // Guard: wallet must be initialized before generating keys
        // Matches SpecWallet.keygen() auto-init check
        if let Err(e) = self.wallet.get_addresses() {
            return Err(Error::Custom(format!(
                "Wallet not initialized — run 'wallet initialize' first. DB error: {e}"
            )));
        }

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

        // Store in database. Addresses table is the single key authority —
        // capability_secrets table has been removed.
        self.wallet.insert_address(&public_str, &secret_str, is_default, 0)
            .map_err(|e| Error::Custom(format!("Failed to store address: {:?}", e)))?;

        output.push(format!("Generated new address: {}", &public_str[..16]));
        output.push(format!("Address (bs58): {public_str}"));
        // Only show first 8 chars of secret hex — full secret available via 'wallet secrets'
        output.push(format!("Secret (hex, first 8): {}...", &hex::encode(secret_bytes)[..8]));

        Ok(keypair)
    }

    /// Money balance
    pub fn capability_balance(&self) -> Result<HashMap<String, u64>> {
        let mut balances: HashMap<String, u64> = HashMap::new();

        // Get all unspent caps
        let cap_records = self.wallet.get_held_capabilities(Some(false)).map_err(|e| Error::Custom(format!("{:?}", e)))?;

        for record in cap_records {
            *balances.entry(record.token_id).or_insert(0) += record.value;
        }

        Ok(balances)
    }

    /// Import secrets from base58-encoded strings via AccountManager.
    /// This is the single import gate — AccountManager validates, persists to sled,
    /// then mirrors to SQLite. No key material is decoded outside AccountManager.
    pub fn import_secrets_base58(&self, b58_lines: &[String], output: &mut Vec<String>) -> Result<usize> {
        if b58_lines.is_empty() {
            return Err(Error::Custom("No secrets provided".into()));
        }

        // Open AccountManager. Loads from sled cache on restart, auto-generates
        // on first use.
        let accounts_tree = self.cache.db.open_tree("accounts")
            .map_err(|e| Error::Custom(format!("sled open_tree: {e}")))?;
        let cached_json = accounts_tree.get("accounts_json")
            .map_err(|e| Error::Custom(format!("sled get: {e}")))?
            .map(|v| String::from_utf8(v.to_vec())
                .map_err(|e| Error::Custom(format!("utf8: {e}"))))
            .transpose()?;
        let mut mgr = dwow_accounts::AccountManager::open(
            cached_json.as_deref(),
            true,   // localnet
            None::<&std::path::Path>,
            dwow_sdk::crypto::keypair::Network::Testnet,
            Some("default"),
        ).map_err(|e| Error::Custom(format!("AccountManager::open: {e}")))?;

        let mut count = 0usize;
        for line in b58_lines {
            let line = line.trim();
            if line.is_empty() { continue; }
            match mgr.import_base58(line) {
                Ok(idx) => {
                    output.push(format!("Imported secret at index {}", idx));
                    count += 1;
                }
                Err(e) => {
                    // Duplicate is not fatal — log and continue
                    if e.contains("already imported") {
                        output.push(format!("Skipped duplicate: {}", e));
                    } else {
                        return Err(Error::Custom(format!("import_base58: {e}")));
                    }
                }
            }
        }

        if count == 0 {
            return Err(Error::Custom("No valid secrets to import".into()));
        }

        // Persist AccountManager to sled (transitional — will move to SQLite)
        mgr.persist_to_sled(&self.cache.db)
            .map_err(|e| Error::Custom(format!("AccountManager persist: {e}")))?;

        // Mirror to wallet SQLite for scanning (transitional — scan will read
        // directly from AccountManager/SQLite after Step 5)
        let secrets = mgr.secrets();
        self.import_secrets(secrets, output)?;

        Ok(count)
    }

    /// Import promissory note secrets
    pub fn import_secrets(&self, secrets: Vec<SecretKey>, output: &mut Vec<String>) -> Result<Vec<SecretKey>> {
        // Guard: empty secrets list is an error
        // Matches SpecWallet.import_secrets() empty-list check
        if secrets.is_empty() {
            return Err(Error::Custom("no secrets provided".to_string()));
        }

        // Check if any addresses exist already (first import sets default)
        let addresses = self.wallet.get_addresses()
            .map_err(|e| Error::Custom(format!("Database error: {:?}", e)))?;
        let mut is_default = addresses.is_empty();

	// Build batch items for atomic import (RC2 fix)
	let mut items = Vec::with_capacity(secrets.len());
	for secret in &secrets {
	    let secret_bytes: [u8; 32] = secret.inner().to_repr();
	    let secret_str = bs58::encode(secret_bytes).into_string();
	    let public = dwow_sdk::crypto::PublicKey::from_secret(*secret);
	    let public_str = bs58::encode(public.to_bytes()).into_string();
	    items.push((public_str.clone(), secret_str.clone()));
	    output.push(format!("Imported secret: {}", &secret_str[..8]));
	}
	self.wallet.import_secrets_batch(&items, is_default)
	    .map_err(|e| Error::Custom(format!("Failed to import secrets: {:?}", e)))?;
        Ok(secrets)
    }

    /// Import secrets from a keys.toml file — delegates to shared AccountManager.
    ///
    /// Uses dwow_accounts::AccountManager::open() — same code path as mining nodes.
    /// Single implementation, single source of truth. Idempotent — safe on restart.
    pub fn import_from_keys_toml(
        &self,
        path: &std::path::Path,
        wallet_name: &str,
        output: &mut Vec<String>,
    ) -> Result<()> {
        if !path.exists() {
            output.push(format!("keys.toml not found at {} — skipping import", path.display()));
            return Ok(());
        }
        // Delegate to shared AccountManager — same resolution order as mining nodes.
        // Pass wallet_name as section_name so AccountManager looks up the correct
        // [wallet-N] section in keys.toml instead of defaulting to NODE_NAME env var.
        let accounts_tree = self.cache.db.open_tree("accounts")
            .map_err(|e| Error::Custom(format!("sled open_tree: {e}")))?;
        let cached_json = accounts_tree.get("accounts_json")
            .map_err(|e| Error::Custom(format!("sled get: {e}")))?
            .map(|v| String::from_utf8(v.to_vec())
                .map_err(|e| Error::Custom(format!("utf8: {e}"))))
            .transpose()?;
        let mgr = dwow_accounts::AccountManager::open(
            cached_json.as_deref(),
            true,              // localnet
            Some(path),        // keys.toml path
            dwow_sdk::crypto::keypair::Network::Testnet,
            Some(wallet_name), // section override — selects [wallet-N]
        ).map_err(|e| Error::Custom(format!("AccountManager::open: {e}")))?;

        // Import secrets into wallet SQLite cache for scanning
        let secrets = mgr.secrets();
        if secrets.is_empty() {
            output.push("No keys found in keys.toml — wallet will have zero secrets".into());
            return Ok(());
        }
        let imported = self.import_secrets(secrets, output)?;
        // Persist AccountManager to sled for restart
        let json = mgr.to_json_string()
            .map_err(|e| Error::Custom(format!("AccountManager to_json: {e}")))?;
        accounts_tree.insert("accounts_json", json.as_bytes())
            .map_err(|e| Error::Custom(format!("sled write: {e}")))?;
        accounts_tree.flush()
            .map_err(|e| Error::Custom(format!("sled flush: {e}")))?;
        output.push(format!(
            "Imported wallet key from keys.toml [{}] via AccountManager ({} secret(s))",
            wallet_name, imported.len()
        ));
        Ok(())
    }

    /// Derive a per-contract instance key from the master secret.
    ///
    /// Per ocap.md §7: the wallet uses `SecretKey::derive_instance` to create
    /// a unique, cryptographically unlinkable key for every contract instance.
    /// This prevents cross-contract linking — a secret used for Promissory Note
    /// cannot be linked to the same holder's Identity credential.
    ///
    /// Call this when a new contract is first encountered during scan.
    /// The derived key is added to `capability_secrets` for AEAD decryption.
    pub fn derive_contract_key(
        &self,
        master_secret: &SecretKey,
        contract_id: &ContractId,
        instance_nonce: u64,
    ) -> Result<SecretKey> {
        let instance_bytes = instance_nonce.to_le_bytes();
        let derived = master_secret.derive_instance(contract_id, &instance_bytes);
        let secret_bytes: [u8; 32] = derived.inner().to_repr();
        let secret_str = bs58::encode(secret_bytes).into_string();
        let public_key = dwow_sdk::crypto::PublicKey::from_secret(derived);
        let public_str = bs58::encode(public_key.to_bytes()).into_string();
        // Store in addresses table — single key authority (capability_secrets removed)
        self.wallet.insert_address(&public_str, &secret_str, false, 0)
            .map_err(|e| Error::Custom(format!("Failed to store derived key: {:?}", e)))?;
        Ok(derived)
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

    /// Reset deploy authorities
    pub fn reset_deploy_authorities(&self, _output: &mut Vec<String>) -> WalletDbResult<()> {
        self.wallet.remove_deploy_authorities()
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

    /// Retrieve a stored contract manifest from the wallet DB.
    /// Returns None if no manifest was stored for this contract.
    pub fn get_contract_manifest(
        &self,
        contract_id: &str,
    ) -> Result<Option<dwow_sdk::manifest::ContractManifest>> {
        self.wallet.get_contract_manifest(contract_id)
            .map_err(|e| Error::Custom(format!("DB error: {e:?}")))
    }

    /// Redeem a Promissory Note cap via RedeemV1 (0x01).
    ///
    /// Destroys the cap's monetary value and creates a zero-value receipt cap
    /// as cryptographic proof of redemption. The receipt is permanent, verifiable,
    /// and non-transferable.
    pub async fn redeem(
        &self,
        cap_id: String,
        spend_hook: Option<pallas::Base>,
    ) -> Result<Transaction> {
        // Look up cap in wallet
        let cap_records = self.wallet.get_held_capabilities(Some(false))
            .map_err(|e| Error::Custom(format!("Failed to get capabilities: {:?}", e)))?;
        let cap_record = cap_records.iter()
            .find(|c| c.cap_id == cap_id)
            .ok_or_else(|| Error::Custom(format!("Capability not found: {}", cap_id)))?;

        // Get secret for this cap
        let secret_bytes: [u8; 32] = bs58::decode(&cap_record.secret)
            .into_vec()
            .map_err(|e| Error::Custom(e.to_string()))?
            .try_into()
            .map_err(|_| Error::Custom("Invalid secret key length".to_string()))?;
        let secret = SecretKey::from_bytes(secret_bytes)
            .map_err(|_| Error::Custom("Failed to parse secret key".to_string()))?;

        // Get Merkle proof
        let merkle_proof = self.wallet.get_merkle_proof(&cap_record.cap_id)
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

        // Parse cap fields
        let coin_blind = crate::transfer::decode_bs58_field(&cap_record.cap_blind)?;
        let token_id = crate::transfer::decode_bs58_field(&cap_record.token_id)?;
        let spend_hook_in = match cap_record.spend_hook {
            Some(ref s) => crate::transfer::decode_bs58_field(s)?,
            None => pallas::Base::zero(),
        };
        let user_data_in = match cap_record.user_data {
            Some(ref s) => crate::transfer::decode_bs58_field(s)?,
            None => pallas::Base::zero(),
        };
        let spend_hook_out = spend_hook.unwrap_or(spend_hook_in);

        // Build RedeemV1 via PromissoryNoteClient — ZK knowledge in contract crate
        let input = crate::contract_imports::promissory_note::RedeemCallInput {
            value: cap_record.value,
            token_id,
            spend_hook: spend_hook_in,
            user_data: user_data_in,
            coin_blind,
            leaf_position: cap_record.leaf_position,
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
                input, output, pallas::Base::zero(), pallas::Base::zero(),
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
            &self.wallet, redeem_leaf, None, None,
        )
    }

    /// Burn Promissory Note caps via BurnV1 (0x03).
    ///
    /// Destroys caps and publishes nullifiers. If any input cap has a non-zero
    /// spend_hook, the PN contract will dispatch a callback to the target contract.
    pub async fn burn(
        &self,
        cap_ids: Vec<String>,
    ) -> Result<Transaction> {
        if cap_ids.is_empty() {
            return Err(Error::Custom("At least one cap ID is required for burn".to_string()));
        }

        let unspent_caps = self.wallet.get_held_capabilities(Some(false))
            .map_err(|e| Error::Custom(format!("Failed to get capabilities: {:?}", e)))?;

        let mut inputs: Vec<crate::contract_imports::promissory_note::BurnCallInput> = vec![];

        for cap_id in &cap_ids {
            let cap_record = unspent_caps.iter()
                .find(|c| &c.cap_id == cap_id)
                .ok_or_else(|| Error::Custom(format!("Capability not found: {}", cap_id)))?;

            let secret_bytes: [u8; 32] = bs58::decode(&cap_record.secret)
                .into_vec().map_err(|e| Error::Custom(e.to_string()))?
                .try_into().map_err(|_| Error::Custom("Invalid secret key length".to_string()))?;
            let secret = SecretKey::from_bytes(secret_bytes)
                .map_err(|_| Error::Custom("Failed to parse secret key".to_string()))?;

            let merkle_proof = self.wallet.get_merkle_proof(&cap_record.cap_id)
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

            let coin_blind = crate::transfer::decode_bs58_field(&cap_record.cap_blind)?;
            let token_id = crate::transfer::decode_bs58_field(&cap_record.token_id)?;
            let spend_hook = match cap_record.spend_hook {
                Some(ref s) => crate::transfer::decode_bs58_field(s)?,
                None => pallas::Base::zero(),
            };
            let user_data = match cap_record.user_data {
                Some(ref s) => crate::transfer::decode_bs58_field(s)?,
                None => pallas::Base::zero(),
            };

            inputs.push(crate::contract_imports::promissory_note::BurnCallInput {
                value: cap_record.value,
                token_id,
                spend_hook,
                user_data,
                coin_blind,
                leaf_position: cap_record.leaf_position,
                merkle_path,
                secret: secret.inner(),
                ephemeral_signature_secret: SecretKey::random(&mut OsRng).inner(),
                tx_commitment: pallas::Base::zero(),
                tx_nonce: pallas::Base::zero(),
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
            &self.wallet, burn_leaf, None, None,
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
            .ok_or_else(|| {
                // Manifest-driven fallback: try the stored manifest before giving up.
                // Contracts deployed with manifests but without hardcoded registry entries
                // can still be invoked through this path.
                // Future: fully replace CONTRACT_METADATA_REGISTRY with manifest lookup.
                Error::Custom(format!(
                    "Unknown contract: {}. If this contract was deployed with a manifest, \
                     it may not have a hardcoded registry entry yet. \
                     Manifest-driven invocation is planned.",
                    contract_id_or_name
                ))
            })?;

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
                return crate::fee_builder::build_fee_and_finalize_tx(&self.wallet, leaf, None, None);
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
        let tx = crate::fee_builder::build_fee_and_finalize_tx(&self.wallet, leaf, None, None)?;

        Ok(tx)
    }

    /// Print a full diagnostic report: P2P config, seed status, peer list,
    /// sync state, chain health, and recent errors. Used by `wallet diagnostic`
    /// CLI and the pipeline's wallet verification phase.
    pub fn diagnostic(&self, output: &mut Vec<String>) -> Result<()> {
        output.push("=== Wallet Diagnostic ===".into());
        output.push(format!("Network: {:?}", self.network));
        output.push(format!("Chain height: {}", self.chain.get_height().unwrap_or(0)));

        if let Some(ref p2p) = self.p2p {
            let peer_count = p2p.hosts().peers().len();
            output.push(format!("P2P: initialized ({} peers)", peer_count));
            output.push(format!("Highest peer tip: {}", self.highest_peer_tip.get()));
            output.push(format!("Is synced: {}", self.is_synced()));
        } else {
            output.push("P2P: NOT INITIALIZED".into());
        }

        if let Some(ref settings) = self.p2p_settings {
            output.push(format!("Localnet: {}", settings.localnet));
            output.push(format!("Connect timeout: {}s", settings.connect_timeout_secs));
        }

        Ok(())
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

