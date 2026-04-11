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

use darkfi_sdk::error::ContractError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GameRoomError {
    InvalidFunction,
    RoomNotFound,
    RoomNotOpen,
    RoomNotActive,
    RoomConcluded,
    PotNotFound,
    PotNotOpen,
    PotClosed,
    PotSettled,
    AccountNotFound,
    InsufficientBalance,
    InsufficientLocked,
    StakeBelowMin,
    StakeAboveMax,
    BetNotFound,
    AlreadyBet,
    NotCurrentBet,
    InvalidBetType,
    InvalidAmount,
    CallerNotPlayer,
    CallerNotOwner,
    AlreadyClaimed,
    NotWinner,
    NullifierExists,
    EntropyNotContributed,
    EntropyDeadlinePassed,
    EntropyRevealMismatch,
    UnauthorizedCaller,
    InvalidChildrenIndexes,
    InvalidChildCall,
    IoError(String),
}

impl From<GameRoomError> for ContractError {
    fn from(e: GameRoomError) -> Self {
        match e {
            GameRoomError::InvalidFunction => ContractError::InvalidFunction,
            GameRoomError::RoomNotFound => ContractError::InvalidFunction,
            GameRoomError::RoomNotOpen => ContractError::InvalidFunction,
            GameRoomError::RoomNotActive => ContractError::InvalidFunction,
            GameRoomError::RoomConcluded => ContractError::InvalidFunction,
            GameRoomError::PotNotFound => ContractError::InvalidFunction,
            GameRoomError::PotNotOpen => ContractError::InvalidFunction,
            GameRoomError::PotClosed => ContractError::InvalidFunction,
            GameRoomError::PotSettled => ContractError::InvalidFunction,
            GameRoomError::AccountNotFound => ContractError::InvalidFunction,
            GameRoomError::InsufficientBalance => ContractError::InvalidFunction,
            GameRoomError::InsufficientLocked => ContractError::InvalidFunction,
            GameRoomError::StakeBelowMin => ContractError::InvalidFunction,
            GameRoomError::StakeAboveMax => ContractError::InvalidFunction,
            GameRoomError::BetNotFound => ContractError::InvalidFunction,
            GameRoomError::AlreadyBet => ContractError::InvalidFunction,
            GameRoomError::NotCurrentBet => ContractError::InvalidFunction,
            GameRoomError::InvalidBetType => ContractError::InvalidFunction,
            GameRoomError::InvalidAmount => ContractError::InvalidFunction,
            GameRoomError::CallerNotPlayer => ContractError::InvalidFunction,
            GameRoomError::CallerNotOwner => ContractError::InvalidFunction,
            GameRoomError::AlreadyClaimed => ContractError::InvalidFunction,
            GameRoomError::NotWinner => ContractError::InvalidFunction,
            GameRoomError::NullifierExists => ContractError::InvalidFunction,
            GameRoomError::EntropyNotContributed => ContractError::InvalidFunction,
            GameRoomError::EntropyDeadlinePassed => ContractError::InvalidFunction,
            GameRoomError::EntropyRevealMismatch => ContractError::InvalidFunction,
            GameRoomError::UnauthorizedCaller => ContractError::InvalidFunction,
            GameRoomError::InvalidChildrenIndexes => ContractError::InvalidFunction,
            GameRoomError::InvalidChildCall => ContractError::InvalidFunction,
            GameRoomError::IoError(_) => ContractError::InvalidFunction,
        }
    }
}