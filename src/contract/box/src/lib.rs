pub mod capability;
pub mod entrypoint;
pub mod error;
pub mod model;

use dwow_sdk::define_contract_function;

define_contract_function!(BoxFunction {
    Initialize = 0x00,
    Put = 0x01,
    Take = 0x02,
});

pub const BOX_CONTRACT_DB_VERSION: &str = "box_version_1";
pub const BOX_CONTRACT_NULLIFIERS_TREE: &str = "nullifiers";
pub const BOX_CONTRACT_INFO_TREE: &str = "info";

pub const BOX_CONTRACT_ZKAS_PUT_NS: &str = "Put";
pub const BOX_CONTRACT_ZKAS_TAKE_NS: &str = "Take";

// Merkle tree infrastructure
pub const BOX_CONTRACT_BOX_ROOTS_TREE: &str = "box_roots";
pub const BOX_CONTRACT_BOX_MERKLE_TREE: &[u8] = b"box_merkle_tree";
pub const BOX_CONTRACT_LATEST_BOX_ROOT: &[u8] = b"latest_box_root";

// Precalculated root for MerkleTree::new(1) with single ZERO leaf.
pub const EMPTY_BOX_TREE_ROOT: [u8; 32] = [
    0xb8, 0xc1, 0x07, 0x5a, 0x80, 0xa8, 0x09, 0x65, 0xc2, 0x39, 0x8f, 0x71, 0x1f, 0xe7, 0x3e, 0x05,
    0xb4, 0xed, 0xae, 0xde, 0xf1, 0x62, 0xf2, 0x61, 0xd4, 0xee, 0xd7, 0xcd, 0x72, 0x74, 0x8d, 0x17,
];
