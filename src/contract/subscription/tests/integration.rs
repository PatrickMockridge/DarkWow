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

//! Subscription contract integration tests

use dwow_serial::{deserialize, serialize};
use dwow_sdk::{
    crypto::{pasta_prelude::Group, PublicKey, SecretKey},
    pasta::pallas,
};
use dwow_subscription_contract::{
    model::{
        permissions, CancelParamsV1, CancelUpdateV1, DaoControlAction, DaoControlParamsV1,
        DaoControlUpdateV1, Plan, RenewParamsV1, RenewUpdateV1, SubscribeParamsV1,
        SubscribeUpdateV1, Subscription, SubscriptionCapability, SubscriptionId, SubscriptionState,
        UpdateUsageParamsV1, UpdateUsageUpdateV1, VerifyAccessParamsV1,
    },
    SubscriptionFunction,
    // Constants
    SUBSCRIPTION_CONTRACT_INFO_TREE, SUBSCRIPTION_CONTRACT_SUBSCRIPTIONS_TREE,
    SUBSCRIPTION_CONTRACT_NULLIFIERS_TREE, SUBSCRIPTION_CONTRACT_PLANS_TREE,
};

/// Helper to create PublicKey from a numeric seed
fn make_pubkey(seed: u64) -> PublicKey {
    let secret = SecretKey::from(pallas::Base::from(seed));
    PublicKey::from_secret(secret)
}

/// Helper to create a dummy subscription for testing
fn create_dummy_subscription(id: SubscriptionId) -> Subscription {
    Subscription {
        id,
        subscriber_pubkey: make_pubkey(1),
        plan_id: 1,
        lock_until_block: 100000,
        deposit: 1000,
        token_id: pallas::Base::zero(),
        value_commit: Group::identity(),
        state: SubscriptionState::Active,
        spent_nullifier: pallas::Base::zero(),
        created_at: 50000,
        dao_escrow_bulla: None,
        dao_membership_note: None,
        uses_allowed: 100,
        rate_period: 1000,
        period_uses: 5,
        last_access_block: 50000,
        uses_remaining: 95,
        instance_seed: [0u8; 32],
    }
}

#[test]
fn test_subscription_function_enum_valid() {
    assert!(SubscriptionFunction::try_from(0x00).is_ok()); // InitializeV1
    assert!(SubscriptionFunction::try_from(0x01).is_ok()); // SubscribeV1
    assert!(SubscriptionFunction::try_from(0x02).is_ok()); // CancelV1
    assert!(SubscriptionFunction::try_from(0x03).is_ok()); // RenewV1
    assert!(SubscriptionFunction::try_from(0x04).is_ok()); // VerifyAccessV1
    assert!(SubscriptionFunction::try_from(0x05).is_ok()); // DaoControlV1
    assert!(SubscriptionFunction::try_from(0x06).is_ok()); // UpdateUsageV1
}

#[test]
fn test_subscription_function_enum_invalid() {
    assert!(SubscriptionFunction::try_from(0xFF).is_err());
    assert!(SubscriptionFunction::try_from(0x07).is_err());
    assert!(SubscriptionFunction::try_from(0x10).is_err());
}

#[test]
fn test_subscription_state_from_u8() {
    assert_eq!(SubscriptionState::try_from(0).unwrap(), SubscriptionState::Active);
    assert_eq!(SubscriptionState::try_from(1).unwrap(), SubscriptionState::Cancelled);
    assert_eq!(SubscriptionState::try_from(2).unwrap(), SubscriptionState::Expired);
    assert!(SubscriptionState::try_from(3).is_err());
    assert!(SubscriptionState::try_from(255).is_err());
}

#[test]
fn test_subscription_derive_id() {
    let subscriber_pubkey = make_pubkey(42);
    let plan_id: u32 = 1;
    let deposit: u64 = 1000;
    let token_id = pallas::Base::zero();
    let lock_until_block: u64 = 100000;
    let subscriber_secret = pallas::Base::from(42);
    let plan_nonce = pallas::Base::from(1);

    let id = Subscription::derive_id(
        &subscriber_pubkey,
        plan_id,
        deposit,
        token_id,
        lock_until_block,
        subscriber_secret,
        plan_nonce,
    );

    // Should be deterministic
    let id2 = Subscription::derive_id(
        &subscriber_pubkey,
        plan_id,
        deposit,
        token_id,
        lock_until_block,
        subscriber_secret,
        plan_nonce,
    );
    assert_eq!(id, id2);
}

#[test]
fn test_subscription_compute_nullifier() {
    let subscription = create_dummy_subscription(pallas::Base::from(1));
    let secret = pallas::Base::from(99);
    let nullifier = subscription.compute_nullifier(secret);

    // Should be deterministic
    let nullifier2 = subscription.compute_nullifier(secret);
    assert_eq!(nullifier, nullifier2);
}

