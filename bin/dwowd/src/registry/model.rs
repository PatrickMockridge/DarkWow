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

use std::{collections::HashMap, str::FromStr, sync::Arc};

use dwow_linear::Output;

use dwow_sdk::crypto::PublicKey;
use dwow_sdk::crypto::keypair::{Address, Network};

use rand::rngs::OsRng;
use sled::IVec;
use tinyjson::JsonValue;
use tracing::info;

use dwow::{
    blockchain::{BlockInfo, Header, HeaderHash},
    rpc::jsonrpc::JsonSubscriber,
    tx::{ContractCallLeaf, Transaction, TransactionBuilder},
    util::{
        encoding::base64,
        time::{NanoTimestamp, Timestamp},
    },
    validator::{
        consensus::Fork,
        pow::{RANDOMX_KEY_CHANGE_DELAY, RANDOMX_KEY_CHANGING_HEIGHT},
        verification::apply_producer_transaction,
        ValidatorPtr,
    },
    zk::{empty_witnesses, ProvingKey, ZkCircuit},
    zkas::ZkBinary,
    Error, Result,
};
use dwow_native_token_contract::{client::pow_reward_v1::PoWRewardCallBuilder, NativeTokenFunction, NATIVE_TOKEN_CONTRACT_ZKAS_MINT_NS_V1, NATIVE_TOKEN_CONTRACT_ZKAS_MINT_V1_BIN};
use dwow_sdk::{
    crypto::{
        keypair::{Keypair, SecretKey},
        pasta_prelude::PrimeField,
        FuncId, MerkleTree, NATIVE_TOKEN_CONTRACT_ID,
    },
    pasta::pallas,
    ContractCall,
};
use dwow_serial::{deserialize_async, Encodable};

use crate::error::RpcError;

/// Auxiliary structure representing node miner rewards recipient configuration.
#[derive(Debug, Clone)]
pub struct MinerRewardsRecipientConfig {
    /// Wallet mining address to receive mining rewards
    pub recipient: Address,
    /// Optional contract spend hook to use in the mining reward
    pub spend_hook: Option<FuncId>,
    /// Optional contract user data to use in the mining reward.
    /// This is not arbitrary data.
    pub user_data: Option<pallas::Base>,
}

impl MinerRewardsRecipientConfig {
    pub fn new(
        recipient: Address,
        spend_hook: Option<FuncId>,
        user_data: Option<pallas::Base>,
    ) -> Self {
        Self { recipient, spend_hook, user_data }
    }

    /// Auxiliary function to convert provided string to its
    /// `MinerRewardsRecipientConfig`. Supports parsing both a normal
    /// `Address` and a `base64` encoded mining configuration. Also
    /// verifies it corresponds to the provided `Network`.
    pub async fn from_str(network: &Network, address: &str) -> std::result::Result<Self, RpcError> {
        // Try to parse the string as an `Address`
        if let Ok(recipient) = Address::from_str(address) {
            if recipient.network() != *network {
                return Err(RpcError::MinerInvalidRecipientPrefix)
            }
            return Ok(Self { recipient, spend_hook: None, user_data: None })
        }

        // Try to parse the string as a `base64` encoded mining
        // configuration
        let Some(address_bytes) = base64::decode(address) else {
            return Err(RpcError::MinerInvalidWalletConfig)
        };
        let Ok((recipient, spend_hook, user_data)) =
            deserialize_async::<(String, Option<String>, Option<String>)>(&address_bytes).await
        else {
            return Err(RpcError::MinerInvalidWalletConfig)
        };
        let Ok(recipient) = Address::from_str(&recipient) else {
            return Err(RpcError::MinerInvalidRecipient)
        };
        if recipient.network() != *network {
            return Err(RpcError::MinerInvalidRecipientPrefix)
        }
        let spend_hook = match spend_hook {
            Some(s) => match FuncId::from_str(&s) {
                Ok(s) => Some(s),
                Err(_) => return Err(RpcError::MinerInvalidSpendHook),
            },
            None => None,
        };
        let user_data: Option<pallas::Base> = match user_data {
            Some(u) => {
                let Ok(bytes) = bs58::decode(&u).into_vec() else {
                    return Err(RpcError::MinerInvalidUserData)
                };
                let bytes: [u8; 32] = match bytes.try_into() {
                    Ok(b) => b,
                    Err(_) => return Err(RpcError::MinerInvalidUserData),
                };
                match pallas::Base::from_repr(bytes).into() {
                    Some(v) => Some(v),
                    None => return Err(RpcError::MinerInvalidUserData),
                }
            }
            None => None,
        };

        Ok(Self { recipient, spend_hook, user_data })
    }
}

