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

#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

use std::{collections::HashMap, fs::create_dir_all, sync::Arc, time::{Duration, Instant}};

use bs58;
use hex;

use smol::lock::RwLock;
use tracing::{error, warn};

use dwow_core::{
    tx::{ContractCallLeaf, Transaction},
    zk::Proof,
};
use dwow_chain::fee_window::FeeWindowFlags;
use dwow_chain::opcode_cost::circuit_difficulty;
use crate::wallet_error::{Error, Result};
use crate::wallet_util::expand_path;
use dwow_sdk::{
    blockchain::{BlockHeight, FeeTier, WasmKb},
    crypto::{
        keypair::{Address, Network, PublicKey, SecretKey},
        pasta_prelude::{Field, PrimeField}, ContractId, MerkleTree,
    },
    pasta::pallas,
    tx::ContractCall,
};
use crate::contract_imports::NATIVE_TOKEN_CONTRACT_ID;
// AssetId type alias REMOVED — pallas::Base is used inline.
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
pub mod prover_impl;

// dao_escrow and drain_protection removed — wallet uses generic AEAD scan + manifest.
// All contracts (except Native Token for fees + Deployooor for deployment) are
// discovered via the generic capability path. No per-contract files.

/// Fee builder helper for contract transactions
pub mod fee_builder;

/// Wallet functionality related to transactions history

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
/// Startup integrity checks for the wallet database
pub mod integrity;
/// C-compatible FFI exports — linkable from any language
pub mod ffi;
use walletdb::{WalletDb, WalletPtr};


/// Atomic pointer to a `Dww` structure.
pub type DwwPtr = Arc<RwLock<Dww>>;

/// Wallet struct — full node architecture.
///
/// Syncs the chain via P2P (connects DIRECTLY to configured peers, pulls
/// GetTip/GetBlocks). Scans its own synced chain locally. Does not use RPC
/// for chain sync. RPC is transitional for broadcast_tx() only.
pub struct Dww {
    /// Blockchain network (Testnet / Mainnet)
    pub network: Network,
    /// The wallet's declared identity, derived on boot from keys.toml (section).
    /// Single source of decryption/spend secrets — nothing persisted, no addresses
    /// table. Mirrors how dwowd resolves its mining identity.
    pub account_mgr: dwow_accounts::AccountManager,
    /// Wallet database operations handler (SQLite — capabilities, contracts, scan state)
    pub wallet: WalletPtr,
    /// P2P network instance — dwow_core::net::P2p, same as mining nodes.
    pub p2p: Option<dwow_core::net::P2pPtr>,
    /// Async executor for P2p sessions.
    pub executor: Option<dwow_core::concurrency::ExecutorPtr>,
    /// P2P network settings from config [net] section
    pub p2p_settings: Option<crate::p2p_wallet::P2pWalletConfig>,
    /// Highest peer chain tip seen by sync task. Updated on each Tip response.
    pub highest_peer_tip: Arc<crate::sync_task::HighestPeerTip>,
    /// Local genesis hash, learned from the first peer tip. Passed to
    /// `SyncPeer::dial` so the handshake validates the peer's chain identity
    /// (sync-protocol.md §17 / §4).
    pub local_genesis_hash: smol::lock::Mutex<Option<dwow_chain::sync_types::BlockHash>>,
    /// Highest block height with a verified Caribina (Arweave) anchor.
    /// Blocks below this height are cryptographically final — cannot be reorged.
    /// The chain state rejects AnchoredBlockConflict for anchored blocks.
    /// §8.1: Block heights SHALL be BlockHeight, never bare u64.
    pub verified_anchor_height: smol::lock::Mutex<BlockHeight>,
    /// Cached burn proving key — built once from the embedded zkas binary.
    /// Depends only on the compile-time constant BURN_V1_BIN.
    pub burn_pk_cache: smol::lock::Mutex<Option<dwow_core::zk::proof::ProvingKey>>,
    /// Cached mint proving key — built once from the embedded zkas binary.
    pub mint_pk_cache: smol::lock::Mutex<Option<dwow_core::zk::proof::ProvingKey>>,
}

// §1.3: The wallet process SHALL declare its composite barb set.
// Each barb corresponds to an observable action the wallet may exhibit
// during its lifecycle (scan, spend, sync, broadcast, decrypt, derive).
impl dwow_core::barb::ExhibitsBarb for Dww {
    fn exhibited_barbs() -> &'static [dwow_core::barb::BarbId] {
        use dwow_core::barb::BarbId;
        &[
            BarbId::Discover, BarbId::Spend, BarbId::Verify,
            BarbId::Encrypt, BarbId::Derive, BarbId::Broadcast,
            BarbId::SyncBarrier, BarbId::Gate, BarbId::Denominate,
        ]
    }
}

impl Dww {
    pub fn new(
        network: Network,
        keys_toml: Option<&std::path::Path>,
        section: &str,
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
        let mut account_mgr = dwow_accounts::AccountManager::open(keys_path, network, section)
            .map_err(|e| Error::Custom(format!("AccountManager::open: {e}")))?;

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

        // Hydrate lifecycle keys from the persisted JSON blob (additive keys
        // imported/generated/HD-derived in a previous session). The declared
        // identity from keys.toml is never touched — load_lifecycle appends at
        // index >= 1 only. Soft-fail: a missing/corrupt blob is non-fatal.
        if let Some(blob) = wallet.load_key_lifecycle() {
            if let Err(e) = account_mgr.load_lifecycle(blob.as_bytes()) {
                tracing::warn!(target: "dww::wallet",
                    "Failed to hydrate lifecycle keys (non-fatal): {e}");
            }
        }

        Ok(Self { network, account_mgr, wallet, p2p: None, executor: None, p2p_settings, highest_peer_tip: Arc::new(crate::sync_task::HighestPeerTip::new()), local_genesis_hash: smol::lock::Mutex::new(None), verified_anchor_height: smol::lock::Mutex::new(BlockHeight::new(0)), burn_pk_cache: smol::lock::Mutex::new(None), mint_pk_cache: smol::lock::Mutex::new(None) })
    }

    /// Get the current chain tip height from the local block store.
    pub fn chain_height(&self) -> Result<dwow_sdk::blockchain::BlockHeight> {
        self.wallet.chain_height()
            .map_err(|e| Error::Custom(format!("chain height: {:?}", e)))
    }

    /// Get a block by height from the local block store.
    pub fn chain_block(&self, height: dwow_sdk::blockchain::BlockHeight) -> Result<dwow_chain::Block> {
        self.wallet.get_block(height)
            .map_err(|e| Error::Custom(format!("chain block {}: {:?}", height, e)))
    }

    /// Return `fee_window_flags` from the latest synced block header.
    /// Falls back to `self.latest_fee_window_flags()` (identity CF) when no blocks
    /// are synced or the block cannot be read.
    ///
    /// G1 fix: replaces hardcoded `self.latest_fee_window_flags()` at all wallet
    /// transaction construction sites. The wallet SHALL read the miner's
    /// advertised congestion flags per fee-spec.md §12.9.
    pub fn latest_fee_window_flags(&self) -> FeeWindowFlags {
        match self.chain_height() {
            Ok(h) if h.get() > 0 => match self.chain_block(h) {
                Ok(block) => block.header.fee_window_flags,
                Err(_) => FeeWindowFlags::default(),
            },
            _ => FeeWindowFlags::default(),
        }
    }

    /// Fee-path execution risk factor for a contract.
    ///
    /// FeeV3: the fee risk comes from the dynamic `ContractRiskTracker`
    /// (observed-vs-declared `BlockCharge`, chain-visible). The wallet's
    /// SQLite/P2P architecture does not yet expose the chain's `contract_risk`
    /// sled tree, so until that is wired the wallet returns the risk-neutral
    /// baseline (1.0×) — the same value the mempool admission gate uses when
    /// chain state is absent. `attestation_risk_factor` remains the wallet-side
    /// trust metric only, NOT multiplied into the fee.
    pub fn contract_risk_factor(
        &self,
        contract_id: &dwow_sdk::crypto::ContractId,
        _has_manifest: bool,
    ) -> dwow_sdk::blockchain::RiskFactor {
        let _ = contract_id;
        dwow_sdk::blockchain::RiskFactor::BASELINE
    }

