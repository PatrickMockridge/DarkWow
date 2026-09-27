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

//! DAO-Escrow contract client API — ZK proof construction
//!
//! One module per proof-bearing endpoint, each exporting a `*_v1_proof` entry point, the call data it
//! bundles, and the public inputs it publishes. The call parameters themselves live in [`crate::model`]
//! and are the contract's own structs; this module does not restate them.
//!
//! ```text
//! init            → InitV2            initialize
//! update          → SetGovernanceConfigV2   update (installs the group; and withdraw's owner path)
//! pay_premium     → PayPremiumV2      pay_premium
//! propose_claim   → ProposeClaimV2    propose_claim
//! vote_claim      → VoteClaimV2       vote_claim
//! ```
//!
//! ## What is not here
//!
//! Six modules lived here and are removed, not stubbed. Five of them
//! (`resolve_dispute`, `verify_member_capability`, and the builders for `enable_drain_protection`,
//! `register_capability_requirement` and `deactivate_capability_requirement`) belonged to the
//! OCap/Identity model that MultiSig groups replaced, and their endpoints, circuits and ZKAS namespaces
//! went with them. The sixth was a builder library — `InitializeBuilder`, `PayPremiumBuilder`, and ten
//! more — whose structs restated `crate::model`'s parameter types field for field under the same
//! names, so the client and the contract each had their own definition of the same wire format and
//! nothing compared them. Nothing consumed any of them either: the test harness builds its calls from
//! `crate::model` and takes only the `*_v1_proof` functions from here. A second encoding of a struct is
//! how the two drift, which is the class this contract's re-wire exists to remove.

/// ZK circuit binary constants
pub mod zkbins;

pub mod init;
pub mod pay_premium;
pub mod propose_claim;
pub mod update;
pub mod vote_claim;
