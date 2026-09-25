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

use dwow_sdk::blockchain::{BlockHeight, BlockReward, BlockTarget, BlockTimestamp, BlockVersion, MoneroBlockHeight};

use super::{Transaction, LinearError, Result};
use crate::fee_window::FeeWindowFlags;
use crate::monero::MoneroPowData;

/// Source of Proof of Work — either native RandomX or Monero merge mining.
#[derive(Debug, Clone)]
pub enum PowSource {
    /// Native RandomX PoW (not merge-mined)
    Native,
    /// Merge-mined through Monero p2pool — carries verifiable proof data
    Monero(MoneroPowData),
}

impl PowSource {
    /// Explicit default — per type-system.md §5.1, consensus authority SHALL NOT
    /// be gated by a bare `Default` impl. Callers must explicitly select the PoW
    /// source; this function exists only for serde backward-compatibility.
    pub const fn native() -> Self {
        PowSource::Native
    }
}

/// Maximum gas a single block can consume across all contract calls.
/// Formerly in the deleted `blockchain.rs` god object. Lives here (not in
/// `execution`) so the non-`pow` wallet build can read it without compiling
/// the contract-execution stack.
pub const BLOCK_GAS_LIMIT: u64 = 100_000_000_000;

// There is deliberately no block-size limit here, and none anywhere in the
// node. A `MAX_BLOCK_SIZE = 4 * 1024 * 1024` stood here until 2026-09-25; it
// cited "L1 barrier #7", which exists in no document in this repository (the
// barrier list was deleted 2026-09-22), and the value traces to a bulk commit
// whose message says only "Add 4 MB size cap on block decode". It was then
// enforced as *block validity*, and it rejected a legitimate contract-deployment
// block at height 2. Byte size is bounded by `BLOCK_GAS_LIMIT` through gas
// accounting; a node-local resource policy is not a validity rule and must not
// reject data. If a cap is ever wanted it is decided from testing — a measured
// payload distribution against a measured node capacity — and it goes in the
// consensus specification before it goes in code. See
// `doc/src/arch/consensus/consensus.md`, "Block and Payload Size".

/// Block header - contains metadata about a block
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockHeader {
    /// Block version
    pub version: BlockVersion,
    /// Hash of the previous block (only one parent - linear chain)
    pub previous: blake3::Hash,
    /// Merkle root of transactions
    pub merkle_root: blake3::Hash,
    /// Block timestamp
    pub timestamp: BlockTimestamp,
    /// PoW target — `hash_u32 <= target` is valid. Higher = easier.
    pub target: BlockTarget,
    /// Nonce for PoW mining
    pub nonce: u32,
    /// Block height in chain
    pub height: BlockHeight,
    /// Merkle root of uncle blocks referenced by this canonical block
    pub uncle_merkle_root: [u8; 32],
    /// Total reward being distributed (canonical + uncle shares)
    pub total_reward: BlockReward,
    /// RandomX key for PoW mining (key used to create VM for this block)
    pub randomx_key: [u8; 32],
    /// Miner's reward public key (32-byte compressed pallas point, `pk_H`).
    /// Spec: uncle_merkle.md §Uncle Minting & Maturity — "Miner identity in the
    /// header". Covered by PoW; used to AEAD-encrypt uncle notes to the uncle miner.
    #[serde(default)]
    pub miner: [u8; 32],
    /// The key that authored this block's Caribina anchor — a **fresh per-block Ed25519
    /// public key**, inside `to_mining_blob()` and therefore COVERED BY POW.
    ///
    /// This is what makes an anchor the *miner's* claim rather than anyone's. Without a
    /// PoW-committed owner, any peer could publish a valid DataItem binding any block and
    /// finality would become unconditional — every block final one block later, no reorgs
    /// ever. With it, forging an anchor for a block requires the secret key that block
    /// commits, and swapping the owner changes the mining preimage.
    ///
    /// Deliberately **not** `header.miner`: that is the reward recipient `pk_H`, a
    /// long-lived consensus key, and signing an Arweave DataItem with it would publish
    /// signatures under a key the chain depends on to a third party. Per-block generation
    /// also preserves the address-cycling property the design calls for.
    #[serde(default)]
    pub anchor_owner: [u8; 32],
    /// Root of the commitment Merkle tree after this block.
    ///
    /// RESERVED — always `[0u8; 32]` in every production block-construction path;
    /// the live commitment/nullifier state is held in the sled `commitment_set` and
    /// `nullifiers` trees and in `CChainState`. Only test fixtures set it non-zero.
    /// It is nevertheless inside `to_mining_blob()`, i.e. COVERED BY PoW, so
    /// populating it changes the mining preimage and therefore the block hash —
    /// that is a deliberate consensus change (and the hook `scaling.md` describes),
    /// never a silent one.
    #[serde(default)]
    pub commitment_merkle_root: [u8; 32],
    /// blake3 root over the block's nullifier set (not an SMT — §9.2).
    /// RESERVED, and PoW-covered — see `commitment_merkle_root` above.
    #[serde(default)]
    pub nullifier_root: [u8; 32],
    /// Caribina Arweave anchor TX ID (SHA-256 of ANS-104 DataItem signature).
    /// [0u8; 32] means no anchor (genesis blocks, bootstrapping, or anchor failure).
    ///
    /// **No longer consulted for finality** (2026-09-22). Enforcement now requires the
    /// verified anchor *proof* in `caribina_anchor` below. The field is kept because it
    /// carries a second, unrelated thing: the genesis block's **network magic** lives in
    /// its first four bytes (`bin/dwowd/src/lib.rs:566`) and
    /// `bin/dwowd/src/task/consensus_linear.rs:383` validates it to reject a wrong-network
    /// genesis during sync. Deriving it from the DataItem would break that; leaving it as
    /// a free field is safe precisely because nothing decides finality by reading it now.
    #[serde(default)]
    pub anchor_tx_id: [u8; 32],
    /// The signed ANS-104 DataItem that is this block's Caribina anchor *proof*, carried
    /// whole so verification is a pure local function.
    ///
    /// Outside the mining blob, and it does not need to be inside it: its authenticity
    /// comes from `anchor_owner` (PoW-committed) and from the payload binding
    /// `caribina::anchor_commitment(self)`, not from PoW coverage of the proof bytes.
    /// `None` means no anchor, which is valid and simply confers no finality.
    #[serde(default)]
    pub caribina_anchor: Option<Vec<u8>>,
    /// Monero p2pool anchor block height (0 = no anchor)
    #[serde(default = "MoneroBlockHeight::serde_default")]
    pub anchor_monero_height: MoneroBlockHeight,
    /// Monero p2pool anchor block hash ([0u8; 32] = no anchor)
    #[serde(default)]
    pub anchor_monero_hash: [u8; 32],
    /// Finality signaling flags bitfield:
    ///   0x01 = FINALITY_CARIBNIA, 0x02 = FINALITY_MONERO, 0x04 = FINALITY_SIGNALED
    #[serde(default)]
    pub finality_flags: u8,
    /// Fee window signalling flags (fee-spec.md §12.6).
    /// Byte 0 = CIRCUIT_CF direction, Byte 1 = WASM_CF direction.
    /// Excluded from mining blob.
    #[serde(default)]
    pub fee_window_flags: FeeWindowFlags,
    /// Proof of Work source — native RandomX or Monero merge-mined.
    /// `MoneroPowData` contains cryptographic proof that the Monero
    /// block was mined with our merge mining tag embedded.
    ///
    /// `skip` was removed on 2026-09-22 (`OBL-C69`). It made this field absent from every serde
    /// encoding, and the P2P block wire *is* serde — so a relayed merge-mined block arrived
    /// reclassified as native and was checked with RandomX over a header xmrig never hashed.
    /// `PowSource` now has `Serialize`/`Deserialize` impls (`src/linear/src/serial_sync.rs`) that
    /// carry the canonical codec's bytes, so the two serializations cannot disagree again.
    #[serde(default = "PowSource::native")]
    pub pow_source: PowSource,
}

/// Uncle block - a block that was mined but not canonical
///
/// P2-9-3: `depth` and `pin_offered` were removed from this struct — both were
/// write-only on the receive path (depth was only copied into UncleProof,
/// pin_offered only gated accept_pin) and neither is verifiable or derivable
/// by a receiver, so they carried no consensus signal on the wire. Depth is
/// now derived on demand via `UncleBlock::depth_for` at the creation sites.
/// This changes the sled `uncles`-tree binary format and the JSON wire format:
/// old sled DBs / old peers are incompatible (devnet wipe required).
/// UNVERIFIED(P2-9-3): needs cargo test -p dwow_chain && cargo test -p dwowd --lib -- wire_format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UncleBlock {
    /// Header of the uncle block
    pub header: BlockHeader,
    /// Transactions in the uncle block
    pub transactions: Vec<Transaction>,
    /// Uncle chain accepted the pin (use it or lose it - one time decision)
    pub pin_accepted: bool,
    /// Pin confirmed — reward amount computed from depth (50% at d1, 25% at d2...).
    /// Actual reward payment is computed downstream by `compute_reward()` and
    /// `verify_uncle_split()`.
    ///
    /// NOTE: `pin_accepted` starts `false` at uncle creation. The uncle miner
    /// may later accept the pin via `accept_pin()` (use-it-or-lose-it).
    /// `compute_reward()` only pays uncles with `pin_accepted == true`.
    pub pin_confirmed: BlockReward,
}