/// Auxiliary structure representing a block template for mining.
#[derive(Debug, Clone)]
pub struct BlockTemplate {
    /// Block that is being mined
    pub block: BlockInfo,
    /// New `sled` trees opened the overlay this block was generated
    pub new_trees: Vec<IVec>,
    /// RandomX current and next keys pair
    pub randomx_keys: (HeaderHash, Option<HeaderHash>),
    /// Compacted block mining target
    pub target: Vec<u8>,
    /// Block difficulty
    pub difficulty: f64,
    /// Ephemeral signing secret for this blocktemplate
    pub secret: SecretKey,
    /// Flag indicating if this template has been submitted
    pub submitted: bool,
}

impl BlockTemplate {
    fn new(
        block: BlockInfo,
        new_trees: Vec<IVec>,
        randomx_keys: (HeaderHash, Option<HeaderHash>),
        target: Vec<u8>,
        difficulty: f64,
        secret: SecretKey,
    ) -> Self {
        Self { block, new_trees, randomx_keys, target, difficulty, secret, submitted: false }
    }

    pub fn job_notification(&self) -> (String, JsonValue) {
        let block_hash = hex::encode(self.block.header.hash().inner()).to_string();
        let mut job = HashMap::from([
            (
                "blob".to_string(),
                JsonValue::from(hex::encode(self.block.header.to_block_hashing_blob()).to_string()),
            ),
            ("job_id".to_string(), JsonValue::from(block_hash.clone())),
            ("height".to_string(), JsonValue::from(self.block.header.height as f64)),
            ("target".to_string(), JsonValue::from(hex::encode(&self.target))),
            ("algo".to_string(), JsonValue::from(String::from("rx/0"))),
            (
                "seed_hash".to_string(),
                JsonValue::from(hex::encode(self.randomx_keys.0.inner()).to_string()),
            ),
        ]);
        if let Some(next_randomx_key) = self.randomx_keys.1 {
            job.insert(
                "next_seed_hash".to_string(),
                JsonValue::from(hex::encode(next_randomx_key.inner()).to_string()),
            );
        }
        (block_hash, JsonValue::from(job))
    }
}

/// Auxiliary structure representing a native miner client record.
#[derive(Debug, Clone)]
pub struct MinerClient {
    /// Miner wallet template key
    pub wallet: String,
    /// Miner recipient configuration
    pub config: MinerRewardsRecipientConfig,
    /// Current mining job
    pub job: String,
    /// Connection publisher to push new jobs
    pub publisher: JsonSubscriber,
}

impl MinerClient {
    pub fn new(wallet: &str, config: &MinerRewardsRecipientConfig, job: &str) -> (String, Self) {
        let mut hasher = blake3::Hasher::new();
        hasher.update(wallet.as_bytes());
        hasher.update(&NanoTimestamp::current_time().inner().to_le_bytes());
        let client_id = hex::encode(hasher.finalize().as_bytes()).to_string();
        let publisher = JsonSubscriber::new("job");
        (
            client_id,
            Self {
                wallet: String::from(wallet),
                config: config.clone(),
                job: job.to_owned(),
                publisher,
            },
        )
    }
}

/// ZK data used to generate the "coinbase" transaction in a block
pub struct PowRewardV1Zk {
    pub zkbin: ZkBinary,
    pub provingkey: ProvingKey,
}

