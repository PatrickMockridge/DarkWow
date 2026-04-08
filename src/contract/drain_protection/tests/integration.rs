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

//! DrainProtection contract integration tests

use darkfi_drain_protection_contract::{
    model::{
        DrainConfig, ExitParamsV1, ExitQueueEntry, ExitRequest, ExitUpdateV1, LockParamsV1,
        LockState, LockUpdateV1, MemberWeight, ProposeParamsV1, ProposeUpdateV1, ProtectedFund,
        RateLimit, TransferParamsV1, TransferRecord, TransferUpdateV1, UnlockParamsV1,
        UnlockUpdateV1, UpdateConfigParamsV1, UpdateConfigUpdateV1, VoteAction, VoteParamsV1,
        VoteProposal, VoteThresholds, VoteUpdateV1,
    },
    DrainProtectionFunction,
    // Constants
    DRAIN_PROTECTION_CONTRACT_INFO_TREE, DRAIN_PROTECTION_CONTRACT_FUNDS_TREE,
    DRAIN_PROTECTION_CONTRACT_PROPOSALS_TREE, DRAIN_PROTECTION_CONTRACT_MEMBERS_TREE,
    DRAIN_PROTECTION_CONTRACT_TRANSFERS_TREE, DRAIN_PROTECTION_CONTRACT_EXITS_TREE,
    DRAIN_PROTECTION_CONTRACT_VOTES_TREE,
};
use darkfi_serial::{deserialize, serialize};
use darkfi_sdk::{
    crypto::{pasta_prelude::Field, PublicKey, SecretKey},
    pasta::pallas,
};

/// Helper to create PublicKey from a numeric seed
fn make_pubkey(seed: u64) -> PublicKey {
    let secret = SecretKey::from(pallas::Base::from(seed));
    PublicKey::from_secret(secret)
}

#[test]
fn test_drain_protection_function_enum_valid() {
    assert!(DrainProtectionFunction::try_from(0x00).is_ok()); // InitializeV1
    assert!(DrainProtectionFunction::try_from(0x01).is_ok()); // ProposeV1
    assert!(DrainProtectionFunction::try_from(0x02).is_ok()); // VoteV1
    assert!(DrainProtectionFunction::try_from(0x03).is_ok()); // ExecuteV1
    assert!(DrainProtectionFunction::try_from(0x04).is_ok()); // ExitV1
    assert!(DrainProtectionFunction::try_from(0x05).is_ok()); // TransferV1
    assert!(DrainProtectionFunction::try_from(0x06).is_ok()); // LockV1
    assert!(DrainProtectionFunction::try_from(0x07).is_ok()); // UnlockV1
    assert!(DrainProtectionFunction::try_from(0x08).is_ok()); // UpdateConfigV1
}

#[test]
fn test_drain_protection_function_enum_invalid() {
    assert!(DrainProtectionFunction::try_from(0xFF).is_err());
    assert!(DrainProtectionFunction::try_from(0x09).is_err());
    assert!(DrainProtectionFunction::try_from(0x10).is_err());
}

#[test]
fn test_lock_state_encoding() {
    // Test LockState serialization
    let unlocked = LockState::Unlocked;
    let locked = LockState::Locked;

    let encoded_unlocked = serialize(&unlocked);
    let decoded_unlocked: LockState = deserialize(&encoded_unlocked).unwrap();
    assert_eq!(decoded_unlocked, LockState::Unlocked);

    let encoded_locked = serialize(&locked);
    let decoded_locked: LockState = deserialize(&encoded_locked).unwrap();
    assert_eq!(decoded_locked, LockState::Locked);
}

#[test]
fn test_rate_limit_default() {
    let rate_limit = RateLimit::default();
    assert_eq!(rate_limit.base_rate_bps, 100);
    assert_eq!(rate_limit.averaging_window_blocks, 1000);
    assert_eq!(rate_limit.vote_required_above_bps, 100);
}

#[test]
fn test_vote_thresholds_default() {
    let thresholds = VoteThresholds::default();
    assert_eq!(thresholds.large_withdrawal_thresh, 667);
    assert_eq!(thresholds.lock_unlock_thresh, 667);
    assert_eq!(thresholds.authority_change_thresh, 667);
    assert_eq!(thresholds.quorum_min_bps, 500);
}

