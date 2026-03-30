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

//! DAO-Escrow contract integration tests

use darkfi_dao_escrow_contract::{
    dao_escrow::{modes, FeeConfig, Membership},
    model::{
        DaoEscrow, DaoEscrowBulla, DaoEscrowMode, EnableDrainProtectionParamsV1,
        EnableDrainProtectionUpdateV1, InitializeParamsV1, InitializeUpdateV1,
        MembershipNote, PayPremiumParamsV1, PayPremiumUpdateV1, UpdateParamsV1,
        UpdateUpdateV1, WithdrawParamsV1, WithdrawUpdateV1,
    },
    DaoEscrowFunction,
    // Constants
    DAO_ESCROW_CONTRACT_INFO_TREE, DAO_ESCROW_CONTRACT_BULLAS_TREE,
    DAO_ESCROW_CONTRACT_MEMBERSHIP_TREE, DAO_ESCROW_CONTRACT_ENDOWMENT_TREE,
};

#[test]
fn test_dao_escrow_function_enum_valid() {
    assert!(DaoEscrowFunction::try_from(0x00).is_ok()); // InitializeV1
    assert!(DaoEscrowFunction::try_from(0x01).is_ok()); // UpdateV1
    assert!(DaoEscrowFunction::try_from(0x02).is_ok()); // PayPremiumV1
    assert!(DaoEscrowFunction::try_from(0x03).is_ok()); // WithdrawV1
    assert!(DaoEscrowFunction::try_from(0x04).is_ok()); // EndowmentWithdrawV1
    assert!(DaoEscrowFunction::try_from(0x05).is_ok()); // TreasurySpendV1
}

#[test]
fn test_dao_escrow_function_enum_invalid() {
    assert!(DaoEscrowFunction::try_from(0xFF).is_err());
    assert!(DaoEscrowFunction::try_from(0x06).is_err());
    assert!(DaoEscrowFunction::try_from(0x10).is_err());
}

#[test]
fn test_dao_escrow_mode_from_u8() {
    assert_eq!(DaoEscrowMode::try_from(0), Ok(DaoEscrowMode::Escrow));
    assert_eq!(DaoEscrowMode::try_from(1), Ok(DaoEscrowMode::Treasury));
    assert_eq!(DaoEscrowMode::try_from(2), Ok(DaoEscrowMode::TreasuryEndowment));
    assert!(DaoEscrowMode::try_from(3).is_err());
    assert!(DaoEscrowMode::try_from(255).is_err());
}

#[test]
fn test_mode_constants() {
    assert_eq!(modes::MODE_ESCROW, 0);
    assert_eq!(modes::MODE_TREASURY, 1);
    assert_eq!(modes::MODE_TREASURY_ENDOWMENT, 2);
}

#[test]
fn test_fee_config_encoding() {
    let config = FeeConfig {
        treasury_share: 7000,
        endowment_share: 3000,
    };

    let encoded = config.encode().unwrap();
    let decoded = FeeConfig::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.treasury_share, config.treasury_share);
    assert_eq!(decoded.endowment_share, config.endowment_share);
}

#[test]
fn test_dao_escrow_derive_bulla() {
    let owner_pubkey = darkfi_sdk::crypto::PublicKey::from_publickey(
        &darkfi_sdk::crypto::Keypair::random(&mut rand::rngs::OsRng).public,
    );
    let pool_token_id = darkfi_sdk::pasta::pallas::Base::ONE;
    let fee_config = Some(FeeConfig { treasury_share: 7000, endowment_share: 3000 });
    let bulla_blind = darkfi_sdk::crypto::BaseBlind::new(darkfi_sdk::pasta::pallas::Base::from(42));

    let bulla = DaoEscrow::derive_bulla(
        DaoEscrowMode::TreasuryEndowment,
        &owner_pubkey,
        pool_token_id,
        &fee_config,
        bulla_blind,
    );

    // Should be deterministic
    let bulla2 = DaoEscrow::derive_bulla(
        DaoEscrowMode::TreasuryEndowment,
        &owner_pubkey,
        pool_token_id,
        &fee_config,
        bulla_blind,
    );
    assert_eq!(bulla, bulla2);

    // Different input should produce different bulla
    let bulla_different = DaoEscrow::derive_bulla(
        DaoEscrowMode::Escrow,
        &owner_pubkey,
        pool_token_id,
        &fee_config,
        bulla_blind,
    );
    assert_ne!(bulla, bulla_different);
}

