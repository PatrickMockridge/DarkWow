//! ZK circuit binary embeddings — compiled only when `feature = "client"` is enabled.

pub const GAME_ROOM_ZKAS_CLAIM_V1_BIN: &[u8] =
    include_bytes!("../../proof/claim_v1.zk.bin");
pub const GAME_ROOM_ZKAS_CREATE_ROOM_V1_BIN: &[u8] =
    include_bytes!("../../proof/create_room_v1.zk.bin");
pub const GAME_ROOM_ZKAS_DEPOSIT_V1_BIN: &[u8] =
    include_bytes!("../../proof/deposit_v1.zk.bin");
pub const GAME_ROOM_ZKAS_PLACE_BET_V1_BIN: &[u8] =
    include_bytes!("../../proof/place_bet_v1.zk.bin");
pub const GAME_ROOM_ZKAS_SETTLE_POT_V1_BIN: &[u8] =
    include_bytes!("../../proof/settle_pot_v1.zk.bin");
