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
    pasta::{group::GroupEncoding, pallas},
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

    pub fn encode(&self) -> Vec<u8> { vec![self.0] }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.is_empty() { return Err(ContractError::IoError("Symbol: empty".into())); }
        Ok(Symbol(data[0]))
    }
}

/// A reel strip (sequence of symbols that cycles)
#[derive(Debug, Clone)]
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
#[derive(Debug, Clone)]
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
#[derive(Debug, Clone)]
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
#[derive(Debug, Clone)]
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
#[derive(Debug, Clone)]
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
#[derive(Debug, Clone)]
pub struct PaytableEntry {
    /// Symbol
    pub symbol: Symbol,
    /// Number of symbols needed
    pub count: u8,
    /// Multiplier (bet * multiplier = win)
    pub multiplier: u64,
}

/// A complete paytable (defines all winning combinations)
#[derive(Debug, Clone)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

impl TryFrom<u8> for SpinState {
    type Error = ContractError;
    fn try_from(v: u8) -> Result<Self, Self::Error> {
        match v {
            0 => Ok(Self::Committed),
            1 => Ok(Self::Revealed),
            2 => Ok(Self::Settled),
            3 => Ok(Self::Cancelled),
            _ => Err(ContractError::IoError("SpinState: invalid discriminant".into())),
        }
    }
}

/// Unique spin identifier (Poseidon hash)
pub type SpinId = pallas::Base;

/// A spin/bet stored on-chain
#[derive(Debug, Clone)]
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
#[derive(Debug, Clone)]
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
#[derive(Debug, Clone)]
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
#[derive(Debug, Clone)]
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
#[derive(Debug, Clone)]
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

// ============================================================================
// RHO-CALCULUS EXPLICIT ENCODE/DECODE
// ============================================================================

impl ReelStrip {
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(1 + self.symbols.len());
        b.push(self.symbols.len() as u8);
        for s in &self.symbols { b.push(s.0); }
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.is_empty() { return Err(ContractError::IoError("ReelStrip: empty data".into())); }
        let n = data[0] as usize;
        if data.len() != 1 + n { return Err(ContractError::IoError(format!("ReelStrip: expected {} bytes, got {}", 1 + n, data.len()))); }
        Ok(ReelStrip { symbols: data[1..1+n].iter().map(|&b| Symbol(b)).collect() })
    }
}

impl Payline {
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(5 + self.rows.len());
        b.extend_from_slice(&self.id.to_le_bytes());
        b.push(self.rows.len() as u8);
        b.extend_from_slice(&self.rows);
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 5 { return Err(ContractError::IoError(format!("Payline: expected at least 5 bytes, got {}", data.len()))); }
        let id = u32::from_le_bytes(data[0..4].try_into().unwrap());
        let n = data[4] as usize;
        if data.len() != 5 + n { return Err(ContractError::IoError(format!("Payline: expected {} bytes, got {}", 5 + n, data.len()))); }
        Ok(Payline { id, rows: data[5..5+n].to_vec() })
    }
}

impl PaytableEntry {
    pub const ENCODED_SIZE: usize = 10;
    pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(10); b.push(self.symbol.0); b.push(self.count); b.extend_from_slice(&self.multiplier.to_le_bytes()); b }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 10 { return Err(ContractError::IoError(format!("PaytableEntry: expected 10 bytes, got {}", data.len()))); }
        Ok(PaytableEntry { symbol: Symbol(data[0]), count: data[1], multiplier: u64::from_le_bytes(data[2..10].try_into().unwrap()) })
    }
}

impl Paytable {
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(1 + self.entries.len() * 10);
        b.push(self.entries.len() as u8);
        for e in &self.entries { b.extend_from_slice(&e.encode()); }
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.is_empty() { return Err(ContractError::IoError("Paytable: empty data".into())); }
        let n = data[0] as usize;
        if data.len() != 1 + n * 10 { return Err(ContractError::IoError(format!("Paytable: expected {} bytes, got {}", 1 + n * 10, data.len()))); }
        let mut entries = Vec::with_capacity(n);
        for i in 0..n { entries.push(PaytableEntry::decode(&data[1+i*10..1+(i+1)*10])?); }
        Ok(Paytable { entries })
    }
}

