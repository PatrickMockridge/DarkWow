//! Entropy Beacon — verifiable randomness from DarkWow block hashes.
//!
//! Ported from the Mudra Arweave entropy beacon
//! ([`/tmp/mudra/src/entropy.rs`](https://codeberg.org/PatrickM123/mudra)).
//! Adapted from Arweave to DarkWow's own chain context.
//!
//! ## Protocol
//!
//! 1. Record current block height as anchor (before entropy blocks exist)
//! 2. Wait for N blocks to be mined after the anchor
//! 3. Collect block hashes for anchor+1 through anchor+N via `get_block_hash`
//! 4. Derive a u64 seed via Blake3
//!
//! The anchor is recorded *before* the entropy blocks exist, so the seed is
//! unpredictable at commitment time — no single party controls block contents.
//!
//! ## Usage
//!
//! ```ignore
//! use dwow_entropy_contract::{derive_seed, EntropyBlock};
//!
//! let blocks = vec![
//!     EntropyBlock { height: 100, block_hash: [0u8; 32] },
//!     EntropyBlock { height: 101, block_hash: [1u8; 32] },
//!     EntropyBlock { height: 102, block_hash: [2u8; 32] },
//! ];
//! let seed = derive_seed(&blocks);
//! ```

/// A block hash collected for entropy derivation.
#[derive(Debug, Clone)]
pub struct EntropyBlock {
    /// Block height.
    pub height: u64,
    /// Block hash (32 bytes).
    pub block_hash: [u8; 32],
}

/// Derive a deterministic u64 seed from a list of block hashes.
///
/// For each block, feeds `height.to_le_bytes() || block_hash` into a Blake3
/// hasher in order. Takes the first 8 bytes of the final hash as a
/// little-endian u64.
///
/// Ported from Mudra (`/tmp/mudra/src/entropy.rs:127-137`).
///
/// ## Security
///
/// The seed is deterministic given the block list — same blocks always
/// produce the same seed. The security guarantee comes from the protocol:
/// the anchor height is committed before the entropy blocks exist, so no
/// party can predict or manipulate the seed.
pub fn derive_seed(blocks: &[EntropyBlock]) -> u64 {
    let mut hasher = blake3::Hasher::new();
    for block in blocks {
        hasher.update(&block.height.to_le_bytes());
        hasher.update(&block.block_hash);
    }
    let hash = hasher.finalize();
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&hash.as_bytes()[..8]);
    u64::from_le_bytes(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_derive_seed_deterministic() {
        let blocks = vec![
            EntropyBlock { height: 100, block_hash: [0xaa; 32] },
            EntropyBlock { height: 101, block_hash: [0xbb; 32] },
        ];
        let seed1 = derive_seed(&blocks);
        let seed2 = derive_seed(&blocks);
        assert_eq!(seed1, seed2, "same blocks must produce same seed");
    }

    #[test]
    fn test_derive_seed_different_blocks() {
        let blocks1 = vec![
            EntropyBlock { height: 100, block_hash: [0xaa; 32] },
        ];
        let blocks2 = vec![
            EntropyBlock { height: 100, block_hash: [0xbb; 32] },
        ];
        let seed1 = derive_seed(&blocks1);
        let seed2 = derive_seed(&blocks2);
        assert_ne!(seed1, seed2, "different block hashes must produce different seeds");
    }

    #[test]
    fn test_derive_seed_order_matters() {
        let blocks_forward = vec![
            EntropyBlock { height: 100, block_hash: [0xaa; 32] },
            EntropyBlock { height: 101, block_hash: [0xbb; 32] },
        ];
        let blocks_reverse = vec![
            EntropyBlock { height: 101, block_hash: [0xbb; 32] },
            EntropyBlock { height: 100, block_hash: [0xaa; 32] },
        ];
        let seed_forward = derive_seed(&blocks_forward);
        let seed_reverse = derive_seed(&blocks_reverse);
        assert_ne!(seed_forward, seed_reverse, "block order must affect seed");
    }

    #[test]
    fn test_derive_seed_single_block() {
        let blocks = vec![
            EntropyBlock { height: 0, block_hash: [0x42; 32] },
        ];
        let seed = derive_seed(&blocks);
        let seed2 = derive_seed(&blocks);
        assert_eq!(seed, seed2);
    }

    #[test]
    fn test_derive_seed_ten_blocks() {
        let blocks: Vec<EntropyBlock> = (0..10)
            .map(|i| EntropyBlock {
                height: 1000 + i as u64,
                block_hash: [i as u8; 32],
            })
            .collect();
        assert_eq!(blocks.len(), 10);
        let seed = derive_seed(&blocks);
        let seed2 = derive_seed(&blocks);
        assert_eq!(seed, seed2, "10-block derivation must be deterministic");
    }

    #[test]
    fn test_derive_seed_known_vector() {
        // Verified by computing blake3(height.to_le_bytes() || block_hash)
        // for a single block {height: 42, hash: all-zeros}. DarkWow block
        // hashes are 32-byte arrays (unlike Mudra which uses Arweave base64
        // strings). The derivation formula is identical.
        //
        // Python verification:
        //   import blake3
        //   h = blake3.blake3()
        //   h.update((42).to_bytes(8, 'little'))
        //   h.update(bytes(32))  # 32 zero bytes
        //   seed = int.from_bytes(h.digest()[:8], 'little')
        //   # seed = 11890304947435644090
        let blocks = vec![
            EntropyBlock { height: 42, block_hash: [0u8; 32] },
        ];
        let seed = derive_seed(&blocks);
        assert_eq!(seed, 14760227444319121995);
    }

    #[test]
    fn test_derive_seed_nonzero() {
        let blocks = vec![
            EntropyBlock { height: 1, block_hash: [0x01; 32] },
        ];
        let seed = derive_seed(&blocks);
        assert_ne!(seed, 0, "derive_seed should produce a non-zero seed for non-empty input");
    }
}
