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

//! Slot Contract Model
//!
//! A composable slot machine contract with modular design.
//!
//! Architecture:
//! - Core handles bet commitment, entropy-based spins, and payout orchestration
//! - Reel strips define symbol layouts (configurable per game)
//! - Paytables define winning combinations and payouts (swappable)
//! - Extension traits for bonus rounds and special features
//!
//! This is like Baccarat where cards dealt are from entropy, but the game
//! logic (hand values, drawing rules) is separate. For slots, spin results
//! are from entropy, but symbol matching/payout logic is modular.

use dwow_sdk::{
    crypto::{pasta_prelude::PrimeField, poseidon_hash, tx_hash_to_base, PublicKey},
    error::ContractError,
    pasta::pallas,
    tx::TransactionHash,
};
use dwow_serial::{SerialDecodable, SerialEncodable};

// ============================================================================
// CORE SLOT TYPES
// ============================================================================

/// Maximum number of reels supported
pub const MAX_REELS: usize = 6;
/// Maximum symbols per reel
pub const MAX_SYMBOLS_PER_REEL: usize = 128;
/// Maximum paylines
pub const MAX_PAYLINES: usize = 100;
/// Standard reel count for classic slots
pub const CLASSIC_REEL_COUNT: usize = 3;
/// Standard reel count for video slots
pub const VIDEO_REEL_COUNT: usize = 5;

/// Symbol ID (0-255 allows for various symbol sets)
#[derive(Debug, Clone, Copy, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub struct Symbol(pub u8);

impl Symbol {
    /// Create a new symbol
    pub fn new(value: u8) -> Self {
        Self(value)
    }

    /// Wild symbol (matches any symbol for wins)
    pub const WILD: Symbol = Symbol(10);

    /// Scatter symbol (triggers bonus)
    pub const SCATTER: Symbol = Symbol(11);

    /// Blank symbol
    pub const BLANK: Symbol = Symbol(0);
}

/// A reel strip (sequence of symbols that cycles)
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ReelStrip {
    /// Symbols on this reel (cycled during spin)
    pub symbols: Vec<Symbol>,
}

impl ReelStrip {
    /// Create a new reel strip
    pub fn new(symbols: Vec<Symbol>) -> Self {
        Self { symbols }
    }

    /// Get symbol at position (wraps around)
    pub fn get(&self, position: u64) -> Symbol {
        if self.symbols.is_empty() {
            return Symbol::BLANK
        }
        let idx = (position as usize) % self.symbols.len();
        self.symbols[idx]
    }

    /// Get reel length
    pub fn len(&self) -> usize {
        self.symbols.len()
    }
}

/// A single spin result (one position per reel)
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct SpinResult {
    /// Positions for each reel (used to look up symbols from reel strips)
    pub positions: Vec<u64>,
}

impl SpinResult {
    /// Create a new spin result from positions
    pub fn new(positions: Vec<u64>) -> Self {
        Self { positions }
    }

    /// Get number of reels
    pub fn reel_count(&self) -> usize {
        self.positions.len()
    }
}

/// Payline definition (which positions form a line)
/// Format: indices into the visible window per reel
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Payline {
    /// Payline ID
    pub id: u32,
    /// Row indices per reel (0=top, 1=middle, 2=bottom for 3-row display)
    pub rows: Vec<u8>,
}

impl Payline {
    /// Create a new payline
    pub fn new(id: u32, rows: Vec<u8>) -> Self {
        Self { id, rows }
    }

    /// Standard horizontal middle line (all row 1)
    pub fn horizontal_middle(num_reels: usize) -> Self {
        Self { id: 0, rows: vec![1; num_reels] }
    }

    /// Horizontal top line (all row 0)
    pub fn horizontal_top(num_reels: usize) -> Self {
        Self { id: 1, rows: vec![0; num_reels] }
    }

    /// Horizontal bottom line (all row 2)
    pub fn horizontal_bottom(num_reels: usize) -> Self {
        Self { id: 2, rows: vec![2; num_reels] }
    }
}

/// A winning combination
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Win {
    /// Payline ID
    pub payline_id: u32,
    /// Symbol that won
    pub symbol: Symbol,
    /// Number of consecutive matching symbols
    pub count: u8,
    /// Payout multiplier (bet * multiplier = win amount)
    pub multiplier: u64,
}

// ============================================================================
// GAME CONFIGURATION
// ============================================================================

