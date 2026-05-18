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

//! 5-Node Linear Local Testnet Harness

use std::sync::Arc;

use dwow_linear::{Block, LinearStore, PoWConsensus, create_block};
use dwow_sdk::{crypto::DEPLOYOOOR_CONTRACT_ID, pasta::pallas};
use randomx::{RandomXFlags, RandomXVM};
use sled::Config;

use crate::blockchain::LinearBlockchain;

/// Linear blockchain node for local testing
#[derive(Clone)]
pub struct LinearNode {
    pub blockchain: Arc<LinearBlockchain>,
    pub store: Arc<LinearStore>,
}

/// 5-Node Linear Harness
pub struct LinearFiveNodeHarness {
    pub alice: LinearNode,
    pub bob: LinearNode,
    pub charlie: LinearNode,
    pub david: LinearNode,
    pub eve: LinearNode,
    /// RandomX VM for block hashing (used in mining)
    pub vm: Arc<RandomXVM>,
}

impl LinearFiveNodeHarness {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let db_alice = Config::new().temporary(true).open()?;
        let db_bob = Config::new().temporary(true).open()?;
        let db_charlie = Config::new().temporary(true).open()?;
        let db_david = Config::new().temporary(true).open()?;
        let db_eve = Config::new().temporary(true).open()?;

        // Create RandomX VM for block hashing/mining
        let flags = RandomXFlags::default();
        let cache = randomx::RandomXCache::new(flags, &[0u8; 32])?;
        let vm = Arc::new(RandomXVM::new(flags, Some(cache), None)?);

        let store_alice = Arc::new(LinearStore::new(Arc::new(db_alice))?);
        let store_bob = Arc::new(LinearStore::new(Arc::new(db_bob))?);
        let store_charlie = Arc::new(LinearStore::new(Arc::new(db_charlie))?);
        let store_david = Arc::new(LinearStore::new(Arc::new(db_david))?);
        let store_eve = Arc::new(LinearStore::new(Arc::new(db_eve))?);

        let blockchain_alice = LinearBlockchain::new(store_alice.clone());
        let blockchain_bob = LinearBlockchain::new(store_bob.clone());
        let blockchain_charlie = LinearBlockchain::new(store_charlie.clone());
        let blockchain_david = LinearBlockchain::new(store_david.clone());
        let blockchain_eve = LinearBlockchain::new(store_eve.clone());

        let alice = LinearNode { blockchain: Arc::new(blockchain_alice), store: store_alice };
        let bob = LinearNode { blockchain: Arc::new(blockchain_bob), store: store_bob };
        let charlie = LinearNode { blockchain: Arc::new(blockchain_charlie), store: store_charlie };
        let david = LinearNode { blockchain: Arc::new(blockchain_david), store: store_david };
        let eve = LinearNode { blockchain: Arc::new(blockchain_eve), store: store_eve };

