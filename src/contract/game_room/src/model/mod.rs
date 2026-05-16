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

//! Game Room contract data structures

use dwow_sdk::{
    crypto::{poseidon_hash, ContractId, PublicKey},
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

impl TryFrom<u8> for RoomState {
    type Error = dwow_sdk::error::ContractError;

    fn try_from(b: u8) -> Result<Self, Self::Error> {
        match b {
            0 => Ok(Self::Open),
            1 => Ok(Self::Active),
            2 => Ok(Self::Concluded),
            _ => Err(dwow_sdk::error::ContractError::InvalidFunction),
        }
    }
}

/// Pot state
#[derive(Debug, Clone, Copy, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub enum PotState {
    Open = 0,
    Closed = 1,
    Settled = 2,
}

impl TryFrom<u8> for PotState {
    type Error = dwow_sdk::error::ContractError;

    fn try_from(b: u8) -> Result<Self, Self::Error> {
        match b {
            0 => Ok(Self::Open),
            1 => Ok(Self::Closed),
            2 => Ok(Self::Settled),
            _ => Err(dwow_sdk::error::ContractError::InvalidFunction),
        }
    }
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

impl TryFrom<u8> for BetType {
    type Error = dwow_sdk::error::ContractError;

    fn try_from(b: u8) -> Result<Self, Self::Error> {
        match b {
            0 => Ok(Self::Ante),
            1 => Ok(Self::Blind),
            2 => Ok(Self::Bet),
            3 => Ok(Self::Raise),
            4 => Ok(Self::Call),
            5 => Ok(Self::AllIn),
            6 => Ok(Self::Fold),
            _ => Err(dwow_sdk::error::ContractError::InvalidFunction),
        }
    }
}

/// Entropy source mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub enum EntropyMode {
    BlockHash = 0,
    TrustedSetup = 1,
}

impl TryFrom<u8> for EntropyMode {
    type Error = dwow_sdk::error::ContractError;

    fn try_from(b: u8) -> Result<Self, Self::Error> {
        match b {
            0 => Ok(Self::BlockHash),
            1 => Ok(Self::TrustedSetup),
            _ => Err(dwow_sdk::error::ContractError::InvalidFunction),
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

/// Game room state
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
    pub fn new(room_id: RoomId, config: RoomConfig, block: u64) -> Self {
        let entropy_deadline = config.entropy_contribution_deadline;
        Self {
            room_id,
            config,
            state: RoomState::Open,
            current_pot_id: None,
            current_bet_amount: 0,
            current_better: None,
            total_entropy_contributions: 0,
            combined_entropy: None,
            created_at: block,
            entropy_deadline: block + entropy_deadline,
        }
    }

    pub fn derive_room_id(
        owner_dao: &ContractId,
        token_id: pallas::Base,
        block_height: u64,
        nonce: pallas::Base,
    ) -> RoomId {
        poseidon_hash([
            owner_dao.inner(),
            token_id,
            pallas::Base::from(block_height),
            nonce,
        ])
    }
}

/// Player account (per-room player state)
///
/// Token balances are tracked by money_v3, not this contract.
/// money_v3::transfer_v1 child calls handle all token movement.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct PlayerAccount {
    pub pubkey: PublicKey,
    pub last_action_block: u64,
    pub has_folded: bool,
    pub entropy_contribution: Option<EntropyContribution>,
}

impl PlayerAccount {
    pub fn new(pubkey: PublicKey, block: u64) -> Self {
        Self { pubkey, last_action_block: block, has_folded: false, entropy_contribution: None }
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
    pub contributions: Vec<PotContribution>,
    pub state: PotState,
    pub betting_round: u8,
    pub created_at: u64,
}

impl Pot {
    pub fn new(pot_id: PotId, room_id: RoomId, block: u64) -> Self {
        Self {
            pot_id,
            room_id,
            total: 0,
            contributions: vec![],
            state: PotState::Open,
            betting_round: 0,
            created_at: block,
        }
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

impl Bet {
    pub fn new(
        bet_id: BetId,
        room_id: RoomId,
        pot_id: PotId,
        player: PublicKey,
        amount: u64,
        bet_type: BetType,
        round: u8,
        nonce: pallas::Base,
        block: u64,
    ) -> Self {
        let commitment = poseidon_hash([
            pallas::Base::from(amount),
            nonce,
            pallas::Base::from(block),
        ]);
        Self { bet_id, room_id, pot_id, player, amount, bet_type, round, commitment, block }
    }
}

// ============================================================================
// PARAMETER TYPES (for contract calls)
// ============================================================================

/// Parameters for CreateRoomV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CreateRoomParamsV1 {
    pub owner: PublicKey,
    pub token_id: pallas::Base,
    pub min_stake: u64,
    pub max_stake: u64,
    pub entropy_mode: EntropyMode,
    pub confirmation_depth: u8,
    pub required_entropy_contributions: u8,
    pub entropy_contribution_deadline: u64,
    pub max_players: u8,
    pub nonce: pallas::Base,
}

/// State update for CreateRoomV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CreateRoomUpdateV1 {
    pub room_id: RoomId,
    pub owner_dao: ContractId,
    pub config: RoomConfig,
}

/// Parameters for DepositV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct DepositParamsV1 {
    pub room_id: RoomId,
    pub player: PublicKey,
    pub amount: u64,
}

/// State update for DepositV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct DepositUpdateV1 {
    pub room_id: RoomId,
    pub player: PublicKey,
    pub amount: u64,
}

