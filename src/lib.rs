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

#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

pub mod error;
pub use error::{ClientFailed, ClientResult, Error, Result};

/// Barb vocabulary — the 22 observable actions (type-system.md §1.1).
/// Unconditionally compiled: type-system interior vocabulary, not
/// networking infrastructure. Re-exported as `net::barb_trait`.
pub mod barb;

#[cfg(feature = "blockchain")]
pub mod blockchain;

#[cfg(feature = "geode")]
pub mod geode;

// event-graph: P2P messaging DAG used by darkirc, evgrd, and dwowd (0x42
// blockchain-event DAG-substrate dissemination, type-system.md §10.4).
// Uses sled-overlay for non-deterministic DAG operations.
// QUARANTINE: the quarantine IS tree-level, not binary-level (§10.4):
// sled-overlay writes go to the DAG tree (e.g. "dwowd_dag"), never to
// blockchain trees ("blocks", "contracts", "coins", "nullifiers", etc.).
// This feature is intentionally enabled by the dwowd binary for the DAG
// substrate; the previous binary-level quarantine ("must never be enabled
// by dwowd") did not match §10.4's own wording.
#[cfg(feature = "event-graph")]
pub mod event_graph;

#[cfg(any(feature = "net-wire", feature = "net-wallet", feature = "net"))]
pub mod net;

#[cfg(feature = "rpc")]
pub mod rpc;

#[cfg(feature = "concurrency")]
pub mod concurrency;

#[cfg(feature = "tx")]
pub mod tx;

#[cfg(feature = "util")]
pub mod util;

#[cfg(feature = "wasm-runtime")]
pub mod runtime;

#[cfg(feature = "zk")]
pub mod zk;

#[cfg(feature = "zkas")]
pub mod zkas;

#[cfg(feature = "dht")]
pub mod dht;

pub const ANSI_LOGO: &str = include_str!("../contrib/darkwow.ansi");

#[macro_export]
macro_rules! cli_desc {
    () => {{
        let commitish = match option_env!("COMMITISH") {
            Some(c) => &format!("-{}", c),
            None => "",
        };
        let desc = format!(
            "{} {}\n{}{}\n{}",
            env!("CARGO_PKG_NAME").to_string(),
            env!("CARGO_PKG_VERSION").to_string(),
            commitish,
            env!("CARGO_PKG_DESCRIPTION").to_string(),
            dwow_core::ANSI_LOGO,
        );

        Box::leak(desc.into_boxed_str()) as &'static str
    }};
}