#[test]
fn test_member_weight_effective_weight() {
    let weight = MemberWeight {
        contribution: 1000,
        deposited_at: 50000,
        weight_multiplier: 1000,
    };

    // Same block = no time bonus
    let effective = weight.effective_weight(50000);
    assert_eq!(effective, 1000);

    // After some time, weight increases
    let effective = weight.effective_weight(60000);
    // blocks_held = 10000, time_multiplier = 1_000 + 1 = 1_001
    // effective = 1000 * 1001 / 1000 = 1001
    assert_eq!(effective, 1001);
}

#[test]
fn test_drain_config_default() {
    let config = DrainConfig::default();
    assert!(config.graduated_tiers.is_none());
    assert!(config.exit_queue.is_none());
    assert!(config.circuit_breaker.is_none());
    assert!(config.guardian_pause.is_none());
    assert!(config.observation_period.is_none());
    assert!(config.split_proposals.is_none());
    assert!(config.no_loss_reserve.is_none());
    assert!(config.dead_mans_switch.is_none());
}

#[test]
fn test_rate_limit_encoding() {
    let rate_limit = RateLimit {
        base_rate_bps: 100,
        averaging_window_blocks: 1000,
        vote_required_above_bps: 100,
    };

    let encoded = serialize(&rate_limit);
    let decoded: RateLimit = deserialize(&encoded).unwrap();

    assert_eq!(decoded.base_rate_bps, rate_limit.base_rate_bps);
    assert_eq!(decoded.averaging_window_blocks, rate_limit.averaging_window_blocks);
}

#[test]
fn test_vote_thresholds_encoding() {
    let thresholds = VoteThresholds {
        large_withdrawal_thresh: 667,
        lock_unlock_thresh: 667,
        authority_change_thresh: 667,
        quorum_min_bps: 500,
    };

    let encoded = serialize(&thresholds);
    let decoded: VoteThresholds = deserialize(&encoded).unwrap();

    assert_eq!(decoded.large_withdrawal_thresh, thresholds.large_withdrawal_thresh);
    assert_eq!(decoded.quorum_min_bps, thresholds.quorum_min_bps);
}

#[test]
fn test_transfer_record_encoding() {
    let record = TransferRecord {
        block: 50000,
        amount: 500,
    };

    let encoded = serialize(&record);
    let decoded: TransferRecord = deserialize(&encoded).unwrap();

    assert_eq!(decoded.block, record.block);
    assert_eq!(decoded.amount, record.amount);
}

#[test]
fn test_exit_request_encoding() {
    let request = ExitRequest {
        id: pallas::Base::from(1),
        member_pubkey: make_pubkey(1),
        weight: 1000,
        requested_value: 995,
        haircut_bps: 50,
        payout_value: 945,
        requested_at: 50000,
        processed: false,
    };

    let encoded = serialize(&request);
    let decoded: ExitRequest = deserialize(&encoded).unwrap();

    assert_eq!(decoded.id, request.id);
    assert_eq!(decoded.requested_value, request.requested_value);
    assert_eq!(decoded.haircut_bps, request.haircut_bps);
    assert_eq!(decoded.processed, request.processed);
}

#[test]
fn test_vote_proposal_encoding() {
    let proposal = VoteProposal {
        id: pallas::Base::from(1),
        action: VoteAction::LockFunds,
        started_at: 50000,
        ends_at: 50600,
        yes_votes: 700,
        no_votes: 100,
        concluded: false,
    };

    let encoded = serialize(&proposal);
    let decoded: VoteProposal = deserialize(&encoded).unwrap();

    assert_eq!(decoded.id, proposal.id);
    assert_eq!(decoded.yes_votes, proposal.yes_votes);
    assert_eq!(decoded.no_votes, proposal.no_votes);
    assert_eq!(decoded.concluded, proposal.concluded);
}

#[test]
fn test_exit_queue_entry_encoding() {
    let entry = ExitQueueEntry {
        position: 1,
        member_pubkey: make_pubkey(1),
        requested_value: 1000,
        weight: 1000,
        queued_at: 50000,
        processed: false,
    };

    let encoded = serialize(&entry);
    let decoded: ExitQueueEntry = deserialize(&encoded).unwrap();

    assert_eq!(decoded.position, entry.position);
    assert_eq!(decoded.requested_value, entry.requested_value);
    assert_eq!(decoded.processed, entry.processed);
}

