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
            RevokeParamsV1, RevokeUpdateV1,
            CapAttrs, CapCommitment, Input, IssueParamsV1, IssueUpdateV1, Nullifier, Output,
            OtcSwapUpdateV1, RegisterTypeParamsV1, RegisterTypeUpdateV1,
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
    // CapCommitment tests
    // ================================================================

    #[test]
    fn test_commitment_from_attributes() {
        let public_key = pallas::Base::from(1);
        let value = 1000u64;
        let token_id = TokenId::from_base(pallas::Base::from(2));
        let spend_hook = FuncId::none();
        let user_data = pallas::Base::zero();
        let blind = Blind(pallas::Base::from(3));

        let commitment = CapCommitment::from_attributes(public_key, value, token_id, spend_hook, user_data, blind.clone());

        let expected = poseidon_hash([
            dwow_sdk::crypto::constants::DRK_POSEIDON_DOMAIN_CAP_COMMIT,
            public_key,
            pallas::Base::from(value),
            token_id.inner(),
            spend_hook.inner(),
            user_data,
            blind.inner(),
        ]);
        assert_eq!(commitment.inner(), expected);
    }

    #[test]
    fn test_commitment_inner() {
        let base = pallas::Base::from(42);
        let commitment = CapCommitment::from_attributes(
            base,
            100,
            TokenId::from_base(pallas::Base::zero()),
            FuncId::none(),
            pallas::Base::zero(),
            Blind(pallas::Base::zero()),
        );
        assert_ne!(commitment.inner(), pallas::Base::zero());
    }

    #[test]
    fn test_commitment_to_bytes() {
        let public_key = pallas::Base::from(1);
        let commitment = CapCommitment::from_attributes(
            public_key,
            1000,
            TokenId::from_base(pallas::Base::from(2)),
            FuncId::none(),
            pallas::Base::zero(),
            Blind(pallas::Base::zero()),
        );
        let bytes = commitment.to_bytes();
        assert_eq!(bytes.len(), 32);
    }

    // ================================================================
    // CapAttrs tests
    // ================================================================

    #[test]
    fn test_commitment_attributes_to_coin() {
        let attrs = CapAttrs {
            public_key: pallas::Base::from(1),
            value: 500,
            token_id: TokenId::from_base(pallas::Base::from(2)),
            spend_hook: FuncId::none(),
            user_data: pallas::Base::zero(),
            blind: Blind(pallas::Base::from(3)),
        };

        let commitment = attrs.to_commitment();
        let expected = CapCommitment::from_attributes(
            attrs.public_key,
            attrs.value,
            attrs.token_id,
            attrs.spend_hook,
            attrs.user_data,
            attrs.blind,
        );
        assert_eq!(commitment.inner(), expected.inner());
    }

    // ================================================================
    // Serialization tests
    // ================================================================

    #[test]
    fn test_nullifier_serialization() {
        let nullifier = Nullifier::new(pallas::Base::from(123), pallas::Base::from(456));
        let encoded = nullifier.encode();
        let decoded = Nullifier::decode(&encoded).unwrap();
        assert_eq!(decoded.inner(), nullifier.inner());
    }

    #[test]
    fn test_commitment_serialization() {
        let commitment = CapCommitment::from_attributes(
            pallas::Base::from(1),
            1000,
            TokenId::from_base(pallas::Base::from(2)),
            FuncId::none(),
            pallas::Base::zero(),
            Blind(pallas::Base::zero()),
        );
        let encoded = commitment.encode();
        let decoded: CapCommitment = CapCommitment::decode(&encoded).unwrap();
        assert_eq!(decoded.inner(), commitment.inner());
    }

    #[test]
    fn test_register_type_params_serialization() {
        let params = RegisterTypeParamsV1 {
            commitment: CapCommitment::from_attributes(
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
            tx_binding: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        };
        let encoded = serialize(&params);
        let decoded: RegisterTypeParamsV1 = deserialize(&encoded).unwrap();
        assert_eq!(decoded.commitment.inner(), params.commitment.inner());
        assert_eq!(decoded.value_commit, params.value_commit);
        assert_eq!(decoded.token_id, params.token_id);
        assert_eq!(decoded.token_commit, params.token_commit);
    }

    #[test]
    fn test_mint_params_serialization() {
        let params = IssueParamsV1 {
            commitment: CapCommitment::from_attributes(
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
            issue_public: pallas::Base::from(3),
            spend_hook: FuncId::none(),
            tx_binding: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        };
        let encoded = serialize(&params);
        let decoded: IssueParamsV1 = deserialize(&encoded).unwrap();
        assert_eq!(decoded.commitment.inner(), params.commitment.inner());
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
        let params = RevokeParamsV1 {
            inputs: vec![input],
            tx_binding: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        };
        let encoded = serialize(&params);
        let decoded: RevokeParamsV1 = deserialize(&encoded).unwrap();
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
            commitment: CapCommitment::from_attributes(
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

        let params = TransferParamsV1 {
            inputs: vec![input],
            outputs: vec![output],
            tx_binding: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        };
        let encoded = serialize(&params);
        let decoded: TransferParamsV1 = deserialize(&encoded).unwrap();
        assert_eq!(decoded.inputs.len(), 1);
        assert_eq!(decoded.outputs.len(), 1);
    }

    // ================================================================
    // Update serialization tests
    // ================================================================

    #[test]
    fn test_register_type_update_serialization() {
        let update = RegisterTypeUpdateV1 {
            token_id: TokenId::from_base(pallas::Base::from(1)),
            commitment: CapCommitment::from_attributes(
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
        let decoded: RegisterTypeUpdateV1 = deserialize(&encoded).unwrap();
        assert_eq!(decoded.token_id, update.token_id);
        assert_eq!(decoded.commitment.inner(), update.commitment.inner());
    }

    #[test]
    fn test_mint_update_serialization() {
        let update = IssueUpdateV1 {
            commitment: CapCommitment::from_attributes(
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
        let decoded: IssueUpdateV1 = deserialize(&encoded).unwrap();
        assert_eq!(decoded.commitment.inner(), update.commitment.inner());
        assert_eq!(decoded.token_id, update.token_id);
        assert_eq!(decoded.new_coin_count, update.new_coin_count);
    }

    #[test]
    fn test_burn_update_serialization() {
        let update = RevokeUpdateV1 {
            nullifiers: vec![
                Nullifier::new(pallas::Base::from(1), pallas::Base::from(2)),
                Nullifier::new(pallas::Base::from(3), pallas::Base::from(4)),
            ],
        };
        let encoded = serialize(&update);
        let decoded: RevokeUpdateV1 = deserialize(&encoded).unwrap();
        assert_eq!(decoded.nullifiers.len(), 2);
        assert_eq!(decoded.nullifiers[0].inner(), update.nullifiers[0].inner());
    }

    #[test]
    fn test_transfer_update_serialization() {
        let update = TransferUpdateV1 {
            nullifiers: vec![Nullifier::new(pallas::Base::from(1), pallas::Base::from(2))],
            commitments: vec![
                CapCommitment::from_attributes(
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
        assert_eq!(decoded.commitments.len(), 1);
    }

    #[test]
    fn test_otc_swap_update_serialization() {
        let update = OtcSwapUpdateV1 {
            nullifiers: vec![Nullifier::new(pallas::Base::from(1), pallas::Base::from(2))],
            commitments: vec![CapCommitment::from_attributes(
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
        assert_eq!(decoded.commitments.len(), 1);
    }
}