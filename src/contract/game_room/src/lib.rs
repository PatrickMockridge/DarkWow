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

//! DarkWow Game Room Contract
//!
//! A generalized betting and pot management contract. This contract provides:
//! - Stake management (bring cash in/out)
//! - Pot management (bets, raises, calls, collective pot)
//! - On-chain proof of bets for external verification
//! - Trusted entropy setup (optional)
//!
//! Game logic, win conditions, and dispute resolution are handled at the
//! app layer by the room owner (escrow-DAO).
//!
//! ## Two-Layer Design
//!
//! ```text
//! App Layer: Game rules, win conditions, dispute resolution (room owner DAO)
//!     ↓ SDK calls
//! Contract Layer: Stake management, pot operations, entropy config
//! ```
//!
//! ## Usage
//!
//! 1. Room owner creates a room via CreateRoomV1
//! 2. Players deposit stake via DepositV1
//! 3. Players place bets via PlaceBetV1, RaiseV1, CallV1
//! 4. Owner DAO settles pot via SettlePotV1
//! 5. Players claim winnings via ClaimV1

pub mod capability;
pub mod error;
pub mod model;

use dwow_sdk::crypto::poseidon_hash;
use dwow_sdk::{
    error::{ContractError, ContractResult},
    msg, pasta, wasm,
};
use dwow_serial::Encodable;

pub use error::GameRoomError;

/// Database tree names
pub const GAME_ROOM_CONTRACT_INFO_TREE: &str = "info";
pub const GAME_ROOM_ROOMS_TREE: &str = "game_room_rooms";
pub const GAME_ROOM_ACCOUNTS_TREE: &str = "game_room_accounts";
pub const GAME_ROOM_POTS_TREE: &str = "game_room_pots";
pub const GAME_ROOM_BETS_TREE: &str = "game_room_bets";
pub const GAME_ROOM_NULLIFIERS_TREE: &str = "game_room_nullifiers";
pub const GAME_ROOM_ENTROPY_TREE: &str = "game_room_entropy";

/// Promissory Note contract ID for cross-contract routing validation
pub const PROMISSORY_NOTE_CONTRACT_ID_KEY: &[u8] = b"promissory_note_cid";

/// Game Room contract functions
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum GameRoomFunction {
    CreateRoomV1 = 0x00,
    DepositV1 = 0x01,
    WithdrawV1 = 0x02,
    PlaceBetV1 = 0x03,
    RaiseV1 = 0x04,
    CallV1 = 0x05,
    FoldV1 = 0x06,
    ClosePotV1 = 0x07,
    SettlePotV1 = 0x08,
    ContributeEntropyV1 = 0x09,
    ClaimV1 = 0x0A,
}

// =============================================================================
// ZK CIRCUIT NAMESPACES
// =============================================================================

/// ZK namespace for CreateRoom circuit
pub const GAME_ROOM_ZKAS_CREATE_ROOM_NS: &str = "CreateRoom";
/// ZK namespace for Deposit circuit
pub const GAME_ROOM_ZKAS_DEPOSIT_NS: &str = "Deposit";
/// ZK namespace for PlaceBet circuit
pub const GAME_ROOM_ZKAS_PLACE_BET_NS: &str = "PlaceBet";
/// ZK namespace for SettlePot circuit
pub const GAME_ROOM_ZKAS_SETTLE_POT_NS: &str = "SettlePot";
/// ZK namespace for Claim circuit
pub const GAME_ROOM_ZKAS_CLAIM_NS: &str = "Claim";

