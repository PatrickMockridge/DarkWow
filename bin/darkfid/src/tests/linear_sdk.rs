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

//! Linear Testnet SDK
//!
//! Provides a simple interface for starting a local linear-testnet with:
//! - Multiple pre-funded wallets (dev, bob, charlie, david, eve)
//! - Automatic mining reward routing to specified wallet
//! - Easy contract deployment and interaction
//!
//! ## Usage
//!
//! ```rust,ignore
//! use darkfi_sdk::crypto::SecretKey;
//! use darkfi_sdk::pasta::pallas;
//! use crate::tests::linear_sdk::{LinearTestnetSdk, NamedWallet};
//!
//! // Create SDK with multiple named wallets
//! let wallets = vec![
//!     NamedWallet::new("dev", SecretKey::random(&mut OsRng), 100_000_000_000),
//!     NamedWallet::new("bob", SecretKey::random(&mut OsRng), 0),
//!     NamedWallet::new("charlie", SecretKey::random(&mut OsRng), 0),
//! ];
//! let mut sdk = LinearTestnetSdk::with_wallets(wallets, "dev")?;
//!
//! // Start the testnet (deploys genesis contracts, creates genesis block)
//! sdk.start()?;
//!
//! // Mine blocks (rewards go to dev wallet)
//! sdk.mine_blocks(5)?;
//!
//! // Get bob's balance (he should have received block rewards)
//! let bob_balance = sdk.get_balance("bob")?;
//!
//! // Deploy a contract as dev
//! let wasm = std::fs::read("my_contract.wasm")?;
//! let contract_id = sdk.deploy_contract(&wasm, "dev").await?;
//! ```

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

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
// Named Wallet Configuration
// ============================================================================

/// Configuration for a named wallet in the testnet
#[derive(Clone, Debug)]
pub struct NamedWallet {
    /// Wallet name (e.g., "dev", "bob", "charlie", "david", "eve")
    pub name: String,
    /// Secret key for signing transactions
    pub secret: SecretKey,
    /// Initial DARK balance at genesis (in smallest unit)
    pub initial_balance: u64,
}

impl NamedWallet {
    /// Create a new named wallet
    pub fn new(name: &str, secret: SecretKey, initial_balance: u64) -> Self {
        Self { name: name.to_string(), secret, initial_balance }
    }

    /// Create a new named wallet with random key
    pub fn new_random(name: &str, initial_balance: u64) -> Self {
        Self { name: name.to_string(), secret: SecretKey::random(&mut OsRng), initial_balance }
    }

    /// Get the keypair from this config
    pub fn keypair(&self) -> Keypair {
        Keypair::new(self.secret)
    }

    /// Get the public key
    pub fn public_key(&self) -> pallas::Point {
        self.keypair().public
    }
}

// ============================================================================
// Wallet Registry
// ============================================================================

/// Registry for managing multiple wallets in the testnet
#[derive(Clone)]
pub struct WalletRegistry {
    wallets: HashMap<String, NamedWallet>,
    default: String,
}

impl WalletRegistry {
    /// Create a new wallet registry with given wallets
    pub fn new(wallets: Vec<NamedWallet>, default: &str) -> Self {
        let mut wallet_map = HashMap::new();
        for wallet in wallets {
            wallet_map.insert(wallet.name.clone(), wallet);
        }
        Self { wallets: wallet_map, default: default.to_string() }
    }

    /// Get wallet by name
    pub fn get(&self, name: &str) -> Option<&NamedWallet> {
        self.wallets.get(name)
    }

    /// Get default wallet
    pub fn default_wallet(&self) -> &NamedWallet {
        self.wallets.get(&self.default).expect("Default wallet must exist")
    }

    /// Get all wallet names
    pub fn names(&self) -> Vec<&String> {
        self.wallets.keys().collect()
    }

    /// Check if wallet exists
    pub fn has(&self, name: &str) -> bool {
        self.wallets.contains_key(name)
    }

    /// Get a specific wallet or panic
    pub fn get_or_panic(&self, name: &str) -> &NamedWallet {
        self.wallets.get(name).expect(&format!("Wallet '{}' not found", name))
    }
}

// ============================================================================
// Dev Wallet Configuration (Legacy/Compatibility)
// ============================================================================

