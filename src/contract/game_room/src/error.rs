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
            GameRoomError::InvalidFunction => Self::InvalidFunction,
            GameRoomError::RoomNotFound => Self::Custom(1),
            GameRoomError::RoomNotOpen => Self::Custom(2),
            GameRoomError::RoomNotActive => Self::Custom(3),
            GameRoomError::RoomConcluded => Self::Custom(4),
            GameRoomError::PotNotFound => Self::Custom(5),
            GameRoomError::PotNotOpen => Self::Custom(6),
            GameRoomError::PotClosed => Self::Custom(7),
            GameRoomError::PotSettled => Self::Custom(8),
            GameRoomError::AccountNotFound => Self::Custom(9),
            GameRoomError::InsufficientBalance => Self::Custom(10),
            GameRoomError::InsufficientLocked => Self::Custom(11),
            GameRoomError::StakeBelowMin => Self::Custom(12),
            GameRoomError::StakeAboveMax => Self::Custom(13),
            GameRoomError::BetNotFound => Self::Custom(14),
            GameRoomError::AlreadyBet => Self::Custom(15),
            GameRoomError::NotCurrentBet => Self::Custom(16),
            GameRoomError::InvalidBetType => Self::Custom(17),
            GameRoomError::InvalidAmount => Self::Custom(18),
            GameRoomError::CallerNotPlayer => Self::Custom(19),
            GameRoomError::CallerNotOwner => Self::Custom(20),
            GameRoomError::AlreadyClaimed => Self::Custom(21),
            GameRoomError::NotWinner => Self::Custom(22),
            GameRoomError::NullifierExists => Self::Custom(23),
            GameRoomError::EntropyNotContributed => Self::Custom(24),
            GameRoomError::EntropyDeadlinePassed => Self::Custom(25),
            GameRoomError::EntropyRevealMismatch => Self::Custom(26),
            GameRoomError::UnauthorizedCaller => Self::Custom(27),
            GameRoomError::InvalidChildrenIndexes => Self::Custom(28),
            GameRoomError::InvalidChildCall => Self::Custom(29),
            GameRoomError::IoError(_) => Self::Custom(30),
        }
    }
}