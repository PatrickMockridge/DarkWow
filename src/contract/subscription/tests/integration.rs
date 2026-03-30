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

//! Subscription contract integration tests

use darkfi_subscription_contract::{
    model::{
        permissions, CancelParamsV1, CancelUpdateV1, DaoControlAction, DaoControlParamsV1,
        DaoControlUpdateV1, Plan, RenewParamsV1, RenewUpdateV1, SubscribeParamsV1,
        SubscribeUpdateV1, Subscription, SubscriptionCapability, SubscriptionId, SubscriptionState,
        VerifyAccessParamsV1,
    },
    SubscriptionFunction,
    // Constants
    SUBSCRIPTION_CONTRACT_INFO_TREE, SUBSCRIPTION_CONTRACT_SUBSCRIPTIONS_TREE,
    SUBSCRIPTION_CONTRACT_NULLIFIERS_TREE, SUBSCRIPTION_CONTRACT_PLANS_TREE,
};

#[test]
fn test_subscription_function_enum_valid() {
    assert!(SubscriptionFunction::try_from(0x00).is_ok()); // InitializeV1
    assert!(SubscriptionFunction::try_from(0x01).is_ok()); // SubscribeV1
    assert!(SubscriptionFunction::try_from(0x02).is_ok()); // CancelV1
    assert!(SubscriptionFunction::try_from(0x03).is_ok()); // RenewV1
    assert!(SubscriptionFunction::try_from(0x04).is_ok()); // VerifyAccessV1
    assert!(SubscriptionFunction::try_from(0x05).is_ok()); // DaoControlV1
}

#[test]
fn test_subscription_function_enum_invalid() {
    assert!(SubscriptionFunction::try_from(0xFF).is_err());
    assert!(SubscriptionFunction::try_from(0x06).is_err());
    assert!(SubscriptionFunction::try_from(0x10).is_err());
}

#[test]
fn test_subscription_state_from_u8() {
    assert_eq!(SubscriptionState::try_from(0), Ok(SubscriptionState::Active));
    assert_eq!(SubscriptionState::try_from(1), Ok(SubscriptionState::Cancelled));
    assert_eq!(SubscriptionState::try_from(2), Ok(SubscriptionState::Expired));
    assert!(SubscriptionState::try_from(3).is_err());
    assert!(SubscriptionState::try_from(255).is_err());
}

#[test]
fn test_subscription_derive_id() {
    let subscriber_pubkey = darkfi_sdk::crypto::PublicKey::from_publickey(
        &darkfi_sdk::crypto::Keypair::random(&mut rand::rngs::OsRng).public,
    );
    let plan_id: u32 = 1;
    let deposit: u64 = 1000;
    let token_id = darkfi_sdk::pasta::pallas::Base::ONE;
    let lock_until_block: u64 = 100000;
    let subscriber_secret = darkfi_sdk::pasta::pallas::Base::from(42);
    let plan_nonce = darkfi_sdk::pasta::pallas::Base::from(1);

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
    let subscription = Subscription {
        id: darkfi_sdk::pasta::pallas::Base::from(1),
        subscriber_pubkey: darkfi_sdk::crypto::PublicKey::from_publickey(
            &darkfi_sdk::crypto::Keypair::random(&mut rand::rngs::OsRng).public,
        ),
        plan_id: 1,
        lock_until_block: 100000,
        deposit: 1000,
        token_id: darkfi_sdk::pasta::pallas::Base::ONE,
        value_commit: darkfi_sdk::pasta::pallas::Point::identity(),
        state: SubscriptionState::Active,
        spent_nullifier: darkfi_sdk::pasta::pallas::Base::ZERO,
        created_at: 50000,
        dao_escrow_bulla: None,
        dao_membership_note: None,
    };

    let secret = darkfi_sdk::pasta::pallas::Base::from(99);
    let nullifier = subscription.compute_nullifier(secret);

    // Should be deterministic
    let nullifier2 = subscription.compute_nullifier(secret);
    assert_eq!(nullifier, nullifier2);
}

#[test]
fn test_subscription_capability_derive() {
    let subscriber = darkfi_sdk::crypto::PublicKey::from_publickey(
        &darkfi_sdk::crypto::Keypair::random(&mut rand::rngs::OsRng).public,
    );
    let plan_id: u32 = 1;
    let subscription_id = darkfi_sdk::pasta::pallas::Base::from(1);
    let permissions: u8 = permissions::READ | permissions::WRITE;
    let expires_at: u64 = 100000;
    let nonce = darkfi_sdk::pasta::pallas::Base::from(42);

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
        id: darkfi_sdk::pasta::pallas::Base::from(1),
        subscriber_pubkey: darkfi_sdk::crypto::PublicKey::from_publickey(
            &darkfi_sdk::crypto::Keypair::random(&mut rand::rngs::OsRng).public,
        ),
        plan_id: 1,
        lock_until_block: 100000,
        deposit: 1000,
        token_id: darkfi_sdk::pasta::pallas::Base::ONE,
        value_commit: darkfi_sdk::pasta::pallas::Point::identity(),
        state: SubscriptionState::Active,
        spent_nullifier: darkfi_sdk::pasta::pallas::Base::ZERO,
        created_at: 50000,
        dao_escrow_bulla: Some(darkfi_sdk::pasta::pallas::Base::from(2)),
        dao_membership_note: Some(darkfi_sdk::pasta::pallas::Base::from(3)),
    };

    let encoded = subscription.encode().unwrap();
    let decoded = Subscription::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.id, subscription.id);
    assert_eq!(decoded.plan_id, subscription.plan_id);
    assert_eq!(decoded.state, subscription.state);
    assert_eq!(decoded.deposit, subscription.deposit);
}

