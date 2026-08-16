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
    crypto::{pasta_prelude::PrimeField, poseidon_hash, ContractId, PublicKey},
    error::ContractError,
    pasta::pallas,
};

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
#[derive(Debug, Clone, Copy, PartialEq, Eq,)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq,)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq,)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq,)]
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
#[derive(Debug, Clone)]
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
    pub const ENCODED_SIZE: usize = 92;

    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(92);
        b.extend_from_slice(&self.owner_dao.to_bytes());
        b.extend_from_slice(&self.token_id.to_repr());
        b.extend_from_slice(&self.min_stake.to_le_bytes());
        b.extend_from_slice(&self.max_stake.to_le_bytes());
        b.push(self.entropy_mode as u8);
        b.push(self.confirmation_depth);
        b.push(self.required_entropy_contributions);
        b.extend_from_slice(&self.entropy_contribution_deadline.to_le_bytes());
        b.push(self.max_players);
        b
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 92 {
            return Err(ContractError::IoError(format!(
                "RoomConfig: expected 92 bytes, got {}",
                data.len()
            )));
        }
        Ok(RoomConfig {
            owner_dao: ContractId::from_bytes(data[0..32].try_into().unwrap())?,
            token_id: Option::<pallas::Base>::from(pallas::Base::from_repr(
                data[32..64].try_into().unwrap(),
            ))
            .ok_or_else(|| {
                ContractError::IoError("RoomConfig: invalid token_id".into())
            })?,
            min_stake: u64::from_le_bytes(data[64..72].try_into().unwrap()),
            max_stake: u64::from_le_bytes(data[72..80].try_into().unwrap()),
            entropy_mode: EntropyMode::try_from(data[80])?,
            confirmation_depth: data[81],
            required_entropy_contributions: data[82],
            entropy_contribution_deadline: u64::from_le_bytes(
                data[83..91].try_into().unwrap(),
            ),
            max_players: data[91],
        })
    }
}

/// Game room state
#[derive(Debug, Clone)]
pub struct GameRoom {
    pub version: u8,
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
    pub instance_seed: [u8; 32],
}

impl GameRoom {
    pub fn encode(&self) -> Vec<u8> {
        let cap = 183
            + if self.current_pot_id.is_some() { 32 } else { 0 }
            + if self.current_better.is_some() { 32 } else { 0 }
            + if self.combined_entropy.is_some() { 32 } else { 0 };
        let mut b = Vec::with_capacity(cap);
        b.push(self.version);
        b.extend_from_slice(&self.room_id.to_repr());
        b.extend_from_slice(&self.config.encode());
        b.push(self.state as u8);
        b.push(self.current_pot_id.is_some() as u8);
        if let Some(pid) = &self.current_pot_id {
            b.extend_from_slice(&pid.to_repr());
        }
        b.extend_from_slice(&self.current_bet_amount.to_le_bytes());
        b.push(self.current_better.is_some() as u8);
        if let Some(ref pk) = self.current_better {
            b.extend_from_slice(&pk.to_bytes());
        }
        b.push(self.total_entropy_contributions);
        b.push(self.combined_entropy.is_some() as u8);
        if let Some(ce) = &self.combined_entropy {
            b.extend_from_slice(&ce.to_repr());
        }
        b.extend_from_slice(&self.created_at.to_le_bytes());
        b.extend_from_slice(&self.entropy_deadline.to_le_bytes());
        b.extend_from_slice(&self.instance_seed);
        b
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 186 {
            return Err(ContractError::IoError(format!(
                "GameRoom: expected at least 186 bytes, got {}",
                data.len()
            )));
        }
        let version = data[0];
        let room_id = Option::<pallas::Base>::from(pallas::Base::from_repr(
            data[1..33].try_into().unwrap(),
        ))
        .ok_or_else(|| ContractError::IoError("GameRoom: invalid room_id".into()))?;
        let config = RoomConfig::decode(&data[33..125])?;
        let state = RoomState::try_from(data[125])?;
        let has_pot = data[126] != 0;
        let (current_pot_id, mut pos) = if has_pot {
            (
                Some(
                    Option::<pallas::Base>::from(pallas::Base::from_repr(
                        data[127..159].try_into().unwrap(),
                    ))
                    .ok_or_else(|| {
                        ContractError::IoError("GameRoom: invalid current_pot_id".into())
                    })?,
                ),
                159usize,
            )
        } else {
            (None, 127usize)
        };
        let current_bet_amount = u64::from_le_bytes(data[pos..pos + 8].try_into().unwrap());
        pos += 8;
        let has_better = data[pos] != 0;
        pos += 1;
        let current_better = if has_better {
            if data.len() < pos + 32 {
                return Err(ContractError::IoError(
                    "GameRoom: data too short for current_better".into(),
                ));
            }
            let pk = PublicKey::from_bytes(data[pos..pos + 32].try_into().unwrap())
                .map_err(|e| {
                    ContractError::IoError(format!(
                        "GameRoom: invalid current_better: {}",
                        e
                    ))
                })?;
            pos += 32;
            Some(pk)
        } else {
            None
        };
        let total_entropy_contributions = data[pos];
        pos += 1;
        let has_entropy = data[pos] != 0;
        pos += 1;
        let combined_entropy = if has_entropy {
            if data.len() < pos + 32 {
                return Err(ContractError::IoError(
                    "GameRoom: data too short for combined_entropy".into(),
                ));
            }
            let ce = Option::<pallas::Base>::from(pallas::Base::from_repr(
                data[pos..pos + 32].try_into().unwrap(),
            ))
            .ok_or_else(|| {
                ContractError::IoError("GameRoom: invalid combined_entropy".into())
            })?;
            pos += 32;
            Some(ce)
        } else {
            None
        };
        let created_at = u64::from_le_bytes(data[pos..pos + 8].try_into().unwrap());
        pos += 8;
        let entropy_deadline = u64::from_le_bytes(data[pos..pos + 8].try_into().unwrap());
        pos += 8;
        let instance_seed: [u8; 32] = data[pos..pos + 32].try_into().unwrap();
        Ok(GameRoom {
            version,
            room_id,
            config,
            state,
            current_pot_id,
            current_bet_amount,
            current_better,
            total_entropy_contributions,
            combined_entropy,
            created_at,
            entropy_deadline,
            instance_seed,
        })
    }

