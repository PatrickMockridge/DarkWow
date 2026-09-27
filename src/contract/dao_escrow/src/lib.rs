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


//! DarkWow DAO-Escrow Contract
//!
//! One endowment pool, governed by one owner-installed MultiSig group.
//!
//! ## The mode, and what it decides
//!
//! A creator chooses one of three modes at `initialize`, and the mode is stored on the endowment:
//!
//! - **`Escrow`** (0) — claims are paid from the endowment. `endowment_withdraw` is legal;
//!   `treasury_spend` is not.
//! - **`Treasury`** (1) — the pool funds operational spending. `treasury_spend` is legal;
//!   `endowment_withdraw` is not.
//! - **`TreasuryEndowment`** (2) — both.
//!
//! **The mode decides which endpoints are legitimate, not which pool holds value**, because this
//! contract holds no balance of its own: `Purse` does, and every spend path here ends in a
//! `promissory_note::transfer_v1` child.
//!
//! A **fee split** between a treasury share and an endowment share used to be documented here and in
//! `FeeConfig`. It is not implemented and never was — nothing in this tree splits an incoming payment
//! between two pools — so both the config struct and the documentation of it are gone.
//!
//! ## Governance
//!
//! The owner installs a MultiSig group with `UpdateV1`, which is one-shot. From then on every spend and
//! every step of the claim lifecycle is authorised by that group's `multisig::FinalizeV1` over a message
//! naming the action, and the multisig contract consumes that approval exactly once.
//!
//! The model this contract was originally designed around — OCap capabilities verified through the
//! `Identity` contract, with per-role capability requirements — is **gone**, and the reason is recorded
//! in `OBL-C151`: nothing ever registered a requirement, so every gate that read one refused every call
//! even once it was reachable. A check that cannot pass is indistinguishable from a broken one.
//!
//! Likewise the **DrainProtection integration** documented here: this contract stored an association
//! bulla and an info flag that no handler ever read, and never once addressed the DrainProtection
//! contract. `EnableDrainProtectionV1` retired with them.
//!
//! ## Trust model
//!
//! - Membership notes are time-locked (block-based expiry), and `pay_premium` enforces `min_premium`.
//! - Claims against the endowment are decided by the group, never by a tally: the group's own threshold
//!   **is** the quorum, so one approved vote decides the claim.
//! - The owner's withdrawal proof is spend-once, via a nullifier the contract records.
//!
//! ## Use cases
//!
//! - **Community insurance** — `Escrow` mode: a pool that pays claims.
//! - **Protocol treasury** — `Treasury` mode: a pool that funds operations.
//! - **Both** — `TreasuryEndowment` mode.

use dwow_sdk::define_contract_function;

// The `modes` module lived here: `MODE_ESCROW`/`MODE_TREASURY`/`MODE_TREASURY_ENDOWMENT` as bare `u8`
// constants, a second encoding of a concept `DaoEscrowMode` already encodes. Two encodings of one thing is
// how they drift, and the mode is now a real init parameter (`DaoEscrowMode`), so the enum is the only one.

// The seven retired selectors — 0x06, 0x0a, 0x0b, 0x0c, 0x0e, 0x0f, 0x10 — are **deliberately absent**, and
// absent rather than mapped to a no-op arm. A caller sending one now reaches `InvalidFunction`, which is a
// refusal a caller can read; the two governance functions used to return `Ok(())` while doing nothing, and a
// silent success on a function whose name promises an action is indistinguishable from the action having
// happened. `OBL-C151`'s own history is the argument: a gate that reads a field nothing can set is
// indistinguishable from a broken one, and a no-op that reports success is the same shape in the other
// direction. The **surviving** selectors keep their original numbers, because they are explicit literals
// here and nothing renumbers.
define_contract_function!(DaoEscrowFunction {
    InitializeV1 = 0x00,
    UpdateV1 = 0x01,
    PayPremiumV1 = 0x02,
    WithdrawV1 = 0x03,
    EndowmentWithdrawV1 = 0x04,
    TreasurySpendV1 = 0x05,
    ProposeClaimV1 = 0x07,
    VoteClaimV1 = 0x08,
    ExecuteClaimV1 = 0x09,
    CancelClaimV1 = 0x0d,
});

/// Internal contract errors
pub mod error;

/// Per-capability key descriptors and action metadata
pub mod capability;
/// Call parameters definitions
pub mod model;

#[cfg(not(feature = "no-entrypoint"))]
/// WASM entrypoint functions
pub mod entrypoint;

#[cfg(feature = "client")]
/// Client API for interaction with this smart contract
pub mod client;

// ============================================================================
// DATABASE TREES
// ============================================================================

/// Info tree (version, config)
pub const DAO_ESCROW_CONTRACT_INFO_TREE: &str = "info";
/// Bullas tree (endowment instances)
pub const DAO_ESCROW_CONTRACT_BULLAS_TREE: &str = "bullas";
/// Membership notes tree (time-limited membership)
pub const DAO_ESCROW_CONTRACT_MEMBERSHIP_TREE: &str = "membership";
/// Endowment pool tree (actual funds)
pub const DAO_ESCROW_CONTRACT_ENDOWMENT_TREE: &str = "endowment";
/// Proposals tree (governance proposals/claims)
pub const DAO_ESCROW_CONTRACT_PROPOSALS_TREE: &str = "proposals";
/// Nullifiers tree (spent approvals, votes and ownership proofs)
pub const DAO_ESCROW_CONTRACT_NULLIFIERS_TREE: &str = "nullifiers";

