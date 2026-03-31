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

//! Baccarat Contract Model
//!
//! Data structures for Baccarat game state, card handling, and outcome calculation.

use darkfi_sdk::{
    crypto::{pasta_prelude::PrimeField, poseidon_hash, tx_hash_to_base, PublicKey},
    pasta::pallas,
    tx::TransactionHash,
};
use darkfi_serial::{SerialDecodable, SerialEncodable};

// ============================================================================
// CARD AND HAND TYPES
// ============================================================================

/// Card represented as u8: 0-51
/// 0-12: Clubs (2,3,4,5,6,7,8,9,10,J,Q,K,A)
/// 13-25: Diamonds
/// 26-38: Hearts
/// 39-51: Spades
#[derive(Debug, Clone, Copy, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub struct Card(pub u8);

impl Card {
    /// Create a new card (0-51)
    pub fn new(value: u8) -> Self {
        Self(value % 52)
    }

    /// Get card rank (0-12): 0=2, 1=3, ..., 8=10, 9=J, 10=Q, 11=K, 12=A
    pub fn rank(&self) -> u8 {
        self.0 % 13
    }

    /// Get card suit (0-3): 0=Clubs, 1=Diamonds, 2=Hearts, 3=Spades
    #[allow(clippy::unused)]
    pub fn suit(&self) -> u8 {
        self.0 / 13
    }

    /// Get the baccarat value of this card (2-9 face value, 10/J/Q/K=0, A=1)
    pub fn baccarat_value(&self) -> u8 {
        let r = self.rank();
        if r >= 9 {
            // 10, J, Q, K = 0
            0
        } else {
            // 2-9 = face value
            r + 2
        }
    }
}

/// Baccarat hand (2-3 cards)
#[derive(Debug, Clone, Copy, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub struct Hand {
    pub card1: Card,
    pub card2: Card,
    pub third_card: Option<Card>,
}

impl Hand {
    /// Calculate hand value (sum of cards % 10)
    pub fn value(&self) -> u8 {
        let mut sum = self.card1.baccarat_value() + self.card2.baccarat_value();
        if let Some(c) = self.third_card {
            sum += c.baccarat_value();
        }
        sum % 10
    }
}

// ============================================================================
// BET TYPES
// ============================================================================

/// Bet type enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub enum BetType {
    Player = 0,
    Banker = 1,
    Tie = 2,
}

impl BetType {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Player),
            1 => Some(Self::Banker),
            2 => Some(Self::Tie),
            _ => None,
        }
    }
}

/// Game outcome
#[derive(Debug, Clone, Copy, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub enum Outcome {
    Player = 0,
    Banker = 1,
    Tie = 2,
}

impl Outcome {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Player),
            1 => Some(Self::Banker),
            2 => Some(Self::Tie),
            _ => None,
        }
    }
}

/// Bet state enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub enum BetState {
    Committed = 0,
    CardsDrawn = 1,
    Settled = 2,
    Cancelled = 3,
}

/// Unique bet identifier (Poseidon hash)
pub type BetId = pallas::Base;

/// Bet structure stored on-chain
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Bet {
    /// Unique bet ID
    pub id: BetId,
    /// Player's public key
    pub player_pub: PublicKey,
    /// What the player bet on
    pub bet_type: BetType,
    /// Amount wagered
    pub bet_value: u64,
    /// Player's secret nonce for randomness
    pub secret_nonce: pallas::Base,
    /// Blinding factor for commitment
    pub blind: pallas::Base,
    /// Player's initial 2 cards
    pub player_hand: Option<[Card; 2]>,
    /// Banker's initial 2 cards
    pub banker_hand: Option<[Card; 2]>,
    /// Player's third card (if drawn)
    pub player_third_card: Option<Card>,
    /// Banker's third card (if drawn)
    pub banker_third_card: Option<Card>,
    /// Resulting outcome
    pub outcome: Option<Outcome>,
    /// Current bet state
    pub state: BetState,
    /// House edge in basis points
    pub house_edge: u32,
    /// Confirmation depth for randomness
    pub confirmation_depth: u8,
    /// Block height when bet was created
    pub created_at: u64,
    /// Earliest block to settle
    pub settle_block: u64,
    /// Pedersen commitment to bet_value
    pub value_commit: pallas::Point,
    /// Token ID being wagered
    pub token_id: pallas::Base,
    /// Nullifier for double-spend prevention
    pub nullifier: BetId,
}

impl Bet {
    /// Derive nullifier for this bet
    pub fn derive_nullifier(&self) -> BetId {
        poseidon_hash([self.id, self.secret_nonce])
    }
}

/// Derive nullifier from bet_id and secret_nonce
pub fn derive_nullifier(bet_id: BetId, secret_nonce: pallas::Base) -> BetId {
    poseidon_hash([bet_id, secret_nonce])
}