// Legacy V1 circuit namespaces (kept for harness compatibility)
pub const GAME_ROOM_ZKAS_CREATE_ROOM_NS_V1: &str = "CreateRoom";
pub const GAME_ROOM_ZKAS_DEPOSIT_NS_V1: &str = "Deposit";
pub const GAME_ROOM_ZKAS_PLACE_BET_NS_V1: &str = "PlaceBet";
pub const GAME_ROOM_ZKAS_SETTLE_POT_NS_V1: &str = "SettlePot";
// V2 circuit namespaces (HAZOP RC3: domain separation — active in get_metadata)
pub const GAME_ROOM_ZKAS_CREATE_ROOM_NS_V2: &str = "CreateRoomV2";
pub const GAME_ROOM_ZKAS_DEPOSIT_NS_V2: &str = "DepositV2";
pub const GAME_ROOM_ZKAS_PLACE_BET_NS_V2: &str = "PlaceBetV2";
pub const GAME_ROOM_ZKAS_SETTLE_POT_NS_V2: &str = "SettlePotV2";
pub const GAME_ROOM_ZKAS_CLAIM_NS_V2: &str = "ClaimV2";
pub const GAME_ROOM_ZKAS_CALL_NS_V2: &str = "CallV2";
pub const GAME_ROOM_ZKAS_CLOSE_POT_NS_V2: &str = "ClosePotV2";
pub const GAME_ROOM_ZKAS_CONTRIBUTE_ENTROPY_NS_V2: &str = "ContributeEntropyV2";
pub const GAME_ROOM_ZKAS_FOLD_NS_V2: &str = "FoldV2";
pub const GAME_ROOM_ZKAS_RAISE_NS_V2: &str = "RaiseV2";
pub const GAME_ROOM_ZKAS_WITHDRAW_NS_V2: &str = "WithdrawV2";

impl TryFrom<u8> for GameRoomFunction {
    type Error = GameRoomError;

    fn try_from(v: u8) -> Result<Self, Self::Error> {
        match v {
            0x00 => Ok(Self::CreateRoomV1),
            0x01 => Ok(Self::DepositV1),
            0x02 => Ok(Self::WithdrawV1),
            0x03 => Ok(Self::PlaceBetV1),
            0x04 => Ok(Self::RaiseV1),
            0x05 => Ok(Self::CallV1),
            0x06 => Ok(Self::FoldV1),
            0x07 => Ok(Self::ClosePotV1),
            0x08 => Ok(Self::SettlePotV1),
            0x09 => Ok(Self::ContributeEntropyV1),
            0x0A => Ok(Self::ClaimV1),
            _ => Err(GameRoomError::InvalidFunction),
        }
    }
}

// ============================================================================
// ENTRYPOINT SUBMODULES
// ============================================================================

pub mod entrypoint;

#[cfg(feature = "client")]
pub mod client;

// ============================================================================
// CONTRACT DEFINITION
// ============================================================================

dwow_sdk::define_contract!(
    init: init_contract,
    exec: process_instruction,
    apply: process_update,
    metadata: get_metadata
);

// ============================================================================
// INITIALIZATION
// ============================================================================

