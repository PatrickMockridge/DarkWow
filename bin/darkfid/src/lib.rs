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
        atomic::AtomicU64,
        Arc,
    },
};

use smol::lock::Mutex;
use tracing::{debug, error, info};

use dwow::{
    net::settings::Settings,
    rpc::{
        jsonrpc::{JsonNotification, JsonSubscriber},
        server::{listen_and_serve, RequestHandler},
        settings::RpcSettings,
    },
    system::{ExecutorPtr, PublisherPtr, StoppableTask, StoppableTaskPtr},
    validator::{Validator, ValidatorConfig, ValidatorPtr},
    Error, Result,
};
use dwow_linear::LinearBlockchain as LinearBlockchainCore;
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
use task::{consensus::ConsensusInitTaskConfig, consensus_init_task, consensus_linear_init_task};

/// P2P net protocols
mod proto;
use proto::{DarkfidP2pHandler, DarkfidP2pHandlerPtr};

/// Miners registry
mod registry;
use registry::{DarkfiMinersRegistry, DarkfiMinersRegistryPtr};
use crate::registry::model::LinearMinerRewardsRecipientConfig;

/// Linear blockchain for localnet
mod blockchain;
pub use blockchain::LinearBlockchain;

/// Runtime integration for linear blockchain
mod runtime_integration;

/// LinearSimpleDb bridge adapter
mod linear_simple_db;

/// LinearContractStore bridge adapter
mod contract_store;

/// Mempool for pending transactions
mod mempool;
pub use mempool::{create_mempool, Mempool, MempoolPtr};

/// ZK verification for linear blockchain
mod zk;

/// Atomic pointer to the DarkWow node
pub type DarkfiNodePtr = Arc<DarkfiNode>;

/// Structure representing a DarkWow node
pub struct DarkfiNode {
    /// Validator(node) pointer
    validator: ValidatorPtr,
    /// Linear blockchain (only set when running in linear-testnet mode)
    linear_blockchain: Option<Arc<LinearBlockchain>>,
    /// Mempool for linear blockchain (only set in linear-testnet mode)
    mempool: Option<MempoolPtr>,
    /// P2P network protocols handler
    p2p_handler: DarkfidP2pHandlerPtr,
    /// Node miners registry pointer
    registry: DarkfiMinersRegistryPtr,
    /// Garbage collection task transactions batch size
    txs_batch_size: usize,
    /// A map of various subscribers exporting live info from the blockchain
    subscribers: HashMap<&'static str, JsonSubscriber>,
    /// Main JSON-RPC connection tracker
    rpc_connections: Mutex<HashSet<StoppableTaskPtr>>,
    /// Management JSON-RPC connection tracker
    management_rpc_connections: Mutex<HashSet<StoppableTaskPtr>>,
    /// Whether node is running in localnet mode
    is_localnet: bool,
    /// Last block timestamp for rate limiting (linear-testnet)
    last_block_time: AtomicU64,
    /// Minimum interval between blocks in seconds (linear-testnet)
    min_block_interval: u64,
    /// ZK proving materials for linear-testnet coinbase (lazy initialized)
    linear_zk: Mutex<Option<crate::registry::model::LinearPowRewardZk>>,
    /// Stored block template for the current mining round (linear-testnet)
    current_linear_template: Mutex<Option<crate::registry::model::LinearBlockTemplate>>,
    /// Publisher for pushing stratum job notifications to miners (linear-testnet)
    linear_stratum_publisher: Mutex<Option<PublisherPtr<JsonNotification>>>,
    /// Stored recipient config for generating new block templates on submit (linear-testnet)
    linear_recipient_config: Mutex<Option<LinearMinerRewardsRecipientConfig>>,
    /// Serializes block submission to prevent concurrent RandomX VM access
    linear_submit_lock: Mutex<()>,
}

