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

//! Game Room SDK Types
//!
//! Shared type definitions for the Game Room SDK.
//! These mirror the contract's model types but are designed for
//! off-chain usage by app developers building game rooms.

use crate::{
    crypto::{ContractId, PublicKey},
    pasta::pallas,
};
use dwow_serial::{SerialDecodable, SerialEncodable};

/// Game room identifier
pub type RoomId = pallas::Base;

/// Pot identifier
pub type PotId = pallas::Base;

/// Bet identifier
pub type BetId = pallas::Base;

// ============================================================================
// ENUM TYPES
// ============================================================================

/// Room state
#[derive(Debug, Clone, Copy, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub enum RoomState {
    Open = 0,
    Active = 1,
    Concluded = 2,
}

/// Pot state
#[derive(Debug, Clone, Copy, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub enum PotState {
    Open = 0,
    Closed = 1,
    Settled = 2,
}

/// Bet type
#[derive(Debug, Clone, Copy, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub enum BetType {
    Ante = 0,
    Blind = 1,
    Bet = 2,
    Raise = 3,
    Call = 4,
    AllIn = 5,
    Fold = 6,
}

impl BetType {
    pub fn as_u8(&self) -> u8 {
        match self {
            Self::Ante => 0,
            Self::Blind => 1,
            Self::Bet => 2,
            Self::Raise => 3,
            Self::Call => 4,
            Self::AllIn => 5,
            Self::Fold => 6,
        }
    }

    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Ante),
            1 => Some(Self::Blind),
            2 => Some(Self::Bet),
            3 => Some(Self::Raise),
            4 => Some(Self::Call),
            5 => Some(Self::AllIn),
            6 => Some(Self::Fold),
            _ => None,
        }
    }
}

/// Entropy source mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub enum EntropyMode {
    BlockHash = 0,
    TrustedSetup = 1,
}

impl EntropyMode {
    pub fn as_u8(&self) -> u8 {
        match self {
            Self::BlockHash => 0,
            Self::TrustedSetup => 1,
        }
    }

    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::BlockHash),
            1 => Some(Self::TrustedSetup),
            _ => None,
        }
    }
}

// ============================================================================
// CORE DATA STRUCTURES
// ============================================================================

/// Room configuration (set at creation)
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct RoomConfig {
    pub owner_dao: ContractId,
    pub token_id: pallas::Base,
    pub min_stake: u64,
    pub max_stake: u64,
    pub entropy_mode: EntropyMode,
    pub confirmation_depth: u8,
    pub required_entropy_contributions: u8,
    pub entropy_contribution_deadline: u64,
    pub max_players: u8,
}

impl RoomConfig {
    pub fn new(
        owner_dao: ContractId,
        token_id: pallas::Base,
        min_stake: u64,
        max_stake: u64,
        entropy_mode: EntropyMode,
    ) -> Self {
        Self {
            owner_dao,
            token_id,
            min_stake,
            max_stake,
            entropy_mode,
            confirmation_depth: 6,         // Default to 6 block confirmations
            required_entropy_contributions: 2, // Default minimum contributions
            entropy_contribution_deadline: 100, // Default deadline in blocks
            max_players: 10,               // Default max players
        }
    }

    pub fn with_confirmation_depth(mut self, depth: u8) -> Self {
        self.confirmation_depth = depth;
        self
    }

    pub fn with_entropy_contributions(mut self, required: u8, deadline: u64) -> Self {
        self.required_entropy_contributions = required;
        self.entropy_contribution_deadline = deadline;
        self
    }

    pub fn with_max_players(mut self, max: u8) -> Self {
        self.max_players = max;
        self
    }
}

/// Game room state (fetched from on-chain)
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct GameRoom {
    pub room_id: RoomId,
    pub config: RoomConfig,
    pub state: RoomState,
    pub current_pot_id: Option<PotId>,
    pub current_bet_amount: u64,
    pub current_better: Option<PublicKey>,
    pub total_entropy_contributions: u8,
    pub combined_entropy: Option<pallas::Base>,
    pub created_at: u64,
    pub entropy_deadline: u64,
}

impl GameRoom {
    pub fn is_open(&self) -> bool {
        self.state == RoomState::Open
    }

    pub fn is_active(&self) -> bool {
        self.state == RoomState::Active
    }

    pub fn is_concluded(&self) -> bool {
        self.state == RoomState::Concluded
    }
}

/// Player account (balance ledger per room)
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct PlayerAccount {
    pub pubkey: PublicKey,
    pub balance: u64,
    pub locked: u64,
    pub last_action_block: u64,
    pub has_folded: bool,
    pub entropy_contribution: Option<EntropyContribution>,
}

