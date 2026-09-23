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

use std::{
    collections::{HashMap, HashSet},
    sync::{
        atomic::{AtomicU8, AtomicU64, Ordering},
        Arc,
    },
};

use smol::lock::Mutex;
use tracing::{debug, error, info, warn};

use dwow_core::{
    blockchain::HeaderHash,
    net::settings::Settings,
    rpc::{
        jsonrpc::{JsonNotification, JsonSubscriber},
        server::{listen_and_serve, RequestHandler},
        settings::RpcSettings,
    },
    concurrency::{ExecutorPtr, PublisherPtr, StoppableTask, StoppableTaskPtr},
    zkas::ZkBinary,
    Error, Result,
};
use dwow_chain::fee_window::{FeeWindowFlags, compute_fee_v3};
use dwow_chain::monero::JobId;
use dwow_sdk::blockchain::{BlockHeight, BlockReward, BlockTarget, BlockTimestamp, BlockVersion, BlockCharge, FeeAmount, FeeTier, MoneroBlockHeight, RiskFactor};
use dwow_sdk::crypto::keypair::Network;
use dwow_sdk::crypto::DEPLOYOOOR_CONTRACT_ID;

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

/// Consensus/sync/miner async tasks
pub mod task;
use task::{consensus_linear_init_task, ConsensusInitTaskConfig};

/// P2P net protocols
pub mod proto;
use proto::{DwowP2pHandler, DwowP2pHandlerPtr};

/// Miners registry
pub mod registry;
use registry::{DwowMinersRegistry, DwowMinersRegistryPtr};
use crate::registry::model::LinearMinerRewardsRecipientConfig;

// execution.rs moved to dwow_chain::execution

/// Mempool for pending transactions
pub use dwow_accounts as accounts;
// fee_estimator → dwow_chain::fee_estimator
// mempool → dwow_mempool crate
use dwow_mempool::{create_mempool, FeeSignallingExtractor, MempoolPtr, MinerConfig};

/// NativeToken fee extraction — FeeV3 (selector 0x08) uses a plaintext fee.
/// The exact fee is in `FeeParamsV3.fee`; no threshold proof, no encrypted channel.
struct NativeTokenFeeSignallingExtractor;

impl NativeTokenFeeSignallingExtractor {
    fn new() -> Self {
        Self
    }
}

impl FeeSignallingExtractor for NativeTokenFeeSignallingExtractor {
    fn extract_fee(&self, tx: &dwow_chain::Transaction) -> FeeAmount {
        // FeeV3: the fee is plaintext in FeeParamsV3.fee.
        for call in &tx.contract_calls {
            if let Some(mb_fee_v3) = call.as_mass_balance_fee_v3() {
                if let Ok(params) = dwow_native_token_contract::model::fee::FeeParamsV3::decode(
                    mb_fee_v3.params_bytes(),
                ) {
                    return params.fee;
                }
            }
        }
        FeeAmount::ZERO
    }

    fn extract_tier(&self, tx: &dwow_chain::Transaction) -> FeeTier {
        for call in &tx.contract_calls {
            if let Some(mb_fee_v3) = call.as_mass_balance_fee_v3() {
                if let Ok(params) = dwow_native_token_contract::model::fee::FeeParamsV3::decode(
                    mb_fee_v3.params_bytes(),
                ) {
                    return params.tier;
                }
            }
        }
        FeeTier::LOW
    }

    fn declare_charge(&self, tx: &dwow_chain::Transaction) -> BlockCharge {
        // Declarative capacity charge per contract call. This is a structural
        // parameter (like WINDOW_SIZE) — not a fee value. It defines the
        // nameplate rating for block packing, not an economic price.
        // Uses BlockCharge nominal type per type-system.md §2.3.1.
        const DECLARATIVE_CHARGE_PER_CALL: BlockCharge = BlockCharge::new(400_000_000);
        BlockCharge::new(tx.contract_calls.len() as u64 * DECLARATIVE_CHARGE_PER_CALL.get())
    }
}

/// ZK verification for linear blockchain

/// Block-level Pedersen mass balance — proof of token balance
// proof_of_token_balance → dwow_chain::proof_of_token_balance

/// Single unified block acceptance — all five entry points call this
mod block_acceptor;
use block_acceptor::accept_block_with_mempool;

/// Atomic pointer to the DarkWow node
pub type DwowNodePtr = Arc<DwowNode>;

/// Typed sync state machine — replaces raw u8 constants.
///
/// Two states. Storage is `AtomicU8` for lock-free reads from hot paths
/// (miner_task, stratum, RPC); writes are direct `sync_state.store` calls in
/// the consensus sync task (task/consensus_linear.rs), which logs the
/// transition.
///
/// Per type-system.md §9.3, consensus state-machine states SHALL be nominal
/// types, not raw integers. This enum + `AtomicU8` storage provides the same
/// lock-free performance as the raw-u8 approach with type-level safety.
///
/// P2-7: the historical Initial/Syncing/WaitingForGenesis states carried no
/// information the miner could act on — every non-CaughtUp state paused
/// mining identically, and of the three only Initial was ever written (once,
/// at construction). They fold into Behind. The explicit discriminants keep
/// `sync_state_code` wire values stable (CaughtUp=2, Behind=3).
// UNVERIFIED(P2-7): needs cargo test -p dwowd --lib -- daemon_sync_integration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SyncState {
    /// Within range of tip — miner may mine.
    CaughtUp = 2,
    /// Not caught up: initializing, actively pulling blocks, behind peers,
    /// or waiting for genesis. The miner pauses in all of these.
    Behind = 3,
}

impl SyncState {
    /// Load the current sync state from the atomic backing store.
    /// Legacy codes 0/1/4 (Initial/Syncing/WaitingForGenesis) fold into
    /// Behind — P2-7.
    pub fn load(state: &AtomicU8) -> Self {
        match state.load(Ordering::SeqCst) {
            2 => Self::CaughtUp,
            0 | 1 | 3 | 4 => Self::Behind,
            n => {
                tracing::error!("corrupt sync_state value {} — falling back to Behind", n);
                Self::Behind
            }
        }
    }

    /// Human-readable label for the sync state. Used by the
    /// `blockchain.get_sync_state` RPC and pipeline diagnostics.
    pub fn label(code: u8) -> &'static str {
        match code {
            2 => "CaughtUp",
            0 | 1 | 3 | 4 => "Behind", // legacy codes fold (P2-7)
            _ => "Unknown",
        }
    }
}

// ---------------------------------------------------------------------------
// MiningState — block production coordination (stratum, merge-mining, miner RPC)
// Extracted from the DwowNode god object. Single concern: everything related
// to producing blocks via mining.
// ---------------------------------------------------------------------------

/// Typed atomic block timestamp for rate limiting.
/// G3: .get() at atomic boundary — audited.
/// G12: AtomicU64 internal — public API uses BlockTimestamp.
pub struct LastBlockTime(AtomicU64);

impl LastBlockTime {
    pub const fn new() -> Self { Self(AtomicU64::new(0)) }
    pub fn get(&self) -> dwow_sdk::blockchain::BlockTimestamp { dwow_sdk::blockchain::BlockTimestamp::new(self.0.load(Ordering::Acquire)) }
    pub fn set_now(&self) { self.0.store(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs(), Ordering::Release); }
}

/// The block template currently offered to miners, with the chain height it was generated at and the
/// recipient its coinbase pays — **one value under one lock** (`OBL-C43`, `OBL-C54`).
///
/// These were three independently-locked fields (`current_linear_template`, `template_height`,
/// `linear_recipient_config`), written in sequence by four call sites and read at four different moments
/// by the submit paths. A concurrent login could therefore publish a template with the previous round's
/// recipient or height, and a submit could pair one round's template with another's config. The stratum
/// submit path carries a branch that exists only because of that race — `anchor_owner !=
/// wallet.public_key()`, "the live template moved between login and submit" — and a mixed pair is not
/// something to check for when it can be made unrepresentable.
#[derive(Clone)]
pub struct ActiveTemplate {
    /// The template itself.
    pub template: crate::registry::model::LinearBlockTemplate,
    /// Chain height this template was generated at. A submission is rejected as stale against any other
    /// height (type-system.md §9.3 — before PoW verification, not after).
    pub height: dwow_sdk::blockchain::BlockHeight,
    /// The recipient the template's coinbase pays. Deliberately **not** an `Option`: a published template
    /// always has a recipient, so "template without recipient" is not a state this type can hold.
    pub recipient_config: LinearMinerRewardsRecipientConfig,
}

/// Block production state shared between stratum, merge-mining, and miner RPC.
pub struct MiningState {
    /// Last block timestamp for rate limiting
    pub last_block_time: LastBlockTime,
    /// The template currently offered to miners, or `None` before the first login. Written only through
    /// [`MiningState::store_template`], so the three parts cannot be published out of step.
    pub active_template: Mutex<Option<ActiveTemplate>>,
    /// Publisher for pushing stratum job notifications to miners
    pub linear_stratum_publisher: Mutex<Option<PublisherPtr<JsonNotification>>>,
    /// Serializes block submission to prevent concurrent RandomX VM access
    pub linear_submit_lock: Mutex<()>,
    /// Genesis hash for merge-mining RPC
    pub linear_genesis_hash: Mutex<Option<HeaderHash>>,
    /// Active merge mining job IDs (JobId → insertion sequence).
    ///
    /// The value is an insertion sequence, not `()`, so eviction at capacity can evict the **oldest**
    /// job as `merge-mining-ffi.md` §4.4 requires (`OBL-C55`). A HashMap's iteration order is arbitrary,
    /// so the previous `keys().next()` eviction was not FIFO however the comment read.
    pub mm_jobs: Mutex<HashMap<JobId, u64>>,
    /// Submitted merge mining job IDs (dedup), keyed by the same insertion sequence as `mm_jobs`.
    pub mm_jobs_submitted: Mutex<HashMap<JobId, u64>>,
    /// Insertion counter shared by both job tables — one monotonic source, so "oldest" means the same
    /// thing in each and a job's relative age cannot invert between them.
    pub mm_jobs_seq: AtomicU64,
    /// Miner block assembly config — fee policy, gas limits, tx count.
    pub miner_config: MinerConfig,
    /// Sync state machine — gates mining until the node is caught up to peers.
    /// 2=CaughtUp (mine); anything else = Behind (pause miner). Legacy codes
    /// 0/1/4 (Initial/Syncing/WaitingForGenesis) fold into Behind (P2-7).
    /// Written by the consensus sync task (task/consensus_linear.rs), read by
    /// the miner task. Shared (`Arc`) so both tasks hold one handle.
    pub sync_state: Arc<AtomicU8>,
}

