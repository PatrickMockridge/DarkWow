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

//! NativeToken Contract Unit Tests
//!
//! Tests for NativeTokenFunction enum and model structures.

#[cfg(test)]
mod tests {
    use dwow_native_token_contract::{
        model::{
            BurnParamsV1, BurnUpdateV1, ClearInput, Coin, CoinAttributes, DRKW_TOKEN_ID,
            FeeParamsV1, FeeUpdateV1, GenesisMintParamsV1, Input, Nullifier, Output,
            PoWRewardParamsV1, PoWRewardUpdateV1, SpendParamsV1, SpendUpdateV1,
            TransferParamsV1, TransferUpdateV1,
            MAX_COIN_VALUE,
        },
        NativeTokenFunction,
    };
    use dwow_sdk::{crypto::BaseBlind, crypto::Blind, crypto::FuncId, crypto::Keypair, crypto::MerkleNode, crypto::TokenId, pasta::pallas};
    use pasta_curves::group::Group;

    // ================================================================
    // NativeTokenFunction enum tests
    // ================================================================

    #[test]
    fn test_native_token_function_enum_valid() {
        assert!(NativeTokenFunction::try_from(0x00).is_ok()); // FeeV1
        assert!(NativeTokenFunction::try_from(0x01).is_ok()); // MintV1
        assert!(NativeTokenFunction::try_from(0x02).is_ok()); // BurnV1
        assert!(NativeTokenFunction::try_from(0x03).is_ok()); // TransferV1
        assert!(NativeTokenFunction::try_from(0x04).is_ok()); // SpendV1
        assert!(NativeTokenFunction::try_from(0x05).is_ok()); // PoWRewardV1
    }

    #[test]
    fn test_native_token_function_enum_invalid() {
        assert!(NativeTokenFunction::try_from(0xFF).is_err()); // Invalid
        assert!(NativeTokenFunction::try_from(0x06).is_err()); // Out of range
        assert!(NativeTokenFunction::try_from(0x10).is_err()); // Out of range
    }

    #[test]
    fn test_native_token_function_names() {
        assert_eq!(NativeTokenFunction::FeeV1 as u8, 0x00);
        assert_eq!(NativeTokenFunction::MintV1 as u8, 0x01);
        assert_eq!(NativeTokenFunction::BurnV1 as u8, 0x02);
        assert_eq!(NativeTokenFunction::TransferV1 as u8, 0x03);
        assert_eq!(NativeTokenFunction::SpendV1 as u8, 0x04);
        assert_eq!(NativeTokenFunction::PoWRewardV1 as u8, 0x05);
    }

    // ================================================================
    // Token constant tests
    // ================================================================

    #[test]
    fn test_dark_token_id_is_zero() {
        assert_eq!(DRKW_TOKEN_ID, TokenId::DRKW);
    }

    #[test]
    fn test_max_coin_value() {
        assert_eq!(MAX_COIN_VALUE, 1_000_000_000_000u64);
    }

    // ================================================================
    // Coin tests
    // ================================================================

    #[test]
    fn test_coin_from_attributes() {
        let keypair = Keypair::random(&mut rand::rngs::OsRng);
        let public = keypair.public;

        // Create coin from attributes
        let value = 0u64;
        let token_id = DRKW_TOKEN_ID.inner();
        let spend_hook = FuncId::none();
        let user_data = pallas::Base::zero();
        let blind = Blind(pallas::Base::zero());

        let coin = Coin::from_attributes(&public, value, token_id, spend_hook, user_data, blind);

        // Verify coin was created (public key coords ensure non-zero hash)
        let inner = coin.inner();
        assert!(inner != pallas::Base::zero());
    }

    #[test]
    fn test_coin_from_attributes_nonzero() {
        let keypair = Keypair::random(&mut rand::rngs::OsRng);
        let public = keypair.public;

        // Create coin with non-zero value
        let value = 1000u64;
        let token_id = DRKW_TOKEN_ID.inner();
        let spend_hook = FuncId::none();
        let user_data = pallas::Base::zero();
        let blind = Blind(pallas::Base::zero());

        let coin = Coin::from_attributes(&public, value, token_id, spend_hook, user_data, blind);

        // Verify coin is created (hash should be non-zero with non-zero value)
        assert!(coin.inner() != pallas::Base::zero());
    }

    #[test]
    fn test_coin_to_bytes() {
        let keypair = Keypair::random(&mut rand::rngs::OsRng);
        let public = keypair.public;

        let coin = Coin::from_attributes(
            &public,
            0,
            DRKW_TOKEN_ID.inner(),
            FuncId::none(),
            pallas::Base::zero(),
            Blind(pallas::Base::zero()),
        );

        let bytes = coin.to_bytes();
        assert_eq!(bytes.len(), 32);
    }