/// Initialize game room contract state
///
/// Sets up database trees for:
/// - Rooms (game room configurations)
/// - Accounts (player balances and locked stakes)
/// - Pots (collective betting pools)
/// - Bets (individual bet records)
/// - Nullifiers (prevent double-actions)
/// - Entropy (trusted setup contributions)
pub fn init_contract(cid: dwow_sdk::crypto::ContractId, _ix: &[u8]) -> ContractResult {
    msg!("[game_room::init_contract] Initializing game room contract");

    let info_db = wasm::db::db_init(cid, GAME_ROOM_CONTRACT_INFO_TREE)?;
    wasm::db::db_set(info_db, PROMISSORY_NOTE_CONTRACT_ID_KEY, &dwow_sdk::crypto::PROMISSORY_NOTE_CONTRACT_ID.to_bytes())?;

    wasm::db::db_init(cid, GAME_ROOM_ROOMS_TREE)?;
    wasm::db::db_init(cid, GAME_ROOM_ACCOUNTS_TREE)?;
    wasm::db::db_init(cid, GAME_ROOM_POTS_TREE)?;
    wasm::db::db_init(cid, GAME_ROOM_BETS_TREE)?;
    wasm::db::db_init(cid, GAME_ROOM_NULLIFIERS_TREE)?;
    wasm::db::db_init(cid, GAME_ROOM_ENTROPY_TREE)?;

    msg!("[game_room::init_contract] Game room contract initialized successfully");


    // V2 circuits (HAZOP RC3: domain separation)
    let call_v2_bincode = include_bytes!("../proof/call.zk.bin");
    wasm::db::zkas_db_set(&call_v2_bincode[..])?;
    let claim_v2_bincode = include_bytes!("../proof/claim.zk.bin");
    wasm::db::zkas_db_set(&claim_v2_bincode[..])?;
    let close_pot_v2_bincode = include_bytes!("../proof/close_pot.zk.bin");
    wasm::db::zkas_db_set(&close_pot_v2_bincode[..])?;
    let contribute_entropy_v2_bincode = include_bytes!("../proof/contribute_entropy.zk.bin");
    wasm::db::zkas_db_set(&contribute_entropy_v2_bincode[..])?;
    let create_room_v2_bincode = include_bytes!("../proof/create_room.zk.bin");
    wasm::db::zkas_db_set(&create_room_v2_bincode[..])?;
    let deposit_v2_bincode = include_bytes!("../proof/deposit.zk.bin");
    wasm::db::zkas_db_set(&deposit_v2_bincode[..])?;
    let fold_v2_bincode = include_bytes!("../proof/fold.zk.bin");
    wasm::db::zkas_db_set(&fold_v2_bincode[..])?;
    let place_bet_v2_bincode = include_bytes!("../proof/place_bet.zk.bin");
    wasm::db::zkas_db_set(&place_bet_v2_bincode[..])?;
    let raise_v2_bincode = include_bytes!("../proof/raise.zk.bin");
    wasm::db::zkas_db_set(&raise_v2_bincode[..])?;
    let settle_pot_v2_bincode = include_bytes!("../proof/settle_pot.zk.bin");
    wasm::db::zkas_db_set(&settle_pot_v2_bincode[..])?;
    let withdraw_v2_bincode = include_bytes!("../proof/withdraw.zk.bin");
    wasm::db::zkas_db_set(&withdraw_v2_bincode[..])?;

    Ok(())
}

// ============================================================================
// METADATA (placeholder for future ZK proof integration)
// ============================================================================

fn get_metadata(_cid: dwow_sdk::crypto::ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<dwow_sdk::dark_tree::DarkLeaf<dwow_sdk::ContractCall>> =
        dwow_serial::deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = GameRoomFunction::try_from(self_.data[0])?;

    msg!("[game_room::get_metadata] Processing function: {:?}", func);

    let metadata = match func {
        GameRoomFunction::CreateRoomV1 => {
            let params = model::CreateRoomParamsV1::decode(&self_.data[1..])?;
            create_room_get_metadata_v1(params)?
        }
        GameRoomFunction::DepositV1 => {
            let params = model::DepositParamsV1::decode(&self_.data[1..])?;
            deposit_get_metadata_v1(params)?
        }
        GameRoomFunction::PlaceBetV1 => {
            let params = model::PlaceBetParamsV1::decode(&self_.data[1..])?;
            place_bet_get_metadata_v1(params)?
        }
        GameRoomFunction::SettlePotV1 => {
            let params = model::SettlePotParamsV1::decode(&self_.data[1..])?;
            settle_pot_get_metadata_v1(params)?
        }
        GameRoomFunction::ClaimV1 => {
            let params = model::ClaimParamsV1::decode(&self_.data[1..])?;
            claim_get_metadata_v1(params)?
        }
        // WithdrawV1, RaiseV1, CallV1, FoldV1, ClosePotV1, ContributeEntropyV1
        // — identity-proof circuits with [player_pub_x, player_pub_y, player_nullifier, tx_binding, tx_nonce]
        GameRoomFunction::WithdrawV1 => {
            let params = model::WithdrawParamsV1::decode(&self_.data[1..])?;
            identity_get_metadata_v1(params.room_id, &params.player, GAME_ROOM_ZKAS_WITHDRAW_NS_V2)?
        }
        GameRoomFunction::RaiseV1 => {
            let params = model::RaiseParamsV1::decode(&self_.data[1..])?;
            identity_get_metadata_v1(params.room_id, &params.player, GAME_ROOM_ZKAS_RAISE_NS_V2)?
        }
        GameRoomFunction::CallV1 => {
            let params = model::CallParamsV1::decode(&self_.data[1..])?;
            identity_get_metadata_v1(params.room_id, &params.player, GAME_ROOM_ZKAS_CALL_NS_V2)?
        }
        GameRoomFunction::FoldV1 => {
            let params = model::FoldParamsV1::decode(&self_.data[1..])?;
            identity_get_metadata_v1(params.room_id, &params.player, GAME_ROOM_ZKAS_FOLD_NS_V2)?
        }
        GameRoomFunction::ClosePotV1 => {
            close_pot_get_metadata_v1()?
        }
        GameRoomFunction::ContributeEntropyV1 => {
            let params = model::ContributeEntropyParamsV1::decode(&self_.data[1..])?;
            identity_get_metadata_v1(params.room_id, &params.player, GAME_ROOM_ZKAS_CONTRIBUTE_ENTROPY_NS_V2)?
        }
        _ => vec![],
    };

    wasm::util::set_return_data(&metadata)
}

