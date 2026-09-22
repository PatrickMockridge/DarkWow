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

//! Pool Stake contract integration tests
//!
//! These tests verify the pool_stake contract's:
//! - Function enum parsing
//! - Data structure encoding/decoding
//! - Model type invariants

use dwow_pool_stake_contract::{
    model::{
        AllocateCoverageParamsV1, AllocateCoverageUpdateV1, ClaimFeesParamsV1, ClaimFeesUpdateV1,
        CoverageAllocation, CreatePoolParamsV1, CreatePoolUpdateV1, JoinPoolParamsV1,
        JoinPoolUpdateV1, LeavePoolParamsV1, LeavePoolUpdateV1, PoolMemberStake, PoolStakeRegistry,
        ReleaseCoverageParamsV1, ReleaseCoverageUpdateV1, SlashCoverageParamsV1, SlashCoverageUpdateV1,
        RebalancePoolSharesParamsV1,
        UpdatePoolConfigParamsV1, UpdatePoolConfigUpdateV1,
    },
    PoolStakeFunction, POOL_STAKE_BP_PRECISION, POOL_STAKE_LEAVE_COOLDOWN_BLOCKS,
    POOL_STAKE_MAX_COVERAGE_RATIO, POOL_STAKE_MIN_STAKE,
};
use dwow_serial::{deserialize, serialize};
use dwow_sdk::{crypto::pasta_prelude::PrimeField, crypto::PublicKey, pasta::pallas};

/// Helper to create a pallas::Base from bytes
fn make_base(bytes: [u8; 32]) -> pallas::Base {
    pallas::Base::from_repr(bytes).unwrap()
}

/// Helper to create PublicKey from a numeric seed
fn make_pubkey(seed: u64) -> PublicKey {
    use dwow_sdk::crypto::SecretKey;
    let secret = SecretKey::from_base(pallas::Base::from(seed));
    PublicKey::from_secret(secret)
}

/// A well-formed `PoolMemberStake` for the update round-trip tests. The update structs now carry
/// whole records (so apply can re-store them without reading), and these tests exist to prove the
/// carried record survives the bridge intact.
fn make_member_stake() -> PoolMemberStake {
    PoolMemberStake {
        version: 1,
        instance_seed: [0u8; 32],
        stake_id: make_base([1u8; 32]),
        pool_id: make_base([2u8; 32]),
        member_pub: make_pubkey(3),
        relayer_id: [4u8; 32],
        original_amount: 1_000_000,
        current_amount: 1_000_000,
        coverage_contribution: 900_000,
        pool_share_bp: 2000,
        accumulated_fees: 0,
        created_at: 4242,
        leave_requested_at: None,
        slash_count: 0,
        is_active: true,
    }
}

/// A well-formed `PoolStakeRegistry`.
fn make_registry() -> PoolStakeRegistry {
    PoolStakeRegistry {
        version: 0,
        pool_id: make_base([2u8; 32]),
        owner_pub: make_pubkey(2),
        total_stake: 1_000_000,
        available_coverage: 900_000,
        allocated_coverage: 100_000,
        member_count: 5,
        max_coverage_ratio: 10000,
        operator_fee_bp: 100,
        created_at: 100,
        total_slashed: 0,
        pool_slash_count: 0,
        is_active: true,
    }
}

/// A well-formed `CoverageAllocation` for the same purpose.
fn make_allocation() -> CoverageAllocation {
    CoverageAllocation {
        version: 1,
        allocation_id: make_base([7u8; 32]),
        pool_id: make_base([2u8; 32]),
        withdrawal_nullifier: [8u8; 32],
        amount: 100_000,
        contributing_members: vec![],
        created_at: 4242,
        timeout_height: 500,
        executed: false,
        slashed: false,
    }
}

#[test]
fn test_pool_stake_function_enum_valid() {
    // Test that all function IDs are valid
    assert!(PoolStakeFunction::try_from(0x00).is_ok()); // CreatePoolV1
    assert!(PoolStakeFunction::try_from(0x01).is_ok()); // JoinPoolV1
    assert!(PoolStakeFunction::try_from(0x02).is_ok()); // LeavePoolV1
    assert!(PoolStakeFunction::try_from(0x03).is_ok()); // AllocateCoverageV1
    assert!(PoolStakeFunction::try_from(0x04).is_ok()); // ReleaseCoverageV1
    assert!(PoolStakeFunction::try_from(0x05).is_ok()); // SlashCoverageV1
    assert!(PoolStakeFunction::try_from(0x06).is_ok()); // ClaimFeesV1
    assert!(PoolStakeFunction::try_from(0x07).is_ok()); // UpdatePoolConfigV1
}