    pub fn new(
        room_id: RoomId,
        config: RoomConfig,
        block: u64,
        instance_seed: [u8; 32],
    ) -> Self {
        let entropy_deadline = config.entropy_contribution_deadline;
        Self {
            version: 0,
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
            instance_seed,
        }
    }

    pub fn derive_room_id(
        owner: &PublicKey,
        token_id: pallas::Base,
        block_height: u64,
        nonce: pallas::Base,
    ) -> RoomId {
        let (ox, oy) = owner.xy().expect("pk not identity");
        poseidon_hash([
            pallas::Base::from(4u64), // DOMAIN_COIN_COMMIT
            ox,
            oy,
            token_id,
            pallas::Base::from(block_height),
            nonce,
        ])
    }
}

/// Player account (per-room player state)
///
/// Token balances are tracked by promissory_note, not this contract.
/// promissory_note::transfer_v1 child calls handle all token movement.
#[derive(Debug, Clone)]
pub struct PlayerAccount {
    pub version: u8,
    pub pubkey: PublicKey,
    pub last_action_block: u64,
    pub has_folded: bool,
    pub entropy_contribution: Option<EntropyContribution>,
    pub instance_seed: [u8; 32],
}

impl PlayerAccount {
    pub fn encode(&self) -> Vec<u8> {
        let inner_cap = if self.entropy_contribution.is_some() {
            if self.entropy_contribution.as_ref().unwrap().revealed_nonce.is_some() {
                73
            } else {
                41
            }
        } else {
            0
        };
        let cap = 75 + inner_cap;
        let mut b = Vec::with_capacity(cap);
        b.push(self.version);
        b.extend_from_slice(&self.pubkey.to_bytes());
        b.extend_from_slice(&self.last_action_block.to_le_bytes());
        b.push(self.has_folded as u8);
        b.push(self.entropy_contribution.is_some() as u8);
        if let Some(ref ec) = self.entropy_contribution {
            b.extend_from_slice(&ec.encode());
        }
        b.extend_from_slice(&self.instance_seed);
        b
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 75 {
            return Err(ContractError::IoError(format!(
                "PlayerAccount: expected at least 75 bytes, got {}",
                data.len()
            )));
        }
        let version = data[0];
        let pubkey = PublicKey::from_bytes(data[1..33].try_into().unwrap()).map_err(|e| {
            ContractError::IoError(format!("PlayerAccount: invalid pubkey: {}", e))
        })?;
        let last_action_block = u64::from_le_bytes(data[33..41].try_into().unwrap());
        let has_folded = data[41] != 0;
        let has_ec = data[42] != 0;
        let (entropy_contribution, pos) = if has_ec {
            if data.len() < 43 {
                return Err(ContractError::IoError(
                    "PlayerAccount: data too short for entropy_contribution".into(),
                ));
            }
            let ec = EntropyContribution::decode(&data[43..])?;
            let ec_size = if ec.revealed_nonce.is_some() { 73 } else { 41 };
            (Some(ec), 43 + ec_size)
        } else {
            (None, 43usize)
        };
        let instance_seed: [u8; 32] = data[pos..pos + 32].try_into().unwrap();
        Ok(PlayerAccount {
            version,
            pubkey,
            last_action_block,
            has_folded,
            entropy_contribution,
            instance_seed,
        })
    }

    pub fn new(pubkey: PublicKey, block: u64, instance_seed: [u8; 32]) -> Self {
        Self {
            version: 0,
            pubkey,
            last_action_block: block,
            has_folded: false,
            entropy_contribution: None,
            instance_seed,
        }
    }
}

/// Entropy contribution (for trusted setup)
#[derive(Debug, Clone)]
pub struct EntropyContribution {
    pub commitment: pallas::Base,
    pub revealed_nonce: Option<pallas::Base>,
    pub contributed_at: u64,
}

impl EntropyContribution {
    pub fn encode(&self) -> Vec<u8> {
        let cap = 41 + if self.revealed_nonce.is_some() { 32 } else { 0 };
        let mut b = Vec::with_capacity(cap);
        b.extend_from_slice(&self.commitment.to_repr());
        b.push(self.revealed_nonce.is_some() as u8);
        if let Some(ref rn) = self.revealed_nonce {
            b.extend_from_slice(&rn.to_repr());
        }
        b.extend_from_slice(&self.contributed_at.to_le_bytes());
        b
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 41 {
            return Err(ContractError::IoError(format!(
                "EntropyContribution: expected at least 41 bytes, got {}",
                data.len()
            )));
        }
        let commitment =
            Option::<pallas::Base>::from(pallas::Base::from_repr(
                data[0..32].try_into().unwrap(),
            ))
            .ok_or_else(|| {
                ContractError::IoError("EntropyContribution: invalid commitment".into())
            })?;
        let has_nonce = data[32] != 0;
        let (revealed_nonce, pos) = if has_nonce {
            if data.len() < 73 {
                return Err(ContractError::IoError(
                    "EntropyContribution: expected 73 bytes for Some, got less".into(),
                ));
            }
            (
                Some(
                    Option::<pallas::Base>::from(pallas::Base::from_repr(
                        data[33..65].try_into().unwrap(),
                    ))
                    .ok_or_else(|| {
                        ContractError::IoError(
                            "EntropyContribution: invalid revealed_nonce".into(),
                        )
                    })?,
                ),
                65usize,
            )
        } else {
            (None, 33usize)
        };
        let contributed_at = u64::from_le_bytes(data[pos..pos + 8].try_into().unwrap());
        Ok(EntropyContribution {
            commitment,
            revealed_nonce,
            contributed_at,
        })
    }
}

/// Pot (collective betting pool)
#[derive(Debug, Clone)]
pub struct Pot {
    pub version: u8,
    pub pot_id: PotId,
    pub room_id: RoomId,
    pub total: u64,
    pub contributions: Vec<PotContribution>,
    pub state: PotState,
    pub betting_round: u8,
    pub created_at: u64,
}

