pub mod capability;
pub mod entrypoint;
pub mod error;
pub mod model;

use dwow_sdk::define_contract_function;

define_contract_function!(BoxFunction {
    InitializeV1 = 0x00,
    PutV1 = 0x01,
    TakeV1 = 0x02,
});

pub const BOX_CONTRACT_DB_VERSION: &str = "box_version_1";
pub const BOX_CONTRACT_BOXES_TREE: &str = "boxes";
pub const BOX_CONTRACT_NULLIFIERS_TREE: &str = "nullifiers";
pub const BOX_CONTRACT_INFO_TREE: &str = "info";
pub const BOX_CONTRACT_ZKAS_PUT_NS_V1: &str = "PutV1";
pub const BOX_CONTRACT_ZKAS_TAKE_NS_V1: &str = "TakeV1";
