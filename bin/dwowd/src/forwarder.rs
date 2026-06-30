/* This file is part of DarkWow
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

//! Coinbase Forwarder — tracks miner's own coinbase outputs and builds
//! NativeToken::TransferV1 transactions to FORWARD_DESTINATION after
//! the coinbase maturity period (COINBASE_MATURITY blocks).
//!
//! Pattern: Bitcoin pool operator. Mine to own keypair, distribute
//! rewards via standard signed transfers after maturity.

use std::collections::VecDeque;
use tracing::{debug, info};

use dwow_chain::COINBASE_MATURITY;

/// A tracked coinbase output produced by this miner.
#[derive(Debug, Clone)]
pub struct TrackedCoinbase {
    /// Block height where this coinbase was created
    pub height: u64,
    /// Reward value in base units
    pub value: u64,
    /// Whether this coinbase has been forwarded yet
    pub forwarded: bool,
}

/// Tracks miner's own coinbase outputs and determines when they mature.
pub struct CoinbaseTracker {
    coinbases: VecDeque<TrackedCoinbase>,
}

impl CoinbaseTracker {
    pub fn new() -> Self {
        Self { coinbases: VecDeque::new() }
    }

    /// Record a new coinbase output after a block is mined.
    pub fn record(&mut self, height: u64, value: u64) {
        self.coinbases.push_back(TrackedCoinbase {
            height, value, forwarded: false,
        });
        info!(target: "dwowd::forwarder",
            "Coinbase recorded: height={} value={} (matures at {})",
            height, value, height + COINBASE_MATURITY);
    }

    /// Return matured coinbases not yet forwarded. Marks them forwarded.
    pub fn matured(&mut self, current_height: u64) -> Vec<TrackedCoinbase> {
        let mut matured = Vec::new();
        while let Some(cb) = self.coinbases.front() {
            if cb.forwarded { self.coinbases.pop_front(); continue; }
            if current_height >= cb.height + COINBASE_MATURITY {
                let mut cb = self.coinbases.pop_front().unwrap();
                cb.forwarded = true;
                matured.push(cb);
            } else { break; }
        }
        matured
    }
}