impl Pot {
    pub fn encode(&self) -> Vec<u8> {
        let cap = 84 + self.contributions.len() * PotContribution::ENCODED_SIZE;
        let mut b = Vec::with_capacity(cap);
        b.push(self.version);
        b.extend_from_slice(&self.pot_id.to_repr());
        b.extend_from_slice(&self.room_id.to_repr());
        b.extend_from_slice(&self.total.to_le_bytes());
        b.push(self.contributions.len() as u8);
        for c in &self.contributions {
            b.extend_from_slice(&c.encode());
        }
        b.push(self.state as u8);
        b.push(self.betting_round);
        b.extend_from_slice(&self.created_at.to_le_bytes());
        b
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 84 {
            return Err(ContractError::IoError(format!(
                "Pot: expected at least 84 bytes, got {}",
                data.len()
            )));
        }
        let version = data[0];
        let pot_id = Option::<pallas::Base>::from(pallas::Base::from_repr(
            data[1..33].try_into().unwrap(),
        ))
        .ok_or_else(|| ContractError::IoError("Pot: invalid pot_id".into()))?;
        let room_id = Option::<pallas::Base>::from(pallas::Base::from_repr(
            data[33..65].try_into().unwrap(),
        ))
        .ok_or_else(|| ContractError::IoError("Pot: invalid room_id".into()))?;
        let total = u64::from_le_bytes(data[65..73].try_into().unwrap());
        let count = data[73] as usize;
        let mut contributions = Vec::with_capacity(count);
        let mut pos = 74usize;
        for _i in 0..count {
            let contrib = PotContribution::decode(&data[pos..])?;
            pos += PotContribution::ENCODED_SIZE;
            contributions.push(contrib);
        }
        let state = PotState::try_from(data[pos])?;
        pos += 1;
        let betting_round = data[pos];
        pos += 1;
        let created_at = u64::from_le_bytes(data[pos..pos + 8].try_into().unwrap());
        Ok(Pot {
            version,
            pot_id,
            room_id,
            total,
            contributions,
            state,
            betting_round,
            created_at,
        })
    }

    pub fn new(pot_id: PotId, room_id: RoomId, block: u64) -> Self {
        Self {
            version: 0,
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
#[derive(Debug, Clone)]
pub struct PotContribution {
    pub player: PublicKey,
    pub amount: u64,
    pub bet_type: BetType,
    pub block: u64,
}

impl PotContribution {
    pub const ENCODED_SIZE: usize = 49;

    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(49);
        b.extend_from_slice(&self.player.to_bytes());
        b.extend_from_slice(&self.amount.to_le_bytes());
        b.push(self.bet_type as u8);
        b.extend_from_slice(&self.block.to_le_bytes());
        b
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 49 {
            return Err(ContractError::IoError(format!(
                "PotContribution: expected 49 bytes, got {}",
                data.len()
            )));
        }
        Ok(PotContribution {
            player: PublicKey::from_bytes(data[0..32].try_into().unwrap()).map_err(|e| {
                ContractError::IoError(format!(
                    "PotContribution: invalid player: {}",
                    e
                ))
            })?,
            amount: u64::from_le_bytes(data[32..40].try_into().unwrap()),
            bet_type: BetType::try_from(data[40])?,
            block: u64::from_le_bytes(data[41..49].try_into().unwrap()),
        })
    }
}

/// Bet record
#[derive(Debug, Clone)]
pub struct Bet {
    pub version: u8,
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
    pub const ENCODED_SIZE: usize = 179;

    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(179);
        b.push(self.version);
        b.extend_from_slice(&self.bet_id.to_repr());
        b.extend_from_slice(&self.room_id.to_repr());
        b.extend_from_slice(&self.pot_id.to_repr());
        b.extend_from_slice(&self.player.to_bytes());
        b.extend_from_slice(&self.amount.to_le_bytes());
        b.push(self.bet_type as u8);
        b.push(self.round);
        b.extend_from_slice(&self.commitment.to_repr());
        b.extend_from_slice(&self.block.to_le_bytes());
        b
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 179 {
            return Err(ContractError::IoError(format!(
                "Bet: expected 179 bytes, got {}",
                data.len()
            )));
        }
        Ok(Bet {
            version: data[0],
            bet_id: Option::<pallas::Base>::from(pallas::Base::from_repr(
                data[1..33].try_into().unwrap(),
            ))
            .ok_or_else(|| ContractError::IoError("Bet: invalid bet_id".into()))?,
            room_id: Option::<pallas::Base>::from(pallas::Base::from_repr(
                data[33..65].try_into().unwrap(),
            ))
            .ok_or_else(|| ContractError::IoError("Bet: invalid room_id".into()))?,
            pot_id: Option::<pallas::Base>::from(pallas::Base::from_repr(
                data[65..97].try_into().unwrap(),
            ))
            .ok_or_else(|| ContractError::IoError("Bet: invalid pot_id".into()))?,
            player: PublicKey::from_bytes(data[97..129].try_into().unwrap()).map_err(|e| {
                ContractError::IoError(format!("Bet: invalid player: {}", e))
            })?,
            amount: u64::from_le_bytes(data[129..137].try_into().unwrap()),
            bet_type: BetType::try_from(data[137])?,
            round: data[138],
            commitment: Option::<pallas::Base>::from(pallas::Base::from_repr(
                data[139..171].try_into().unwrap(),
            ))
            .ok_or_else(|| ContractError::IoError("Bet: invalid commitment".into()))?,
            block: u64::from_le_bytes(data[171..179].try_into().unwrap()),
        })
    }

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
            pallas::Base::from(4u64), // DOMAIN_COIN_COMMIT
            pallas::Base::from(amount),
            nonce,
            pallas::Base::from(block),
        ]);
        Self {
            version: 0,
            bet_id,
            room_id,
            pot_id,
            player,
            amount,
            bet_type,
            round,
            commitment,
            block,
        }
    }
}