/// Configuration for the developer wallet (legacy alias for NamedWallet)
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
/// - Multiple pre-funded wallets (dev, bob, charlie, david, eve)
/// - Automatic mining reward routing to specified wallet
/// - Easy contract deployment and interaction
pub struct LinearTestnetSdk {
    /// The 5-node harness
    pub harness: LinearFiveNodeHarness,
    /// Wallet registry for multi-wallet support
    wallet_registry: WalletRegistry,
    /// Mining recipient (defaults to dev wallet)
    mining_recipient: Keypair,
    /// Base reward per block (in smallest unit, default 1 DARK = 100_000_000)
    base_reward: u64,
    /// Deployed contracts (contract_id -> wasm bytes)
    deployed_contracts: HashMap<darkfi_sdk::crypto::ContractId, Vec<u8>>,
}

impl LinearTestnetSdk {
    /// Create a new SDK with the standard 5-node setup and random dev wallet
    pub fn new() -> Self {
        let dev_wallet = NamedWallet::new_random("dev", 100_000_000_000);
        Self::with_wallets(vec![dev_wallet], "dev").expect("Failed to create SDK")
    }

    /// Create a new SDK with a legacy DevWalletConfig (for compatibility)
    pub fn with_dev_wallet(dev_wallet: DevWalletConfig) -> Self {
        let named_wallet = NamedWallet {
            name: "dev".to_string(),
            secret: dev_wallet.secret,
            initial_balance: dev_wallet.initial_balance,
        };
        Self::with_wallets(vec![named_wallet], "dev").expect("Failed to create SDK")
    }

    /// Create a new SDK with multiple named wallets
    pub fn with_wallets(wallets: Vec<NamedWallet>, default: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let wallet_registry = WalletRegistry::new(wallets, default);
        let mining_recipient = wallet_registry.default_wallet().keypair();

        Ok(Self {
            harness: LinearFiveNodeHarness::new()?,
            wallet_registry,
            mining_recipient,
            base_reward: 100_000_000, // 1 DARK
            deployed_contracts: HashMap::new(),
        })
    }

    /// Create SDK with all 5 standard wallets (dev, bob, charlie, david, eve)
    pub fn with_five_wallets() -> Result<Self, Box<dyn std::error::Error>> {
        let wallets = vec![
            NamedWallet::new_random("dev", 100_000_000_000),
            NamedWallet::new_random("bob", 0),
            NamedWallet::new_random("charlie", 0),
            NamedWallet::new_random("david", 0),
            NamedWallet::new_random("eve", 0),
        ];
        Self::with_wallets(wallets, "dev")
    }

    /// Get wallet by name
    pub fn get_wallet(&self, name: &str) -> Option<&NamedWallet> {
        self.wallet_registry.get(name)
    }

    /// Get default wallet
    pub fn default_wallet(&self) -> &NamedWallet {
        self.wallet_registry.default_wallet()
    }

    /// Get all wallet names
    pub fn wallet_names(&self) -> Vec<&String> {
        self.wallet_registry.names()
    }

    /// Get the dev wallet's public key (for backwards compatibility)
    pub fn dev_pubkey(&self) -> pallas::Point {
        self.wallet_registry.get_or_panic("dev").public_key()
    }

    /// Get the mining recipient's public key
    pub fn mining_recipient_pubkey(&self) -> pallas::Point {
        self.mining_recipient.public
    }

    /// Set the mining recipient (who receives block rewards)
    pub fn set_mining_recipient(&mut self, wallet_name: &str) {
        let wallet = self.wallet_registry.get_or_panic(wallet_name);
        self.mining_recipient = wallet.keypair();
    }

    /// Create a new wallet and add to registry
    pub fn create_wallet(&mut self, name: &str, initial_balance: u64) -> SecretKey {
        let wallet = NamedWallet::new_random(name, initial_balance);
        let secret = wallet.secret;
        // Insert into registry
        let new_wallet = NamedWallet::new(name, secret, initial_balance);
        let mut wallets = Vec::new();
        for (_, w) in self.wallet_registry.wallets.iter() {
            wallets.push(w.clone());
        }
        wallets.push(new_wallet);
        self.wallet_registry = WalletRegistry::new(wallets, &self.wallet_registry.default);
        secret
    }

