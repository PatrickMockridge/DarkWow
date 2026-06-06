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
    sync::{
        atomic::{AtomicBool, AtomicU64},
        Arc,
    },
};

use smol::lock::Mutex;
use tracing::{debug, error, info};

use dwow_core::{
    blockchain::HeaderHash,
    net::settings::Settings,
    rpc::{
        jsonrpc::{JsonNotification, JsonSubscriber},
        server::{listen_and_serve, RequestHandler},
        settings::RpcSettings,
    },
    system::{ExecutorPtr, PublisherPtr, StoppableTask, StoppableTaskPtr},
    Error, Result,
};
use dwow_sdk::crypto::keypair::Network;
use dwow_sdk::crypto::{DEPLOYOOOR_CONTRACT_ID, NATIVE_TOKEN_CONTRACT_ID};

#[cfg(test)]
mod tests;

mod error;
use error::{server_error, RpcError};

/// Contract registry and handlers for generalized invocation
mod contract_registry;
mod contract_handler;

/// JSON-RPC requests handler and methods
mod rpc;
use rpc::{management::ManagementRpcHandler, DefaultRpcHandler};

/// Validator async tasks
pub mod task;
use task::{consensus_linear_init_task, ConsensusInitTaskConfig};

/// P2P net protocols
mod proto;
use proto::{DwowP2pHandler, DwowP2pHandlerPtr};

/// Miners registry
mod registry;
use registry::{DwowMinersRegistry, DwowMinersRegistryPtr};
use crate::registry::model::LinearMinerRewardsRecipientConfig;

mod execution;

/// Mempool for pending transactions
mod mempool;
pub use mempool::{create_mempool, Mempool, MempoolPtr};

/// ZK verification for linear blockchain
mod zk;

/// Atomic pointer to the DarkWow node
pub type DwowNodePtr = Arc<DwowNode>;

// ---------------------------------------------------------------------------
// MiningState — block production coordination (stratum, merge-mining, miner RPC)
// Extracted from the DwowNode god object. Single concern: everything related
// to producing blocks via mining.
// ---------------------------------------------------------------------------

/// Block production state shared between stratum, merge-mining, and miner RPC.
pub struct MiningState {
    /// Last block timestamp for rate limiting
    pub last_block_time: AtomicU64,
    /// ZK proving materials for coinbase (lazy initialized)
    pub linear_zk: Mutex<Option<crate::registry::model::LinearPowRewardZk>>,
    /// Current block template for the active mining round
    pub current_linear_template: Mutex<Option<crate::registry::model::LinearBlockTemplate>>,
    /// Publisher for pushing stratum job notifications to miners
    pub linear_stratum_publisher: Mutex<Option<PublisherPtr<JsonNotification>>>,
    /// Recipient config for generating new block templates on submit
    pub linear_recipient_config: Mutex<Option<LinearMinerRewardsRecipientConfig>>,
    /// Serializes block submission to prevent concurrent RandomX VM access
    pub linear_submit_lock: Mutex<()>,
    /// Genesis hash for merge-mining RPC
    pub linear_genesis_hash: Mutex<Option<HeaderHash>>,
    /// Active merge mining job IDs (aux_hash → ())
    pub mm_jobs: Mutex<HashMap<String, ()>>,
    /// Submitted merge mining job IDs (dedup)
    pub mm_jobs_submitted: Mutex<HashSet<String>>,
    /// Set when consensus sync completes — gates mining until caught up.
    /// Will be replaced by oneshot channel in next step.
    pub sync_complete: AtomicBool,
}

impl MiningState {
    pub fn new() -> Self {
        Self {
            last_block_time: AtomicU64::new(0),
            linear_zk: Mutex::new(None),
            current_linear_template: Mutex::new(None),
            linear_stratum_publisher: Mutex::new(None),
            linear_recipient_config: Mutex::new(None),
            linear_submit_lock: Mutex::new(()),
            linear_genesis_hash: Mutex::new(None),
            mm_jobs: Mutex::new(HashMap::new()),
            mm_jobs_submitted: Mutex::new(HashSet::new()),
            sync_complete: AtomicBool::new(false),
        }
    }
}