/// Game configuration (set during initialization)
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct GameConfig {
    pub version: u8,
    /// Number of reels
    pub reel_count: usize,
    /// Number of visible rows per reel
    pub row_count: usize,
    /// Reel strips for each reel
    pub reels: Vec<ReelStrip>,
    /// Active paylines
    pub paylines: Vec<Payline>,
    /// House edge in basis points (e.g., 500 = 5%)
    pub house_edge: u32,
}

/// Paytable entry
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct PaytableEntry {
    /// Symbol
    pub symbol: Symbol,
    /// Number of symbols needed
    pub count: u8,
    /// Multiplier (bet * multiplier = win)
    pub multiplier: u64,
}

/// A complete paytable (defines all winning combinations)
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Paytable {
    /// Entries sorted by count descending
    pub entries: Vec<PaytableEntry>,
}

impl Paytable {
    /// Create a new paytable
    pub fn new(entries: Vec<PaytableEntry>) -> Self {
        Self { entries }
    }

    /// Look up multiplier for symbol + count combination
    pub fn lookup(&self, symbol: Symbol, count: u8) -> Option<u64> {
        self.entries
            .iter()
            .find(|e| e.symbol == symbol && e.count == count)
            .map(|e| e.multiplier)
    }
}

// ============================================================================
// PAYTABLES FOR DIFFERENT SLOT VARIANTS
// ============================================================================

/// Classic 3-reel single-line slot (high RTP, simple)
pub mod classic_paytable {
    use super::*;

    pub fn create() -> Paytable {
        Paytable::new(vec![
            // 3x BAR pays 100x
            PaytableEntry { symbol: Symbol(1), count: 3, multiplier: 100 },
            // 3x 7 pays 50x
            PaytableEntry { symbol: Symbol(7), count: 3, multiplier: 50 },
            // 3x cherry pays 20x
            PaytableEntry { symbol: Symbol(2), count: 3, multiplier: 20 },
            // 2x cherry pays 5x
            PaytableEntry { symbol: Symbol(2), count: 2, multiplier: 5 },
            // Any 2x BAR pays 10x
            PaytableEntry { symbol: Symbol(1), count: 2, multiplier: 10 },
        ])
    }

    pub fn default_reels() -> Vec<ReelStrip> {
        vec![
            // Reel 1: weighted toward lower wins
            ReelStrip::new(vec![
                Symbol(1), Symbol(2), Symbol(2), Symbol(3), Symbol(3),
                Symbol(4), Symbol(4), Symbol(4), Symbol(5), Symbol(5),
                Symbol(5), Symbol(5), Symbol(6), Symbol(6), Symbol(7),
            ]),
            // Reel 2
            ReelStrip::new(vec![
                Symbol(2), Symbol(2), Symbol(3), Symbol(3), Symbol(3),
                Symbol(4), Symbol(4), Symbol(4), Symbol(5), Symbol(5),
                Symbol(5), Symbol(6), Symbol(6), Symbol(7), Symbol(1),
            ]),
            // Reel 3
            ReelStrip::new(vec![
                Symbol(1), Symbol(3), Symbol(4), Symbol(4), Symbol(5),
                Symbol(5), Symbol(5), Symbol(6), Symbol(6), Symbol(7),
                Symbol(7), Symbol(2), Symbol(2), Symbol(3), Symbol(1),
            ]),
        ]
    }
}

/// Video slot 5-reel multi-payline (more features, lower RTP)
pub mod video_paytable {
    use super::*;