impl GameConfig {
    pub fn encode(&self) -> Vec<u8> {
        let reels_bytes: usize = self.reels.iter().map(|r| 1 + r.symbols.len()).sum();
        let paylines_bytes: usize = self.paylines.iter().map(|p| 5 + p.rows.len()).sum();
        let cap = 17 + reels_bytes + paylines_bytes;
        let mut b = Vec::with_capacity(cap);
        b.push(self.version);
        b.extend_from_slice(&(self.reel_count as u64).to_le_bytes());
        b.extend_from_slice(&(self.row_count as u64).to_le_bytes());
        b.push(self.reels.len() as u8);
        for r in &self.reels { b.extend_from_slice(&r.encode()); }
        b.push(self.paylines.len() as u8);
        for p in &self.paylines { b.extend_from_slice(&p.encode()); }
        b.extend_from_slice(&self.house_edge.to_le_bytes());
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 17 { return Err(ContractError::IoError(format!("GameConfig: expected at least 17 bytes, got {}", data.len()))); }
        let version = data[0];
        let reel_count = u64::from_le_bytes(data[1..9].try_into().unwrap()) as usize;
        let row_count = u64::from_le_bytes(data[9..17].try_into().unwrap()) as usize;
        let reel_n = data[17] as usize;
        let mut pos = 18;
        let mut reels = Vec::with_capacity(reel_n);
        for _ in 0..reel_n {
            if data.len() < pos + 1 { return Err(ContractError::IoError("GameConfig: data too short for reel".into())); }
            let rn = data[pos] as usize;
            if data.len() < pos + 1 + rn { return Err(ContractError::IoError("GameConfig: reel data truncated".into())); }
            reels.push(ReelStrip { symbols: data[pos+1..pos+1+rn].iter().map(|&b| Symbol(b)).collect() });
            pos += 1 + rn;
        }
        if data.len() < pos + 1 { return Err(ContractError::IoError("GameConfig: data too short for paylines".into())); }
        let pl_n = data[pos] as usize; pos += 1;
        let mut paylines = Vec::with_capacity(pl_n);
        for _ in 0..pl_n {
            if data.len() < pos + 5 { return Err(ContractError::IoError("GameConfig: payline data truncated".into())); }
            let id = u32::from_le_bytes(data[pos..pos+4].try_into().unwrap());
            let rn = data[pos+4] as usize; pos += 5;
            if data.len() < pos + rn { return Err(ContractError::IoError("GameConfig: payline rows truncated".into())); }
            paylines.push(Payline { id, rows: data[pos..pos+rn].to_vec() });
            pos += rn;
        }
        if data.len() < pos + 4 { return Err(ContractError::IoError("GameConfig: data too short for house_edge".into())); }
        let house_edge = u32::from_le_bytes(data[pos..pos+4].try_into().unwrap());
        Ok(GameConfig { version, reel_count, row_count, reels, paylines, house_edge })
    }
}

impl SpinResult {
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(1 + self.positions.len() * 8);
        b.push(self.positions.len() as u8);
        for p in &self.positions { b.extend_from_slice(&p.to_le_bytes()); }
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.is_empty() { return Err(ContractError::IoError("SpinResult: empty data".into())); }
        let n = data[0] as usize;
        if data.len() != 1 + n * 8 { return Err(ContractError::IoError(format!("SpinResult: expected {} bytes, got {}", 1 + n * 8, data.len()))); }
        let mut positions = Vec::with_capacity(n);
        for i in 0..n { positions.push(u64::from_le_bytes(data[1+i*8..1+(i+1)*8].try_into().unwrap())); }
        Ok(SpinResult { positions })
    }
}