impl MiningState {
    pub fn new(sync_state: Arc<AtomicU8>) -> Self {
        Self {
            last_block_time: LastBlockTime::new(),
            active_template: Mutex::new(None),
            linear_stratum_publisher: Mutex::new(None),
            linear_submit_lock: Mutex::new(()),
            linear_genesis_hash: Mutex::new(None),
            mm_jobs: Mutex::new(HashMap::new()),
            mm_jobs_submitted: Mutex::new(HashMap::new()),
            mm_jobs_seq: AtomicU64::new(0),
            miner_config: MinerConfig::default(),
            sync_state,
        }
    }

    /// Publish `template` as the active one, with the height it was generated at and the recipient its
    /// coinbase pays.
    ///
    /// **The only writer of [`MiningState::active_template`]** (`OBL-C43`, `OBL-C54`). Four call sites
    /// used to write the three fields themselves, in sequence, and the pairing was left to each of them:
    /// the stratum login wrote all three, the merge-mining login wrote the template and the height but
    /// *dropped* the recipient it had just resolved, and both submit paths regenerated a template and then
    /// depended on a recipient having been stored by some earlier stratum login. Passing them together
    /// makes the publication atomic and removes the ordering question.
    pub async fn store_template(
        &self,
        template: crate::registry::model::LinearBlockTemplate,
        height: BlockHeight,
        recipient_config: LinearMinerRewardsRecipientConfig,
    ) {
        *self.active_template.lock().await = Some(ActiveTemplate { template, height, recipient_config });
    }
}

// ---------------------------------------------------------------------------
// RpcState — JSON-RPC connection lifecycle
// ---------------------------------------------------------------------------

/// RPC connection tracking and event subscribers.
pub struct RpcState {
    /// Event subscribers (blocks, dnet)
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
    /// Dynamic fee estimator (tracks block gas utilization)
    pub fee_estimator: Arc<dwow_chain::fee_estimator::FeeEstimator>,
    /// P2P network protocols handler
    pub p2p_handler: DwowP2pHandlerPtr,
    /// Node miners registry pointer
    pub registry: DwowMinersRegistryPtr,
    /// RPC connection lifecycle
    pub rpc_state: Arc<RpcState>,
    /// Mining / block production state
    pub mining_state: Arc<MiningState>,
    /// Whether node is running with easy mining (trusted local environment).
    /// Gates miner control RPC methods.
    pub is_localnet: bool,
    /// Minimum interval between blocks in seconds
    pub min_block_interval: u64,
    /// Account manager — the node's declared identity, derived on boot.
    pub account_manager: Arc<smol::lock::RwLock<crate::accounts::AccountManager>>,
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
        account_manager: Arc<smol::lock::RwLock<crate::accounts::AccountManager>>,
        sync_state: Arc<AtomicU8>,
    ) -> Result<DwowNodePtr> {
        Ok(Arc::new(Self {
            chain_state,
            mempool,
            p2p_handler,
            registry,
            rpc_state: Arc::new(RpcState::new(subscribers)),
            mining_state: Arc::new(MiningState::new(sync_state)),
            is_localnet,
            min_block_interval,
            account_manager,
            fee_estimator: Arc::new(dwow_chain::fee_estimator::FeeEstimator::default()),
        }))
    }

    // UNVERIFIED(HYG-3-2): removed pub fn is_localnet — zero callers repo-wide
    // (the underlying is_localnet field is still read by rpc/miner.rs directly);
    // needs cargo check -p dwowd -j 2
    // UNVERIFIED(HYG-3-3): removed pub fn mempool — zero callers repo-wide;
    // needs cargo check -p dwowd -j 2
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
    /// Mempool maintenance task — periodic stale eviction every 5 minutes
    /// (MOC close-out item 10). Without this, eviction only fires on add().
    mempool_task: StoppableTaskPtr,
    /// Whether built-in mining is enabled. Observer/relay nodes set false.
    mining_enabled: bool,
}

/// Build the 9 genesis contract-deployment transactions (hard-coded genesis).
///
/// The genesis block CARRIES the contract deployments — P2P sync provides
/// EVERYTHING a syncing node needs. The WASM blobs are compiled into the
/// binary here, but they are only materialized into chain state by the
/// genesis-deployment consensus rule
/// (`dwow_chain::execution::apply_genesis_deployments`) when the genesis
/// block is executed — identically on the authority that builds it and on
/// every node that syncs it. No node deploys contracts at startup.
///
/// Transactions are built in `dwow_chain::execution::genesis_contracts()`
/// order (genesis.md Bootstrap Sequence — order is consensus). Each carries
/// one `ContractCall` to Deployooor with the DeployV1 selector (`0x00`)
/// followed by `DeployParamsV1`:
/// - `wasm_bincode`: the contract WASM
/// - `ix`: the manifest.toml bytes (wallet-facing metadata). DELIBERATELY EMPTY
///   for Deployooor and NativeToken, both of which DO have a `manifest.toml` on
///   disk: the wallet handles those two natively (Path 1 — see wallet.md §6.4)
///   rather than through manifest-declared capability discovery, so deploying
///   their manifests would make the wallet scan them twice. The on-disk manifests
///   are maintained as documentation of the interface and are not consensus
///   artefacts. (The comment here previously claimed those contracts "have no
///   manifests", which was false.)
/// - `public_key`: a fixed deterministic key — the consensus rule binds
///   deployments to the well-known ContractIds by table position, NOT by
///   `derive_public`; the key only needs to be deterministic because it is
///   part of the hash preimage
/// - `singleton`/`singleton_name`: position binding verified by the rule
fn build_genesis_deployment_txs() -> Vec<dwow_chain::Transaction> {
    use dwow_sdk::deploy::DeployParamsV1;

    // WASM + manifest table, in genesis_contracts() order.
    let contracts: Vec<(&[u8], &[u8], &str)> = vec![
        (include_bytes!("../../../src/contract/deployooor/dwow_deployooor_contract.wasm"), &[], "Deployooor"),
        (include_bytes!("../../../src/contract/native_token/dwow_native_token_contract.wasm"), &[], "NativeToken"),
        (include_bytes!("../../../src/contract/promissory_note/dwow_promissory_note_contract.wasm"), include_bytes!("../../../src/contract/promissory_note/manifest.toml"), "PromissoryNote"),
        (include_bytes!("../../../src/contract/identity/dwow_identity_contract.wasm"), include_bytes!("../../../src/contract/identity/manifest.toml"), "Identity"),
        (include_bytes!("../../../src/contract/oracle/dwow_oracle_contract.wasm"), include_bytes!("../../../src/contract/oracle/manifest.toml"), "Oracle"),
        (include_bytes!("../../../src/contract/attestation/dwow_attestation_contract.wasm"), include_bytes!("../../../src/contract/attestation/manifest.toml"), "Attestation"),
        (include_bytes!("../../../src/contract/purse/dwow_purse_contract.wasm"), include_bytes!("../../../src/contract/purse/manifest.toml"), "Purse"),
        (include_bytes!("../../../src/contract/box/dwow_box_contract.wasm"), include_bytes!("../../../src/contract/box/manifest.toml"), "Box"),
        (include_bytes!("../../../src/contract/multisig/dwow_multisig_contract.wasm"), include_bytes!("../../../src/contract/multisig/manifest.toml"), "MultiSig"),
    ];

    // Fixed deterministic key — the genesis rule ignores it (binding is by
    // the hard-coded table), but it sits in the tx hash preimage so it must
    // be identical on every build.
    let public_key = dwow_sdk::crypto::PublicKey::from_secret(
        dwow_sdk::crypto::SecretKey::from_base(dwow_sdk::pasta::pallas::Base::from(1u64)),
    );

    contracts
        .into_iter()
        .map(|(wasm, manifest, name)| {
            let params = DeployParamsV1 {
                wasm_bincode: wasm.to_vec(),
                public_key,
                ix: manifest.to_vec(),
                singleton: true,
                singleton_name: name.to_string(),
            };
            // DeployV1 selector convention: data[0] == 0x00, then params.
            let mut data = vec![0x00u8];
            data.extend_from_slice(&dwow_serial::serialize(&params));
            dwow_chain::Transaction {
                version: BlockVersion::CURRENT,
                inputs: vec![],
                outputs: vec![],
                contract_calls: vec![dwow_chain::ContractCall {
                    contract_id: *DEPLOYOOOR_CONTRACT_ID,
                    data,
                }],
                lock_time: 0,
                nullifiers: vec![],
                witness: vec![],
            }
        })
        .collect()
}

