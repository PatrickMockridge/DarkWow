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

//! Capability resolver for the wallet.
//!
//! Scans the local chain (full node) to derive the user's current capabilities
//! and compute available actions. Each contract gets a single-pass resolver
//! that scans its sled tree, derives capabilities, and builds per-instance
//! actions in one traversal.

use std::collections::{HashMap, HashSet};
use std::str::FromStr;

use dwow_sdk::capability::{
    Action, Capability, CapabilityDescriptor, CapabilityExpression, CapabilityId,
    CapabilityOutput, CapabilitySource,
};
use dwow_sdk::crypto::{pasta_prelude::PrimeField, ContractId, PublicKey, SecretKey};
use tracing::warn;

use dwow_promissory_note_contract::capability::{CAP_NOTE, CAP_MINT_AUTHORITY, CAP_RECEIPT};

use crate::cache::Cache;
use crate::walletdb::{AddressRecord, WalletDb};

/// Result of a capability resolution: what the user holds and what they can do.
#[derive(Clone, Debug)]

// All per-contract capability resolvers removed. The wallet now discovers
// contract interfaces from on-chain manifests via the generic fallback path
// and the orphan capability handler. Only Promissory Note (genesis) retains
// a hardcoded resolver. See: doc/src/arch/manifest.md, doc/src/arch/wallet.md
//
// The old per-contract resolver code (resolve_escrow, resolve_darkbet_exchange,
// resolve_dao_escrow, resolve_betting_stake, resolve_bearer_bond, resolve_pool_stake,
// resolve_lottery, resolve_baccarat, resolve_darktoshi_dice, resolve_game_room,
// resolve_roulette, resolve_slot, resolve_otc_swap, resolve_auction, resolve_dex,
// resolve_subscription, resolve_relayer_endowment) and their test code have been
// removed. These now belong in their contract crates as optional Rust client code
// or are replaced by manifest-based discovery.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolver_new_is_empty() {
        let resolver = CapabilityResolver::new();
        assert!(resolver.descriptors().is_empty());
    }
}