#[test]
fn test_membership_derive_note() {
    let dao_escrow_bulla = darkfi_sdk::pasta::pallas::Base::from(1);
    let member_pubkey = darkfi_sdk::crypto::PublicKey::from_publickey(
        &darkfi_sdk::crypto::Keypair::random(&mut rand::rngs::OsRng).public,
    );
    let value: u64 = 1000;
    let token_id = darkfi_sdk::pasta::pallas::Base::ONE;
    let expiry: u64 = 100000;
    let blind = darkfi_sdk::crypto::BaseBlind::new(darkfi_sdk::pasta::pallas::Base::from(42));

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
        bulla: darkfi_sdk::pasta::pallas::Base::from(1),
        mode: DaoEscrowMode::TreasuryEndowment,
        owner_pubkey: darkfi_sdk::crypto::PublicKey::from_publickey(
            &darkfi_sdk::crypto::Keypair::random(&mut rand::rngs::OsRng).public,
        ),
        pool_token_id: darkfi_sdk::pasta::pallas::Base::ONE,
        total_pool: 100000,
        total_treasury: 70000,
        total_endowment: 30000,
        member_count: 10,
        fee_config: Some(FeeConfig { treasury_share: 7000, endowment_share: 3000 }),
        min_premium: 100,
        max_members: 1000,
        created_at: 50000,
        bulla_blind: darkfi_sdk::crypto::BaseBlind::new(darkfi_sdk::pasta::pallas::Base::from(42)),
        paused: false,
        drain_protection_enabled: true,
        drain_protection_bulla: Some(darkfi_sdk::pasta::pallas::Base::from(2)),
    };

    let encoded = escrow.encode().unwrap();
    let decoded = DaoEscrow::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.bulla, escrow.bulla);
    assert_eq!(decoded.mode, escrow.mode);
    assert_eq!(decoded.total_pool, escrow.total_pool);
    assert_eq!(decoded.member_count, escrow.member_count);
    assert_eq!(decoded.paused, escrow.paused);
}

#[test]
fn test_membership_encoding() {
    let membership = Membership {
        note: darkfi_sdk::pasta::pallas::Base::from(1),
        dao_escrow_bulla: darkfi_sdk::pasta::pallas::Base::from(2),
        member_pubkey: darkfi_sdk::crypto::PublicKey::from_publickey(
            &darkfi_sdk::crypto::Keypair::random(&mut rand::rngs::OsRng).public,
        ),
        value: 1000,
        token_id: darkfi_sdk::pasta::pallas::Base::ONE,
        expiry: 100000,
        created_at: 50000,
    };

    let encoded = membership.encode().unwrap();
    let decoded = Membership::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.note, membership.note);
    assert_eq!(decoded.value, membership.value);
    assert_eq!(decoded.expiry, membership.expiry);
}

#[test]
fn test_initialize_params_encoding() {
    let params = InitializeParamsV1 {
        dao_bulla: darkfi_sdk::pasta::pallas::Base::from(1),
        owner_pubkey: darkfi_sdk::crypto::PublicKey::from_publickey(
            &darkfi_sdk::crypto::Keypair::random(&mut rand::rngs::OsRng).public,
        ),
        endowment_token_id: darkfi_sdk::pasta::pallas::Base::ONE,
        bulla_blind: darkfi_sdk::crypto::BaseBlind::new(darkfi_sdk::pasta::pallas::Base::from(42)),
        enable_drain_protection: true,
    };

    let encoded = params.encode().unwrap();
    let decoded = InitializeParamsV1::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.dao_bulla, params.dao_bulla);
    assert_eq!(decoded.enable_drain_protection, params.enable_drain_protection);
}

#[test]
fn test_initialize_update_encoding() {
    let update = InitializeUpdateV1 {
        bulla: darkfi_sdk::pasta::pallas::Base::from(1),
    };

    let encoded = update.encode().unwrap();
    let decoded = InitializeUpdateV1::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.bulla, update.bulla);
}