impl PowRewardV1Zk {
    pub async fn new(validator: &ValidatorPtr) -> Result<Self> {
        info!(
            target: "dwowd::registry::model::PowRewardV1Zk::new",
            "Generating PowRewardV1 ZkCircuit and ProvingKey...",
        );

        let validator = validator.read().await;
        let (zkbin, _) = validator.blockchain.contracts.get_zkas(
            &validator.blockchain.sled_db,
            &NATIVE_TOKEN_CONTRACT_ID,
            NATIVE_TOKEN_CONTRACT_ZKAS_MINT_NS_V1,
        )?;

        let circuit = ZkCircuit::new(empty_witnesses(&zkbin)?, &zkbin);
        let provingkey = ProvingKey::build(zkbin.k, &circuit);

        Ok(Self { zkbin, provingkey })
    }

    /// Create ZK data for linear-testnet mode
    /// Note: Linear-testnet uses simple UTXO rewards and doesn't need ZK proofs.
    /// This is a placeholder that returns an error to prevent accidental use.
    pub async fn new_linear(_linear_blockchain: Arc<crate::blockchain::LinearBlockchain>) -> Result<Self> {
        Err(Error::Custom("Linear mining uses LinearPowRewardZk instead".to_string()))
    }
}

/// Linear blockchain miner rewards recipient configuration.
/// Reward value is computed from `dwow_sdk::blockchain::expected_reward(height)`,
/// not configured statically.
#[derive(Debug, Clone)]
pub struct LinearMinerRewardsRecipientConfig {
    /// Public key to receive mining rewards
    pub recipient: PublicKey,
}

impl LinearMinerRewardsRecipientConfig {
    #[allow(dead_code)]
    pub fn new(recipient: PublicKey) -> Self {
        Self { recipient }
    }

    pub async fn from_str(address: &str) -> std::result::Result<Self, RpcError> {
        // Use DarkWow's Address parser which validates:
        // - base58 decode with checksum
        // - network prefix (0x39 mainnet, 0xaf testnet)
        // - 37-byte length
        // - blake3 checksum over [prefix][pubkey]
        let addr = Address::from_str(address)
            .map_err(|_| RpcError::MinerInvalidRecipientPrefix)?;

        // linear-testnet maps to Network::Testnet (prefix 0xaf)
        if addr.network() != Network::Testnet {
            return Err(RpcError::MinerInvalidRecipientPrefix)
        }

        let recipient = *addr.public_key();
        Ok(Self { recipient })
    }
}

/// Linear blockchain block template for mining
#[derive(Debug, Clone)]
pub struct LinearBlockTemplate {
    /// Previous block hash
    pub previous: [u8; 32],
    /// Block height
    pub height: u64,
    /// Difficulty target
    pub target: u32,
    /// Unix timestamp (seconds) — captured once and reused for mining blob + verification
    pub timestamp: u64,
    /// Coinbase reward value
    pub value: u64,
    /// ZK proof for the coinbase transaction
    pub zk_proof: Vec<u8>,
    /// ZK public inputs: [coin, value_commit.x, value_commit.y, token_commit]
    pub zk_public_inputs: [[u8; 32]; 4],
    /// Coin commitment (poseidon hash of coin attributes)
    pub coin: [u8; 32],
    /// Pedersen value commitment x-coordinate (32 bytes)
    pub value_commit_x: [u8; 32],
    /// Pedersen value commitment y-coordinate (32 bytes)
    pub value_commit_y: [u8; 32],
    /// Poseidon token commitment
    pub token_commit: [u8; 32],
    /// AEAD encrypted note (contains coin blinds, value, token_id for recipient)
    pub encrypted_note: Vec<u8>,
    /// Coin merkle root after including this block's coinbase coin
    pub coin_merkle_root: [u8; 32],
    /// Nullifier root (all spent nullifiers)
    pub nullifier_root: [u8; 32],
}

impl LinearBlockTemplate {
    /// Create a coinbase output for the miner (transparent fallback)
    #[allow(dead_code)]
    pub fn create_coinbase_output(recipient: &PublicKey, value: u64) -> Output {
        Output {
            value,
            script: recipient.to_bytes().to_vec(),
        }
    }
}