impl UncleBlock {
    /// Calculate the hash of this uncle block's header using the given VM.
    ///
    /// Uses the compact mining blob format (same as `Block::hash_with_vm()`)
    /// so that uncle PoW verification is consistent with how the block was
    /// originally mined. The uncle was mined as a `Block` — its PoW was
    /// computed over `to_mining_blob()`, not JSON serialization.
    ///
    /// **Warning:** The caller must ensure the VM is keyed with this block's
    /// own `header.randomx_key`. Passing a VM with a different key produces
    /// a garbage hash.
    #[cfg(feature = "pow")]
    pub fn hash_with_vm(&self, vm: &randomx::RandomXVM) -> Result<blake3::Hash> {
        let header_bytes = self.header.to_mining_blob();
        // Use first 32 bytes of RandomX output as the hash
        let rx_hash = vm.calculate_hash(&header_bytes)
            .map_err(|e| LinearError::RandomXError(format!("RandomX hash failed: {e}")))?;
        // `rx_hash[..32]` panicked on the indexing and `copy_from_slice` panicked on a length
        // mismatch; `get` + `try_into` makes a short hash an error instead of either.
        let hash_bytes: [u8; 32] = rx_hash
            .get(..32)
            .and_then(|s| s.try_into().ok())
            .ok_or_else(|| LinearError::RandomXError(format!(
                "RandomX returned {} bytes, need 32", rx_hash.len(),
            )))?;
        Ok(blake3::Hash::from_bytes(hash_bytes))
    }

    /// Accept the pin offer from canonical chain (use it or lose it)
    /// This is a one-time decision - once accepted, cannot be undone.
    ///
    /// P2-9-3: the `pin_offered` gate was removed with the field — the pin
    /// offer is obligated for qualifying uncles (create_uncle always offered
    /// it) and rejection is strictly dominated for the uncle miner, so the
    /// guard was write-only bookkeeping.
    pub fn accept_pin(&mut self) {
        self.pin_accepted = true;
    }

    /// Depth of an uncle whose header is at `uncle_height` when referenced by
    /// a canonical block at `current_height` — clamped to MAX_UNCLE_DEPTH.
    ///
    /// P2-9-3: `depth` is no longer a stored/wire field; it is derived here at
    /// the creation sites (prepare_block, mm_rpc, stratum) where the canonical
    /// height is known.
    pub fn depth_for(current_height: BlockHeight, uncle_height: BlockHeight) -> u8 {
        current_height.get().saturating_sub(uncle_height.get())
            .min(MAX_UNCLE_DEPTH as u64) as u8
    }
}

/// Convert a rejected block into an uncle block
///
/// P2-9-3: `depth` feeds only the `pin_confirmed` split here — it is no
/// longer stored on the struct (callers derive it via `UncleBlock::depth_for`).
pub fn create_uncle(block: Block, depth: u8, base_reward: BlockReward) -> UncleBlock {
    let depth = depth.min(MAX_UNCLE_DEPTH);
    let pin_confirmed = base_reward.split_for_uncle(depth);
    UncleBlock {
        header: block.header,
        transactions: block.transactions,
        pin_accepted: false,
        pin_confirmed,
    }
}

/// Proof of an uncle for stateless verification.
///
/// Holds only what cannot be derived from the [`UncleBlock`] it accompanies: the
/// uncle's header is the caller's `uncles[i].header`, and its PoW hash is
/// recomputed by [`verify_uncle_proof`] (never trusted from the proof). Both used
/// to be carried here as well, which made the builder run a full RandomX
/// cache+VM initialisation per uncle purely to fill a field the verifier then
/// recomputed — 2N initialisations of the deliberately expensive step per block,
/// on the accept path.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UncleProof {
    /// Merkle proof path from uncle to root
    pub merkle_path: Vec<[u8; 32]>,
    /// Uncle's position in merkle tree (leaf index)
    pub position: u32,
}

impl BlockHeader {
    /// Serialize the header to a compact binary blob for mining and hashing.
    /// Format (292 bytes total; 260 until `anchor_owner` was appended, `OBL-C64`):
    ///   [previous(32)][version(1)][target(4)][reserved(2)][nonce(4)]
    ///   [height(8)][merkle_root(32)][timestamp(8)][uncle_merkle_root(32)]
    ///   [total_reward(8)][randomx_key(32)][commitment_merkle_root(32)][nullifier_root(32)]
    ///   [pow_source_disc(1)][miner(32)][anchor_owner(32)]
    ///   — 0 = Native, 1 = Monero (MoneroPowData NOT included)
    /// Nonce is at byte offset 39 (matches xmrig's hardcoded Monero rx/0 offset).
    /// anchor_tx_id, caribina_anchor, anchor_monero_height, anchor_monero_hash and
    /// finality_flags are excluded — they are set after PoW is found and are not covered
    /// by the mining hash. `anchor_owner` is the exception among the anchor fields and is
    /// inside deliberately: it is generated *before* mining (it must be in the preimage
    /// xmrig hashes) and is what authenticates the anchor. See its field doc.
    pub fn to_mining_blob(&self) -> Vec<u8> {
        let mut blob = Vec::with_capacity(Self::MINING_BLOB_LEN);
        blob.extend_from_slice(self.previous.as_bytes());            // 0..32
        blob.push(self.version.get());                                // 32
        blob.extend_from_slice(&self.target.to_le_bytes()); // 33..37
        blob.extend_from_slice(&[0u8; 2]);                           // 37..39 (reserved)
        blob.extend_from_slice(&self.nonce.to_le_bytes());           // 39..43 (nonce)
        blob.extend_from_slice(&self.height.to_le_bytes());          // 43..51
        blob.extend_from_slice(self.merkle_root.as_bytes());         // 51..83
        blob.extend_from_slice(&self.timestamp.to_le_bytes());       // 83..91
        blob.extend_from_slice(&self.uncle_merkle_root);             // 91..123
        blob.extend_from_slice(&self.total_reward.to_le_bytes());    // 123..131
        blob.extend_from_slice(&self.randomx_key);                   // 131..163
        blob.extend_from_slice(&self.commitment_merkle_root);              // 163..195
        blob.extend_from_slice(&self.nullifier_root);                // 195..227
        let disc: u8 = match self.pow_source {
            PowSource::Native => 0,
            PowSource::Monero(_) => 1,
        };
        blob.push(disc);                                              // 227
        blob.extend_from_slice(&self.miner);                          // 228..260
        blob.extend_from_slice(&self.anchor_owner);                   // 260..292
        blob
    }

    /// The byte offset of the nonce within the mining blob (bytes 39..42).
    /// Matches xmrig's hardcoded Monero rx/0 nonce offset.
    pub const NONCE_OFFSET: usize = 39;

    /// The byte offset of the `pow_source` discriminator within the mining
    /// blob (byte 227): 0 for native PoW, 1 for merge-mined Monero.
    ///
    /// Read by offset from outside this crate — the stratum/merge-mining FFI
    /// rewrites the whole blob and the miner rewrites the nonce — so the
    /// position is consensus-relevant and belongs here, not as a literal at
    /// each call site.
    pub const POW_SOURCE_OFFSET: usize = 227;

    /// The byte offset of `anchor_owner` within the mining blob (bytes 260..292),
    /// immediately after `miner`.
    pub const ANCHOR_OWNER_OFFSET: usize = 260;

    /// The expected length of the mining blob.
    ///
    /// 260 until 2026-09-22, when `anchor_owner` was appended (bytes 260..292). This is a
    /// **consensus-format change**: the preimage RandomX hashes grew, so every block mined
    /// before it fails PoW against the new layout and the chain resets. `NONCE_OFFSET` (39)
    /// and `POW_SOURCE_OFFSET` (227) are unchanged, which matters because
    /// `bin/dwowd/src/rpc/stratum.rs` sends `"reserved_offset": 39` to xmrig.
    pub const MINING_BLOB_LEN: usize = 292;
}

/// Block - a single block in the linear chain
// ANCHOR: block-struct
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    /// Block header
    pub header: BlockHeader,
    /// Transactions in this block
    pub transactions: Vec<Transaction>,
}
// ANCHOR_END: block-struct

impl Block {
    /// Calculate the hash of this block's header using the given VM.
    /// Uses the compact mining blob format so the hash matches what
    /// external miners (xmrig) compute.
    ///
    /// **Warning:** The caller must ensure the VM is keyed with this block's
    /// own `header.randomx_key`. Passing a VM with a different key produces
    /// a garbage hash that will fail validation. Use
    /// [`get_vm(block.header.randomx_key)`] to create the correct VM.
    #[cfg(feature = "pow")]
    pub fn hash_with_vm(&self, vm: &randomx::RandomXVM) -> Result<blake3::Hash> {
        let blob = self.header.to_mining_blob();
        let rx_hash = vm.calculate_hash(&blob)
            .map_err(|e| LinearError::RandomXError(format!("RandomX hash failed: {e}")))?;
        // Total: `get(..32)` + `try_into` rather than `&rx_hash[..32]`, which panics on a short
        // slice. RandomX does return 32 bytes, but that is a fact about the library, and a panic
        // here aborts a node in the middle of hashing a block.
        let hash_bytes: [u8; 32] = rx_hash
            .get(..32)
            .and_then(|s| s.try_into().ok())
            .ok_or_else(|| {
                LinearError::RandomXError(format!(
                    "RandomX returned {} bytes, expected at least 32",
                    rx_hash.len()
                ))
            })?;
        Ok(blake3::Hash::from_bytes(hash_bytes))
    }

    /// Verify the merkle root matches the transactions
    pub fn verify_merkle_root(&self) -> bool {
        compute_merkle_root(&self.transactions) == self.header.merkle_root
    }
}