impl DarkfiNode {
    pub async fn new(
        validator: ValidatorPtr,
        linear_blockchain: Option<Arc<LinearBlockchain>>,
        mempool: Option<MempoolPtr>,
        p2p_handler: DarkfidP2pHandlerPtr,
        registry: DarkfiMinersRegistryPtr,
        txs_batch_size: usize,
        subscribers: HashMap<&'static str, JsonSubscriber>,
        is_localnet: bool,
        min_block_interval: u64,
    ) -> Result<DarkfiNodePtr> {
        Ok(Arc::new(Self {
            validator,
            linear_blockchain,
            mempool,
            p2p_handler,
            registry,
            txs_batch_size,
            subscribers,
            rpc_connections: Mutex::new(HashSet::new()),
            management_rpc_connections: Mutex::new(HashSet::new()),
            is_localnet,
            last_block_time: AtomicU64::new(0),
            min_block_interval,
            linear_zk: Mutex::new(None),
            current_linear_template: Mutex::new(None),
            linear_stratum_publisher: Mutex::new(None),
            linear_recipient_config: Mutex::new(None),
            linear_submit_lock: Mutex::new(()),
        }))
    }

    /// Returns whether the node is running in localnet mode
    pub fn is_localnet(&self) -> bool {
        self.is_localnet
    }

    /// Returns the mempool if running in linear-testnet mode
    pub fn mempool(&self) -> Option<MempoolPtr> {
        self.mempool.clone()
    }
}

/// Atomic pointer to the DarkWow daemon
pub type DarkfidPtr = Arc<Darkfid>;

/// Structure representing a DarkWow daemon
pub struct Darkfid {
    /// Darkfi node instance
    node: DarkfiNodePtr,
    /// `dnet` background task
    dnet_task: StoppableTaskPtr,
    /// Main JSON-RPC background task
    rpc_task: StoppableTaskPtr,
    /// Management JSON-RPC background task
    management_rpc_task: StoppableTaskPtr,
    /// Consensus protocol background task
    consensus_task: StoppableTaskPtr,
}

impl Darkfid {
    /// Initialize a DarkWow daemon.
    ///
    /// Generates a new `DarkfiNode` for provided configuration,
    /// along with all the corresponding background tasks.
    pub async fn init(
        network: Network,
        sled_db: &sled::Db,
        config: &ValidatorConfig,
        net_settings: &Settings,
        txs_batch_size: &Option<usize>,
        ex: &ExecutorPtr,
        is_localnet: bool,
    ) -> Result<DarkfidPtr> {
        info!(target: "darkfid::Darkfid::init", "Initializing a Darkfi daemon...");
        // Initialize validator
        let validator = Validator::new(sled_db, config).await?;

        // Initialize P2P network
        let p2p_handler = DarkfidP2pHandler::init(net_settings, ex, None).await?;

        // Initialize the miners registry
        let registry = DarkfiMinersRegistry::init(network, &validator).await?;

        // Grab blockchain network configured transactions batch size for garbage collection
        let txs_batch_size = match txs_batch_size {
            Some(b) => {
                if *b > 0 {
                    *b
                } else {
                    50
                }
            }
            None => 50,
        };

        // Here we initialize various subscribers that can export live blockchain/consensus data.
        let mut subscribers = HashMap::new();
        subscribers.insert("blocks", JsonSubscriber::new("blockchain.subscribe_blocks"));
        subscribers.insert("txs", JsonSubscriber::new("blockchain.subscribe_txs"));
        subscribers.insert("proposals", JsonSubscriber::new("blockchain.subscribe_proposals"));
        subscribers.insert("dnet", JsonSubscriber::new("dnet.subscribe_events"));

        // Initialize node
        let min_block_interval = net_settings.pow.min_block_interval.unwrap_or(10);
        let node =
            DarkfiNode::new(validator, None, None, p2p_handler, registry, txs_batch_size, subscribers, is_localnet, min_block_interval).await?;

        // Generate the background tasks
        let dnet_task = StoppableTask::new();
        let rpc_task = StoppableTask::new();
        let management_rpc_task = StoppableTask::new();
        let consensus_task = StoppableTask::new();

        info!(target: "darkfid::Darkfid::init", "Darkfi daemon initialized successfully!");

        Ok(Arc::new(Self { node, dnet_task, rpc_task, management_rpc_task, consensus_task }))
    }