/// `get_metadata` for CreateRoomV1 — CreateRoomV2 circuit
///
/// Circuit constrain_instance order: [tx_binding, tx_nonce, derived_room_id]
/// derived_room_id = poseidon_hash(DOMAIN_COIN_COMMIT, owner_pub_x, owner_pub_y, token_id, block_height, nonce)
fn create_room_get_metadata_v1(
    params: model::CreateRoomParamsV1,
) -> Result<Vec<u8>, ContractError> {
    let (ox, oy) = params.owner.xy().expect("pk not identity");
    // block_height is not in CreateRoomParamsV1; use zero placeholder
    let block_height = pasta::pallas::Base::zero();
    let derived_room_id = poseidon_hash([
        pasta::pallas::Base::from(4u64), // DOMAIN_COIN_COMMIT
        ox,
        oy,
        params.token_id,
        block_height,
        params.nonce,
    ]);

    let tx_binding = pasta::pallas::Base::zero(); // Pattern A
    let tx_nonce_val = pasta::pallas::Base::zero(); // Pattern A

    let mut zk_public_inputs: Vec<(String, Vec<pasta::pallas::Base>)> = vec![];
    zk_public_inputs.push((
        GAME_ROOM_ZKAS_CREATE_ROOM_NS_V2.to_string(),
        vec![tx_binding, tx_nonce_val, derived_room_id],
    ));

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    Ok(metadata)
}

/// `get_metadata` for DepositV1 — DepositV2 circuit
///
/// Circuit constrain_instance order: [derived_account_key, tx_binding, tx_nonce, derived_player_key]
/// derived_account_key = poseidon_hash(DOMAIN_COIN_COMMIT, room_id, player_pub_x)
/// derived_player_key = poseidon_hash(DOMAIN_COIN_COMMIT, player_pub_x, player_pub_y)
fn deposit_get_metadata_v1(
    params: model::DepositParamsV1,
) -> Result<Vec<u8>, ContractError> {
    let (px, py) = params.player.xy().expect("pk not identity");
    let derived_account_key = poseidon_hash([
        pasta::pallas::Base::from(4u64), // DOMAIN_COIN_COMMIT
        params.room_id,
        px,
    ]);
    let derived_player_key = poseidon_hash([
        pasta::pallas::Base::from(4u64), // DOMAIN_COIN_COMMIT
        px,
        py,
    ]);

    let tx_binding = pasta::pallas::Base::zero(); // Pattern A
    let tx_nonce_val = pasta::pallas::Base::zero(); // Pattern A

    let mut zk_public_inputs: Vec<(String, Vec<pasta::pallas::Base>)> = vec![];
    zk_public_inputs.push((
        GAME_ROOM_ZKAS_DEPOSIT_NS_V2.to_string(),
        vec![derived_account_key, tx_binding, tx_nonce_val, derived_player_key],
    ));

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    Ok(metadata)
}

