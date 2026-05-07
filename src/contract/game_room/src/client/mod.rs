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

//! Game Room Contract Client API
//!
//! This module provides the client-side API for building Game Room contract calls.

use darkfi_sdk::{
    crypto::{poseidon_hash, PublicKey, SecretKey},
    pasta::pallas,
};

use crate::model::{
    BetType, CallParamsV1, ClaimParamsV1, ClosePotParamsV1, ContributeEntropyParamsV1,
    CreateRoomParamsV1, DepositParamsV1, EntropyMode, FoldParamsV1, PlaceBetParamsV1, PotId,
    RaiseParamsV1, RoomId, RoomState, SettlePotParamsV1, WithdrawParamsV1,
};

/// Client-side room note for tracking rooms
#[derive(Debug, Clone)]
pub struct RoomNote {
    pub room_id: RoomId,
    pub owner: PublicKey,
    pub state: RoomState,
    pub current_pot_id: Option<PotId>,
    pub token_id: pallas::Base,
}

/// Client-side deposit note
#[derive(Debug, Clone)]
pub struct DepositNote {
    pub room_id: RoomId,
    pub player: PublicKey,
    pub amount: u64,
    pub balance: u64,
}

/// Client-side bet note
#[derive(Debug, Clone)]
pub struct BetNote {
    pub bet_id: pallas::Base,
    pub room_id: RoomId,
    pub pot_id: PotId,
    pub player: PublicKey,
    pub amount: u64,
    pub bet_type: BetType,
}

/// Own bet with secret for entropy contribution
pub struct OwnBet {
    pub note: BetNote,
    pub secret: SecretKey,
}

/// Builder for creating room calls
pub struct CreateRoomV1Builder {
    owner: PublicKey,
    token_id: pallas::Base,
    min_stake: u64,
    max_stake: u64,
    entropy_mode: EntropyMode,
    confirmation_depth: u8,
    required_entropy_contributions: u8,
    entropy_contribution_deadline: u64,
    max_players: u8,
    nonce: pallas::Base,
}

impl CreateRoomV1Builder {
    /// Create a new CreateRoomV1 builder
    pub fn new(owner: PublicKey, token_id: pallas::Base) -> Self {
        let secret = SecretKey::random(&mut rand::rngs::OsRng);
        Self {
            owner,
            token_id,
            min_stake: 100,
            max_stake: 10000,
            entropy_mode: EntropyMode::BlockHash,
            confirmation_depth: 3,
            required_entropy_contributions: 0,
            entropy_contribution_deadline: 0,
            max_players: 10,
            nonce: secret.inner(),
        }
    }

    /// Set minimum stake
    pub fn min_stake(mut self, min_stake: u64) -> Self {
        self.min_stake = min_stake;
        self
    }

    /// Set maximum stake
    pub fn max_stake(mut self, max_stake: u64) -> Self {
        self.max_stake = max_stake;
        self
    }

    /// Set entropy mode
    pub fn entropy_mode(mut self, mode: EntropyMode) -> Self {
        self.entropy_mode = mode;
        self
    }

    /// Set confirmation depth
    pub fn confirmation_depth(mut self, depth: u8) -> Self {
        self.confirmation_depth = depth;
        self
    }

    /// Set required entropy contributions (for trusted setup)
    pub fn required_entropy_contributions(mut self, count: u8) -> Self {
        self.required_entropy_contributions = count;
        self
    }

    /// Set entropy contribution deadline
    pub fn entropy_contribution_deadline(mut self, deadline: u64) -> Self {
        self.entropy_contribution_deadline = deadline;
        self
    }

    /// Set max players
    pub fn max_players(mut self, max: u8) -> Self {
        self.max_players = max;
        self
    }

    /// Build the create room parameters
    pub fn build(&self) -> CreateRoomParamsV1 {
        CreateRoomParamsV1 {
            owner: self.owner,
            token_id: self.token_id,
            min_stake: self.min_stake,
            max_stake: self.max_stake,
            entropy_mode: self.entropy_mode,
            confirmation_depth: self.confirmation_depth,
            required_entropy_contributions: self.required_entropy_contributions,
            entropy_contribution_deadline: self.entropy_contribution_deadline,
            max_players: self.max_players,
            nonce: self.nonce,
        }
    }
}

