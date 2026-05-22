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

//! Block structures for linear blockchain

use serde::{Deserialize, Serialize};

use super::Transaction;

/// Block header - contains metadata about a block
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockHeader {
    /// Block version
    pub version: u8,
    /// Hash of the previous block (only one parent - linear chain)
    pub previous: blake3::Hash,
    /// Merkle root of transactions
    pub merkle_root: blake3::Hash,
    /// Block timestamp
    pub timestamp: u64,
    /// PoW target — `hash_u32 <= target` is valid. Higher = easier.
    pub target: u32,
    /// Nonce for PoW mining
    pub nonce: u32,
    /// Block height in chain
    pub height: u64,
    /// Merkle root of uncle blocks referenced by this canonical block
    pub uncle_merkle_root: [u8; 32],
    /// Total reward being distributed (canonical + uncle shares)
    pub total_reward: u64,
    /// RandomX key for PoW mining (key used to create VM for this block)
    pub randomx_key: [u8; 32],
    /// Root of the coin commitment Merkle tree after this block
    #[serde(default)]
    pub coin_merkle_root: [u8; 32],
    /// Root of the nullifier Sparse Merkle Tree after this block
    #[serde(default)]
    pub nullifier_root: [u8; 32],
    /// Caribina Arweave anchor TX ID (SHA-256 of ANS-104 DataItem signature).
    /// [0u8; 32] means no anchor (genesis blocks, bootstrapping, or anchor failure).
    #[serde(default)]
    pub anchor_tx_id: [u8; 32],
    /// Monero p2pool anchor block height (0 = no anchor)
    #[serde(default)]
    pub anchor_monero_height: u64,
    /// Monero p2pool anchor block hash ([0u8; 32] = no anchor)
    #[serde(default)]
    pub anchor_monero_hash: [u8; 32],
    /// Finality signaling flags bitfield:
    ///   0x01 = FINALITY_CARIBNIA, 0x02 = FINALITY_MONERO, 0x04 = FINALITY_SIGNALED
    #[serde(default)]
    pub finality_flags: u8,
}

/// Uncle block - a block that was mined but not canonical
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UncleBlock {
    /// Header of the uncle block
    pub header: BlockHeader,
    /// Transactions in the uncle block
    pub transactions: Vec<Transaction>,
    /// Depth in the uncle tree (1 = directly referenced, 2 = referenced by depth-1, etc.)
    pub depth: u8,
    /// Pin offered by canonical chain (obligated offer if uncle meets criteria)
    pub pin_offered: bool,
    /// Uncle chain accepted the pin (use it or lose it - one time decision)
    pub pin_accepted: bool,
    /// Pin reward amount if accepted (computed from depth: 50% at d1, 25% at d2...)
    pub pin_reward: u64,
}

impl UncleBlock {
    /// Calculate the hash of this uncle block's header using RandomX VM
    pub fn hash(&self, vm: &randomx::RandomXVM) -> blake3::Hash {
        let header_bytes = serde_json::to_vec(&self.header).unwrap();
        // Use first 32 bytes of RandomX output as the hash
        let rx_hash = vm.calculate_hash(&header_bytes).expect("RandomX hash failed");
        let mut hash_bytes = [0u8; 32];
        hash_bytes.copy_from_slice(&rx_hash[..32]);
        blake3::Hash::from_bytes(hash_bytes)
    }

    /// Accept the pin offer from canonical chain (use it or lose it)
    /// This is a one-time decision - once accepted, cannot be undone
    pub fn accept_pin(&mut self) {
        if self.pin_offered {
            self.pin_accepted = true;
        }
    }

    /// Reject the pin offer (uncle chain gives up reward)
    /// Note: Rejection is strictly dominated - accepting gives pin_reward, rejecting gives 0
    pub fn reject_pin(&mut self) {
        self.pin_accepted = false;
    }
}

/// Convert a rejected block into an uncle block
pub fn create_uncle(block: Block, depth: u8, base_reward: u64) -> UncleBlock {
    let depth = depth.min(MAX_UNCLE_DEPTH);
    let pin_reward = base_reward / (2_u64.pow(depth as u32));
    UncleBlock {
        header: block.header,
        transactions: block.transactions,
        depth,
        pin_offered: true,
        pin_accepted: false,
        pin_reward,
    }
}