// ============================================================================
// PARAMETER TYPES (for contract calls)
// ============================================================================

fn read_base(data: &[u8]) -> Result<pallas::Base, ContractError> { Option::<pallas::Base>::from(pallas::Base::from_repr(data.try_into().unwrap())).ok_or_else(|| ContractError::IoError("invalid base".into())) }

fn write_len_prefixed(b: &mut Vec<u8>, data: &[u8]) {
    b.extend_from_slice(&(data.len() as u32).to_le_bytes());
    b.extend_from_slice(data);
}

fn read_len_prefixed(data: &[u8]) -> Result<(Vec<u8>, usize), ContractError> {
    if data.len() < 4 {
        return Err(ContractError::IoError("len-prefixed record too short".into()));
    }
    let len = u32::from_le_bytes(data[0..4].try_into().unwrap()) as usize;
    if data.len() < 4 + len {
        return Err(ContractError::IoError("len-prefixed record truncated".into()));
    }
    Ok((data[4..4 + len].to_vec(), 4 + len))
}

/// Parameters for CreateRoomV1
#[derive(Debug, Clone,)]
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
    pub block_height: u64,
    pub nonce: pallas::Base,
    pub instance_seed: [u8; 32],
}

impl dwow_serial::Encodable for CreateRoomParamsV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for CreateRoomParamsV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl CreateRoomParamsV1 { pub const ENCODED_SIZE: usize = 164; pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(164); b.extend_from_slice(&self.owner.to_bytes()); b.extend_from_slice(&self.token_id.to_repr()); b.extend_from_slice(&self.min_stake.to_le_bytes()); b.extend_from_slice(&self.max_stake.to_le_bytes()); b.push(self.entropy_mode as u8); b.push(self.confirmation_depth); b.push(self.required_entropy_contributions); b.extend_from_slice(&self.entropy_contribution_deadline.to_le_bytes()); b.push(self.max_players); b.extend_from_slice(&self.block_height.to_le_bytes()); b.extend_from_slice(&self.nonce.to_repr()); b.extend_from_slice(&self.instance_seed); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 164 { return Err(ContractError::IoError(format!("CreateRoomParamsV1: expected 164 bytes, got {}", data.len()))); } Ok(CreateRoomParamsV1 { owner: PublicKey::from_bytes(data[0..32].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("CreateRoomParamsV1: invalid owner: {}", e)))?, token_id: read_base(&data[32..64])?, min_stake: u64::from_le_bytes(data[64..72].try_into().unwrap()), max_stake: u64::from_le_bytes(data[72..80].try_into().unwrap()), entropy_mode: EntropyMode::try_from(data[80])?, confirmation_depth: data[81], required_entropy_contributions: data[82], entropy_contribution_deadline: u64::from_le_bytes(data[83..91].try_into().unwrap()), max_players: data[91], block_height: u64::from_le_bytes(data[92..100].try_into().unwrap()), nonce: read_base(&data[100..132])?, instance_seed: data[132..164].try_into().unwrap() }) } }

#[derive(Debug, Clone,)] pub struct DepositParamsV1 { pub room_id: RoomId, pub player: PublicKey, pub amount: u64, pub instance_seed: [u8; 32] }
impl dwow_serial::Encodable for DepositParamsV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for DepositParamsV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl DepositParamsV1 { pub const ENCODED_SIZE: usize = 104; pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(104); b.extend_from_slice(&self.room_id.to_repr()); b.extend_from_slice(&self.player.to_bytes()); b.extend_from_slice(&self.amount.to_le_bytes()); b.extend_from_slice(&self.instance_seed); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 104 { return Err(ContractError::IoError(format!("DepositParamsV1: expected 104 bytes, got {}", data.len()))); } Ok(DepositParamsV1 { room_id: read_base(&data[0..32])?, player: PublicKey::from_bytes(data[32..64].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("DepositParamsV1: invalid player: {}", e)))?, amount: u64::from_le_bytes(data[64..72].try_into().unwrap()), instance_seed: data[72..104].try_into().unwrap() }) } }

#[derive(Debug, Clone,)] pub struct WithdrawParamsV1 { pub room_id: RoomId, pub player: PublicKey, pub amount: u64, pub player_nullifier: pallas::Base }
impl dwow_serial::Encodable for WithdrawParamsV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for WithdrawParamsV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl WithdrawParamsV1 { pub const ENCODED_SIZE: usize = 104; pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(104); b.extend_from_slice(&self.room_id.to_repr()); b.extend_from_slice(&self.player.to_bytes()); b.extend_from_slice(&self.amount.to_le_bytes()); b.extend_from_slice(&self.player_nullifier.to_repr()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 104 { return Err(ContractError::IoError(format!("WithdrawParamsV1: expected 104 bytes, got {}", data.len()))); } Ok(WithdrawParamsV1 { room_id: read_base(&data[0..32])?, player: PublicKey::from_bytes(data[32..64].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("WithdrawParamsV1: invalid player: {}", e)))?, amount: u64::from_le_bytes(data[64..72].try_into().unwrap()), player_nullifier: read_base(&data[72..104])? }) } }

#[derive(Debug, Clone,)] pub struct PlaceBetParamsV1 { pub room_id: RoomId, pub pot_id: PotId, pub player: PublicKey, pub amount: u64, pub bet_type: BetType, pub nonce: pallas::Base, pub block_height: pallas::Base }
impl dwow_serial::Encodable for PlaceBetParamsV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for PlaceBetParamsV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl PlaceBetParamsV1 { pub const ENCODED_SIZE: usize = 169; pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(169); b.extend_from_slice(&self.room_id.to_repr()); b.extend_from_slice(&self.pot_id.to_repr()); b.extend_from_slice(&self.player.to_bytes()); b.extend_from_slice(&self.amount.to_le_bytes()); b.push(self.bet_type as u8); b.extend_from_slice(&self.nonce.to_repr()); b.extend_from_slice(&self.block_height.to_repr()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 169 { return Err(ContractError::IoError(format!("PlaceBetParamsV1: expected 169 bytes, got {}", data.len()))); } Ok(PlaceBetParamsV1 { room_id: read_base(&data[0..32])?, pot_id: read_base(&data[32..64])?, player: PublicKey::from_bytes(data[64..96].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("PlaceBetParamsV1: invalid player: {}", e)))?, amount: u64::from_le_bytes(data[96..104].try_into().unwrap()), bet_type: BetType::try_from(data[104])?, nonce: read_base(&data[105..137])?, block_height: read_base(&data[137..169])? }) } }