/// Build a privacy-preserving coinbase transaction for the linear blockchain.
///
/// Uses the Mint_V1 ZK circuit to create:
/// 1. A ZK proof that the coin was correctly minted
/// 2. Pedersen value commitment (hidden value)
/// 3. Poseidon token commitment (hidden token)
/// 4. Poseidon coin commitment (hash of all attributes)
/// 5. AEAD encrypted note containing coin blinds and block signing secret
///
/// The recipient can decrypt the note with their secret key to recover
/// the coin's blinding factors, enabling them to spend the coin later.
pub async fn build_linear_coinbase(
    recipient: PublicKey,
    value: u64,
    linear_zk: &LinearPowRewardZk,
) -> Result<(
    dwow_linear::CoinbaseTransaction,
    [[u8; 32]; 4],
)> {
    use dwow_native_token_contract::client::pow_reward_v1::PoWRewardCallBuilder;
    use dwow_sdk::crypto::Keypair;
    use dwow_sdk::crypto::pasta_prelude::{Curve, CurveAffine};
    use dwow_serial::Encodable;
    use rand::rngs::OsRng;

    // Generate an ephemeral keypair for the block signer
    // Its secret is embedded in the encrypted note's memo field
    let block_signing_keypair = Keypair::random(&mut OsRng);

    // Build the PoW reward using the same PoWRewardCallBuilder as overlay
    let debris = PoWRewardCallBuilder {
        signature_keypair: block_signing_keypair,
        block_height: 0, // linear chain uses u64 heights; 0 is fine for mint
        fees: 0,
        recipient: Some(recipient),
        spend_hook: None,
        user_data: None,
        mint_zkbin: linear_zk.zkbin.clone(),
        mint_pk: linear_zk.provingkey.clone(),
    }
    .build_with_custom_reward(value)?;

    let params = &debris.params;
    let output = &params.output;

    // Extract public inputs from ZK proof
    let coin_bytes: [u8; 32] = output.coin.inner().to_repr();

    let valcom_coords = output.value_commit.to_affine().coordinates().unwrap();
    let mut value_commit_x = [0u8; 32];
    let mut value_commit_y = [0u8; 32];
    value_commit_x.copy_from_slice(&valcom_coords.x().to_repr());
    value_commit_y.copy_from_slice(&valcom_coords.y().to_repr());

    let token_commit_bytes: [u8; 32] = output.token_commit.to_repr();

    let public_inputs: [[u8; 32]; 4] = [
        coin_bytes,
        value_commit_x,
        value_commit_y,
        token_commit_bytes,
    ];

    // Serialize the ZK proofs using dwow_serial Encodable
    let mut proof_bytes = vec![];
    for proof in &debris.proofs {
        proof.encode(&mut proof_bytes)
            .map_err(|e| Error::Custom(format!("Failed to encode ZK proof: {}", e)))?;
    }

    // Serialize the encrypted note using dwow_serial Encodable
    let mut note_bytes = vec![];
    output.note.encode(&mut note_bytes)
        .map_err(|e| Error::Custom(format!("Failed to encode encrypted note: {}", e)))?;

    let coinbase = dwow_linear::CoinbaseTransaction {
        proof: proof_bytes,
        public_inputs,
        coin: coin_bytes,
        value_commit_x,
        value_commit_y,
        token_commit: token_commit_bytes,
        encrypted_note: note_bytes,
    };

    Ok((coinbase, public_inputs))
}

/// Linear blockchain ZK mining data.
/// Loads the Mint_V1 ZK circuit and proving key for creating privacy-preserving
/// coinbase transactions. Uses the same ZK infrastructure as the overlay DAG.
#[derive(Clone)]
pub struct LinearPowRewardZk {
    pub zkbin: ZkBinary,
    pub provingkey: ProvingKey,
}