    #[test]
    fn test_coin_inner() {
        let keypair = Keypair::random(&mut rand::rngs::OsRng);
        let public = keypair.public;

        let coin = Coin::from_attributes(
            &public,
            0,
            DRKW_TOKEN_ID.inner(),
            FuncId::none(),
            pallas::Base::zero(),
            Blind(pallas::Base::zero()),
        );

        // Verify inner value is accessible
        let inner = coin.inner();
        assert!(inner != pallas::Base::zero());
    }

    // ================================================================
    // CoinAttributes tests
    // ================================================================

    #[test]
    fn test_coin_attributes_to_coin() {
        let keypair = Keypair::random(&mut rand::rngs::OsRng);

        let attributes = CoinAttributes {
            version: 0,
            public_key: keypair.public,
            value: 0,
            token_id: DRKW_TOKEN_ID.inner(),
            spend_hook: FuncId::none(),
            user_data: pallas::Base::zero(),
            blind: Blind(pallas::Base::zero()),
        };

        let coin = attributes.to_coin();
        // Public key ensures non-zero coin
        assert!(coin.inner() != pallas::Base::zero());
    }

    #[test]
    fn test_coin_attributes_to_coin_nonzero() {
        let keypair = Keypair::random(&mut rand::rngs::OsRng);

        let attributes = CoinAttributes {
            version: 0,
            public_key: keypair.public,
            value: 500,
            token_id: DRKW_TOKEN_ID.inner(),
            spend_hook: FuncId::none(),
            user_data: pallas::Base::zero(),
            blind: Blind(pallas::Base::zero()),
        };

        let coin = attributes.to_coin();
        assert!(coin.inner() != pallas::Base::zero());
    }

    // ================================================================
    // Nullifier tests
    // ================================================================

    #[test]
    fn test_nullifier_from_bytes_zero() {
        let bytes = [0u8; 32];
        let nullifier =
            dwow_native_token_contract::model::Nullifier::from_bytes(bytes).unwrap();
        assert_eq!(nullifier.inner(), pallas::Base::zero());
    }

    #[test]
    fn test_nullifier_from_bytes_nonzero() {
        // Use a valid canonical representation - 1 as a base field element
        let mut bytes = [0u8; 32];
        bytes[0] = 0x01;
        let nullifier =
            dwow_native_token_contract::model::Nullifier::from_bytes(bytes).unwrap();
        assert!(nullifier.inner() != pallas::Base::zero());
    }

    #[test]
    fn test_nullifier_to_bytes() {
        let nullifier =
            dwow_native_token_contract::model::Nullifier::from_bytes([0u8; 32]).unwrap();
        let bytes = nullifier.to_bytes();
        assert_eq!(bytes.len(), 32);
        assert_eq!(bytes, [0u8; 32]);
    }

    #[test]
    fn test_nullifier_roundtrip() {
        // Use a valid canonical byte representation
        let mut original_bytes = [0u8; 32];
        original_bytes[0] = 0x01; // Smallest valid non-zero canonical byte
        let nullifier =
            dwow_native_token_contract::model::Nullifier::from_bytes(original_bytes).unwrap();
        let output_bytes = nullifier.to_bytes();
        assert_eq!(original_bytes, output_bytes);
    }

    // ================================================================
    // MerkleNode tests
    // ================================================================

    #[test]
    fn test_merkle_node_new() {
        let node = MerkleNode::new(pallas::Base::zero());
        assert_eq!(node.inner(), pallas::Base::zero());
    }

    #[test]
    fn test_merkle_node_from_bytes() {
        let bytes = [0u8; 32];
        let node = MerkleNode::from_bytes(bytes).unwrap();
        assert_eq!(node.inner(), pallas::Base::zero());
    }

    // ================================================================
    // ClearInput tests
    // ================================================================

    #[test]
    fn test_clear_input_structure() {
        let keypair = Keypair::random(&mut rand::rngs::OsRng);

        let clear_input = ClearInput {
            value: 1000,
            token_id: DRKW_TOKEN_ID.inner(),
            value_blind: Blind(pallas::Scalar::zero()),
            token_blind: pallas::Base::zero(),
            signature_public: keypair.public,
        };

        assert_eq!(clear_input.value, 1000);
        assert_eq!(clear_input.token_id, DRKW_TOKEN_ID.inner());
    }

    #[test]
    fn test_clear_input_token_id() {
        let keypair = Keypair::random(&mut rand::rngs::OsRng);

        let clear_input = ClearInput {
            value: 0,
            token_id: pallas::Base::from(123),
            value_blind: Blind(pallas::Scalar::zero()),
            token_blind: pallas::Base::zero(),
            signature_public: keypair.public,
        };

        assert_eq!(clear_input.token_id, pallas::Base::from(123));
    }

