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

//! PromissoryNote Contract Integration Tests
//!
//! Tests for PromissoryNoteFunction enum and model structures.

#[cfg(test)]
mod tests {
    use dwow_promissory_note_contract::{
        model::{
            BurnParamsV1, BurnUpdateV1,
            Coin, CoinAttributes, Input, MintParamsV1, MintUpdateV1, Nullifier, Output,
            OtcSwapUpdateV1, TokenMintParamsV1, TokenMintUpdateV1,
            TransferParamsV1, TransferUpdateV1, MAX_COIN_VALUE,
        },
        PromissoryNoteFunction, PROMISSORY_NOTE_MAX_COIN_VALUE,
    };
    use dwow_sdk::{
        crypto::{pasta_prelude::Group, poseidon_hash, Blind, FuncId, MerkleNode, TokenId},
        pasta::pallas,
    };
    use dwow_serial::{deserialize, serialize};

    // ================================================================
    // PromissoryNoteFunction enum tests
    // ================================================================

    #[test]
    fn test_promissory_note_function_enum_valid() {
        assert!(PromissoryNoteFunction::try_from(0x00).is_ok()); // RegisterTypeV1
        assert!(PromissoryNoteFunction::try_from(0x01).is_ok()); // RedeemV1
        assert!(PromissoryNoteFunction::try_from(0x02).is_ok()); // IssueV1
        assert!(PromissoryNoteFunction::try_from(0x03).is_ok()); // RevokeV1
        assert!(PromissoryNoteFunction::try_from(0x04).is_ok()); // TransferV1
        assert!(PromissoryNoteFunction::try_from(0x05).is_ok()); // OtcSwapV1
    }

    #[test]
    fn test_promissory_note_function_enum_invalid() {
        assert!(PromissoryNoteFunction::try_from(0xFF).is_err()); // Invalid
        assert!(PromissoryNoteFunction::try_from(0x06).is_err()); // Out of range
        assert!(PromissoryNoteFunction::try_from(0x07).is_err()); // Out of range
        assert!(PromissoryNoteFunction::try_from(0x10).is_err()); // Out of range
    }

    #[test]
    fn test_promissory_note_function_names() {
        assert_eq!(PromissoryNoteFunction::RegisterTypeV1 as u8, 0x00);
        assert_eq!(PromissoryNoteFunction::IssueV1 as u8, 0x02);
        assert_eq!(PromissoryNoteFunction::RevokeV1 as u8, 0x03);
        assert_eq!(PromissoryNoteFunction::TransferV1 as u8, 0x04);
        assert_eq!(PromissoryNoteFunction::OtcSwapV1 as u8, 0x05);
    }

    // ================================================================
    // Constants tests
    // ================================================================

    #[test]
    fn test_max_coin_value() {
        assert_eq!(MAX_COIN_VALUE, 1_000_000_000_000u64);
        assert_eq!(PROMISSORY_NOTE_MAX_COIN_VALUE, 1_000_000_000_000u64);
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
        let token_id = TokenId::from_base(pallas::Base::from(2));
        let spend_hook = FuncId::none();
        let user_data = pallas::Base::zero();
        let blind = Blind(pallas::Base::from(3));

        let coin = Coin::from_attributes(public_key, value, token_id, spend_hook, user_data, blind);

        let expected = poseidon_hash([
            public_key,
            pallas::Base::from(value),
            token_id.inner(),
            spend_hook.inner(),
            user_data,
            blind.inner(),
        ]);
        assert_eq!(coin.inner(), expected);
    }

    #[test]
    fn test_coin_inner() {
        let base = pallas::Base::from(42);
        let coin = Coin::from_attributes(
            base,
            100,
            TokenId::from_base(pallas::Base::zero()),
            FuncId::none(),
            pallas::Base::zero(),
            Blind(pallas::Base::zero()),
        );
        assert_ne!(coin.inner(), pallas::Base::zero());
    }