// ---------------------------------------------------------------------------
// RpcState — JSON-RPC connection lifecycle
// ---------------------------------------------------------------------------

/// RPC connection tracking and event subscribers.
pub struct RpcState {
    /// Event subscribers (blocks, txs, proposals, dnet)
    pub subscribers: HashMap<&'static str, JsonSubscriber>,
    /// Main JSON-RPC connection tracker
    pub rpc_connections: Mutex<HashSet<StoppableTaskPtr>>,
    /// Management JSON-RPC connection tracker
    pub management_rpc_connections: Mutex<HashSet<StoppableTaskPtr>>,
}

impl RpcState {
    pub fn new(subscribers: HashMap<&'static str, JsonSubscriber>) -> Self {
        Self {
            subscribers,
            rpc_connections: Mutex::new(HashSet::new()),
            management_rpc_connections: Mutex::new(HashSet::new()),
        }
    }
}

// ---------------------------------------------------------------------------
// DwowNode
// ---------------------------------------------------------------------------

/// Structure representing a DarkWow node.
/// Each field is a reference to a single-concern sub-struct — no god object.
pub struct DwowNode {
    /// Chain state — single authoritative source of truth
    pub chain_state: Option<Arc<dwow_chain::CChainState>>,
    /// Mempool for pending transactions
    pub mempool: Option<MempoolPtr>,
    /// P2P network protocols handler
    pub p2p_handler: DwowP2pHandlerPtr,
    /// Node miners registry pointer
    pub registry: DwowMinersRegistryPtr,
    /// RPC connection lifecycle
    pub rpc_state: Arc<RpcState>,
    /// Mining / block production state
    pub mining_state: Arc<MiningState>,
    /// Whether node is running in localnet mode
    pub is_localnet: bool,
    /// Minimum interval between blocks in seconds
    pub min_block_interval: u64,
}

impl DwowNode {
    pub async fn new(
        chain_state: Option<Arc<dwow_chain::CChainState>>,
        mempool: Option<MempoolPtr>,
        p2p_handler: DwowP2pHandlerPtr,
        registry: DwowMinersRegistryPtr,
        subscribers: HashMap<&'static str, JsonSubscriber>,
        is_localnet: bool,
        min_block_interval: u64,
    ) -> Result<DwowNodePtr> {
        Ok(Arc::new(Self {
            chain_state,
            mempool,
            p2p_handler,
            registry,
            rpc_state: Arc::new(RpcState::new(subscribers)),
            mining_state: Arc::new(MiningState::new()),
            is_localnet,
            min_block_interval,
        }))
    }

    /// Returns whether the node is running in localnet mode
    pub fn is_localnet(&self) -> bool {
        self.is_localnet
    }

    /// Returns the mempool if running in darkwow-devnet mode
    pub fn mempool(&self) -> Option<MempoolPtr> {
        self.mempool.clone()
    }
}

/// Atomic pointer to the DarkWow daemon
pub type DwowdPtr = Arc<Dwowd>;

/// Structure representing a DarkWow daemon
pub struct Dwowd {
    /// DarkWow node instance
    node: DwowNodePtr,
    /// `dnet` background task
    dnet_task: StoppableTaskPtr,
    /// Main JSON-RPC background task
    rpc_task: StoppableTaskPtr,
    /// Management JSON-RPC background task
    management_rpc_task: StoppableTaskPtr,
    /// Consensus protocol background task
    consensus_task: StoppableTaskPtr,
    /// Built-in mining task (replaces bash /dev/tcp loop)
    miner_task: StoppableTaskPtr,
    /// Database path for mining address file
    db_path: std::path::PathBuf,
}