#[test]
fn test_update_params_encoding() {
    let params = UpdateParamsV1 {
        bulla: darkfi_sdk::pasta::pallas::Base::from(1),
    };

    let encoded = params.encode().unwrap();
    let decoded = UpdateParamsV1::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.bulla, params.bulla);
}

#[test]
fn test_update_update_encoding() {
    let update = UpdateUpdateV1 {
        bulla: darkfi_sdk::pasta::pallas::Base::from(1),
    };

    let encoded = update.encode().unwrap();
    let decoded = UpdateUpdateV1::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.bulla, update.bulla);
}

#[test]
fn test_pay_premium_params_encoding() {
    let params = PayPremiumParamsV1 {
        dao_escrow_bulla: darkfi_sdk::pasta::pallas::Base::from(1),
        membership_note: darkfi_sdk::pasta::pallas::Base::from(2),
        value_commit: darkfi_sdk::pasta::pallas::Point::identity(),
        value: 500,
        token_id: darkfi_sdk::pasta::pallas::Base::ONE,
        expiry: 100000,
        membership_blind: darkfi_sdk::crypto::BaseBlind::new(darkfi_sdk::pasta::pallas::Base::from(42)),
        value_blind: darkfi_sdk::crypto::BaseBlind::new(darkfi_sdk::pasta::pallas::Base::from(43)),
    };

    let encoded = params.encode().unwrap();
    let decoded = PayPremiumParamsV1::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.dao_escrow_bulla, params.dao_escrow_bulla);
    assert_eq!(decoded.value, params.value);
    assert_eq!(decoded.expiry, params.expiry);
}

#[test]
fn test_pay_premium_update_encoding() {
    let update = PayPremiumUpdateV1 {
        dao_escrow_bulla: darkfi_sdk::pasta::pallas::Base::from(1),
        membership_note: darkfi_sdk::pasta::pallas::Base::from(2),
        total_endowment: 10500,
        member_count: 11,
    };

    let encoded = update.encode().unwrap();
    let decoded = PayPremiumUpdateV1::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.dao_escrow_bulla, update.dao_escrow_bulla);
    assert_eq!(decoded.total_endowment, update.total_endowment);
    assert_eq!(decoded.member_count, update.member_count);
}

#[test]
fn test_withdraw_params_encoding() {
    let params = WithdrawParamsV1 {
        dao_escrow_bulla: darkfi_sdk::pasta::pallas::Base::from(1),
        value: 500,
        recipient_pubkey: darkfi_sdk::crypto::PublicKey::from_publickey(
            &darkfi_sdk::crypto::Keypair::random(&mut rand::rngs::OsRng).public,
        ),
    };

    let encoded = params.encode().unwrap();
    let decoded = WithdrawParamsV1::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.dao_escrow_bulla, params.dao_escrow_bulla);
    assert_eq!(decoded.value, params.value);
}

#[test]
fn test_withdraw_update_encoding() {
    let update = WithdrawUpdateV1 {
        dao_escrow_bulla: darkfi_sdk::pasta::pallas::Base::from(1),
        value: 500,
        total_endowment: 9500,
    };

    let encoded = update.encode().unwrap();
    let decoded = WithdrawUpdateV1::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.dao_escrow_bulla, update.dao_escrow_bulla);
    assert_eq!(decoded.value, update.value);
    assert_eq!(decoded.total_endowment, update.total_endowment);
}

#[test]
fn test_enable_drain_protection_params_encoding() {
    let params = EnableDrainProtectionParamsV1 {
        dao_escrow_bulla: darkfi_sdk::pasta::pallas::Base::from(1),
        drain_protection_bulla: darkfi_sdk::pasta::pallas::Base::from(2),
    };

    let encoded = params.encode().unwrap();
    let decoded = EnableDrainProtectionParamsV1::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.dao_escrow_bulla, params.dao_escrow_bulla);
    assert_eq!(decoded.drain_protection_bulla, params.drain_protection_bulla);
}

#[test]
fn test_enable_drain_protection_update_encoding() {
    let update = EnableDrainProtectionUpdateV1 {
        dao_escrow_bulla: darkfi_sdk::pasta::pallas::Base::from(1),
        drain_protection_bulla: darkfi_sdk::pasta::pallas::Base::from(2),
    };

    let encoded = update.encode().unwrap();
    let decoded = EnableDrainProtectionUpdateV1::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

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