    #[test]
    fn test_coin_to_bytes() {
        let public_key = pallas::Base::from(1);
        let coin = Coin::from_attributes(
            public_key,
            1000,
            TokenId::from_base(pallas::Base::from(2)),
            FuncId::none(),
            pallas::Base::zero(),
            Blind(pallas::Base::zero()),
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
            token_id: TokenId::from_base(pallas::Base::from(2)),
            spend_hook: FuncId::none(),
            user_data: pallas::Base::zero(),
            blind: Blind(pallas::Base::from(3)),
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
            TokenId::from_base(pallas::Base::from(2)),
            FuncId::none(),
            pallas::Base::zero(),
            Blind(pallas::Base::zero()),
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
                TokenId::from_base(pallas::Base::from(2)),
                FuncId::none(),
                pallas::Base::zero(),
                Blind(pallas::Base::zero()),
            ),
            value_commit: pallas::Point::generator(),
            token_id: TokenId::from_base(pallas::Base::from(4)),
            token_auth_parent: pallas::Base::from(0),
            token_commit: pallas::Base::from(5),
            spend_hook: FuncId::none(),
        };
        let encoded = serialize(&params);
        let decoded: TokenMintParamsV1 = deserialize(&encoded).unwrap();
        assert_eq!(decoded.coin.inner(), params.coin.inner());
        assert_eq!(decoded.value_commit, params.value_commit);
        assert_eq!(decoded.token_id, params.token_id);
        assert_eq!(decoded.token_commit, params.token_commit);
    }

    #[test]
    fn test_mint_params_serialization() {
        let params = MintParamsV1 {
            coin: Coin::from_attributes(
                pallas::Base::from(4),
                500,
                TokenId::from_base(pallas::Base::from(5)),
                FuncId::none(),
                pallas::Base::zero(),
                Blind(pallas::Base::zero()),
            ),
            value_commit: pallas::Point::generator(),
            token_id: TokenId::from_base(pallas::Base::from(5)),
            token_registry_root: MerkleNode::from_bytes([0u8; 32]).unwrap(),
            mint_public: pallas::Base::from(3),
            spend_hook: FuncId::none(),
        };
        let encoded = serialize(&params);
        let decoded: MintParamsV1 = deserialize(&encoded).unwrap();
        assert_eq!(decoded.coin.inner(), params.coin.inner());
        assert_eq!(decoded.value_commit, params.value_commit);
    }

    #[test]
    fn test_burn_params_serialization() {
        let input = Input {
            value_commit: pallas::Point::generator(),
            token_commit: pallas::Base::from(2),
            nullifier: Nullifier::new(pallas::Base::from(3), pallas::Base::from(4)),
            merkle_root: MerkleNode::from_bytes([0u8; 32]).unwrap(),
            user_data_enc: pallas::Base::zero(),
            spend_hook: FuncId::none(),
            signature_public: pallas::Base::zero(),
        };
        let params = BurnParamsV1 { inputs: vec![input] };
        let encoded = serialize(&params);
        let decoded: BurnParamsV1 = deserialize(&encoded).unwrap();
        assert_eq!(decoded.inputs.len(), 1);
        assert_eq!(decoded.inputs[0].value_commit, pallas::Point::generator());
    }

    #[test]
    fn test_transfer_params_serialization() {
        let input = Input {
            value_commit: pallas::Point::generator(),
            token_commit: pallas::Base::from(2),
            nullifier: Nullifier::new(pallas::Base::from(3), pallas::Base::from(4)),
            merkle_root: MerkleNode::from_bytes([0u8; 32]).unwrap(),
            user_data_enc: pallas::Base::zero(),
            spend_hook: FuncId::none(),
            signature_public: pallas::Base::zero(),
        };

        let output = Output {
            value_commit: pallas::Point::generator(),
            token_commit: pallas::Base::from(7),
            coin: Coin::from_attributes(
                pallas::Base::from(8),
                50,
                TokenId::from_base(pallas::Base::from(5)),
                FuncId::none(),
                pallas::Base::zero(),
                Blind(pallas::Base::zero()),
            ),
            note: dwow_sdk::crypto::note::AeadEncryptedNote {
                ciphertext: vec![0u8; 32],
                ephem_public: dwow_sdk::crypto::PublicKey::try_from(
                    dwow_sdk::pasta::pallas::Point::generator(),
                ).unwrap(),
            },
            spend_hook: FuncId::none(),
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
            token_id: TokenId::from_base(pallas::Base::from(1)),
            coin: Coin::from_attributes(
                pallas::Base::from(2),
                100,
                TokenId::from_base(pallas::Base::from(3)),
                FuncId::none(),
                pallas::Base::zero(),
                Blind(pallas::Base::zero()),
            ),
            token_auth_parent: pallas::Base::zero(),
        };
        let encoded = serialize(&update);
        let decoded: TokenMintUpdateV1 = deserialize(&encoded).unwrap();
        assert_eq!(decoded.token_id, update.token_id);
        assert_eq!(decoded.coin.inner(), update.coin.inner());
    }

    #[test]
    fn test_mint_update_serialization() {
        let update = MintUpdateV1 {
            coin: Coin::from_attributes(
                pallas::Base::from(1),
                500,
                TokenId::from_base(pallas::Base::from(2)),
                FuncId::none(),
                pallas::Base::zero(),
                Blind(pallas::Base::zero()),
            ),
            token_id: TokenId::from_base(pallas::Base::from(2)),
            new_coin_count: 1,
        };
        let encoded = serialize(&update);
        let decoded: MintUpdateV1 = deserialize(&encoded).unwrap();
        assert_eq!(decoded.coin.inner(), update.coin.inner());
        assert_eq!(decoded.token_id, update.token_id);
        assert_eq!(decoded.new_coin_count, update.new_coin_count);
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
                    TokenId::from_base(pallas::Base::from(4)),
                    FuncId::none(),
                    pallas::Base::zero(),
                    Blind(pallas::Base::zero()),
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
                TokenId::from_base(pallas::Base::from(4)),
                FuncId::none(),
                pallas::Base::zero(),
                Blind(pallas::Base::zero()),
            )],
        };
        let encoded = serialize(&update);
        let decoded: OtcSwapUpdateV1 = deserialize(&encoded).unwrap();
        assert_eq!(decoded.nullifiers.len(), 1);
        assert_eq!(decoded.coins.len(), 1);
    }
}