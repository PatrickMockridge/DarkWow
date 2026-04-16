/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by the
 * Free Software Foundation; either version 3 of the License, or at your
 * option) any later version.
 *
 * This program is distributed in the hope that it will be useful, but WITHOUT
 * ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
 * FOR A PARTICULAR PURPOSE. See the GNU General Public License for more
 * details.
 *
 * You should have received a copy of the GNU General Public License along with
 * this program; if not, see <https://www.gnu.org/licenses/>.
 */

//! MoneyV3 Contract Integration Tests
//!
//! Tests for MoneyV3Function enum and model structures.

#[cfg(test)]
mod tests {
    use darkfi_money_v3_contract::{
        model::{
            AuthProof, AuthTokenMintParamsV1, AuthTokenMintUpdateV1, BurnParamsV1, BurnUpdateV1,
            Coin, CoinAttributes, Input, MintParamsV1, MintUpdateV1, Nullifier, Output,
            OtcSwapParamsV1, OtcSwapUpdateV1, TokenMintParamsV1, TokenMintUpdateV1,
            TransferParamsV1, TransferUpdateV1, MAX_COIN_VALUE,
        },
        MoneyV3Function, MONEY_V3_MAX_COIN_VALUE,
    };
    use darkfi_sdk::{
        crypto::{pasta_prelude::PrimeField, poseidon_hash, MerkleNode},
        pasta::pallas,
    };
    use darkfi_serial::{deserialize, serialize};

    // ================================================================
    // MoneyV3Function enum tests
    // ================================================================

    #[test]
    fn test_money_v3_function_enum_valid() {
        assert!(MoneyV3Function::try_from(0x00).is_ok()); // TokenMintV1
        assert!(MoneyV3Function::try_from(0x01).is_ok()); // AuthTokenMintV1
        assert!(MoneyV3Function::try_from(0x02).is_ok()); // MintV1
        assert!(MoneyV3Function::try_from(0x03).is_ok()); // BurnV1
        assert!(MoneyV3Function::try_from(0x04).is_ok()); // TransferV1
        assert!(MoneyV3Function::try_from(0x05).is_ok()); // OtcSwapV1
    }

    #[test]
    fn test_money_v3_function_enum_invalid() {
        assert!(MoneyV3Function::try_from(0xFF).is_err()); // Invalid
        assert!(MoneyV3Function::try_from(0x06).is_err()); // Out of range
        assert!(MoneyV3Function::try_from(0x10).is_err()); // Out of range
    }

    #[test]
    fn test_money_v3_function_names() {
        assert_eq!(MoneyV3Function::TokenMintV1 as u8, 0x00);
        assert_eq!(MoneyV3Function::AuthTokenMintV1 as u8, 0x01);
        assert_eq!(MoneyV3Function::MintV1 as u8, 0x02);
        assert_eq!(MoneyV3Function::BurnV1 as u8, 0x03);
        assert_eq!(MoneyV3Function::TransferV1 as u8, 0x04);
        assert_eq!(MoneyV3Function::OtcSwapV1 as u8, 0x05);
    }

    // ================================================================
    // Constants tests
    // ================================================================

    #[test]
    fn test_max_coin_value() {
        assert_eq!(MAX_COIN_VALUE, 1_000_000_000_000u64);
        assert_eq!(MONEY_V3_MAX_COIN_VALUE, 1_000_000_000_000u64);
    }

    // ================================================================
    // Nullifier tests
    // ================================================================

    #[test]
    fn test_nullifier_new() {
        let secret = pallas::Base::from(123);
        let coin = pallas::Base::from(456);
        let nullifier = Nullifier::new(secret, coin);
        assert_eq!(nullifier.inner(), poseidon_hash([secret, coin]));
    }

    #[test]
    fn test_nullifier_new_for_auth() {
        let secret = pallas::Base::from(123);
        let token_id = pallas::Base::from(456);
        let nullifier = Nullifier::new_for_auth(secret, token_id);
        assert_eq!(nullifier.inner(), poseidon_hash([secret, token_id]));
    }