/// Builder for deposit calls
pub struct DepositV1Builder {
    room_id: RoomId,
    player: PublicKey,
    amount: u64,
}

impl DepositV1Builder {
    /// Create a new DepositV1 builder
    pub fn new(room_id: RoomId, player: PublicKey, amount: u64) -> Self {
        Self { room_id, player, amount }
    }

    /// Build the deposit parameters
    pub fn build(&self) -> DepositParamsV1 {
        DepositParamsV1 { room_id: self.room_id, player: self.player, amount: self.amount }
    }
}

/// Builder for withdraw calls
pub struct WithdrawV1Builder {
    room_id: RoomId,
    player: PublicKey,
    amount: u64,
}

impl WithdrawV1Builder {
    /// Create a new WithdrawV1 builder
    pub fn new(room_id: RoomId, player: PublicKey, amount: u64) -> Self {
        Self { room_id, player, amount }
    }

    /// Build the withdraw parameters
    pub fn build(&self) -> WithdrawParamsV1 {
        WithdrawParamsV1 { room_id: self.room_id, player: self.player, amount: self.amount }
    }
}

/// Builder for place bet calls
pub struct PlaceBetV1Builder {
    room_id: RoomId,
    player: PublicKey,
    amount: u64,
    bet_type: BetType,
    nonce: pallas::Base,
}

impl PlaceBetV1Builder {
    /// Create a new PlaceBetV1 builder
    pub fn new(room_id: RoomId, player: PublicKey, amount: u64, bet_type: BetType) -> Self {
        let secret = SecretKey::random(&mut rand::rngs::OsRng);
        Self { room_id, player, amount, bet_type, nonce: secret.inner() }
    }

    /// Set nonce (for reproducibility)
    pub fn nonce(mut self, nonce: pallas::Base) -> Self {
        self.nonce = nonce;
        self
    }

    /// Build the place bet parameters
    pub fn build(&self) -> PlaceBetParamsV1 {
        PlaceBetParamsV1 {
            room_id: self.room_id,
            player: self.player,
            amount: self.amount,
            bet_type: self.bet_type,
            nonce: self.nonce,
        }
    }
}

/// Builder for raise calls
pub struct RaiseV1Builder {
    room_id: RoomId,
    player: PublicKey,
    amount: u64,
    nonce: pallas::Base,
}

impl RaiseV1Builder {
    /// Create a new RaiseV1 builder
    pub fn new(room_id: RoomId, player: PublicKey, amount: u64) -> Self {
        let secret = SecretKey::random(&mut rand::rngs::OsRng);
        Self { room_id, player, amount, nonce: secret.inner() }
    }

    /// Build the raise parameters
    pub fn build(&self) -> RaiseParamsV1 {
        RaiseParamsV1 { room_id: self.room_id, player: self.player, amount: self.amount, nonce: self.nonce }
    }
}

/// Builder for call calls
pub struct CallV1Builder {
    room_id: RoomId,
    player: PublicKey,
    nonce: pallas::Base,
}

impl CallV1Builder {
    /// Create a new CallV1 builder
    pub fn new(room_id: RoomId, player: PublicKey) -> Self {
        let secret = SecretKey::random(&mut rand::rngs::OsRng);
        Self { room_id, player, nonce: secret.inner() }
    }

    /// Build the call parameters
    pub fn build(&self) -> CallParamsV1 {
        CallParamsV1 { room_id: self.room_id, player: self.player, nonce: self.nonce }
    }
}

/// Builder for fold calls
pub struct FoldV1Builder {
    room_id: RoomId,
    player: PublicKey,
}

impl FoldV1Builder {
    /// Create a new FoldV1 builder
    pub fn new(room_id: RoomId, player: PublicKey) -> Self {
        Self { room_id, player }
    }