    pub fn create() -> Paytable {
        Paytable::new(vec![
            // 5x WILD pays 1000x
            PaytableEntry { symbol: Symbol::WILD, count: 5, multiplier: 1000 },
            // 5x SCATTER pays 100x (also triggers bonus)
            PaytableEntry { symbol: Symbol::SCATTER, count: 5, multiplier: 100 },
            // 5x A pays 200x
            PaytableEntry { symbol: Symbol(12), count: 5, multiplier: 200 },
            // 4x A pays 50x
            PaytableEntry { symbol: Symbol(12), count: 4, multiplier: 50 },
            // 3x A pays 20x
            PaytableEntry { symbol: Symbol(12), count: 3, multiplier: 20 },
            // 5x K pays 100x
            PaytableEntry { symbol: Symbol(13), count: 5, multiplier: 100 },
            // 4x K pays 40x
            PaytableEntry { symbol: Symbol(13), count: 4, multiplier: 40 },
            // 3x K pays 15x
            PaytableEntry { symbol: Symbol(13), count: 3, multiplier: 15 },
            // 5x Q pays 75x
            PaytableEntry { symbol: Symbol(14), count: 5, multiplier: 75 },
            // 4x Q pays 30x
            PaytableEntry { symbol: Symbol(14), count: 4, multiplier: 30 },
            // 3x Q pays 10x
            PaytableEntry { symbol: Symbol(14), count: 3, multiplier: 10 },
            // 5x J pays 50x
            PaytableEntry { symbol: Symbol(15), count: 5, multiplier: 50 },
            // 4x J pays 20x
            PaytableEntry { symbol: Symbol(15), count: 4, multiplier: 20 },
            // 3x J pays 8x
            PaytableEntry { symbol: Symbol(15), count: 3, multiplier: 8 },
            // 5x 10 pays 40x
            PaytableEntry { symbol: Symbol(10), count: 5, multiplier: 40 },
            // 4x 10 pays 15x
            PaytableEntry { symbol: Symbol(10), count: 4, multiplier: 15 },
            // 3x 10 pays 5x
            PaytableEntry { symbol: Symbol(10), count: 3, multiplier: 5 },
            // 3x SCATTER pays 5x (bonus trigger)
            PaytableEntry { symbol: Symbol::SCATTER, count: 3, multiplier: 5 },
        ])
    }

    pub fn default_reels() -> Vec<ReelStrip> {
        vec![
            // Reel 1: WILD, high symbols, more blanks
            ReelStrip::new(vec![
                Symbol::BLANK, Symbol::BLANK, Symbol(12), Symbol(13),
                Symbol(14), Symbol(15), Symbol(10), Symbol::WILD,
                Symbol(12), Symbol(13), Symbol::BLANK, Symbol(14),
                Symbol(15), Symbol::SCATTER, Symbol(10), Symbol(12),
            ]),
            // Reel 2
            ReelStrip::new(vec![
                Symbol(13), Symbol::BLANK, Symbol(12), Symbol(14),
                Symbol(15), Symbol::SCATTER, Symbol(10), Symbol::BLANK,
                Symbol(13), Symbol(12), Symbol(14), Symbol::WILD,
                Symbol(15), Symbol(13), Symbol(10), Symbol::BLANK,
            ]),
            // Reel 3
            ReelStrip::new(vec![
                Symbol(14), Symbol(15), Symbol::SCATTER, Symbol::BLANK,
                Symbol(13), Symbol(12), Symbol(10), Symbol(14),
                Symbol::BLANK, Symbol::WILD, Symbol(15), Symbol(13),
                Symbol::SCATTER, Symbol(12), Symbol(10), Symbol::BLANK,
            ]),
            // Reel 4
            ReelStrip::new(vec![
                Symbol::WILD, Symbol(12), Symbol::BLANK, Symbol(13),
                Symbol(14), Symbol(15), Symbol::SCATTER, Symbol(10),
                Symbol::BLANK, Symbol(12), Symbol(13), Symbol::WILD,
                Symbol(14), Symbol(15), Symbol::BLANK, Symbol(10),
            ]),
            // Reel 5
            ReelStrip::new(vec![
                Symbol(15), Symbol(10), Symbol::BLANK, Symbol::SCATTER,
                Symbol(12), Symbol(13), Symbol(14), Symbol(15),
                Symbol::WILD, Symbol(10), Symbol::BLANK, Symbol(12),
                Symbol::SCATTER, Symbol(13), Symbol(14), Symbol::BLANK,
            ]),
        ]
    }
}

// ============================================================================
// BET AND SPIN STATE
// ============================================================================

/// Spin state enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub enum SpinState {
    /// Spin has been committed, waiting for reveal
    Committed = 0,
    /// Cards have been revealed (positions determined)
    Revealed = 1,
    /// Spin has been settled (payout calculated)
    Settled = 2,
    /// Spin was cancelled (timeout)
    Cancelled = 3,
}

/// Unique spin identifier (Poseidon hash)
pub type SpinId = pallas::Base;