    #[test]
    fn test_nullifier_from_base() {
        let base = pallas::Base::from(789);
        let nullifier = Nullifier::from_base(base);
        assert_eq!(nullifier.inner(), base);
    }

    #[test]
    fn test_nullifier_to_bytes() {
        let secret = pallas::Base::from(123);
        let coin = pallas::Base::from(456);
        let nullifier = Nullifier::new(secret, coin);
        let bytes = nullifier.to_bytes();
        assert_eq!(bytes.len(), 32);
    }

    #[test]
    fn test_nullifier_inner() {
        let base = pallas::Base::from(999);
        let nullifier = Nullifier::from_base(base);
        assert_eq!(nullifier.inner(), base);
    }

    // ================================================================
    // Coin tests
    // ================================================================

    #[test]
    fn test_coin_from_attributes() {
        let public_key = pallas::Base::from(1);
        let value = 1000u64;
        let token_id = pallas::Base::from(2);
        let spend_hook = pallas::Base::zero();
        let user_data = pallas::Base::zero();
        let blind = pallas::Base::from(3);

        let coin = Coin::from_attributes(public_key, value, token_id, spend_hook, user_data, blind);

        let expected = poseidon_hash([
            public_key,
            pallas::Base::from(value),
            token_id,
            spend_hook,
            user_data,
            blind,
        ]);
        assert_eq!(coin.inner(), expected);
    }

    #[test]
    fn test_coin_inner() {
        let base = pallas::Base::from(42);
        let coin = Coin(base);
        assert_eq!(coin.inner(), base);
    }

    #[test]
    fn test_coin_to_bytes() {
        let public_key = pallas::Base::from(1);
        let coin = Coin::from_attributes(
            public_key,
            1000,
            pallas::Base::from(2),
            pallas::Base::zero(),
            pallas::Base::zero(),
            pallas::Base::zero(),
        );
        let bytes = coin.to_bytes();
        assert_eq!(bytes.len(), 32);
    }

    // ================================================================
    // CoinAttributes tests
    // ================================================================

    #[test]
    fn test_coin_attributes_to_coin() {
        let attrs = CoinAttributes {
            public_key: pallas::Base::from(1),
            value: 500,
            token_id: pallas::Base::from(2),
            spend_hook: pallas::Base::zero(),
            user_data: pallas::Base::zero(),
            blind: pallas::Base::from(3),
        };

        let coin = attrs.to_coin();
        let expected = Coin::from_attributes(
            attrs.public_key,
            attrs.value,
            attrs.token_id,
            attrs.spend_hook,
            attrs.user_data,
            attrs.blind,
        );
        assert_eq!(coin.inner(), expected.inner());
    }

    // ================================================================
    // Serialization tests
    // ================================================================

    #[test]
    fn test_nullifier_serialization() {
        let nullifier = Nullifier::new(pallas::Base::from(123), pallas::Base::from(456));
        let encoded = serialize(&nullifier);
        let decoded: Nullifier = deserialize(&encoded).unwrap();
        assert_eq!(decoded.inner(), nullifier.inner());
    }

    #[test]
    fn test_coin_serialization() {
        let coin = Coin::from_attributes(
            pallas::Base::from(1),
            1000,
            pallas::Base::from(2),
            pallas::Base::zero(),
            pallas::Base::zero(),
            pallas::Base::zero(),
        );
        let encoded = serialize(&coin);
        let decoded: Coin = deserialize(&encoded).unwrap();
        assert_eq!(decoded.inner(), coin.inner());
    }

    #[test]
    fn test_token_mint_params_serialization() {
        let params = TokenMintParamsV1 {
            coin: Coin::from_attributes(
                pallas::Base::from(1),
                1000,
                pallas::Base::from(2),
                pallas::Base::zero(),
                pallas::Base::zero(),
                pallas::Base::zero(),
            ),
            value_commit: pallas::Base::from(3),
            token_id: pallas::Base::from(4),
            token_commit: pallas::Base::from(5),
        };
        let encoded = serialize(&params);
        let decoded: TokenMintParamsV1 = deserialize(&encoded).unwrap();
        assert_eq!(decoded.coin.inner(), params.coin.inner());
        assert_eq!(decoded.value_commit, params.value_commit);
        assert_eq!(decoded.token_id, params.token_id);
        assert_eq!(decoded.token_commit, params.token_commit);
    }