impl Win {
    pub const ENCODED_SIZE: usize = 14;
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(14);
        b.extend_from_slice(&self.payline_id.to_le_bytes());
        b.push(self.symbol.0);
        b.push(self.count);
        b.extend_from_slice(&self.multiplier.to_le_bytes());
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 14 { return Err(ContractError::IoError(format!("Win: expected 14 bytes, got {}", data.len()))); }
        Ok(Win { payline_id: u32::from_le_bytes(data[0..4].try_into().unwrap()), symbol: Symbol(data[4]), count: data[5], multiplier: u64::from_le_bytes(data[6..14].try_into().unwrap()) })
    }
}

impl Spin {
    pub fn encode(&self) -> Vec<u8> {
        let result_bytes = if let Some(ref r) = self.result { r.encode() } else { vec![] };
        let cap = 302 + result_bytes.len() + self.wins.len() * 14;
        let mut b = Vec::with_capacity(cap);
        b.push(self.version);
        b.extend_from_slice(&self.id.to_repr());
        b.extend_from_slice(&self.player_pub.to_bytes());
        b.extend_from_slice(&self.bet_value.to_le_bytes());
        b.extend_from_slice(&self.paylines_played.to_le_bytes());
        b.extend_from_slice(&self.secret_nonce_commit.to_repr());
        b.extend_from_slice(&self.blind.to_repr());
        b.push(self.result.is_some() as u8);
        b.extend_from_slice(&result_bytes);
        b.push(self.wins.len() as u8);
        for w in &self.wins { b.extend_from_slice(&w.encode()); }
        b.extend_from_slice(&self.payout.to_le_bytes());
        b.push(self.state as u8);
        b.extend_from_slice(&self.house_edge.to_le_bytes());
        b.push(self.confirmation_depth);
        b.extend_from_slice(&self.created_at.to_le_bytes());
        b.extend_from_slice(&self.settle_block.to_le_bytes());
        b.extend_from_slice(&self.value_commit.to_bytes());
        b.extend_from_slice(&self.token_id.to_repr());
        b.extend_from_slice(&self.nullifier.to_repr());
        b.extend_from_slice(&self.instance_seed);
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 302 { return Err(ContractError::IoError(format!("Spin: expected at least 302 bytes, got {}", data.len()))); }
        let version = data[0];
        let id = Option::<pallas::Base>::from(pallas::Base::from_repr(data[1..33].try_into().unwrap())).ok_or_else(|| ContractError::IoError("Spin: invalid id".into()))?;
        let player_pub = PublicKey::from_bytes(data[33..65].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("Spin: invalid player_pub: {}", e)))?;
        let bet_value = u64::from_le_bytes(data[65..73].try_into().unwrap());
        let paylines_played = u32::from_le_bytes(data[73..77].try_into().unwrap());
        let secret_nonce_commit = Option::<pallas::Base>::from(pallas::Base::from_repr(data[77..109].try_into().unwrap())).ok_or_else(|| ContractError::IoError("Spin: invalid secret_nonce_commit".into()))?;
        let blind = Option::<pallas::Base>::from(pallas::Base::from_repr(data[109..141].try_into().unwrap())).ok_or_else(|| ContractError::IoError("Spin: invalid blind".into()))?;
        let has_result = data[141] != 0;
        let (result, win_start) = if has_result {
            let r = SpinResult::decode(&data[142..])?;
            let next = 142 + r.encode().len();
            (Some(r), next)
        } else {
            (None, 142)
        };
        if data.len() < win_start + 1 { return Err(ContractError::IoError("Spin: data too short for wins".into())); }
        let win_count = data[win_start] as usize;
        let win_end = win_start + 1 + win_count * 14;
        if data.len() < win_end + 89 { return Err(ContractError::IoError("Spin: data too short for tail".into())); }
        let mut wins = Vec::with_capacity(win_count);
        for i in 0..win_count { wins.push(Win::decode(&data[win_start+1+i*14..win_start+1+(i+1)*14])?); }
        let p = win_end;
        let payout = u64::from_le_bytes(data[p..p+8].try_into().unwrap());
        let state = SpinState::try_from(data[p+8])?;
        let house_edge = u32::from_le_bytes(data[p+9..p+13].try_into().unwrap());
        let confirmation_depth = data[p+13];
        let created_at = u64::from_le_bytes(data[p+14..p+22].try_into().unwrap());
        let settle_block = u64::from_le_bytes(data[p+22..p+30].try_into().unwrap());
        let value_commit = Option::<pallas::Point>::from(pallas::Point::from_bytes(data[p+30..p+62].try_into().unwrap())).ok_or_else(|| ContractError::IoError("Spin: invalid value_commit".into()))?;
        let token_id = Option::<pallas::Base>::from(pallas::Base::from_repr(data[p+62..p+94].try_into().unwrap())).ok_or_else(|| ContractError::IoError("Spin: invalid token_id".into()))?;
        let nullifier = Option::<pallas::Base>::from(pallas::Base::from_repr(data[p+94..p+126].try_into().unwrap())).ok_or_else(|| ContractError::IoError("Spin: invalid nullifier".into()))?;
        let instance_seed: [u8; 32] = data[p+126..p+158].try_into().unwrap();
        Ok(Spin { version, id, player_pub, bet_value, paylines_played, secret_nonce_commit, blind, result, wins, payout, state, house_edge, confirmation_depth, created_at, settle_block, value_commit, token_id, nullifier, instance_seed })
    }
}