/// A spin/bet stored on-chain
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Spin {
    pub version: u8,
    /// Unique spin ID
    pub id: SpinId,
    /// Player's public key
    pub player_pub: PublicKey,
    /// Bet value (amount wagered)
    pub bet_value: u64,
    /// Paylines played (bitmask or count)
    pub paylines_played: u32,
    /// Player's secret nonce commitment for randomness (Poseidon hash of secret_nonce)
    pub secret_nonce_commit: pallas::Base,
    /// Blinding factor
    pub blind: pallas::Base,
    /// Spin result (positions per reel)
    pub result: Option<SpinResult>,
    /// Winning combinations found
    pub wins: Vec<Win>,
    /// Total payout
    pub payout: u64,
    /// Current spin state
    pub state: SpinState,
    /// House edge in basis points
    pub house_edge: u32,
    /// Confirmation depth for randomness
    pub confirmation_depth: u8,
    /// Block height when spin was created
    pub created_at: u64,
    /// Earliest block to settle
    pub settle_block: u64,
    /// Pedersen commitment to bet_value
    pub value_commit: pallas::Point,
    /// Token ID being wagered
    pub token_id: pallas::Base,
    /// Nullifier for double-spend prevention
    pub nullifier: SpinId,
    pub instance_seed: [u8; 32],
}

impl Spin {
    /// Derive nullifier for this spin
    pub fn derive_nullifier(&self) -> SpinId {
        poseidon_hash([self.id, self.secret_nonce_commit])
    }
}

// ============================================================================
// PARAMS AND UPDATES
// ============================================================================

/// Parameters for CommitSpinV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CommitSpinParamsV1 {
    /// Player's public key
    pub player_pub: PublicKey,
    /// Bet value (amount wagered per line * paylines)
    pub bet_value: u64,
    /// Number of paylines to play
    pub paylines_played: u32,
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
    pub instance_seed: [u8; 32],
}

/// Update produced by CommitSpinV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CommitSpinUpdateV1 {
    pub spin_id: SpinId,
    pub player_pub: PublicKey,
    pub bet_value: u64,
    pub paylines_played: u32,
    pub secret_nonce_commit: pallas::Base,
    pub blind: pallas::Base,
    pub house_edge: u32,
    pub confirmation_depth: u8,
    pub token_id: pallas::Base,
    pub value_commit: pallas::Point,
    pub settle_block: u64,
    pub nullifier: SpinId,
    pub state: SpinState,
    pub created_at: u64,
    pub instance_seed: [u8; 32],
}

/// Parameters for RevealSpinV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct RevealSpinParamsV1 {
    /// Spin ID to reveal
    pub spin_id: SpinId,
    /// Secret nonce (for verification)
    pub secret_nonce: pallas::Base,
}

/// Update produced by RevealSpinV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct RevealSpinUpdateV1 {
    pub spin_id: SpinId,
    pub positions: Vec<u64>,
    pub state: SpinState,
}

/// Parameters for SettleSpinV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct SettleSpinParamsV1 {
    /// Spin ID to settle
    pub spin_id: SpinId,
    /// Expected payout (must match ZK circuit public input)
    pub payout: u64,
}

/// Update produced by SettleSpinV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct SettleSpinUpdateV1 {
    pub spin_id: SpinId,
    pub wins: Vec<Win>,
    pub payout: u64,
    pub state: SpinState,
}

/// Parameters for CancelSpinV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CancelSpinParamsV1 {
    /// Spin ID to cancel
    pub spin_id: SpinId,
}

/// Update produced by CancelSpinV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CancelSpinUpdateV1 {
    pub spin_id: SpinId,
    pub house_take: u64,
    pub state: SpinState,
}

// ============================================================================
// CORE GAME LOGIC (ENTROPY AND PAYOUTS)
// ============================================================================