impl Dwowd {
    /// Initialize a DarkWow daemon for darkwow-devnet mode.
    ///
    /// Uses LinearBlockchain instead of Validator for consensus.
    pub async fn init_linear(
        network: Network,
        sled_db: &sled::Db,
        db_path: &std::path::Path,
        net_settings: &Settings,
        ex: &ExecutorPtr,
        finality_config: Option<dwow_chain::FinalityConfig>,
        create_genesis: bool,
    ) -> Result<DwowdPtr> {
        info!(target: "dwowd::Dwowd::init_linear", "Initializing a DarkWow daemon for darkwow-devnet...");

        let finality_config = finality_config.unwrap_or_default();
        info!(target: "dwowd::Dwowd::init_linear", "Finality mode: {:?}, caribina_enabled: {}", finality_config.mode, finality_config.caribina_enabled);

        // Create PoW config from network settings
        let pow_config = dwow_chain::PoWConfig {
            target_block_time: net_settings.pow.target_block_time.unwrap_or(120),
            initial_target: net_settings.pow.initial_target.unwrap_or(0x00FFFFFF) as u32,
            min_target: net_settings.pow.min_target.unwrap_or(1) as u32,
            max_target: net_settings.pow.max_target.unwrap_or(u32::MAX) as u32,
        };

        // Single authoritative chain state (replaces dual LinearBlockchain instances).
        // CChainState provides: store, consensus, VM pool, coin/nullifier sets.
        let chain_state = dwow_chain::CChainState::new(
            Arc::new(sled_db.clone()),
            pow_config.target_block_time,
            pow_config.initial_target,
            pow_config.min_target,
            pow_config.max_target,
            finality_config.clone(),
        ).map_err(|e| Error::Custom(e.to_string()))?;

        // CChainState is the single authoritative chain state.
        // No second instance. No diverged caches.

        // Deploy native contracts to linear blockchain
        info!(target: "dwowd::Dwowd::init_linear", "Deploying native contracts to linear blockchain...");
        let deployooor_wasm = include_bytes!("../../../src/contract/deployooor/dwow_deployooor_contract.wasm").to_vec();
        // NOTE: Contract deployment will move into genesis block (Phase 5).
        // For now, store WASM bytes directly so the chain can reference them.
        chain_state.store.set_contract_data(
            &DEPLOYOOOR_CONTRACT_ID.to_bytes(),
            &deployooor_wasm,
        ).map_err(|e| Error::Custom(e.to_string()))?;
        info!(target: "dwowd::Dwowd::init_linear", "Deployooor contract stored");

        let native_token_wasm = include_bytes!("../../../src/contract/native_token/dwow_native_token_contract.wasm").to_vec();
        chain_state.store.set_contract_data(
            &NATIVE_TOKEN_CONTRACT_ID.to_bytes(),
            &native_token_wasm,
        ).map_err(|e| Error::Custom(e.to_string()))?;
        info!(target: "dwowd::Dwowd::init_linear", "NativeToken contract stored");
        // NOTE: NativeToken's init_contract is not called here — sled trees
        // are lazily created on first db_lookup. ZK circuits are embedded in
        // the WASM module via include_bytes! in the entrypoint. Full
        // initialization (empty coin root seeding, nullifier root seeding)
        // should be done by calling the WASM init entrypoint post-genesis
        // or by seeding trees directly here (TODO: public testnet hardening).

        // Genesis block creation — only the designated genesis authority
        // creates the block. Other nodes start with height=0 and sync it
        // via P2P from the authority. This models production behavior where
        // one node bootstraps the chain.
        let linear_genesis_hash: HeaderHash = if create_genesis {
            use dwow_chain::{Block, BlockHeader, Miner, Output, PowSource, Transaction};
            use std::time::SystemTime;

            let genesis_height = 1u64;

            let target = u32::MAX;
            // Deterministic genesis timestamp — must be identical across all
            // nodes. Using 0 as a clear "genesis block" marker. This guarantees
            // every CREATE_GENESIS=true node produces the same genesis block.
            // Publish this genesis hash so joining nodes can verify.
            let timestamp = 0u64;

            let genesis_reward = dwow_sdk::blockchain::expected_reward(genesis_height as u32);

            let genesis_tx = Transaction {
                version: 1,
                inputs: vec![],
                outputs: vec![Output { value: genesis_reward, script: vec![] }],
                contract_calls: vec![],
                lock_time: 0,
                coinbase: None,
            };
            let genesis_merkle_root = genesis_tx.hash();

            let header = BlockHeader {
                version: 1,
                previous: blake3::Hash::from_bytes([0u8; 32]),
                merkle_root: genesis_merkle_root,
                timestamp,
                target,
                nonce: 0,
                height: genesis_height,
                uncle_merkle_root: [0u8; 32],
                total_reward: genesis_reward,
                randomx_key: Miner::derive_key_from_height(genesis_height),
                coin_merkle_root: [0u8; 32],
                nullifier_root: [0u8; 32],
                anchor_tx_id: [0u8; 32],
                anchor_monero_height: 0,
                anchor_monero_hash: [0u8; 32],
                finality_flags: 0,
                pow_source: PowSource::Native,
            };

            let genesis_block = Block { header, transactions: vec![genesis_tx] };
            let genesis_hash = chain_state.hash_block_with_cached_vm(&genesis_block);

            chain_state.connect_block(&genesis_block, &[], None)
                .map_err(|e| Error::Custom(format!("Failed to insert genesis block: {}", e)))?;

            info!(
                target: "dwowd::Dwowd::init_linear",
                "Genesis block created at height 1: {}",
                genesis_hash,
            );

            HeaderHash(genesis_hash.into())
        } else {
            info!(
                target: "dwowd::Dwowd::init_linear",
                "Skipping genesis creation — will sync from network"
            );
            HeaderHash([0u8; 32])
        };

        // Create mempool early — needed by both the P2P handler (for cleanup)
        // and the miner RPC (for transaction submission).
        let mempool = Some(create_mempool());

        // Initialize P2P network.
        // - chain_state → single source of truth for both sync and broadcast handlers
        let p2p_handler = DwowP2pHandler::init(
            net_settings,
            ex,
            Some(chain_state.clone()),
            Some(chain_state.clone()),
            mempool.clone(),
        ).await?;

        // Initialize the miners registry (placeholder for now)
        let registry = DwowMinersRegistry::init_linear(network, chain_state.clone()).await?;

        // Auto-generate mining keypair if one does not exist.
        // The address is persisted for the Docker entrypoint/xmrig to consume.
        {
            use dwow_sdk::crypto::keypair::{Address, Keypair, StandardAddress};
            use dwow_sdk::crypto::pasta_prelude::PrimeField;
            use rand::rngs::OsRng;
            use std::fs;

            let miner_address_path = db_path.join("mining_address");
            let miner_secret_path = db_path.join("mining_secret");

            if miner_address_path.exists() {
                let addr_str = fs::read_to_string(&miner_address_path)
                    .map_err(|e| Error::Custom(format!("Failed to read mining address: {}", e)))?;
                info!(
                    target: "dwowd::Dwowd::init_linear",
                    "Loaded persisted mining address: {}",
                    addr_str.trim(),
                );
            } else {
                let kp = Keypair::random(&mut OsRng);
                let std_addr = StandardAddress::from_public(Network::Testnet, kp.public);
                let addr: Address = std_addr.into();
                let addr_str = addr.to_string();

                fs::write(&miner_address_path, &addr_str)
                    .map_err(|e| Error::Custom(format!("Failed to persist mining address: {}", e)))?;

                let secret_hex = hex::encode(kp.secret.inner().to_repr());
                fs::write(&miner_secret_path, &secret_hex)
                    .map_err(|e| Error::Custom(format!("Failed to persist mining secret: {}", e)))?;

                info!(
                    target: "dwowd::Dwowd::init_linear",
                    "Generated new mining keypair. Address: {}",
                    addr_str,
                );
            }
        }

        // Here we initialize various subscribers that can export live blockchain/consensus data.
        let mut subscribers = HashMap::new();
        subscribers.insert("blocks", JsonSubscriber::new("blockchain.subscribe_blocks"));
        subscribers.insert("txs", JsonSubscriber::new("blockchain.subscribe_txs"));
        subscribers.insert("proposals", JsonSubscriber::new("blockchain.subscribe_proposals"));
        subscribers.insert("dnet", JsonSubscriber::new("dnet.subscribe_events"));

        let min_block_interval = net_settings.pow.min_block_interval.unwrap_or(10);
        let node = DwowNode::new(
            Some(chain_state),
            mempool,
            p2p_handler,
            registry,
            subscribers,
            false,
            min_block_interval,
        ).await?;

        // Store genesis hash for mm_rpc
        node.mining_state.linear_genesis_hash.lock().await.replace(linear_genesis_hash);

        // Generate the background tasks
        let dnet_task = StoppableTask::new();
        let rpc_task = StoppableTask::new();
        let management_rpc_task = StoppableTask::new();
        let consensus_task = StoppableTask::new();
        let miner_task = StoppableTask::new();

        info!(target: "dwowd::Dwowd::init_linear", "DarkWow daemon for darkwow-devnet initialized successfully!");

        Ok(Arc::new(Self { node, dnet_task, rpc_task, management_rpc_task, consensus_task, miner_task, db_path: db_path.to_path_buf() }))
    }