impl LinearPowRewardZk {
    pub async fn new(_linear_blockchain: Arc<crate::blockchain::LinearBlockchain>) -> Result<Self> {
        info!(
            target: "dwowd::registry::model::LinearPowRewardZk::new",
            "Initializing linear ZK mining data...",
        );

        let zkbin = ZkBinary::decode(NATIVE_TOKEN_CONTRACT_ZKAS_MINT_V1_BIN, false)
            .map_err(|e| Error::Custom(format!("Failed to decode Mint_V1 ZK binary: {}", e)))?;

        let circuit = ZkCircuit::new(empty_witnesses(&zkbin)?, &zkbin);
        let provingkey = ProvingKey::build(zkbin.k, &circuit);

        info!(
            target: "dwowd::registry::model::LinearPowRewardZk::new",
            "Mint_V1 ZK circuit loaded (k={})", zkbin.k,
        );

        Ok(Self { zkbin, provingkey })
    }
}

/// Generate next block template for linear blockchain.
/// When `linear_zk` is provided, creates a privacy-preserving ZK coinbase.
/// Otherwise falls back to a transparent coinbase (for development/testing).
pub async fn generate_linear_block_template(
    linear_blockchain: &crate::blockchain::LinearBlockchain,
    recipient_config: &LinearMinerRewardsRecipientConfig,
    linear_zk: Option<&LinearPowRewardZk>,
) -> Result<LinearBlockTemplate> {
    let height = linear_blockchain.get_height() + 1;

    // Previous block hash - use zero hash if this is the first block.
    // Must hash with the previous block's own RandomX key, not the new block's key.
    let previous_hash: [u8; 32] = if height == 1 {
        [0u8; 32]
    } else {
        let latest_block = linear_blockchain.get_latest_block()
            .map_err(|e| Error::Custom(format!("Failed to get latest block: {}", e)))?;
        let prev_key = latest_block.header.randomx_key;
        let prev_vm = linear_blockchain.get_vm(prev_key);
        *latest_block.hash(&prev_vm).as_bytes()
    };

    // Difficulty target - always use consensus, even for blocks after genesis.
    // Reading from the previous block would propagate the genesis block's
    // u32::MAX difficulty to subsequent blocks, breaking PoW.
    let target = {
        let consensus = linear_blockchain.consensus.lock().unwrap();
        consensus.target()
    };

    // Compute block reward from the exponential-decay emission schedule.
    use dwow_sdk::blockchain::expected_reward;
    let reward = expected_reward(height as u32);

    // Capture timestamp once so mining blob and submit verification use the same value
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // Build ZK coinbase if ZK materials are available
    if let Some(zk) = linear_zk {
        let (coinbase, public_inputs) = build_linear_coinbase(
            recipient_config.recipient,
            reward,
            zk,
        ).await?;

        let coin_merkle_root = linear_blockchain.compute_root_including_coin(&coinbase.coin);
        let nullifier_root = linear_blockchain.compute_nullifier_root();

        return Ok(LinearBlockTemplate {
            previous: previous_hash,
            height,
            target,
            timestamp,
            value: reward,
            zk_proof: coinbase.proof,
            zk_public_inputs: public_inputs,
            coin: coinbase.coin,
            value_commit_x: coinbase.value_commit_x,
            value_commit_y: coinbase.value_commit_y,
            token_commit: coinbase.token_commit,
            encrypted_note: coinbase.encrypted_note,
            coin_merkle_root,
            nullifier_root,
        });
    }

    // Fallback: transparent coinbase (no ZK proof)
    Ok(LinearBlockTemplate {
        previous: previous_hash,
        height,
        target,
        timestamp,
        value: reward,
        zk_proof: vec![],
        zk_public_inputs: [[0u8; 32]; 4],
        coin: [0u8; 32],
        value_commit_x: [0u8; 32],
        value_commit_y: [0u8; 32],
        token_commit: [0u8; 32],
        encrypted_note: vec![],
        coin_merkle_root: [0u8; 32],
        nullifier_root: [0u8; 32],
    })
}