        Ok(Self { alice, bob, charlie, david, eve, vm })
    }

    /// Deploy genesis contracts to all 5 nodes
    pub fn deploy_genesis_contracts(&self) -> Result<(), Box<dyn std::error::Error>> {
        let deployooor_wasm =
            include_bytes!("../../../../src/contract/deployooor/dwow_deployooor_contract.wasm").to_vec();
        let native_token_wasm =
            include_bytes!("../../../../src/contract/native_token/dwow_native_token_contract.wasm").to_vec();

        let native_token_id = dwow_sdk::crypto::ContractId::from(pallas::Base::from(42));

        for node in self.all_nodes() {
            node.blockchain.deploy_contract(&deployooor_wasm, *DEPLOYOOOR_CONTRACT_ID)?;
            node.blockchain.deploy_contract(&native_token_wasm, native_token_id)?;
        }

        Ok(())
    }

    pub fn all_nodes(&self) -> [&LinearNode; 5] {
        [&self.alice, &self.bob, &self.charlie, &self.david, &self.eve]
    }

    /// Alice mines genesis block
    pub fn alice_create_genesis(&self) -> Block {
        let difficulty_target = 0x0000_FFFF;
        let previous = blake3::hash(&[]);
        let mut block = create_block(previous, 0, vec![], difficulty_target, &*self.vm);

        let consensus = PoWConsensus::new(60, difficulty_target);
        while !consensus.check_difficulty(&block.hash(&*self.vm)) {
            block.header.nonce += 1;
        }
        block
    }

    /// Alice mines a block on top of the given previous hash
    pub fn alice_mine_block(&self, height: u64, previous: blake3::Hash) -> Block {
        let difficulty_target = 0x0000_FFFF;
        let mut block = create_block(previous, height, vec![], difficulty_target, &*self.vm);

        let consensus = PoWConsensus::new(60, difficulty_target);
        while !consensus.check_difficulty(&block.hash(&*self.vm)) {
            block.header.nonce += 1;
        }
        block
    }

    /// Apply a block to all nodes (simulating P2P broadcast + sync)
    pub fn broadcast_block(&self, block: &Block) -> Result<(), Box<dyn std::error::Error>> {
        for node in self.all_nodes() {
            let block_clone = block.clone();
            let blockchain = node.blockchain.clone();
            smol::block_on(async {
                blockchain.apply_block(&block_clone).await
            })?;
        }
        Ok(())
    }

    pub fn verify_sync(&self) -> Result<(), Box<dyn std::error::Error>> {
        let alice_height = self.alice.blockchain.get_height();
        for (i, node) in self.all_nodes().iter().enumerate() {
            let height = node.blockchain.get_height();
            if height != alice_height {
                return Err(format!("Node {} height {} != alice height {}", i, height, alice_height).into());
            }
        }
        Ok(())
    }
}