#[test]
fn test_subscription_capability_derive() {
    let subscriber = make_pubkey(42);
    let plan_id: u32 = 1;
    let subscription_id = pallas::Base::from(1);
    let permissions: u8 = permissions::READ | permissions::WRITE;
    let expires_at: u64 = 100000;
    let nonce = pallas::Base::from(42);

    let capability = SubscriptionCapability::derive_capability(
        &subscriber,
        plan_id,
        subscription_id,
        permissions,
        expires_at,
        nonce,
    );

    // Should be deterministic
    let capability2 = SubscriptionCapability::derive_capability(
        &subscriber,
        plan_id,
        subscription_id,
        permissions,
        expires_at,
        nonce,
    );
    assert_eq!(capability, capability2);
}

#[test]
fn test_permission_constants() {
    assert_eq!(permissions::READ, 0b0000_0001);
    assert_eq!(permissions::WRITE, 0b0000_0010);
    assert_eq!(permissions::CANCEL, 0b0000_0100);
    assert_eq!(permissions::RENEW, 0b0000_1000);
    assert_eq!(permissions::ADMIN, 0b1000_0000);
}

#[test]
fn test_subscription_encoding() {
    let subscription = Subscription {
        id: pallas::Base::from(1),
        subscriber_pubkey: make_pubkey(1),
        plan_id: 1,
        lock_until_block: 100000,
        deposit: 1000,
        token_id: pallas::Base::zero(),
        value_commit: Group::identity(),
        state: SubscriptionState::Active,
        spent_nullifier: pallas::Base::zero(),
        created_at: 50000,
        dao_escrow_bulla: Some(pallas::Base::from(2)),
        dao_membership_note: Some(pallas::Base::from(3)),
        uses_allowed: 100,
        rate_period: 1000,
        period_uses: 5,
        last_access_block: 50000,
        uses_remaining: 95,
        instance_seed: [0u8; 32],
    };

    let encoded = serialize(&subscription);
    let decoded: Subscription = deserialize(&encoded).unwrap();

    assert_eq!(decoded.id, subscription.id);
    assert_eq!(decoded.plan_id, subscription.plan_id);
    assert_eq!(decoded.state, subscription.state);
    assert_eq!(decoded.deposit, subscription.deposit);
}

#[test]
fn test_plan_encoding() {
    let plan = Plan {
        id: 1,
        name_hash: pallas::Base::from(1),
        price: 1000,
        token_id: pallas::Base::zero(),
        duration_blocks: 10000,
        treasury_share: 8000,
        endowment_share: 2000,
        active: true,
        dao_escrow_discount: 2000,
        required_dao_escrow: Some(pallas::Base::from(2)),
    };

    let encoded = serialize(&plan);
    let decoded: Plan = deserialize(&encoded).unwrap();

    assert_eq!(decoded.id, plan.id);
    assert_eq!(decoded.price, plan.price);
    assert_eq!(decoded.treasury_share, plan.treasury_share);
    assert_eq!(decoded.active, plan.active);
}

#[test]
fn test_subscribe_params_encoding() {
    let params = SubscribeParamsV1 {
        plan_id: 1,
        subscriber_pubkey: make_pubkey(1),
        commitment: pallas::Base::from(1),
        value_commit: Group::identity(),
        merkle_proof: vec![pallas::Base::from(1)],
        merkle_root: pallas::Base::from(2),
        dao_escrow_bulla: Some(pallas::Base::from(3)),
        dao_membership_note: Some(pallas::Base::from(4)),
        dao_escrow_merkle_root: Some(pallas::Base::from(5)),
        dao_merkle_proof: Some(vec![pallas::Base::from(6)]),
        dao_leaf_pos: Some(1),
        instance_seed: [0u8; 32],
    };

    let encoded = serialize(&params);
    let decoded: SubscribeParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.plan_id, params.plan_id);
    assert_eq!(decoded.commitment, params.commitment);
}

#[test]
fn test_subscribe_update_encoding() {
    let subscription = create_dummy_subscription(pallas::Base::zero());
    let update = SubscribeUpdateV1 { subscription: subscription.clone() };

    let encoded = serialize(&update);
    let decoded: SubscribeUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.subscription.id, subscription.id);
}

#[test]
fn test_cancel_params_encoding() {
    let recipient_pubkey = make_pubkey(1);
    let params = CancelParamsV1 {
        subscription_id: pallas::Base::zero(),
        subscriber_secret: pallas::Base::zero(),
        spent_nullifier: pallas::Base::zero(),
        current_block: 50000,
        recipient_pubkey,
    };

    let encoded = serialize(&params);
    let decoded: CancelParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.subscription_id, params.subscription_id);
    assert_eq!(decoded.current_block, params.current_block);
}