    /// Start the DarkWow daemon in the given executor, using the
    /// provided JSON-RPC settings and consensus initialization
    /// configuration.
    pub async fn start(
        &self,
        executor: &ExecutorPtr,
        rpc_settings: &RpcSettings,
        management_rpc_settings: &RpcSettings,
        stratum_rpc_settings: &Option<RpcSettings>,
        mm_rpc_settings: &Option<RpcSettings>,
        config: &ConsensusInitTaskConfig,
    ) -> Result<()> {
        info!(target: "dwowd::Dwowd::start", "Starting DarkWow daemon...");

        // Start the `dnet` task
        info!(target: "dwowd::Dwowd::start", "Starting dnet subs task");
        let dnet_sub_ = self.node.rpc_state.subscribers.get("dnet").unwrap().clone();
        let p2p_ = self.node.p2p_handler.p2p.clone();
        self.dnet_task.clone().start(
            async move {
                let dnet_sub = p2p_.dnet_subscribe().await;
                loop {
                    let event = dnet_sub.receive().await;
                    debug!(target: "dwowd::Dwowd::dnet_task", "Got dnet event: {event:?}");
                    dnet_sub_.notify(vec![event.into()].into()).await;
                }
            },
            |res| async {
                match res {
                    Ok(()) | Err(Error::DetachedTaskStopped) => { /* Do nothing */ }
                    Err(e) => error!(target: "dwowd::Dwowd::start", "Failed starting dnet subs task: {e}"),
                }
            },
            Error::DetachedTaskStopped,
            executor.clone(),
        );

        // Start the main JSON-RPC task
        info!(target: "dwowd::Dwowd::start", "Starting main JSON-RPC server");
        let node_ = self.node.clone();
        self.rpc_task.clone().start(
            listen_and_serve::<DefaultRpcHandler>(rpc_settings.clone(), self.node.clone(), None, executor.clone()),
            |res| async move {
                match res {
                    Ok(()) | Err(Error::RpcServerStopped) => <DwowNode as RequestHandler<DefaultRpcHandler>>::stop_connections(&node_).await,
                    Err(e) => error!(target: "dwowd::Dwowd::start", "Failed starting main JSON-RPC server: {e}"),
                }
            },
            Error::RpcServerStopped,
            executor.clone(),
        );

        // Start the management JSON-RPC task
        info!(target: "dwowd::Dwowd::start", "Starting management JSON-RPC server");
        let node_ = self.node.clone();
        self.management_rpc_task.clone().start(
            listen_and_serve::<ManagementRpcHandler>(management_rpc_settings.clone(), self.node.clone(), None, executor.clone()),
            |res| async move {
                match res {
                    Ok(()) | Err(Error::RpcServerStopped) => <DwowNode as RequestHandler<ManagementRpcHandler>>::stop_connections(&node_).await,
                    Err(e) => error!(target: "dwowd::Dwowd::start", "Failed starting management JSON-RPC server: {e}"),
                }
            },
            Error::RpcServerStopped,
            executor.clone(),
        );

        // Start the miners registry
        info!(target: "dwowd::Dwowd::start", "Starting miners registry");
        self.node.registry.start(executor, &self.node, stratum_rpc_settings, mm_rpc_settings)?;

        // Start the P2P network
        info!(target: "dwowd::Dwowd::start", "Starting P2P network");
        self.node.p2p_handler.start(executor, &self.node).await?;

        // Broadcast genesis if this node created it. The broadcast in
        // init_linear() happens before p2p.start() and is silently dropped.
        // Now that P2P is running, connected peers can receive the broadcast.
        if let Some(linear_chain) = &self.node.chain_state {
            if linear_chain.get_height() >= 1 {
                if let Ok(genesis) = linear_chain.get_block(1) {
                    info!(target: "dwowd::Dwowd::start",
                        "Broadcasting genesis to connected peers...");
                    crate::proto::linear_broadcast::broadcast_block(
                        &self.node.p2p_handler.p2p,
                        genesis,
                    ).await;
                }
            }
        }

        // Start the consensus protocol (linear mode)
        info!(target: "dwowd::Dwowd::start", "Starting consensus protocol task");
        self.consensus_task.clone().start(
            consensus_linear_init_task(
                self.node.clone(),
                config.clone(),
                executor.clone(),
            ),
            |res| async move {
                match res {
                    Ok(()) | Err(Error::ConsensusTaskStopped) | Err(Error::MinerTaskStopped) => { /* Do nothing */ }
                    Err(e) => error!(target: "dwowd::Dwowd::start", "Failed starting consensus initialization task: {e}"),
                }
            },
            Error::ConsensusTaskStopped,
            executor.clone(),
        );

        // Start the built-in miner (replaces the bash /dev/tcp loop)
        info!(target: "dwowd::Dwowd::start", "Starting built-in miner task");
        let miner_node = self.node.clone();
        let miner_db_path = self.db_path.clone();
        self.miner_task.clone().start(
            miner_task(miner_node, miner_db_path),
            |res| async move {
                match res {
                    Ok(()) | Err(Error::MinerTaskStopped) => {}
                    Err(e) => error!(target: "dwowd::Dwowd::start",
                        "Miner task stopped: {e}"),
                }
            },
            Error::MinerTaskStopped,
            executor.clone(),
        );

        info!(target: "dwowd::Dwowd::start", "DarkWow daemon started successfully!");
        Ok(())
    }

