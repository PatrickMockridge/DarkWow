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

//! Integration tests for the MultiSig contract.
//!
//! Tests data model encode/decode round-trips without ZK proofs.

use dwow_multisig_contract::model::{CreateGroupParamsV1, SignParamsV1, FinalizeParamsV1, GroupId};
use dwow_sdk::crypto::{Keypair, SecretKey};
use dwow_sdk::pasta::pallas;

fn dummy_pubkey() -> dwow_sdk::crypto::PublicKey {
    Keypair::new(SecretKey::from_base(pallas::Base::from(42))).public
}

#[test]
fn test_create_group_params_roundtrip() {
    let params = CreateGroupParamsV1 {
        pubkeys: vec![dummy_pubkey(); 3],
        threshold: 2,
        proof: vec![1u8, 2, 3, 4],
        tx_binding: pallas::Base::from(99u64),
        tx_nonce: pallas::Base::from(88u64),
    };

    let encoded = params.encode();
    assert!(!encoded.is_empty(), "encode must produce non-empty output");

    let decoded = CreateGroupParamsV1::decode(&encoded)
        .expect("round-trip must succeed");
    assert_eq!(decoded.threshold, params.threshold);
    assert_eq!(decoded.pubkeys.len(), params.pubkeys.len());
    assert_eq!(decoded.proof, params.proof);

    assert_eq!(params.encode(), encoded, "encode must be deterministic");
}

#[test]
fn test_sign_params_roundtrip() {
    let params = SignParamsV1 {
        group_id: GroupId(pallas::Base::from(42u64)),
        message_hash: pallas::Base::from(12345u64),
        signer_pub: dummy_pubkey(),
        proof: vec![1u8, 2, 3, 4],
        tx_binding: pallas::Base::from(99u64),
        tx_nonce: pallas::Base::from(88u64),
    };

    let encoded = params.encode();
    assert!(!encoded.is_empty());

    let decoded = SignParamsV1::decode(&encoded)
        .expect("round-trip must succeed");
    assert_eq!(decoded.message_hash, params.message_hash);
    assert_eq!(decoded.proof, params.proof);
    assert_eq!(decoded.tx_binding, params.tx_binding);

    assert_eq!(params.encode(), encoded, "encode must be deterministic");
}

#[test]
fn test_finalize_params_roundtrip() {
    let params = FinalizeParamsV1 {
        group_id: GroupId(pallas::Base::from(42u64)),
        message_hash: pallas::Base::from(12345u64),
        proof: vec![5u8, 6, 7, 8],
        tx_binding: pallas::Base::from(99u64),
        tx_nonce: pallas::Base::from(88u64),
    };

    let encoded = params.encode();
    assert!(!encoded.is_empty());

    let decoded = FinalizeParamsV1::decode(&encoded)
        .expect("round-trip must succeed");
    assert_eq!(decoded.message_hash, params.message_hash);
    assert_eq!(decoded.proof, params.proof);

    assert_eq!(params.encode(), encoded, "encode must be deterministic");
}

#[test]
fn test_decode_rejects_empty() {
    assert!(CreateGroupParamsV1::decode(&[]).is_err());
    assert!(SignParamsV1::decode(&[]).is_err());
    assert!(FinalizeParamsV1::decode(&[]).is_err());
}

#[test]
fn test_decode_rejects_short() {
    assert!(CreateGroupParamsV1::decode(&[0u8; 5]).is_err());
    assert!(SignParamsV1::decode(&[0u8; 10]).is_err());
    assert!(FinalizeParamsV1::decode(&[0u8; 10]).is_err());
}