/// `get_metadata` for PlaceBetV1 — PlaceBetV2 circuit
///
/// Circuit constrain_instance order: [derived_bet_id, tx_binding, tx_nonce, derived_commitment]
/// derived_bet_id = poseidon_hash(DOMAIN_COIN_COMMIT, pot_id, player_pub_x, amount, block_height)
/// derived_commitment = poseidon_hash(DOMAIN_COIN_COMMIT, amount, nonce, block_height)
fn place_bet_get_metadata_v1(
    params: model::PlaceBetParamsV1,
) -> Result<Vec<u8>, ContractError> {
    let (px, _py) = params.player.xy().expect("pk not identity");
    let derived_bet_id = poseidon_hash([
        pasta::pallas::Base::from(4u64), // DOMAIN_COIN_COMMIT
        params.pot_id,
        px,
        pasta::pallas::Base::from(params.amount),
        params.block_height,
    ]);
    let derived_commitment = poseidon_hash([
        pasta::pallas::Base::from(4u64), // DOMAIN_COIN_COMMIT
        pasta::pallas::Base::from(params.amount),
        params.nonce,
        params.block_height,
    ]);

    let tx_binding = pasta::pallas::Base::zero(); // Pattern A
    let tx_nonce_val = pasta::pallas::Base::zero(); // Pattern A

    let mut zk_public_inputs: Vec<(String, Vec<pasta::pallas::Base>)> = vec![];
    zk_public_inputs.push((
        GAME_ROOM_ZKAS_PLACE_BET_NS_V2.to_string(),
        vec![derived_bet_id, tx_binding, tx_nonce_val, derived_commitment],
    ));

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    Ok(metadata)
}

/// `get_metadata` for SettlePotV1 — SettlePotV2 circuit
///
/// Circuit constrain_instance order: [derived_room_id, tx_binding, tx_nonce, derived_pot_id]
/// derived_room_id = poseidon_hash(DOMAIN_COIN_COMMIT, house_pub_x, house_pub_y, nonce)
/// derived_pot_id = poseidon_hash(DOMAIN_COIN_COMMIT, room_id, pot_total, house_pub_x)
fn settle_pot_get_metadata_v1(
    params: model::SettlePotParamsV1,
) -> Result<Vec<u8>, ContractError> {
    let (cx, cy) = params.caller.xy().expect("pk not identity");
    let derived_room_id = poseidon_hash([
        pasta::pallas::Base::from(4u64), // DOMAIN_COIN_COMMIT
        cx,
        cy,
        params.nonce,
    ]);
    let derived_pot_id = poseidon_hash([
        pasta::pallas::Base::from(4u64), // DOMAIN_COIN_COMMIT
        params.room_id,
        pasta::pallas::Base::from(params.pot_total),
        cx,
    ]);

    let tx_binding = pasta::pallas::Base::zero(); // Pattern A
    let tx_nonce_val = pasta::pallas::Base::zero(); // Pattern A

    let mut zk_public_inputs: Vec<(String, Vec<pasta::pallas::Base>)> = vec![];
    zk_public_inputs.push((
        GAME_ROOM_ZKAS_SETTLE_POT_NS_V2.to_string(),
        vec![derived_room_id, tx_binding, tx_nonce_val, derived_pot_id],
    ));

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    Ok(metadata)
}