#[test]
fn test_pool_stake_function_enum_invalid() {
    // Test that invalid function IDs return errors
    assert!(PoolStakeFunction::try_from(0xFF).is_err());
    assert!(PoolStakeFunction::try_from(0x09).is_err());
    assert!(PoolStakeFunction::try_from(0x10).is_err());
}

#[test]
fn test_pool_stake_registry_encoding() {
    let registry = PoolStakeRegistry {

        version: 0,        pool_id: make_base([1u8; 32]),
        owner_pub: make_pubkey(2),
        total_stake: 1000000,
        available_coverage: 900000,
        allocated_coverage: 100000,
        member_count: 5,
        max_coverage_ratio: 10000,
        operator_fee_bp: 100,
        created_at: 100,
        total_slashed: 0,
        pool_slash_count: 0,
        is_active: true,
    };

    let encoded = registry.encode();
    let decoded = PoolStakeRegistry::decode(&encoded).unwrap();

    assert_eq!(decoded.pool_id, registry.pool_id);
    assert_eq!(decoded.total_stake, 1000000);
    assert_eq!(decoded.available_coverage, 900000);
    assert_eq!(decoded.member_count, 5);
    assert!(decoded.is_active);
}

#[test]
fn test_pool_member_stake_encoding() {
    let stake = PoolMemberStake {

        version: 0,        stake_id: make_base([1u8; 32]),
        pool_id: make_base([2u8; 32]),
        member_pub: make_pubkey(3),
        relayer_id: [4u8; 32],
        original_amount: 1000000,
        current_amount: 950000,
        coverage_contribution: 900000,
        pool_share_bp: 2000,  // 20% share
        accumulated_fees: 5000,
        created_at: 100,
        leave_requested_at: None,
        slash_count: 0,
        is_active: true,
        instance_seed: [0u8; 32],
    };

    let encoded = stake.encode();
    let decoded = PoolMemberStake::decode(&encoded).unwrap();

    assert_eq!(decoded.stake_id, stake.stake_id);
    assert_eq!(decoded.original_amount, 1000000);
    assert_eq!(decoded.pool_share_bp, 2000);
    assert!(decoded.is_active);
}

#[test]
fn test_pool_member_stake_with_leave_request() {
    let stake = PoolMemberStake {

        version: 0,        stake_id: make_base([1u8; 32]),
        pool_id: make_base([2u8; 32]),
        member_pub: make_pubkey(3),
        relayer_id: [4u8; 32],
        original_amount: 1000000,
        current_amount: 950000,
        coverage_contribution: 900000,
        pool_share_bp: 2000,
        accumulated_fees: 5000,
        created_at: 100,
        leave_requested_at: Some(200),
        slash_count: 0,
        is_active: true,
        instance_seed: [0u8; 32],
    };

    let encoded = stake.encode();
    let decoded = PoolMemberStake::decode(&encoded).unwrap();

    assert!(decoded.leave_requested_at.is_some());
    assert_eq!(decoded.leave_requested_at.unwrap(), 200);
}

#[test]
fn test_coverage_allocation_encoding() {
    let allocation = CoverageAllocation {

        version: 0,        allocation_id: make_base([1u8; 32]),
        pool_id: make_base([2u8; 32]),
        withdrawal_nullifier: [3u8; 32],
        amount: 100000,
        contributing_members: vec![make_base([4u8; 32]), make_base([5u8; 32])],
        created_at: 100,
        timeout_height: 500,
        executed: false,
        slashed: false,
    };

    let encoded = allocation.encode().unwrap();
    let decoded = CoverageAllocation::decode(&encoded).unwrap();

    assert_eq!(decoded.allocation_id, allocation.allocation_id);
    assert_eq!(decoded.amount, 100000);
    assert_eq!(decoded.contributing_members.len(), 2);
    assert!(!decoded.executed);
    assert!(!decoded.slashed);
}

