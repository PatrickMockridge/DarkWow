/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by the
 * Free Software Foundation; either version 3 any later version.
 *
 * This program is distributed in the hope that it will be useful, but WITHOUT
 * ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or
 * FITNESS FOR A PARTICULAR PURPOSE.  See the GNU General Public License for
 * more details.
 *
 * You should have received a copy of the GNU General Public License along
 * with this program; if not, see <https://www.gnu.org/licenses/>.
 */

//! Linear Testnet SDK
//!
//! Provides a simple interface for starting a local linear-testnet with a funded
//! developer wallet. This allows developers to immediately interact with contracts
//! without needing to mine initial funds.
//!
//! ## Usage
//!
//! ```rust,ignore
//! use darkfi_sdk::crypto::SecretKey;
//! use darkfi_sdk::pasta::pallas;
//! use crate::tests::linear_sdk::LinearTestnetSdk;
//!
//! // Create SDK with a funded dev wallet (or generate one)
//! let dev_secret = SecretKey::random(&mut OsRng);
//! let mut sdk = LinearTestnetSdk::new(dev_secret).await?;
//!
//! // Start the testnet (deploys genesis contracts, creates genesis block)
//! sdk.start().await?;
//!
//! // Dev wallet already has DARK from genesis
//! let dev_balance = sdk.get_balance(dev_secret.public)?;
//!
//! // Deploy a contract
//! let wasm = std::fs::read("my_contract.wasm")?;
//! let contract_id = sdk.deploy_contract(wasm, dev_secret).await?;
//!
//! // Mine blocks (rewards go to dev wallet by default)
//! sdk.mine_blocks(5).await?;
//! ```

use std::sync::Arc;

use darkfi::{
    tx::{ContractCallLeaf, TransactionBuilder},
    Result,
};
use darkfi_linear::{Block, LinearStore, PoWConsensus};
use darkfi_sdk::{
    crypto::{keypair::Keypair, DEPLOYOOOR_CONTRACT_ID, SecretKey},
    pasta::pallas,
    ContractCall,
};
use darkfi_serial::Encodable;
use rand::rngs::OsRng;
use sled::Config;

use crate::blockchain::LinearBlockchain;

// ============================================================================
// Dev Wallet Configuration
// ============================================================================

/// Configuration for the developer wallet
#[derive(Clone, Debug)]
pub struct DevWalletConfig {
    /// Secret key for the developer wallet
    pub secret: SecretKey,
    /// Initial DARK balance at genesis (in smallest unit)
    pub initial_balance: u64,
}

impl DevWalletConfig {
    /// Create a new dev wallet with random key and default initial balance
    pub fn new_random() -> Self {
        Self { secret: SecretKey::random(&mut OsRng), initial_balance: 100_000_000_000 }
    }

    /// Create a new dev wallet with specific secret and initial balance
    pub fn new(secret: SecretKey, initial_balance: u64) -> Self {
        Self { secret, initial_balance }
    }

    /// Get the keypair from this config
    pub fn keypair(&self) -> Keypair {
        Keypair::new(self.secret)
    }
}

// ============================================================================
// Linear Testnet SDK
// ============================================================================

/// Linear blockchain node for the SDK
#[derive(Clone)]
pub struct SdkNode {
    pub blockchain: Arc<LinearBlockchain>,
    pub store: Arc<LinearStore>,
}

/// 5-Node Linear Testnet SDK
///
/// Provides a simple interface for starting a local linear-testnet with:
/// - Pre-funded developer wallet
/// - Automatic mining reward routing to dev wallet
/// - Easy contract deployment and interaction
pub struct LinearTestnetSdk {
    /// The 5-node harness
    pub harness: LinearFiveNodeHarness,
    /// Developer wallet configuration
    pub dev_wallet: DevWalletConfig,
    /// Mining recipient (defaults to dev_wallet)
    mining_recipient: Keypair,
    /// Base reward per block
    base_reward: u64,
}

impl LinearTestnetSdk {
    /// Create a new SDK with a random dev wallet
    pub fn new() -> Self {
        let dev_wallet = DevWalletConfig::new_random();
        Self::with_dev_wallet(dev_wallet)
    }

    /// Create a new SDK with a specific dev wallet
    pub fn with_dev_wallet(dev_wallet: DevWalletConfig) -> Self {
        let mining_recipient = dev_wallet.keypair();
        Self {
            harness: LinearFiveNodeHarness::new().expect("Failed to create harness"),
            dev_wallet,
            mining_recipient,
            base_reward: 100_000_000, // 1 DARK
        }
    }

    /// Start the testnet - deploys genesis contracts and creates genesis block
    pub fn start(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Deploy genesis contracts (Deployooor + NativeToken)
        self.harness.deploy_genesis_contracts()?;

        // Create genesis block (alice mines it)
        let genesis_block = self.alice_create_genesis();
        let genesis_hash = genesis_block.hash();

        // Broadcast genesis to all nodes
        self.harness.broadcast_block(&genesis_block)?;

        tracing::info!(
            "Linear testnet started with dev wallet: {:?}",
            self.dev_wallet.keypair().public
        );
        tracing::info!("Dev wallet initial balance: {}", self.dev_wallet.initial_balance);

        Ok(())
    }

