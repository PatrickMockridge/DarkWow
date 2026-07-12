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

//! DAO-Escrow contract integration tests

use dwow_dao_escrow_contract::{
    modes::{MODE_ESCROW, MODE_TREASURY, MODE_TREASURY_ENDOWMENT},
    model::{
        DaoEscrow, DaoEscrowBulla, DaoEscrowMode, EnableDrainProtectionParamsV1,
        EnableDrainProtectionUpdateV1, FeeConfig, InitializeParamsV1, InitializeUpdateV1,
        Membership, MembershipNote, PayPremiumParamsV1, PayPremiumUpdateV1, UpdateParamsV1,
        UpdateUpdateV1, WithdrawParamsV1, WithdrawUpdateV1,
    },
    DaoEscrowFunction, DAO_ESCROW_CONTRACT_BULLAS_TREE, DAO_ESCROW_CONTRACT_ENDOWMENT_TREE,
    DAO_ESCROW_CONTRACT_INFO_TREE, DAO_ESCROW_CONTRACT_MEMBERSHIP_TREE,
};
use dwow_serial::{deserialize, serialize};
use dwow_sdk::{
    crypto::{pasta_prelude::Group, BaseBlind, PublicKey, ScalarBlind, SecretKey, TokenId},
    pasta::pallas,
};

/// Helper to create PublicKey from a numeric seed
fn make_pubkey(seed: u64) -> PublicKey {
    let secret = SecretKey::from(pallas::Base::from(seed));
    PublicKey::from_secret(secret)
}

/// Helper to create BaseBlind from a numeric seed
fn make_blind(seed: u64) -> BaseBlind {
    BaseBlind::from(seed)
}

#[test]
fn test_dao_escrow_function_enum_valid() {
    assert!(DaoEscrowFunction::try_from(0x00).is_ok()); // InitializeV1
    assert!(DaoEscrowFunction::try_from(0x01).is_ok()); // UpdateV1
    assert!(DaoEscrowFunction::try_from(0x02).is_ok()); // PayPremiumV1
    assert!(DaoEscrowFunction::try_from(0x03).is_ok()); // WithdrawV1
    assert!(DaoEscrowFunction::try_from(0x04).is_ok()); // EndowmentWithdrawV1
    assert!(DaoEscrowFunction::try_from(0x05).is_ok()); // TreasurySpendV1
    assert!(DaoEscrowFunction::try_from(0x06).is_ok()); // EnableDrainProtectionV1
    assert!(DaoEscrowFunction::try_from(0x07).is_ok()); // ProposeClaimV1
    assert!(DaoEscrowFunction::try_from(0x08).is_ok()); // VoteClaimV1
    assert!(DaoEscrowFunction::try_from(0x09).is_ok()); // ExecuteClaimV1
    assert!(DaoEscrowFunction::try_from(0x0a).is_ok()); // RegisterCapabilityRequirementV1
    assert!(DaoEscrowFunction::try_from(0x0b).is_ok()); // VerifyMemberCapabilityV1
    assert!(DaoEscrowFunction::try_from(0x0c).is_ok()); // ResolveDisputeV1
    assert!(DaoEscrowFunction::try_from(0x0d).is_ok()); // CancelClaimV1
    assert!(DaoEscrowFunction::try_from(0x0e).is_ok()); // SetGovernanceConfigV1
}

#[test]
fn test_dao_escrow_function_enum_invalid() {
    assert!(DaoEscrowFunction::try_from(0xFF).is_err());
    assert!(DaoEscrowFunction::try_from(0x11).is_err());
    assert!(DaoEscrowFunction::try_from(0x20).is_err());
}

#[test]
fn test_dao_escrow_mode_encoding() {
    let escrow = DaoEscrowMode::Escrow;
    let treasury = DaoEscrowMode::Treasury;
    let endowment = DaoEscrowMode::TreasuryEndowment;

    let encoded_escrow = serialize(&escrow);
    let decoded_escrow: DaoEscrowMode = deserialize(&encoded_escrow).unwrap();
    assert_eq!(decoded_escrow, DaoEscrowMode::Escrow);

    let encoded_treasury = serialize(&treasury);
    let decoded_treasury: DaoEscrowMode = deserialize(&encoded_treasury).unwrap();
    assert_eq!(decoded_treasury, DaoEscrowMode::Treasury);

    let encoded_endowment = serialize(&endowment);
    let decoded_endowment: DaoEscrowMode = deserialize(&encoded_endowment).unwrap();
    assert_eq!(decoded_endowment, DaoEscrowMode::TreasuryEndowment);
}

#[test]
fn test_mode_constants() {
    assert_eq!(MODE_ESCROW, 0);
    assert_eq!(MODE_TREASURY, 1);
    assert_eq!(MODE_TREASURY_ENDOWMENT, 2);
}

#[test]
fn test_fee_config_encoding() {
    let config = FeeConfig { version: 0, treasury_share: 7000, endowment_share: 3000 };

    let encoded = serialize(&config);
    let decoded: FeeConfig = deserialize(&encoded).unwrap();

    assert_eq!(decoded.treasury_share, config.treasury_share);
    assert_eq!(decoded.endowment_share, config.endowment_share);
}