#[test]
fn test_exit_params_encoding() {
    let params = ExitParamsV1 {
        member_pubkey: make_pubkey(1),
        contribution_weight: 1000,
        current_block: 50000,
        proof: vec![1, 2, 3],
    };

    let encoded = serialize(&params);
    let decoded: ExitParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.contribution_weight, params.contribution_weight);
    assert_eq!(decoded.current_block, params.current_block);
}

#[test]
fn test_exit_update_encoding() {
    let update = ExitUpdateV1 {
        exit_id: pallas::Base::from(1),
        member_pubkey: make_pubkey(1),
        payout_value: 945,
        haircut_collected: 50,
    };

    let encoded = serialize(&update);
    let decoded: ExitUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.payout_value, update.payout_value);
    assert_eq!(decoded.haircut_collected, update.haircut_collected);
}

#[test]
fn test_lock_params_encoding() {
    let params = LockParamsV1 {
        duration_blocks: 600,
        signature: pallas::Base::from(1),
    };

    let encoded = serialize(&params);
    let decoded: LockParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.duration_blocks, params.duration_blocks);
}

#[test]
fn test_lock_update_encoding() {
    let update = LockUpdateV1 {
        locked_until: 50600,
    };

    let encoded = serialize(&update);
    let decoded: LockUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.locked_until, update.locked_until);
}

#[test]
fn test_unlock_params_encoding() {
    let params = UnlockParamsV1 {
        signature: pallas::Base::from(1),
    };

    let encoded = serialize(&params);
    let decoded: UnlockParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.signature, params.signature);
}

#[test]
fn test_unlock_update_encoding() {
    let update = UnlockUpdateV1 {
        unlocked_at: 50000,
    };

    let encoded = serialize(&update);
    let decoded: UnlockUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.unlocked_at, update.unlocked_at);
}

#[test]
fn test_transfer_params_encoding() {
    let params = TransferParamsV1 {
        amount: 500,
        recipient: make_pubkey(1),
        signature: pallas::Base::from(1),
        exceeds_rate_limit: false,
        vote_proposal_id: None,
    };

    let encoded = serialize(&params);
    let decoded: TransferParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.amount, params.amount);
    assert_eq!(decoded.exceeds_rate_limit, params.exceeds_rate_limit);
}

#[test]
fn test_transfer_update_encoding() {
    let update = TransferUpdateV1 {
        amount: 500,
        recipient: make_pubkey(1),
        rate_limited: true,
    };

    let encoded = serialize(&update);
    let decoded: TransferUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.amount, update.amount);
    assert_eq!(decoded.rate_limited, update.rate_limited);
}

#[test]
fn test_update_config_params_encoding() {
    let params = UpdateConfigParamsV1 {
        rate_limit: Some(RateLimit::default()),
        thresholds: Some(VoteThresholds::default()),
        new_spend_authority: Some(make_pubkey(1)),
    };

    let encoded = serialize(&params);
    let decoded: UpdateConfigParamsV1 = deserialize(&encoded).unwrap();

    assert!(decoded.rate_limit.is_some());
    assert!(decoded.thresholds.is_some());
    assert!(decoded.new_spend_authority.is_some());
}

#[test]
fn test_update_config_update_encoding() {
    let update = UpdateConfigUpdateV1 {
        authority_change_timelock: Some(100000),
    };

    let encoded = serialize(&update);
    let decoded: UpdateConfigUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.authority_change_timelock, update.authority_change_timelock);
}

#[test]
fn test_constants() {
    assert_eq!(DRAIN_PROTECTION_CONTRACT_INFO_TREE, "info");
    assert_eq!(DRAIN_PROTECTION_CONTRACT_FUNDS_TREE, "funds");
    assert_eq!(DRAIN_PROTECTION_CONTRACT_PROPOSALS_TREE, "proposals");
    assert_eq!(DRAIN_PROTECTION_CONTRACT_MEMBERS_TREE, "members");
    assert_eq!(DRAIN_PROTECTION_CONTRACT_TRANSFERS_TREE, "transfers");
    assert_eq!(DRAIN_PROTECTION_CONTRACT_EXITS_TREE, "exits");
    assert_eq!(DRAIN_PROTECTION_CONTRACT_VOTES_TREE, "votes");
}