/// Auxiliary function to generate next mining block template, in an
/// atomic manner.
pub async fn generate_next_block_template(
    extended_fork: &mut Fork,
    recipient_config: &MinerRewardsRecipientConfig,
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    verify_fees: bool,
) -> Result<BlockTemplate> {
    // Grab forks' last block proposal(previous)
    let last_proposal = extended_fork.last_proposal()?;

    // Grab forks' next block height
    let next_block_height = last_proposal.block.header.height + 1;

    // Grab forks' RandomX keys for that height
    let randomx_keys = if next_block_height > RANDOMX_KEY_CHANGING_HEIGHT &&
        next_block_height % RANDOMX_KEY_CHANGING_HEIGHT == RANDOMX_KEY_CHANGE_DELAY
    {
        (
            extended_fork
                .module
                .darkfi_rx_keys
                .1
                .ok_or_else(|| Error::ParseFailed("darkfi_rx_keys.1 unwrap() error"))?,
            None,
        )
    } else {
        extended_fork.module.darkfi_rx_keys
    };

    // Grab forks' next mine target and difficulty
    let (target, difficulty) = extended_fork.module.next_mine_target_and_difficulty()?;

    // The target should be compacted to 8 bytes. We'll send the MSB.
    let target_bytes = target.to_bytes_le();
    let mut padded = [0u8; 32];
    let len = target_bytes.len().min(32);
    padded[..len].copy_from_slice(&target_bytes[..len]);
    let target = padded[24..32].to_vec();

    // Cast difficulty to f64. This should always work.
    let difficulty = difficulty.to_string().parse()?;

    // Grab forks' unproposed transactions
    let (mut txs, _, fees) = extended_fork.unproposed_txs(next_block_height, verify_fees).await?;

    // Create an ephemeral block signing keypair. Its secret key will
    // be stored in the PowReward transaction's encrypted note for
    // later retrieval. It is encrypted towards the recipient's public
    // key.
    let block_signing_keypair = Keypair::random(&mut OsRng);

    // Generate reward transaction
    let tx = generate_transaction(
        next_block_height,
        fees,
        &block_signing_keypair,
        recipient_config,
        zkbin,
        pk,
    )?;

    // Apply producer transaction in the forks' overlay
    let _ = apply_producer_transaction(
        &extended_fork.overlay,
        next_block_height,
        extended_fork.module.target,
        &tx,
        &mut MerkleTree::new(1),
    )
    .await?;
    txs.push(tx);

    // Grab the updated contracts states root
    let diff =
        extended_fork.overlay.lock().unwrap().overlay.lock().unwrap().diff(&extended_fork.diffs)?;
    let state_root =
        extended_fork.overlay.lock().unwrap().contracts.update_state_monotree(&diff)?;

    // Generate the new header
    let mut header =
        Header::new(last_proposal.hash, next_block_height, 0, Timestamp::current_time());
    header.state_root = state_root;

    // Generate the block
    let mut next_block = BlockInfo::new_empty(header);

    // Add transactions to the block
    next_block.append_txs(txs);

    Ok(BlockTemplate::new(
        next_block,
        diff.new_trees(),
        randomx_keys,
        target,
        difficulty,
        block_signing_keypair.secret,
    ))
}

/// Auxiliary function to generate a Money::PoWReward transaction.
fn generate_transaction(
    block_height: u32,
    fees: u64,
    block_signing_keypair: &Keypair,
    recipient_config: &MinerRewardsRecipientConfig,
    zkbin: &ZkBinary,
    pk: &ProvingKey,
) -> Result<Transaction> {
    // Build the transaction debris
    let debris = PoWRewardCallBuilder {
        signature_keypair: *block_signing_keypair,
        block_height,
        fees,
        recipient: Some(*recipient_config.recipient.public_key()),
        spend_hook: recipient_config.spend_hook.map(|h| h.inner()),
        user_data: recipient_config.user_data,
        mint_zkbin: zkbin.clone(),
        mint_pk: pk.clone(),
    }
    .build()?;

    // Generate and sign the actual transaction
    let mut data = vec![NativeTokenFunction::PoWRewardV1 as u8];
    debris.params.encode(&mut data)?;
    let call = ContractCall { contract_id: *NATIVE_TOKEN_CONTRACT_ID, data };
    let mut tx_builder =
        TransactionBuilder::new(ContractCallLeaf { call, proofs: debris.proofs }, vec![])?;
    let mut tx = tx_builder.build()?;
    let sigs = tx.create_sigs(&[block_signing_keypair.secret])?;
    tx.signatures = vec![sigs];

    Ok(tx)
}