// --- Bridge update structs ---

impl CancelSpinUpdateV1 {
    pub const ENCODED_SIZE: usize = 41;
    pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(41); b.extend_from_slice(&self.spin_id.to_repr()); b.extend_from_slice(&self.house_take.to_le_bytes()); b.push(self.state as u8); b }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 41 { return Err(ContractError::IoError(format!("CancelSpinUpdateV1: expected 41 bytes, got {}", data.len()))); }
        Ok(CancelSpinUpdateV1 { spin_id: Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("CancelSpinUpdateV1: invalid spin_id".into()))?, house_take: u64::from_le_bytes(data[32..40].try_into().unwrap()), state: SpinState::try_from(data[40])? })
    }
}

impl RevealSpinUpdateV1 {
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(34 + self.positions.len() * 8);
        b.extend_from_slice(&self.spin_id.to_repr());
        b.push(self.positions.len() as u8);
        for p in &self.positions { b.extend_from_slice(&p.to_le_bytes()); }
        b.push(self.state as u8);
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 34 { return Err(ContractError::IoError(format!("RevealSpinUpdateV1: expected at least 34 bytes, got {}", data.len()))); }
        let spin_id = Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("RevealSpinUpdateV1: invalid spin_id".into()))?;
        let n = data[32] as usize;
        if data.len() != 34 + n * 8 { return Err(ContractError::IoError(format!("RevealSpinUpdateV1: expected {} bytes, got {}", 34 + n * 8, data.len()))); }
        let mut positions = Vec::with_capacity(n);
        for i in 0..n { positions.push(u64::from_le_bytes(data[33+i*8..33+(i+1)*8].try_into().unwrap())); }
        let state = SpinState::try_from(data[33 + n * 8])?;
        Ok(RevealSpinUpdateV1 { spin_id, positions, state })
    }
}