/// `get_metadata` for ClaimV1 — ClaimV2 circuit
///
/// Circuit constrain_instance order: [derived_claim_id, tx_binding, tx_nonce]
/// derived_claim_id = poseidon_hash(DOMAIN_COIN_COMMIT, pot_id, winner_pub_x, payout_amount, nonce)
/// derived_winner_key is circuit-computed but NOT constrained as instance
fn claim_get_metadata_v1(
    params: model::ClaimParamsV1,
) -> Result<Vec<u8>, ContractError> {
    let (wx, _wy) = params.winner.xy().expect("pk not identity");
    let derived_claim_id = poseidon_hash([
        pasta::pallas::Base::from(4u64), // DOMAIN_COIN_COMMIT
        params.pot_id,
        wx,
        pasta::pallas::Base::from(params.payout_amount),
        params.nonce,
    ]);

    let tx_binding = pasta::pallas::Base::zero(); // Pattern A
    let tx_nonce_val = pasta::pallas::Base::zero(); // Pattern A

    let mut zk_public_inputs: Vec<(String, Vec<pasta::pallas::Base>)> = vec![];
    zk_public_inputs.push((
        GAME_ROOM_ZKAS_CLAIM_NS_V2.to_string(),
        vec![derived_claim_id, tx_binding, tx_nonce_val],
    ));

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    Ok(metadata)
}

/// Identity-proof metadata helper for circuits with [player_pub_x, player_pub_y, player_nullifier, tx_binding, tx_nonce]
/// Used by: WithdrawV2, RaiseV2, CallV2, FoldV2, ContributeEntropyV2
fn identity_get_metadata_v1(
    _room_id: pasta::pallas::Base,
    player: &dwow_sdk::crypto::PublicKey,
    ns: &str,
) -> Result<Vec<u8>, ContractError> {
    let (px, py) = player.xy().expect("pk not identity");
    let player_nullifier = pasta::pallas::Base::zero(); // Pattern A placeholder
    let tx_binding = pasta::pallas::Base::zero(); // Pattern A
    let tx_nonce_val = pasta::pallas::Base::zero(); // Pattern A

    let zk_public_inputs: Vec<(String, Vec<pasta::pallas::Base>)> = vec![(
        ns.to_string(),
        vec![px, py, player_nullifier, tx_binding, tx_nonce_val],
    )];

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    Ok(metadata)
}

/// Metadata for ClosePotV1 — ClosePotV2 circuit (no player pubkey in params, zero-fill)
fn close_pot_get_metadata_v1() -> Result<Vec<u8>, ContractError> {
    let zk_public_inputs: Vec<(String, Vec<pasta::pallas::Base>)> = vec![(
        GAME_ROOM_ZKAS_CLOSE_POT_NS_V2.to_string(),
        vec![
            pasta::pallas::Base::zero(), // player_pub_x
            pasta::pallas::Base::zero(), // player_pub_y
            pasta::pallas::Base::zero(), // player_nullifier
            pasta::pallas::Base::zero(), // tx_binding
            pasta::pallas::Base::zero(), // tx_nonce
        ],
    )];

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    Ok(metadata)
}

// ============================================================================
// INSTRUCTION PROCESSING
// ============================================================================

fn process_instruction(
    cid: dwow_sdk::crypto::ContractId,
    ix: &[u8],
) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<dwow_sdk::dark_tree::DarkLeaf<dwow_sdk::ContractCall>> =
        dwow_serial::deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = GameRoomFunction::try_from(self_.data[0])?;

    let update_data = match func {
        GameRoomFunction::CreateRoomV1 => {
            entrypoint::create_room::game_room_create_process_instruction_v1(cid, call_idx, calls)?
        }
        GameRoomFunction::DepositV1 => {
            entrypoint::deposit::game_room_deposit_process_instruction_v1(cid, call_idx, calls)?
        }
        GameRoomFunction::WithdrawV1 => {
            entrypoint::withdraw::game_room_withdraw_process_instruction_v1(cid, call_idx, calls)?
        }
        GameRoomFunction::PlaceBetV1 => {
            entrypoint::place_bet::game_room_place_bet_process_instruction_v1(cid, call_idx, calls)?
        }
        GameRoomFunction::RaiseV1 => entrypoint::raise::game_room_raise_process_instruction_v1(cid, call_idx, calls)?,
        GameRoomFunction::CallV1 => entrypoint::call::game_room_call_process_instruction_v1(cid, call_idx, calls)?,
        GameRoomFunction::FoldV1 => entrypoint::fold::game_room_fold_process_instruction_v1(cid, call_idx, calls)?,
        GameRoomFunction::ClosePotV1 => {
            entrypoint::close_pot::game_room_close_pot_process_instruction_v1(cid, call_idx, calls)?
        }
        GameRoomFunction::SettlePotV1 => {
            entrypoint::settle_pot::game_room_settle_pot_process_instruction_v1(cid, call_idx, calls)?
        }
        GameRoomFunction::ContributeEntropyV1 => {
            return entrypoint::entropy::process_contribute_entropy_instruction(cid, call_idx, calls)
        }
        GameRoomFunction::ClaimV1 => entrypoint::claim::game_room_claim_process_instruction_v1(cid, call_idx, calls)?,
    };

    wasm::util::set_return_data(&update_data)
}