#[test]
fn test_cancel_update_encoding() {
    let subscription = create_dummy_subscription(pallas::Base::zero());
    let update = CancelUpdateV1 {
        subscription_id: subscription.id,
        spent_nullifier: pallas::Base::zero(),
        updated_subscription: subscription.clone(),
    };

    let encoded = serialize(&update);
    let decoded: CancelUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.subscription_id, update.subscription_id);
    assert_eq!(decoded.spent_nullifier, update.spent_nullifier);
    assert_eq!(decoded.updated_subscription.id, subscription.id);
}

#[test]
fn test_renew_params_encoding() {
    let params = RenewParamsV1 {
        subscription_id: pallas::Base::from(1),
        subscriber_secret: pallas::Base::from(2),
        new_lock_until_block: 110000,
        spent_nullifier: pallas::Base::from(3),
        value_commit: Group::identity(),
        merkle_proof: vec![pallas::Base::from(4)],
    };

    let encoded = serialize(&params);
    let decoded: RenewParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.subscription_id, params.subscription_id);
    assert_eq!(decoded.new_lock_until_block, params.new_lock_until_block);
}

#[test]
fn test_renew_update_encoding() {
    let subscription = create_dummy_subscription(pallas::Base::zero());
    let update = RenewUpdateV1 {
        subscription_id: subscription.id,
        spent_nullifier: pallas::Base::zero(),
        new_subscription: subscription.clone(),
    };

    let encoded = serialize(&update);
    let decoded: RenewUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.subscription_id, update.subscription_id);
    assert_eq!(decoded.spent_nullifier, update.spent_nullifier);
    assert_eq!(decoded.new_subscription.id, subscription.id);
}

#[test]
fn test_verify_access_params_encoding() {
    let params = VerifyAccessParamsV1 {
        subscription_id: pallas::Base::from(1),
        capability: pallas::Base::from(2),
        nonce: pallas::Base::from(3),
    };

    let encoded = serialize(&params);
    let decoded: VerifyAccessParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.subscription_id, params.subscription_id);
    assert_eq!(decoded.capability, params.capability);
}

#[test]
fn test_dao_control_update_encoding() {
    let update = DaoControlUpdateV1 {
        action: DaoControlAction::PlanUpdated(1),
    };

    let encoded = serialize(&update);
    let decoded: DaoControlUpdateV1 = deserialize(&encoded).unwrap();

    match decoded.action {
        DaoControlAction::PlanUpdated(id) => assert_eq!(id, 1),
        _ => panic!("Expected PlanUpdated"),
    }
}

#[test]
fn test_constants() {
    assert_eq!(SUBSCRIPTION_CONTRACT_INFO_TREE, "info");
    assert_eq!(SUBSCRIPTION_CONTRACT_SUBSCRIPTIONS_TREE, "subscriptions");
    assert_eq!(SUBSCRIPTION_CONTRACT_NULLIFIERS_TREE, "nullifiers");
    assert_eq!(SUBSCRIPTION_CONTRACT_PLANS_TREE, "plans");
}

#[test]
fn test_update_usage_params_encoding() {
    let params = UpdateUsageParamsV1 {
        subscription_id: pallas::Base::from(1),
        subscriber_pub_x: pallas::Base::from(2),
        subscriber_pub_y: pallas::Base::from(3),
        subscriber_secret: pallas::Base::from(4),
        current_block: 50000,
        nonce: pallas::Base::from(5),
        spent_nullifier: pallas::Base::from(6),
        merkle_proof: vec![pallas::Base::from(7)],
    };

    let encoded = serialize(&params);
    let decoded: UpdateUsageParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.subscription_id, params.subscription_id);
    assert_eq!(decoded.current_block, params.current_block);
}

#[test]
fn test_update_usage_update_encoding() {
    let update = UpdateUsageUpdateV1 {
        subscription_id: pallas::Base::from(1),
        period_uses: 5,
        last_access_block: 50010,
        uses_remaining: 95,
        is_new_period: false,
    };

    let encoded = serialize(&update);
    let decoded: UpdateUsageUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.subscription_id, update.subscription_id);
    assert_eq!(decoded.period_uses, update.period_uses);
    assert_eq!(decoded.last_access_block, update.last_access_block);
    assert_eq!(decoded.uses_remaining, update.uses_remaining);
    assert_eq!(decoded.is_new_period, update.is_new_period);
}

#[test]
fn test_update_usage_new_period_encoding() {
    let update = UpdateUsageUpdateV1 {
        subscription_id: pallas::Base::from(1),
        period_uses: 0,
        last_access_block: 100000,
        uses_remaining: 100,
        is_new_period: true,
    };

    let encoded = serialize(&update);
    let decoded: UpdateUsageUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.is_new_period, true);
    assert_eq!(decoded.period_uses, 0);
    assert_eq!(decoded.uses_remaining, 100);
}