impl Default for LinearFiveNodeHarness {
    fn default() -> Self {
        Self::new().expect("Failed to create LinearFiveNodeHarness")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dwow_linear::{create_block_with_uncles, create_uncle, UncleBlock};

    #[test]
    fn test_linear_five_node_consensus() -> Result<(), Box<dyn std::error::Error>> {
        let harness = LinearFiveNodeHarness::new()?;

        // Deploy genesis contracts
        harness.deploy_genesis_contracts()?;

        // Alice creates genesis block (ONE single genesis)
        let genesis_block = harness.alice_create_genesis();
        let genesis_hash = genesis_block.hash(&*harness.vm);

        // Broadcast genesis to all nodes (including Alice)
        harness.broadcast_block(&genesis_block)?;

        // Verify all nodes have genesis at height 0
        for node in harness.all_nodes() {
            assert_eq!(node.blockchain.get_height(), 0);
        }

        // Alice mines blocks 1-5, each broadcast to all
        let mut previous = genesis_hash;
        for height in 1..=5 {
            let block = harness.alice_mine_block(height, previous);
            harness.broadcast_block(&block)?;
            previous = block.hash(&*harness.vm);
        }

        // Verify all nodes agree on height 5
        harness.verify_sync()?;
        for node in harness.all_nodes() {
            assert_eq!(node.blockchain.get_height(), 5);
        }

        Ok(())
    }

    #[test]
    fn test_linear_block_with_uncles() -> Result<(), Box<dyn std::error::Error>> {
        let harness = LinearFiveNodeHarness::new()?;
        let vm = &*harness.vm;

        // Deploy genesis contracts
        harness.deploy_genesis_contracts()?;

        // Alice creates genesis block
        let genesis_block = harness.alice_create_genesis();
        let genesis_hash = genesis_block.hash(vm);

        // Broadcast genesis to all nodes
        harness.broadcast_block(&genesis_block)?;

        // Apply genesis to get chain state
        let blockchain = &harness.alice.blockchain;

        // Mine canonical block at height 1 (this extends genesis)
        let difficulty_target = 0x0000_FFFF;
        let canonical_block = harness.alice_mine_block(1, genesis_hash);

        // Apply canonical block
        smol::block_on(async {
            blockchain.apply_block(&canonical_block).await
        })?;

        // Verify height is 1
        assert_eq!(blockchain.get_height(), 1);

        // Now mine an uncle block at the same height
        let base_reward = 100_000_000;
        let uncle_block = harness.alice_mine_block(1, genesis_hash);
        let mut uncle = create_uncle(uncle_block.clone(), 1, base_reward);

        // Verify pin is offered but not yet accepted
        assert!(uncle.pin_offered);
        assert!(!uncle.pin_accepted);

        // Uncle chain accepts the pin (use it or lose it - one time decision)
        // Note: Rejection is strictly dominated - accepting gives 50M, rejecting gives 0
        uncle.accept_pin();
        assert!(uncle.pin_accepted);

        // Create canonical block at height 2 that includes the uncle
        let canonical_hash = canonical_block.hash(vm);
        let mut canonical_block2 =
            create_block_with_uncles(canonical_hash, 2, vec![], difficulty_target, &[uncle.clone()], vm);

        // Mine the canonical block so it satisfies PoW
        let consensus = PoWConsensus::new(60, difficulty_target);
        while !consensus.check_difficulty(&canonical_block2.hash(vm)) {
            canonical_block2.header.nonce += 1;
        }

        // Verify uncle merkle root is set
        assert_ne!(canonical_block2.header.uncle_merkle_root, [0u8; 32]);

        // Apply canonical block with uncle
        smol::block_on(async {
            blockchain.apply_block_with_uncles(&canonical_block2, &[uncle]).await
        })?;

        // Verify block was applied
        assert_eq!(blockchain.get_height(), 2);

        // Verify uncle was stored with pin_accepted = true
        let stored_uncle = blockchain.store.get_uncle(uncle_block.hash(vm).as_bytes())?;
        assert!(stored_uncle.is_some());
        let stored_uncle = stored_uncle.unwrap();
        assert!(stored_uncle.pin_accepted);
        assert_eq!(stored_uncle.pin_reward, 50_000_000);

        Ok(())
    }

    #[test]
    fn test_pin_reject_flow() -> Result<(), Box<dyn std::error::Error>> {
        let harness = LinearFiveNodeHarness::new()?;
        let vm = &*harness.vm;
        harness.deploy_genesis_contracts()?;
        let genesis_block = harness.alice_create_genesis();
        let genesis_hash = genesis_block.hash(vm);
        harness.broadcast_block(&genesis_block)?;
        let blockchain = &harness.alice.blockchain;

        // Mine canonical block at height 1
        let difficulty_target = 0x0000_FFFF;
        let canonical_block = harness.alice_mine_block(1, genesis_hash);
        smol::block_on(async {
            blockchain.apply_block(&canonical_block).await
        })?;

        // Mine uncle block at same height
        let base_reward = 100_000_000;
        let uncle_block = harness.alice_mine_block(1, genesis_hash);
        let mut uncle = create_uncle(uncle_block.clone(), 1, base_reward);

        // Uncle chain rejects the pin (gives up reward)
        // Note: This is irrational but we're testing the flow
        uncle.reject_pin();
        assert!(!uncle.pin_accepted);

        // Create canonical block at height 2 that includes the uncle
        let canonical_hash = canonical_block.hash(vm);
        let mut canonical_block2 =
            create_block_with_uncles(canonical_hash, 2, vec![], difficulty_target, &[uncle.clone()], vm);

        // Mine the canonical block so it satisfies PoW
        let consensus = PoWConsensus::new(60, difficulty_target);
        while !consensus.check_difficulty(&canonical_block2.hash(vm)) {
            canonical_block2.header.nonce += 1;
        }

        // Apply canonical block with uncle
        smol::block_on(async {
            blockchain.apply_block_with_uncles(&canonical_block2, &[uncle]).await
        })?;

        // Verify block was applied
        assert_eq!(blockchain.get_height(), 2);

        // Verify uncle was stored with pin_accepted = false (rejected)
        let stored_uncle = blockchain.store.get_uncle(uncle_block.hash(vm).as_bytes())?;
        assert!(stored_uncle.is_some());
        let stored_uncle = stored_uncle.unwrap();
        assert!(!stored_uncle.pin_accepted);
        // Uncle gets 0 reward when rejected, canonical absorbs everything

        Ok(())
    }
}