    /// Build the fold parameters
    pub fn build(&self) -> FoldParamsV1 {
        FoldParamsV1 { room_id: self.room_id, player: self.player }
    }
}

/// Builder for close pot calls
pub struct ClosePotV1Builder {
    room_id: RoomId,
    pot_id: PotId,
}

impl ClosePotV1Builder {
    /// Create a new ClosePotV1 builder
    pub fn new(room_id: RoomId, pot_id: PotId) -> Self {
        Self { room_id, pot_id }
    }

    /// Build the close pot parameters
    pub fn build(&self) -> ClosePotParamsV1 {
        ClosePotParamsV1 { room_id: self.room_id, pot_id: self.pot_id }
    }
}

/// Builder for settle pot calls
pub struct SettlePotV1Builder {
    caller: PublicKey,
    room_id: RoomId,
    pot_id: PotId,
    winners: Vec<(PublicKey, u64)>,
}

impl SettlePotV1Builder {
    /// Create a new SettlePotV1 builder
    pub fn new(room_id: RoomId, pot_id: PotId, winners: Vec<(PublicKey, u64)>) -> Self {
        Self { caller: winners[0].0, room_id, pot_id, winners }
    }

    /// Build the settle pot parameters
    pub fn build(&self) -> SettlePotParamsV1 {
        SettlePotParamsV1 {
            caller: self.caller,
            room_id: self.room_id,
            pot_id: self.pot_id,
            winners: self.winners.clone(),
            signature: vec![], // Filled by owner DAO
        }
    }
}

/// Builder for contribute entropy calls
pub struct ContributeEntropyV1Builder {
    room_id: RoomId,
    player: PublicKey,
    commitment: pallas::Base,
    reveal: Option<pallas::Base>,
}

impl ContributeEntropyV1Builder {
    /// Create a new ContributeEntropyV1 builder
    pub fn new(room_id: RoomId, player: PublicKey) -> Self {
        let secret = SecretKey::random(&mut rand::rngs::OsRng);
        let commitment = poseidon_hash([secret.inner(), pallas::Base::zero()]);
        Self { room_id, player, commitment, reveal: None }
    }

    /// Set reveal nonce
    pub fn reveal(mut self, nonce: pallas::Base) -> Self {
        self.reveal = Some(nonce);
        self
    }

    /// Build the contribute entropy parameters
    pub fn build(&self) -> ContributeEntropyParamsV1 {
        ContributeEntropyParamsV1 {
            room_id: self.room_id,
            player: self.player,
            commitment: self.commitment,
            reveal: self.reveal,
        }
    }
}

/// Builder for claim calls
pub struct ClaimV1Builder {
    room_id: RoomId,
    pot_id: PotId,
    winner: PublicKey,
    payout_amount: u64,
}

impl ClaimV1Builder {
    /// Create a new ClaimV1 builder
    pub fn new(room_id: RoomId, pot_id: PotId, winner: PublicKey, payout_amount: u64) -> Self {
        Self { room_id, pot_id, winner, payout_amount }
    }

    /// Build the claim parameters
    pub fn build(&self) -> ClaimParamsV1 {
        ClaimParamsV1 {
            room_id: self.room_id,
            pot_id: self.pot_id,
            winner: self.winner,
            payout_amount: self.payout_amount,
            proof: vec![], // Filled by ZK
        }
    }
}

/// Validate bet amount is within room limits
pub fn validate_bet_amount(amount: u64, min_stake: u64, max_stake: u64) -> Result<(), crate::error::GameRoomError> {
    if amount < min_stake {
        return Err(crate::error::GameRoomError::StakeBelowMin)
    }
    if amount > max_stake {
        return Err(crate::error::GameRoomError::StakeAboveMax)
    }
    Ok(())
}

/// Validate max players
pub fn validate_max_players(max_players: u8) -> Result<(), crate::error::GameRoomError> {
    if max_players == 0 || max_players > 100 {
        return Err(crate::error::GameRoomError::InvalidAmount)
    }
    Ok(())
}