#[derive(Debug, Clone,)] pub struct RaiseParamsV1 { pub room_id: RoomId, pub player: PublicKey, pub amount: u64, pub nonce: pallas::Base, pub player_nullifier: pallas::Base }
impl dwow_serial::Encodable for RaiseParamsV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for RaiseParamsV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl RaiseParamsV1 { pub const ENCODED_SIZE: usize = 136; pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(136); b.extend_from_slice(&self.room_id.to_repr()); b.extend_from_slice(&self.player.to_bytes()); b.extend_from_slice(&self.amount.to_le_bytes()); b.extend_from_slice(&self.nonce.to_repr()); b.extend_from_slice(&self.player_nullifier.to_repr()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 136 { return Err(ContractError::IoError(format!("RaiseParamsV1: expected 136 bytes, got {}", data.len()))); } Ok(RaiseParamsV1 { room_id: read_base(&data[0..32])?, player: PublicKey::from_bytes(data[32..64].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("RaiseParamsV1: invalid player: {}", e)))?, amount: u64::from_le_bytes(data[64..72].try_into().unwrap()), nonce: read_base(&data[72..104])?, player_nullifier: read_base(&data[104..136])? }) } }

#[derive(Debug, Clone,)] pub struct CallParamsV1 { pub room_id: RoomId, pub player: PublicKey, pub nonce: pallas::Base, pub player_nullifier: pallas::Base }
impl dwow_serial::Encodable for CallParamsV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for CallParamsV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl CallParamsV1 { pub const ENCODED_SIZE: usize = 128; pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(128); b.extend_from_slice(&self.room_id.to_repr()); b.extend_from_slice(&self.player.to_bytes()); b.extend_from_slice(&self.nonce.to_repr()); b.extend_from_slice(&self.player_nullifier.to_repr()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 128 { return Err(ContractError::IoError(format!("CallParamsV1: expected 128 bytes, got {}", data.len()))); } Ok(CallParamsV1 { room_id: read_base(&data[0..32])?, player: PublicKey::from_bytes(data[32..64].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("CallParamsV1: invalid player: {}", e)))?, nonce: read_base(&data[64..96])?, player_nullifier: read_base(&data[96..128])? }) } }

#[derive(Debug, Clone,)] pub struct FoldParamsV1 { pub room_id: RoomId, pub player: PublicKey, pub player_nullifier: pallas::Base }
impl dwow_serial::Encodable for FoldParamsV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for FoldParamsV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl FoldParamsV1 { pub const ENCODED_SIZE: usize = 96; pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(96); b.extend_from_slice(&self.room_id.to_repr()); b.extend_from_slice(&self.player.to_bytes()); b.extend_from_slice(&self.player_nullifier.to_repr()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 96 { return Err(ContractError::IoError(format!("FoldParamsV1: expected 96 bytes, got {}", data.len()))); } Ok(FoldParamsV1 { room_id: read_base(&data[0..32])?, player: PublicKey::from_bytes(data[32..64].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("FoldParamsV1: invalid player: {}", e)))?, player_nullifier: read_base(&data[64..96])? }) } }

#[derive(Debug, Clone,)] pub struct ClosePotParamsV1 { pub room_id: RoomId, pub pot_id: PotId, pub player: PublicKey, pub player_nullifier: pallas::Base }
impl dwow_serial::Encodable for ClosePotParamsV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for ClosePotParamsV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl ClosePotParamsV1 { pub const ENCODED_SIZE: usize = 128; pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(128); b.extend_from_slice(&self.room_id.to_repr()); b.extend_from_slice(&self.pot_id.to_repr()); b.extend_from_slice(&self.player.to_bytes()); b.extend_from_slice(&self.player_nullifier.to_repr()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 128 { return Err(ContractError::IoError(format!("ClosePotParamsV1: expected 128 bytes, got {}", data.len()))); } Ok(ClosePotParamsV1 { room_id: read_base(&data[0..32])?, pot_id: read_base(&data[32..64])?, player: PublicKey::from_bytes(data[64..96].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("ClosePotParamsV1: invalid player: {}", e)))?, player_nullifier: read_base(&data[96..128])? }) } }

#[derive(Debug, Clone,)] pub struct SettlePotParamsV1 { pub caller: PublicKey, pub room_id: RoomId, pub pot_id: PotId, pub winners: Vec<(PublicKey, u64)>, pub signature: Vec<u8>, pub nonce: pallas::Base, pub pot_total: u64 }
impl dwow_serial::Encodable for SettlePotParamsV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for SettlePotParamsV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl SettlePotParamsV1 { pub fn encode(&self) -> Vec<u8> { let wc: usize = self.winners.iter().map(|_| 40).sum(); let mut b = Vec::with_capacity(98+wc+self.signature.len()); b.extend_from_slice(&self.caller.to_bytes()); b.extend_from_slice(&self.room_id.to_repr()); b.extend_from_slice(&self.pot_id.to_repr()); b.push(self.winners.len() as u8); for (pk, amt) in &self.winners { b.extend_from_slice(&pk.to_bytes()); b.extend_from_slice(&amt.to_le_bytes()); } b.push(self.signature.len() as u8); b.extend_from_slice(&self.signature); b.extend_from_slice(&self.nonce.to_repr()); b.extend_from_slice(&self.pot_total.to_le_bytes()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() < 98 { return Err(ContractError::IoError("SettlePotParamsV1: too short".into())); } let caller = PublicKey::from_bytes(data[0..32].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("SettlePotParamsV1: invalid caller: {}", e)))?; let room_id = read_base(&data[32..64])?; let pot_id = read_base(&data[64..96])?; let wc = data[96] as usize; let mut pos = 97+wc*40; if data.len() < pos+1 { return Err(ContractError::IoError("SettlePotParamsV1: winners truncated".into())); } let mut winners = Vec::with_capacity(wc); for i in 0..wc { let s = 97+i*40; let pk = PublicKey::from_bytes(data[s..s+32].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("SettlePotParamsV1: invalid winner pk[{}]: {}", i, e)))?; let amt = u64::from_le_bytes(data[s+32..s+40].try_into().unwrap()); winners.push((pk, amt)); } let sig_len = data[pos] as usize; pos += 1; if data.len() < pos+sig_len+40 { return Err(ContractError::IoError("SettlePotParamsV1: signature truncated".into())); } let signature = data[pos..pos+sig_len].to_vec(); pos += sig_len; let nonce = read_base(&data[pos..pos+32])?; let pot_total = u64::from_le_bytes(data[pos+32..pos+40].try_into().unwrap()); Ok(SettlePotParamsV1 { caller, room_id, pot_id, winners, signature, nonce, pot_total }) } }