impl PlayerAccount {
    pub fn available_balance(&self) -> u64 {
        self.balance.saturating_sub(self.locked)
    }
}

/// Entropy contribution (for trusted setup)
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct EntropyContribution {
    pub commitment: pallas::Base,
    pub revealed_nonce: Option<pallas::Base>,
    pub contributed_at: u64,
}

/// Pot (collective betting pool)
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Pot {
    pub pot_id: PotId,
    pub room_id: RoomId,
    pub total: u64,
    pub state: PotState,
    pub betting_round: u8,
    pub created_at: u64,
}

impl Pot {
    pub fn is_open(&self) -> bool {
        self.state == PotState::Open
    }

    pub fn is_closed(&self) -> bool {
        self.state == PotState::Closed
    }

    pub fn is_settled(&self) -> bool {
        self.state == PotState::Settled
    }
}

/// Individual contribution to a pot
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct PotContribution {
    pub player: PublicKey,
    pub amount: u64,
    pub bet_type: BetType,
    pub block: u64,
}

/// Bet record
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Bet {
    pub bet_id: BetId,
    pub room_id: RoomId,
    pub pot_id: PotId,
    pub player: PublicKey,
    pub amount: u64,
    pub bet_type: BetType,
    pub round: u8,
    pub commitment: pallas::Base,
    pub block: u64,
}

// ============================================================================
// SDK CONFIGURATION
// ============================================================================

/// Configuration for SDK initialization
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct GameRoomSdkConfig {
    /// RPC endpoint URL
    pub rpc_url: String,
    /// Contract ID for the game room
    pub contract_id: ContractId,
    /// User's keypair for signing transactions
    pub keypair: crate::crypto::Keypair,
}

impl GameRoomSdkConfig {
    pub fn new(rpc_url: &str, contract_id: ContractId, keypair: crate::crypto::Keypair) -> Self {
        Self {
            rpc_url: rpc_url.to_string(),
            contract_id,
            keypair,
        }
    }
}

/// Entropy mode configuration
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct EntropyConfig {
    pub mode: EntropyMode,
    pub confirmation_depth: u8,
    pub required_contributions: u8,
    pub contribution_deadline: u64,
}

impl Default for EntropyConfig {
    fn default() -> Self {
        Self {
            mode: EntropyMode::BlockHash,
            confirmation_depth: 6,
            required_contributions: 2,
            contribution_deadline: 100,
        }
    }
}

/// Room creation parameters
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CreateRoomParams {
    pub owner: PublicKey,
    pub token_id: pallas::Base,
    pub min_stake: u64,
    pub max_stake: u64,
    pub entropy_config: EntropyConfig,
    pub max_players: u8,
    pub nonce: pallas::Base,
}

/// Deposit parameters
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct DepositParams {
    pub room_id: RoomId,
    pub player: PublicKey,
    pub amount: u64,
}

/// Withdraw parameters
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct WithdrawParams {
    pub room_id: RoomId,
    pub player: PublicKey,
    pub amount: u64,
}

/// Place bet parameters
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct PlaceBetParams {
    pub room_id: RoomId,
    pub player: PublicKey,
    pub amount: u64,
    pub bet_type: BetType,
    pub nonce: pallas::Base,
}

/// Raise parameters
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct RaiseParams {
    pub room_id: RoomId,
    pub player: PublicKey,
    pub amount: u64,
    pub nonce: pallas::Base,
}

/// Call parameters
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CallParams {
    pub room_id: RoomId,
    pub player: PublicKey,
    pub nonce: pallas::Base,
}

/// Fold parameters
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct FoldParams {
    pub room_id: RoomId,
    pub player: PublicKey,
}

/// Close pot parameters
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ClosePotParams {
    pub room_id: RoomId,
    pub pot_id: PotId,
}

/// Settle pot parameters
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct SettlePotParams {
    pub caller: PublicKey,
    pub room_id: RoomId,
    pub pot_id: PotId,
    pub winners: Vec<(PublicKey, u64)>,
    pub signature: Vec<u8>,
}

/// Entropy contribution parameters
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ContributeEntropyParams {
    pub room_id: RoomId,
    pub player: PublicKey,
    pub commitment: pallas::Base,
    pub reveal: Option<pallas::Base>,
}

/// Claim parameters
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ClaimParams {
    pub room_id: RoomId,
    pub pot_id: PotId,
    pub winner: PublicKey,
}