    // ================================================================
    // Input/Output helper functions
    // ================================================================

    fn create_test_input() -> Input {
        let keypair = Keypair::random(&mut rand::rngs::OsRng);

        Input {
            value_commit: pallas::Point::identity(),
            token_commit: pallas::Base::zero(),
            nullifier:
                dwow_native_token_contract::model::Nullifier::from_bytes([0u8; 32]).unwrap(),
            merkle_root: MerkleNode::new(pallas::Base::zero()),
            user_data_enc: pallas::Base::zero(),
            spend_hook: pallas::Base::zero(),
            signature_public: keypair.public,
        }
    }

    fn create_test_output() -> Output {
        let keypair = Keypair::random(&mut rand::rngs::OsRng);

        let coin = Coin::from_attributes(
            &keypair.public,
            1000,
            DRKW_TOKEN_ID.inner(),
            FuncId::none(),
            pallas::Base::zero(),
            Blind(pallas::Base::zero()),
        );

        Output {
            value_commit: pallas::Point::identity(),
            token_commit: pallas::Base::zero(),
            coin,
            nullifier: Nullifier::from_bytes([1u8; 32]).unwrap(),
            note: dwow_sdk::crypto::note::AeadEncryptedNote {
                ciphertext: vec![0u8; 32],
                ephem_public: keypair.public,
            },
        }
    }

    // ================================================================
    // FeeParamsV1 tests
    // ================================================================

    #[test]
    fn test_fee_params_v1_structure() {
        let params = FeeParamsV1 {
            input: create_test_input(),
            output: create_test_output(),
            fee_value_blind: pallas::Scalar::zero(),
            fee_token_blind: pallas::Base::zero(),
            fee: 0,
            tx_binding: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        };

        assert!(params.input.value_commit == pallas::Point::identity());
        assert!(params.output.coin.inner() != pallas::Base::zero());
    }

    // ================================================================
    // FeeUpdateV1 tests
    // ================================================================

    #[test]
    fn test_fee_update_v1_structure() {
        let keypair = Keypair::random(&mut rand::rngs::OsRng);
        let coin = Coin::from_attributes(
            &keypair.public,
            1000,
            DRKW_TOKEN_ID.inner(),
            FuncId::none(),
            pallas::Base::zero(),
            Blind(pallas::Base::zero()),
        );

        let update = FeeUpdateV1 {
            nullifier:
                dwow_native_token_contract::model::Nullifier::from_bytes([0u8; 32]).unwrap(),
            coin,
            height: 100,
            fee: 5,
        };

        assert_eq!(update.height, 100);
        assert_eq!(update.fee, 5);
    }

    // ================================================================
    // GenesisMintParamsV1 tests
    // ================================================================

    #[test]
    fn test_genesis_mint_params_v1_empty_outputs() {
        let keypair = Keypair::random(&mut rand::rngs::OsRng);

        let params = GenesisMintParamsV1 {
            input: ClearInput {
                value: 1000,
                token_id: DRKW_TOKEN_ID.inner(),
                value_blind: Blind(pallas::Scalar::zero()),
                token_blind: pallas::Base::zero(),
                signature_public: keypair.public,
            },
            outputs: vec![],
        };

        assert_eq!(params.outputs.len(), 0);
    }

    #[test]
    fn test_genesis_mint_params_v1_with_outputs() {
        let keypair = Keypair::random(&mut rand::rngs::OsRng);

        let params = GenesisMintParamsV1 {
            input: ClearInput {
                value: 2000,
                token_id: DRKW_TOKEN_ID.inner(),
                value_blind: Blind(pallas::Scalar::zero()),
                token_blind: pallas::Base::zero(),
                signature_public: keypair.public,
            },
            outputs: vec![create_test_output()],
        };

        assert_eq!(params.outputs.len(), 1);
    }

    // ================================================================
    // PoWRewardParamsV1 tests
    // ================================================================

    #[test]
    fn test_pow_reward_params_v1_structure() {
        let keypair = Keypair::random(&mut rand::rngs::OsRng);

        let params = PoWRewardParamsV1 {
            input: ClearInput {
                value: 1000,
                token_id: DRKW_TOKEN_ID.inner(),
                value_blind: Blind(pallas::Scalar::zero()),
                token_blind: pallas::Base::zero(),
                signature_public: keypair.public,
            },
            output: create_test_output(),
            nullifier: Nullifier::from_bytes([2u8; 32]).unwrap(),
            expected_cumulative_supply: 0,
            old_cumulative_commit: pallas::Point::identity(),
            old_cumulative_blind: pallas::Scalar::zero(),
            new_cumulative_commit: pallas::Point::identity(),
            tx_binding: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        };

        assert_eq!(params.input.value, 1000);
    }

