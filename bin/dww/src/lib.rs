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

use smol::lock::RwLock;
use tracing::info;

use dwow_core::{
    net::hosts::HostColor,
    tx::{ContractCallLeaf, Transaction},
    zk::Proof,
    zkas::ZkBinary,
};
use crate::wallet_error::{Error, Result};
use crate::wallet_util::expand_path;
use dwow_sdk::{
    crypto::{
        keypair::{Address, Network, PublicKey, SecretKey},
        pasta_prelude::PrimeField, ContractId, MerkleTree,
    },
    pasta::pallas,
    tx::ContractCall,
};
use crate::contract_imports::NATIVE_TOKEN_CONTRACT_ID;
// TokenId type alias REMOVED — pallas::Base is used inline.
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
    /// The wallet's declared identity, derived on boot from keys.toml (section).
    /// Single source of decryption/spend secrets — nothing persisted, no addresses
    /// table. Mirrors how dwowd resolves its mining identity.
    pub account_mgr: dwow_accounts::AccountManager,
    /// Chain block store — wallet's own synced blocks
    pub chain: dwow_chain::LinearStore,
    /// Blockchain cache database operations handler (Sled — SMT indices, scan progress)
    pub cache: Cache,
    /// Wallet database operations handler (SQLite — capabilities, contracts, scan state)
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
        keys_toml: Option<&std::path::Path>,
        section: &str,
        chain_path: String,
        _cache_path: String,  // TODO: remove — cache now uses SQLite, not sled
        wallet_path: String,
        wallet_pass: String,
        production_mode: bool,
        p2p_settings: Option<crate::p2p_wallet::P2pWalletConfig>,
    ) -> Result<Self> {
        // Resolve the wallet's declared identity deterministically, derive-on-boot.
        // keys.toml is required — the wallet must declare its key; it is never
        // generated or persisted. Section (WALLET_NAME) selects the identity.
        let keys_path = keys_toml.ok_or_else(|| Error::Custom(
            "no keys.toml provided (--keys or KEYS_FILE env): the wallet must declare its key".into()))?;
        let account_mgr = dwow_accounts::AccountManager::open(keys_path, network, section)
            .map_err(|e| Error::Custom(format!("AccountManager::open: {e}")))?;

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

        // Initialize wallet (SQLite — all wallet state lives here)
        let wallet_path = expand_path(&wallet_path)?;
        if !wallet_path.exists() {
            if let Some(parent) = wallet_path.parent() {
                create_dir_all(parent)?;
            }
        }
        let Ok(wallet) = WalletDb::new(Some(wallet_path.clone()), Some(&wallet_pass), production_mode) else {
            return Err(Error::DatabaseError(format!("{}", WalletDbError::InitializationFailed)));
        };

        // Open SQLite connection for Cache (scan state — merkle trees, SMT, scanned blocks).
        // Uses the same wallet.db file with the same SQLCipher key. WAL mode allows
        // concurrent connections to the same encrypted database.
        let cache_conn = rusqlite::Connection::open(&wallet_path)
            .map_err(|e| Error::DatabaseError(format!("cache sqlite open: {e}")))?;
        cache_conn.execute_batch(&format!("PRAGMA key = '{}';", wallet_pass))
            .map_err(|e| Error::DatabaseError(format!("cache pragma key: {e}")))?;
        cache_conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;")
            .map_err(|e| Error::DatabaseError(format!("cache pragma: {e}")))?;
        let cache = Cache::new(std::sync::Arc::new(std::sync::Mutex::new(cache_conn)));

        Ok(Self { network, account_mgr, chain, cache, wallet, p2p: None, executor: None, p2p_settings, highest_peer_tip: Arc::new(crate::sync_task::HighestPeerTip::new()), verified_anchor_height: smol::lock::Mutex::new(0) })
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
        info!(target: "dww::wallet",
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
    /// Polls for tx confirmation by re-scanning and checking cap state.
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
        // The externally_revoked column was removed (never populated or read).

        // Register default DRKW native token alias so `transfer 1.0 DRKW <addr>` works
        // The tokens table is gone — a token's identity is discovered via scan,
        // not declared at init (capabilities carry their own token_id).
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
        // reset_deploy_authorities call-site REMOVED — the table is never
        // populated (insert/get dead); resetting an empty table is a no-op.
        // reset_tx_history call-site REMOVED — the transactions_history table is
        // populated (broadcast + scan writers) but never read; its reset is a no-op
        // on the current coinbase→balance→transfer path.
        output.push(String::from("Successfully reset full wallet state"));
        Ok(())
    }

    /// Get the capability commitment tree from cache.
    /// Stores H(w, params) for all capabilities — per ocap.md:238.
    pub fn get_capability_commitment_tree(&self) -> Result<MerkleTree> {
        match self.cache.get_merkle_tree(b"capability_commitment_tree") {
            Some(tree) => Ok(tree),
            None => {
                // Create an empty Merkle tree for darkwow-devnet (no previous state)
                let tree = MerkleTree::new(1);
                Ok(tree)
            }
        }
    }

    /// Get secrets for AEAD decryption — derived on boot from the wallet's declared
    /// identity (keys.toml section), the single source of truth. No addresses table,
    /// no persistence, no auto-generation. `AccountManager::open` (at construction)
    /// already hard-fails on a missing declaration, so this is non-empty in practice.
    pub fn get_secrets(&self) -> Result<Vec<SecretKey>> {
        let secrets = self.account_mgr.secrets();
        if secrets.is_empty() {
            tracing::warn!(
                target: "dww::wallet",
                "get_secrets: declared identity resolved to zero secrets — check keys.toml section",
            );
        } else {
            tracing::info!(
                target: "dww::wallet",
                "get_secrets: {} secret(s) derived from declared identity", secrets.len(),
            );
        }
        Ok(secrets)
    }

    /// AEAD self-test: encrypt a known test vector with the wallet's own key,
    /// then decrypt and verify roundtrip. Runs at daemon startup to prove the
    /// AEAD implementation in this binary works BEFORE touching the network.
    /// If this fails, the binary's crypto is broken independent of chain state.
    pub fn aead_self_test(&self) -> Result<()> {
        let secrets = self.get_secrets()?;
        if secrets.is_empty() {
            // No keys imported yet — self-test skipped. This is not an error;
            // the wallet can operate without keys (scan will find nothing).
            tracing::info!(
                target: "dww::wallet",
                "AEAD self-test skipped — no keys in wallet",
            );
            return Ok(());
        }
        let secret = &secrets[0];
        let public = dwow_sdk::crypto::keypair::PublicKey::from_secret(*secret);
        let test_plaintext: Vec<u8> =
            b"DarkWow AEAD pipeline self-test vector 2026".to_vec();

        use dwow_sdk::crypto::note::AeadEncryptedNote;
        use rand::rngs::OsRng;

        let encrypted = AeadEncryptedNote::encrypt(
            &test_plaintext, &public, &mut OsRng,
        ).map_err(|e| Error::Custom(format!(
            "AEAD self-test encrypt failed: {:?}", e
        )))?;

        let decrypted: Vec<u8> = encrypted.decrypt(secret)
            .map_err(|e| Error::Custom(format!(
                "AEAD self-test decrypt failed: {:?}", e
            )))?;

        if decrypted == test_plaintext {
            tracing::info!(
                target: "dww::wallet",
                "AEAD self-test PASSED ({} byte roundtrip)", test_plaintext.len(),
            );
            Ok(())
        } else {
            Err(Error::Custom(format!(
                "AEAD self-test FAILED: plaintext mismatch \
                (expected {} bytes, got {} bytes)",
                test_plaintext.len(), decrypted.len(),
            )))
        }
    }

    /// Get held capabilities from wallet.
    /// Returns CapRecords directly — no per-contract conversion.
    pub fn get_held_capabilities(&self, revoked: Option<bool>) -> Result<Vec<CapRecord>> {
        self.wallet.get_held_capabilities(revoked).map_err(|e| Error::Custom(format!("{:?}", e)))
    }

    // get_capabilities_for_token REMOVED — dead code. Fee builder calls
    // wallet.get_capabilities_for_token() directly (walletdb returns CapRecords).

    // get_token REMOVED — dead code. Only called by parse_token_pair
    // (also removed — zero callers). Token resolution is handled
    // generically through manifests and CapRecord.token_id.

    /// Get aliases mapped by token
    pub fn get_aliases_mapped_by_token(&self) -> Result<HashMap<String, String>> {
        // The aliases table is gone (never populated); returns empty.
        Ok(HashMap::new())
    }

    /// Get default address — derived from the declared identity.
    pub fn default_address(&self) -> Result<Address> {
        let public = self.account_mgr.default_public_key()
            .map_err(|e| Error::Custom(format!("AccountManager: {e}")))?;
        let std_addr = dwow_sdk::crypto::keypair::StandardAddress::from_public(self.network, public);
        Ok(std_addr.into())
    }

    /// Get all addresses — derived from the declared identity (no stored table).
    pub fn addresses(&self) -> Result<Vec<(u64, PublicKey, SecretKey, u64)>> {
        let secrets = self.account_mgr.secrets();
        let mut result: Vec<(u64, PublicKey, SecretKey, u64)> = vec![];
        for (i, secret) in secrets.into_iter().enumerate() {
            let public = PublicKey::from_secret(secret);
            result.push((i as u64, public, secret, 0));
        }
        Ok(result)
    }

    // default_secret REMOVED — callerless (spend paths use CapRecord.secret).

}