// ============================================================================
// PARAMS AND UPDATES
// ============================================================================

/// Parameters for CommitBetV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CommitBetParamsV1 {
    /// Player's public key
    pub player_pub: PublicKey,
    /// Bet type (0=Player, 1=Banker, 2=Tie)
    pub bet_type: u8,
    /// Bet value (amount wagered)
    pub bet_value: u64,
    /// Secret nonce for randomness
    pub secret_nonce: pallas::Base,
    /// Blinding factor
    pub blind: pallas::Base,
    /// House edge in basis points
    pub house_edge: u32,
    /// Confirmation depth
    pub confirmation_depth: u8,
    /// Token ID
    pub token_id: pallas::Base,
    /// Value commitment
    pub value_commit: pallas::Point,
}

impl CommitBetParamsV1 {
    /// Get bet type
    pub fn get_bet_type(&self) -> Option<BetType> {
        BetType::from_u8(self.bet_type)
    }
}

/// Update produced by CommitBetV1 - contains all info needed to reconstruct bet
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CommitBetUpdateV1 {
    pub bet_id: BetId,
    pub player_pub: PublicKey,
    pub bet_type: BetType,
    pub bet_value: u64,
    pub secret_nonce: pallas::Base,
    pub blind: pallas::Base,
    pub house_edge: u32,
    pub confirmation_depth: u8,
    pub token_id: pallas::Base,
    pub value_commit: pallas::Point,
    pub settle_block: u64,
    pub nullifier: BetId,
    pub state: BetState,
    pub created_at: u64,
}

/// Parameters for DrawCardsV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct DrawCardsParamsV1 {
    /// Bet ID to draw cards for
    pub bet_id: BetId,
    /// Secret nonce (for verification)
    pub secret_nonce: pallas::Base,
}

/// Update produced by DrawCardsV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct DrawCardsUpdateV1 {
    pub bet_id: BetId,
    pub player_card1: Card,
    pub player_card2: Card,
    pub banker_card1: Card,
    pub banker_card2: Card,
    pub player_third_card: Option<Card>,
    pub banker_third_card: Option<Card>,
    pub outcome: Outcome,
    pub state: BetState,
}

/// Parameters for SettleBetV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct SettleBetParamsV1 {
    /// Bet ID to settle
    pub bet_id: BetId,
}

/// Update produced by SettleBetV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct SettleBetUpdateV1 {
    pub bet_id: BetId,
    pub payout: u64,
    pub state: BetState,
}

/// Parameters for HouseCloseV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct HouseCloseParamsV1 {
    /// Bet ID to close
    pub bet_id: BetId,
}

/// Update produced by HouseCloseV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct HouseCloseUpdateV1 {
    pub bet_id: BetId,
    pub house_take: u64,
    pub state: BetState,
}

// ============================================================================
// CARD DEALING AND OUTCOME CALCULATION
// ============================================================================

/// Deal cards using block hash entropy
/// Returns (player_hand, banker_hand, player_third_card, banker_third_card)
/// Third cards are returned separately because their dealing depends on drawing rules
#[allow(clippy::unused)]
pub fn deal_cards(block_hashes: &[TransactionHash], bet_id: BetId) -> (Hand, Hand, Option<Card>, Option<Card>) {
    // Combine entropy from block hashes
    let mut entropy = bet_id;
    for (i, hash) in block_hashes.iter().enumerate() {
        let block_entropy = tx_hash_to_base(&hash.0);
        entropy = poseidon_hash([entropy, block_entropy, pallas::Base::from(i as u64)]);
    }

    // Use entropy to seed card selection
    let bytes = entropy.to_repr();

    // Create seeds from entropy bytes
    let seed1 = u64::from_le_bytes(bytes[0..8].try_into().unwrap());
    let seed2 = u64::from_le_bytes(bytes[8..16].try_into().unwrap());
    let seed3 = u64::from_le_bytes(bytes[16..24].try_into().unwrap());
    let seed4 = u64::from_le_bytes(bytes[24..32].try_into().unwrap());

    // Create seeds for third cards (derived from entropy of first 4 cards)
    let seed5 = seed1.wrapping_mul(31).wrapping_add(seed2);
    let seed6 = seed3.wrapping_mul(17).wrapping_add(seed4);

    // For simplicity, we use modulo 52 to select cards
    let player_card1 = Card::new(seed1 as u8);
    let player_card2 = Card::new(seed2 as u8);
    let banker_card1 = Card::new(seed3 as u8);
    let banker_card2 = Card::new(seed4 as u8);

    // Third cards - only drawn if rules require them
    let player_third = Card::new(seed5 as u8);
    let banker_third = Card::new(seed6 as u8);

    (Hand { card1: player_card1, card2: player_card2, third_card: None },
     Hand { card1: banker_card1, card2: banker_card2, third_card: None },
     Some(player_third),
     Some(banker_third))
}