    /// Stop the DarkWow daemon.
    pub async fn stop(&self) -> Result<()> {
        info!(target: "dwowd::Dwowd::stop", "Terminating DarkWow daemon...");

        // Stop the `dnet` node
        info!(target: "dwowd::Dwowd::stop", "Stopping dnet subs task...");
        self.dnet_task.stop().await;

        // Stop the main JSON-RPC task
        info!(target: "dwowd::Dwowd::stop", "Stopping main JSON-RPC server...");
        self.rpc_task.stop().await;

        // Stop the management JSON-RPC task
        info!(target: "dwowd::Dwowd::stop", "Stopping management JSON-RPC server...");
        self.management_rpc_task.stop().await;

        // Stop the miners registry
        info!(target: "dwowd::Dwowd::stop", "Stopping miners registry...");
        self.node.registry.stop().await;

        // Stop the P2P network
        info!(target: "dwowd::Dwowd::stop", "Stopping P2P network protocols handler...");
        self.node.p2p_handler.stop().await;

        // Stop the consensus task
        info!(target: "dwowd::Dwowd::stop", "Stopping consensus task...");
        self.consensus_task.stop().await;

        // Stop the miner task
        info!(target: "dwowd::Dwowd::stop", "Stopping miner task...");
        self.miner_task.stop().await;

        // Flush linear blockchain store
        info!(target: "dwowd::Dwowd::stop", "Flushing sled database...");
        if let Some(ref chain) = self.node.chain_state {
            let _ = chain.store.flush();
            info!(target: "dwowd::Dwowd::stop", "Flushed linear blockchain store");
        }

        info!(target: "dwowd::Dwowd::stop", "DarkWow daemon terminated successfully!");
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Built-in Miner Task
//
// Replaces the fragile bash /dev/tcp loop in entrypoint.sh. The node mines
// internally like every production node (Bitcoin Core -gen, Geth --mine).
// ---------------------------------------------------------------------------

/// Internal mining task — loops indefinitely, mining blocks when sync is complete.
async fn miner_task(node: DwowNodePtr, db_path: std::path::PathBuf) -> Result<()> {
    use std::fs;
    use dwow_chain::{Miner, PowSource, UncleBlock};
    use crate::proto::linear_broadcast::broadcast_block;
    use crate::registry::model::{build_linear_coinbase, LinearPowRewardZk};
    use dwow_sdk::crypto::PublicKey;

    info!(target: "dwowd::miner_task", "Built-in miner starting...");

    // Read mining address from persisted file (written by init_chain)
    let miner_address_path = db_path.join("mining_address");
    let address_str = loop {
        match fs::read_to_string(&miner_address_path) {
            Ok(s) => {
                let s = s.trim().to_string();
                if !s.is_empty() { break s; }
            }
            Err(_) => {}
        }
        smol::Timer::after(std::time::Duration::from_secs(2)).await;
    };
    info!(target: "dwowd::miner_task", "Miner address: {}", address_str);

    // Wait for sync to complete before mining
    while !node.mining_state.sync_complete.load(std::sync::atomic::Ordering::SeqCst) {
        smol::Timer::after(std::time::Duration::from_secs(1)).await;
    }
    info!(target: "dwowd::miner_task", "Sync complete, starting mining loop");

    // Decode the address to a public key
    let recipient_bytes = match bs58::decode(&address_str).with_check(None).into_vec() {
        Ok(v) => v,
        Err(e) => {
            error!(target: "dwowd::miner_task", "Invalid mining address: {}", e);
            return Err(Error::Custom(format!("Invalid mining address: {}", e)));
        }
    };
    let public_key_bytes: [u8; 32] = match recipient_bytes[1..33].try_into() {
        Ok(b) => b,
        Err(_) => return Err(Error::Custom("Invalid address length".to_string())),
    };
    let public_key = match PublicKey::from_bytes(public_key_bytes) {
        Ok(pk) => pk,
        Err(e) => return Err(Error::Custom(format!("Invalid public key: {}", e))),
    };

    // Mining loop
    loop {
        let chain_state = match &node.chain_state {
            Some(cs) => cs.clone(),
            None => {
                smol::Timer::after(std::time::Duration::from_secs(5)).await;
                continue;
            }
        };

        let latest_block = match chain_state.get_latest_block() {
            Ok(b) => b,
            Err(e) => {
                error!(target: "dwowd::miner_task", "Failed to get latest block: {}", e);
                smol::Timer::after(std::time::Duration::from_secs(2)).await;
                continue;
            }
        };

        let height = latest_block.header.height + 1;
        let previous = chain_state.hash_block_with_cached_vm(&latest_block);
        let randomx_key = Miner::derive_key_from_height(height);
        // H1+H2 fix: miner creates its OWN VM, not from the shared cache.
        // Using chain_state.get_vm() would return an Arc<RandomXVM> that the
        // broadcast handler could also access concurrently during connect_block.
        // RandomX FFI is not thread-safe — concurrent access on the same VM
        // from two smol tasks causes a segfault.
        // Creating a fresh VM eliminates the entire concurrent access class.
        let flags = randomx::RandomXFlags::get_recommended_flags() & !randomx::RandomXFlags::JIT;
        let rx_cache = randomx::RandomXCache::new(flags, &randomx_key)
            .expect("Failed to create RandomX cache for miner");
        let vm = Arc::new(
            randomx::RandomXVM::new(flags, Some(rx_cache), None)
                .expect("Failed to create RandomX VM for miner"),
        );
        // Chain-derived target: matches Python model's get_next_work_required.
        // Reads timestamps from canonical chain blocks, not accumulator.
        let target = {
            let consensus = chain_state.consensus.lock().unwrap();
            consensus.get_next_work_required(&chain_state.store, height)
        };

        // Collect competing blocks from the previous height as uncles.
        // These are blocks mined by peers at the same height as our tip.
        // Including them distributes partial rewards via uncle-merkle consensus.
        let uncles: Vec<UncleBlock> = {
            let competing = chain_state.take_competing_blocks(latest_block.header.height);
            competing.iter().map(|block| {
                UncleBlock {
                    header: block.header.clone(),
                    transactions: block.transactions.clone(),
                    depth: 1,
                    pin_offered: false,
                    pin_accepted: false,
                    pin_reward: 0,
                }
            }).collect()
        };
        if !uncles.is_empty() {
            info!(target: "dwowd::miner_task",
                "Including {} uncles at height {}", uncles.len(), height);
        }

        info!(target: "dwowd::miner_task",
            "Mining block {} (target={:#010x})", height, target);

        // Lazy-init ZK proving materials
        let linear_zk = {
            let mut zk_lock = node.mining_state.linear_zk.lock().await;
            if zk_lock.is_none() {
                match LinearPowRewardZk::new(chain_state.clone()).await {
                    Ok(zk) => *zk_lock = Some(zk),
                    Err(e) => {
                        error!(target: "dwowd::miner_task", "ZK init failed: {}", e);
                        smol::Timer::after(std::time::Duration::from_secs(2)).await;
                        continue;
                    }
                }
            }
            zk_lock.clone()
        };

        // Build coinbase using the documented emission schedule
        let coinbase_reward = dwow_sdk::blockchain::expected_reward(height as u32);
        let (coinbase, _) = match build_linear_coinbase(
            public_key.clone(),
            coinbase_reward,
            linear_zk.as_ref().unwrap(),
            height as u32,
        ).await {
            Ok(cb) => cb,
            Err(e) => {
                error!(target: "dwowd::miner_task", "Coinbase build failed: {}", e);
                smol::Timer::after(std::time::Duration::from_secs(2)).await;
                continue;
            }
        };

        // Build transactions: coinbase + mempool
        let mempool_txs = if let Some(ref m) = node.mempool {
            m.take_all().await
        } else {
            Vec::new()
        };
        let mut all_txs = vec![dwow_chain::Transaction {
            version: 1,
            inputs: vec![],
            outputs: vec![],
            contract_calls: vec![],
            lock_time: 0,
            coinbase: Some(coinbase),
        }];
        all_txs.extend(mempool_txs);

        // Check if a block already arrived at this height (P2P broadcast
        // from a peer mining at the same time). If so, skip mining — the
        // peer's block is already committed.
        if chain_state.get_height() >= height {
            info!(target: "dwowd::miner_task",
                "Block already exists at height {} — peer beat us to it", height);
            smol::Timer::after(std::time::Duration::from_secs(1)).await;
            continue;
        }

        // Mine
        let miner_consensus = dwow_chain::PoWConsensus::new(120, target, 1, u32::MAX);
        let miner = Miner::new(std::sync::Arc::new(miner_consensus));
        let mined_block = match miner.mine(&vm, previous, height, all_txs, target) {
            Ok(b) => b,
            Err(e) => {
                error!(target: "dwowd::miner_task", "Mining failed: {}", e);
                smol::Timer::after(std::time::Duration::from_secs(2)).await;
                continue;
            }
        };

        // Drop VM reference before apply_block — avoids concurrent
        // RandomX access if a P2P block arrives during application.
        drop(vm);

        // Check again after mining — peer may have sent a block while we hashed
        if chain_state.get_height() >= height {
            info!(target: "dwowd::miner_task",
                "Peer block arrived at height {} during mining — discarding ours", height);
            smol::Timer::after(std::time::Duration::from_secs(1)).await;
            continue;
        }

        // Apply with uncles if any exist
        let apply_result = if uncles.is_empty() {
            chain_state.apply_block(&mined_block).await
        } else {
            chain_state.apply_block_with_uncles(&mined_block, &uncles).await
        };
        match apply_result {
            Ok(()) => {
                let applied_hash = chain_state.hash_block_with_cached_vm(&mined_block);
                info!(target: "dwowd::miner_task",
                    "Block {} mined and applied: {}",
                    height, applied_hash);
            }
            Err(e) => {
                error!(target: "dwowd::miner_task",
                    "Failed to apply mined block: {}", e);
                smol::Timer::after(std::time::Duration::from_secs(2)).await;
                continue;
            }
        }

        // Broadcast
        broadcast_block(&node.p2p_handler.p2p, mined_block).await;

        // Rate-limit: wait for min_block_interval before next block
        let min_interval = node.min_block_interval;
        let last = node.mining_state.last_block_time.load(std::sync::atomic::Ordering::Relaxed);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let elapsed = now.saturating_sub(last);
        if elapsed < min_interval {
            smol::Timer::after(std::time::Duration::from_secs(
                min_interval.saturating_sub(elapsed)
            )).await;
        }
        node.mining_state.last_block_time.store(now, std::sync::atomic::Ordering::Relaxed);
    }
}