/// Create the genesis block with a proper coinbase (Bitcoin-style).
///
/// The genesis coinbase sends the first PoW reward to the miner as a plaintext
/// `pow_reward_v1` contract call (0x05, no ZK proof since b6bf44f79). The
/// nullifier `nf = poseidon_hash(sk_H, C)` IS the block's validity proof — the
/// account's capability claim, not a public key signature.
///
/// Genesis follows the same block construction path as every other block:
/// build coinbase → execute WASM → read cumulative supply → atomic commit.
/// No special bootstrap case. The cumulative supply chain starts here:
/// S_1 = identity + C_1 where C_1 commits to INITIAL_REWARD.
///
/// Split out of [`init_genesis`] so the genesis pin can be checked without
/// executing genesis. `init_genesis` runs the nine deployments through the WASM
/// VM — building a halo2 verifying key for every circuit — and then commits
/// block 1; that path measured 497 seconds, and none of it affects the hash,
/// because `accept_block` takes `&Block` and so cannot change what is hashed.
/// Building the block and hashing it is therefore exactly equivalent to the full
/// path, and costs seconds instead of minutes.
///
/// Genesis PoW is a formality (`target` is `BlockTarget::MAX`), the timestamp is
/// fixed at 0, and `miner` is zeroed, so every node derives one block from the
/// same inputs.
async fn build_genesis_block(
    prev_entry: &dwow_chain::CumulativeSupplyEntry,
    recipient: crate::accounts::MiningRecipient,
    magic_bytes: [u8; 4],
) -> Result<dwow_chain::Block> {
    use dwow_chain::{Block, BlockHeader, Miner, PowSource, Transaction};
    use dwow_sdk::blockchain::expected_reward;

    let genesis_height = BlockHeight::GENESIS;
    let target = BlockTarget::MAX;
    // Deterministic genesis timestamp — must be identical across all nodes.
    let timestamp = 0u64;

    // Genesis block reward — same as every other block. One emission schedule.
    let genesis_reward = expected_reward(genesis_height);

    // Build plaintext coinbase (no ZK — b6bf44f79): the reward value is in the clear
    // (effective_value, total_pin) and the note's value is bound to consensus by the
    // plaintext note preimage, so no amount is concealed. The Output still carries an
    // AEAD note record for wallet discovery, encrypted with a DERIVED ephemeral key
    // (consensus-coinbase.md §2.7 "no random keys") — not a random one. The recipient's
    // per-block derived secret sk_H is used for nullifier computation:
    // nf = poseidon_hash(sk_H.inner(), C). Same code path as every subsequent block.
    //
    // Calls the builder that takes values rather than a store handle, so this function has no
    // store of its own: the cumulative supply state arrives as `prev_entry` (at genesis the
    // identity state) and nothing else is read. No clock, no RNG, no network.
    let (coinbase, pow_reward_call, _coin_blind) =
        crate::registry::model::build_linear_coinbase_effective(
            recipient,
            genesis_reward,
            genesis_reward, // no uncles at genesis ⇒ effective == full value
            prev_entry,
            genesis_height,
        )
        .await?;

    tracing::info!(
        target: "dwowd::Dwowd::init_genesis",
        "Genesis coinbase built: reward={} commitment=0x{} nullifier=0x{}",
        genesis_reward,
        hex::encode(coinbase.commitment.to_bytes()),
        hex::encode(coinbase.nullifier.to_bytes()),
    );

    // Genesis transaction: PoWRewardV1 at transactions[0].contract_calls[0].
    // This IS the block's validity proof — the nullifier proves the miner
    // controls sk_H, the per-block derived secret.
    let genesis_tx = Transaction {
        version: BlockVersion::CURRENT,
        inputs: vec![],
        outputs: vec![],
        contract_calls: vec![pow_reward_call],
        lock_time: 0,
        nullifiers: vec![coinbase.nullifier],
        witness: vec![],
    };

    // The genesis block CARRIES the 9 contract deployments (positions 1..=9,
    // coinbase at 0 per validate_block_structure). P2P sync provides
    // EVERYTHING — a syncing node materializes contracts by executing this
    // block via the same apply_genesis_deployments consensus rule that runs
    // here through accept_block below. The genesis hash now pins the full
    // contract set (WASM + manifests ride in transactions → merkle root).
    let mut transactions = vec![genesis_tx];
    transactions.extend(build_genesis_deployment_txs());
    let genesis_merkle_root = dwow_chain::compute_merkle_root(&transactions);

    // Embed network magic bytes in genesis block anchor field.
    let mut anchor_tx_id = [0u8; 32];
    anchor_tx_id[0..4].copy_from_slice(&magic_bytes);

    let header = BlockHeader {
        version: BlockVersion::CURRENT,
        previous: blake3::Hash::from_bytes([0u8; 32]),
        merkle_root: genesis_merkle_root,
        timestamp: BlockTimestamp::new(timestamp),
        target,
        nonce: 0,
        height: genesis_height,
        uncle_merkle_root: [0u8; 32],
        total_reward: genesis_reward,
        randomx_key: Miner::derive_key_from_height(genesis_height),
        miner: [0u8; 32],
        // Genesis is unanchored and never mined for PoW, so it commits no anchor author.
        anchor_owner: [0u8; 32],
        commitment_merkle_root: [0u8; 32],
        nullifier_root: [0u8; 32],
        anchor_tx_id,
        caribina_anchor: None,
        anchor_monero_height: MoneroBlockHeight::new(0),
        anchor_monero_hash: [0u8; 32],
        finality_flags: 0,
        fee_window_flags: FeeWindowFlags::default(),
        pow_source: PowSource::Native,
    };

    Ok(Block { header, transactions })
}

/// Create the genesis block and commit it through the standard acceptance path.
///
/// This is the node's genesis path. Returns the genesis block hash for
/// merge-mining RPC and P2P broadcasting.
async fn init_genesis(
    chain_state: &Arc<dwow_chain::CChainState>,
    recipient: crate::accounts::MiningRecipient,
    magic_bytes: [u8; 4],
) -> Result<HeaderHash> {
    // ── Stage 1 (effect): read the cumulative supply state ────────────────────────────────
    // The one effect the value path needs, named here at the boundary. Genesis has committed
    // no entries, so this is `CumulativeSupplyEntry::genesis()` — the identity state.
    let prev_entry = chain_state.supply_chain.get_latest();
    // ── Stage 2 (pure): build the block ───────────────────────────────────────────────────
    // A function of (prev_entry, recipient, magic_bytes): it reads no store, no clock, no RNG.
    let genesis_block = build_genesis_block(&prev_entry, recipient, magic_bytes).await?;
    let target = BlockTarget::MAX;

    // Create RandomX VM for WASM execution.
    // Genesis PoW is a formality: target = u32::MAX → any hash passes.
    let rx_flags =
        randomx::RandomXFlags::get_recommended_flags() & !randomx::RandomXFlags::JIT;
    let rx_cache = randomx::RandomXCache::new(rx_flags, &genesis_block.header.randomx_key)
        .map_err(|e| Error::Custom(format!("Genesis RandomX cache: {}", e)))?;
    let vm = Arc::new(
        randomx::RandomXVM::new(rx_flags, Some(rx_cache), None)
            .map_err(|e| Error::Custom(format!("Genesis RandomX VM: {}", e)))?,
    );

    // Execute and commit genesis through the standard block acceptance path.
    // This runs WASM (pow_reward_v1), reads cumulative supply from overlay,
    // and commits block + contracts + supply_chain atomically.
    // `accept_block_with_mempool`, with an explicit `None`: no mempool exists yet — genesis is accepted
    // before the mempool is created, so a reorg triggered here has nowhere to return transactions to
    // (`OBL-C44`), and the call site says so rather than relying on a default.
    crate::block_acceptor::accept_block_with_mempool(
        chain_state,
        &genesis_block,
        &[],
        &vm,
        target,
        None,
        None,
    )
    .map_err(|e| Error::Custom(format!("Genesis block acceptance failed: {}", e)))?;

    // Fee accumulator verification after genesis: the production init_genesis
    // cannot directly query contract state (sled tree handles are private).
    // Verification is performed by:
    //   - GenesisHarness::init_genesis() in tests/genesis.rs (Step 3)
    //   - HeavyweightPipeline::init_genesis() in tests/blockchain.rs (Step 3)
    //   - test_genesis_determinism AC-FEE-1/2 assertions
    //   - test_block_creation AC-FEE-4 stranded-fee canary
    // Per genesis.md Structural Identity §Fee lifecycle.

    // A RandomX hash failure is a typed error, not a panic. The `#[expect]` this replaces justified
    // itself by citing "safety.md C1" — but C1 is that document's *finding* that the RandomX
    // `.expect()` sites make "the node panic under memory pressure / bad key", listed in its
    // prioritised remediation order. The dispensation was citing the hazard as its licence, and
    // removing it here retires one of the sites that finding names.
    let genesis_hash = chain_state
        .hash_block_with_cached_vm(&genesis_block)
        .map_err(|e| Error::Custom(format!("Genesis block hashing failed: {e}")))?;

    // The pin is deliberately NOT enforced here. This function is the tests'
    // genesis fixture as well as the node's genesis path, and a mismatch at
    // this line aborted every test that builds a genesis block before that
    // test reached its subject: 98 of 124 `dwowd --lib` tests failed, and not
    // one of them named the pin. Enforcement lives in `check_genesis_pin`,
    // called from the node startup path in `init_linear`; the pin's own state
    // is asserted by `genesis_pin_is_current` in tests/genesis.rs.

    // The coinbase commitment and nullifier are logged by `build_genesis_block`,
    // which is where the coinbase is built; this line reports the block it
    // produced and the hash over it.
    info!(
        target: "dwowd::Dwowd::init_linear",
        "Genesis block created at height 1: hash={}",
        genesis_hash,
    );

    Ok(HeaderHash(genesis_hash.into()))
}

/// The genesis hash this build expects, or `None` while
/// `bin/dwowd/genesis_hash.txt` still holds the all-zeros placeholder.
///
/// The file is read at compile time, so a binary cannot disagree with the pin
/// it was compiled against.
pub(crate) fn pinned_genesis_hash() -> Option<&'static str> {
    let expected_hex = include_str!("../genesis_hash.txt").trim();
    if expected_hex.chars().all(|c| c == '0') {
        None
    } else {
        Some(expected_hex)
    }
}

