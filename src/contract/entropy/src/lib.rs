pub mod entrypoint;
pub mod error;
pub mod model;

use dwow_sdk::define_contract_function;

define_contract_function!(EntropyFunction {
    Initialize = 0x00,
    CommitEntropy = 0x01,
    RevealEntropy = 0x02,
});

pub const ENTROPY_CONTRACT_INFO_TREE: &str = "info";
pub const ENTROPY_CONTRACT_NULLIFIERS_TREE: &str = "nullifiers";

pub const ENTROPY_CONTRACT_ZKAS_COMMIT_NS_V1: &str = "CommitEntropyV1";
pub const ENTROPY_CONTRACT_ZKAS_REVEAL_NS_V1: &str = "RevealEntropyV1";