#[derive(Debug, Clone,)] pub struct ContributeEntropyParamsV1 { pub room_id: RoomId, pub player: PublicKey, pub commitment: pallas::Base, pub player_nullifier: pallas::Base, pub reveal: Option<pallas::Base> }
impl dwow_serial::Encodable for ContributeEntropyParamsV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for ContributeEntropyParamsV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl ContributeEntropyParamsV1 { pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(130); b.extend_from_slice(&self.room_id.to_repr()); b.extend_from_slice(&self.player.to_bytes()); b.extend_from_slice(&self.commitment.to_repr()); b.extend_from_slice(&self.player_nullifier.to_repr()); b.push(self.reveal.is_some() as u8); if let Some(v) = self.reveal { b.extend_from_slice(&v.to_repr()); } b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() < 129 { return Err(ContractError::IoError("ContributeEntropyParamsV1: too short".into())); } let room_id = read_base(&data[0..32])?; let player = PublicKey::from_bytes(data[32..64].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("ContributeEntropyParamsV1: invalid player: {}", e)))?; let commitment = read_base(&data[64..96])?; let player_nullifier = read_base(&data[96..128])?; let has_reveal = data[128] != 0; let reveal = if has_reveal { if data.len() != 161 { return Err(ContractError::IoError(format!("ContributeEntropyParamsV1: expected 161 bytes, got {}", data.len()))); } Some(read_base(&data[129..161])?) } else { None }; Ok(ContributeEntropyParamsV1 { room_id, player, commitment, player_nullifier, reveal }) } }

#[derive(Debug, Clone,)]
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

impl dwow_serial::Encodable for ClaimParamsV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for ClaimParamsV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl ClaimParamsV1 { pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(130+self.proof.len()); b.extend_from_slice(&self.room_id.to_repr()); b.extend_from_slice(&self.pot_id.to_repr()); b.extend_from_slice(&self.winner.to_bytes()); b.extend_from_slice(&self.payout_amount.to_le_bytes()); b.push(self.proof.len() as u8); b.extend_from_slice(&self.proof); b.extend_from_slice(&self.nonce.to_repr()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() < 130 { return Err(ContractError::IoError("ClaimParamsV1: too short".into())); } let room_id = read_base(&data[0..32])?; let pot_id = read_base(&data[32..64])?; let winner = PublicKey::from_bytes(data[64..96].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("ClaimParamsV1: invalid winner: {}", e)))?; let payout_amount = u64::from_le_bytes(data[96..104].try_into().unwrap()); let proof_len = data[104] as usize; let pos = 105+proof_len; if data.len() < pos+32 { return Err(ContractError::IoError("ClaimParamsV1: proof truncated".into())); } let proof = data[105..pos].to_vec(); let nonce = read_base(&data[pos..pos+32])?; Ok(ClaimParamsV1 { room_id, pot_id, winner, payout_amount, proof, nonce }) } }

#[derive(Debug, Clone,)] pub struct CreatePotParamsV1 { pub room_id: RoomId, pub player: PublicKey, pub nonce: pallas::Base, pub player_nullifier: pallas::Base }
impl dwow_serial::Encodable for CreatePotParamsV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for CreatePotParamsV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl CreatePotParamsV1 { pub const ENCODED_SIZE: usize = 128; pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(128); b.extend_from_slice(&self.room_id.to_repr()); b.extend_from_slice(&self.player.to_bytes()); b.extend_from_slice(&self.nonce.to_repr()); b.extend_from_slice(&self.player_nullifier.to_repr()); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() != 128 { return Err(ContractError::IoError(format!("CreatePotParamsV1: expected 128 bytes, got {}", data.len()))); } Ok(CreatePotParamsV1 { room_id: read_base(&data[0..32])?, player: PublicKey::from_bytes(data[32..64].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("CreatePotParamsV1: invalid player: {}", e)))?, nonce: read_base(&data[64..96])?, player_nullifier: read_base(&data[96..128])? }) } }

// ============================================================================
// BRIDGE UPDATE STRUCTS
// ============================================================================

/// Bridge state update for CreateRoomV1
#[derive(Debug, Clone)]
pub struct CreateRoomUpdateV1 {
    pub room: GameRoom,
}

/// Bridge state update for DepositV1
#[derive(Debug, Clone)]
pub struct DepositUpdateV1 {
    pub room_id: RoomId,
    pub account: PlayerAccount,
}

/// Bridge state update for WithdrawV1
#[derive(Debug, Clone)]
pub struct WithdrawUpdateV1 {
    pub room_id: RoomId,
    pub player: PublicKey,
    pub player_nullifier: pallas::Base,
}

/// Bridge state update for PlaceBetV1
#[derive(Debug, Clone)]
pub struct PlaceBetUpdateV1 {
    pub bet: Bet,
    pub pot: Pot,
    pub account: PlayerAccount,
    pub room: GameRoom,
}

