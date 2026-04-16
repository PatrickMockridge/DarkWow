/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

//! DarkFi Game Room Contract
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
//! ```
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

pub mod error;
pub mod model;

use darkfi_sdk::{error::ContractResult, msg, wasm};

pub use error::GameRoomError;

/// Database tree names
pub const GAME_ROOM_ROOMS_TREE: &str = "game_room_rooms";
pub const GAME_ROOM_ACCOUNTS_TREE: &str = "game_room_accounts";
pub const GAME_ROOM_POTS_TREE: &str = "game_room_pots";
pub const GAME_ROOM_BETS_TREE: &str = "game_room_bets";
pub const GAME_ROOM_NULLIFIERS_TREE: &str = "game_room_nullifiers";
pub const GAME_ROOM_ENTROPY_TREE: &str = "game_room_entropy";

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

darkfi_sdk::define_contract!(
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
pub fn init_contract(cid: darkfi_sdk::crypto::ContractId, _ix: &[u8]) -> ContractResult {
    msg!("[game_room::init_contract] Initializing game room contract");

    wasm::db::db_init(cid, GAME_ROOM_ROOMS_TREE)?;
    wasm::db::db_init(cid, GAME_ROOM_ACCOUNTS_TREE)?;
    wasm::db::db_init(cid, GAME_ROOM_POTS_TREE)?;
    wasm::db::db_init(cid, GAME_ROOM_BETS_TREE)?;
    wasm::db::db_init(cid, GAME_ROOM_NULLIFIERS_TREE)?;
    wasm::db::db_init(cid, GAME_ROOM_ENTROPY_TREE)?;

    msg!("[game_room::init_contract] Game room contract initialized successfully");
    Ok(())
}

// ============================================================================
// METADATA (placeholder for future ZK proof integration)
// ============================================================================

fn get_metadata(_cid: darkfi_sdk::crypto::ContractId, _ix: &[u8]) -> ContractResult {
    // Placeholder - ZK proof integration deferred
    Ok(())
}

// ============================================================================
// INSTRUCTION PROCESSING
// ============================================================================

fn process_instruction(
    cid: darkfi_sdk::crypto::ContractId,
    ix: &[u8],
) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<darkfi_sdk::dark_tree::DarkLeaf<darkfi_sdk::ContractCall>> =
        darkfi_serial::deserialize(ix)?;
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
            entrypoint::entropy::game_room_contribute_entropy_process_instruction_v1(cid, call_idx, calls)?
        }
        GameRoomFunction::ClaimV1 => entrypoint::claim::game_room_claim_process_instruction_v1(cid, call_idx, calls)?,
    };

    wasm::util::set_return_data(&update_data)
}

// ============================================================================
// STATE UPDATE
// ============================================================================

fn process_update(cid: darkfi_sdk::crypto::ContractId, update_data: &[u8]) -> ContractResult {
    match GameRoomFunction::try_from(update_data[0])? {
        GameRoomFunction::CreateRoomV1 => {
            let update: model::CreateRoomUpdateV1 =
                darkfi_serial::deserialize(&update_data[1..])?;
            Ok(entrypoint::create_room::game_room_create_process_update_v1(cid, update)?)
        }
        GameRoomFunction::DepositV1 => {
            let update: model::DepositUpdateV1 =
                darkfi_serial::deserialize(&update_data[1..])?;
            Ok(entrypoint::deposit::game_room_deposit_process_update_v1(cid, update)?)
        }
        GameRoomFunction::WithdrawV1 => {
            let update: model::WithdrawUpdateV1 =
                darkfi_serial::deserialize(&update_data[1..])?;
            Ok(entrypoint::withdraw::game_room_withdraw_process_update_v1(cid, update)?)
        }
        GameRoomFunction::PlaceBetV1 => {
            let update: model::PlaceBetUpdateV1 =
                darkfi_serial::deserialize(&update_data[1..])?;
            Ok(entrypoint::place_bet::game_room_place_bet_process_update_v1(cid, update)?)
        }
        GameRoomFunction::RaiseV1 => {
            let update: model::RaiseUpdateV1 = darkfi_serial::deserialize(&update_data[1..])?;
            Ok(entrypoint::raise::game_room_raise_process_update_v1(cid, update)?)
        }
        GameRoomFunction::CallV1 => {
            let update: model::CallUpdateV1 = darkfi_serial::deserialize(&update_data[1..])?;
            Ok(entrypoint::call::game_room_call_process_update_v1(cid, update)?)
        }
        GameRoomFunction::FoldV1 => {
            let update: model::FoldUpdateV1 = darkfi_serial::deserialize(&update_data[1..])?;
            Ok(entrypoint::fold::game_room_fold_process_update_v1(cid, update)?)
        }
        GameRoomFunction::ClosePotV1 => {
            let update: model::ClosePotUpdateV1 =
                darkfi_serial::deserialize(&update_data[1..])?;
            Ok(entrypoint::close_pot::game_room_close_pot_process_update_v1(cid, update)?)
        }
        GameRoomFunction::SettlePotV1 => {
            let update: model::SettlePotUpdateV1 =
                darkfi_serial::deserialize(&update_data[1..])?;
            Ok(entrypoint::settle_pot::game_room_settle_pot_process_update_v1(cid, update)?)
        }
        GameRoomFunction::ContributeEntropyV1 => {
            let update: model::ContributeEntropyUpdateV1 =
                darkfi_serial::deserialize(&update_data[1..])?;
            Ok(entrypoint::entropy::game_room_contribute_entropy_process_update_v1(cid, update)?)
        }
        GameRoomFunction::ClaimV1 => {
            let update: model::ClaimUpdateV1 = darkfi_serial::deserialize(&update_data[1..])?;
            Ok(entrypoint::claim::game_room_claim_process_update_v1(cid, update)?)
        }
    }
}