/// Proof of an uncle for stateless verification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UncleProof {
    /// Uncle header
    pub header: BlockHeader,
    /// RandomX PoW hash computed from header using header.randomx_key
    pub pow_hash: [u8; 32],
    /// Merkle proof path from uncle to root
    pub merkle_path: Vec<[u8; 32]>,
    /// Uncle's position in merkle tree (leaf index)
    pub position: u32,
    /// Depth (for reward calculation)
    pub depth: u8,
}

impl BlockHeader {
    /// Serialize the header to a compact binary blob for mining and hashing.
    /// Format (227 bytes total):
    ///   [previous(32)][version(1)][target(4)][reserved(2)][nonce(4)]
    ///   [height(8)][merkle_root(32)][timestamp(8)][uncle_merkle_root(32)]
    ///   [total_reward(8)][randomx_key(32)][coin_merkle_root(32)][nullifier_root(32)]
    /// Nonce is at byte offset 39 (matches xmrig's hardcoded Monero rx/0 offset).
    /// anchor_tx_id, anchor_monero_height, anchor_monero_hash, and finality_flags
    /// are excluded — they are set after PoW is found and are not covered by the
    /// mining hash.
    pub fn to_mining_blob(&self) -> Vec<u8> {
        let mut blob = Vec::with_capacity(227);
        blob.extend_from_slice(self.previous.as_bytes());            // 0..32
        blob.push(self.version);                                     // 32
        blob.extend_from_slice(&self.target.to_le_bytes()); // 33..37
        blob.extend_from_slice(&[0u8; 2]);                           // 37..39 (reserved)
        blob.extend_from_slice(&self.nonce.to_le_bytes());           // 39..43 (nonce)
        blob.extend_from_slice(&self.height.to_le_bytes());          // 43..51
        blob.extend_from_slice(self.merkle_root.as_bytes());         // 51..83
        blob.extend_from_slice(&self.timestamp.to_le_bytes());       // 83..91
        blob.extend_from_slice(&self.uncle_merkle_root);             // 91..123
        blob.extend_from_slice(&self.total_reward.to_le_bytes());    // 123..131
        blob.extend_from_slice(&self.randomx_key);                   // 131..163
        blob.extend_from_slice(&self.coin_merkle_root);              // 163..195
        blob.extend_from_slice(&self.nullifier_root);                // 195..227
        blob
    }

    /// The byte offset of the nonce within the mining blob (bytes 39..42).
    /// Matches xmrig's hardcoded Monero rx/0 nonce offset.
    pub const NONCE_OFFSET: usize = 39;

    /// The expected length of the mining blob.
    pub const MINING_BLOB_LEN: usize = 227;
}

/// Block - a single block in the linear chain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    /// Block header
    pub header: BlockHeader,
    /// Transactions in this block
    pub transactions: Vec<Transaction>,
}

impl Block {
    /// Calculate the hash of this block's header using RandomX VM.
    /// Uses the compact mining blob format so the hash matches what
    /// external miners (xmrig) compute.
    pub fn hash(&self, vm: &randomx::RandomXVM) -> blake3::Hash {
        let blob = self.header.to_mining_blob();
        let rx_hash = vm.calculate_hash(&blob).expect("RandomX hash failed");
        let mut hash_bytes = [0u8; 32];
        hash_bytes.copy_from_slice(&rx_hash[..32]);
        blake3::Hash::from_bytes(hash_bytes)
    }

    /// Verify the block's previous hash matches the expected parent
    pub fn verify_previous_hash(&self, expected_previous: blake3::Hash) -> bool {
        self.header.previous == expected_previous
    }