/// `CoverageAllocation` is **stored state** and its member count was a `u8`. 256 is the smallest
/// count that truncates to `0` — after which `created_at`, `timeout_height`, `executed` and
/// `slashed` all move. The test above asserts `executed`/`slashed` but not the two heights, and it
/// uses two members, so it could not see either the truncation or the shift.
#[test]
fn test_coverage_allocation_member_count_is_not_a_byte() {
    let members: Vec<pallas::Base> = (1..=256u64).map(pallas::Base::from).collect();
    let allocation = CoverageAllocation {
        version: 0,
        allocation_id: make_base([7u8; 32]),
        pool_id: make_base([8u8; 32]),
        withdrawal_nullifier: [9u8; 32],
        amount: 4242,
        contributing_members: members.clone(),
        created_at: 111,
        timeout_height: 222,
        executed: true,
        slashed: true,
    };

    let encoded = allocation.encode().unwrap();
    assert_eq!(encoded.len(), 127 + 256 * 32);

    let decoded = CoverageAllocation::decode(&encoded).unwrap();
    assert_eq!(decoded.contributing_members.len(), 256);
    assert_eq!(decoded.contributing_members[255], members[255]);
    assert_eq!(decoded.created_at, 111);
    assert_eq!(decoded.timeout_height, 222);
    assert!(decoded.executed);
    assert!(decoded.slashed);
}

/// The rebalance param's member list, with 256 ids — the count's width is what a truncation would
/// corrupt, and the exact-length check is where it would be caught or missed.
#[test]
fn test_rebalance_pool_shares_count_is_not_a_byte() {
    let member_ids: Vec<pallas::Base> = (1..=256u64).map(pallas::Base::from).collect();
    let params = RebalancePoolSharesParamsV1 {
        pool_id: make_base([3u8; 32]),
        owner_pub: make_pubkey(4),
        member_ids: member_ids.clone(),
    };

    let encoded = params.encode().unwrap();
    assert_eq!(encoded.len(), 68 + 256 * 32);

    let decoded = RebalancePoolSharesParamsV1::decode(&encoded).unwrap();
    assert_eq!(decoded.member_ids.len(), 256);
    assert_eq!(decoded.member_ids[255], member_ids[255]);
    assert_eq!(decoded.pool_id, params.pool_id);
}

#[test]
fn test_create_pool_params_encoding() {
    let params = CreatePoolParamsV1 {
        instance_seed: [0u8; 32],
        owner_pub: make_pubkey(1),
        max_coverage_ratio: 10000,
        operator_fee_bp: 100,
        pool_config_hash: make_base([5u8; 32]),
        nonce: 1,
        derived_pool_id: make_base([6u8; 32]),
    };

    let encoded = serialize(&params);
    let decoded: CreatePoolParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.owner_pub, params.owner_pub);
    assert_eq!(decoded.max_coverage_ratio, 10000);
    assert_eq!(decoded.operator_fee_bp, 100);
}

#[test]
fn test_create_pool_update_encoding() {
    let update = CreatePoolUpdateV1 {
        instance_seed: [0u8; 32],
        pool_id: make_base([1u8; 32]),
        owner_pub: make_pubkey(2),
        max_coverage_ratio: 10000,
        operator_fee_bp: 100,
        created_at: 100,
    };

    let encoded = serialize(&update);
    let decoded: CreatePoolUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.pool_id, update.pool_id);
    assert_eq!(decoded.created_at, 100);
}

#[test]
fn test_join_pool_params_encoding() {
    let params = JoinPoolParamsV1 {
        instance_seed: [0u8; 32],
        pool_id: make_base([1u8; 32]),
        amount: 1000000,
        relayer_id: [2u8; 32],
        member_pub: make_pubkey(5),
        asset_id: make_base([3u8; 32]),
        nonce: 1,
        derived_member_id: make_base([4u8; 32]),
        value_commit_x: make_base([5u8; 32]),
        value_commit_y: make_base([6u8; 32]),
    };

    let encoded = serialize(&params);
    let decoded: JoinPoolParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.pool_id, params.pool_id);
    assert_eq!(decoded.amount, 1000000);
}

#[test]
fn test_join_pool_update_encoding() {
    // The update carries the finished registry record, so apply can re-store it without a
    // db_get-in-apply (contract-wasm-type-system.md §B.2.2 / register OBL-C72).
    let pool = PoolStakeRegistry {
        version: 0,
        pool_id: make_base([2u8; 32]),
        owner_pub: make_pubkey(2),
        total_stake: 5000000,
        available_coverage: 4500000,
        allocated_coverage: 100000,
        member_count: 5,
        max_coverage_ratio: 10000,
        operator_fee_bp: 100,
        created_at: 100,
        total_slashed: 0,
        pool_slash_count: 0,
        is_active: true,
    };

    let update = JoinPoolUpdateV1 {
        instance_seed: [0u8; 32],
        stake_id: make_base([1u8; 32]),
        member_pub: make_pubkey(3),
        relayer_id: [4u8; 32],
        amount: 1000000,
        coverage_contribution: 900000,
        pool_share_bp: 2000,
        created_at: 4242,
        pool,
    };

    let encoded = serialize(&update);
    let decoded: JoinPoolUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.stake_id, update.stake_id);
    assert_eq!(decoded.pool_share_bp, 2000);
    assert_eq!(decoded.created_at, 4242);
    // The carried record must survive the round trip intact — this is the value apply writes.
    assert_eq!(decoded.pool.member_count, 5);
    assert_eq!(decoded.pool.total_stake, 5000000);
    assert_eq!(decoded.pool.pool_id, update.pool.pool_id);
}