#[test]
fn test_dao_escrow_derive_bulla() {
    let dao_bulla = DaoEscrowBulla(pallas::Base::from(42u64));
    let owner_pubkey = make_pubkey(1);
    let pool_token_id = TokenId(pallas::Base::one());
    let bulla_blind = make_blind(42);

    let bulla = DaoEscrow::derive_bulla(
        dao_bulla,
        &owner_pubkey,
        pool_token_id,
        bulla_blind,
    );

    // Should be deterministic
    let bulla2 = DaoEscrow::derive_bulla(
        dao_bulla,
        &owner_pubkey,
        pool_token_id,
        bulla_blind,
    );
    assert_eq!(bulla, bulla2);

    // Different dao_bulla should produce different bulla
    let different_dao = DaoEscrowBulla(pallas::Base::from(99u64));
    let bulla_different = DaoEscrow::derive_bulla(
        different_dao,
        &owner_pubkey,
        pool_token_id,
        bulla_blind,
    );
    assert_ne!(bulla, bulla_different);
}

#[test]
fn test_membership_derive_note() {
    let dao_escrow_bulla = DaoEscrowBulla(pallas::Base::from(1));
    let member_pubkey = make_pubkey(1);
    let value: u64 = 1000;
    let token_id = pallas::Base::one();
    let expiry: u64 = 100000;
    let blind = make_blind(42);

    let note = Membership::derive_note(
        dao_escrow_bulla,
        &member_pubkey,
        value,
        token_id,
        expiry,
        blind,
    );

    // Should be deterministic
    let note2 = Membership::derive_note(
        dao_escrow_bulla,
        &member_pubkey,
        value,
        token_id,
        expiry,
        blind,
    );
    assert_eq!(note, note2);

    // Different input should produce different note
    let note_different = Membership::derive_note(
        dao_escrow_bulla,
        &member_pubkey,
        value + 1,
        token_id,
        expiry,
        blind,
    );
    assert_ne!(note, note_different);
}

#[test]
fn test_dao_escrow_encoding() {
    let escrow = DaoEscrow {
        version: 0,
        instance_seed: [0u8; 32],
        bulla: DaoEscrowBulla(pallas::Base::from(1)),
        mode: DaoEscrowMode::TreasuryEndowment,
        owner_pubkey: make_pubkey(1),
        pool_token_id: TokenId(pallas::Base::one()),
        multisig_group_id: pallas::Base::zero(),
        pool_purse_id: pallas::Base::from(100000),
        treasury_purse_id: pallas::Base::from(70000),
        endowment_purse_id: pallas::Base::from(30000),
        member_count: 10,
        fee_config: Some(FeeConfig { version: 0, treasury_share: 7000, endowment_share: 3000 }),
        min_premium: 100,
        max_members: 1000,
        created_at: 50000,
        bulla_blind: make_blind(42),
        paused: false,
        drain_protection_enabled: true,
        drain_protection_bulla: Some(DaoEscrowBulla(pallas::Base::from(2))),
    };

    let encoded = serialize(&escrow);
    let decoded: DaoEscrow = deserialize(&encoded).unwrap();

    assert_eq!(decoded.bulla, escrow.bulla);
    assert_eq!(decoded.mode, escrow.mode);
    assert_eq!(decoded.member_count, escrow.member_count);
    assert_eq!(decoded.paused, escrow.paused);
}

#[test]
fn test_membership_encoding() {
    let membership = Membership {
        version: 0,
        note: MembershipNote(pallas::Base::from(1)),
        dao_escrow_bulla: DaoEscrowBulla(pallas::Base::from(2)),
        member_pubkey: make_pubkey(1),
        value: 1000,
        token_id: TokenId(pallas::Base::one()),
        expiry: 100000,
        created_at: 50000,
    };

    let encoded = serialize(&membership);
    let decoded: Membership = deserialize(&encoded).unwrap();

    assert_eq!(decoded.note, membership.note);
    assert_eq!(decoded.value, membership.value);
    assert_eq!(decoded.expiry, membership.expiry);
}

#[test]
fn test_initialize_params_encoding() {
    let params = InitializeParamsV1 {
        instance_seed: [0u8; 32],
        dao_bulla: DaoEscrowBulla(pallas::Base::from(1)),
        owner_pubkey: make_pubkey(1),
        endowment_token_id: TokenId(pallas::Base::one()),
        bulla_blind: make_blind(42),
        enable_drain_protection: true,
    };

    let encoded = serialize(&params);
    let decoded: InitializeParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.dao_bulla, params.dao_bulla);
    assert_eq!(decoded.enable_drain_protection, params.enable_drain_protection);
}