/// Compare a genesis hash against the compile-time pin.
///
/// `what` names which hash this is, so the failure is actionable: "computed"
/// while creating genesis, "stored" when reopening an existing datadir.
///
/// This is the ONLY place the pin is enforced, and it is called from the node
/// startup path (`Dwowd::init_linear`) — never from `init_genesis`, which is
/// also the tests' genesis fixture. Genesis is a production-critical event, so
/// the pin has to bite on the path that creates and adopts a chain; it must not
/// sit inside the shared fixture, where its failure is indistinguishable from
/// the test's own failure.
pub(crate) fn check_genesis_pin(hash: &[u8; 32], what: &str) -> Result<()> {
    let Some(expected_hex) = pinned_genesis_hash() else {
        tracing::warn!(
            target: "dwowd::Dwowd::init_linear",
            "Genesis hash placeholder (all zeros) — the pin is not set. {} hash: {}. \
             Copy it into bin/dwowd/genesis_hash.txt to enable verification.",
            what,
            hex::encode(hash),
        );
        return Ok(());
    };

    // Compare BYTES, not rendered strings. `genesis_hash.txt` holds lowercase
    // hex, while the `HeaderHash` newtype renders the same 32 bytes as base58 —
    // comparing Display output would reject a perfectly correct pin and stop the
    // node from ever starting.
    let expected: [u8; 32] = hex::decode(expected_hex)
        .map_err(|e| Error::Custom(format!(
            "bin/dwowd/genesis_hash.txt is not valid hex: {e}")))?
        .as_slice()
        .try_into()
        .map_err(|_| Error::Custom(
            "bin/dwowd/genesis_hash.txt must be 64 hex characters (32 bytes)".into()))?;

    if *hash == expected {
        return Ok(());
    }
    error!(
        target: "dwowd::Dwowd::init_linear",
        "GENESIS HASH MISMATCH ({}): got={} expected={}",
        what,
        hex::encode(hash),
        expected_hex
    );
    // The remedy depends on which hash disagreed: a stored genesis is almost
    // always a datadir from another network or build, while a freshly computed
    // one means this build's genesis inputs moved.
    let remedy = if what == "stored" {
        "This datadir belongs to a different network or build. Wipe it and \
         resync — or, if this build legitimately moved genesis, re-record \
         bin/dwowd/genesis_hash.txt from a fresh ceremony."
    } else {
        "The genesis inputs have changed — contract WASM bytes, the genesis \
         timestamp, or the genesis key. Rebuild the contracts from clean, \
         re-run the genesis ceremony, and record the new hash in \
         bin/dwowd/genesis_hash.txt."
    };
    Err(Error::Custom(format!(
        "Genesis hash does not match the compiled-in pin ({what}): \
         got={} expected={expected_hex}. {remedy}",
        hex::encode(hash),
    )))
}

impl Dwowd {
    /// Initialize a DarkWow daemon for darkwow-devnet mode.
    ///
    /// Consensus runs on the shared `CChainState` + the consensus_linear task.
    pub async fn init_linear(
        network: Network,
        sled_db: &sled::Db,
        net_settings: &Settings,
        ex: &ExecutorPtr,
        finality_config: Option<dwow_chain::FinalityConfig>,
        create_genesis: bool,
        keys_toml: Option<&std::path::Path>,
        mining_enabled: bool,
    ) -> Result<DwowdPtr> {
        info!(target: "dwowd::Dwowd::init_linear", "Initializing a DarkWow daemon for darkwow-devnet...");

        let finality_config = finality_config.unwrap_or_default();
        info!(target: "dwowd::Dwowd::init_linear", "Finality mode: {:?}, caribina_enabled: {}", finality_config.mode, finality_config.caribina_enabled);

        // Create PoW config from network settings.
        // When mining_easy is true, override with easy difficulty for CPU-limited
        // environments (local dockernet) — ensures blocks are produced even on a
        // single thread.
        let pow_config = if net_settings.mining_easy {
            info!(target: "dwowd::Dwowd::init_linear", "mining_easy=true: using easy mining difficulty");
            dwow_chain::PoWConfig {
                target_block_time: net_settings.pow.target_block_time.unwrap_or(10),
                initial_target: BlockTarget::new(net_settings.pow.initial_target.unwrap_or(u32::MAX) as u32),
                min_target: BlockTarget::new(net_settings.pow.min_target.unwrap_or(u32::MAX) as u32),
                max_target: BlockTarget::new(net_settings.pow.max_target.unwrap_or(u32::MAX) as u32),
            }
        } else {
            dwow_chain::PoWConfig {
                target_block_time: net_settings.pow.target_block_time.unwrap_or(120),
                initial_target: BlockTarget::new(net_settings.pow.initial_target.unwrap_or(0x0FFFFFFF) as u32),
                min_target: BlockTarget::new(net_settings.pow.min_target.unwrap_or(1) as u32),
                max_target: BlockTarget::new(net_settings.pow.max_target.unwrap_or(u32::MAX) as u32),
            }
        };

        // Single authoritative chain state.
        // CChainState provides: store, consensus, VM pool, commitment/nullifier sets.
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

        // NO contract deployment at startup — genesis carries it.
        // The 9 genesis contracts ride in the genesis block as deployment
        // transactions (build_genesis_deployment_txs) and are materialized
        // exclusively by the genesis-deployment consensus rule when the
        // block executes: on THIS node below if create_genesis=true, or on
        // first sync from the network otherwise. A node either creates
        // genesis or syncs it — never both.

        // Resolve mining identity from the owner's declaration (keys.toml section),
        // deterministically, on every boot. NO sled cache, NO auto-generation: the
        // section is this node's NODE_NAME (required — no silent default), and a
        // missing section is a hard error. MUST happen before genesis — the coinbase
        // reward is bound to this key.
        let section_name = std::env::var("NODE_NAME").map_err(|_| Error::Custom(
            "NODE_NAME not set: each node must declare which keys.toml section is its identity".into()))?;
        let keys_toml_path = keys_toml.ok_or_else(|| Error::Custom(
            "No --keys keys.toml provided: every node must declare its key".into()))?;
        let account_mgr = crate::accounts::AccountManager::open(
            keys_toml_path, network, &section_name,
        )
            .map_err(|e| Error::Custom(format!("AccountManager: {e}")))?;

        let genesis_public_key = account_mgr.default_public_key()
            .map_err(|e| Error::Custom(format!("AccountManager: {e}")))?;
        let secret_count = account_mgr.secrets().len();
        info!(target: "dwowd::init_linear",
            "Mining key resolved: section=[{}] secrets={} public={}",
            section_name, secret_count,
            hex::encode(genesis_public_key.to_bytes()),
        );
        let account_mgr = Arc::new(smol::lock::RwLock::new(account_mgr));

        // Create mempool early — needed by both the P2P handler (for cleanup)
        // and the miner RPC (for transaction submission).
        let mempool = Some(create_mempool(
            Box::new(NativeTokenFeeSignallingExtractor::new()),
            Some(chain_state.clone()),
        ));

        // Shared sync-state handle — the sync task, the miner gate, and the
        // block-broadcast handler all read/write the SAME AtomicU8, so a pushed
        // block can mark CaughtUp directly (production "mine when you hold the
        // best chain").
        // P2-7: non-CaughtUp states fold into Behind.
        // UNVERIFIED(P2-7): needs cargo test -p dwowd --lib -- daemon_sync_integration
        let sync_state = Arc::new(AtomicU8::new(SyncState::Behind as u8));

        // Initialize P2P network.
        // - chain_state → single source of truth for both sync and broadcast handlers
        // - sled_database → passed so the event-graph opens its tree inside
        //   the same physical sled::Db (tree-level quarantine per §10.4).
        let p2p_handler = DwowP2pHandler::init(
            net_settings,
            ex,
            Some(chain_state.clone()),
            Some(chain_state.clone()),
            mempool.clone(),
            Some(sled_db.clone()),
        ).await?;

        // Initialize the miners registry (placeholder for now)
        let registry = DwowMinersRegistry::init_linear(network, chain_state.clone()).await?;

        // Genesis block creation with proper coinbase (Bitcoin-style).
        // Construct MiningRecipient from the AccountManager BEFORE calling
        // init_genesis. This separates Concern 1 (account management) from
        // Concern 2 (block construction / nullifier signing).
        let linear_genesis_hash: HeaderHash = if create_genesis {
            // Authority restart guard: a persistent volume already holds
            // genesis — verify it against the compile-time pin instead of
            // re-creating (accept_block would reject a duplicate height 1).
            if chain_state.get_height() >= BlockHeight::GENESIS {
                let stored_genesis = chain_state.get_block(BlockHeight::GENESIS)
                    .map_err(|e| Error::Custom(format!(
                        "height >= 1 but genesis block unreadable: {e}")))?;
                // Same class and same stale citation as the genesis path above: a datadir whose
                // stored genesis cannot be re-hashed is a typed startup error, not a panic.
                let stored_hash = chain_state.hash_block_with_cached_vm(&stored_genesis)
                    .map_err(|e| Error::Custom(format!(
                        "stored genesis block hashing failed: {e}")))?;
                // A datadir that already holds genesis is held to the same pin as
                // a freshly created one, so a database belonging to a different
                // network or build is refused rather than silently reused.
                check_genesis_pin(stored_hash.as_bytes(), "stored")?;
                let stored = HeaderHash(stored_hash.into());
                info!(
                    target: "dwowd::Dwowd::init_linear",
                    "Genesis already exists (height {}), reusing stored genesis hash={}",
                    chain_state.get_height(), stored_hash,
                );
                stored
            } else {
                let magic_bytes = net_settings.magic_bytes.0;
                let acct_guard = account_mgr.read().await;
                let recipient = crate::accounts::MiningRecipient::from_account(
                    &acct_guard, BlockHeight::GENESIS,
                )
                .map_err(|e| Error::Custom(format!("MiningRecipient for genesis: {}", e)))?;
                drop(acct_guard); // release lock before async ZK work
                let created = init_genesis(&chain_state, recipient, magic_bytes).await?;
                check_genesis_pin(&created.0, "computed")?;
                created
            }
        } else {
            info!(
                target: "dwowd::Dwowd::init_linear",
                "Skipping genesis creation — will sync from network"
            );
            HeaderHash([0u8; 32])
        };

        // Here we initialize various subscribers that can export live blockchain/consensus data.
        // Only "blocks" (rpc/blockchain.rs) and "dnet" (rpc/management.rs) are ever
        // retrieved; the tx/proposal subscription endpoints were never implemented.
        let mut subscribers = HashMap::new();
        subscribers.insert("blocks", JsonSubscriber::new("blockchain.subscribe_blocks"));
        subscribers.insert("dnet", JsonSubscriber::new("dnet.subscribe_events"));

        let min_block_interval = net_settings.pow.min_block_interval.unwrap_or(10);
        let node = DwowNode::new(
            Some(chain_state),
            mempool,
            p2p_handler,
            registry,
            subscribers,
            net_settings.mining_easy,
            min_block_interval,
            account_mgr,
            sync_state,
        ).await?;

        // Store genesis hash for mm_rpc
        node.mining_state.linear_genesis_hash.lock().await.replace(linear_genesis_hash);

        // Generate the background tasks
        let dnet_task = StoppableTask::new();
        let rpc_task = StoppableTask::new();
        let management_rpc_task = StoppableTask::new();
        let consensus_task = StoppableTask::new();
        let miner_task = StoppableTask::new();
        let mempool_task = StoppableTask::new();

        info!(target: "dwowd::Dwowd::init_linear", "DarkWow daemon for darkwow-devnet initialized successfully!");

        Ok(Arc::new(Self { node, dnet_task, rpc_task, management_rpc_task, consensus_task, miner_task, mempool_task, mining_enabled }))
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
        let dnet_sub_ = self.node.rpc_state.subscribers
            .get("dnet")
            .ok_or_else(|| Error::Custom("dnet subscriber not initialized".into()))?
            .clone();
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
            if linear_chain.get_height() >= BlockHeight::GENESIS {
                if let Ok(genesis) = linear_chain.get_block(BlockHeight::GENESIS) {
                    info!(target: "dwowd::Dwowd::start",
                        "Broadcasting genesis to connected peers...");
                    crate::proto::linear_broadcast::broadcast_block(
                        &self.node.p2p_handler.p2p,
                        genesis.clone(),
                        vec![],
                    ).await;
                    // NO DAG announce for genesis: the genesis block carries
                    // the 9 contract deployments (multi-MB WASM payload) and
                    // is exempt from the block size cap. Flood broadcast +
                    // GetBlocks sync are the load-bearing delivery paths for
                    // genesis; the DAG substrate (§10.4) carries only
                    // post-genesis blocks.
                }
            }
        }