    #[test]
    fn test_auth_token_mint_params_serialization() {
        let params = AuthTokenMintParamsV1 {
            nullifier: Nullifier::new(pallas::Base::from(1), pallas::Base::from(2)),
            mint_public: pallas::Base::from(3),
            token_id: pallas::Base::from(4),
            token_registry_root: MerkleNode::from_bytes(&[0u8; 32]),
        };
        let encoded = serialize(&params);
        let decoded: AuthTokenMintParamsV1 = deserialize(&encoded).unwrap();
        assert_eq!(decoded.nullifier.inner(), params.nullifier.inner());
        assert_eq!(decoded.mint_public, params.mint_public);
        assert_eq!(decoded.token_id, params.token_id);
    }

    #[test]
    fn test_mint_params_serialization() {
        let auth_proof = AuthProof {
            nullifier: Nullifier::new(pallas::Base::from(1), pallas::Base::from(2)),
            mint_public: pallas::Base::from(3),
        };
        let params = MintParamsV1 {
            auth_proof,
            coin: Coin::from_attributes(
                pallas::Base::from(4),
                500,
                pallas::Base::from(5),
                pallas::Base::zero(),
                pallas::Base::zero(),
                pallas::Base::zero(),
            ),
            value_commit: pallas::Base::from(6),
        };
        let encoded = serialize(&params);
        let decoded: MintParamsV1 = deserialize(&encoded).unwrap();
        assert_eq!(decoded.coin.inner(), params.coin.inner());
        assert_eq!(decoded.value_commit, params.value_commit);
    }

    #[test]
    fn test_burn_params_serialization() {
        let input = Input {
            value_commit: pallas::Base::from(1),
            token_commit: pallas::Base::from(2),
            nullifier: Nullifier::new(pallas::Base::from(3), pallas::Base::from(4)),
            merkle_root: MerkleNode::from_bytes(&[0u8; 32]),
            user_data_enc: pallas::Base::zero(),
            signature_public: pallas::Base::zero(),
            value: 100,
            token_id: pallas::Base::from(5),
            spend_hook: pallas::Base::zero(),
            user_data: pallas::Base::zero(),
            coin_blind: pallas::Base::zero(),
            leaf_position: 0,
            merkle_path: vec![],
        };
        let params = BurnParamsV1 { inputs: vec![input] };
        let encoded = serialize(&params);
        let decoded: BurnParamsV1 = deserialize(&encoded).unwrap();
        assert_eq!(decoded.inputs.len(), 1);
        assert_eq!(decoded.inputs[0].value, 100);
    }

    #[test]
    fn test_transfer_params_serialization() {
        let input = Input {
            value_commit: pallas::Base::from(1),
            token_commit: pallas::Base::from(2),
            nullifier: Nullifier::new(pallas::Base::from(3), pallas::Base::from(4)),
            merkle_root: MerkleNode::from_bytes(&[0u8; 32]),
            user_data_enc: pallas::Base::zero(),
            signature_public: pallas::Base::zero(),
            value: 100,
            token_id: pallas::Base::from(5),
            spend_hook: pallas::Base::zero(),
            user_data: pallas::Base::zero(),
            coin_blind: pallas::Base::zero(),
            leaf_position: 0,
            merkle_path: vec![],
        };

        let output = Output {
            value_commit: pallas::Base::from(6),
            token_commit: pallas::Base::from(7),
            coin: Coin::from_attributes(
                pallas::Base::from(8),
                50,
                pallas::Base::from(5),
                pallas::Base::zero(),
                pallas::Base::zero(),
                pallas::Base::zero(),
            ),
            note: darkfi_sdk::crypto::note::AeadEncryptedNote {
                ciphertext: vec![0u8; 32],
                nonce: [0u8; 12],
                tag: [0u8; 16],
            },
        };

        let params = TransferParamsV1 { inputs: vec![input], outputs: vec![output] };
        let encoded = serialize(&params);
        let decoded: TransferParamsV1 = deserialize(&encoded).unwrap();
        assert_eq!(decoded.inputs.len(), 1);
        assert_eq!(decoded.outputs.len(), 1);
    }