#[test]
fn test_subscription_with_rate_limits() {
    let subscription = Subscription {
        id: pallas::Base::from(1),
        subscriber_pubkey: make_pubkey(1),
        plan_id: 1,
        lock_until_block: 100000,
        deposit: 1000,
        token_id: pallas::Base::zero(),
        value_commit: Group::identity(),
        state: SubscriptionState::Active,
        spent_nullifier: pallas::Base::zero(),
        created_at: 50000,
        dao_escrow_bulla: None,
        dao_membership_note: None,
        uses_allowed: 100,
        rate_period: 1000,
        period_uses: 5,
        last_access_block: 50000,
        uses_remaining: 95,
        instance_seed: [0u8; 32],
    };

    // Verify rate limit fields are set correctly
    assert_eq!(subscription.uses_allowed, 100);
    assert_eq!(subscription.rate_period, 1000);
    assert_eq!(subscription.period_uses, 5);
    assert_eq!(subscription.last_access_block, 50000);
    assert_eq!(subscription.uses_remaining, 95);
}

#[test]
fn test_subscription_encoding_with_rate_limits() {
    let subscription = Subscription {
        id: pallas::Base::from(1),
        subscriber_pubkey: make_pubkey(1),
        plan_id: 1,
        lock_until_block: 100000,
        deposit: 1000,
        token_id: pallas::Base::zero(),
        value_commit: Group::identity(),
        state: SubscriptionState::Active,
        spent_nullifier: pallas::Base::zero(),
        created_at: 50000,
        dao_escrow_bulla: None,
        dao_membership_note: None,
        uses_allowed: 100,
        rate_period: 1000,
        period_uses: 5,
        last_access_block: 50000,
        uses_remaining: 95,
        instance_seed: [0u8; 32],
    };

    let encoded = serialize(&subscription);
    let decoded: Subscription = deserialize(&encoded).unwrap();

    assert_eq!(decoded.uses_allowed, 100);
    assert_eq!(decoded.rate_period, 1000);
    assert_eq!(decoded.period_uses, 5);
    assert_eq!(decoded.last_access_block, 50000);
    assert_eq!(decoded.uses_remaining, 95);
}

#[test]
fn test_rate_limit_scenario_new_period() {
    // Simulate a new period scenario
    let uses_allowed: u64 = 100;
    let rate_period: u64 = 1000;
    let period_uses: u64 = 0;  // Reset in new period
    let last_access_block: u64 = 50000;
    let current_block: u64 = 51000;  // 1000 blocks later, new period

    // Check if new period
    let blocks_since_last = current_block.saturating_sub(last_access_block);
    let is_new_period = blocks_since_last >= rate_period;

    assert!(is_new_period);

    // In new period, uses_remaining should equal uses_allowed
    let uses_remaining = if is_new_period { uses_allowed } else { uses_allowed - period_uses };
    assert_eq!(uses_remaining, 100);
}

#[test]
fn test_rate_limit_scenario_same_period() {
    // Simulate same period scenario
    let uses_allowed: u64 = 100;
    let rate_period: u64 = 1000;
    let period_uses: u64 = 5;  // 5 uses already consumed
    let last_access_block: u64 = 50000;
    let current_block: u64 = 50500;  // Only 500 blocks later, same period

    // Check if new period
    let blocks_since_last = current_block.saturating_sub(last_access_block);
    let is_new_period = blocks_since_last >= rate_period;

    assert!(!is_new_period);

    // In same period, uses_remaining = uses_allowed - period_uses
    let uses_remaining = if is_new_period { uses_allowed } else { uses_allowed - period_uses };
    assert_eq!(uses_remaining, 95);
}

#[test]
fn test_rate_limit_scenario_exhausted() {
    // Simulate exhausted rate limit scenario
    let uses_allowed: u64 = 100;
    let rate_period: u64 = 1000;
    let period_uses: u64 = 100;  // All uses consumed
    let last_access_block: u64 = 50000;
    let current_block: u64 = 50500;  // Same period

    // Check if new period
    let blocks_since_last = current_block.saturating_sub(last_access_block);
    let is_new_period = blocks_since_last >= rate_period;

    assert!(!is_new_period);

    // In same period, uses_remaining = uses_allowed - period_uses = 0
    let uses_remaining = if is_new_period { uses_allowed } else { uses_allowed - period_uses };
    assert_eq!(uses_remaining, 0);

    // Access should be denied when uses_remaining == 0
    let access_granted = uses_remaining > 0;
    assert!(!access_granted);
}