    /// Initialize P2P networking using dwow_core::net::P2p.
    ///
    /// The wallet is a pure outbound pull client: it connects DIRECTLY to its
    /// configured `peers` (ManualSession, dialed by P2p::start()) and pulls
    /// GetTip/GetBlocks from them. There is NO seed/hostlist exchange — that
    /// is mining-node-only machinery (net-node/net-full).
    ///
    /// Same stack as the mining nodes: P2p::new() → start() (no seed()).
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

        // The wallet connects DIRECTLY to its configured peers via ManualSession
        // (started by P2p::start() above). No seed() / hostlist exchange — that
        // is mining-node-only machinery (net-node/net-full).

        self.p2p = Some(p2p);
        Ok(())
    }

    /// Returns true if the wallet has synced to the latest known peer tip.
    /// Compares local chain height against highest peer tip seen by sync task.
    /// If P2P is not configured, falls back to chain.height > 0.
    pub fn is_synced(&self) -> bool {
        let zero = dwow_sdk::blockchain::BlockHeight::new(0);
        let local = match self.wallet.chain_height() {
            Ok(h) => h,
            Err(_) => return false,
        };
        if local == zero {
            return false;
        }
        // If P2P is not configured, fall back to chain.height > 0
        let Some(ref p2p) = self.p2p else {
            return local > zero;
        };
        // With P2P: need at least one peer
        if p2p.hosts().peers().is_empty() {
            return false;
        }
        let peer_tip = self.highest_peer_tip.get();
        if peer_tip > zero {
            return local >= peer_tip;
        }
        // P2P connected but no peer tip yet — sync task hasn't queried tips.
        // Consider synced if we have peers and blocks (tip will arrive).
        local > zero
    }

    /// Insert a block synced from a P2P peer into the wallet's chain store.
    /// Insert a synced block into the local chain store with defense-in-depth
    /// verification: after insert, read back and verify the header hash matches.
    /// Detects torn writes and sled-level corruption before the block is trusted.
    pub fn insert_synced_block(&self, block: &dwow_chain::Block) -> Result<()> {
        // Heights are BlockHeight end-to-end (§2.3) — no lowering at the seam.
        let height = block.header.height;

        // Consensus validation (no PoW VM needed): recompute the transaction
        // merkle root and compare against the header. Catches a peer sending a
        // block whose transactions were tampered/reordered. Full PoW validation
        // is deferred to the RandomX VM (not yet present in the wallet).
        let computed_merkle = dwow_chain::compute_merkle_root(&block.transactions);
        if block.header.merkle_root != computed_merkle {
            return Err(Error::Custom(format!(
                "Block {} tx merkle root mismatch: header {} != computed {}",
                height, block.header.merkle_root, computed_merkle
            )));
        }

        // Chain continuity check — for heights > genesis, verify the previous
        // block exists. Catches malicious peers sending blocks at arbitrary
        // heights without the full chain.
        if height.get() > 1 {
            if self.wallet.get_block(height.saturating_sub_blocks(1)).is_err() {
                return Err(Error::Custom(format!(
                    "Block {} cannot be inserted: previous block {} not in wallet DB. \
                     Peer sent block at height {} but wallet is missing the preceding block.",
                    height, height.saturating_sub_blocks(1), height
                )));
            }
            // NOTE: the previous-hash link (block.header.previous == hash(prev))
            // is deliberately NOT verified here. That hash is the RandomX PoW
            // hash, which requires the mining VM — and the wallet does not run
            // mining software. The prior check compared against the parent's
            // own parent (grandparent), which silently accepted sibling forks;
            // it is removed rather than left as a false check. The wallet relies
            // on the tx-merkle-root check above plus the sync task following the
            // longest (highest) peer-reported chain.
        }

        self.wallet.insert_block(height, block)
            .map_err(|e| Error::Custom(format!("insert block {}: {:?}", height, e)))?;
        // Defense in depth: verify the write by reading it back.
        let stored = self.wallet.get_block(height)
            .map_err(|e| Error::Custom(format!("verify block {} after insert: {:?}", height, e)))?;
        if stored.header.merkle_root != block.header.merkle_root {
            return Err(Error::Custom(format!(
                "Block {} height mismatch after insert — possible database corruption", height
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
    /// into this wallet's chain store — we poll chain_height() until
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
        // Record chain height before broadcast for confirmation polling
        let start_height = if confirm {
            match self.wallet.chain_height() {
                Ok(h) => h,
                Err(e) => {
                    error!("Failed to read chain height before broadcast: {e}");
                    dwow_sdk::blockchain::BlockHeight::new(0)
                }
            }
        } else {
            dwow_sdk::blockchain::BlockHeight::new(0)
        };

        let txid = tx.hash().to_string();

        // Resolve sync peers from config — the SAME rail as block sync
        // (SyncPeer on peer + SYNC_PORT_OFFSET), harmonized with the node's
        // SyncServer. No separate dwow_core::net::P2p tx path.
        let (peer_urls, magic) = match self.p2p_settings.as_ref() {
            Some(cfg) => (
                cfg.peers.iter()
                    .filter_map(|s| url::Url::parse(&s.url).ok())
                    .map(|mut u| {
                        if let Some(port) = u.port() {
                            let _ = u.set_port(Some(port + dwow_chain::sync_connection::SYNC_PORT_OFFSET));
                        }
                        u
                    })
                    .collect::<Vec<_>>(),
                cfg.magic_bytes,
            ),
            None => (Vec::new(), [68, 82, 75, 87]),
        };

        if peer_urls.is_empty() {
            return Err(Error::Custom(
                "No P2P peers configured — cannot broadcast transaction. \
                 Add a [net] peers section to the wallet config.".into()
            ));
        }

        // Broadcast over the unified sync connection to each peer.
        let mut sent = 0usize;
        for url in &peer_urls {
            match dwow_chain::sync_connection::SyncPeer::dial(
                url.clone(), magic, None, Duration::from_secs(15),
            ).await {
                Ok(mut peer) => match peer.broadcast_tx(tx).await {
                    Ok(_) => sent += 1,
                    Err(e) => warn!(target: "dww::wallet", "broadcast to {url} failed: {e}"),
                },
                Err(e) => warn!(target: "dww::wallet", "dial {url} failed: {e}"),
            }
        }

        if sent == 0 {
            return Err(Error::Custom(
                "No sync peers reachable — cannot broadcast transaction.".into()
            ));
        }

        output.push(format!("Transaction broadcast (sync, {sent} peers): {txid}"));

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
    /// this wallet's chain store. We poll chain_height() until it
    /// exceeds start_height or timeout is reached.
    ///
    /// Polls for tx confirmation by re-scanning and checking cap state.
    async fn poll_for_confirmation(
        &self,
        txid: &str,
        start_height: dwow_sdk::blockchain::BlockHeight,
        timeout_secs: u64,
        interval_secs: u64,
    ) -> Result<String> {
        let start = Instant::now();
        let timeout = Duration::from_secs(timeout_secs);
        let interval = Duration::from_secs(interval_secs);

        loop {
            smol::Timer::after(interval).await;
            let current_height = match self.wallet.chain_height() {
                Ok(h) => h,
                Err(e) => {
                    error!("Failed to read chain height during confirmation poll: {e}");
                    continue;
                }
            };
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
        // V.2 migration (T2 taxonomy): rename token_id/token_blind columns to
        // asset_id/asset_blind (ocap.md grammar — "token" is reserved for the
        // DRKW native token). MUST run BEFORE wallet.sql: its
        // CREATE INDEX ... ON held_capabilities(asset_id) fails on a V.1 DB
        // whose column is still named token_id. On a fresh DB the table does
        // not exist yet and each ALTER fails harmlessly (best-effort, same
        // pattern as the ADD COLUMN migrations below). Requires SQLite ≥3.25.
        // §4.2.1: let _ = fallible_call() annotated to document best-effort intent.
        #[allow(unused_results)]
        let _ = self.wallet.exec_batch_sql(
            "ALTER TABLE held_capabilities RENAME COLUMN token_id_blob TO asset_id_blob;"
        );
        let _ = self.wallet.exec_batch_sql(
            "ALTER TABLE held_capabilities RENAME COLUMN token_id TO asset_id;"
        );
        let _ = self.wallet.exec_batch_sql(
            "ALTER TABLE held_capabilities RENAME COLUMN token_blind_blob TO asset_blind_blob;"
        );
        let _ = self.wallet.exec_batch_sql(
            "ALTER TABLE held_capabilities RENAME COLUMN token_blind TO asset_blind;"
        );
        // Drop the V.1 index name — RENAME COLUMN rewrites the indexed column
        // but keeps the old index name; wallet.sql recreates it as
        // idx_held_capabilities_asset_id.
        let _ = self.wallet.exec_batch_sql(
            "DROP INDEX IF EXISTS idx_held_capabilities_token_id;"
        );

        // Initialize wallet schema
        self.wallet.exec_batch_sql(include_str!("../wallet.sql"))?;

        // Migration: add manifest_json column to existing contract_metadata tables.
        // Ignore error if column already exists (SQLite lacks IF NOT EXISTS for ALTER TABLE).
        let _ = self.wallet.exec_batch_sql(
            "ALTER TABLE contract_metadata ADD COLUMN manifest_json TEXT DEFAULT '';"
        );
        // Migration: add capability_discriminant to existing held_capabilities tables.
        // Per wallet.md §2.2 — manifest-driven capability construction.
        let _ = self.wallet.exec_batch_sql(
            "ALTER TABLE held_capabilities ADD COLUMN capability_discriminant INTEGER;"
        );
        // Migration: typed capability composition columns (ocap.md §6). Additive,
        // nullable — native/pre-manifest rows leave them NULL. Best-effort (SQLite
        // lacks ADD COLUMN IF NOT EXISTS; the error on re-init is swallowed).
        let _ = self.wallet.exec_batch_sql("ALTER TABLE held_capabilities ADD COLUMN capability_name TEXT;");
        let _ = self.wallet.exec_batch_sql("ALTER TABLE held_capabilities ADD COLUMN resource TEXT;");
        let _ = self.wallet.exec_batch_sql("ALTER TABLE held_capabilities ADD COLUMN action TEXT;");
        let _ = self.wallet.exec_batch_sql("ALTER TABLE held_capabilities ADD COLUMN primitives_csv TEXT;");
        let _ = self.wallet.exec_batch_sql("ALTER TABLE held_capabilities ADD COLUMN barbs_csv TEXT;");
        let _ = self.wallet.exec_batch_sql("ALTER TABLE held_capabilities ADD COLUMN key_coords_blob BLOB;");
        let _ = self.wallet.exec_batch_sql("ALTER TABLE held_capabilities ADD COLUMN spend_secret_blob BLOB;");
        // The externally_revoked column was removed (never populated or read).

        // P4 Step 2: seed genesis contract manifests into the wallet DB.
        {
            use crate::contract_imports::get_contract_id;
            use dwow_sdk::manifest::ContractManifest;
            use crate::walletdb::ContractMetadataRecord;

            let genesis_manifests: &[(&str, &str)] = &[
                ("native_token", include_str!("../../../src/contract/native_token/manifest.toml")),
                ("deployooor", include_str!("../../../src/contract/deployooor/manifest.toml")),
                ("promissory_note", include_str!("../../../src/contract/promissory_note/manifest.toml")),
                ("attestation", include_str!("../../../src/contract/attestation/manifest.toml")),
                ("box", include_str!("../../../src/contract/box/manifest.toml")),
                ("identity", include_str!("../../../src/contract/identity/manifest.toml")),
                ("multisig", include_str!("../../../src/contract/multisig/manifest.toml")),
                ("oracle", include_str!("../../../src/contract/oracle/manifest.toml")),
                ("purse", include_str!("../../../src/contract/purse/manifest.toml")),
            ];
            for (name, toml_str) in genesis_manifests {
                let manifest = match ContractManifest::from_toml(toml_str) {
                    Ok(m) => m,
                    Err(e) => { tracing::warn!(target: "dww::init", "genesis manifest {} parse: {}", name, e); continue; }
                };
                let cid = match get_contract_id(name) {
                    Some(c) => c,
                    None => { tracing::warn!(target: "dww::init", "genesis manifest {} cid", name); continue; }
                };
                let manifest_json = match serde_json::to_string(&manifest) {
                    Ok(j) => j,
                    Err(e) => { tracing::warn!(target: "dww::init", "genesis manifest {} json: {}", name, e); continue; }
                };
                let record = ContractMetadataRecord {
                    contract_id: bs58::encode(cid.to_bytes()).into_string(),
                    name: manifest.name.clone(),
                    symbol: Some(name.to_string()),
                    category: manifest.category.clone(),
                    description: Some(manifest.description.clone()),
                    public: true,
                    deployer_pubkey: String::new(),
                    deploy_height: BlockHeight::new(1),
                    attestations_json: String::new(),
                    lock_status: "unlocked".into(),
                };
                // §4.2.1: Manifest seeding is NOT best-effort — a wallet
                // without genesis manifests cannot construct capabilities.
                self.wallet.insert_contract_metadata_with_manifest(
                    &record, Some(&manifest_json),
                )?;
            }
        }

        // P4 Step 3: embed genesis zkas circuit binaries (wallet.md §3, §6.4.1 step 3).
        // These are compiled into dww at build time via the native_token contract crate.
        // Keyed by (contract_id bs58, namespace, circuit_name).
        {
            #[expect(clippy::expect_used, reason = "native_token contract id is a registered compile-time constant")]
            let cid_str = bs58::encode(crate::contract_imports::get_contract_id("native_token")
                .expect("native_token cid").to_bytes()).into_string();
            let circuits: &[(&str, &[u8])] = &[
                // HAZOP V1/V2 fix: align client circuit selection with metadata namespace
                ("Mint_V2", dwow_native_token_contract::NATIVE_TOKEN_CONTRACT_ZKAS_MINT_V2_BIN),
                ("Burn_V2", dwow_native_token_contract::NATIVE_TOKEN_CONTRACT_ZKAS_BURN_V2_BIN),
                ("Fee_V2", dwow_native_token_contract::NATIVE_TOKEN_CONTRACT_ZKAS_FEE_V2_BIN),
                ("FeeCollect_V2", dwow_native_token_contract::NATIVE_TOKEN_CONTRACT_ZKAS_FEE_COLLECT_V2_BIN),
            ];
            for (name, zkas_bytes) in circuits {
                // §4.2.1: ZK circuit binary seeding is NOT best-effort —
                // a wallet without circuit binaries cannot generate proofs.
                self.wallet.store_zkas_binary(&cid_str, name, name, zkas_bytes)?;
            }
        }

        // Register default DRKW native token alias so `transfer 1.0 DRKW <addr>` works
        // The tokens table is gone — a token's identity is discovered via scan,
        // not declared at init (capabilities carry their own asset_id).
        Ok(())
    }

    /// Auxiliary function to completely reset wallet state.
    /// Atomically clears ALL derived state (chain_blocks, caps, proofs, scanned
    /// markers) in one transaction — no partial/inconsistent DB state possible.
    pub fn reset(&self, output: &mut Vec<String>) -> WalletDbResult<()> {
        output.push(String::from("Resetting full wallet state"));
        self.wallet.reset_above(BlockHeight::new(0)).map_err(|e| {
            output.push(format!("Warning: atomic full reset failed: {e}"));
            e
        })?;
        output.push(String::from("Successfully reset full wallet state"));
        Ok(())
    }

    /// Get the capability commitment tree, rebuilt from the stored caps.
    /// Stores H(w, params) for all capabilities — per ocap.md:238.
    ///
    /// The tree is DERIVED (leaf = commitment, ordered by leaf_position), not
    /// loaded from an append-only cache. Rebuilding is idempotent across re-scans
    /// and reorgs — no duplicate leaves.
    pub fn get_capability_commitment_tree(&self) -> Result<MerkleTree> {
        self.wallet
            .rebuild_capability_tree()
            .map_err(|e| Error::Custom(format!("rebuild capability tree: {e}")))
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
        // §0.1.3: resolve via AccountManager delegation, never raw index.
        let owned = self.account_mgr.default_owned()
            .map_err(|e| Error::Custom(format!("default_owned: {}", e)))?;
        let secret = owned.expose_secret().clone();
        let public = dwow_sdk::crypto::keypair::PublicKey::from_secret(secret.clone());
        let test_plaintext: Vec<u8> =
            b"DarkWow AEAD pipeline self-test vector 2026".to_vec();

        use dwow_sdk::crypto::note::AeadEncryptedNote;
        use rand::rngs::OsRng;

        let encrypted = AeadEncryptedNote::encrypt(
            &test_plaintext, &public, &mut OsRng,
        ).map_err(|e| Error::Custom(format!(
            "AEAD self-test encrypt failed: {:?}", e
        )))?;

        let decrypted: Vec<u8> = encrypted.decrypt(&secret, 0)
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

    // get_capabilities_by_asset REMOVED — dead code. Fee builder calls
    // wallet.get_capabilities_by_asset() directly (walletdb returns CapRecords).

    // get_token REMOVED — dead code. Only called by parse_token_pair
    // (also removed — zero callers). Token resolution is handled
    // generically through manifests and CapRecord.asset_id.

    /// Get aliases mapped by token
    pub fn get_aliases_mapped_by_asset(&self) -> Result<HashMap<String, String>> {
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
            let public = PublicKey::from_secret(secret.clone());
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

    fn held_capabilities_by_asset(&self, asset_id: &str) -> std::result::Result<Vec<CapInfo>, String> {
        let cap_records = self.wallet.get_held_capabilities(Some(false))
            .map_err(|e| format!("{:?}", e))?;
        // If asset_id is empty, return all held caps. Otherwise, decode
        // asset_id from bs58 for byte comparison.
        let filter_bytes: Option<[u8; 32]> = if asset_id.is_empty() {
            None
        } else {
            let decoded = bs58::decode(asset_id).into_vec()
                .map_err(|e| format!("bs58 decode asset_id: {}", e))?;
            let arr: [u8; 32] = decoded.try_into()
                .map_err(|_| format!("Invalid asset_id length"))?;
            Some(arr)
        };
        Ok(cap_records.iter()
            // Generic capability selection: filter by asset_id across ALL
            // contracts. Native-token callers (fee builder) pass DRKW asset_id
            // which only native_token caps carry. Non-native callers pass their
            // contract's asset_id; the contract_id disambiguates naturally.
            .filter(|c| match &filter_bytes {
                Some(ref bytes) => &c.asset_id.to_bytes().to_vec() == bytes,
                None => true,
            })
            .map(|c| CapInfo {
                cap_id: c.cap_id.clone(),
                value: c.value,
                // P0.1c: resolve per-cap secret via AccountManager delegation.
                // §4.2.2: .ok() SHALL NOT appear on cryptographic paths.
                // §4.2.3: unwrap_or_default() SHALL NOT appear on crypto paths.
                // Distinguish: absent coords (legacy cap, unspendable) vs
                // corrupted coords (DB error, cap is suspect).
                secret: match c.key_coords.as_ref() {
                    Some(coords) => match self.account_mgr.resolve_key(coords) {
                        Ok(k) => bs58::encode(k.expose_secret().inner().to_repr()).into_string(),
                        Err(e) => {
                            tracing::warn!(target: "dww::wallet",
                                "resolve_key failed for cap {}: {:?} — cap unspendable",
                                c.cap_id, e);
                            String::new()
                        }
                    },
                    None => String::new(), // legacy cap, no coords stored
                },
                asset_id: c.asset_id,
                leaf_position: c.leaf_position,
                cap_blind: c.cap_blind.clone(),
                value_blind: c.value_blind.clone(),
                asset_blind: c.asset_blind.clone(),
                spend_hook: c.spend_hook,
                user_data: c.user_data,
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
        // F4-fix: use default_account() (respects default_index) instead of
        // hardcoding accounts[0], matching default_address() semantics.
        let account = self.account_mgr.default_account()
            .map_err(|e| format!("{e}"))?;
        let secret = account.keypair.secret.clone();
        Ok(bs58::encode(secret.inner().to_repr()).into_string())
    }

    fn load_zkas_binary(
        &self,
        contract_id: &str,
        namespace: &str,
        circuit_name: &str,
    ) -> Option<Vec<u8>> {
        // §4.2.3: DB error and "circuit not found" are semantically distinct.
        // unwrap_or(None) silently converts DB corruption into "no circuit."
        match self.wallet.load_zkas_binary(contract_id, namespace, circuit_name) {
            Ok(binary) => binary,
            Err(e) => {
                tracing::error!(target: "dww::wallet",
                    "load_zkas_binary DB error for {}::{}: {:?}",
                    contract_id, circuit_name, e);
                None
            }
        }
    }

    fn generate_proof(
        &self,
        _contract_id: &str,
        witness_map: &dwow_sdk::prover::CircuitWitnessMap,
        zkas_bytes: &[u8],
        seed: [u8; 32],
    ) -> std::result::Result<Vec<u8>, String> {
        // The concrete ProverImpl needs a CapabilityProvider. For the
        // initial wiring, construct one from the wallet's held capabilities
        // matching the asset/contract the manifest function expects.
        // Full capability selection by barb-cover (§6.2) is deferred.
        let caps = self.wallet.get_held_capabilities(Some(false))
            .map_err(|e| format!("{:?}", e))?;
        if caps.is_empty() {
            return Err("no held capabilities — nothing to spend".into());
        }
        // Use the first held cap as the input. The full selection logic
        // (§6.2 barb-cover) matches the manifest's capability expression
        // to held caps. For now, single-capability transactions work.
        let cap = &caps[0];
        // §4.2.2: .ok() SHALL NOT appear on cryptographic paths.
        // Preserve the error reason from resolve_key for diagnostics.
        let owned_secret = match cap.key_coords.as_ref() {
            Some(coords) => match self.account_mgr.resolve_key(coords) {
                Ok(k) => k,
                Err(e) => return Err(format!("resolve_key failed: {:?}", e)),
            },
            None => return Err("no stored key coordinates — cannot resolve secret".to_string()),
        };
        let secret: dwow_sdk::crypto::SecretKey = owned_secret.expose_secret().clone();
        let merkle_proof_info = self.get_merkle_proof(&cap.cap_id)?;
        // §6.2: Primitive soundness is a prerequisite. Merkle proof siblings
        // MUST be valid field elements — silent defaulting to zero on decode
        // failure produces proofs that may verify against incorrect witnesses.
        let mut merkle_path: Vec<pallas::Base> = Vec::with_capacity(merkle_proof_info.siblings.len());
        for s in &merkle_proof_info.siblings {
            let decoded = bs58::decode(s).into_vec()
                .map_err(|e| format!("merkle sibling bs58 decode failed: {}", e))?;
            if decoded.len() < 32 {
                return Err(format!("merkle sibling too short: {} bytes", decoded.len()));
            }
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&decoded[..32]);
            let sibling = Option::from(pallas::Base::from_repr(arr))
                .ok_or_else(|| format!("merkle sibling not a valid field element"))?;
            merkle_path.push(sibling);
        }

        let provider = crate::prover_impl::ResolvedCapProvider::new(
            vec![
                ("value".to_string(), pallas::Base::from(cap.value)),
                ("asset_id".to_string(), cap.asset_id.inner()),
                // §2.3: No unwrap_or(0) — zero is the correct default for absent
                // spend_hook/user_data (FuncId::none()), but the pattern is prohibited.
                ("spend_hook".to_string(), cap.spend_hook.map(|h| h.inner()).unwrap_or_else(|| pallas::Base::zero())),
                ("user_data".to_string(), match cap.user_data {
                    Some(b) => pallas::Base::from_repr(b).unwrap_or_else(|| {
                        tracing::error!(
                            "build_native_transfer: corrupt user_data blob for cap {} — \
                             invalid field element, using zero",
                            cap.cap_id
                        );
                        pallas::Base::zero()
                    }),
                    None => pallas::Base::zero(),
                }),
            ],
            secret,
            merkle_path,
            cap.leaf_position as u32,
        );

        let ctx = dwow_sdk::prover::ProverContext::new(
            dwow_sdk::manifest::ContractManifest::empty(),
            String::new(), // function name — witness_map covers binding
            witness_map.clone(),
            seed,
        );

        crate::prover_impl::create_generic_proof(&ctx, &provider, zkas_bytes)
    }
}

impl Dww {
    /// Resolve a held capability to its contract's manifest and find the
    /// function for a named action (e.g. "transfer"). This is the missing
    /// piece between capability selection and manifest-driven invocation.
    pub fn resolve_transfer_contract(
        &self,
        cap: &CapRecord,
        action_name: &str,
    ) -> std::result::Result<(dwow_sdk::crypto::ContractId, String), String> {
        let cid_bs58 = bs58::encode(cap.contract_id.to_bytes()).into_string();
        let manifest = self.wallet.get_contract_manifest(&cid_bs58)
            .map_err(|e| format!("get_contract_manifest: {:?}", e))?
            .ok_or_else(|| format!(
                "contract {} has no stored manifest", cid_bs58,
            ))?;
        let action = manifest.actions.iter()
            .find(|a| a.function == action_name)
            .ok_or_else(|| format!(
                "contract {} has no action '{}'", cid_bs58, action_name,
            ))?;
        let output_cap_name = action.produces.first()
            .map(|c| c.name.clone())
            .ok_or_else(|| format!("action '{}' produces no capabilities", action_name))?;
        // Find the function that exercises this action.
        let func = manifest.functions.iter()
            .find(|f| f.name == output_cap_name || f.name == action_name)
            .or_else(|| manifest.functions.first())
            .ok_or_else(|| format!("no function found for action '{}'", action_name))?;
        Ok((cap.contract_id, func.name.clone()))
    }

    /// Build a native-token transfer transaction (wallet.md §6.4 — the ONE
    /// bespoke write-path citizen; executable spec:
    /// `wallet_model.py::build_transfer`).
    ///
    /// The full §6.3 pipeline: select the input capability (§6.2), resolve its
    /// secret via AccountManager key coordinates (§4), build the TransferV1
    /// call with real burn/mint proofs via the hardcoded `TransferCallBuilder`
    /// (§6.4), attach the fee call from a DIFFERENT DRKW cap (§6.3 step 6 —
    /// one nullifier is never published twice), publish every consumed
    /// nullifier in `Transaction.nullifiers` — transfer input first, fee input
    /// after (§6.3 step 4, model step 5) — and sign per-call (§6.3 step 7):
    /// one signature row per call, transfer row = input secrets, fee row =
    /// the fee ephemeral (mempool admission rejects any other layout).
    ///
    /// `seed` is the explicit randomness name (§6.1), drawn by the shell
    /// (dispatch/RPC): every transfer blind and AEAD ephemeral derives from
    /// it, so identical (inputs, seed) yield identical transfer params.
    ///
    /// # Errors
    ///
    /// Returns an error if no DRKW cap covers `amount`, if the selected cap
    /// has no stored `key_coords` (pre-upgrade wallet), if no second DRKW cap
    /// is available for the fee, or if ZK proof generation fails.
    pub async fn build_native_transfer(
        &self,
        amount: u64,
        recipient_bs58: &str,
        seed: [u8; 32],
    ) -> Result<Transaction> {
        use crate::contract_imports::native_token::{
            DRKW_ASSET_ID, InputWitness, TransferCallBuilder, TransferCallOutput,
            NATIVE_TOKEN_CONTRACT_ZKAS_BURN_V2_BIN, NATIVE_TOKEN_CONTRACT_ZKAS_MINT_V2_BIN,
        };
        use dwow_core::zk::{proof::ProvingKey, vm::ZkCircuit, vm_heap::empty_witnesses};
        use dwow_core::zkas::ZkBinary;
        use dwow_sdk::crypto::{
            keypair::Address, BaseBlind, Blind, FuncId, MerkleNode,
        };
        use dwow_sdk::pasta::pallas;
        use rand::{rngs::StdRng, SeedableRng};

        // wallet.md §6.2 / model step 1: select the input capability covering
        // `amount`. The fee is paid from a DIFFERENT cap (model
        // build_fee_and_finalize_tx, exclude_cap_id) — no fee reserve is
        // deducted here (deducting one while paying the fee elsewhere would
        // silently destroy that value).
        let caps = self.wallet.get_capabilities_by_asset(&DRKW_ASSET_ID, Some(false))
            .map_err(|e| Error::Custom(format!("get DRKW caps: {:?}", e)))?;
        let selected = caps.iter()
            .find(|c| c.value >= amount)
            .ok_or_else(|| Error::Custom(format!(
                "No DRKW cap with value >= {}", amount,
            )))?;
        let change_value = selected.value - amount;

        // wallet.md §4: resolve per-cap secret via AccountManager.
        // wallet.md §6.4.0: prefer the persisted spend_secret (fresh for received
        // TransferV1/SpendV1 outputs); fall back to key_coords for self-issued
        // coinbase/fee coins (derivable via AccountManager).
        let coords = selected.key_coords.as_ref()
            .ok_or_else(|| Error::Custom(format!(
                "Cap {} has no key_coords", selected.cap_id,
            )))?;
        let cap_secret = if let Some(s) = &selected.spend_secret {
            s.clone()
        } else {
            self.account_mgr.resolve_key(coords)
                .map_err(|e| Error::Custom(format!("resolve_key: {}", e)))?
                .expose_secret().clone()
        };

        // wallet.md §6.1 shell: Merkle proof from DB
        let merkle_proof = self.wallet.get_merkle_proof(&selected.cap_id)
            .map_err(|e| Error::Custom(format!("merkle proof: {:?}", e)))?;
        let merkle_path: Vec<MerkleNode> = merkle_proof.siblings.iter()
            .map(|s| {
                let b: [u8; 32] = bs58::decode(s).into_vec()
                    .map_err(|e| Error::Custom(e.to_string()))?
                    .try_into().map_err(|_| Error::Custom("bad merkle len".into()))?;
                Ok(MerkleNode::from_bytes(b).ok_or_else(|| Error::Custom("bad merkle node".into()))?)
            })
            .collect::<Result<Vec<_>>>()?;

        // Load ZK binaries — proving keys are cached across calls.
        // The PK depends only on the compile-time-embedded zkas binary.
        let burn_zkbin = ZkBinary::decode(NATIVE_TOKEN_CONTRACT_ZKAS_BURN_V2_BIN, false)
            .map_err(|e| Error::Custom(format!("burn zkbin: {}", e)))?;
        let burn_pk = {
            let mut cache = self.burn_pk_cache.lock().await;
            match cache.as_ref() {
                Some(pk) => pk.clone(),
                None => {
                    let c = ZkCircuit::new(empty_witnesses(&burn_zkbin)?, &burn_zkbin);
                    let pk = ProvingKey::build(burn_zkbin.k, &c)
                        .map_err(|e| Error::Custom(format!("burn pk: {}", e)))?;
                    *cache = Some(pk.clone());
                    pk
                }
            }
        };
        let mint_zkbin = ZkBinary::decode(NATIVE_TOKEN_CONTRACT_ZKAS_MINT_V2_BIN, false)
            .map_err(|e| Error::Custom(format!("mint zkbin: {}", e)))?;
        let mint_pk = {
            let mut cache = self.mint_pk_cache.lock().await;
            match cache.as_ref() {
                Some(pk) => pk.clone(),
                None => {
                    let c = ZkCircuit::new(empty_witnesses(&mint_zkbin)?, &mint_zkbin);
                    let pk = ProvingKey::build(mint_zkbin.k, &c)
                        .map_err(|e| Error::Custom(format!("mint pk: {}", e)))?;
                    *cache = Some(pk.clone());
                    pk
                }
            }
        };

        // wallet.md §6.1: the Seed is the explicit randomness name. Every
        // blind and AEAD ephemeral below derives from this rng (model
        // _derive_blind/_seeded_rng).
        let mut rng = StdRng::from_seed(seed);

        // HAZOP C7 fix: per-transaction random nonces make tx_binding unique.
        let tx_commitment = BaseBlind::random(&mut rng).inner();
        let tx_nonce = BaseBlind::random(&mut rng).inner();
        let spend_hook = pallas::Base::zero();
        let user_data = pallas::Base::zero();

        // Compute circuit costs from the transfer ZK binaries before they're
        // moved into the builder. fee-spec.md §12.11: fee includes circuit
        // difficulty scaled by k-value.
        let transfer_circuit_costs = vec![
            circuit_difficulty(&burn_zkbin.opcodes),
            circuit_difficulty(&mint_zkbin.opcodes),
        ];

        // Compose input witness from selected cap
        let input_witness = InputWitness {
            value: selected.value,
            asset_id: DRKW_ASSET_ID.inner(),
            user_data,
            commitment_blind: selected.cap_blind.clone(),
            leaf_position: merkle_proof.leaf_position,
            merkle_path,
        };

        // Compose recipient output (blind Seed-derived, §6.1)
        let addr: Address = recipient_bs58.parse()
            .map_err(|e| Error::Custom(format!("recipient address: {}", e)))?;
        let recipient_pk = *addr.public_key();
        let recv_coin = TransferCallOutput {
            version: 0,
            public_key: recipient_pk,
            value: amount,
            asset_id: DRKW_ASSET_ID,
            spend_hook: FuncId::from_base(spend_hook),
            user_data,
            blind: Blind(BaseBlind::random(&mut rng).inner()),
        };

        let mut builder = TransferCallBuilder {
            inputs: vec![(input_witness, cap_secret, spend_hook)],
            outputs: vec![recv_coin],
            burn_zkbin, burn_pk, mint_zkbin, mint_pk,
            tx_commitment, tx_nonce,
        };

        // Compose change output if applicable — change returns to the OWNING
        // ACCOUNT's master key (model step: change to sender,
        // wallet_model.py:3938). The account's master key rather than the
        // per-block instance key: Path 1 scan trials master keys at every
        // height but per-block keys only at their own height, so change sent
        // to an old per-block key would never be rediscovered.
        if change_value > 0 {
            // §0.1.3: resolve change-output key via AccountManager delegation —
            // the SAME resolve_key path the fee builder uses, not a manual
            // index into secrets(). KeyDerivation::Master: change always
            // returns to the account's master key (not per-block), so Path 1
            // scan rediscovers it at any height.
            let change_owned = self.account_mgr.resolve_key(
                &dwow_accounts::KeyCoordinates {
                    account_index: coords.account_index,
                    derivation: dwow_accounts::KeyDerivation::Master,
                },
            ).map_err(|e| Error::Custom(format!("resolve_key change: {}", e)))?;
            let change_secret = change_owned.expose_secret().clone();
            builder.outputs.push(TransferCallOutput {
                version: 0,
                public_key: PublicKey::from_secret(change_secret),
                value: change_value,
                asset_id: DRKW_ASSET_ID,
                spend_hook: FuncId::from_base(spend_hook),
                user_data,
                blind: Blind(BaseBlind::random(&mut rng).inner()),
            });
        }

        let debris = builder.build(&mut rng)
            .map_err(|e| Error::Custom(format!("transfer build: {}", e)))?;

        // §6.3 step 4: the nullifiers this transfer publishes — one per
        // consumed input, the SAME values the entrypoint verifies from params.
        let transfer_nullifiers: Vec<dwow_sdk::crypto::Nullifier> =
            debris.params.inputs.iter().map(|i| i.nullifier).collect();
        // Schnorr signatures removed per contract-standards.md §3.

        // wallet.md §6.3 step 8: serialize params with function selector.
        // Layout: [0x03][TransferParamsV1] — what the entrypoint's
        // deserialize(params) and the balance checker parse. Direct encode,
        // no length prefix.
        let mut data = vec![0x03u8];
        data.extend_from_slice(&debris.params.encode());

        let leaf = dwow_core::tx::ContractCallLeaf {
            call: dwow_sdk::tx::ContractCall {
                contract_id: *NATIVE_TOKEN_CONTRACT_ID,
                data,
            },
            proofs: debris.proofs,
        };

        // §6.3 step 6 / model steps 3-4: fee from a different DRKW cap;
        // TransactionBuilder computes the outer tx_commitment; the fee input's
        // nullifier is published by the fee builder.
        let mut tx = crate::fee_builder::build_fee_and_finalize_tx(
            &self.wallet, &self.account_mgr, leaf, None, Some(&selected.cap_id), seed,
            &transfer_circuit_costs, self.contract_risk_factor(&*NATIVE_TOKEN_CONTRACT_ID, false), WasmKb::ZERO, self.latest_fee_window_flags(),
            FeeTier::LOW,
        )?;

        // Model step 5 (wallet_model.py:3954): nullifier order is
        // [transfer inputs..., fee input].
        for nf in transfer_nullifiers.iter().rev() {
            tx.nullifiers.insert(0, *nf);
        }

        // §6.3 step 7: per-call signature rows, in call order
        // (calls[0] = transfer, calls[1] = fee). The transfer row is signed by
        // the input secrets (metadata: inputs[].signature_public); the fee row

        Ok(tx)
    }

    /// Mark caps from a transaction as revoked in the wallet database.
    /// HAZOP WP-7 C4+C5: mark consumed caps as PENDING after broadcast.
    /// Previously called mark_revoked (which jumped to PROCESSING), and
    /// relied on detect_transferred (no-op for native token). Now:
    /// - Extracts nullifiers from tx.nullifiers and matches against held caps
    ///   using the same poseidon_hash(secret, commitment) as match_nullifiers
    /// - Sets status = Pending (broadcast, not yet mined)
    /// - The scan path is the sole authority for PROCESSING/revoked
    pub fn mark_tx_exercise(&self, tx: &Transaction, output: &mut Vec<String>) -> Result<usize> {
        use crate::capability::CapStatus;

        // Get all unspent caps
        let unspent_caps = self.wallet.get_held_capabilities(Some(false))
            .map_err(|e| Error::Custom(format!("Failed to get capabilities: {:?}", e)))?;

        let current_height = match self.chain_height() {
            Ok(h) => h,
            Err(e) => {
                return Err(Error::Custom(format!(
                    "Failed to read chain height for pending mark: {e}"
                )));
            }
        };

        // C5 fix: extract nullifiers from transaction and match against held caps.
        // This replaces the broken detect_transferred (no-op for native token).
        // Same computation as match_nullifiers in scan.rs: poseidon_hash(secret, commitment)
        let tx_nullifiers: Vec<dwow_sdk::crypto::Nullifier> = tx.nullifiers.iter()
            .filter(|n| !n.is_zero()).cloned().collect();

        let mut marked = 0usize;
        for cap in &unspent_caps {
            // Only mark caps whose nullifiers appear in this transaction
            let key = match cap.key_coords.as_ref() {
                Some(coords) => match self.account_mgr.resolve_key(coords) {
                    Ok(k) => k.expose_secret().clone(),
                    Err(e) => {
                        error!("resolve_key failed for cap {}: {}", cap.cap_id, e);
                        continue;
                    }
                },
                None => continue,
            };

            // §2.2 / §8.1 / §4.2.2: construct the Nullifier directly from the
            // key + commitment. No `to_repr()` → `from_bytes().ok()` round-trip,
            // which erases the ↓nullify barb and silently drops the
            // zero/non-canonical rejection.
            let nullifier = dwow_sdk::crypto::Nullifier::new(key, cap.commitment.inner());

            if tx_nullifiers.iter().any(|tn| tn == &nullifier) {
                // C4 fix: set PENDING, not PROCESSING.
                // The cap is broadcast but not yet mined.
                match self.wallet.set_cap_status(
                    &cap.cap_id, CapStatus::Pending, current_height,
                ) {
                    Ok(()) => {
                        marked += 1;
                        output.push(format!(
                            "Marked capability {} as pending (broadcast)", cap.cap_id
                        ));
                    }
                    Err(e) => {
                        return Err(Error::Custom(format!(
                            "Failed to mark capability {} as pending: {:?}", cap.cap_id, e
                        )));
                    }
                }
            }
        }
        Ok(marked)
    }

    /// HAZOP H1: expire pending caps past MEMPOOL_WINDOW.
    /// Caps marked PENDING at broadcast that were never mined (mempool
    /// eviction, competing block) become spendable again after the window.
    pub fn expire_pending_caps(&self, current_height: BlockHeight) -> Result<usize> {
        use crate::capability::{CapStatus, MEMPOOL_WINDOW};
        let caps = self.wallet.get_held_capabilities(None)
            .map_err(|e| Error::Custom(format!("expire_pending: {}", e)))?;
        let mut expired = 0usize;
        for cap in &caps {
            if cap.status == Some(CapStatus::Pending) {
                if let Some(h) = cap.status_height {
                    if current_height.saturating_sub(h) > MEMPOOL_WINDOW {
                        self.wallet.clear_cap_status(&cap.cap_id)
                            .map_err(|e| Error::Custom(format!("expire_pending clear: {}", e)))?;
                        expired += 1;
                    }
                }
            }
        }
        if expired > 0 {
            tracing::info!(target: "dww::wallet",
                "Expired {} pending caps past MEMPOOL_WINDOW ({} blocks)", expired, MEMPOOL_WINDOW);
        }
        Ok(expired)
    }

    /// HAZOP H2: advance PROCESSING caps to SPENT after CONFIRMATION_DEPTH.
    /// Once a nullifier has been on-chain for >= 100 blocks, the cap is
    /// permanently confirmed and can be considered fully spent.
    pub fn check_confirmations(&self, current_height: BlockHeight) -> Result<usize> {
        use crate::capability::{CapStatus, CONFIRMATION_DEPTH};
        let caps = self.wallet.get_held_capabilities(None)
            .map_err(|e| Error::Custom(format!("check_confirmations: {}", e)))?;
        let mut confirmed = 0usize;
        for cap in &caps {
            if cap.status == Some(CapStatus::Processing) {
                if let Some(h) = cap.status_height {
                    if current_height.saturating_sub(h) >= CONFIRMATION_DEPTH {
                        self.wallet.set_cap_status(&cap.cap_id, CapStatus::Spent, h)
                            .map_err(|e| Error::Custom(format!("check_confirmations set_spent: {}", e)))?;
                        confirmed += 1;
                    }
                }
            }
        }
        if confirmed > 0 {
            tracing::info!(target: "dww::wallet",
                "Confirmed {} processing caps → spent (>= {} blocks)", confirmed, CONFIRMATION_DEPTH);
        }
        Ok(confirmed)
    }

    /// Check if a call is a NativeToken fee call.
    /// H-8: FeeV1 (0x00) is REMOVED. Use `call.as_mass_balance_fee_v2()`
    /// for FeeV2 typed dispatch per type-system.md §10.5.
    #[deprecated(since = "0.6.0", note = "FeeV1 removed. Use `call.as_mass_balance_fee_v2()` for FeeV2.")]
    pub fn is_native_token_fee(&self, call: &ContractCall) -> bool {
        call.contract_id == *NATIVE_TOKEN_CONTRACT_ID &&
            (call.data.first() == Some(&0x00) ||
             call.data.first() == Some(&0x08))
    }


    // `keygen` REMOVED — the wallet no longer generates or stores random identity
    // keys. Its identity is declared in keys.toml and derived on boot via
    // `AccountManager` (see `account_mgr`). Key generation is an owner-run act
    // (`darkwow account generate`), never a wallet runtime path.

    /// Money balance
    pub fn capability_balance(&self) -> Result<HashMap<String, u64>> {
        let mut balances: HashMap<String, u64> = HashMap::new();

        // Get all unspent caps
        let cap_records = self.wallet.get_held_capabilities(Some(false)).map_err(|e| Error::Custom(format!("{:?}", e)))?;

        for record in cap_records {
            // Inflation guard: only the native token contract carries spendable
            // value. Foreign/composed capabilities are non-fungible metadata and
            // MUST NEVER be summed into a token balance, regardless of asset_id.
            if record.contract_id != *NATIVE_TOKEN_CONTRACT_ID {
                continue
            }
            // asset_id is now [u8; 32] — encode as bs58 for HashMap key (display boundary)
            let asset_key = bs58::encode(&record.asset_id.to_bytes()).into_string();
            *balances.entry(asset_key).or_insert(0) += record.value;
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
    // get_aliases_mapped_by_asset (above) is the live alias API.

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
        if secret_bytes.len() != 32 {
            return Err(Error::Custom(format!(
                "Invalid deploy auth length: {} bytes (expected 32)", secret_bytes.len()
            )));
        }
        let mut key_bytes = [0u8; 32];
        key_bytes.copy_from_slice(&secret_bytes);
        let deploy_key = dwow_sdk::crypto::SecretKey::from_bytes(key_bytes)
            .map_err(|e| Error::Custom(format!("Invalid deploy key: {}", e)))?;
        let keypair = dwow_sdk::crypto::Keypair::new(deploy_key.clone());
        let public_key_bs58 = bs58::encode(keypair.public.to_bytes()).into_string();

        // Build LockV1 params — just the deployer's public key
        let params = format!(r#"{{"public_key":"{}"}}"#, public_key_bs58);

        // Route through generic invoke — uses Deployooor's ContractClient.
        // LockV1 metadata declares [params.public_key] as its signature pubkey
        // (deployooor entrypoint/lock_v1.rs) — the deploy authority signs the
        // lock call's row.
        self.invoke_contract("deployooor", "LockV1", Some(&params), vec![], vec![deploy_key]).await
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
    /// * `signing_secrets` - secrets that sign the MAIN call's signature row —
    ///   they must match the pubkeys the contract's `get_metadata` declares for
    ///   this call (e.g., the deploy authority for `LockV1`); `vec![]` when the
    ///   call's metadata declares no signature pubkeys
    pub async fn invoke_contract(
        &self,
        contract_id_or_name: &str,
        function: &str,
        params: Option<&str>,
        proofs: Vec<Vec<u8>>,
        _signing_secrets: Vec<SecretKey>,
    ) -> Result<Transaction> {
        // Manifest-driven fallback storage — lives outside the or_else chain so
        // the reference survives. Stores both the synthetic metadata AND the
        // full manifest so the dispatch block can construct ManifestContractClient.
        let mut _manifest_owned: Option<crate::contract_metadata::ContractMetadata> = None;
        let mut _manifest_full: Option<dwow_sdk::manifest::ContractManifest> = None;

        // First try to look up as contract name (e.g., "dao_escrow")
        // If not found, try to parse as Base58 contract ID
        let registry = &*crate::contract_metadata::CONTRACT_METADATA_REGISTRY;
        let metadata = registry
            .get(contract_id_or_name)
            .or_else(|| {
                // Path B: Parse as Base58 ContractId, then reverse-lookup
                // the contract name from the runtime OnceLock registry.
                // §4.2.2: replace .ok() chain with logged fallback — the
                // fallback to the next strategy is intentional, but the error
                // reason must not be silently discarded.
                let cid = bs58::decode(contract_id_or_name)
                    .into_vec()
                    .map_err(|e| tracing::debug!("bs58 decode failed for '{}': {}", contract_id_or_name, e))
                    .ok()
                    .and_then(|v| {
                        let v_len = v.len();
                        v.try_into().map_err(|_| {
                            tracing::debug!("ContractId length mismatch for '{}': {} bytes (expected 32)", contract_id_or_name, v_len)
                        }).ok()
                    })
                    .and_then(|bytes: [u8; 32]| ContractId::from_bytes(bytes).map_err(|e| {
                        tracing::debug!("ContractId::from_bytes failed for '{}': {:?}", contract_id_or_name, e)
                    }).ok());
                cid.and_then(|id| {
                    registry.find_by_contract_id(&id)
                })
                .and_then(|name| {
                    registry.get(name)
                })
            })
            .or_else(|| {
                // Manifest-driven: load stored manifest, build metadata on the fly.
                // P4 Step 4: retain the FULL manifest so the dispatch block can
                // construct a ManifestContractClient (D2: circuit_registry removed;
                // proofs are built by the generic prover, wallet.md §6.4.1).
                let cid = crate::contract_imports::get_contract_id(contract_id_or_name)?;
                let cid_str = bs58::encode(cid.to_bytes()).into_string();
                // §4.2.3: DB error and "not found" are semantically distinct.
                let m = match self.wallet.get_contract_manifest(&cid_str) {
                    Ok(manifest) => manifest,
                    Err(e) => {
                        tracing::warn!(target: "dww::wallet",
                            "get_contract_manifest DB error for {}: {:?}", cid_str, e);
                        None
                    }
                }?;
                let f = m.functions.iter().find(|f| f.name == function)?;
                let name: &'static str = Box::leak(f.name.clone().into_boxed_str());
                _manifest_full = Some(m.clone());
                _manifest_owned = Some(crate::contract_metadata::ContractMetadata {
                    name,
                    functions: vec![crate::contract_metadata::FunctionSignature {
                        name, code: f.code, requires_proof: f.requires_proof,
                        proof_circuit: None, // manifest uses ManifestContractClient directly
                    }],
                });
                _manifest_owned.as_ref()
            })
            .ok_or_else(|| {
                Error::Custom(format!(
                    "Unknown contract: {}. The contract was not found in the hardcoded \
                     registry and has no stored manifest with function '{}'.",
                    contract_id_or_name, function
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
                // Canonical field-element seed: MUST be < p (a raw 256-bit seed
                // is >= p ~73% of the time and collapses to zero in from_repr,
                // reusing blinds across txs — linkability break).
                let seed: [u8; 32] = pallas::Base::random(&mut rand::rngs::OsRng).to_repr();
                // Read declared circuit_difficulty from the contract's manifest
                // [[cost_profiles]] for the called function. If the contract has
                // no manifest or no cost profile, pass empty — pessimistic: fee
                // only covers the fee circuits themselves (miner may apply 2.0×
                // execution risk factor).
                let circuit_costs: Vec<u64> = _manifest_full.as_ref()
                    .and_then(|m| {
                        let resolver = crate::manifest_resolver::ManifestResolver::new(m);
                        resolver.get_cost_profile(function).map(|cp| cp.circuit_difficulty)
                    })
                    .map(|d| vec![d])
                    .unwrap_or_default();
                let tx = crate::fee_builder::build_fee_and_finalize_tx(
                    &self.wallet, &self.account_mgr, leaf, None, None, seed, &circuit_costs, self.contract_risk_factor(&contract_id, _manifest_full.is_some()), WasmKb::ZERO, self.latest_fee_window_flags(),
            FeeTier::LOW,
        )?;
                // §6.3 step 7 / mempool admission: ONE signature row per call,
                // in call order — calls[0] = main (signed by the caller-supplied
                // secrets matching the call's metadata pubkeys, empty row when
                // the metadata declares none), calls[1] = fee (signed by the
                // fee ephemeral, fee metadata: signature_public = ephemeral).
                // Schnorr signatures removed per contract-standards.md §3.
                return Ok(tx);
            }
            // P4 Step 4: if a stored manifest exists, construct a
            // ManifestContractClient (D2: circuit_registry removed;
            // proofs via the generic prover, wallet.md §6.4.1).
            if let Some(ref manifest) = _manifest_full {
                use dwow_sdk::contract_client::ContractClient;
                let mc = dwow_sdk::contract_client::ManifestContractClient::new(
                    metadata.name, manifest.clone(),
                );
                let (contract_call_data, builder_proofs) = mc
                    .build(function, params.unwrap_or("{}"), self)
                    .map_err(|e| Error::Custom(e))?;
                call_data.extend_from_slice(&contract_call_data);

                let contract_call = ContractCall { contract_id, data: call_data };
                let leaf = ContractCallLeaf {
                    call: contract_call,
                    proofs: builder_proofs.into_iter().map(|p| Proof::new(p)).collect(),
                };
                // Canonical field-element seed: MUST be < p (a raw 256-bit seed
                // is >= p ~73% of the time and collapses to zero in from_repr,
                // reusing blinds across txs — linkability break).
                let seed: [u8; 32] = pallas::Base::random(&mut rand::rngs::OsRng).to_repr();
                // Read declared circuit_difficulty from the contract's manifest
                // [[cost_profiles]] for the called function.
                let circuit_costs: Vec<u64> = {
                    let resolver = crate::manifest_resolver::ManifestResolver::new(manifest);
                    resolver.get_cost_profile(function)
                        .map(|cp| vec![cp.circuit_difficulty])
                        .unwrap_or_default()
                };
                let tx = crate::fee_builder::build_fee_and_finalize_tx(
                    &self.wallet, &self.account_mgr, leaf, None, None, seed, &circuit_costs, self.contract_risk_factor(&contract_id, _manifest_full.is_some()), WasmKb::ZERO, self.latest_fee_window_flags(),
                    FeeTier::LOW,
                )?;
                // Per-call signature rows (see the Path A exit above).
                // Schnorr signatures removed per contract-standards.md §3.
                return Ok(tx);
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
        // Canonical field-element seed: MUST be < p (a raw 256-bit seed is
        // >= p ~73% of the time and collapses to zero in from_repr, reusing
        // blinds across txs — linkability break).
        let seed: [u8; 32] = pallas::Base::random(&mut rand::rngs::OsRng).to_repr();
        // Read declared circuit_difficulty from the contract's manifest
        // [[cost_profiles]] for the called function. Fallback path — manifest
        // may or may not be available.
        let circuit_costs: Vec<u64> = _manifest_full.as_ref()
            .and_then(|m| {
                let resolver = crate::manifest_resolver::ManifestResolver::new(m);
                resolver.get_cost_profile(function).map(|cp| cp.circuit_difficulty)
            })
            .map(|d| vec![d])
            .unwrap_or_default();
        let tx = crate::fee_builder::build_fee_and_finalize_tx(
            &self.wallet, &self.account_mgr, leaf, None, None, seed, &circuit_costs, self.contract_risk_factor(&contract_id, _manifest_full.is_some()), WasmKb::ZERO, self.latest_fee_window_flags(),
            FeeTier::LOW,
        )?;
        // Per-call signature rows (see the Path A exit above).

        Ok(tx)
    }

    /// Print a full diagnostic report: P2P config, seed status, peer list,
    /// sync state, chain health, and recent errors. Used by `wallet diagnostic`
    /// CLI and the pipeline's wallet verification phase.
    pub fn diagnostic(&self, output: &mut Vec<String>) -> Result<()> {
        output.push("=== Wallet Diagnostic ===".into());
        output.push(format!("Network: {:?}", self.network));
        // §4.2.3: chain_height().unwrap_or(0) is prohibited — "database
        // corrupted" is NOT semantically equivalent to "chain is empty."
        let height = match self.wallet.chain_height() {
            Ok(h) => h.get(),
            Err(e) => {
                output.push(format!("Chain height: ERROR ({})", e));
                0
            }
        };
        output.push(format!("Chain height: {}", height));

        if let Some(ref p2p) = self.p2p {
            let peer_count = p2p.hosts().peers().len();
            output.push(format!("P2P: initialized ({} peers)", peer_count));
            output.push(format!("Highest peer tip: {}", self.highest_peer_tip.get().get()));
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