    // ================================================================
    // Update serialization tests
    // ================================================================

    #[test]
    fn test_token_mint_update_serialization() {
        let update = TokenMintUpdateV1 {
            token_id: pallas::Base::from(1),
            coin: Coin::from_attributes(
                pallas::Base::from(2),
                100,
                pallas::Base::from(3),
                pallas::Base::zero(),
                pallas::Base::zero(),
                pallas::Base::zero(),
            ),
        };
        let encoded = serialize(&update);
        let decoded: TokenMintUpdateV1 = deserialize(&encoded).unwrap();
        assert_eq!(decoded.token_id, update.token_id);
        assert_eq!(decoded.coin.inner(), update.coin.inner());
    }

    #[test]
    fn test_auth_token_mint_update_serialization() {
        let update = AuthTokenMintUpdateV1 {
            nullifier: Nullifier::new(pallas::Base::from(1), pallas::Base::from(2)),
        };
        let encoded = serialize(&update);
        let decoded: AuthTokenMintUpdateV1 = deserialize(&encoded).unwrap();
        assert_eq!(decoded.nullifier.inner(), update.nullifier.inner());
    }

    #[test]
    fn test_mint_update_serialization() {
        let update = MintUpdateV1 {
            coin: Coin::from_attributes(
                pallas::Base::from(1),
                500,
                pallas::Base::from(2),
                pallas::Base::zero(),
                pallas::Base::zero(),
                pallas::Base::zero(),
            ),
        };
        let encoded = serialize(&update);
        let decoded: MintUpdateV1 = deserialize(&encoded).unwrap();
        assert_eq!(decoded.coin.inner(), update.coin.inner());
    }

    #[test]
    fn test_burn_update_serialization() {
        let update = BurnUpdateV1 {
            nullifiers: vec![
                Nullifier::new(pallas::Base::from(1), pallas::Base::from(2)),
                Nullifier::new(pallas::Base::from(3), pallas::Base::from(4)),
            ],
        };
        let encoded = serialize(&update);
        let decoded: BurnUpdateV1 = deserialize(&encoded).unwrap();
        assert_eq!(decoded.nullifiers.len(), 2);
        assert_eq!(decoded.nullifiers[0].inner(), update.nullifiers[0].inner());
    }

    #[test]
    fn test_transfer_update_serialization() {
        let update = TransferUpdateV1 {
            nullifiers: vec![Nullifier::new(pallas::Base::from(1), pallas::Base::from(2))],
            coins: vec![
                Coin::from_attributes(
                    pallas::Base::from(3),
                    50,
                    pallas::Base::from(4),
                    pallas::Base::zero(),
                    pallas::Base::zero(),
                    pallas::Base::zero(),
                ),
            ],
        };
        let encoded = serialize(&update);
        let decoded: TransferUpdateV1 = deserialize(&encoded).unwrap();
        assert_eq!(decoded.nullifiers.len(), 1);
        assert_eq!(decoded.coins.len(), 1);
    }

    #[test]
    fn test_otc_swap_update_serialization() {
        let update = OtcSwapUpdateV1 {
            nullifiers: vec![Nullifier::new(pallas::Base::from(1), pallas::Base::from(2))],
            coins: vec![Coin::from_attributes(
                pallas::Base::from(3),
                100,
                pallas::Base::from(4),
                pallas::Base::zero(),
                pallas::Base::zero(),
                pallas::Base::zero(),
            )],
        };
        let encoded = serialize(&update);
        let decoded: OtcSwapUpdateV1 = deserialize(&encoded).unwrap();
        assert_eq!(decoded.nullifiers.len(), 1);
        assert_eq!(decoded.coins.len(), 1);
    }
}