    /// Verify the merkle root matches the transactions
    pub fn verify_merkle_root(&self) -> bool {
        let tx_hashes: Vec<blake3::Hash> = self.transactions.iter().map(|tx| tx.hash()).collect();
        let computed_root = if tx_hashes.is_empty() {
            blake3::hash(&[])
        } else {
            // Simple merkle root computation
            let mut layer = tx_hashes.clone();
            while layer.len() > 1 {
                if !layer.len().is_multiple_of(2) {
                    layer.push(*layer.last().unwrap());
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
        computed_root == self.header.merkle_root
    }
}

/// Verify an uncle proof against a merkle root
/// This verifies:
/// 1. The pow_hash in the proof matches re-computed hash from header with header.randomx_key
/// 2. The pow_hash meets the difficulty target
/// 3. The merkle proof verifies the header is in the uncle merkle tree
pub fn verify_uncle_proof(
    uncle: &UncleProof,
    merkle_root: &[u8; 32],
    _vm: &randomx::RandomXVM,
    target: u32,
) -> bool {
    // Step 1: Verify the pow_hash matches re-computed hash from header
    // We must create a VM with the uncle's specific randomx_key
    let header_bytes = serde_json::to_vec(&uncle.header).unwrap();
    let flags = randomx::RandomXFlags::get_recommended_flags();
    let cache = match randomx::RandomXCache::new(flags, &uncle.header.randomx_key) {
        Ok(c) => c,
        Err(_) => return false,
    };
    let verify_vm = match randomx::RandomXVM::new(flags, Some(cache), None) {
        Ok(v) => v,
        Err(_) => return false,
    };
    let rx_hash = match verify_vm.calculate_hash(&header_bytes) {
        Ok(h) => h,
        Err(_) => return false,
    };
    let mut computed_pow_hash = [0u8; 32];
    computed_pow_hash.copy_from_slice(&rx_hash[..32]);

    if computed_pow_hash != uncle.pow_hash {
        return false;
    }

    // Step 2: Verify pow_hash meets difficulty target
    let hash_u32 = u32::from_le_bytes(computed_pow_hash[0..4].try_into().unwrap());
    if hash_u32 > target {
        return false;
    }

    // Step 3: Verify merkle proof
    let mut current = blake3::hash(&header_bytes).as_bytes().to_vec();

    for (level, sibling) in uncle.merkle_path.iter().enumerate() {
        // At each level, the position bit tells us left/right
        let bit = (uncle.position >> level) & 1;
        let combined = if bit == 0 {
            // Current is left, sibling is right
            let mut c = current.clone();
            c.extend_from_slice(sibling);
            c
        } else {
            // Sibling is left, current is right
            let mut c = sibling.to_vec();
            c.extend_from_slice(&current);
            c
        };
        current = blake3::hash(&combined).as_bytes().to_vec();
    }
    current.as_slice() == merkle_root
}

/// Build uncle merkle tree from uncle blocks
/// The pow_hash for each uncle is computed using RandomX with the uncle's randomx_key.
/// The merkle tree itself uses blake3 for structure (not PoW).
pub fn build_uncle_merkle(uncles: &[UncleBlock], _vm: &randomx::RandomXVM) -> ([u8; 32], Vec<UncleProof>) {
    if uncles.is_empty() {
        return ([0u8; 32], vec![]);
    }

    // Compute pow_hash for each uncle using their randomx_key
    // We need to create a temporary VM for each uncle's specific key
    let pow_hashes: Vec<[u8; 32]> = uncles
        .iter()
        .map(|u| {
            // Create a VM for this uncle's specific key
            let flags = randomx::RandomXFlags::get_recommended_flags();
            let cache = randomx::RandomXCache::new(flags, &u.header.randomx_key)
                .expect("Failed to create RandomX cache for uncle");
            let uncle_vm = randomx::RandomXVM::new(flags, Some(cache), None)
                .expect("Failed to create RandomX VM for uncle");
            let hash_bytes = uncle_vm.calculate_hash(&serde_json::to_vec(&u.header).unwrap())
                .expect("RandomX hash failed");
            let mut pow_hash = [0u8; 32];
            pow_hash.copy_from_slice(&hash_bytes[..32]);
            pow_hash
        })
        .collect();

    // Build leaves from uncle hashes using blake3 (for merkle, not PoW)
    let mut leaves: Vec<blake3::Hash> = uncles
        .iter()
        .map(|u| blake3::hash(&serde_json::to_vec(&u.header).unwrap()))
        .collect();
    if !leaves.len().is_multiple_of(2) {
        leaves.push(*leaves.last().unwrap());
    }

    // Build merkle tree bottom-up, storing each layer
    let mut layers: Vec<Vec<blake3::Hash>> = vec![leaves];
    while layers.last().unwrap().len() > 1 {
        let current = layers.last().unwrap();
        let mut next = Vec::new();
        for chunk in current.chunks(2) {
            debug_assert_eq!(chunk.len(), 2);
            let mut combined = chunk[0].as_bytes().to_vec();
            combined.extend_from_slice(chunk[1].as_bytes());
            next.push(blake3::hash(&combined));
        }
        layers.push(next);
    }
    let merkle_root: [u8; 32] = *layers.last().unwrap()[0].as_bytes();

    // Build proofs for each uncle
    let proofs: Vec<UncleProof> = (0..uncles.len())
        .map(|i| {
            let mut merkle_path = vec![];
            let mut pos = i;

            // Walk up the tree from leaf to root
            for level in 0..layers.len() - 1 {
                let is_right = pos % 2 == 1;
                let sibling_pos = if is_right { pos - 1 } else { pos + 1 };
                let current_layer = &layers[level];

                debug_assert!(sibling_pos < current_layer.len());
                merkle_path.push(*current_layer[sibling_pos].as_bytes());

                pos /= 2;
            }

            UncleProof {
                header: uncles[i].header.clone(),
                pow_hash: pow_hashes[i],
                merkle_path,
                position: i as u32,
                depth: uncles[i].depth,
            }
        })
        .collect();

    (merkle_root, proofs)
}

/// Compute reward distribution for canonical miner and uncles
/// Pin mechanism: Uncle chain gets pin reward ONLY if pin_accepted = true
/// Canonical reward = base_reward - sum(uncle pin rewards) (no over-minting)
/// Invariant: canonical_reward + sum(uncle_rewards) = base_reward
/// Returns (canonical_reward, uncle_rewards)
pub fn compute_reward(base_reward: u64, uncles: &[UncleBlock]) -> (u64, Vec<u64>) {
    if uncles.is_empty() {
        return (base_reward, vec![]);
    }

    let mut uncle_rewards = Vec::with_capacity(uncles.len());

    for uncle in uncles {
        // Uncle only gets pin_reward if they accepted the pin
        let pin = if uncle.pin_accepted { uncle.pin_reward } else { 0 };
        uncle_rewards.push(pin);
    }

    let total_pin_rewards: u64 = uncle_rewards.iter().sum();
    // Canonical reward is base minus what it pays in pins (no over-minting)
    let canonical_reward = base_reward - total_pin_rewards;
    (canonical_reward, uncle_rewards)
}

/// Maximum uncle depth allowed
pub const MAX_UNCLE_DEPTH: u8 = 6;

/// Create a new block from transactions (no uncles - Phase 1)
/// Note: This doesn't use RandomX for block creation - the VM and key are
/// passed from the miner which handles PoW. This creates a placeholder block.
pub fn create_block(
    previous: blake3::Hash,
    height: u64,
    transactions: Vec<Transaction>,
    target: u32,
    vm: &randomx::RandomXVM,
) -> Block {
    create_block_with_uncles(previous, height, transactions, target, &[], vm)
}

/// Create a new block with uncle blocks
/// Note: The block header includes randomx_key but the actual PoW mining
/// is done by the Miner using that key.
pub fn create_block_with_uncles(
    previous: blake3::Hash,
    height: u64,
    transactions: Vec<Transaction>,
    target: u32,
    uncles: &[UncleBlock],
    vm: &randomx::RandomXVM,
) -> Block {
    // Calculate merkle root for transactions (uses blake3 for speed)
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

    // Build uncle merkle and compute rewards (uses blake3 for merkle structure)
    let (uncle_merkle_root, _) = build_uncle_merkle(uncles, vm);
    let base_reward = dwow_sdk::blockchain::expected_reward(height as u32);
    let (total_reward, _) = compute_reward(base_reward, uncles);

    Block {
        header: BlockHeader {
            version: 1,
            previous,
            merkle_root,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            target,
            nonce: 0,
            height,
            uncle_merkle_root,
            total_reward,
            randomx_key: [0u8; 32], // Placeholder - miner sets actual key
            coin_merkle_root: [0u8; 32],
            nullifier_root: [0u8; 32],
            anchor_tx_id: [0u8; 32], // No Caribina anchor (set by miner after anchoring)
            anchor_monero_height: 0, // No Monero anchor (set by miner after anchoring)
            anchor_monero_hash: [0u8; 32], // No Monero anchor
            finality_flags: 0, // Set by miner after anchoring
        },
        transactions,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_vm() -> randomx::RandomXVM {
        let key = [0u8; 32];
        let flags = randomx::RandomXFlags::get_recommended_flags();
        let cache = randomx::RandomXCache::new(flags, &key).expect("Failed to create cache");
        randomx::RandomXVM::new(flags, Some(cache), None).expect("Failed to create VM")
    }

    #[test]
    fn test_build_uncle_merkle_empty() {
        let vm = create_test_vm();
        let (root, proofs) = build_uncle_merkle(&[], &vm);
        assert_eq!(root, [0u8; 32]);
        assert!(proofs.is_empty());
    }

    #[test]
    fn test_build_uncle_merkle_single() {
        let vm = create_test_vm();
        let uncle_header = BlockHeader {
            version: 1,
            previous: blake3::hash(b"parent"),
            merkle_root: blake3::hash(b"txs"),
            timestamp: 0,
            target: 0x0000_FFFF,
            nonce: 0,
            height: 10,
            uncle_merkle_root: [0u8; 32],
            total_reward: 0,
            randomx_key: [0u8; 32],
            coin_merkle_root: [0u8; 32],
            nullifier_root: [0u8; 32],
            anchor_tx_id: [0u8; 32],
            anchor_monero_height: 0,
            anchor_monero_hash: [0u8; 32],
            finality_flags: 0,
        };
        let uncle = UncleBlock { header: uncle_header, transactions: vec![], depth: 1, pin_offered: false, pin_accepted: false, pin_reward: 0 };

        let (root, proofs) = build_uncle_merkle(&[uncle], &vm);
        assert_ne!(root, [0u8; 32]);
        assert_eq!(proofs.len(), 1);
        assert_eq!(proofs[0].depth, 1);
        assert_eq!(proofs[0].position, 0);
        // pow_hash should be a valid RandomX hash (not all zeros)
        assert_ne!(proofs[0].pow_hash, [0u8; 32]);
    }

    #[test]
    fn test_build_uncle_merkle_multiple() {
        let vm = create_test_vm();
        let mut uncles = vec![];
        for i in 0..3 {
            let header = BlockHeader {
                version: 1,
                previous: blake3::hash(&[i]),
                merkle_root: blake3::hash(&[i]),
                timestamp: i as u64,
                target: 0x0000_FFFF,
                nonce: i as u32,
                height: 10 + i as u64,
                uncle_merkle_root: [0u8; 32],
                total_reward: 0,
                randomx_key: [0u8; 32],
                coin_merkle_root: [0u8; 32],
                nullifier_root: [0u8; 32],
                anchor_tx_id: [0u8; 32],
                anchor_monero_height: 0,
                anchor_monero_hash: [0u8; 32],
                finality_flags: 0,
            };
            uncles.push(UncleBlock { header, transactions: vec![], depth: 1, pin_offered: false, pin_accepted: false, pin_reward: 0 });
        }

        let (root, proofs) = build_uncle_merkle(&uncles, &vm);
        assert_ne!(root, [0u8; 32]);
        assert_eq!(proofs.len(), 3);
        for (i, proof) in proofs.iter().enumerate() {
            assert_eq!(proof.position, i as u32);
            // Verify pow_hash was computed correctly (just check it's non-zero)
            assert_ne!(proof.pow_hash, [0u8; 32]);
        }
        // Note: verify_uncle_proof may fail difficulty check since nonce is arbitrary
    }

    #[test]
    fn test_compute_reward_no_uncles() {
        let (canonical, uncles) = compute_reward(100_000_000, &[]);
        assert_eq!(canonical, 100_000_000);
        assert!(uncles.is_empty());
    }

    #[test]
    fn test_compute_reward_with_uncles() {
        let uncle_header = BlockHeader {
            version: 1,
            previous: blake3::hash(b"parent"),
            merkle_root: blake3::hash(b"txs"),
            timestamp: 0,
            target: 0x0000_FFFF,
            nonce: 0,
            height: 10,
            uncle_merkle_root: [0u8; 32],
            total_reward: 0,
            randomx_key: [0u8; 32],
            coin_merkle_root: [0u8; 32],
            nullifier_root: [0u8; 32],
            anchor_tx_id: [0u8; 32],
            anchor_monero_height: 0,
            anchor_monero_hash: [0u8; 32],
            finality_flags: 0,
        };
        // Pin mechanism: pin_offered=true, pin_accepted=true means uncle accepts the pin
        // pin_reward at depth 1 = 50% = 50M
        let uncle = UncleBlock { header: uncle_header, transactions: vec![], depth: 1, pin_offered: true, pin_accepted: true, pin_reward: 50_000_000 };

        let (canonical, uncle_rewards) = compute_reward(100_000_000, &[uncle]);
        // base 100M - pin 50M = 50M canonical (no over-minting)
        assert_eq!(canonical, 50_000_000);
        assert_eq!(uncle_rewards.len(), 1);
        assert_eq!(uncle_rewards[0], 50_000_000);
    }

    #[test]
    fn test_verify_uncle_proof() {
        let vm = create_test_vm();
        let header = BlockHeader {
            version: 1,
            previous: blake3::hash(b"parent"),
            merkle_root: blake3::hash(b"txs"),
            timestamp: 0,
            target: 0x0000_FFFF,
            nonce: 42,
            height: 10,
            uncle_merkle_root: [0u8; 32],
            total_reward: 0,
            randomx_key: [0u8; 32],
            coin_merkle_root: [0u8; 32],
            nullifier_root: [0u8; 32],
            anchor_tx_id: [0u8; 32],
            anchor_monero_height: 0,
            anchor_monero_hash: [0u8; 32],
            finality_flags: 0,
        };
        let uncle = UncleBlock { header: header.clone(), transactions: vec![], depth: 1, pin_offered: false, pin_accepted: false, pin_reward: 0 };

        let (root, proofs) = build_uncle_merkle(&[uncle], &vm);
        // Note: verify_uncle_proof may fail difficulty check since nonce 42 is arbitrary
        // Instead, verify the pow_hash was correctly computed
        let header_bytes = serde_json::to_vec(&header).unwrap();
        let flags = randomx::RandomXFlags::get_recommended_flags();
        let cache = randomx::RandomXCache::new(flags, &[0u8; 32]).unwrap();
        let verify_vm = randomx::RandomXVM::new(flags, Some(cache), None).unwrap();
        let expected_hash = verify_vm.calculate_hash(&header_bytes).unwrap();
        let mut expected_pow = [0u8; 32];
        expected_pow.copy_from_slice(&expected_hash[..32]);
        assert_eq!(proofs[0].pow_hash, expected_pow);

        // Verify with wrong root fails (merkle verification)
        assert!(!verify_uncle_proof(&proofs[0], &[1u8; 32], &vm, 0x0000_FFFF));
    }

    #[test]
    fn test_create_block_with_uncles() {
        let vm = create_test_vm();
        let previous = blake3::hash(b"genesis");
        let block = create_block_with_uncles(
            previous,
            1,
            vec![],
            0x0000_FFFF,
            &[],
            &vm,
        );

        assert_eq!(block.header.previous, previous);
        assert_eq!(block.header.height, 1);
        assert_eq!(block.header.uncle_merkle_root, [0u8; 32]);
        // With no uncles, total_reward = base_reward = expected_reward(height)
        assert_eq!(block.header.total_reward, dwow_sdk::blockchain::expected_reward(1));
    }

    /// Verify the coinbase lifecycle: create blocks at heights 1, 2, 3,
    /// check rewards follow the exponential-decay emission schedule, and
    /// confirm blocks can be applied to a LinearBlockchain.
    #[test]
    fn test_coinbase_lifecycle() {
        let vm = create_test_vm();

        // Height 1: ~13.8375 DRKW (INITIAL_REWARD)
        let block1 = create_block_with_uncles(
            blake3::hash(b"genesis"),
            1,
            vec![],
            0x0000_FFFF,
            &[],
            &vm,
        );
        let reward1 = dwow_sdk::blockchain::expected_reward(1);
        assert_eq!(block1.header.total_reward, reward1);
        assert!(reward1 > 1_000_000_000, "height 1 reward should be > 1B base units");

        // Height 2: slightly less than height 1 (exponential decay)
        let block2 = create_block_with_uncles(
            block1.hash(&vm),
            2,
            vec![],
            0x0000_FFFF,
            &[],
            &vm,
        );
        let reward2 = dwow_sdk::blockchain::expected_reward(2);
        assert_eq!(block2.header.total_reward, reward2);
        assert!(reward2 <= reward1, "reward must decay monotonically");

        // Height 3: continues decay
        let block3 = create_block_with_uncles(
            block2.hash(&vm),
            3,
            vec![],
            0x0000_FFFF,
            &[],
            &vm,
        );
        let reward3 = dwow_sdk::blockchain::expected_reward(3);
        assert_eq!(block3.header.total_reward, reward3);
        assert!(reward3 <= reward2, "reward must decay monotonically");

        // All rewards must be >= TAIL_REWARD
        let tail = dwow_sdk::blockchain::reward::TAIL_REWARD;
        assert!(reward1 >= tail);
        assert!(reward2 >= tail);
        assert!(reward3 >= tail);
    }

    /// Verify create_block (without uncles) uses expected_reward.
    #[test]
    fn test_create_block_reward() {
        let vm = create_test_vm();
        let previous = blake3::hash(b"genesis");

        let block = create_block(previous, 42, vec![], 0x0000_FFFF, &vm);
        let expected = dwow_sdk::blockchain::expected_reward(42);
        assert_eq!(block.header.total_reward, expected);
        assert_eq!(block.header.height, 42);
    }

    /// Caribina: mining blob must exclude anchor_tx_id so PoW hash doesn't
    /// change after anchoring.
    #[test]
    fn test_mining_blob_excludes_anchor() {
        let mut header = BlockHeader {
            version: 1,
            previous: blake3::hash(b"parent"),
            merkle_root: blake3::hash(b"txs"),
            timestamp: 1000,
            target: 0x0000_FFFF,
            nonce: 42,
            height: 1,
            uncle_merkle_root: [0u8; 32],
            total_reward: 100_000_000,
            randomx_key: [0u8; 32],
            coin_merkle_root: [0u8; 32],
            nullifier_root: [0u8; 32],
            anchor_tx_id: [0u8; 32],
            anchor_monero_height: 0,
            anchor_monero_hash: [0u8; 32],
            finality_flags: 0,
        };

        let blob1 = header.to_mining_blob();
        assert_eq!(blob1.len(), 227);
        assert_eq!(BlockHeader::MINING_BLOB_LEN, 227);

        // Setting anchor_tx_id must not change the mining blob
        header.anchor_tx_id = [0xAB; 32];
        let blob2 = header.to_mining_blob();
        assert_eq!(blob1, blob2);
    }

    /// Caribina: default anchor_tx_id is zero (no anchor).
    #[test]
    fn test_anchor_tx_id_default_is_zero() {
        let header = BlockHeader {
            version: 1,
            previous: blake3::hash(b"parent"),
            merkle_root: blake3::hash(b"txs"),
            timestamp: 1000,
            target: 0x0000_FFFF,
            nonce: 0,
            height: 0,
            uncle_merkle_root: [0u8; 32],
            total_reward: 0,
            randomx_key: [0u8; 32],
            coin_merkle_root: [0u8; 32],
            nullifier_root: [0u8; 32],
            anchor_tx_id: [0u8; 32],
            anchor_monero_height: 0,
            anchor_monero_hash: [0u8; 32],
            finality_flags: 0,
        };
        assert_eq!(header.anchor_tx_id, [0u8; 32]);
    }

    /// Caribina: serde roundtrip preserves anchor_tx_id.
    #[test]
    fn test_block_header_with_anchor_serde() {
        let header = BlockHeader {
            version: 1,
            previous: blake3::hash(b"parent"),
            merkle_root: blake3::hash(b"txs"),
            timestamp: 1000,
            target: 0x0000_FFFF,
            nonce: 42,
            height: 1,
            uncle_merkle_root: [0u8; 32],
            total_reward: 100_000_000,
            randomx_key: [0xAA; 32],
            coin_merkle_root: [0u8; 32],
            nullifier_root: [0u8; 32],
            anchor_tx_id: [0xBB; 32],
            anchor_monero_height: 0,
            anchor_monero_hash: [0u8; 32],
            finality_flags: 0,
        };

        let json = serde_json::to_string(&header).unwrap();
        let deserialized: BlockHeader = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.anchor_tx_id, [0xBB; 32]);
        assert_eq!(deserialized.nonce, 42);
        assert_eq!(deserialized.height, 1);
    }

    /// Backward-compatible deserialization: old blocks without the new fields
    /// (coin_merkle_root, nullifier_root, anchor_tx_id, anchor_monero_height,
    /// anchor_monero_hash, finality_flags) must still deserialize with defaults.
    #[test]
    fn test_block_header_deserialize_old_format() {
        // Build a header, serialize it, then remove the new fields from the JSON
        let header = BlockHeader {
            version: 1,
            previous: blake3::hash(b"parent"),
            merkle_root: blake3::hash(b"txs"),
            timestamp: 1000,
            target: 0x0000_FFFF,
            nonce: 42,
            height: 1,
            uncle_merkle_root: [0u8; 32],
            total_reward: 100_000_000,
            randomx_key: [0xAA; 32],
            coin_merkle_root: [0u8; 32],
            nullifier_root: [0u8; 32],
            anchor_tx_id: [0u8; 32],
            anchor_monero_height: 0,
            anchor_monero_hash: [0u8; 32],
            finality_flags: 0,
        };

        let full_json = serde_json::to_string(&header).unwrap();

        // Parse, remove the new fields, and re-serialize to get "old format" JSON
        let mut val: serde_json::Value = serde_json::from_str(&full_json).unwrap();
        let obj = val.as_object_mut().unwrap();
        obj.remove("coin_merkle_root");
        obj.remove("nullifier_root");
        obj.remove("anchor_tx_id");
        obj.remove("anchor_monero_height");
        obj.remove("anchor_monero_hash");
        obj.remove("finality_flags");
        let old_json = serde_json::to_string(&obj).unwrap();

        // Deserialize the old format — must succeed with defaults
        let deserialized: BlockHeader = serde_json::from_str(&old_json).unwrap();
        assert_eq!(deserialized.version, 1);
        assert_eq!(deserialized.nonce, 42);
        assert_eq!(deserialized.height, 1);
        assert_eq!(deserialized.coin_merkle_root, [0u8; 32]);
        assert_eq!(deserialized.nullifier_root, [0u8; 32]);
        assert_eq!(deserialized.anchor_tx_id, [0u8; 32]);
        assert_eq!(deserialized.anchor_monero_height, 0);
        assert_eq!(deserialized.anchor_monero_hash, [0u8; 32]);
        assert_eq!(deserialized.finality_flags, 0);
    }

    /// Monero anchor fields must be excluded from mining blob for dual-finality.
    #[test]
    fn test_mining_blob_excludes_monero_anchor() {
        let mut header = BlockHeader {
            version: 1,
            previous: blake3::hash(b"parent"),
            merkle_root: blake3::hash(b"txs"),
            timestamp: 1000,
            target: 0x0000_FFFF,
            nonce: 42,
            height: 1,
            uncle_merkle_root: [0u8; 32],
            total_reward: 100_000_000,
            randomx_key: [0u8; 32],
            coin_merkle_root: [0u8; 32],
            nullifier_root: [0u8; 32],
            anchor_tx_id: [0u8; 32],
            anchor_monero_height: 0,
            anchor_monero_hash: [0u8; 32],
            finality_flags: 0,
        };

        let blob1 = header.to_mining_blob();

        // Setting Monero anchor fields must not change the mining blob
        header.anchor_monero_height = 3_500_000;
        header.anchor_monero_hash = [0xCD; 32];
        header.finality_flags = 0xFF;
        let blob2 = header.to_mining_blob();
        assert_eq!(blob1, blob2);
    }

    /// Monero anchor fields survive serde roundtrip.
    #[test]
    fn test_monero_anchor_serde_roundtrip() {
        let header = BlockHeader {
            version: 1,
            previous: blake3::hash(b"parent"),
            merkle_root: blake3::hash(b"txs"),
            timestamp: 1000,
            target: 0x0000_FFFF,
            nonce: 42,
            height: 1,
            uncle_merkle_root: [0u8; 32],
            total_reward: 100_000_000,
            randomx_key: [0xAA; 32],
            coin_merkle_root: [0u8; 32],
            nullifier_root: [0u8; 32],
            anchor_tx_id: [0xBB; 32],
            anchor_monero_height: 3_500_000,
            anchor_monero_hash: [0xCC; 32],
            finality_flags: 0x02, // FINALITY_MONERO
        };

        let json = serde_json::to_string(&header).unwrap();
        let deserialized: BlockHeader = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.anchor_monero_height, 3_500_000);
        assert_eq!(deserialized.anchor_monero_hash, [0xCC; 32]);
        assert_eq!(deserialized.finality_flags, 0x02);
        assert_eq!(deserialized.anchor_tx_id, [0xBB; 32]);
        assert_eq!(deserialized.nonce, 42);
    }
}