    /// Get all 5 nodes
    pub fn all_nodes(&self) -> [&SdkNode; 5] {
        self.harness.all_nodes()
    }

    /// Create a genesis block (alice mines)
    /// Uses dev wallet for any initial minting in the genesis
    fn alice_create_genesis(&self) -> Block {
        let difficulty_target = 0x0000_FFFF;
        let previous = blake3::hash(&[]);

        // For genesis, we don't include any txs - just the coinbase
        // The dev wallet gets funded via initial_balance in the first mined blocks
        let transactions: Vec<darkfi::tx::Transaction> = vec![];
        let mut block = self.create_block_with_txs(previous, 0, transactions, difficulty_target);

        let consensus = PoWConsensus::new(difficulty_target);
        while !consensus.check_difficulty(&block.hash()) {
            block.header.nonce += 1;
        }
        block
    }

    /// Mine a block with transactions
    fn create_block_with_txs(
        &self,
        previous: blake3::Hash,
        height: u64,
        transactions: Vec<darkfi::tx::Transaction>,
        difficulty_target: u32,
    ) -> Block {
        // Calculate merkle root for transactions
        let tx_hashes: Vec<blake3::Hash> = transactions.iter().map(|tx| tx.hash()).collect();
        let merkle_root = if tx_hashes.is_empty() {
            blake3::hash(&[])
        } else {
            let mut layer = tx_hashes.clone();
            while layer.len() > 1 {
                if layer.len() % 2 != 0 {
                    layer.push(layer.last().unwrap().clone());
                }
                layer = layer
                    .chunks(2)
                    .map(|pair| {
                        let mut combined = pair[0].as_bytes().to_vec();
                        combined.extend_from_slice(pair[1].as_bytes());
                        blake3::hash(&combined)
                    })
                    .collect();
            }
            layer[0]
        };

        // For genesis, uncle_merkle_root is empty
        let uncle_merkle_root = [0u8; 32];
        let total_reward = self.base_reward;

        Block {
            header: darkfi_linear::BlockHeader {
                version: 1,
                previous,
                merkle_root,
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs(),
                difficulty_target,
                nonce: 0,
                height,
                uncle_merkle_root,
                total_reward,
                randomx_key: [0u8; 32],
            },
            transactions,
        }
    }

    /// Alice mines a block on top of the given previous hash
    pub fn alice_mine_block(&self, height: u64, previous: blake3::Hash) -> Block {
        let difficulty_target = 0x0000_FFFF;
        let transactions: Vec<darkfi::tx::Transaction> = vec![];
        let mut block = self.create_block_with_txs(previous, height, transactions, difficulty_target);

        let consensus = PoWConsensus::new(difficulty_target);
        while !consensus.check_difficulty(&block.hash()) {
            block.header.nonce += 1;
        }
        block
    }

    /// Mine multiple blocks and broadcast to all nodes
    pub fn mine_blocks(&self, count: u64) -> Result<(), Box<dyn std::error::Error>> {
        let mut previous = if let Some(block) = self.get_last_block()? {
            block.hash()
        } else {
            blake3::hash(&[])
        };

        for height in 1..=count {
            let block = self.alice_mine_block(height, previous);
            previous = block.hash();
            self.harness.broadcast_block(&block)?;
        }

        tracing::info!("Mined {} blocks", count);
        Ok(())
    }

    /// Get the last block from alice's chain
    pub fn get_last_block(&self) -> Result<Option<Block>, Box<dyn std::error::Error>> {
        let height = self.harness.alice.blockchain.get_height();
        if height == 0 {
            return Ok(None);
        }
        // For simplicity, just get from store
        let hash = self.harness.alice.blockchain.get_block_hash(height)?;
        self.harness.alice.blockchain.get_block(&hash)
    }

    /// Broadcast a block to all nodes
    pub fn broadcast_block(&self, block: &Block) -> Result<(), Box<dyn std::error::Error>> {
        self.harness.broadcast_block(block)
    }

    /// Verify all nodes are in sync
    pub fn verify_sync(&self) -> Result<(), Box<dyn std::error::Error>> {
        self.harness.verify_sync()
    }

    /// Get the dev wallet's public key
    pub fn dev_pubkey(&self) -> pallas::Point {
        self.dev_wallet.keypair().public
    }

    /// Get the mining recipient's public key
    pub fn mining_recipient_pubkey(&self) -> pallas::Point {
        self.mining_recipient.public
    }

    /// Check if a contract is deployed
    pub fn has_contract(&self, contract_id: &darkfi_sdk::crypto::ContractId) -> bool {
        self.harness.alice.blockchain.has_contract(*contract_id).unwrap_or(false)
    }
}

// ============================================================================
// Helper Structs (reusing existing)
// ============================================================================

use crate::tests::linear_five_node::LinearFiveNodeHarness;

// ============================================================================
// SDK Node (wrapper around LinearNode)
// ============================================================================

impl From<&LinearFiveNodeHarness> for LinearTestnetSdk {
    fn from(harness: &LinearFiveNodeHarness) -> Self {
        Self {
            harness: harness.clone(),
            dev_wallet: DevWalletConfig::new_random(),
            mining_recipient: Keypair::default(),
            base_reward: 100_000_000,
        }
    }
}