        // Start the consensus protocol (linear mode)
        info!(target: "dwowd::Dwowd::start", "Starting consensus protocol task");
        let consensus_node = self.node.clone();
        self.consensus_task.clone().start(
            consensus_linear_init_task(
                self.node.clone(),
                config.clone(),
                executor.clone(),
            ),
            move |res| {
                let node = consensus_node.clone();
                async move {
                match res {
                    Ok(()) | Err(Error::ConsensusTaskStopped) | Err(Error::MinerTaskStopped) => { /* Do nothing */ }
                    Err(e) => {
                        error!(target: "dwowd::Dwowd::start", "Consensus initialization task failed: {e}");
                        error!(target: "dwowd::Dwowd::start",
                            "CONSENSUS TASK CRASHED — sync_state set to Behind. \
                             Mining is permanently disabled until node restart. \
                             This is a terminal error; the node cannot self-recover.");
                        // HAZOP F5: consensus task failure must pause the miner.
                        // Without this, the miner continues with stale state.
                        node.mining_state.sync_state.store(SyncState::Behind as u8, Ordering::SeqCst);
                    }
                }
                }
            },
            Error::ConsensusTaskStopped,
            executor.clone(),
        );

        // Start the built-in miner (replaces the bash /dev/tcp loop)
        if self.mining_enabled {
            info!(target: "dwowd::Dwowd::start", "Starting built-in miner task");
            let miner_node = self.node.clone();
            self.miner_task.clone().start(
                miner_task(miner_node),
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
        } else {
            info!(target: "dwowd::Dwowd::start", "Mining disabled — relay-only mode");
        }

        // Start the mempool maintenance task — periodic stale eviction every 5
        // minutes (MOC close-out item 10). Without this, eviction only fires on
        // add(), so a quiet mempool retains stale txs + nullifiers indefinitely.
        {
            let mempool = self.node.mempool.clone();
            self.mempool_task.clone().start(
                async move {
                    loop {
                        smol::Timer::after(std::time::Duration::from_secs(300)).await;
                        if let Some(mp) = mempool.as_ref() {
                            let evicted = mp.evict_stale().await;
                            if evicted > 0 {
                                info!(target: "dwowd::mempool_task",
                                    "Evicted {} stale transactions", evicted);
                            }
                        }
                    }
                },
                |_| async {}, // no completion handler — runs forever
                Error::ConsensusTaskStopped, // harmless sentinel, never reached
                executor.clone(),
            );
        }

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
        self.mempool_task.stop().await;

        // Flush linear blockchain store
        info!(target: "dwowd::Dwowd::stop", "Flushing sled database...");
        if let Some(ref chain) = self.node.chain_state {
            let _ = chain.store.flush();
            info!(target: "dwowd::Dwowd::stop", "Flushed linear blockchain store");
        }

        // No AccountManager persistence: the identity is re-derived deterministically
        // from the owner's declaration (keys.toml) on every boot. There is no runtime
        // key state to flush — nothing is cached or mutated at runtime.

        info!(target: "dwowd::Dwowd::stop", "DarkWow daemon terminated successfully!");
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// ---------------------------------------------------------------------------
// Unified Block Preparation — single source of truth for all 4 mining paths
// ---------------------------------------------------------------------------

/// Result of block preparation: all assembled parts ready for mining + acceptance.
struct PreparedBlock {
    uncles: Vec<dwow_chain::UncleBlock>,
    competing_originals: Vec<dwow_chain::Block>,
    mempool_txs: Vec<dwow_chain::Transaction>,
    coinbase_tx: dwow_chain::Transaction,
    /// FeeCollectV1 transaction — final transaction, closes the merkle tree
    /// (consensus-coinbase.md §3). Plaintext since 2026-09: no ZK proof. Carries
    /// one empty per-call proof slot in the witness (same L1 carriage shape as
    /// user transactions) and the fee nullifier in tx.nullifiers. None iff no
    /// fees were paid in this block.
    fee_collect_tx: Option<dwow_chain::Transaction>,
}

/// Prepare a block for mining — collects uncles, builds coinbase, selects txs.
/// Called by all four mining entry points. Replaces 4-way duplicate code.
async fn prepare_block(
    chain_state: &Arc<dwow_chain::CChainState>,
    mining_state: &MiningState,
    mempool: Option<&Arc<dwow_mempool::Mempool>>,
    recipient: crate::accounts::MiningRecipient,
    height: BlockHeight,
    base_reward: BlockReward,
) -> Result<PreparedBlock> {
    use crate::registry::model::{build_linear_coinbase_effective, build_uncle_mint_tx};
    use dwow_chain::UncleBlock;

    // 0. Peek competing blocks (read-only) + build uncles with accept_pin, so the
    //    coinbase can be minted at the reduced effective value (uncle split).
    //    Spec: uncle_merkle.md §Uncle Minting & Maturity — "Canonical note reduction".
    let latest_height = chain_state.get_height();
    let competing_originals = chain_state.peek_competing_blocks(latest_height);
    let uncles: Vec<UncleBlock> = competing_originals.iter().map(|block| {
        let depth = dwow_chain::UncleBlock::depth_for(height, block.header.height);
        let mut uncle = dwow_chain::create_uncle(block.clone(), depth, base_reward);
        uncle.accept_pin(); // "rejection is strictly dominated" — the uncle always accepts
        uncle
    }).collect();
    // Single implementation of Σ pin, shared with the host check and the
    // connect-time split check (`dwow_chain::total_accepted_pin`).
    let total_pin = dwow_chain::total_accepted_pin(&uncles)
        .map_err(|e| dwow_core::Error::Custom(format!("Σ pin: {e}")))?
        .get();
    let effective_value = BlockReward::new(base_reward.get().checked_sub(total_pin).ok_or_else(
        || dwow_core::Error::Custom(format!(
            "Σ pin {total_pin} exceeds base reward {base_reward}"
        )),
    )?);

    // 1. Build the plaintext coinbase FIRST — fallible operation.
    //    No destructive state mutation yet (we only PEEKED competing blocks).
    //    Clone: `recipient` is used again in step 6 (build_fee_collect_tx —
    //    same sk_H for coinbase and fee collection, spec §3.2).
    // The store read happens here, at the edge; the builder is a function of values.
    let prev_entry = chain_state.supply_chain.get_latest();
    let (_, pow_reward_call, _coin_blind) = build_linear_coinbase_effective(
        recipient.clone(),
        base_reward,
        effective_value,
        &prev_entry,
        height,
    ).await?;

    // 2. Select mempool transactions (infallible)
    let mempool_txs = if let Some(m) = mempool {
        m.select_for_block(&mining_state.miner_config).await
    } else {
        Vec::new()
    };

    // 3. Filter immature coinbase spends (soft gate, infallible)
    let mut mempool_txs: Vec<_> = mempool_txs.into_iter().filter(|tx| {
        // P2-6: selector-only soft filter — keep the structural predicate.
        // UNVERIFIED(P2-6): needs cargo test -p dwowd --lib -- daemon_sync_integration
        if tx.first_call_is_pow_reward() { return true; }
        for nullifier in &tx.nullifiers {
            if let Some(nf_height) = chain_state.nullifier_height(nullifier) {
                if height.saturating_sub(nf_height) < dwow_chain::COINBASE_MATURITY {
                    return false;
                }
            }
        }
        true
    }).collect();

    // 3b. Mint one spendable note per accepted uncle (UncleMintV1, 0x07).
    // Spec: uncle_merkle.md §Uncle Minting & Maturity — "Per-uncle note mint".
    for (idx, uncle) in uncles.iter().enumerate() {
        if !uncle.pin_accepted || uncle.pin_confirmed.get() == 0 {
            continue;
        }
        let tx_nonce = dwow_sdk::pasta::pallas::Base::from(height.get() * 1000 + idx as u64);
        let uncle_tx = build_uncle_mint_tx(uncle, height, tx_nonce)?;
        mempool_txs.push(uncle_tx);
    }

    // 4. Assemble coinbase transaction (infallible)
    let coinbase_tx = dwow_chain::Transaction {
        version: BlockVersion::CURRENT,
        inputs: vec![],
        outputs: vec![],
        contract_calls: vec![pow_reward_call.clone()],
        lock_time: 0,
        ..Default::default()
    };

    // 5. Build FeeCollectV1 — the "collection plate" as final transaction
    //    (consensus-coinbase.md §3.12). Fallible but no destructive mutation
    //    yet — competing blocks still safe in chain_state.
    // Sum FeeV3 fees: the fee is plaintext in FeeParamsV3.fee (no decryption).
    // P2-9-5: single source of truth — the same sum feeds stratum/mm_rpc
    // templates via registry::model.
    let total_fees = crate::registry::model::sum_block_fee_v3(&mempool_txs);
    // ── Contract risk factor update (FI-RISK-3, FI-RISK-5) ────────────
    // Record observed-vs-declared BlockCharge so the dynamic tracker can
    // escalate/de-escalate per-contract risk at the next window boundary.
    // The miner owns only the native_token fee zkbin, so it measures the fee
    // circuit's actual gas; non-fee contracts are not measured yet (their
    // observed == declared → risk-neutral) until full gas metering lands.
    {
        use dwow_chain::opcode_cost::circuit_difficulty;
        let fee_gas = dwow_core::zkas::ZkBinary::decode(
            dwow_native_token_contract::NATIVE_TOKEN_CONTRACT_ZKAS_FEE_V3_BIN, false,
        ).map(|zkbin| circuit_difficulty(&zkbin.opcodes))
          .unwrap_or_else(|e| {
              tracing::warn!(target: "dwowd::miner",
                  "FeeV3 zkbin decode failed: {e}; risk tracker uses declared gas");
              0
          });

        let mut tracker = chain_state.contract_risk_tracker
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        for tx in &mempool_txs {
            for call in &tx.contract_calls {
                if call.as_mass_balance_fee_v3().is_some() {
                    // Declared charge is the flat per-call promise (§12.4.5);
                    // observed is the fee circuit's measured row count.
                    tracker.record(call.contract_id, "fee_v3".into(), 400_000_000, fee_gas, height.get());
                }
                let risk = tracker.get_risk_factor(&call.contract_id);
                if risk > RiskFactor::BASELINE {
                    debug!(target: "dwowd::prepare_block",
                        "Elevated risk factor for contract {}: {} (baseline {})",
                        call.contract_id, risk, RiskFactor::BASELINE);
                }
            }
        }
        drop(tracker);
    }
    let prod_tf = total_fees;
    let fee_collect_tx = crate::registry::model::build_fee_collect_tx(
        &recipient,
        &mempool_txs,
        height,
        prod_tf,
    )?;

    // 6. Destructively take the competing blocks — the LAST step.
    //    take_competing_blocks is DESTRUCTIVE. All fallible operations
    //    (coinbase build, uncle-mint, fee_collect_tx) MUST succeed before this.
    //    The uncles were already built (step 0) from the peeked blocks.
    let competing_originals: Vec<dwow_chain::Block> =
        chain_state.take_competing_blocks(latest_height);
    if !uncles.is_empty() {
        info!(target: "dwowd::prepare_block",
            "Including {} uncles at height {}", uncles.len(), height);
    }

    Ok(PreparedBlock {
        uncles,
        competing_originals, mempool_txs, coinbase_tx,
        fee_collect_tx,
    })
}

// ---------------------------------------------------------------------------
// Built-in Miner Task
//
// Replaces the fragile bash /dev/tcp loop in entrypoint.sh. The node mines
// internally like every production node (Bitcoin Core -gen, Geth --mine).
// ---------------------------------------------------------------------------

/// Internal mining task — loops indefinitely, mining blocks when sync is complete.
async fn miner_task(node: DwowNodePtr) -> Result<()> {
    use dwow_chain::Miner;
    use crate::proto::linear_broadcast::broadcast_block;

    info!(target: "dwowd::miner_task", "Built-in miner starting...");

    // Wait for sync to reach CaughtUp before mining — this is the "miner starts
    // in observer mode" transition (node-startup-spec.md §2). Before CaughtUp the
    // node is sync-only (observer); after CaughtUp it becomes a mining node.
    // Bitcoin production pattern: mining before sync produces orphan blocks.
    let mut wait_count = 0u32;
    const STARTUP_TIMEOUT: u32 = 600; // 10 minutes
    while SyncState::load(&node.mining_state.sync_state) != SyncState::CaughtUp {
        if wait_count % 30 == 0 {
            // P2-7: the WaitingForGenesis branch collapsed into the generic
            // wait — that state was never written (see the SyncState doc).
            // UNVERIFIED(P2-7): needs cargo test -p dwowd --lib -- daemon_sync_integration
            info!(target: "dwowd::miner_task",
                "Waiting for CaughtUp — current sync_state={:?} ({}s elapsed)",
                SyncState::load(&node.mining_state.sync_state), wait_count);
        }
        if wait_count == STARTUP_TIMEOUT {
            error!(target: "dwowd::miner_task",
                "MINER STARTUP TIMEOUT: sync_state still {:?} after {}s. \
                 Consensus task may be deadlocked or stuck. \
                 Check consensus_linear_init_task logs.",
                SyncState::load(&node.mining_state.sync_state), wait_count);
        }
        smol::Timer::after(std::time::Duration::from_secs(1)).await;
        wait_count += 1;
    }
    info!(target: "dwowd::miner_task",
        "Sync caught up after {}s, starting mining loop", wait_count);

    // Mining loop — re-checks sync_state each iteration.
    // The consensus init task may set sync_state=Behind if the node
    // falls behind peers during the continuous sync cycle.
    let mut last_logged_sync_state = SyncState::CaughtUp;
    loop {
        // Re-check sync before each mining attempt.
        // Avoids mining at a stale height when peers have advanced.
        let state = SyncState::load(&node.mining_state.sync_state);
        if state != SyncState::CaughtUp {
            // HAZOP H1: log state transitions — NOT every 1s spin.
            // Operator sees WHY mining stopped without log-spam.
            if last_logged_sync_state != state {
                warn!(target: "dwowd::miner_task",
                    "Mining paused: sync_state={:?} (was {:?}, now waiting for CaughtUp)",
                    state, last_logged_sync_state);
                last_logged_sync_state = state;
            }
            smol::Timer::after(std::time::Duration::from_secs(1)).await;
            continue;
        }
        // Reset tracking when back to CaughtUp
        if last_logged_sync_state != SyncState::CaughtUp {
            info!(target: "dwowd::miner_task",
                "Mining resumed: sync_state={:?} — miner re-enabled", state);
            last_logged_sync_state = SyncState::CaughtUp;
        }

        let chain_state = match &node.chain_state {
            Some(cs) => cs.clone(),
            None => {
                error!(target: "dwowd::miner_task",
                    "chain_state is None — mining cannot proceed. Node may be misconfigured.");
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

        let height = latest_block.header.height.succ();

        // ── Fee window boundary detection ─────────────────────────────────
        // At height ≡ 0 (mod 20), adjust thresholds based on mempool congestion
        // and encode the signal in the block header (fee-spec.md §12.10).
        let fee_window_flags: FeeWindowFlags = {
            use dwow_sdk::blockchain::FeeWindowId;
            if FeeWindowId::is_window_boundary(height) {
                let premium_pending = node.mempool.as_ref()
                    .map(|mp| mp.premium_queue_len())
                    .unwrap_or_else(|| {
                        warn!(target: "dwowd::miner_task",
                            "Window boundary at height {} but mempool is None — \
                             using zero congestion. If this node is mining, \
                             mempool should be active.", height);
                        0
                    }) as u64;
                let standard_pending = node.mempool.as_ref()
                    .map(|mp| mp.standard_queue_len())
                    .unwrap_or(0) as u64;
                if let Some(ref fw) = chain_state.fee_window {
                    let circuit_cf = fw.adjust_circuit(premium_pending, standard_pending);
                    let wasm_cf = fw.adjust_wasm(premium_pending, standard_pending);
                    let flags = fw.encode_flags();
                    if let Some(ref mp) = node.mempool {
                        // FeeV3: compute the three tier prices from the fee circuit's
                        // gas (the minimum gas any fee tx carries) at the current CFs.
                        // §12.5: PRICE_{LOW,MEDIUM,HIGH} = {1,2,4} × CF (gas is the fee).
                        use dwow_chain::opcode_cost::circuit_difficulty;
                        let gas_ref = dwow_core::zkas::ZkBinary::decode(
                            dwow_native_token_contract::NATIVE_TOKEN_CONTRACT_ZKAS_FEE_V3_BIN, false,
                        ).map(|fee_zkbin| circuit_difficulty(&fee_zkbin.opcodes))
                          .unwrap_or_else(|e| {
                              tracing::warn!(target: "dwowd::miner",
                                  "FeeV3 zkbin decode failed: {e}; fee tiers use default gas");
                              0
                          });

                        // §12.4.4: high tier uses CF_premium; medium/low use CF_standard
                        // (compute_fee_v3 selects the CF component by tier).
                        let price_high = compute_fee_v3(gas_ref, circuit_cf, FeeTier::HIGH, RiskFactor::BASELINE);
                        let price_medium = compute_fee_v3(gas_ref, circuit_cf, FeeTier::MEDIUM, RiskFactor::BASELINE);
                        let price_low = compute_fee_v3(gas_ref, circuit_cf, FeeTier::LOW, RiskFactor::BASELINE);
                        mp.update_tier_prices(price_high, price_medium, price_low);
                    }
                    // ── Contract risk factor evaluation (FI-RISK-2) ──────────
                    // At each window boundary, update risk factors for contracts
                    // with recorded cost deviations in the current window.
                    // Risk factors are per-contract, stored in sled, and read by
                    // compute_fee_v3() for admission threshold computation.
                    {
                        let mut tracker = chain_state.contract_risk_tracker
                            .lock()
                            .unwrap_or_else(|e| e.into_inner());
                        // Evaluate ALL contracts with pending deviations.
                        // FI-RISK-2: escalates under-declaring contracts,
                        // de-escalates conforming contracts, leaves others at
                        // baseline. For contracts with no deviations, this is
                        // a no-op (get_risk_factor returns baseline, FI-RISK-4).
                        let updated = tracker.evaluate_all_windows();
                        if !updated.is_empty() {
                            info!(target: "dwowd::miner_task",
                                "Contract risk factors updated: {} contracts",
                                updated.len());
                            for (cid, risk) in &updated {
                                debug!(target: "dwowd::miner_task",
                                    "  contract={} risk={}", cid, risk);
                            }
                        }
                        // Persist updated risk factors to sled after evaluation.
                        if let Err(e) = tracker.save_to_tree(&chain_state.store.contract_risk) {
                            warn!(target: "dwowd::miner_task",
                                "Failed to persist contract risk factors: {}", e);
                        }
                        drop(tracker);
                    }

                    info!(target: "dwowd::miner_task",
                        "Fee window boundary at height {}: circuit_premium={:?}, circuit_standard={:?}, wasm_premium={:?}, wasm_standard={:?}, flags=0x{:04x}",
                        height, circuit_cf.premium(), circuit_cf.standard(), wasm_cf.premium(), wasm_cf.standard(), flags.get());
                    flags
                } else {
                    FeeWindowFlags::default()
                }
            } else {
                FeeWindowFlags::default()
            }
        };

        // Memory diagnostics — log every 5 blocks to catch leaks early.
        // Reads /proc/self/status for VmRSS (no allocator dependency).
        // Rust's ownership model eliminates use-after-free but doesn't
        // prevent unbounded collection growth. This is the canary.
        if height.get() % 5 == 0 {
            let vm_cache_size = chain_state.vm_cache_size();
            let commitment_set_size = chain_state.commitment_set_size();
            let resident_kb = std::fs::read_to_string("/proc/self/status")
                .ok()
                .and_then(|s| {
                    s.lines()
                        .find(|l| l.starts_with("VmRSS:"))
                        .and_then(|l| l.split_whitespace().nth(1).map(|v| v.to_string()))
                })
                .unwrap_or_else(|| "unknown".to_string());
            info!(target: "dwowd::memory",
                "block={} resident={}kB vm_cache={} commitment_set={}",
                height, resident_kb, vm_cache_size, commitment_set_size,
            );
        }
        let previous = match chain_state.hash_block_with_cached_vm(&latest_block) {
            Ok(h) => h,
            Err(e) => {
                tracing::error!(target: "dwowd::miner",
                    "hash_block_with_cached_vm failed: {e} — retrying next cycle");
                continue;
            }
        };
        let randomx_key = Miner::derive_key_from_height(height);
        // H1+H2 fix: miner creates its OWN VM, not from the shared cache.
        // Using chain_state.get_vm() would return an Arc<Mutex<RandomXVM>> that the
        // broadcast handler could also access concurrently during connect_block.
        // RandomX FFI is not thread-safe — concurrent access on the same VM
        // from two smol tasks causes a segfault.
        // Creating a fresh VM from a pooled RandomXCache reuses the 256 MB
        // allocation — only the 2 MB scratchpad is allocated fresh.
        let flags = randomx::RandomXFlags::get_recommended_flags() & !randomx::RandomXFlags::JIT;
        let rx_cache = match chain_state.get_cache(randomx_key) {
            Ok(c) => c,
            Err(e) => {
                tracing::error!(target: "dwowd::miner",
                    "Failed to create RandomX cache: {} — retrying next cycle", e);
                continue;
            }
        };
        let vm = match randomx::RandomXVM::new(flags, Some(rx_cache), None) {
            Ok(vm) => Arc::new(vm),
            Err(e) => {
                tracing::error!(target: "dwowd::miner",
                    "Failed to create RandomX VM: {} — retrying next cycle", e);
                continue;
            }
        };
        // Chain-derived target: matches Python model's get_next_work_required.
        // Reads timestamps from canonical chain blocks, not accumulator.
        let target = {
            let consensus = chain_state.consensus.lock().unwrap_or_else(|e| e.into_inner());
            match consensus.get_next_work_required(&chain_state.store, height) {
                Ok(t) => t,
                Err(e) => {
                    tracing::error!(target: "dwowd::miner",
                        "get_next_work_required failed: {e} — retrying next cycle");
                    continue;
                }
            }
        };

        let base_reward = dwow_sdk::blockchain::expected_reward(height);
        info!(target: "dwowd::miner_task",
            "Mining block {} (target={:#010x})", height, target);

        // Coinbase recipient is ALWAYS this node's own declared key (decision:
        // one miner, one key — no external/forwarded recipient). MiningRecipient
        // can only be built from a key the node holds.
        let recipient = match crate::accounts::MiningRecipient::from_account(
            &*node.account_manager.read().await, height,
        ) {
            Ok(r) => r,
            Err(e) => {
                error!(target: "dwowd::miner_task", "AccountManager error: {e}");
                smol::Timer::after(std::time::Duration::from_secs(2)).await;
                continue;
            }
        };

        // Unified block preparation — collects uncles, builds coinbase, selects txs.
        // No ZK materials needed: coinbase/uncle/fee-collect are all plaintext.
        // UNVERIFIED(F2-6): needs cargo test -p dwowd --lib
        // The coinbase note binds this recipient's pk_H; the block header's
        // `miner` field must carry the same key (enforced at `block_acceptor.rs`,
        // genesis exempt — see `doc/src/dev/contracts/safety.md` RC5).
        let miner_pk = recipient.public().to_bytes();
        let prep = match prepare_block(
            &chain_state, &node.mining_state, node.mempool.as_ref(),
            recipient, height, base_reward,
        ).await {
            Ok(p) => p,
            Err(e) => {
                error!(target: "dwowd::miner_task", "Block preparation failed: {e}");
                smol::Timer::after(std::time::Duration::from_secs(2)).await;
                continue;
            }
        };

        let uncles = prep.uncles;
        let competing_originals = prep.competing_originals;
        let mempool_txs = prep.mempool_txs.clone(); // cloned for error recovery
        info!(target: "dwowd::miner_task",
            "Block {} assembly complete ({} mempool txs, {} uncles)",
            height, mempool_txs.len(), uncles.len());
        let mut all_txs = vec![prep.coinbase_tx];
        all_txs.extend(prep.mempool_txs); // original moved into all_txs
        // FeeCollectV1 closes the merkle tree — final transaction (spec §3.1).
        // Carries proof in witness + nullifier in tx.nullifiers (built in
        // prepare_block).
        if let Some(fee_tx) = prep.fee_collect_tx {
            all_txs.push(fee_tx);
        }

        // Check if a block already arrived at this height (P2P broadcast
        // from a peer mining at the same time). If so, skip mining — the
        // peer's block is already committed. Re-insert mempool txs.
        if chain_state.get_height() >= height {
            info!(target: "dwowd::miner_task",
                "Block already exists at height {} — peer beat us to it", height);
            if let Some(ref mp) = node.mempool {
                // skip coinbase at index 0 — `get` rather than a slice, so an empty `all_txs`
                // is a skip instead of a panic
                for tx in all_txs.get(1..).unwrap_or(&[]) {
                    let _ = mp.add(tx.clone()).await;
                }
            }
            smol::Timer::after(std::time::Duration::from_secs(1)).await;
            continue;
        }

        // Mine
        info!(target: "dwowd::miner_task",
            "Beginning RandomX mining for block {} (target={:#010x}, {} txs)",
            height, target, all_txs.len());
        let miner_consensus = dwow_chain::PoWConsensus::new(120, target, BlockTarget::new(1), BlockTarget::MAX);
        let miner = Miner::new(std::sync::Arc::new(miner_consensus));
        // Per-block Caribina anchor key: generated before mining because its public half is inside
        // the mined region, kept because its secret signs the proof after the nonce is found.
        let anchor_wallet = dwow_chain::caribina::CaribinaWallet::generate();
        let mut mined_block = match miner.mine(&vm, previous, height, all_txs, target, miner_pk, anchor_wallet.public_key(), &uncles) {
            Ok(b) => {
                info!(target: "dwowd::miner_task",
                    "Block {} mined with nonce {}", height, b.header.nonce);
                b
            }
            Err(e) => {
                error!(target: "dwowd::miner_task", "Mining failed: {}", e);
                // Re-insert competing blocks that were destructively consumed
                // by take_competing_blocks() during prepare_block.
                // UNVERIFIED(HYG-5-4): needs cargo check -p dwowd -j 2 && cargo test
                // -p dwowd --lib --test-threads=2 (re-insert key fixed: prepare_block
                // takes from the tip height, but this path re-inserted at the mining
                // height — one past the take key, where the next miner, still at the
                // old tip, would never look. Now round-trips at the take key,
                // matching the accept-failure path below and mm_rpc.)
                if !competing_originals.is_empty() {
                    info!(target: "dwowd::miner_task",
                        "Re-inserting {} competing blocks after mining failure",
                        competing_originals.len());
                    chain_state.put_competing_blocks(latest_block.header.height, competing_originals);
                }
                // Re-insert mempool txs — they were consumed by all_txs above.
                // Matches miner_mine_linear recovery pattern (rpc/miner.rs:206-218).
                if let Some(ref mp) = node.mempool {
                    for tx in &mempool_txs {
                        let _ = mp.add(tx.clone()).await;
                    }
                }
                smol::Timer::after(std::time::Duration::from_secs(2)).await;
                continue;
            }
        };

        // Set fee window flags on the mined block header (fee-spec.md §12.10).
        // Flags are set AFTER mining (excluded from mining blob) and BEFORE
        // accept_block so the block carries the signal on-chain.
        // SPEC-4: fee window behaviour is consensus-critical; no feature gate.
        {
            mined_block.header.fee_window_flags = fee_window_flags;
        }

        // Build and attach the Caribina anchor proof — also after mining and before accept, for the
        // same reason as the flags above: the proof binds this header, and `chain_state`'s enforcement
        // requires a *verified* proof to confer finality (OBL-C63). This task never anchored at all
        // before 2026-09-22, so a block the daemon mined itself was never final.
        let anchor_proof = {
            let fc = &chain_state.finality_config;
            if fc.should_anchor() {
                let proof =
                    dwow_chain::caribina::build_anchor_proof(&mined_block.header, &anchor_wallet);
                mined_block.header.caribina_anchor = Some(proof.clone());
                mined_block.header.finality_flags = fc.mine_flags();
                debug_assert!(
                    dwow_chain::caribina::verify_anchor_proof(&mined_block.header),
                    "a proof this node just built must verify, or no self-mined block is ever final"
                );
                Some(proof)
            } else {
                None
            }
        };

        // Check again after mining, on both counts that can have changed while we hashed: a peer may have
        // sent a block for this height, and the consensus task may have declared the node Behind.
        //
        // The sync state is re-read **here**, next to the commit, and not only at the top of the loop
        // (OBL-C56). The top-of-loop check is observed, not held: RandomX mining can take minutes, so a
        // transition that arrives mid-attempt used to stay invisible until the next iteration — by which
        // time this block had already been proposed at a height the network had moved past. A lease
        // cannot be held across the attempt, because the sync task must stay free to declare the node
        // Behind (blocking it on a mining attempt would stall sync on the one thing it exists to detect).
        // So the state is re-observed at the last point where abandoning the block is still free, and
        // both reasons to abandon share one recovery path.
        let peer_ahead = chain_state.get_height() >= height;
        let fell_behind = SyncState::load(&node.mining_state.sync_state) != SyncState::CaughtUp;
        if peer_ahead || fell_behind {
            info!(target: "dwowd::miner_task",
                "Discarding mined block {} because {} — re-inserting mempool txs",
                height,
                if peer_ahead { "a peer block arrived at this height" }
                else { "the node fell behind while we hashed (sync_state != CaughtUp)" });
            // Re-insert mempool transactions — they were consumed into all_txs
            // at block assembly. The pre-mining race path (above) does the same.
            if let Some(ref mp) = node.mempool {
                for tx in &mempool_txs {
                    let _ = mp.add(tx.clone()).await;
                }
            }
            smol::Timer::after(std::time::Duration::from_secs(1)).await;
            continue;
        }

        // Accept block — single unified path (block_acceptor::accept_block).
        // Covers: proof-of-token-balance, WASM execution, overlay aggregation,
        // and atomic connect_block with contract state.
        let apply_result = accept_block_with_mempool(
            &chain_state,
            &mined_block,
            &uncles,
            &vm,
            target,
            Some(&node.fee_estimator),
            // A reorg triggered here returns the displaced blocks' transactions to the mempool
            // (`OBL-C44`).
            node.mempool.as_ref(),
        );

        // Drop VM reference after block acceptance — avoids concurrent
        // RandomX access if a P2P block arrives during the next iteration.
        drop(vm);
        match apply_result {
            Ok(outcome) => {
                // Logging-only hash — a failure here must not kill the miner task.
                let applied_hash = match chain_state.hash_block_with_cached_vm(&mined_block) {
                    Ok(h) => h,
                    Err(e) => {
                        tracing::warn!(target: "dwowd::miner_task",
                            "post-apply hash failed: {e}");
                        blake3::Hash::from([0u8; 32])
                    }
                };
                match outcome {
                    dwow_chain::BlockConnectOutcome::CanonicalExtension { new_height: _ } => {
                        info!(target: "dwowd::miner_task",
                            "Block {} mined and applied: {}",
                            height, applied_hash);

                        // Fee estimation now recorded in accept_block via the fee_estimator
                        // parameter, using actual WASM execution gas (stats.gas_used) rather
                        // than the ad-hoc calls*400M estimate. No duplicate recording needed.

                        // Remove mined transactions from mempool — ONLY for canonical blocks.
                        // CompetingStored and UncleExtended do NOT advance the canonical
                        // chain; their transactions remain pending in the mempool.
                        if let Some(ref mp) = node.mempool {
                            let tx_hashes: Vec<blake3::Hash> = mined_block.transactions.iter()
                                .map(|tx| tx.hash()).collect();
                            mp.mark_mined(&tx_hashes).await;
                        }
                    }
                    dwow_chain::BlockConnectOutcome::CompetingStored => {
                        info!(target: "dwowd::miner_task",
                            "Block {} stored as competing (peer beat us) — mempool unchanged",
                            height);
                    }
                    dwow_chain::BlockConnectOutcome::UncleExtended => {
                        info!(target: "dwowd::miner_task",
                            "Block {} stored as uncle extension — mempool unchanged",
                            height);
                    }
                    dwow_chain::BlockConnectOutcome::AlreadyKnown => {
                        info!(target: "dwowd::miner_task",
                            "Block {} already in chain (duplicate) — skipped", height);
                    }
                }
            }
            Err(e) => {
                error!(target: "dwowd::miner_task",
                    "Failed to apply mined block: {}", e);
                // H3.4: Re-insert competing blocks that were destructively
                // consumed by take_competing_blocks().
                if !competing_originals.is_empty() {
                    info!(target: "dwowd::miner_task",
                        "Re-inserting {} competing blocks after accept failure",
                        competing_originals.len());
                    chain_state.put_competing_blocks(
                        latest_block.header.height,
                        competing_originals,
                    );
                }
                // H-H7: Re-insert mempool transactions.
                // Matches miner_mine_linear recovery (rpc/miner.rs:234-243).
                if let Some(ref mp) = node.mempool {
                    for tx in &mempool_txs {
                        let _ = mp.add(tx.clone()).await;
                    }
                }
                smol::Timer::after(std::time::Duration::from_secs(2)).await;
                continue;
            }
        }

        // Broadcast
        let peer_count = node.p2p_handler.p2p.hosts().peers().len();
        if peer_count == 0 {
            warn!(target: "dwowd::miner_task",
                "Block {} mined but ZERO peers connected — block will not reach network until peers connect",
                height);
        }
        broadcast_block(&node.p2p_handler.p2p, mined_block, uncles).await;

        // Rate-limit: wait for min_block_interval before next block
        let min_interval = node.min_block_interval;
        let last = node.mining_state.last_block_time.get().get(); // G3: rate-limit comparison uses raw u64
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let elapsed = now.saturating_sub(last);
        if elapsed < min_interval {
            smol::Timer::after(std::time::Duration::from_secs(
                min_interval.saturating_sub(elapsed)
            )).await;
        }
        node.mining_state.last_block_time.set_now();
    }
}

// ---------------------------------------------------------------------------
// active_template — the unit test for OBL-C43/OBL-C54
// ---------------------------------------------------------------------------
//
// Named `active_template_tests` rather than `tests` because the crate root already declares
// `mod tests;` (the heavyweight pipeline harness) and a second `tests` module would collide.
#[cfg(test)]
mod active_template_tests {
    use super::*;
    use std::sync::atomic::AtomicU8;

    const TEST_KEY_TOML: &str = "[node0]\nwallet_secret = \
         \"755c6e8a21b3e15f146ba636a146c228b5f91202fc7e0bb0065efdd9fd685405\"\n";

    /// A template whose `previous` hash identifies the publication it belongs to, so a test can tell two
    /// publications apart field by field.
    fn template(tag: u8) -> crate::registry::model::LinearBlockTemplate {
        crate::registry::model::LinearBlockTemplate {
            previous: [tag; 32],
            height: BlockHeight::new(1),
            target: BlockTarget::MAX,
            timestamp: 1_700_000_000,
            value: BlockReward::ZERO,
            pow_reward_call_data: vec![tag],
            miner: [tag; 32],
            anchor_wallet: dwow_chain::caribina::CaribinaWallet::generate(),
            transactions: vec![],
            merkle_root: blake3::hash(&[tag]),
            uncles: vec![],
        }
    }

    fn keys_file() -> std::path::PathBuf {
        let path = std::env::temp_dir()
            .join(format!("dwow_active_template_{}.toml", std::process::id()));
        std::fs::write(&path, TEST_KEY_TOML).expect("write test keys");
        path
    }

    /// `OBL-C43`/`OBL-C54` — `store_template` publishes the template, its height and its recipient as
    /// **one** value, so no reader can pair one round's template with another round's height or recipient.
    ///
    /// The assertion that matters is the one after the *second* publication: no field of the first survives.
    /// A publication done in parts — template, then height, then recipient, as three separately-locked
    /// fields — fails it, because a reader landing between two of those writes sees exactly such a mixture.
    #[test]
    fn test_store_template_publishes_the_triple_as_one_value() {
        let state = MiningState::new(std::sync::Arc::new(AtomicU8::new(SyncState::CaughtUp as u8)));
        assert!(
            smol::block_on(state.active_template.lock()).is_none(),
            "control: nothing is published before a login"
        );

        let path = keys_file();
        let mgr = crate::accounts::AccountManager::open(&path, Network::Testnet, "node0")
            .expect("open test keys");
        let _ = std::fs::remove_file(&path);
        let first = LinearMinerRewardsRecipientConfig::from_account(&mgr, BlockHeight::new(1))
            .map_err(|_| "from_account failed at height 1")
            .expect("recipient at height 1");
        let second = LinearMinerRewardsRecipientConfig::from_account(&mgr, BlockHeight::new(2))
            .map_err(|_| "from_account failed at height 2")
            .expect("recipient at height 2");
        assert_ne!(
            first.recipient, second.recipient,
            "control: per-block cycling must give two different recipients, or the pairing below could \
             not be distinguished"
        );

        smol::block_on(state.store_template(template(0x11), BlockHeight::new(1), first.clone()));
        {
            let guard = smol::block_on(state.active_template.lock());
            let published = guard.as_ref().expect("a published template");
            assert_eq!(published.template.previous, [0x11; 32], "the template is the one published");
            assert_eq!(published.height, BlockHeight::new(1), "with the height it was published at");
            assert_eq!(published.recipient_config.recipient, first.recipient, "and its recipient");
        }

        // Republish a different triple, with a different height and a different recipient.
        smol::block_on(state.store_template(template(0x22), BlockHeight::new(2), second.clone()));
        let guard = smol::block_on(state.active_template.lock());
        let published = guard.as_ref().expect("a published template");
        assert_eq!(published.template.previous, [0x22; 32]);
        assert_eq!(published.template.pow_reward_call_data, vec![0x22]);
        assert_eq!(published.template.miner, [0x22; 32]);
        assert_eq!(
            published.height,
            BlockHeight::new(2),
            "OBL-C43/C54: the height must move with the template — a reader that takes them separately \
             can see the new template against the previous round's height"
        );
        assert_eq!(
            published.recipient_config.recipient, second.recipient,
            "OBL-C43/C54: so must the recipient — a next-round template paid to the previous round's \
             recipient is the outcome this bundles away"
        );
    }
}