/// Bridge state update for RaiseV1
#[derive(Debug, Clone)]
pub struct RaiseUpdateV1 {
    pub bet: Bet,
    pub pot: Pot,
    pub account: PlayerAccount,
    pub room: GameRoom,
    pub player_nullifier: pallas::Base,
}

/// Bridge state update for CallV1
#[derive(Debug, Clone)]
pub struct CallUpdateV1 {
    pub bet: Bet,
    pub pot: Pot,
    pub account: PlayerAccount,
    pub player_nullifier: pallas::Base,
}

/// Bridge state update for FoldV1
#[derive(Debug, Clone)]
pub struct FoldUpdateV1 {
    pub room_id: RoomId,
    pub account: PlayerAccount,
    pub player_nullifier: pallas::Base,
}

/// Bridge state update for ClosePotV1
#[derive(Debug, Clone)]
pub struct ClosePotUpdateV1 {
    pub pot: Pot,
    pub room: GameRoom,
    pub player_nullifier: pallas::Base,
}

/// Bridge state update for SettlePotV1
#[derive(Debug, Clone)]
pub struct SettlePotUpdateV1 {
    pub pot: Pot,
}

/// Bridge state update for ContributeEntropyV1
#[derive(Debug, Clone)]
pub struct ContributeEntropyUpdateV1 {
    pub account: PlayerAccount,
    pub room: GameRoom,
    pub player_nullifier: pallas::Base,
}

/// Bridge state update for ClaimV1
#[derive(Debug, Clone)]
pub struct ClaimUpdateV1 {
    pub room_id: RoomId,
    pub pot_id: PotId,
    pub winner: PublicKey,
    pub amount: u64,
    pub claim_nullifier: pallas::Base,
}

/// Bridge state update for CreatePotV1
#[derive(Debug, Clone)]
pub struct CreatePotUpdateV1 {
    pub pot: Pot,
    pub room: GameRoom,
    pub player_nullifier: pallas::Base,
}

// ============================================================================
// RHO-CALCULUS EXPLICIT ENCODE/DECODE
// ============================================================================

// --- Bridge update structs ---

impl dwow_serial::Encodable for CreateRoomUpdateV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for CreateRoomUpdateV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl CreateRoomUpdateV1 {
    pub fn encode(&self) -> Vec<u8> { self.room.encode() }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        Ok(CreateRoomUpdateV1 { room: GameRoom::decode(data)? })
    }
}

impl dwow_serial::Encodable for DepositUpdateV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for DepositUpdateV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl DepositUpdateV1 {
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(&self.room_id.to_repr());
        write_len_prefixed(&mut b, &self.account.encode());
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 32 {
            return Err(ContractError::IoError("DepositUpdateV1: too short".into()));
        }
        let room_id = read_base(&data[0..32])?;
        let (account_b, _) = read_len_prefixed(&data[32..])?;
        Ok(DepositUpdateV1 { room_id, account: PlayerAccount::decode(&account_b)? })
    }
}

impl dwow_serial::Encodable for WithdrawUpdateV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for WithdrawUpdateV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl WithdrawUpdateV1 {
    pub const ENCODED_SIZE: usize = 96;
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(96);
        b.extend_from_slice(&self.room_id.to_repr());
        b.extend_from_slice(&self.player.to_bytes());
        b.extend_from_slice(&self.player_nullifier.to_repr());
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 96 {
            return Err(ContractError::IoError(format!(
                "WithdrawUpdateV1: expected 96 bytes, got {}",
                data.len()
            )));
        }
        Ok(WithdrawUpdateV1 {
            room_id: read_base(&data[0..32])?,
            player: PublicKey::from_bytes(data[32..64].try_into().unwrap()).map_err(|e| {
                ContractError::IoError(format!("WithdrawUpdateV1: invalid player: {}", e))
            })?,
            player_nullifier: read_base(&data[64..96])?,
        })
    }
}

impl dwow_serial::Encodable for PlaceBetUpdateV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for PlaceBetUpdateV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl PlaceBetUpdateV1 {
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(&self.bet.encode());
        write_len_prefixed(&mut b, &self.pot.encode());
        write_len_prefixed(&mut b, &self.account.encode());
        write_len_prefixed(&mut b, &self.room.encode());
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < Bet::ENCODED_SIZE {
            return Err(ContractError::IoError("PlaceBetUpdateV1: too short".into()));
        }
        let bet = Bet::decode(&data[0..Bet::ENCODED_SIZE])?;
        let (pot_b, n1) = read_len_prefixed(&data[Bet::ENCODED_SIZE..])?;
        let (account_b, n2) = read_len_prefixed(&data[Bet::ENCODED_SIZE + n1..])?;
        let (room_b, _) = read_len_prefixed(&data[Bet::ENCODED_SIZE + n1 + n2..])?;
        Ok(PlaceBetUpdateV1 {
            bet,
            pot: Pot::decode(&pot_b)?,
            account: PlayerAccount::decode(&account_b)?,
            room: GameRoom::decode(&room_b)?,
        })
    }
}

impl dwow_serial::Encodable for RaiseUpdateV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for RaiseUpdateV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl RaiseUpdateV1 {
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(&self.bet.encode());
        write_len_prefixed(&mut b, &self.pot.encode());
        write_len_prefixed(&mut b, &self.account.encode());
        write_len_prefixed(&mut b, &self.room.encode());
        b.extend_from_slice(&self.player_nullifier.to_repr());
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < Bet::ENCODED_SIZE {
            return Err(ContractError::IoError("RaiseUpdateV1: too short".into()));
        }
        let bet = Bet::decode(&data[0..Bet::ENCODED_SIZE])?;
        let (pot_b, n1) = read_len_prefixed(&data[Bet::ENCODED_SIZE..])?;
        let (account_b, n2) = read_len_prefixed(&data[Bet::ENCODED_SIZE + n1..])?;
        let (room_b, n3) = read_len_prefixed(&data[Bet::ENCODED_SIZE + n1 + n2..])?;
        let nf_off = Bet::ENCODED_SIZE + n1 + n2 + n3;
        let player_nullifier = read_base(&data[nf_off..nf_off + 32])?;
        Ok(RaiseUpdateV1 {
            bet,
            pot: Pot::decode(&pot_b)?,
            account: PlayerAccount::decode(&account_b)?,
            room: GameRoom::decode(&room_b)?,
            player_nullifier,
        })
    }
}

