pub mod entrypoint;
pub mod error;
pub mod model;

use dwow_sdk::define_contract_function;

define_contract_function!(MultiSigFunction {
    InitializeV1 = 0x00,
    CreateGroupV1 = 0x01,
    SignV1 = 0x02,
    FinalizeV1 = 0x03,
});

pub const MULTISIG_CONTRACT_DB_VERSION: &str = "multisig_version_1";
pub const MULTISIG_CONTRACT_GROUPS_TREE: &str = "groups";
pub const MULTISIG_CONTRACT_SIGNATURES_TREE: &str = "signatures";
pub const MULTISIG_CONTRACT_NULLIFIERS_TREE: &str = "nullifiers";
pub const MULTISIG_CONTRACT_INFO_TREE: &str = "info";
pub const MULTISIG_CONTRACT_ZKAS_CREATE_GROUP_NS_V1: &str = "CreateGroupV1";
pub const MULTISIG_CONTRACT_ZKAS_SIGN_NS_V1: &str = "SignV1";
pub const MULTISIG_CONTRACT_ZKAS_FINALIZE_NS_V1: &str = "FinalizeV1";