/// Build a binary merkle tree bottom-up from `leaves`, returning every layer with
/// the leaf layer first. Each ODD layer is padded by duplicating its last hash.
///
/// This is the single merkle construction for the whole chain — the transaction
/// tree ([`compute_merkle_root`]) and the uncle tree ([`build_uncle_merkle`]) both
/// use it. The uncle tree previously had its own copy that padded only the leaf
/// layer, leaving 3-element intermediate layers (and an out-of-bounds index) for
/// 5 or 6 uncles.
///
/// `leaves` MUST be non-empty; callers handle the empty case themselves, since the
/// transaction tree and the uncle tree give it different sentinels.
fn merkle_layers(leaves: Vec<blake3::Hash>) -> Vec<Vec<blake3::Hash>> {
    // Seeded non-empty and only grows, so `layers` is never empty.
    let mut layers: Vec<Vec<blake3::Hash>> = vec![leaves];
    loop {
        // `last()` rather than `[len() - 1]`: the length is never zero, but the compiler cannot
        // know that, so an index is a panic path with a location compiled in. The `None` arm is
        // unreachable and yields an empty slice, which fails the length test below and exits.
        let current: &[blake3::Hash] = match layers.last() {
            Some(layer) => layer.as_slice(),
            None => &[],
        };
        if current.len() <= 1 {
            break;
        }
        // Pair adjacent hashes, duplicating an odd final element by pairing it with itself — the
        // same rule as the padded-chunks form it replaces, expressed without an index and without
        // `debug_assert_eq!`. That assertion was compiled out in release, so in release nothing
        // guarded `pair[1]`; the pair-index in this family is the class that was once a remote DoS
        // here (see the uncle-proof builder below, and MAX_UNCLE_COUNT = 6).
        let mut next = Vec::with_capacity(current.len().div_ceil(2));
        let mut it = current.iter();
        while let Some(a) = it.next() {
            let b = it.next().unwrap_or(a);
            let mut combined = a.as_bytes().to_vec();
            combined.extend_from_slice(b.as_bytes());
            next.push(blake3::hash(&combined));
        }
        layers.push(next);
    }
    layers
}

/// Compute the transaction merkle root — the single canonical algorithm.
///
/// Shared by block builders (genesis ceremony, miner template) and
/// [`Block::verify_merkle_root`]. Odd layers duplicate the last hash;
/// the empty set hashes to `blake3::hash(&[])`.
pub fn compute_merkle_root(transactions: &[Transaction]) -> blake3::Hash {
    let tx_hashes: Vec<blake3::Hash> = transactions.iter().map(|tx| tx.hash()).collect();
    if tx_hashes.is_empty() {
        blake3::hash(&[])
    } else {
        // `merkle_layers` returns >= 1 layer for a non-empty leaf set, and the last
        // layer of a non-empty set always has exactly 1 element.
        //
        // `last()` + `first()` rather than `layers[len - 1][0]`: total, so no panic path. The
        // `None` arm is unreachable for a non-empty leaf set, and it returns the same value the
        // empty branch above produces rather than a different one — so if it ever were reached,
        // the two branches agree instead of silently disagreeing.
        let layers = merkle_layers(tx_hashes);
        layers
            .last()
            .and_then(|layer| layer.first())
            .copied()
            .unwrap_or_else(|| blake3::hash(&[]))
    }
}

