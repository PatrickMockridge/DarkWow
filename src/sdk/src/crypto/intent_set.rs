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

//! Generic intent-set transition model.
//!
//! This is the contract-agnostic core for managing intent commitments and
//! nullifiers. A contract can bind these transitions to concrete ZK proofs,
//! merkle trees, and storage backends.
//!
//! ## The Intent-Set State Machine
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                 IntentSet State Machine                               │
//! ├─────────────────────────────────────────────────────────────────┤
//! │                                                                   │
//! │  STATE:                                                         │
//! │    commitments_root: Base  // Merkle root of commitments          │
//! │    commitments_count: u64  // Number of posted intents            │
//! │    nullifiers_root: Base   // Merkle root of spent nullifiers     │
//! │                                                                   │
//! │  POST Transition:                                                │
//! │    1. Validate: old_root == current.root                         │
//! │    2. Update: new_root, count + 1                               │
//! │                                                                   │
//! │  CONSUME Transition:                                             │
//! │    1. Validate: old_commit_root == current.commitments_root       │
//! │    2. Validate: old_nullifier_root == current.nullifiers_root    │
//! │    3. Validate: nullifier not already spent                      │
//! │    4. Update: new roots, add nullifier                         │
//! │                                                                   │
//! │  EXPIRE Transition:                                               │
//! │    Intent expires automatically after expiry block height         │
//! │    No state update needed, consumer must check expiry             │
//! │                                                                   │
//! └─────────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Usage
//!
//! ```rust,ignore
//! use darkfi_sdk::crypto::{IntentSetIndexV1, IntentPostTransitionV1, IntentConsumeTransitionV1};
//!
//! let mut index = IntentSetIndexV1::default();
//!
//! // Validate and apply a post transition
//! let post = IntentPostTransitionV1 { ... };
//! index.validate_post(&post)?;
//! index.apply_post(&post)?;
//!
//! // Validate and apply a consume transition
//! let consume = IntentConsumeTransitionV1 { ... };
//! index.validate_consume(&consume)?;
//! index.apply_consume(&consume)?;
//! ```

#[cfg(feature = "async")]
use darkfi_serial::async_trait;
use darkfi_serial::{SerialDecodable, SerialEncodable};
use pasta_curves::pallas;

use super::{IntentCommitment, IntentNullifier};
use crate::ContractError;

fn transition_error(msg: &str) -> ContractError {
    ContractError::IoError(msg.to_string())
}

/// On-chain intent-set state snapshot.
#[derive(Clone, Debug, Default, Eq, PartialEq, SerialEncodable, SerialDecodable)]
pub struct IntentSetState {
    /// Root of the intent-commitment tree.
    pub commitments_root: pallas::Base,
    /// Number of inserted commitments.
    pub commitments_count: u64,
    /// Root of the nullifier tree.
    pub nullifiers_root: pallas::Base,
}

/// State transition for posting a new intent commitment.
#[derive(Clone, Debug, Eq, PartialEq, SerialEncodable, SerialDecodable)]
pub struct IntentPostTransitionV1 {
    /// Posted commitment (kept for event/indexer use).
    pub commitment: IntentCommitment,
    /// Old commitment root before insertion.
    pub old_commitments_root: pallas::Base,
    /// New commitment root after insertion.
    pub new_commitments_root: pallas::Base,
    /// Old commitment count.
    pub old_commitments_count: u64,
    /// New commitment count.
    pub new_commitments_count: u64,
}

/// State transition for consuming/canceling an intent.
#[derive(Clone, Debug, Eq, PartialEq, SerialEncodable, SerialDecodable)]
pub struct IntentConsumeTransitionV1 {
    /// Consumed intent nullifier.
    pub nullifier: IntentNullifier,
    /// Old commitment root before update.
    pub old_commitments_root: pallas::Base,
    /// New commitment root after update.
    pub new_commitments_root: pallas::Base,
    /// Old nullifier root before insert.
    pub old_nullifiers_root: pallas::Base,
    /// New nullifier root after insert.
    pub new_nullifiers_root: pallas::Base,
}

/// Intent consume payload optionally anchored to a parent call in the tx tree.
#[derive(Clone, Debug, Eq, PartialEq, SerialEncodable, SerialDecodable)]
pub struct IntentConsumeCallV1 {
    /// The consume transition
    pub transition: IntentConsumeTransitionV1,
    /// Function ID of the parent call that authorized this consume
    pub auth_parent: crate::crypto::FuncId,
}

/// Minimal state machine for validating and applying intent-set transitions.
#[derive(Clone, Debug, Default, Eq, PartialEq, SerialEncodable, SerialDecodable)]
pub struct IntentSetIndexV1 {
    /// Current state
    pub state: IntentSetState,
    /// Set of consumed nullifiers (for replay protection)
    consumed_nullifiers: Vec<IntentNullifier>,
}

impl IntentSetIndexV1 {
    /// Create a new IntentSetIndexV1 with empty state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Validate an intent post transition against the current state.
    pub fn validate_post(&self, transition: &IntentPostTransitionV1) -> Result<(), ContractError> {
        if transition.old_commitments_root != self.state.commitments_root {
            return Err(transition_error("Intent post transition has stale commitment root"))
        }

        if transition.old_commitments_count != self.state.commitments_count {
            return Err(transition_error("Intent post transition has stale commitment count"))
        }

        let expected_new_count = transition
            .old_commitments_count
            .checked_add(1)
            .ok_or_else(|| transition_error("Intent post commitment count overflow"))?;
        if transition.new_commitments_count != expected_new_count {
            return Err(transition_error(
                "Intent post transition has invalid new commitment count",
            ))
        }

        Ok(())
    }