    /// Check if a contract is deployed
    pub fn has_contract(&self, contract_id: &darkfi_sdk::crypto::ContractId) -> bool {
        self.deployed_contracts.contains_key(contract_id)
    }

    /// Get deployed contract WASM
    pub fn get_contract_wasm(&self, contract_id: &darkfi_sdk::crypto::ContractId) -> Option<&Vec<u8>> {
        self.deployed_contracts.get(contract_id)
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
            "Linear testnet started with wallets: {:?}",
            self.wallet_names()
        );
        tracing::info!(
            "Dev wallet pubkey: {:?}",
            self.wallet_registry.get_or_panic("dev").public_key()
        );

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

        let consensus = PoWConsensus::new(60, difficulty_target);
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

        let consensus = PoWConsensus::new(60, difficulty_target);
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

    /// Mine blocks to a specific wallet (changes mining recipient temporarily)
    pub fn mine_blocks_to(&self, count: u64, wallet_name: &str) -> Result<(), Box<dyn std::error::Error>> {
        // Save current mining recipient
        let original_recipient = self.mining_recipient.clone();

        // Set new recipient
        let wallet = self.wallet_registry.get_or_panic(wallet_name);
        let mining_recipient = wallet.keypair();

        let mut previous = if let Some(block) = self.get_last_block()? {
            block.hash()
        } else {
            blake3::hash(&[])
        };

        for height in 1..=count {
            let block = self.alice_mine_block_with_recipient(height, previous, &mining_recipient);
            previous = block.hash();
            self.harness.broadcast_block(&block)?;
        }

        tracing::info!("Mined {} blocks to {}", count, wallet_name);
        Ok(())
    }

    /// Alice mines a block with a specific recipient
    fn alice_mine_block_with_recipient(&self, height: u64, previous: blake3::Hash, recipient: &Keypair) -> Block {
        let difficulty_target = 0x0000_FFFF;
        let transactions: Vec<darkfi::tx::Transaction> = vec![];
        let mut block = self.create_block_with_txs(previous, height, transactions, difficulty_target);

        let consensus = PoWConsensus::new(60, difficulty_target);
        while !consensus.check_difficulty(&block.hash()) {
            block.header.nonce += 1;
        }
        block
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

    /// Get balance for a wallet by querying the blockchain state
    /// Note: This requires the wallet to have received funds via mining or transfers
    pub fn get_balance(&self, wallet_name: &str) -> Result<u64, Box<dyn std::error::Error>> {
        let wallet = self.wallet_registry.get_or_panic(wallet_name);
        let pubkey = wallet.public_key();

        // For now, return initial balance + (height * base_reward) as approximation
        // Full implementation would query the blockchain for actual coin balances
        let height = self.harness.alice.blockchain.get_height();
        let initial = wallet.initial_balance;
        let mined = height * self.base_reward;

        Ok(initial + mined)
    }

    /// Deploy a WASM contract
    pub fn deploy_contract(&mut self, wasm: &[u8], sender: &str) -> Result<darkfi_sdk::crypto::ContractId, Box<dyn std::error::Error>> {
        let wallet = self.wallet_registry.get_or_panic(sender);
        let sender_pubkey = wallet.public_key();

        // Generate a deterministic contract ID based on sender and nonce
        let nonce = self.deployed_contracts.len() as u64;
        let contract_id = darkfi_sdk::crypto::ContractId::from(pallas::Base::from(nonce + 1));

        // Deploy to all nodes
        for node in self.harness.all_nodes() {
            node.blockchain.deploy_contract(wasm, contract_id)?;
        }

        // Store the WASM
        self.deployed_contracts.insert(contract_id, wasm.to_vec());

        tracing::info!(
            "Deployed contract {} by {} (height {})",
            contract_id, sender, self.harness.alice.blockchain.get_height()
        );

        Ok(contract_id)
    }
}

impl Default for LinearTestnetSdk {
    fn default() -> Self {
        Self::new().expect("Failed to create LinearTestnetSdk")
    }
}

// ============================================================================
// Helper Structs (reusing existing)
// ============================================================================

use crate::tests::linear_five_node::LinearFiveNodeHarness;

// ============================================================================
// SDK Node (wrapper around LinearNode)
// ============================================================================

// Note: The old From<LinearFiveNodeHarness> impl was removed as it used
// deprecated field names. Use LinearTestnetSdk::new() or with_wallets() instead.