    /// Initialize a DarkWow daemon for linear-testnet mode.
    ///
    /// Uses LinearBlockchain instead of Validator for consensus.
    pub async fn init_linear(
        network: Network,
        sled_db: &sled::Db,
        db_path: &std::path::Path,
        net_settings: &Settings,
        txs_batch_size: &Option<usize>,
        ex: &ExecutorPtr,
    ) -> Result<DarkfidPtr> {
        info!(target: "darkfid::Darkfid::init_linear", "Initializing a Darkfi daemon for linear-testnet...");

        // Initialize linear blockchain (dwow_linear for P2P)
        let linear_blockchain_p2p = Arc::new(LinearBlockchainCore::new(Arc::new(sled_db.clone())).map_err(|e| Error::Custom(e.to_string()))?);

        // Initialize darkfid's blockchain wrapper (uses dwow_linear store)
        let store = linear_blockchain_p2p.store.clone();

        // Create PoW config from network settings
        let pow_config = crate::blockchain::LinearPoWConfig {
            target_block_time: net_settings.pow.target_block_time.unwrap_or(120),
            initial_difficulty: net_settings.pow.initial_difficulty.unwrap_or(0x000000FF) as u32,
            min_difficulty: net_settings.pow.min_difficulty.unwrap_or(1) as u32,
            max_difficulty: net_settings.pow.max_difficulty.unwrap_or(u32::MAX) as u32,
        };
        let linear_blockchain = Arc::new(LinearBlockchain::with_pow_config(store, pow_config));

        // Deploy native contracts to linear blockchain
        info!(target: "darkfid::Darkfid::init_linear", "Deploying native contracts to linear blockchain...");
        let deployooor_wasm = include_bytes!("../../../src/contract/deployooor/dwow_deployooor_contract.wasm").to_vec();
        linear_blockchain.deploy_contract(&deployooor_wasm, *DEPLOYOOOR_CONTRACT_ID)?;
        info!(target: "darkfid::Darkfid::init_linear", "Deployooor contract deployed");

        let native_token_wasm = include_bytes!("../../../src/contract/native_token/dwow_native_token_contract.wasm").to_vec();
        linear_blockchain.deploy_contract(&native_token_wasm, *NATIVE_TOKEN_CONTRACT_ID)?;
        info!(target: "darkfid::Darkfid::init_linear", "NativeToken contract deployed");

        // Create genesis block at height 1 with a valid RandomX hash.
        // Uses max difficulty so any nonce passes (instant genesis).
        // This exercises the RandomX VM early and ensures proper chain state
        // before miners connect. If RandomX is compiled with incompatible CPU
        // flags, this fails immediately with a clear error instead of crashing
        // during stratum submission.
        {
            use dwow_linear::{Block, BlockHeader, Miner, Transaction, Output};
            use std::time::SystemTime;

            let genesis_height = 1u64;
            let randomx_key = Miner::derive_key_from_height(genesis_height);
            let vm = linear_blockchain.get_vm(randomx_key);

            let difficulty_target = u32::MAX;
            let timestamp = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_secs();

            let genesis_reward = dwow_sdk::blockchain::expected_reward(genesis_height as u32);

            let genesis_tx = Transaction {
                version: 1,
                inputs: vec![],
                outputs: vec![Output { value: genesis_reward, script: vec![] }],
                contract_calls: vec![],
                lock_time: 0,
                coinbase: None,
            };

            let header = BlockHeader {
                version: 1,
                previous: blake3::Hash::from_bytes([0u8; 32]),
                merkle_root: blake3::hash(&[]),
                timestamp,
                difficulty_target,
                nonce: 0,
                height: genesis_height,
                uncle_merkle_root: [0u8; 32],
                total_reward: genesis_reward,
                randomx_key,
                coin_merkle_root: [0u8; 32],
                nullifier_root: [0u8; 32],
            };

            let genesis_block = Block { header, transactions: vec![genesis_tx] };
            let genesis_hash = genesis_block.hash(&vm);

            linear_blockchain.insert_block(&genesis_block)
                .map_err(|e| Error::Custom(format!("Failed to insert genesis block: {}", e)))?;

            info!(
                target: "darkfid::Darkfid::init_linear",
                "Genesis block created at height 1: {}",
                genesis_hash,
            );
        }

        // Initialize P2P network (linear P2P handlers use dwow_linear types)
        let p2p_handler = DarkfidP2pHandler::init(net_settings, ex, Some(linear_blockchain_p2p.clone())).await?;

        // Initialize the miners registry (placeholder for now)
        let registry = DarkfiMinersRegistry::init_linear(network, linear_blockchain.clone()).await?;

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
                    target: "darkfid::Darkfid::init_linear",
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
                    target: "darkfid::Darkfid::init_linear",
                    "Generated new mining keypair. Address: {}",
                    addr_str,
                );
            }
        }

        // Grab blockchain network configured transactions batch size for garbage collection
        let txs_batch_size = match txs_batch_size {
            Some(b) => if *b > 0 { *b } else { 50 },
            None => 50,
        };

        // Here we initialize various subscribers that can export live blockchain/consensus data.
        let mut subscribers = HashMap::new();
        subscribers.insert("blocks", JsonSubscriber::new("blockchain.subscribe_blocks"));
        subscribers.insert("txs", JsonSubscriber::new("blockchain.subscribe_txs"));
        subscribers.insert("proposals", JsonSubscriber::new("blockchain.subscribe_proposals"));
        subscribers.insert("dnet", JsonSubscriber::new("dnet.subscribe_events"));

        // Initialize node with linear blockchain
        // NOTE: A minimal Validator is still created because DarkfiNode requires it.
        // The validator is unused for consensus in linear mode (stratum delegates to
        // linear_blockchain before touching the validator), but its sled_db is the
        // same database used by the linear blockchain, so flush works correctly.
        let validator = Validator::new(&sled_db, &ValidatorConfig {
            confirmation_threshold: 3,
            max_forks: 8,
            pow_target: 120,
            pow_fixed_difficulty: None,
            genesis_block: None,
            verify_fees: true,
        }).await?;

        let min_block_interval = net_settings.pow.min_block_interval.unwrap_or(10);
        let node = DarkfiNode::new(
            validator,
            Some(linear_blockchain),
            Some(create_mempool()),
            p2p_handler,
            registry,
            txs_batch_size,
            subscribers,
            false,
            min_block_interval,
        ).await?;

        // Generate the background tasks
        let dnet_task = StoppableTask::new();
        let rpc_task = StoppableTask::new();
        let management_rpc_task = StoppableTask::new();
        let consensus_task = StoppableTask::new();

        info!(target: "darkfid::Darkfid::init_linear", "Darkfi daemon for linear-testnet initialized successfully!");

        Ok(Arc::new(Self { node, dnet_task, rpc_task, management_rpc_task, consensus_task }))
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
        is_linear: bool,
    ) -> Result<()> {
        info!(target: "darkfid::Darkfid::start", "Starting Darkfi daemon...");

        // Start the `dnet` task
        info!(target: "darkfid::Darkfid::start", "Starting dnet subs task");
        let dnet_sub_ = self.node.subscribers.get("dnet").unwrap().clone();
        let p2p_ = self.node.p2p_handler.p2p.clone();
        self.dnet_task.clone().start(
            async move {
                let dnet_sub = p2p_.dnet_subscribe().await;
                loop {
                    let event = dnet_sub.receive().await;
                    debug!(target: "darkfid::Darkfid::dnet_task", "Got dnet event: {event:?}");
                    dnet_sub_.notify(vec![event.into()].into()).await;
                }
            },
            |res| async {
                match res {
                    Ok(()) | Err(Error::DetachedTaskStopped) => { /* Do nothing */ }
                    Err(e) => error!(target: "darkfid::Darkfid::start", "Failed starting dnet subs task: {e}"),
                }
            },
            Error::DetachedTaskStopped,
            executor.clone(),
        );

        // Start the main JSON-RPC task
        info!(target: "darkfid::Darkfid::start", "Starting main JSON-RPC server");
        let node_ = self.node.clone();
        self.rpc_task.clone().start(
            listen_and_serve::<DefaultRpcHandler>(rpc_settings.clone(), self.node.clone(), None, executor.clone()),
            |res| async move {
                match res {
                    Ok(()) | Err(Error::RpcServerStopped) => <DarkfiNode as RequestHandler<DefaultRpcHandler>>::stop_connections(&node_).await,
                    Err(e) => error!(target: "darkfid::Darkfid::start", "Failed starting main JSON-RPC server: {e}"),
                }
            },
            Error::RpcServerStopped,
            executor.clone(),
        );

        // Start the management JSON-RPC task
        info!(target: "darkfid::Darkfid::start", "Starting management JSON-RPC server");
        let node_ = self.node.clone();
        self.management_rpc_task.clone().start(
            listen_and_serve::<ManagementRpcHandler>(management_rpc_settings.clone(), self.node.clone(), None, executor.clone()),
            |res| async move {
                match res {
                    Ok(()) | Err(Error::RpcServerStopped) => <DarkfiNode as RequestHandler<ManagementRpcHandler>>::stop_connections(&node_).await,
                    Err(e) => error!(target: "darkfid::Darkfid::start", "Failed starting management JSON-RPC server: {e}"),
                }
            },
            Error::RpcServerStopped,
            executor.clone(),
        );

        // Start the miners registry
        info!(target: "darkfid::Darkfid::start", "Starting miners registry");
        self.node.registry.start(executor, &self.node, stratum_rpc_settings, mm_rpc_settings)?;

        // Start the P2P network
        info!(target: "darkfid::Darkfid::start", "Starting P2P network");
        self.node.p2p_handler.start(executor, &self.node).await?;

        // Start the consensus protocol
        info!(target: "darkfid::Darkfid::start", "Starting consensus protocol task");
        if is_linear {
            self.consensus_task.clone().start(
                consensus_linear_init_task(
                    self.node.clone(),
                    config.clone(),
                    executor.clone(),
                ),
                |res| async move {
                    match res {
                        Ok(()) | Err(Error::ConsensusTaskStopped) | Err(Error::MinerTaskStopped) => { /* Do nothing */ }
                        Err(e) => error!(target: "darkfid::Darkfid::start", "Failed starting consensus initialization task: {e}"),
                    }
                },
                Error::ConsensusTaskStopped,
                executor.clone(),
            );
        } else {
            self.consensus_task.clone().start(
                consensus_init_task(
                    self.node.clone(),
                    config.clone(),
                    executor.clone(),
                ),
                |res| async move {
                    match res {
                        Ok(()) | Err(Error::ConsensusTaskStopped) | Err(Error::MinerTaskStopped) => { /* Do nothing */ }
                        Err(e) => error!(target: "darkfid::Darkfid::start", "Failed starting consensus initialization task: {e}"),
                    }
                },
                Error::ConsensusTaskStopped,
                executor.clone(),
            );
        }

        info!(target: "darkfid::Darkfid::start", "Darkfi daemon started successfully!");
        Ok(())
    }

    /// Stop the DarkWow daemon.
    pub async fn stop(&self) -> Result<()> {
        info!(target: "darkfid::Darkfid::stop", "Terminating Darkfi daemon...");

        // Stop the `dnet` node
        info!(target: "darkfid::Darkfid::stop", "Stopping dnet subs task...");
        self.dnet_task.stop().await;

        // Stop the main JSON-RPC task
        info!(target: "darkfid::Darkfid::stop", "Stopping main JSON-RPC server...");
        self.rpc_task.stop().await;

        // Stop the management JSON-RPC task
        info!(target: "darkfid::Darkfid::stop", "Stopping management JSON-RPC server...");
        self.management_rpc_task.stop().await;

        // Stop the miners registry
        info!(target: "darkfid::Darkfid::stop", "Stopping miners registry...");
        self.node.registry.stop().await;

        // Stop the P2P network
        info!(target: "darkfid::Darkfid::stop", "Stopping P2P network protocols handler...");
        self.node.p2p_handler.stop().await;

        // Stop the consensus task
        info!(target: "darkfid::Darkfid::stop", "Stopping consensus task...");
        self.consensus_task.stop().await;

        // Flush sled database data
        info!(target: "darkfid::Darkfid::stop", "Flushing sled database...");
        let flushed_bytes =
            self.node.validator.read().await.blockchain.sled_db.flush_async().await?;
        info!(target: "darkfid::Darkfid::stop", "Flushed {flushed_bytes} bytes");

        info!(target: "darkfid::Darkfid::stop", "Darkfi daemon terminated successfully!");
        Ok(())
    }
}
