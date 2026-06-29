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

//! Wallet P2P configuration types.
//!
//! The wallet uses `dwow_core::net::P2p` via the `net-wallet` feature for
//! all P2P networking. This module provides the TOML-direct config struct
//! (`P2pWalletConfig`) and transport marker trait (`WalletStream`).
//!
//! No structopt, no SettingsOpt, no config merging. Config is TOML-direct.

use serde::{Deserialize, Serialize};
use smol::io::{AsyncRead, AsyncWrite};

#[cfg(feature = "transport")]
use dwow_transport;

/// Marker trait for type-erased async streams — same pattern as
/// dwow_transport::PtStream. Produced by both Layer 0 (built-in TCP)
/// and Layer 1 (dwow_transport).
pub trait WalletStream: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> WalletStream for T {}

// ============================================================================
// Config — direct TOML deserialization, no SettingsOpt, no structopt
// ============================================================================

/// Seed node address parsed from TOML.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeedAddr {
    pub url: String,
}

/// P2P configuration for the wallet. Parsed directly from TOML `[net]` section.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct P2pWalletConfig {
    #[serde(default)]
    pub seeds: Vec<SeedAddr>,
    /// Peer nodes to connect to directly (ManualSession). Provides a lilith-free
    /// bootstrap path. Combined with PEX gossip, a wallet with even one
    /// configured peer can discover the entire network.
    #[serde(default)]
    pub peers: Vec<SeedAddr>,
    #[serde(default = "default_magic_bytes")]
    pub magic_bytes: [u8; 4],
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_max_peers")]
    pub max_peers: usize,
    #[serde(default = "default_connect_timeout")]
    pub connect_timeout_secs: u64,
    #[serde(default = "default_request_timeout")]
    pub request_timeout_secs: u64,
    #[serde(default)]
    pub localnet: bool,
    /// App name used in version handshake. Must match what the seed/mining nodes
    /// expect on the target network (typically "darkfid" for darkwow-devnet and
    /// darkwow-testnet). If not set, defaults to "dwow-wallet" for backward
    /// compatibility.
    #[serde(default)]
    pub app_name: Option<String>,
    /// Datastore path for Tor arti data/cache directories. Expanded by caller.
    #[serde(default)]
    pub datastore: Option<String>,
}

fn default_magic_bytes() -> [u8; 4] { [0xd9, 0xef, 0xb6, 0x7d] }
fn default_port() -> u16 { 31340 }
fn default_max_peers() -> usize { 8 }
fn default_connect_timeout() -> u64 { 10 }
fn default_request_timeout() -> u64 { 30 }