#[test]
fn test_plan_encoding() {
    let plan = Plan {
        id: 1,
        name_hash: darkfi_sdk::pasta::pallas::Base::from(1),
        price: 1000,
        token_id: darkfi_sdk::pasta::pallas::Base::ONE,
        duration_blocks: 10000,
        treasury_share: 8000,
        endowment_share: 2000,
        active: true,
        dao_escrow_discount: 2000,
        required_dao_escrow: Some(darkfi_sdk::pasta::pallas::Base::from(2)),
    };

    let encoded = plan.encode().unwrap();
    let decoded = Plan::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.id, plan.id);
    assert_eq!(decoded.price, plan.price);
    assert_eq!(decoded.treasury_share, plan.treasury_share);
    assert_eq!(decoded.active, plan.active);
}

#[test]
fn test_subscribe_params_encoding() {
    let params = SubscribeParamsV1 {
        plan_id: 1,
        subscriber_pubkey: darkfi_sdk::crypto::PublicKey::from_publickey(
            &darkfi_sdk::crypto::Keypair::random(&mut rand::rngs::OsRng).public,
        ),
        commitment: darkfi_sdk::pasta::pallas::Base::from(1),
        value_commit: darkfi_sdk::pasta::pallas::Point::identity(),
        merkle_proof: vec![darkfi_sdk::pasta::pallas::Base::from(1)],
        merkle_root: darkfi_sdk::pasta::pallas::Base::from(2),
        dao_escrow_bulla: Some(darkfi_sdk::pasta::pallas::Base::from(3)),
        dao_membership_note: Some(darkfi_sdk::pasta::pallas::Base::from(4)),
        dao_escrow_merkle_root: Some(darkfi_sdk::pasta::pallas::Base::from(5)),
        dao_merkle_proof: Some(vec![darkfi_sdk::pasta::pallas::Base::from(6)]),
        dao_leaf_pos: Some(1),
    };

    let encoded = params.encode().unwrap();
    let decoded = SubscribeParamsV1::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.plan_id, params.plan_id);
    assert_eq!(decoded.commitment, params.commitment);
}

#[test]
fn test_subscribe_update_encoding() {
    let update = SubscribeUpdateV1 {
        subscription_id: darkfi_sdk::pasta::pallas::Base::from(1),
    };

    let encoded = update.encode().unwrap();
    let decoded = SubscribeUpdateV1::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.subscription_id, update.subscription_id);
}

#[test]
fn test_cancel_params_encoding() {
    let params = CancelParamsV1 {
        subscription_id: darkfi_sdk::pasta::pallas::Base::from(1),
        subscriber_secret: darkfi_sdk::pasta::pallas::Base::from(2),
        spent_nullifier: darkfi_sdk::pasta::pallas::Base::from(3),
        current_block: 50000,
        recipient_pubkey: darkfi_sdk::crypto::PublicKey::from_publickey(
            &darkfi_sdk::crypto::Keypair::random(&mut rand::rngs::OsRng).public,
        ),
    };

    let encoded = params.encode().unwrap();
    let decoded = CancelParamsV1::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.subscription_id, params.subscription_id);
    assert_eq!(decoded.current_block, params.current_block);
}

#[test]
fn test_cancel_update_encoding() {
    let update = CancelUpdateV1 {
        subscription_id: darkfi_sdk::pasta::pallas::Base::from(1),
        spent_nullifier: darkfi_sdk::pasta::pallas::Base::from(2),
    };

    let encoded = update.encode().unwrap();
    let decoded = CancelUpdateV1::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.subscription_id, update.subscription_id);
    assert_eq!(decoded.spent_nullifier, update.spent_nullifier);
}

#[test]
fn test_renew_params_encoding() {
    let params = RenewParamsV1 {
        subscription_id: darkfi_sdk::pasta::pallas::Base::from(1),
        subscriber_secret: darkfi_sdk::pasta::pallas::Base::from(2),
        new_lock_until_block: 110000,
        spent_nullifier: darkfi_sdk::pasta::pallas::Base::from(3),
        value_commit: darkfi_sdk::pasta::pallas::Point::identity(),
        merkle_proof: vec![darkfi_sdk::pasta::pallas::Base::from(4)],
    };

    let encoded = params.encode().unwrap();
    let decoded = RenewParamsV1::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.subscription_id, params.subscription_id);
    assert_eq!(decoded.new_lock_until_block, params.new_lock_until_block);
}

#[test]
fn test_renew_update_encoding() {
    let update = RenewUpdateV1 {
        subscription_id: darkfi_sdk::pasta::pallas::Base::from(1),
        spent_nullifier: darkfi_sdk::pasta::pallas::Base::from(2),
    };

    let encoded = update.encode().unwrap();
    let decoded = RenewUpdateV1::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.subscription_id, update.subscription_id);
    assert_eq!(decoded.spent_nullifier, update.spent_nullifier);
}

#[test]
fn test_verify_access_params_encoding() {
    let params = VerifyAccessParamsV1 {
        subscription_id: darkfi_sdk::pasta::pallas::Base::from(1),
        capability: darkfi_sdk::pasta::pallas::Base::from(2),
        nonce: darkfi_sdk::pasta::pallas::Base::from(3),
    };

    let encoded = params.encode().unwrap();
    let decoded = VerifyAccessParamsV1::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.subscription_id, params.subscription_id);
    assert_eq!(decoded.capability, params.capability);
}

#[test]
fn test_dao_control_update_encoding() {
    let update = DaoControlUpdateV1 {
        action: DaoControlAction::PlanUpdated(1),
    };

    let encoded = update.encode().unwrap();
    let decoded = DaoControlUpdateV1::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

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