    /// Apply a validated post transition.
    pub fn apply_post(&mut self, transition: &IntentPostTransitionV1) -> Result<(), ContractError> {
        self.validate_post(transition)?;

        self.state.commitments_root = transition.new_commitments_root;
        self.state.commitments_count = transition.new_commitments_count;

        Ok(())
    }

    /// Validate a consume transition against the current state.
    pub fn validate_consume(
        &self,
        transition: &IntentConsumeTransitionV1,
    ) -> Result<(), ContractError> {
        if transition.old_commitments_root != self.state.commitments_root {
            return Err(transition_error(
                "Intent consume transition has stale commitment root",
            ))
        }

        if transition.old_nullifiers_root != self.state.nullifiers_root {
            return Err(transition_error(
                "Intent consume transition has stale nullifier root",
            ))
        }

        if self.consumed_nullifiers.contains(&transition.nullifier) {
            return Err(transition_error(
                "Intent consume transition reuses an existing nullifier",
            ))
        }

        Ok(())
    }

    /// Apply a validated consume transition.
    pub fn apply_consume(
        &mut self,
        transition: &IntentConsumeTransitionV1,
    ) -> Result<(), ContractError> {
        self.validate_consume(transition)?;

        self.state.commitments_root = transition.new_commitments_root;
        self.state.nullifiers_root = transition.new_nullifiers_root;
        self.consumed_nullifiers.push(transition.nullifier);

        Ok(())
    }

    /// Get the current state.
    pub fn state(&self) -> &IntentSetState {
        &self.state
    }

    /// Check if a nullifier has been consumed.
    pub fn is_nullifier_consumed(&self, nullifier: &IntentNullifier) -> bool {
        self.consumed_nullifiers.contains(nullifier)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_post_transition(
        commitment: u64,
        old_root: u64,
        new_root: u64,
        old_count: u64,
        new_count: u64,
    ) -> IntentPostTransitionV1 {
        IntentPostTransitionV1 {
            commitment: IntentCommitment(pallas::Base::from(commitment)),
            old_commitments_root: pallas::Base::from(old_root),
            new_commitments_root: pallas::Base::from(new_root),
            old_commitments_count: old_count,
            new_commitments_count: new_count,
        }
    }

    fn make_consume_transition(
        nullifier: u64,
        old_commit_root: u64,
        new_commit_root: u64,
        old_null_root: u64,
        new_null_root: u64,
    ) -> IntentConsumeTransitionV1 {
        IntentConsumeTransitionV1 {
            nullifier: IntentNullifier(pallas::Base::from(nullifier)),
            old_commitments_root: pallas::Base::from(old_commit_root),
            new_commitments_root: pallas::Base::from(new_commit_root),
            old_nullifiers_root: pallas::Base::from(old_null_root),
            new_nullifiers_root: pallas::Base::from(new_null_root),
        }
    }

    #[test]
    fn post_rejects_stale_root_and_bad_count_increment() {
        let mut index = IntentSetIndexV1::new();
        index.state.commitments_root = pallas::Base::from(10);

        let stale = make_post_transition(1, 0, 11, 0, 1);
        let err = index.validate_post(&stale).unwrap_err().to_string();
        assert!(err.contains("stale"));

        let bad_count = make_post_transition(1, 10, 11, 0, 3);
        let err = index.validate_post(&bad_count).unwrap_err().to_string();
        assert!(err.contains("invalid new commitment count"));
    }

    #[test]
    fn post_accepts_valid_transition() {
        let mut index = IntentSetIndexV1::new();
        let post = make_post_transition(1, 0, 100, 0, 1);

        index.apply_post(&post).unwrap();

        assert_eq!(index.state.commitments_root, pallas::Base::from(100));
        assert_eq!(index.state.commitments_count, 1);
    }

    #[test]
    fn consume_rejects_stale_root_and_duplicate_nullifier() {
        let mut index = IntentSetIndexV1::new();
        index.state.commitments_root = pallas::Base::from(100);
        index.state.commitments_count = 1;
        index.state.nullifiers_root = pallas::Base::from(50);

        let stale = make_consume_transition(7, 99, 101, 50, 51);
        let err = index.validate_consume(&stale).unwrap_err().to_string();
        assert!(err.contains("stale"));

        let consume = make_consume_transition(7, 100, 101, 50, 51);
        index.apply_consume(&consume).unwrap();

        let dup = make_consume_transition(7, 101, 102, 51, 52);
        let err = index.validate_consume(&dup).unwrap_err().to_string();
        assert!(err.contains("reuses"));
    }

    #[test]
    fn consume_replay_is_rejected() {
        let mut index = IntentSetIndexV1::new();
        index.state.commitments_root = pallas::Base::from(200);
        index.state.commitments_count = 2;
        index.state.nullifiers_root = pallas::Base::from(10);

        let consume = make_consume_transition(99, 200, 201, 10, 11);
        index.apply_consume(&consume).unwrap();

        let err = index.validate_consume(&consume).unwrap_err().to_string();
        assert!(err.contains("reuses"));
    }
}