#[test]
fn test_initialize_update_encoding() {
    let update = InitializeUpdateV1 {
        instance_seed: [0u8; 32],
        bulla: DaoEscrowBulla(pallas::Base::from(1)),
        owner_pubkey: make_pubkey(1),
        bulla_blind: make_blind(42),
    };

    let encoded = serialize(&update);
    let decoded: InitializeUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.bulla, update.bulla);
    assert_eq!(decoded.owner_pubkey, update.owner_pubkey);
    assert_eq!(decoded.bulla_blind, update.bulla_blind);
}

#[test]
fn test_update_params_encoding() {
    let params = UpdateParamsV1 { bulla: DaoEscrowBulla(pallas::Base::from(1)) };

    let encoded = serialize(&params);
    let decoded: UpdateParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.bulla, params.bulla);
}

#[test]
fn test_update_update_encoding() {
    let update = UpdateUpdateV1 { bulla: DaoEscrowBulla(pallas::Base::from(1)) };

    let encoded = serialize(&update);
    let decoded: UpdateUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.bulla, update.bulla);
}

#[test]
fn test_pay_premium_params_encoding() {
    let params = PayPremiumParamsV1 {
        dao_escrow_bulla: DaoEscrowBulla(pallas::Base::from(1)),
        membership_note: MembershipNote(pallas::Base::from(2)),
        value_commit: Group::identity(),
        value: 500,
        token_id: TokenId(pallas::Base::one()),
        expiry: 100000,
        membership_blind: make_blind(42),
        value_blind: ScalarBlind::from(43u64),
        member_pubkey: make_pubkey(1),
    };

    let encoded = serialize(&params);
    let decoded: PayPremiumParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.dao_escrow_bulla, params.dao_escrow_bulla);
    assert_eq!(decoded.value, params.value);
    assert_eq!(decoded.expiry, params.expiry);
    assert_eq!(decoded.member_pubkey, params.member_pubkey);
}

#[test]
fn test_pay_premium_update_encoding() {
    let update = PayPremiumUpdateV1 {
        dao_escrow_bulla: DaoEscrowBulla(pallas::Base::from(1)),
        membership_note: MembershipNote(pallas::Base::from(2)),
        amount: 10500,
        member_count: 11,
        member_pubkey: make_pubkey(1),
        token_id: TokenId(pallas::Base::one()),
        expiry: 100000,
    };

    let encoded = serialize(&update);
    let decoded: PayPremiumUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.dao_escrow_bulla, update.dao_escrow_bulla);
    assert_eq!(decoded.amount, update.amount);
    assert_eq!(decoded.member_count, update.member_count);
    assert_eq!(decoded.member_pubkey, update.member_pubkey);
    assert_eq!(decoded.token_id, update.token_id);
    assert_eq!(decoded.expiry, update.expiry);
}

#[test]
fn test_withdraw_params_encoding() {
    let params = WithdrawParamsV1 {
        dao_escrow_bulla: DaoEscrowBulla(pallas::Base::from(1)),
        value: 500,
        recipient_pubkey: make_pubkey(1),
        capability_proof: None,
    };

    let encoded = serialize(&params);
    let decoded: WithdrawParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.dao_escrow_bulla, params.dao_escrow_bulla);
    assert_eq!(decoded.value, params.value);
}

#[test]
fn test_withdraw_update_encoding() {
    let update = WithdrawUpdateV1 {
        dao_escrow_bulla: DaoEscrowBulla(pallas::Base::from(1)),
        value: 500,
        amount: 9500,
    };

    let encoded = serialize(&update);
    let decoded: WithdrawUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.dao_escrow_bulla, update.dao_escrow_bulla);
    assert_eq!(decoded.value, update.value);
    assert_eq!(decoded.amount, update.amount);
}

#[test]
fn test_enable_drain_protection_params_encoding() {
    let params = EnableDrainProtectionParamsV1 {
        dao_escrow_bulla: DaoEscrowBulla(pallas::Base::from(1)),
        drain_protection_bulla: DaoEscrowBulla(pallas::Base::from(2)),
    };

    let encoded = serialize(&params);
    let decoded: EnableDrainProtectionParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.dao_escrow_bulla, params.dao_escrow_bulla);
    assert_eq!(decoded.drain_protection_bulla, params.drain_protection_bulla);
}

#[test]
fn test_enable_drain_protection_update_encoding() {
    let update = EnableDrainProtectionUpdateV1 {
        dao_escrow_bulla: DaoEscrowBulla(pallas::Base::from(1)),
        drain_protection_bulla: DaoEscrowBulla(pallas::Base::from(2)),
    };

    let encoded = serialize(&update);
    let decoded: EnableDrainProtectionUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.dao_escrow_bulla, update.dao_escrow_bulla);
    assert_eq!(decoded.drain_protection_bulla, update.drain_protection_bulla);
}

#[test]
fn test_constants() {
    assert_eq!(DAO_ESCROW_CONTRACT_INFO_TREE, "info");
    assert_eq!(DAO_ESCROW_CONTRACT_BULLAS_TREE, "bullas");
    assert_eq!(DAO_ESCROW_CONTRACT_MEMBERSHIP_TREE, "membership");
    assert_eq!(DAO_ESCROW_CONTRACT_ENDOWMENT_TREE, "endowment");
}