/// Derive spin positions using block hash entropy
/// Returns a position for each reel
pub fn derive_spin_positions(
    block_hashes: &[TransactionHash],
    spin_id: SpinId,
    _secret_nonce: pallas::Base,
    num_reels: usize,
) -> Vec<u64> {
    // Combine entropy from block hashes
    let mut entropy = spin_id;
    for (i, hash) in block_hashes.iter().enumerate() {
        let block_entropy = tx_hash_to_base(&hash.0);
        entropy = poseidon_hash([entropy, block_entropy, pallas::Base::from(i as u64)]);
    }

    // Use entropy to seed positions for each reel
    let bytes = entropy.to_repr();

    // Create seeds for each reel from entropy bytes
    let mut positions = Vec::with_capacity(num_reels);
    for i in 0..num_reels {
        // Use different bytes for each reel
        let start = (i * 4) % 32;
        let seed_base = if start + 8 <= 32 {
            u64::from_le_bytes([
                bytes[start], bytes[start + 1], bytes[start + 2], bytes[start + 3],
                bytes[start + 4], bytes[start + 5], bytes[start + 6], bytes[start + 7],
            ])
        } else {
            0u64
        };

        // Derive final position using hash with reel index
        let reel_entropy = poseidon_hash([entropy, pallas::Base::from(i as u64)]);
        let reel_bytes = reel_entropy.to_repr();
        let position_seed = u64::from_le_bytes([
            reel_bytes[0], reel_bytes[1], reel_bytes[2], reel_bytes[3],
            reel_bytes[4], reel_bytes[5], reel_bytes[6], reel_bytes[7],
        ]);

        // Combine seeds and take modulo to get position
        let position = seed_base.wrapping_mul(31).wrapping_add(position_seed);
        positions.push(position);
    }

    positions
}

/// Calculate wins for a spin result given a paytable and paylines
/// Returns all winning combinations found
pub fn calculate_wins(
    result: &SpinResult,
    reel_strips: &[ReelStrip],
    paylines: &[Payline],
    paytable: &Paytable,
) -> Vec<Win> {
    let mut wins = Vec::new();

    // For each payline, check for wins
    for payline in paylines {
        if payline.rows.len() != result.positions.len() {
            continue // Skip paylines with wrong reel count
        }

        // Get symbols along this payline
        let mut symbols_on_line = Vec::new();
        for (reel_idx, &position) in result.positions.iter().enumerate() {
            let reel_strip = &reel_strips[reel_idx];
            let symbol = reel_strip.get(position + payline.rows[reel_idx] as u64);
            symbols_on_line.push(symbol);
        }

        // Find consecutive matching symbols (from left)
        let mut count = 0;
        let mut winning_symbol = symbols_on_line[0];

        for symbol in &symbols_on_line {
            // Wild matches anything
            if *symbol == Symbol::WILD || *symbol == winning_symbol || winning_symbol == Symbol::WILD {
                if winning_symbol == Symbol::BLANK {
                    winning_symbol = *symbol;
                }
                count += 1;
            } else {
                // Check if we had a win before breaking
                if count >= 3 {
                    if let Some(multiplier) = paytable.lookup(winning_symbol, count) {
                        wins.push(Win {
                            payline_id: payline.id,
                            symbol: winning_symbol,
                            count,
                            multiplier,
                        });
                    }
                }
                // Reset for new symbol run
                winning_symbol = *symbol;
                count = 1;
            }
        }

        // Check final run
        if count >= 3 {
            if let Some(multiplier) = paytable.lookup(winning_symbol, count) {
                wins.push(Win {
                    payline_id: payline.id,
                    symbol: winning_symbol,
                    count,
                    multiplier,
                });
            }
        }
    }

    wins
}

/// Calculate total payout for all wins
/// Applies house edge
pub fn calculate_payout(bet_value: u64, wins: &[Win], house_edge: u32) -> u64 {
    // Sum multipliers
    let total_multiplier: u64 = wins.iter().map(|w| w.multiplier).sum();

    // Calculate gross payout
    let gross_payout = (bet_value as u128).saturating_mul(total_multiplier as u128);

    // Apply house edge: payout = gross * (10000 - house_edge) / 10000
    let net_payout = gross_payout *
        ((10000u32.saturating_sub(house_edge)) as u128) /
        10000;

    net_payout as u64
}

/// Calculate house's take when player loses
pub fn calculate_house_take(bet_value: u64, house_edge: u32) -> u64 {
    // House takes: bet_value * house_edge / 10000
    ((bet_value as u128).saturating_mul(house_edge as u128) / 10000) as u64
}

/// Derive spin ID from parameters
pub fn derive_spin_id(
    player_pub: &PublicKey,
    bet_value: u64,
    paylines: u32,
    secret_nonce: pallas::Base,
    blind: pallas::Base,
    token_id: pallas::Base,
) -> SpinId {
    poseidon_hash([
        player_pub.x().expect("pk not identity"),
        player_pub.y().expect("pk not identity"),
        pallas::Base::from(bet_value),
        pallas::Base::from(paylines as u64),
        secret_nonce,
        blind,
        token_id,
    ])
}