// ============================================================================
// WalletStateProvider impl — provides wallet state to ContractClient::build()
// ============================================================================

use dwow_sdk::contract_client::{CapInfo, MerkleProofInfo, WalletStateProvider};

impl WalletStateProvider for Dww {
    fn default_address(&self) -> std::result::Result<String, String> {
        // Raw bs58 of the declared identity's public key (no stored table).
        let public = self.account_mgr.default_public_key()?;
        Ok(bs58::encode(public.to_bytes()).into_string())
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

    /// Get the default secret, derived from the declared identity.
    fn get_secret(&self) -> std::result::Result<String, String> {
        // Raw bs58 of the declared identity's secret (no stored table).
        let secret = self.account_mgr.secrets().into_iter().next()
            .ok_or_else(|| "Declared identity resolved to zero secrets".to_string())?;
        Ok(bs58::encode(secret.inner().to_repr()).into_string())
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
        let fee_cap_records = self.wallet.get_capabilities_for_token(&dark_token_id_str, Some(false))
            .map_err(|e| Error::Custom(format!("Failed to get DRKW capabilities: {:?}", e)))?;

        if fee_cap_records.is_empty() {
            return Err(Error::Custom(
                "No DRKW capabilities available for fee payment.".to_string(),
            ));
        }

        let fee_cap = &fee_cap_records[0];
        let dark_secret_bytes = bs58::decode(&fee_cap.secret)
            .into_vec()
            .map_err(|e| Error::Custom(e.to_string()))?
            .try_into()
            .map_err(|_| Error::Custom("Invalid DRKW secret key length".to_string()))?;
        let dark_secret = SecretKey::from_bytes(dark_secret_bytes)
            .map_err(|_| Error::Custom("Failed to parse DRKW secret key".to_string()))?;

        let dark_merkle_proof = self.wallet.get_merkle_proof(&fee_cap.cap_id)
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

        let dark_coin_blind_bytes = bs58::decode(&fee_cap.cap_blind)
            .into_vec()
            .map_err(|e| Error::Custom(e.to_string()))?
            .try_into()
            .map_err(|_| Error::Custom("Invalid capability blind length".to_string()))?;
        let fee_cap_blind = pallas::Base::from_repr(dark_coin_blind_bytes)
            .into_option()
            .ok_or_else(|| Error::Custom("Invalid capability blind".to_string()))?;

        // Load fee ZK binary and build fee proof
        let fee_zkbin = ZkBinary::decode(NATIVE_TOKEN_CONTRACT_ZKAS_FEE_V1_BIN, false)
            .map_err(|e| Error::Custom(format!("Failed to decode fee ZK binary: {:?}", e)))?;

        let fee_empty_wits = empty_witnesses(&fee_zkbin)?;
        let fee_circuit = ZkCircuit::new(fee_empty_wits, &fee_zkbin);
        let fee_pk = ProvingKey::build(fee_zkbin.k, &fee_circuit)
            .map_err(|e| Error::Custom(format!("ProvingKey::build fee: {:?}", e)))?;

        let fee_input = FeeCallInput {
            value: fee_cap.value,
            token_id: DRKW_TOKEN_ID,
            spend_hook: pallas::Base::zero(),
            user_data: pallas::Base::zero(),
            coin_blind: fee_cap_blind,
            leaf_position: fee_cap.leaf_position,
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
            value: fee_cap.value.saturating_sub(DEFAULT_FEE),
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
                // Use current chain tip as the revoke height; reorg reconciler
                // will un-revoke if the block is reverted.
                let current_height = self.chain_height().unwrap_or(0) as u32;
                if let Err(e) = self.wallet.mark_revoked(capability_id, current_height) {
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
    pub fn is_native_token_fee(&self, call: &ContractCall) -> bool {
        call.contract_id == *NATIVE_TOKEN_CONTRACT_ID &&
            call.data.first() == Some(&0x00) // FeeV1 function code
    }


    // `keygen` REMOVED — the wallet no longer generates or stores random identity
    // keys. Its identity is declared in keys.toml and derived on boot via
    // `AccountManager` (see `account_mgr`). Key generation is an owner-run, offline
    // act (`dwowd --genkey`), never a wallet runtime path.

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

    // `import_secrets_base58` / `import_secrets` / `import_from_keys_toml` /
    // `derive_contract_key` REMOVED — the wallet no longer stores secrets in an
    // addresses table. Its identity is declared in keys.toml and derived on boot
    // via `AccountManager` (`account_mgr`); `get_secrets()` reads from it.
    // Per-contract derivation, when needed, uses `SecretKey::derive_instance`
    // on the declared identity (re-derived, never stored).

    // get_aliases REMOVED — dead code (zero callers).
    // add_alias REMOVED — dead code (zero callers).
    // get_aliases_mapped_by_token (above) is the live alias API.

    /// Reset deploy authorities
    pub fn reset_deploy_authorities(&self, _output: &mut Vec<String>) -> WalletDbResult<()> {
        self.wallet.remove_deploy_authorities()
    }

    // get_mint_authority_for_token REMOVED — dead code (zero callers).
    // get_mint_authorities REMOVED — dead code (zero callers).
    // Mint authority queries are handled generically through manifests.

    /// Retrieve a stored contract manifest from the wallet DB.
    /// Returns None if no manifest was stored for this contract.
    pub fn get_contract_manifest(
        &self,
        contract_id: &str,
    ) -> Result<Option<dwow_sdk::manifest::ContractManifest>> {
        self.wallet.get_contract_manifest(contract_id)
            .map_err(|e| Error::Custom(format!("DB error: {e:?}")))
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

// cap_records_to_pn_notes REMOVED — CapRecord is the canonical type.
// Per ocap.md §Capability Grammar: no per-contract wrapper types.