#[test]
fn test_leave_pool_params_encoding() {
    let params = LeavePoolParamsV1 {
        stake_id: make_base([1u8; 32]),
    };

    let encoded = serialize(&params);
    let decoded: LeavePoolParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.stake_id, params.stake_id);
}

#[test]
fn test_leave_pool_update_encoding() {
    // The update carries the stake with its post-leave state applied, so apply re-stores it
    // without a db_get-in-apply (OBL-C72). It also carries the cooldown-start transition.
    let mut stake = make_member_stake();
    stake.is_active = false;
    stake.current_amount = 0;

    let update = LeavePoolUpdateV1 {
        stake,
        payout_amount: 950000,
        unstake_penalty: 50000,
    };

    let encoded = serialize(&update);
    let decoded: LeavePoolUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.stake.stake_id, update.stake.stake_id);
    assert!(!decoded.stake.is_active);
    assert_eq!(decoded.payout_amount, 950000);
    assert_eq!(decoded.unstake_penalty, 50000);
}

#[test]
fn test_allocate_coverage_params_encoding() {
    let params = AllocateCoverageParamsV1 {
        pool_id: make_base([1u8; 32]),
        withdrawal_nullifier: [2u8; 32],
        amount: 100000,
        timeout_height: 500,
        member_pub: make_pubkey(5),
        withdrawal_id: make_base([3u8; 32]),
        nonce: 1,
        derived_allocation_id: make_base([4u8; 32]),
    };

    let encoded = serialize(&params);
    let decoded: AllocateCoverageParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.pool_id, params.pool_id);
    assert_eq!(decoded.amount, 100000);
    assert_eq!(decoded.timeout_height, 500);
}

#[test]
fn test_allocate_coverage_update_encoding() {
    let pool = PoolStakeRegistry {
        version: 0,
        pool_id: make_base([2u8; 32]),
        owner_pub: make_pubkey(2),
        total_stake: 1000000,
        available_coverage: 800000,
        allocated_coverage: 200000,
        member_count: 5,
        max_coverage_ratio: 10000,
        operator_fee_bp: 100,
        created_at: 100,
        total_slashed: 0,
        pool_slash_count: 0,
        is_active: true,
    };

    let update = AllocateCoverageUpdateV1 {
        allocation_id: make_base([1u8; 32]),
        withdrawal_nullifier: [3u8; 32],
        amount: 100000,
        contributing_members: vec![make_base([4u8; 32]), make_base([5u8; 32])],
        timeout_height: 500,
        created_at: 4242,
        pool,
    };

    let encoded = serialize(&update);
    let decoded: AllocateCoverageUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.allocation_id, update.allocation_id);
    assert_eq!(decoded.contributing_members.len(), 2);
    assert_eq!(decoded.created_at, 4242);
    // The carried record survives the round trip — this is the value apply writes.
    assert_eq!(decoded.pool.available_coverage, 800000);
    assert_eq!(decoded.pool.pool_id, update.pool.pool_id);
}

#[test]
fn test_release_coverage_params_encoding() {
    let params = ReleaseCoverageParamsV1 {
        allocation_id: make_base([1u8; 32]),
        owner_pub: make_pubkey(5),
    };

    let encoded = serialize(&params);
    let decoded: ReleaseCoverageParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.allocation_id, params.allocation_id);
}

#[test]
fn test_release_coverage_update_encoding() {
    let mut allocation = make_allocation();
    allocation.executed = true;
    let allocation_bytes = allocation.encode().unwrap();

    let mut pool = make_registry();
    pool.available_coverage = 900000;
    pool.allocated_coverage = 100000;

    let update = ReleaseCoverageUpdateV1 {
        allocation_id: make_base([1u8; 32]),
        released_amount: 100000,
        allocation_bytes,
        pool,
    };

    let encoded = serialize(&update);
    let decoded: ReleaseCoverageUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.allocation_id, update.allocation_id);
    assert_eq!(decoded.released_amount, 100000);
    // The carried record survives the bridge; the allocation bytes are the record apply stores.
    assert_eq!(decoded.pool.available_coverage, 900000);
    assert_eq!(decoded.pool.pool_id, update.pool.pool_id);
    assert_eq!(CoverageAllocation::decode(&decoded.allocation_bytes).unwrap().executed, true);
}