/// Parameters for WithdrawV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct WithdrawParamsV1 {
    pub room_id: RoomId,
    pub player: PublicKey,
    pub amount: u64,
}

/// State update for WithdrawV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct WithdrawUpdateV1 {
    pub room_id: RoomId,
    pub player: PublicKey,
    pub amount: u64,
}

/// Parameters for PlaceBetV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct PlaceBetParamsV1 {
    pub room_id: RoomId,
    pub pot_id: PotId,
    pub player: PublicKey,
    pub amount: u64,
    pub bet_type: BetType,
    pub nonce: pallas::Base,
    pub block_height: pallas::Base,
}

/// State update for PlaceBetV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct PlaceBetUpdateV1 {
    pub room_id: RoomId,
    pub pot_id: PotId,
    pub player: PublicKey,
    pub bet_id: BetId,
    pub amount: u64,
    pub new_pot_total: u64,
    pub new_current_bet: u64,
    pub new_current_better: PublicKey,
}

/// Parameters for RaiseV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct RaiseParamsV1 {
    pub room_id: RoomId,
    pub player: PublicKey,
    pub amount: u64,
    pub nonce: pallas::Base,
}

/// State update for RaiseV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct RaiseUpdateV1 {
    pub room_id: RoomId,
    pub player: PublicKey,
    pub amount: u64,
    pub new_pot_total: u64,
    pub new_current_bet: u64,
}

/// Parameters for CallV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CallParamsV1 {
    pub room_id: RoomId,
    pub player: PublicKey,
    pub nonce: pallas::Base,
}

/// State update for CallV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CallUpdateV1 {
    pub room_id: RoomId,
    pub player: PublicKey,
    pub amount: u64,
    pub new_pot_total: u64,
}

/// Parameters for FoldV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct FoldParamsV1 {
    pub room_id: RoomId,
    pub player: PublicKey,
}

/// State update for FoldV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct FoldUpdateV1 {
    pub room_id: RoomId,
    pub player: PublicKey,
    pub has_folded: bool,
}

/// Parameters for ClosePotV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ClosePotParamsV1 {
    pub room_id: RoomId,
    pub pot_id: PotId,
}

/// State update for ClosePotV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ClosePotUpdateV1 {
    pub room_id: RoomId,
    pub pot_id: PotId,
    pub new_pot_state: PotState,
    pub new_betting_round: u8,
    pub new_current_bet: u64,
    pub new_current_better: Option<PublicKey>,
}

/// Parameters for SettlePotV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct SettlePotParamsV1 {
    pub caller: PublicKey,
    pub room_id: RoomId,
    pub pot_id: PotId,
    pub winners: Vec<(PublicKey, u64)>,
    pub signature: Vec<u8>,
    pub nonce: pallas::Base,
    pub pot_total: u64,
}

/// State update for SettlePotV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct SettlePotUpdateV1 {
    pub room_id: RoomId,
    pub pot_id: PotId,
    pub new_pot_state: PotState,
    pub winners: Vec<PublicKey>,
    pub payouts: Vec<u64>,
}

/// Parameters for ContributeEntropyV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ContributeEntropyParamsV1 {
    pub room_id: RoomId,
    pub player: PublicKey,
    pub commitment: pallas::Base,
    pub reveal: Option<pallas::Base>,
}

/// State update for ContributeEntropyV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ContributeEntropyUpdateV1 {
    pub room_id: RoomId,
    pub player: PublicKey,
    pub combined_entropy: Option<pallas::Base>,
    pub contributions_count: u8,
}

/// Parameters for ClaimV1
///
/// Money Integration: This function REQUIRES money_v3::transfer_v1 child calls to be
/// bundled for distributing the prize payout to the winner. The child call should
/// transfer the claimed amount to the winner's public key.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ClaimParamsV1 {
    pub room_id: RoomId,
    pub pot_id: PotId,
    pub winner: PublicKey,
    /// The payout amount the winner is claiming (must match settled payout)
    pub payout_amount: u64,
    /// ZK proof that the payout_amount is correct for this winner
    pub proof: Vec<u8>,
    pub nonce: pallas::Base,
}

/// State update for ClaimV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ClaimUpdateV1 {
    pub room_id: RoomId,
    pub pot_id: PotId,
    pub winner: PublicKey,
    pub amount: u64,
}