/// Verify an uncle proof against a merkle root
/// This verifies:
/// 1. The uncle header re-hashes (with its OWN randomx_key) to a hash meeting `target`
/// 2. The proof is no deeper than MAX_UNCLE_DEPTH
/// 3. The merkle proof verifies the header is in the uncle merkle tree
///
/// The header comes from the caller's `UncleBlock` rather than from the proof, so
/// there is nothing self-referential to check: the hash is recomputed from the
/// header every time and never trusted from the proof.
#[cfg(feature = "pow")]
pub fn verify_uncle_proof(
    header: &BlockHeader,
    proof: &UncleProof,
    merkle_root: &[u8; 32],
    target: BlockTarget,
) -> bool {
    // Step 1: Recompute the PoW hash from the header.
    // Uses to_mining_blob() (same as Block::hash_with_vm) — the uncle
    // was mined as a Block, so its PoW was computed over the mining blob.
    // Must match UncleBlock::hash_with_vm() and build_uncle_merkle(). The VM must
    // be keyed with the uncle's OWN randomx_key (uncles are mined at H-1 with
    // K(H-1); the canonical K(H) would produce garbage).
    let header_bytes = header.to_mining_blob();
    let flags = randomx::RandomXFlags::get_recommended_flags();
    let cache = match randomx::RandomXCache::new(flags, &header.randomx_key) {
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
    let Some(computed_pow_hash) = rx_hash.get(..32).and_then(|s| <[u8; 32]>::try_from(s).ok()) else {
        return false
    };

    // Step 2: Verify the PoW hash meets the difficulty target
    let hash_u32 = u32::from_le_bytes([computed_pow_hash[0], computed_pow_hash[1], computed_pow_hash[2], computed_pow_hash[3]]);
    if !target.hash_is_valid(hash_u32) {
        return false;
    }

    // Step 3: Validate proof depth — must not exceed MAX_UNCLE_DEPTH
    if proof.merkle_path.len() > MAX_UNCLE_DEPTH as usize {
        return false;
    }

    // Step 4: Verify merkle proof
    let mut current = blake3::hash(&header_bytes).as_bytes().to_vec();

    for (level, sibling) in proof.merkle_path.iter().enumerate() {
        // At each level, the position bit tells us left/right
        let bit = (proof.position >> level) & 1;
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

/// Build the uncle merkle tree from uncle blocks.
///
/// Pure blake3 over `blake3(to_mining_blob(header))` leaves: the tree is
/// structural, not proof-of-work. PoW is verified separately by
/// [`verify_uncle_proof`], which is the only place a RandomX hash is needed. This
/// function used to spin up a RandomX cache+VM per uncle purely to fill the
/// proof's now-removed `pow_hash` field.
///
/// The empty uncle set hashes to `[0u8; 32]`.
pub fn build_uncle_merkle(uncles: &[UncleBlock]) -> ([u8; 32], Vec<UncleProof>) {
    if uncles.is_empty() {
        return ([0u8; 32], vec![]);
    }

    // Leaf hash MUST match verify_uncle_proof() — both use to_mining_blob() for the
    // canonical, fixed-length (292-byte) representation. JSON is variable-length
    // and non-canonical (whitespace, key ordering) and cannot be used for proofs.
    let leaves: Vec<blake3::Hash> = uncles
        .iter()
        .map(|u| blake3::hash(&u.header.to_mining_blob()))
        .collect();

    // Same construction as the transaction tree, including the "pad every odd
    // layer" rule — see `merkle_layers`. Padding only the leaf layer (the previous
    // behaviour here) left 3-element intermediate layers for 5 or 6 uncles
    // (MAX_UNCLE_COUNT = 6 permits both), and the pair-index then went out of
    // bounds: a panic reachable from `accept_block`, i.e. a remote DoS on any node.
    let layers = merkle_layers(leaves);
    // `leaves` is non-empty, so the last layer exists and holds exactly 1 element — but that is
    // an argument, not a guarantee the compiler can see; `last`/`first` state it in the code.
    let merkle_root: [u8; 32] = match layers.last().and_then(|l| l.first()) {
        Some(n) => *n.as_bytes(),
        None => [0u8; 32],
    };

    // Build proofs for each uncle
    let proofs: Vec<UncleProof> = (0..uncles.len())
        .map(|i| {
            let mut merkle_path = vec![];
            let mut pos = i;

            // Walk up the tree from leaf to root.
            //
            // `layers[level]` holds the UNPADDED hashes for that level, so an odd
            // layer's LAST element has no right-hand sibling — the padding step in
            // `merkle_layers` pairs it with itself. Its sibling is therefore itself.
            // (The spec clamps the same way:
            // chain_validation_model.py::build_uncle_merkle, "Duplicate last leaf
            // if odd (match Rust)".)
            for level in 0..layers.len() - 1 {
                let Some(current_layer) = layers.get(level) else { break };
                let sibling_pos = if pos % 2 == 1 {
                    pos - 1
                } else if pos + 1 < current_layer.len() {
                    pos + 1
                } else {
                    pos // last element of an odd layer — paired with itself
                };
                // The position arithmetic above keeps `sibling_pos` inside the layer; `get`
                // makes that a read rather than a check compiled into the artifact.
                if let Some(node) = current_layer.get(sibling_pos) {
                    merkle_path.push(*node.as_bytes());
                }

                pos /= 2;
            }

            UncleProof { merkle_path, position: i as u32 }
        })
        .collect();

    (merkle_root, proofs)
}

/// Σ of the pins actually payable to `uncles` — accepted pins only.
///
/// THE single implementation of Σ pin. It was previously re-derived in four
/// places with different filters and different overflow behaviour (`u64::sum()`,
/// which panics in debug and wraps in release; bare `+`; `saturating_add`), on a
/// consensus-critical quantity where silent wrapping would let a block pass the
/// split check with a bogus total.
///
/// Overflow is an error, never a wrap.
pub fn total_accepted_pin(uncles: &[UncleBlock]) -> Result<BlockReward> {
    let mut total: u64 = 0;
    for uncle in uncles {
        // A rejected pin pays nothing.
        if !uncle.pin_accepted {
            continue;
        }
        total = total.checked_add(uncle.pin_confirmed.get()).ok_or_else(|| {
            LinearError::BlockIsInvalid(format!(
                "Σ pin overflow: {} + {}", total, uncle.pin_confirmed
            ))
        })?;
    }
    Ok(BlockReward::new(total))
}

/// Compute reward distribution for canonical miner and uncles
/// Pin mechanism: Uncle chain gets pin reward ONLY if pin_accepted = true
/// Canonical reward = base_reward - sum(uncle pin rewards) (no over-minting)
/// Invariant: canonical_reward + sum(uncle_rewards) = base_reward
/// Returns (canonical_reward, uncle_rewards) — or `Err` if the invariant is
/// violated. A violated supply invariant is a consensus bug; it must abort the
/// block, never degrade to a zero reward and carry on.
pub fn compute_reward(
    base_reward: BlockReward,
    uncles: &[UncleBlock],
) -> Result<(BlockReward, Vec<u64>)> {
    let base = base_reward.get();
    if uncles.is_empty() {
        return Ok((base_reward, vec![]));
    }

    let mut uncle_rewards = Vec::with_capacity(uncles.len());
    for uncle in uncles {
        // Uncle only gets pin_confirmed if they accepted the pin
        let pin = if uncle.pin_accepted { uncle.pin_confirmed.get() } else { 0 };
        uncle_rewards.push(pin);
    }

    let total_pin_confirmed = total_accepted_pin(uncles)?;
    // Canonical reward is base minus what it pays in pins. `verify_uncle_split()`
    // also catches this at commit time; failing here too keeps the two layers
    // consistent instead of one of them silently degrading.
    let canonical_reward = base.checked_sub(total_pin_confirmed.get()).ok_or_else(|| {
        LinearError::BlockIsInvalid(format!(
            "pin rewards ({}) exceed base reward ({base}) — supply invariant violated",
            total_pin_confirmed
        ))
    })?;
    Ok((BlockReward::new(canonical_reward), uncle_rewards))
}

/// Maximum uncle depth allowed (how many generations back an uncle can reference).
pub const MAX_UNCLE_DEPTH: u8 = 6;

/// Maximum number of uncle blocks allowed in a single canonical block.
/// Prevents block bloat and gas exhaustion during uncle transaction execution.
/// One uncle per depth level is the natural bound.
pub const MAX_UNCLE_COUNT: usize = 6;

/// Maximum competing (uncle-candidate) blocks stored per height.
/// Bounds the competing-block sled tree and in-memory caches (H5).
pub const MAX_COMPETING_BLOCKS: usize = 20;

/// Create a new block from transactions (no uncles - Phase 1)
/// Note: This doesn't use RandomX for block creation - the VM and key are
/// passed from the miner which handles PoW. This creates a placeholder block.
pub fn create_block(
    previous: blake3::Hash,
    height: BlockHeight,
    transactions: Vec<Transaction>,
    target: BlockTarget,
) -> Result<Block> {
    create_block_with_uncles(previous, height, transactions, target, &[])
}

/// Create a new block with uncle blocks
/// Note: The block header includes randomx_key but the actual PoW mining
/// is done by the Miner using that key.
pub fn create_block_with_uncles(
    previous: blake3::Hash,
    height: BlockHeight,
    transactions: Vec<Transaction>,
    target: BlockTarget,
    uncles: &[UncleBlock],
) -> Result<Block> {
    // Calculate merkle root for transactions — single canonical algorithm
    // (shared with verify_merkle_root and the genesis ceremony).
    let merkle_root = compute_merkle_root(&transactions);

    // Build uncle merkle and compute rewards (uses blake3 for merkle structure)
    let (uncle_merkle_root, _) = build_uncle_merkle(uncles);
    let base_reward = dwow_sdk::blockchain::expected_reward(height);
    let (total_reward, _) = compute_reward(base_reward, uncles)?;

    #[expect(clippy::unwrap_used, reason = "system clock is always after UNIX_EPOCH")]
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    Ok(Block {
        header: BlockHeader {
            version: BlockVersion::CURRENT,
            previous,
            merkle_root,
            timestamp: BlockTimestamp::new(timestamp),
            target,
            nonce: 0,
            height,
            uncle_merkle_root,
            total_reward,
            randomx_key: [0u8; 32], // Placeholder - miner sets actual key
            miner: [0u8; 32],       // Placeholder - miner sets reward public key (pk_H)
            anchor_owner: [0u8; 32], // Set by the template before mining; must be in the blob
            commitment_merkle_root: [0u8; 32],
            nullifier_root: [0u8; 32],
            anchor_tx_id: [0u8; 32], // No Caribina anchor (set by miner after anchoring)
            caribina_anchor: None,   // No anchor proof until the miner publishes one
            anchor_monero_height: MoneroBlockHeight::new(0), // No Monero anchor (set by miner after anchoring)
            anchor_monero_hash: [0u8; 32], // No Monero anchor
            finality_flags: 0,
            fee_window_flags: FeeWindowFlags::default(), // Set by miner after anchoring
            pow_source: PowSource::Native,

        },
        transactions,
    })
}

#[cfg(all(test, feature = "pow"))]
mod tests {
    use super::*;
    use crate::fee_window::WindowSignalling;

    fn create_test_vm() -> randomx::RandomXVM {
        let key = [0u8; 32];
        let flags = randomx::RandomXFlags::get_recommended_flags();
        let cache = randomx::RandomXCache::new(flags, &key).expect("Failed to create cache");
        randomx::RandomXVM::new(flags, Some(cache), None).expect("Failed to create VM")
    }

    #[test]
    fn test_build_uncle_merkle_empty() {
        let (root, proofs) = build_uncle_merkle(&[]);
        assert_eq!(root, [0u8; 32]);
        assert!(proofs.is_empty());
    }

    #[test]
    fn test_build_uncle_merkle_single() {
        let uncle_header = BlockHeader {
            version: BlockVersion::CURRENT,
            previous: blake3::hash(b"parent"),
            merkle_root: blake3::hash(b"txs"),
            timestamp: BlockTimestamp::new(0),
            target: BlockTarget::new(0x0000_FFFF),
            nonce: 0,
            height: BlockHeight::new(10),
            uncle_merkle_root: [0u8; 32],
            total_reward: BlockReward::ZERO,
            randomx_key: [0u8; 32],
            miner: [0u8; 32],
            commitment_merkle_root: [0u8; 32],
            nullifier_root: [0u8; 32],
            anchor_tx_id: [0u8; 32],
            anchor_monero_height: MoneroBlockHeight::new(0),
            anchor_monero_hash: [0u8; 32],
            finality_flags: 0,
            fee_window_flags: FeeWindowFlags::default(),
            pow_source: PowSource::Native,
            anchor_owner: [0u8; 32],
            caribina_anchor: None,

        };
        let uncle = UncleBlock { header: uncle_header, transactions: vec![], pin_accepted: false, pin_confirmed: BlockReward::new(0) };

        let (root, proofs) = build_uncle_merkle(&[uncle]);
        assert_ne!(root, [0u8; 32]);
        assert_eq!(proofs.len(), 1);
        assert_eq!(proofs[0].position, 0);
        // A single uncle's path is empty: the leaf IS the root (same rule as a
        // single-transaction block).
        assert!(proofs[0].merkle_path.is_empty());
    }

    #[test]
    fn test_build_uncle_merkle_multiple() {
        let mut uncles = vec![];
        for i in 0..3 {
            let header = BlockHeader {
                version: BlockVersion::CURRENT,
                previous: blake3::hash(&[i]),
                merkle_root: blake3::hash(&[i]),
                timestamp: BlockTimestamp::new(i as u64),
                target: BlockTarget::new(0x0000_FFFF),
                nonce: i as u32,
                height: BlockHeight::new(10 + i as u64),
                uncle_merkle_root: [0u8; 32],
                total_reward: BlockReward::ZERO,
                randomx_key: [0u8; 32],
                miner: [0u8; 32],
                commitment_merkle_root: [0u8; 32],
                nullifier_root: [0u8; 32],
                anchor_tx_id: [0u8; 32],
                anchor_monero_height: MoneroBlockHeight::new(0),
                anchor_monero_hash: [0u8; 32],
                finality_flags: 0,
            fee_window_flags: FeeWindowFlags::default(),
            pow_source: PowSource::Native,
            anchor_owner: [0u8; 32],
            caribina_anchor: None,

            };
            uncles.push(UncleBlock { header, transactions: vec![], pin_accepted: false, pin_confirmed: BlockReward::new(0) });
        }

        let (root, proofs) = build_uncle_merkle(&uncles);
        assert_ne!(root, [0u8; 32]);
        assert_eq!(proofs.len(), 3);
        // 3 uncles pads to 4 leaves → depth 2. (PoW is not checked here: the
        // nonces are arbitrary, so verify_uncle_proof may fail the target.)
        for (i, proof) in proofs.iter().enumerate() {
            assert_eq!(proof.position, i as u32);
            assert_eq!(proof.merkle_path.len(), 2);
        }
    }

    /// consensus-coinbase.md §4: build_uncle_merkle MUST produce proofs that
    /// verify_uncle_proof can verify. The leaf hash inputs MUST match —
    /// both use to_mining_blob() for canonical representation.
    #[test]
    fn test_uncle_merkle_proof_round_trip() {
        let mut uncles = vec![];
        for i in 0..3 {
            let header = BlockHeader {
                version: BlockVersion::CURRENT,
                previous: blake3::hash(&[i]),
                merkle_root: blake3::hash(&[i]),
                timestamp: BlockTimestamp::new(i as u64),
                target: BlockTarget::new(0xFFFF_FFFF), // max target — any hash passes
                nonce: i as u32,
                height: BlockHeight::new(10 + i as u64),
                uncle_merkle_root: [0u8; 32],
                total_reward: BlockReward::ZERO,
                randomx_key: [0u8; 32],
                miner: [0u8; 32],
                commitment_merkle_root: [0u8; 32],
                nullifier_root: [0u8; 32],
                anchor_tx_id: [0u8; 32],
                anchor_monero_height: MoneroBlockHeight::new(0),
                anchor_monero_hash: [0u8; 32],
                finality_flags: 0,
            fee_window_flags: FeeWindowFlags::default(),
                pow_source: PowSource::Native,
                anchor_owner: [0u8; 32],
                caribina_anchor: None,
            };
            uncles.push(UncleBlock {
                header,
                transactions: vec![],
                pin_accepted: false,
                pin_confirmed: BlockReward::new(0),
            });
        }
        let (root, proofs) = build_uncle_merkle(&uncles);
        assert_ne!(root, [0u8; 32]);
        assert_eq!(proofs.len(), 3);
        for (uncle, proof) in uncles.iter().zip(proofs.iter()) {
            // Leaf hash inputs MUST match — verify_uncle_proof uses
            // to_mining_blob(), build_uncle_merkle must also use it.
            // With target=u32::MAX, any hash passes difficulty.
            assert!(
                verify_uncle_proof(
                    &uncle.header, proof, &root, BlockTarget::new(0xFFFF_FFFF)
                ),
                "Uncle proof at position {} must verify against merkle root",
                proof.position
            );
        }
    }

    /// Regression: `MAX_UNCLE_COUNT` is 6, so 5 and 6 uncles are both
    /// admissible — and both produced a 3-element intermediate layer. Padding
    /// only the leaf layer left that intermediate layer odd, and the `chunks(2)`
    /// pair-index then indexed out of bounds: `debug_assert` in debug, `panic`
    /// in release. Any peer could therefore crash a node by broadcasting a 5- or
    /// 6-uncle block, and the miner crashed itself building one.
    ///
    /// Every admissible count must build a tree, produce one proof per uncle at
    /// the padded depth `ceil(log2(n))`, and have that proof verify against the
    /// root — a builder/verifier padding mismatch would be a consensus bug.
    #[test]
    fn test_build_uncle_merkle_all_admissible_counts() {
        let (empty_root, empty_proofs) = build_uncle_merkle(&[]);
        assert_eq!(empty_root, [0u8; 32]);
        assert!(empty_proofs.is_empty());

        for n in 1..=MAX_UNCLE_COUNT {
            let uncles: Vec<UncleBlock> = (0..n)
                .map(|i| {
                    let header = BlockHeader {
                        version: BlockVersion::CURRENT,
                        previous: blake3::hash(&[i as u8]),
                        merkle_root: blake3::hash(&[i as u8]),
                        timestamp: BlockTimestamp::new(i as u64),
                        target: BlockTarget::new(0xFFFF_FFFF), // max target — any hash passes
                        nonce: i as u32,
                        height: BlockHeight::new(10 + i as u64),
                        uncle_merkle_root: [0u8; 32],
                        total_reward: BlockReward::ZERO,
                        randomx_key: [0u8; 32],
                        miner: [0u8; 32],
                        commitment_merkle_root: [0u8; 32],
                        nullifier_root: [0u8; 32],
                        anchor_tx_id: [0u8; 32],
                        anchor_monero_height: MoneroBlockHeight::new(0),
                        anchor_monero_hash: [0u8; 32],
                        finality_flags: 0,
                        fee_window_flags: FeeWindowFlags::default(),
                        pow_source: PowSource::Native,
                        anchor_owner: [0u8; 32],
                        caribina_anchor: None,
                    };
                    UncleBlock {
                        header,
                        transactions: vec![],
                        pin_accepted: false,
                        pin_confirmed: BlockReward::new(0),
                    }
                })
                .collect();

            // The panic this test guards against fired in here.
            let (root, proofs) = build_uncle_merkle(&uncles);
            assert_ne!(root, [0u8; 32], "{n} uncles: root must be non-zero");
            assert_eq!(proofs.len(), n, "{n} uncles: one proof per uncle");

            // Every layer is padded to even length, so the tree is a full binary
            // tree and each path has depth ceil(log2(n)).
            let expected_depth = if n <= 1 {
                0
            } else {
                (usize::BITS - (n - 1).leading_zeros()) as usize
            };
            for (uncle, proof) in uncles.iter().zip(proofs.iter()) {
                assert_eq!(
                    proof.merkle_path.len(),
                    expected_depth,
                    "{n} uncles: position {} path depth",
                    proof.position
                );
                assert!(
                    verify_uncle_proof(
                        &uncle.header, proof, &root, BlockTarget::new(0xFFFF_FFFF)
                    ),
                    "{n} uncles: proof at position {} must verify against the root",
                    proof.position
                );
            }
        }
    }

    #[test]
    fn test_compute_reward_no_uncles() {
        let (canonical, uncles) = compute_reward(BlockReward::new(100_000_000), &[]).expect("no uncles");
        assert_eq!(canonical, BlockReward::new(100_000_000));
        assert!(uncles.is_empty());
    }

    #[test]
    fn test_compute_reward_with_uncles() {
        let uncle_header = BlockHeader {
            version: BlockVersion::CURRENT,
            previous: blake3::hash(b"parent"),
            merkle_root: blake3::hash(b"txs"),
            timestamp: BlockTimestamp::new(0),
            target: BlockTarget::new(0x0000_FFFF),
            nonce: 0,
            height: BlockHeight::new(10),
            uncle_merkle_root: [0u8; 32],
            total_reward: BlockReward::ZERO,
            randomx_key: [0u8; 32],
            miner: [0u8; 32],
            commitment_merkle_root: [0u8; 32],
            nullifier_root: [0u8; 32],
            anchor_tx_id: [0u8; 32],
            anchor_monero_height: MoneroBlockHeight::new(0),
            anchor_monero_hash: [0u8; 32],
            finality_flags: 0,
            fee_window_flags: FeeWindowFlags::default(),
            pow_source: PowSource::Native,
            anchor_owner: [0u8; 32],
            caribina_anchor: None,

        };
        // Pin mechanism: pin_accepted=true means the uncle accepts the pin.
        // pin_confirmed at depth 1 = 50% = 50M.
        let uncle = UncleBlock { header: uncle_header, transactions: vec![], pin_accepted: true, pin_confirmed: BlockReward::new(50_000_000) };

        let (canonical, uncle_rewards) =
            compute_reward(BlockReward::new(100_000_000), &[uncle]).expect("split");
        // base 100M - pin 50M = 50M canonical (no over-minting)
        assert_eq!(canonical, BlockReward::new(50_000_000));
        assert_eq!(uncle_rewards.len(), 1);
        assert_eq!(uncle_rewards[0], 50_000_000);
    }

    #[test]
    fn test_verify_uncle_proof() {
        let header = BlockHeader {
            version: BlockVersion::CURRENT,
            previous: blake3::hash(b"parent"),
            merkle_root: blake3::hash(b"txs"),
            timestamp: BlockTimestamp::new(0),
            target: BlockTarget::new(0x0000_FFFF),
            nonce: 42,
            height: BlockHeight::new(10),
            uncle_merkle_root: [0u8; 32],
            total_reward: BlockReward::ZERO,
            randomx_key: [0u8; 32],
            miner: [0u8; 32],
            commitment_merkle_root: [0u8; 32],
            nullifier_root: [0u8; 32],
            anchor_tx_id: [0u8; 32],
            anchor_monero_height: MoneroBlockHeight::new(0),
            anchor_monero_hash: [0u8; 32],
            finality_flags: 0,
            fee_window_flags: FeeWindowFlags::default(),
            pow_source: PowSource::Native,
            anchor_owner: [0u8; 32],
            caribina_anchor: None,

        };
        let uncle = UncleBlock { header: header.clone(), transactions: vec![], pin_accepted: false, pin_confirmed: BlockReward::new(0) };

        let (root, proofs) = build_uncle_merkle(&[uncle]);
        // A single uncle's leaf IS the root, so the proof is an empty path and the
        // root is the header's mining-blob hash.
        assert_eq!(root, *blake3::hash(&header.to_mining_blob()).as_bytes());
        assert!(proofs[0].merkle_path.is_empty());

        // With an impossible target the PoW gate rejects...
        assert!(!verify_uncle_proof(
            &header, &proofs[0], &root, BlockTarget::new(0x0000_0000)
        ));
        // ...and with the root replaced the merkle verification rejects.
        assert!(!verify_uncle_proof(
            &header, &proofs[0], &[1u8; 32], BlockTarget::new(0xFFFF_FFFF)
        ));
        // The honest target accepts (nonce 42 is arbitrary, so this asserts the
        // PoW gate is *satisfiable*, not that this nonce wins a real race).
        assert!(verify_uncle_proof(
            &header, &proofs[0], &root, BlockTarget::new(0xFFFF_FFFF)
        ));
    }

    #[test]
    fn test_create_block_with_uncles() {
        let previous = blake3::hash(b"genesis");
        let block = create_block_with_uncles(
            previous,
            BlockHeight::new(1),
            vec![],
            BlockTarget::new(0x0000_FFFF),
            &[],
        ).expect("test block creation failed");

        assert_eq!(block.header.previous, previous);
        assert_eq!(block.header.height, BlockHeight::new(1));
        assert_eq!(block.header.uncle_merkle_root, [0u8; 32]);
        // With no uncles, total_reward = base_reward = expected_reward(height)
        assert_eq!(block.header.total_reward, dwow_sdk::blockchain::expected_reward(BlockHeight::new(1)));
    }

    /// Verify the coinbase lifecycle: create blocks at heights 1, 2, 3,
    /// check rewards follow the exponential-decay emission schedule.
    #[test]
    fn test_coinbase_lifecycle() {
        let vm = create_test_vm();

        // Height 1: genesis — full INITIAL_REWARD (~13.84 DRKW).
        // The cumulative chain S_H = S_{H-1} + C_H starts here:
        // S_1 = identity + C_1 where C_1 commits to INITIAL_REWARD.
        let block1 = create_block_with_uncles(
            blake3::hash(b"genesis"),
            BlockHeight::new(1),
            vec![],
            BlockTarget::new(0x0000_FFFF),
            &[],
        ).expect("test block creation failed");
        let reward1 = dwow_sdk::blockchain::expected_reward(BlockHeight::new(1));
        assert_eq!(block1.header.total_reward, reward1);
        assert_eq!(reward1, dwow_sdk::blockchain::reward::INITIAL_REWARD,
            "genesis height 1 reward should be INITIAL_REWARD");

        // Height 2: first decay step from INITIAL_REWARD.
        let block2 = create_block_with_uncles(
            block1.hash_with_vm(&vm).expect("hash failed"),
            BlockHeight::new(2),
            vec![],
            BlockTarget::new(0x0000_FFFF),
            &[],
        ).expect("test block creation failed");
        let reward2 = dwow_sdk::blockchain::expected_reward(BlockHeight::new(2));
        assert_eq!(block2.header.total_reward, reward2);
        assert!(reward2 > BlockReward::new(1_000_000_000), "height 2 reward should be > 1B base units");

        // Height 3: slightly less than height 2 (exponential decay)
        let block3 = create_block_with_uncles(
            block2.hash_with_vm(&vm).expect("hash failed"),
            BlockHeight::new(3),
            vec![],
            BlockTarget::new(0x0000_FFFF),
            &[],
        ).expect("test block creation failed");
        let reward3 = dwow_sdk::blockchain::expected_reward(BlockHeight::new(3));
        assert_eq!(block3.header.total_reward, reward3);
        assert!(reward3 <= reward2, "reward must decay monotonically");

        // All rewards from genesis onward must be >= TAIL_REWARD
        let tail = dwow_sdk::blockchain::reward::TAIL_REWARD;
        assert!(reward1 >= tail, "genesis reward must be >= tail emission");
        assert!(reward2 >= tail);
        assert!(reward3 >= tail);
    }

    /// Verify create_block (without uncles) uses expected_reward.
    #[test]
    fn test_create_block_reward() {
        let previous = blake3::hash(b"genesis");

        let block = create_block(previous, BlockHeight::new(42), vec![], BlockTarget::new(0x0000_FFFF))
            .expect("test block creation failed");
        let expected = dwow_sdk::blockchain::expected_reward(BlockHeight::new(42));
        assert_eq!(block.header.total_reward, expected);
        assert_eq!(block.header.height, BlockHeight::new(42));
    }

    /// Caribina: mining blob must exclude anchor_tx_id so PoW hash doesn't
    /// change after anchoring.
    #[test]
    fn test_mining_blob_excludes_anchor() {
        let mut header = BlockHeader {
            version: BlockVersion::CURRENT,
            previous: blake3::hash(b"parent"),
            merkle_root: blake3::hash(b"txs"),
            timestamp: BlockTimestamp::new(1000),
            target: BlockTarget::new(0x0000_FFFF),
            nonce: 42,
            height: BlockHeight::new(1),
            uncle_merkle_root: [0u8; 32],
            total_reward: BlockReward::new(100_000_000),
            randomx_key: [0u8; 32],
            miner: [0u8; 32],
            commitment_merkle_root: [0u8; 32],
            nullifier_root: [0u8; 32],
            anchor_tx_id: [0u8; 32],
            anchor_monero_height: MoneroBlockHeight::new(0),
            anchor_monero_hash: [0u8; 32],
            finality_flags: 0,
            fee_window_flags: FeeWindowFlags::default(),
            pow_source: PowSource::Native,
            anchor_owner: [0u8; 32],
            caribina_anchor: None,

        };

        let blob1 = header.to_mining_blob();
        assert_eq!(blob1.len(), 292);
        assert_eq!(BlockHeader::MINING_BLOB_LEN, 292);

        // Pin the offsets that outside code reads the blob by. When the `miner`
        // field was added the blob grew 228 -> 260, and the len/offset literals
        // in bin/dwowd/src/rpc/mm_rpc.rs were left behind, so two tests failed
        // for a reason that had nothing to do with mining. Naming the offsets
        // here means the next layout change is a compile error, not a mystery.
        assert_eq!(BlockHeader::NONCE_OFFSET, 39);
        assert_eq!(BlockHeader::POW_SOURCE_OFFSET, 227);
        assert_eq!(BlockHeader::ANCHOR_OWNER_OFFSET, 260);
        assert_eq!(blob1[BlockHeader::POW_SOURCE_OFFSET], 0,
            "native PoW writes discriminator 0 at POW_SOURCE_OFFSET");
        assert_eq!(&blob1[BlockHeader::POW_SOURCE_OFFSET + 1..BlockHeader::ANCHOR_OWNER_OFFSET],
            &header.miner,
            "the miner pubkey sits right after the discriminator");
        assert_eq!(&blob1[BlockHeader::ANCHOR_OWNER_OFFSET..], &header.anchor_owner,
            "anchor_owner is the tail of the blob, and being in the blob is the whole point: \
             it is what makes the anchor's author unforgeable (OBL-C64)");

        // The finality fields the miner sets *after* finding the nonce must not change the
        // blob, or the PoW solution would be invalidated by anchoring. `anchor_owner` above is
        // the deliberate exception — it is set before mining.
        header.anchor_tx_id = [0xAB; 32];
        header.finality_flags = 0x07;
        header.anchor_monero_height = MoneroBlockHeight::new(3_000_000);
        header.anchor_monero_hash = [0xCD; 32];
        let blob2 = header.to_mining_blob();
        assert_eq!(blob1, blob2, "post-mining anchor fields must not touch the mining preimage");

        // And the converse, which is the property the fix added: changing the *owner* does
        // change the blob, so a relayer cannot re-attribute an anchor without redoing PoW.
        header.anchor_owner = [0xEF; 32];
        assert_ne!(blob1, header.to_mining_blob(),
            "anchor_owner is PoW-covered — swapping it must change the preimage (OBL-C64)");
    }

    /// OBL-C64 — the anchor's *author* is authenticated by the work that produced the block, while the
    /// post-mining finality fields stay outside the preimage.
    ///
    /// **This test was the characterization test for the defect and is now the regression control**, and
    /// the distinction it draws is the design. `anchor_owner` is inside `to_mining_blob()`, so a relaying
    /// peer cannot re-attribute an anchor to a different key without redoing the proof-of-work — which is
    /// what stops anyone publishing an anchor for a block they did not mine. The four fields the miner
    /// sets *after* finding the nonce (`anchor_tx_id`, `anchor_monero_height`, `anchor_monero_hash`,
    /// `finality_flags`) remain outside it, because they must not invalidate the PoW solution; they are
    /// no longer consulted for finality, which is why their malleability no longer buys an attacker
    /// anything (see `chain_state.rs`'s enforcement, which requires a verified proof instead).
    ///
    /// The earlier version of this test asserted that *all* of them were invariant, i.e. that none was
    /// authenticated — the defect. It failed when `anchor_owner` was added, as it was written to.
    #[test]
    fn test_anchor_owner_is_pow_covered_but_post_mining_fields_are_not() {
        let base = BlockHeader {
            version: BlockVersion::CURRENT,
            previous: blake3::hash(b"parent"),
            merkle_root: blake3::hash(b"txs"),
            timestamp: BlockTimestamp::new(1000),
            target: BlockTarget::new(0x0000_FFFF),
            nonce: 42,
            height: BlockHeight::new(1),
            uncle_merkle_root: [0u8; 32],
            total_reward: BlockReward::new(100_000_000),
            randomx_key: [0u8; 32],
            miner: [0x22; 32],
            commitment_merkle_root: [0u8; 32],
            nullifier_root: [0u8; 32],
            anchor_tx_id: [0u8; 32],
            anchor_monero_height: MoneroBlockHeight::new(0),
            anchor_monero_hash: [0u8; 32],
            finality_flags: 0,
            fee_window_flags: FeeWindowFlags::default(),
            pow_source: PowSource::Native,
            anchor_owner: [0u8; 32],
            caribina_anchor: None,
        };
        let blob = base.to_mining_blob();

        let variants = [
            ([0xAB; 32], MoneroBlockHeight::new(0), [0u8; 32], 0),
            ([0xCD; 32], MoneroBlockHeight::new(0), [0u8; 32], 0),
            ([0u8; 32], MoneroBlockHeight::new(3_000_000), [0u8; 32], 0),
            ([0u8; 32], MoneroBlockHeight::new(0), [0xEE; 32], 0),
            ([0u8; 32], MoneroBlockHeight::new(0), [0u8; 32], 0x04),
            ([0xFF; 32], MoneroBlockHeight::new(3_000_000), [0xFF; 32], 0x07),
        ];
        for (tx_id, monero_height, monero_hash, flags) in variants {
            let mut h = base.clone();
            h.anchor_tx_id = tx_id;
            h.anchor_monero_height = monero_height;
            h.anchor_monero_hash = monero_hash;
            h.finality_flags = flags;
            assert_eq!(
                h.to_mining_blob(),
                blob,
                "a variant differing only in post-mining anchor fields must leave the mining preimage \
                 alone, or anchoring would invalidate the PoW solution it is anchoring"
            );
        }

        // The authenticated case: the owner is in the preimage.
        let mut owned = base.clone();
        owned.anchor_owner = [0xAA; 32];
        let owned_blob = owned.to_mining_blob();
        assert_ne!(owned_blob, blob, "anchor_owner must be PoW-covered");
        assert_eq!(
            &owned_blob[BlockHeader::ANCHOR_OWNER_OFFSET..],
            &[0xAA; 32],
            "and it is the tail of the blob, at ANCHOR_OWNER_OFFSET"
        );

        // Two owners never share a preimage, which is what makes the owner an authenticator rather
        // than a label. Asserted over several values because a single pair could collide by accident
        // of the layout (e.g. if the field were written at the wrong offset, overwriting `miner`).
        for owner in [[0x01; 32], [0x7F; 32], [0xFF; 32]] {
            let mut h = base.clone();
            h.anchor_owner = owner;
            assert_ne!(h.to_mining_blob(), owned_blob, "distinct owners, distinct preimages");
        }
    }

    /// OBL-C69 (fixed) — the serde serialization of `BlockHeader` preserves `pow_source`.
    ///
    /// **This test was the characterization test for the defect and is now the regression control.**
    /// `pow_source` was declared `#[serde(default = "PowSource::native", skip)]`, so it was never
    /// written and deserialization reconstructed `PowSource::Native` — a merge-mined block was
    /// reclassified as native. That was not merely data loss: the merge-mining path skips native PoW
    /// verification precisely *because* the header was never hashed by xmrig, so the downgraded block
    /// asserted a PoW claim it could not satisfy. The P2P block wire is serde
    /// (`linear_broadcast.rs:112`, `:144`, `:172`), so this was the live path.
    ///
    /// `PowSource` now has `Serialize`/`Deserialize` impls that carry the canonical codec's bytes
    /// (`src/linear/src/serial_sync.rs`), so the two serializations of this consensus field cannot
    /// disagree again. The stronger assertion lives in `bin/dwowd/src/tests/wire_format.rs`, which
    /// round-trips a block that genuinely carries a `MoneroPowData`.
    #[test]
    fn test_serde_serialization_of_header_carries_pow_source() {
        let header = BlockHeader {
            version: BlockVersion::CURRENT,
            previous: blake3::hash(b"parent"),
            merkle_root: blake3::hash(b"txs"),
            timestamp: BlockTimestamp::new(1000),
            target: BlockTarget::new(0x0000_FFFF),
            nonce: 7,
            height: BlockHeight::new(1),
            uncle_merkle_root: [0u8; 32],
            total_reward: BlockReward::new(100_000_000),
            randomx_key: [0u8; 32],
            miner: [0x22; 32],
            commitment_merkle_root: [0u8; 32],
            nullifier_root: [0u8; 32],
            anchor_tx_id: [0u8; 32],
            anchor_monero_height: MoneroBlockHeight::new(0),
            anchor_monero_hash: [0u8; 32],
            finality_flags: 0,
            fee_window_flags: FeeWindowFlags::default(),
            pow_source: PowSource::Native,
            anchor_owner: [0u8; 32],
            caribina_anchor: None,
        };

        let json = serde_json::to_string(&header).expect("BlockHeader is Serialize");
        assert!(
            json.contains("pow_source"),
            "the serde encoding must carry `pow_source`; when it did not, a relayed merge-mined block \
             arrived reclassified as native (OBL-C69). If this fails the `skip` attribute is back."
        );

        // Round-trip it, so the assertion is about the wire and not only about one direction.
        let decoded: BlockHeader = serde_json::from_str(&json).expect("BlockHeader is Deserialize");
        assert!(
            matches!(decoded.pow_source, PowSource::Native),
            "a native header must round-trip as native"
        );

        // The Monero variant's round-trip is asserted in
        // `bin/dwowd/src/tests/wire_format.rs`, which has a real `MoneroPowData` fixture to hand —
        // constructing one here would mean parsing a Monero testnet block inside a unit test.
    }

    /// L1-FW-5a: fee_window_flags excluded from mining blob + len invariant.
    /// Partition B — PoW/wire boundary. Flags are set after mining, must not
    /// touch the PoW hash. Pattern: test_mining_blob_excludes_anchor.
    #[test]
    fn test_mining_blob_excludes_fee_window_flags() {
        let mut header = BlockHeader {
            version: BlockVersion::CURRENT,
            previous: blake3::hash(b"parent"),
            merkle_root: blake3::hash(b"txs"),
            timestamp: BlockTimestamp::new(1000),
            target: BlockTarget::new(0x0000_FFFF),
            nonce: 42,
            height: BlockHeight::new(1),
            uncle_merkle_root: [0u8; 32],
            total_reward: BlockReward::new(100_000_000),
            randomx_key: [0u8; 32],
            miner: [0u8; 32],
            commitment_merkle_root: [0u8; 32],
            nullifier_root: [0u8; 32],
            anchor_tx_id: [0u8; 32],
            anchor_monero_height: MoneroBlockHeight::new(0),
            anchor_monero_hash: [0u8; 32],
            finality_flags: 0,
            fee_window_flags: FeeWindowFlags::default(),
            pow_source: PowSource::Native,
            anchor_owner: [0u8; 32],
            caribina_anchor: None,
        };

        let blob_zero = header.to_mining_blob();
        assert_eq!(blob_zero.len(), 292);

        // Setting fee_window_flags must not change the mining blob
        header.fee_window_flags = FeeWindowFlags::pack(
            WindowSignalling::encode_cm(0x01),  // circuit: +10%
            WindowSignalling::encode_cm(0x00),  // wasm: hold
        );
        let blob_flagged = header.to_mining_blob();
        assert_eq!(blob_zero, blob_flagged,
            "fee_window_flags must not affect mining blob");
    }

    /// L1-FW-5b: BlockHeader serde roundtrip preserves fee_window_flags.
    /// Partition B — persistence/wire lift.
    #[test]
    fn test_fee_window_flags_serde_roundtrip() {
        let mut header = BlockHeader {
            version: BlockVersion::CURRENT,
            previous: blake3::hash(b"parent"),
            merkle_root: blake3::hash(b"txs"),
            timestamp: BlockTimestamp::new(1000),
            target: BlockTarget::new(0x0000_FFFF),
            nonce: 0,
            height: BlockHeight::new(1),
            uncle_merkle_root: [0u8; 32],
            total_reward: BlockReward::new(100_000_000),
            randomx_key: [0u8; 32],
            miner: [0u8; 32],
            commitment_merkle_root: [0u8; 32],
            nullifier_root: [0u8; 32],
            anchor_tx_id: [0u8; 32],
            anchor_monero_height: MoneroBlockHeight::new(0),
            anchor_monero_hash: [0u8; 32],
            finality_flags: 0,
            fee_window_flags: FeeWindowFlags::pack(
                WindowSignalling::encode_cm(0x01),
                WindowSignalling::encode_cm(0x00),
            ),
            pow_source: PowSource::Native,
            anchor_owner: [0u8; 32],
            caribina_anchor: None,
        };

        // JSON roundtrip preserves flags
        let json = serde_json::to_vec(&header).expect("serialize");
        let restored: BlockHeader = serde_json::from_slice(&json).expect("deserialize");
        assert_eq!(restored.fee_window_flags, FeeWindowFlags::pack(
            WindowSignalling::encode_cm(0x01),
            WindowSignalling::encode_cm(0x00),
        ),
            "fee_window_flags should survive serde roundtrip");

        // Zero flags also roundtrip correctly
        header.fee_window_flags = FeeWindowFlags::default();
        let json2 = serde_json::to_vec(&header).expect("serialize zero");
        let restored2: BlockHeader = serde_json::from_slice(&json2).expect("deserialize zero");
        assert_eq!(restored2.fee_window_flags, FeeWindowFlags::default(),
            "zero fee_window_flags should survive serde roundtrip");
    }

    /// Caribina: default anchor_tx_id is zero (no anchor).
    #[test]
    fn test_anchor_tx_id_default_is_zero() {
        let header = BlockHeader {
            version: BlockVersion::CURRENT,
            previous: blake3::hash(b"parent"),
            merkle_root: blake3::hash(b"txs"),
            timestamp: BlockTimestamp::new(1000),
            target: BlockTarget::new(0x0000_FFFF),
            nonce: 0,
            height: BlockHeight::new(0),
            uncle_merkle_root: [0u8; 32],
            total_reward: BlockReward::ZERO,
            randomx_key: [0u8; 32],
            miner: [0u8; 32],
            commitment_merkle_root: [0u8; 32],
            nullifier_root: [0u8; 32],
            anchor_tx_id: [0u8; 32],
            anchor_monero_height: MoneroBlockHeight::new(0),
            anchor_monero_hash: [0u8; 32],
            finality_flags: 0,
            fee_window_flags: FeeWindowFlags::default(),
            pow_source: PowSource::Native,
            anchor_owner: [0u8; 32],
            caribina_anchor: None,

        };
        assert_eq!(header.anchor_tx_id, [0u8; 32]);
    }

    /// Caribina: serde roundtrip preserves anchor_tx_id.
    #[test]
    fn test_block_header_with_anchor_serde() {
        let header = BlockHeader {
            version: BlockVersion::CURRENT,
            previous: blake3::hash(b"parent"),
            merkle_root: blake3::hash(b"txs"),
            timestamp: BlockTimestamp::new(1000),
            target: BlockTarget::new(0x0000_FFFF),
            nonce: 42,
            height: BlockHeight::new(1),
            uncle_merkle_root: [0u8; 32],
            total_reward: BlockReward::new(100_000_000),
            randomx_key: [0xAA; 32],
            miner: [0xAA; 32],
            commitment_merkle_root: [0u8; 32],
            nullifier_root: [0u8; 32],
            anchor_tx_id: [0xBB; 32],
            anchor_monero_height: MoneroBlockHeight::new(0),
            anchor_monero_hash: [0u8; 32],
            finality_flags: 0,
            fee_window_flags: FeeWindowFlags::default(),
            pow_source: PowSource::Native,
            anchor_owner: [0u8; 32],
            caribina_anchor: None,

        };

        let json = serde_json::to_string(&header).unwrap();
        let deserialized: BlockHeader = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.anchor_tx_id, [0xBB; 32]);
        assert_eq!(deserialized.nonce, 42);
        assert_eq!(deserialized.height, BlockHeight::new(1));
    }

    /// Backward-compatible deserialization: old blocks without the new fields
    /// (commitment_merkle_root, nullifier_root, anchor_tx_id, anchor_monero_height,
    /// anchor_monero_hash, finality_flags) must still deserialize with defaults.
    #[test]
    fn test_block_header_deserialize_old_format() {
        // Build a header, serialize it, then remove the new fields from the JSON
        let header = BlockHeader {
            version: BlockVersion::CURRENT,
            previous: blake3::hash(b"parent"),
            merkle_root: blake3::hash(b"txs"),
            timestamp: BlockTimestamp::new(1000),
            target: BlockTarget::new(0x0000_FFFF),
            nonce: 42,
            height: BlockHeight::new(1),
            uncle_merkle_root: [0u8; 32],
            total_reward: BlockReward::new(100_000_000),
            randomx_key: [0xAA; 32],
            miner: [0xAA; 32],
            commitment_merkle_root: [0u8; 32],
            nullifier_root: [0u8; 32],
            anchor_tx_id: [0u8; 32],
            anchor_monero_height: MoneroBlockHeight::new(0),
            anchor_monero_hash: [0u8; 32],
            finality_flags: 0,
            fee_window_flags: FeeWindowFlags::default(),
            pow_source: PowSource::Native,
            anchor_owner: [0u8; 32],
            caribina_anchor: None,

        };

        let full_json = serde_json::to_string(&header).unwrap();

        // Parse, remove the new fields, and re-serialize to get "old format" JSON
        let mut val: serde_json::Value = serde_json::from_str(&full_json).unwrap();
        let obj = val.as_object_mut().unwrap();
        obj.remove("commitment_merkle_root");
        obj.remove("nullifier_root");
        obj.remove("anchor_tx_id");
        obj.remove("anchor_monero_height");
        obj.remove("anchor_monero_hash");
        obj.remove("finality_flags");
        let old_json = serde_json::to_string(&obj).unwrap();

        // Deserialize the old format — must succeed with defaults
        let deserialized: BlockHeader = serde_json::from_str(&old_json).unwrap();
        assert_eq!(deserialized.version, BlockVersion::new(1));
        assert_eq!(deserialized.nonce, 42);
        assert_eq!(deserialized.height, BlockHeight::new(1));
        assert_eq!(deserialized.commitment_merkle_root, [0u8; 32]);
        assert_eq!(deserialized.nullifier_root, [0u8; 32]);
        assert_eq!(deserialized.anchor_tx_id, [0u8; 32]);
        assert_eq!(deserialized.anchor_monero_height, MoneroBlockHeight::new(0));
        assert_eq!(deserialized.anchor_monero_hash, [0u8; 32]);
        assert_eq!(deserialized.finality_flags, 0);
    }

    /// Monero anchor fields must be excluded from mining blob for dual-finality.
    #[test]
    fn test_mining_blob_excludes_monero_anchor() {
        let mut header = BlockHeader {
            version: BlockVersion::CURRENT,
            previous: blake3::hash(b"parent"),
            merkle_root: blake3::hash(b"txs"),
            timestamp: BlockTimestamp::new(1000),
            target: BlockTarget::new(0x0000_FFFF),
            nonce: 42,
            height: BlockHeight::new(1),
            uncle_merkle_root: [0u8; 32],
            total_reward: BlockReward::new(100_000_000),
            randomx_key: [0u8; 32],
            miner: [0u8; 32],
            commitment_merkle_root: [0u8; 32],
            nullifier_root: [0u8; 32],
            anchor_tx_id: [0u8; 32],
            anchor_monero_height: MoneroBlockHeight::new(0),
            anchor_monero_hash: [0u8; 32],
            finality_flags: 0,
            fee_window_flags: FeeWindowFlags::default(),
            pow_source: PowSource::Native,
            anchor_owner: [0u8; 32],
            caribina_anchor: None,

        };

        let blob1 = header.to_mining_blob();

        // Setting Monero anchor fields must not change the mining blob
        header.anchor_monero_height = MoneroBlockHeight::new(3_500_000);
        header.anchor_monero_hash = [0xCD; 32];
        header.finality_flags = 0xFF;
        let blob2 = header.to_mining_blob();
        assert_eq!(blob1, blob2);
    }

    /// Monero anchor fields survive serde roundtrip.
    #[test]
    fn test_monero_anchor_serde_roundtrip() {
        let header = BlockHeader {
            version: BlockVersion::CURRENT,
            previous: blake3::hash(b"parent"),
            merkle_root: blake3::hash(b"txs"),
            timestamp: BlockTimestamp::new(1000),
            target: BlockTarget::new(0x0000_FFFF),
            nonce: 42,
            height: BlockHeight::new(1),
            uncle_merkle_root: [0u8; 32],
            total_reward: BlockReward::new(100_000_000),
            randomx_key: [0xAA; 32],
            miner: [0xAA; 32],
            commitment_merkle_root: [0u8; 32],
            nullifier_root: [0u8; 32],
            anchor_tx_id: [0xBB; 32],
            anchor_monero_height: MoneroBlockHeight::new(3_500_000),
            anchor_monero_hash: [0xCC; 32],
            finality_flags: 0x02,
            fee_window_flags: FeeWindowFlags::default(), // FINALITY_MONERO
            pow_source: PowSource::Native,
            anchor_owner: [0u8; 32],
            caribina_anchor: None,
        };

        let json = serde_json::to_string(&header).unwrap();
        let deserialized: BlockHeader = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.anchor_monero_height, MoneroBlockHeight::new(3_500_000));
        assert_eq!(deserialized.anchor_monero_hash, [0xCC; 32]);
        assert_eq!(deserialized.finality_flags, 0x02);
        assert_eq!(deserialized.anchor_tx_id, [0xBB; 32]);
        assert_eq!(deserialized.nonce, 42);
    }

    /// Sentinel: mining blob byte-level stability (Change 4 prerequisite).
    ///
    /// Constructs a BlockHeader with known field values and verifies
    /// `to_mining_blob()` produces a 292-byte output matching a hardcoded
    /// reference. This test gates the consensus newtype migration (BlockTarget,
    /// BlockReward) — the blob MUST be byte-identical before and after the
    /// migration. A single-byte difference breaks ALL block hashes, PoW
    /// verification, and chain history.
    ///
    /// Field offsets verified:
    ///   target (u32 LE): bytes 33..37
    ///   nonce (u32 LE):   bytes 39..43
    ///   total_reward (u64 LE): bytes 123..131
    #[test]
    fn test_mining_blob_newtype_stability() {
        let header = BlockHeader {
            version: BlockVersion::CURRENT,
            previous: blake3::hash(b"sentinel_parent"),
            merkle_root: blake3::hash(b"sentinel_txs"),
            timestamp: BlockTimestamp::new(1_700_000_000),
            target: BlockTarget::new(0x1A2B_3C4D),
            nonce: 42,
            height: BlockHeight::new(5),
            uncle_merkle_root: [0x11; 32],
            total_reward: BlockReward::new(100_000_000),
            randomx_key: [0x22; 32],
            miner: [0x22; 32],
            commitment_merkle_root: [0x33; 32],
            nullifier_root: [0x44; 32],
            anchor_tx_id: [0u8; 32],
            anchor_monero_height: MoneroBlockHeight::new(0),
            anchor_monero_hash: [0u8; 32],
            finality_flags: 0,
            fee_window_flags: FeeWindowFlags::default(),
            pow_source: PowSource::Native,
            anchor_owner: [0u8; 32],
            caribina_anchor: None,
        };

        let blob = header.to_mining_blob();
        // The newtypes' byte positions are what this test is about; the length moved 260 -> 292
        // for `anchor_owner`, which is a deliberate consensus-format change, not a newtype slip.
        assert_eq!(blob.len(), 292, "Mining blob length changed — would fork the chain");

        // Verify specific byte offsets for the fields being migrated to newtypes.
        // These MUST remain byte-identical after BlockTarget and BlockReward are
        // introduced. The newtype .get()/.to_le_bytes() must produce identical bytes.

        // target (u32 LE at offset 33..37)
        let target_bytes: [u8; 4] = blob[33..37].try_into().unwrap();
        assert_eq!(
            u32::from_le_bytes(target_bytes), 0x1A2B_3C4D,
            "target LE bytes at offset 33..37 — newtype must produce identical bytes"
        );

        // nonce (u32 LE at offset 39..43)
        let nonce_bytes: [u8; 4] = blob[39..43].try_into().unwrap();
        assert_eq!(
            u32::from_le_bytes(nonce_bytes), 42,
            "nonce at offset 39..43"
        );

        // total_reward (u64 LE at offset 123..131)
        let reward_bytes: [u8; 8] = blob[123..131].try_into().unwrap();
        assert_eq!(
            u64::from_le_bytes(reward_bytes), 100_000_000,
            "total_reward LE bytes at offset 123..131 — newtype must produce identical bytes"
        );
    }

    /// Sentinel: BlockHeader JSON serde shape stability (Change 4 prerequisite).
    ///
    /// Verifies that `target` and `total_reward` serialize as bare JSON numbers,
    /// NOT as tuple-struct wrappers. After the migration to BlockTarget(u32) and
    /// BlockReward(u64), the JSON shape MUST remain:
    ///
    ///   {"target": 439041101, "total_reward": 100000000, "height": 5, ...}
    ///
    /// and NOT:
    ///
    ///   {"target": {"0": 439041101}, "total_reward": {"0": 100000000}, ...}
    ///
    /// A serde shape change would make ALL sled-stored blocks unreadable.
    #[test]
    fn test_block_header_newtype_serde_shape() {
        let header = BlockHeader {
            version: BlockVersion::CURRENT,
            previous: blake3::hash(b"sentinel_parent"),
            merkle_root: blake3::hash(b"sentinel_txs"),
            timestamp: BlockTimestamp::new(1_700_000_000),
            target: BlockTarget::new(0x1A2B_3C4D), // 439,041,101 decimal
            nonce: 42,
            height: BlockHeight::new(5),
            uncle_merkle_root: [0x11; 32],
            total_reward: BlockReward::new(100_000_000),
            randomx_key: [0x22; 32],
            miner: [0x22; 32],
            commitment_merkle_root: [0x33; 32],
            nullifier_root: [0x44; 32],
            anchor_tx_id: [0u8; 32],
            anchor_monero_height: MoneroBlockHeight::new(0),
            anchor_monero_hash: [0u8; 32],
            finality_flags: 0,
            fee_window_flags: FeeWindowFlags::default(),
            pow_source: PowSource::Native,
            anchor_owner: [0u8; 32],
            caribina_anchor: None,
        };

        let json = serde_json::to_string(&header)
            .expect("BlockHeader serialization must succeed");

        // Parse to check shape — target and total_reward must be bare numbers
        let parsed: serde_json::Value = serde_json::from_str(&json)
            .expect("JSON must be valid");

        // target must be a bare number, NOT an object/tuple-struct
        let target_val = &parsed["target"];
        assert!(
            target_val.is_number(),
            "target must serialize as bare JSON number, got: {}",
            target_val
        );
        assert_eq!(target_val.as_u64().unwrap(), 439_041_101);

        // total_reward must be a bare number
        let reward_val = &parsed["total_reward"];
        assert!(
            reward_val.is_number(),
            "total_reward must serialize as bare JSON number, got: {}",
            reward_val
        );
        assert_eq!(reward_val.as_u64().unwrap(), 100_000_000);

        // height must be a bare number (BlockHeight baseline)
        let height_val = &parsed["height"];
        assert!(
            height_val.is_number(),
            "height must serialize as bare JSON number (BlockHeight pattern), got: {}",
            height_val
        );
        assert_eq!(height_val.as_u64().unwrap(), 5);

        // Full round-trip: deserialize back and verify field equality
        let deserialized: BlockHeader = serde_json::from_str(&json)
            .expect("Deserialization must succeed");
        assert_eq!(deserialized.target, header.target);
        assert_eq!(deserialized.total_reward, header.total_reward);
        assert_eq!(deserialized.height, header.height);
        assert_eq!(deserialized.nonce, header.nonce);
        assert_eq!(deserialized.timestamp, header.timestamp);
    }
}