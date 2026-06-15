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

//! Oracle contract integration tests

use dwow_serial::{deserialize, serialize};
use dwow_sdk::pasta::pallas;
use dwow_oracle_contract::{
    model::{AttestValueParamsV1, Oracle, PushValueParamsV1, RegisterOracleParamsV1},
    OracleFunction,
    // Constants
    ORACLE_CONTRACT_ORACLES_TREE, ORACLE_CONTRACT_ATTESTATIONS_TREE, ORACLE_CONTRACT_INFO_TREE,
};

#[test]
fn test_oracle_function_enum_valid() {
    assert!(OracleFunction::try_from(0x00).is_ok()); // RegisterOracleV1
    assert!(OracleFunction::try_from(0x01).is_ok()); // PushValueV1
    assert!(OracleFunction::try_from(0x02).is_ok()); // AttestValueV1
}

#[test]
fn test_oracle_function_enum_invalid() {
    assert!(OracleFunction::try_from(0xFF).is_err());
    assert!(OracleFunction::try_from(0x05).is_err());
    assert!(OracleFunction::try_from(0x10).is_err());
}

#[test]
fn test_oracle_encoding() {
    let oracle = Oracle {
        version: 0,
        id: dwow_sdk::pasta::pallas::Base::from(1),
        oracle_pub_x: dwow_sdk::pasta::pallas::Base::from(2),
        oracle_pub_y: dwow_sdk::pasta::pallas::Base::from(3),
        name: "BTC/USD Price Feed".to_string(),
        data_type: "price".to_string(),
        value: dwow_sdk::pasta::pallas::Base::from(50000),
        updated_at: 50000,
        is_active: true,
    };

    let encoded = serialize(&oracle);
    let decoded: Oracle = deserialize(&encoded).unwrap();

    assert_eq!(decoded.id, oracle.id);
    assert_eq!(decoded.name, oracle.name);
    assert_eq!(decoded.value, oracle.value);
    assert_eq!(decoded.is_active, oracle.is_active);
}

#[test]
fn test_register_oracle_params_encoding() {
    let params = RegisterOracleParamsV1 {
        proof: vec![1, 2, 3],
        oracle_id: pallas::Base::from(1),
        oracle_pub_x: pallas::Base::from(2),
        oracle_pub_y: pallas::Base::from(3),
        name: "BTC/USD Price Feed".to_string(),
        data_type: "price".to_string(),
    };

    let encoded = serialize(&params);
    let decoded: RegisterOracleParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.oracle_id, params.oracle_id);
    assert_eq!(decoded.name, params.name);
    assert_eq!(decoded.data_type, params.data_type);
}

#[test]
fn test_push_value_params_encoding() {
    let params = PushValueParamsV1 {
        proof: vec![1, 2, 3],
        oracle_id: pallas::Base::from(1),
        value: pallas::Base::from(50000),
    };

    let encoded = serialize(&params);
    let decoded: PushValueParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.oracle_id, params.oracle_id);
    assert_eq!(decoded.value, params.value);
}

#[test]
fn test_attest_value_params_encoding() {
    let params = AttestValueParamsV1 {
        proof: vec![1, 2, 3],
        oracle_id: pallas::Base::from(1),
        attestation_id: pallas::Base::from(2),
        predicate: 0, // Matches
        threshold: pallas::Base::from(50000),
    };

    let encoded = serialize(&params);
    let decoded: AttestValueParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.oracle_id, params.oracle_id);
    assert_eq!(decoded.attestation_id, params.attestation_id);
    assert_eq!(decoded.predicate, params.predicate);
    assert_eq!(decoded.threshold, params.threshold);
}

#[test]
fn test_constants() {
    assert_eq!(ORACLE_CONTRACT_ORACLES_TREE, "oracles");
    assert_eq!(ORACLE_CONTRACT_ATTESTATIONS_TREE, "attestations");
    assert_eq!(ORACLE_CONTRACT_INFO_TREE, "info");
}