/// Calculate baccarat outcome using standard drawing rules
/// Takes pre-derived third cards from entropy
#[allow(clippy::assigning_clones, clippy::let_and_return)]
pub fn calculate_outcome(
    player_hand: &mut Hand,
    banker_hand: &mut Hand,
    player_third_card: Option<Card>,
    banker_third_card: Option<Card>,
) -> Outcome {
    let player_value = player_hand.value();
    let banker_value = banker_hand.value();

    // Natural 8 or 9 wins immediately
    if player_value >= 8 || banker_value >= 8 {
        return if player_value > banker_value {
            Outcome::Player
        } else if banker_value > player_value {
            Outcome::Banker
        } else {
            Outcome::Tie
        }
    }

    // Player draws third card if 0-5
    if player_value <= 5 {
        // Player draws - use entropy-derived card
        player_hand.third_card = player_third_card;
    }

    // Banker drawing rules (complex):
    // 0-2: Always draw
    // 3: Draw if player drew (any third card)
    // 4: Draw if player drew 2-7
    // 5: Draw if player drew 4-7
    // 6-7: Stand

    let player_third_val = player_hand.third_card.map(|c| c.baccarat_value());

    let should_banker_draw = match banker_value {
        0 | 1 | 2 => true,
        3 => player_third_val.is_some(), // Draw if player drew
        4 => {
            if let Some(v) = player_third_val {
                v >= 2 && v <= 7 // Draw if 2-7
            } else {
                false
            }
        }
        5 => {
            if let Some(v) = player_third_val {
                v >= 4 && v <= 7 // Draw if 4-7
            } else {
                false
            }
        }
        6 | 7 => false, // Stand
        _ => false,
    };

    if should_banker_draw {
        banker_hand.third_card = banker_third_card;
    }

    // Calculate final values
    let final_player = player_hand.value();
    let final_banker = banker_hand.value();

    if final_player > final_banker {
        Outcome::Player
    } else if final_banker > final_player {
        Outcome::Banker
    } else {
        Outcome::Tie
    }
}

/// Calculate payout for a winning bet using actual house edge
#[allow(clippy::let_and_return)]
pub fn calculate_payout(bet: &Bet, outcome: Outcome) -> u64 {
    // Check if bet won
    let won = match (bet.bet_type, outcome) {
        (BetType::Player, Outcome::Player) => true,
        (BetType::Banker, Outcome::Banker) => true,
        (BetType::Tie, Outcome::Tie) => true,
        _ => false,
    };

    if !won {
        return 0
    }

    // Calculate payout based on bet type, using actual house_edge
    // house_edge is in basis points (e.g., 150 = 1.5%)
    match bet.bet_type {
        BetType::Player => {
            // Player bet pays 1:1, house edge applied to player
            // payout = bet_value * (10000 - house_edge) / 10000
            let payout = (bet.bet_value as u128) *
                ((10000 - bet.house_edge) as u128) /
                10000;
            payout as u64
        }
        BetType::Banker => {
            // Banker bet pays 0.95:1 with ~1.06% house edge
            // Standard payout is 0.95:1, but we use house_edge for precision
            // payout = bet_value * 9500 / 10000 * (10000 - house_edge) / 10000
            // Simplified: bet_value * (9500 - house_edge*0.5) / 10000
            let payout = (bet.bet_value as u128) *
                ((9500u32.saturating_sub(bet.house_edge / 2)) as u128) /
                10000;
            payout as u64
        }
        BetType::Tie => {
            // Tie bet pays 8:1 with ~14.36% house edge
            // Standard payout is 8:1, house edge is much higher for tie
            // payout = bet_value * 8000 / 1000 * (10000 - house_edge) / 10000
            // Simplified: bet_value * (8000 - house_edge*8) / 1000
            let payout = (bet.bet_value as u128) *
                ((8000u32.saturating_sub(bet.house_edge * 8)) as u128) /
                1000;
            payout as u64
        }
    }
}

/// Calculate house's take when player loses
#[allow(clippy::let_and_return)]
pub fn calculate_house_take(bet: &Bet) -> u64 {
    bet.bet_value
}

/// Derive bet ID from parameters
pub fn derive_bet_id(
    player_pub: &PublicKey,
    bet_type: u8,
    bet_value: u64,
    secret_nonce: pallas::Base,
    blind: pallas::Base,
    token_id: pallas::Base,
) -> BetId {
    poseidon_hash([
        player_pub.x(),
        player_pub.y(),
        pallas::Base::from(bet_type as u64),
        pallas::Base::from(bet_value),
        secret_nonce,
        blind,
        token_id,
    ])
}