impl SettleSpinUpdateV1 {
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(41 + self.wins.len() * 14);
        b.extend_from_slice(&self.spin_id.to_repr());
        b.push(self.wins.len() as u8);
        for w in &self.wins { b.extend_from_slice(&w.encode()); }
        b.extend_from_slice(&self.payout.to_le_bytes());
        b.push(self.state as u8);
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 41 { return Err(ContractError::IoError(format!("SettleSpinUpdateV1: expected at least 41 bytes, got {}", data.len()))); }
        let spin_id = Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("SettleSpinUpdateV1: invalid spin_id".into()))?;
        let n = data[32] as usize;
        if data.len() != 41 + n * 14 { return Err(ContractError::IoError(format!("SettleSpinUpdateV1: expected {} bytes, got {}", 41 + n * 14, data.len()))); }
        let mut wins = Vec::with_capacity(n);
        for i in 0..n { wins.push(Win::decode(&data[33+i*14..33+(i+1)*14])?); }
        let p = 33 + n * 14;
        let payout = u64::from_le_bytes(data[p..p+8].try_into().unwrap());
        let state = SpinState::try_from(data[p+8])?;
        Ok(SettleSpinUpdateV1 { spin_id, wins, payout, state })
    }
}

impl CommitSpinUpdateV1 {
    pub const ENCODED_SIZE: usize = 290;
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(290);
        b.extend_from_slice(&self.spin_id.to_repr());
        b.extend_from_slice(&self.player_pub.to_bytes());
        b.extend_from_slice(&self.bet_value.to_le_bytes());
        b.extend_from_slice(&self.paylines_played.to_le_bytes());
        b.extend_from_slice(&self.secret_nonce_commit.to_repr());
        b.extend_from_slice(&self.blind.to_repr());
        b.extend_from_slice(&self.house_edge.to_le_bytes());
        b.push(self.confirmation_depth);
        b.extend_from_slice(&self.token_id.to_repr());
        b.extend_from_slice(&self.value_commit.to_bytes());
        b.extend_from_slice(&self.settle_block.to_le_bytes());
        b.extend_from_slice(&self.nullifier.to_repr());
        b.push(self.state as u8);
        b.extend_from_slice(&self.created_at.to_le_bytes());
        b.extend_from_slice(&self.instance_seed);
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 290 { return Err(ContractError::IoError(format!("CommitSpinUpdateV1: expected 290 bytes, got {}", data.len()))); }
        Ok(CommitSpinUpdateV1 {
            spin_id: Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("CommitSpinUpdateV1: invalid spin_id".into()))?,
            player_pub: PublicKey::from_bytes(data[32..64].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("CommitSpinUpdateV1: invalid player_pub: {}", e)))?,
            bet_value: u64::from_le_bytes(data[64..72].try_into().unwrap()),
            paylines_played: u32::from_le_bytes(data[72..76].try_into().unwrap()),
            secret_nonce_commit: Option::<pallas::Base>::from(pallas::Base::from_repr(data[76..108].try_into().unwrap())).ok_or_else(|| ContractError::IoError("CommitSpinUpdateV1: invalid secret_nonce_commit".into()))?,
            blind: Option::<pallas::Base>::from(pallas::Base::from_repr(data[108..140].try_into().unwrap())).ok_or_else(|| ContractError::IoError("CommitSpinUpdateV1: invalid blind".into()))?,
            house_edge: u32::from_le_bytes(data[140..144].try_into().unwrap()),
            confirmation_depth: data[144],
            token_id: Option::<pallas::Base>::from(pallas::Base::from_repr(data[145..177].try_into().unwrap())).ok_or_else(|| ContractError::IoError("CommitSpinUpdateV1: invalid token_id".into()))?,
            value_commit: Option::<pallas::Point>::from(pallas::Point::from_bytes(data[177..209].try_into().unwrap())).ok_or_else(|| ContractError::IoError("CommitSpinUpdateV1: invalid value_commit".into()))?,
            settle_block: u64::from_le_bytes(data[209..217].try_into().unwrap()),
            nullifier: Option::<pallas::Base>::from(pallas::Base::from_repr(data[217..249].try_into().unwrap())).ok_or_else(|| ContractError::IoError("CommitSpinUpdateV1: invalid nullifier".into()))?,
            state: SpinState::try_from(data[249])?,
            created_at: u64::from_le_bytes(data[250..258].try_into().unwrap()),
            instance_seed: data[258..290].try_into().unwrap(),
        })
    }
}