impl dwow_serial::Encodable for CallUpdateV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for CallUpdateV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl CallUpdateV1 {
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(&self.bet.encode());
        write_len_prefixed(&mut b, &self.pot.encode());
        write_len_prefixed(&mut b, &self.account.encode());
        b.extend_from_slice(&self.player_nullifier.to_repr());
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < Bet::ENCODED_SIZE {
            return Err(ContractError::IoError("CallUpdateV1: too short".into()));
        }
        let bet = Bet::decode(&data[0..Bet::ENCODED_SIZE])?;
        let (pot_b, n1) = read_len_prefixed(&data[Bet::ENCODED_SIZE..])?;
        let (account_b, n2) = read_len_prefixed(&data[Bet::ENCODED_SIZE + n1..])?;
        let nf_off = Bet::ENCODED_SIZE + n1 + n2;
        let player_nullifier = read_base(&data[nf_off..nf_off + 32])?;
        Ok(CallUpdateV1 {
            bet,
            pot: Pot::decode(&pot_b)?,
            account: PlayerAccount::decode(&account_b)?,
            player_nullifier,
        })
    }
}

impl dwow_serial::Encodable for FoldUpdateV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for FoldUpdateV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl FoldUpdateV1 {
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(&self.room_id.to_repr());
        write_len_prefixed(&mut b, &self.account.encode());
        b.extend_from_slice(&self.player_nullifier.to_repr());
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 32 {
            return Err(ContractError::IoError("FoldUpdateV1: too short".into()));
        }
        let room_id = read_base(&data[0..32])?;
        let (account_b, n1) = read_len_prefixed(&data[32..])?;
        let player_nullifier = read_base(&data[32 + n1..32 + n1 + 32])?;
        Ok(FoldUpdateV1 {
            room_id,
            account: PlayerAccount::decode(&account_b)?,
            player_nullifier,
        })
    }
}

impl dwow_serial::Encodable for ClosePotUpdateV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for ClosePotUpdateV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl ClosePotUpdateV1 {
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::new();
        write_len_prefixed(&mut b, &self.pot.encode());
        write_len_prefixed(&mut b, &self.room.encode());
        b.extend_from_slice(&self.player_nullifier.to_repr());
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        let (pot_b, n1) = read_len_prefixed(data)?;
        let (room_b, n2) = read_len_prefixed(&data[n1..])?;
        let player_nullifier = read_base(&data[n1 + n2..n1 + n2 + 32])?;
        Ok(ClosePotUpdateV1 {
            pot: Pot::decode(&pot_b)?,
            room: GameRoom::decode(&room_b)?,
            player_nullifier,
        })
    }
}

impl dwow_serial::Encodable for SettlePotUpdateV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for SettlePotUpdateV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl SettlePotUpdateV1 {
    pub fn encode(&self) -> Vec<u8> { self.pot.encode() }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        Ok(SettlePotUpdateV1 { pot: Pot::decode(data)? })
    }
}

impl dwow_serial::Encodable for ContributeEntropyUpdateV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for ContributeEntropyUpdateV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl ContributeEntropyUpdateV1 {
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::new();
        write_len_prefixed(&mut b, &self.account.encode());
        write_len_prefixed(&mut b, &self.room.encode());
        b.extend_from_slice(&self.player_nullifier.to_repr());
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        let (account_b, n1) = read_len_prefixed(data)?;
        let (room_b, n2) = read_len_prefixed(&data[n1..])?;
        let player_nullifier = read_base(&data[n1 + n2..n1 + n2 + 32])?;
        Ok(ContributeEntropyUpdateV1 {
            account: PlayerAccount::decode(&account_b)?,
            room: GameRoom::decode(&room_b)?,
            player_nullifier,
        })
    }
}

impl dwow_serial::Encodable for ClaimUpdateV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for ClaimUpdateV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl ClaimUpdateV1 {
    pub const ENCODED_SIZE: usize = 136;
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(136);
        b.extend_from_slice(&self.room_id.to_repr());
        b.extend_from_slice(&self.pot_id.to_repr());
        b.extend_from_slice(&self.winner.to_bytes());
        b.extend_from_slice(&self.amount.to_le_bytes());
        b.extend_from_slice(&self.claim_nullifier.to_repr());
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 136 {
            return Err(ContractError::IoError(format!(
                "ClaimUpdateV1: expected 136 bytes, got {}",
                data.len()
            )));
        }
        Ok(ClaimUpdateV1 {
            room_id: read_base(&data[0..32])?,
            pot_id: read_base(&data[32..64])?,
            winner: PublicKey::from_bytes(data[64..96].try_into().unwrap()).map_err(|e| {
                ContractError::IoError(format!("ClaimUpdateV1: invalid winner: {}", e))
            })?,
            amount: u64::from_le_bytes(data[96..104].try_into().unwrap()),
            claim_nullifier: read_base(&data[104..136])?,
        })
    }
}

impl dwow_serial::Encodable for CreatePotUpdateV1 { fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> { let b = self.encode(); w.write_all(&b)?; Ok(b.len()) } }
impl dwow_serial::Decodable for CreatePotUpdateV1 { fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> { let mut b = vec![]; d.read_to_end(&mut b)?; Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}"))) } }
impl CreatePotUpdateV1 {
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::new();
        write_len_prefixed(&mut b, &self.pot.encode());
        write_len_prefixed(&mut b, &self.room.encode());
        b.extend_from_slice(&self.player_nullifier.to_repr());
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        let (pot_b, n1) = read_len_prefixed(data)?;
        let (room_b, n2) = read_len_prefixed(&data[n1..])?;
        let player_nullifier = read_base(&data[n1 + n2..n1 + n2 + 32])?;
        Ok(CreatePotUpdateV1 {
            pot: Pot::decode(&pot_b)?,
            room: GameRoom::decode(&room_b)?,
            player_nullifier,
        })
    }
}