// Four trees were declared here and written by nobody, so they are removed rather than wired: `votes`
// (the tally lives on the `Proposal`, which is the record that is actually read), `capability_requirements`
// and `disputes` (their endpoints retired with the OCap model), and `governance` (the orphan of
// `GovernanceConfig`, which `multisig_group_id` replaced). Three keys went with them — `db_version`,
// `merkle_tree` and `last_root` — each of which had one `db_set` and no reader, and no merkle root was ever
// written at all.

// ============================================================================
// ZKAS CIRCUIT NAMESPACES
// ============================================================================

/// ZKAS namespace for initialization V2 (domain-separated)
pub const DAO_ESCROW_ZKAS_INIT_NS_V2: &str = "InitV2";
/// ZKAS namespace for premium payment V2 (domain-separated)
pub const DAO_ESCROW_ZKAS_PREMIUM_NS_V2: &str = "PayPremiumV2";
/// ZKAS namespace for claim proposal V2 (domain-separated)
pub const DAO_ESCROW_ZKAS_PROPOSE_CLAIM_NS_V2: &str = "ProposeClaimV2";
/// ZKAS namespace for claim voting V2 (domain-separated)
pub const DAO_ESCROW_ZKAS_VOTE_CLAIM_NS_V2: &str = "VoteClaimV2";
/// ZKAS namespace for the ownership proof (`UpdateV1`, and `WithdrawV1`'s owner path)
pub const DAO_ESCROW_ZKAS_SET_GOVERNANCE_CONFIG_NS_V2: &str = "SetGovernanceConfigV2";

// The seven V1 namespaces were retired here. They named circuits that do not exist on disk — the V1 `.zk`
// and `.zk.bin` files were deleted in `rc3 Batch 4`, as the note below records — and a live namespace
// constant naming a deleted circuit is what kept the belief that in-contract ZK verification ran here. Two
// V2 namespaces went with the circuits they named: `VerifyMemberCapabilityV2` and `ResolveDisputeV2`.

// ============================================================================
// ZK CIRCUIT BINARIES (for client-side proof generation)
// ============================================================================

// V1 ZK circuit binaries removed (rc3 Batch 4) — V1 .zk source and .zk.bin files deleted.

// The DrainProtection integration section lived here. It declared two info keys — a flag and a bulla —
// that no handler ever read, plus a doc block describing rate limiting, a 2/3 vote and member-exit haircuts
// that this contract never implemented: it only ever *stored* an association and never once addressed the
// DrainProtection contract. `EnableDrainProtectionV1` retired with them.
//
// The `identity_cid`, `purse_cid` and `box_cid` keys retired for the same reason: each had one `db_set` at
// init and zero `db_get`. `identity_cid` in particular was seeded as `[0u8; 32]` and its reader treated
// zero as "skip the routing check" — fail-open — which `OBL-C152` records. Unit 3 of the re-wire
// programme will re-introduce a purse id when dao_escrow actually calls the Purse contract; until then a
// stored key nothing reads is the dead state this programme exists to remove.
/// Promissory Note contract ID for cross-contract routing validation
pub const PROMISSORY_NOTE_CONTRACT_ID_KEY: &[u8] = b"promissory_note_cid";
/// MultiSig contract ID — the contract a governance approval child must target. A governance-gated
/// endpoint validates that its `multisig::FinalizeV1` child is addressed here before reading the
/// approval, so a child aimed at a different contract cannot stand in for one (`OBL-C151`).
pub const MULTISIG_CONTRACT_ID_KEY: &[u8] = b"multisig_cid";

/// Thread-safe flag for deterministic ZK proof generation.
/// Set by tests before endpoint exercise to eliminate OsRng from collateral/debt
/// blinds, note encryption, and proof generation, so a chain-replay determinism
/// check (PI-7) produces identical bytes on both chains.
/// Must be set BEFORE any ZK proof is created.
#[cfg(feature = "deterministic-zk")]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(feature = "deterministic-zk")]
static DETERMINISTIC_ZK: AtomicBool = AtomicBool::new(false);

/// Enable deterministic ZK proof generation for testing.
/// Replaces OsRng with StdRng::seed_from_u64(0).
#[cfg(feature = "deterministic-zk")]
pub fn enable_deterministic_zk() {
    DETERMINISTIC_ZK.store(true, Ordering::SeqCst);
}

/// Returns true if deterministic ZK mode is enabled. Always `false` unless the
/// `deterministic-zk` feature is enabled (test builds only — heavyweight-spec.md §7.4 DZ-4).
pub fn deterministic_zk_enabled() -> bool {
    #[cfg(feature = "deterministic-zk")]
    {
        DETERMINISTIC_ZK.load(Ordering::SeqCst)
    }
    #[cfg(not(feature = "deterministic-zk"))]
    {
        false
    }
}