    // ================================================================
    // TransferParamsV1 tests
    // ================================================================

    #[test]
    fn test_transfer_params_v1_empty() {
        let params = TransferParamsV1 {
            inputs: vec![],
            outputs: vec![],
            tx_binding: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        };

        assert_eq!(params.inputs.len(), 0);
        assert_eq!(params.outputs.len(), 0);
    }

    #[test]
    fn test_transfer_params_v1_with_io() {
        let params = TransferParamsV1 {
            inputs: vec![create_test_input()],
            outputs: vec![create_test_output()],
            tx_binding: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        };

        assert_eq!(params.inputs.len(), 1);
        assert_eq!(params.outputs.len(), 1);
    }

    // ================================================================
    // SpendParamsV1 tests
    // ================================================================

    #[test]
    fn test_spend_params_v1_structure() {
        let params = SpendParamsV1 {
            input: create_test_input(),
            output: create_test_output(),
            tx_binding: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        };

        assert!(params.input.merkle_root.inner() == pallas::Base::zero());
    }

    // ================================================================
    // BurnParamsV1 tests
    // ================================================================

    #[test]
    fn test_burn_params_v1_empty() {
        let params = BurnParamsV1 {
            inputs: vec![],
            tx_binding: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero()
        };

        assert_eq!(params.inputs.len(), 0);
    }

    #[test]
    fn test_burn_params_v1_with_inputs() {
        let params = BurnParamsV1 {
            inputs: vec![create_test_input()],
            tx_binding: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero()
        };

        assert_eq!(params.inputs.len(), 1);
    }

    // ================================================================
    // Update structures tests
    // ================================================================

    #[test]
    fn test_genesis_mint_update_v1_structure() {
        let keypair = Keypair::random(&mut rand::rngs::OsRng);
        let coin = Coin::from_attributes(
            &keypair.public,
            1000,
            DRKW_TOKEN_ID.inner(),
            FuncId::none(),
            pallas::Base::zero(),
            Blind(pallas::Base::zero()),
        );

        let update = dwow_native_token_contract::model::GenesisMintUpdateV1 { coins: vec![coin] };

        assert_eq!(update.coins.len(), 1);
    }

    #[test]
    fn test_pow_reward_update_v1_structure() {
        let keypair = Keypair::random(&mut rand::rngs::OsRng);
        let coin = Coin::from_attributes(
            &keypair.public,
            1000,
            DRKW_TOKEN_ID.inner(),
            FuncId::none(),
            pallas::Base::zero(),
            Blind(pallas::Base::zero()),
        );

        let update = PoWRewardUpdateV1 {
            coin,
            height: 100,
            new_total_supply: 0,
            cumulative_value_commit: pallas::Point::identity(),
            aggregate_blind: pallas::Scalar::zero(),
        };

        assert_eq!(update.height, 100);
    }

    #[test]
    fn test_transfer_update_v1_structure() {
        let keypair = Keypair::random(&mut rand::rngs::OsRng);
        let coin = Coin::from_attributes(
            &keypair.public,
            1000,
            DRKW_TOKEN_ID.inner(),
            FuncId::none(),
            pallas::Base::zero(),
            Blind(pallas::Base::zero()),
        );
        let nullifier =
            dwow_native_token_contract::model::Nullifier::from_bytes([0u8; 32]).unwrap();

        let update = TransferUpdateV1 { nullifiers: vec![nullifier], coins: vec![coin] };

        assert_eq!(update.nullifiers.len(), 1);
        assert_eq!(update.coins.len(), 1);
    }

    #[test]
    fn test_spend_update_v1_structure() {
        let keypair = Keypair::random(&mut rand::rngs::OsRng);
        let coin = Coin::from_attributes(
            &keypair.public,
            1000,
            DRKW_TOKEN_ID.inner(),
            FuncId::none(),
            pallas::Base::zero(),
            Blind(pallas::Base::zero()),
        );
        let nullifier =
            dwow_native_token_contract::model::Nullifier::from_bytes([0u8; 32]).unwrap();

        let update = SpendUpdateV1 { nullifier, coin };

        assert!(update.coin.inner() != pallas::Base::zero());
    }

    #[test]
    fn test_burn_update_v1_structure() {
        let nullifier =
            dwow_native_token_contract::model::Nullifier::from_bytes([0u8; 32]).unwrap();

        let update = BurnUpdateV1 { nullifiers: vec![nullifier] };

        assert_eq!(update.nullifiers.len(), 1);
    }
}