// ============================================================================
// STATE UPDATE
// ============================================================================

fn process_update(cid: dwow_sdk::crypto::ContractId, update_data: &[u8]) -> ContractResult {
    match GameRoomFunction::try_from(update_data[0])? {
        GameRoomFunction::CreateRoomV1 => {
            let update: model::CreateRoomUpdateV1 =
                model::CreateRoomUpdateV1::decode(&update_data[1..])?;
            Ok(entrypoint::create_room::game_room_create_process_update_v1(cid, update)?)
        }
        GameRoomFunction::DepositV1 => {
            let update: model::DepositUpdateV1 =
                model::DepositUpdateV1::decode(&update_data[1..])?;
            Ok(entrypoint::deposit::game_room_deposit_process_update_v1(cid, update)?)
        }
        GameRoomFunction::WithdrawV1 => {
            let update: model::WithdrawUpdateV1 =
                model::WithdrawUpdateV1::decode(&update_data[1..])?;
            Ok(entrypoint::withdraw::game_room_withdraw_process_update_v1(cid, update)?)
        }
        GameRoomFunction::PlaceBetV1 => {
            let update: model::PlaceBetUpdateV1 =
                model::PlaceBetUpdateV1::decode(&update_data[1..])?;
            Ok(entrypoint::place_bet::game_room_place_bet_process_update_v1(cid, update)?)
        }
        GameRoomFunction::RaiseV1 => {
            let update: model::RaiseUpdateV1 = model::RaiseUpdateV1::decode(&update_data[1..])?;
            Ok(entrypoint::raise::game_room_raise_process_update_v1(cid, update)?)
        }
        GameRoomFunction::CallV1 => {
            let update: model::CallUpdateV1 = model::CallUpdateV1::decode(&update_data[1..])?;
            Ok(entrypoint::call::game_room_call_process_update_v1(cid, update)?)
        }
        GameRoomFunction::FoldV1 => {
            let update: model::FoldUpdateV1 = model::FoldUpdateV1::decode(&update_data[1..])?;
            Ok(entrypoint::fold::game_room_fold_process_update_v1(cid, update)?)
        }
        GameRoomFunction::ClosePotV1 => {
            let update: model::ClosePotUpdateV1 =
                model::ClosePotUpdateV1::decode(&update_data[1..])?;
            Ok(entrypoint::close_pot::game_room_close_pot_process_update_v1(cid, update)?)
        }
        GameRoomFunction::SettlePotV1 => {
            let update: model::SettlePotUpdateV1 =
                model::SettlePotUpdateV1::decode(&update_data[1..])?;
            Ok(entrypoint::settle_pot::game_room_settle_pot_process_update_v1(cid, update)?)
        }
        GameRoomFunction::ContributeEntropyV1 => {
            let update: model::ContributeEntropyUpdateV1 =
                model::ContributeEntropyUpdateV1::decode(&update_data[1..])?;
            entrypoint::entropy::apply_contribute_entropy_update(cid, update)
        }
        GameRoomFunction::ClaimV1 => {
            let update: model::ClaimUpdateV1 = model::ClaimUpdateV1::decode(&update_data[1..])?;
            Ok(entrypoint::claim::game_room_claim_process_update_v1(cid, update)?)
        }
    }
}