#[test]
fn test_slash_coverage_params_encoding() {
    let params = SlashCoverageParamsV1 {
        allocation_id: make_base([1u8; 32]),
        owner_pub: make_pubkey(5),
        slash_amount: 100000,
        user_pub: make_pubkey(6),
        nonce: 1,
        derived_slash_id: make_base([7u8; 32]),
    };

    let encoded = serialize(&params);
    let decoded: SlashCoverageParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.allocation_id, params.allocation_id);
    assert_eq!(decoded.slash_amount, 100000);
}

#[test]
fn test_slash_coverage_update_encoding() {
    let mut allocation = make_allocation();
    allocation.slashed = true;
    let allocation_bytes = allocation.encode().unwrap();

    let mut member = make_member_stake();
    member.slash_count = 1;

    let mut pool = make_registry();
    pool.available_coverage = 800000;
    pool.allocated_coverage = 0;
    pool.total_slashed = 100000;
    pool.pool_slash_count = 1;

    let update = SlashCoverageUpdateV1 {
        allocation_id: make_base([1u8; 32]),
        slashed_amount: 100000,
        compensated_user: [2u8; 32],
        allocation_bytes,
        member_stakes: vec![member],
        pool,
    };

    let encoded = serialize(&update);
    let decoded: SlashCoverageUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.slashed_amount, 100000);
    assert_eq!(decoded.pool.available_coverage, 800000);
    assert_eq!(decoded.pool.pool_slash_count, 1);
    // The per-member adjustments and the slashed allocation ride the bridge intact.
    assert_eq!(decoded.member_stakes.len(), 1);
    assert_eq!(decoded.member_stakes[0].slash_count, 1);
    assert_eq!(CoverageAllocation::decode(&decoded.allocation_bytes).unwrap().slashed, true);
}

#[test]
fn test_claim_fees_params_encoding() {
    let params = ClaimFeesParamsV1 {
        stake_id: make_base([1u8; 32]),
        owner_pub: make_pubkey(5),
    };

    let encoded = serialize(&params);
    let decoded: ClaimFeesParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.stake_id, params.stake_id);
}

#[test]
fn test_claim_fees_update_encoding() {
    // Fees zeroed in exec and carried, so apply re-stores rather than reading (OBL-C72).
    let mut stake = make_member_stake();
    stake.accumulated_fees = 0;

    let update = ClaimFeesUpdateV1 { stake, claimed_amount: 5000 };

    let encoded = serialize(&update);
    let decoded: ClaimFeesUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.stake.stake_id, update.stake.stake_id);
    assert_eq!(decoded.claimed_amount, 5000);
    assert_eq!(decoded.stake.accumulated_fees, 0);
}

#[test]
fn test_update_pool_config_params_encoding() {
    let params = UpdatePoolConfigParamsV1 {
        pool_id: make_base([1u8; 32]),
        owner_pub: make_pubkey(5),
        max_coverage_ratio: Some(15000),
        operator_fee_bp: Some(200),
    };

    let encoded = serialize(&params);
    let decoded: UpdatePoolConfigParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.pool_id, params.pool_id);
    assert!(decoded.max_coverage_ratio.is_some());
    assert!(decoded.operator_fee_bp.is_some());
}

#[test]
fn test_update_pool_config_update_encoding() {
    let mut pool = make_registry();
    pool.max_coverage_ratio = 15000;
    pool.operator_fee_bp = 200;

    let update = UpdatePoolConfigUpdateV1 { pool };

    let encoded = serialize(&update);
    let decoded: UpdatePoolConfigUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.pool.pool_id, update.pool.pool_id);
    assert_eq!(decoded.pool.max_coverage_ratio, 15000);
    assert_eq!(decoded.pool.operator_fee_bp, 200);
}

#[test]
fn test_constants() {
    // Verify minimum stake amount (1 DAI equivalent)
    assert_eq!(POOL_STAKE_MIN_STAKE, 1_000_000);

    // Verify maximum coverage ratio (10000 = 1:1 stake:coverage)
    assert_eq!(POOL_STAKE_MAX_COVERAGE_RATIO, 10000);

    // Verify leave cooldown period (100 blocks)
    assert_eq!(POOL_STAKE_LEAVE_COOLDOWN_BLOCKS, 100);

    // Verify basis points precision
    assert_eq!(POOL_STAKE_BP_PRECISION, 10000);
}