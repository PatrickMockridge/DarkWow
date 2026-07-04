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

//! Oracle Test Harness
//!
//! Provides isolated testing for Oracle contract.

use dwow_core::{
    zk::{ProvingKey, ZkCircuit},
    zkas::ZkBinary,
};
use dwow_sdk::{crypto::PublicKey, pasta::pallas};
use dwow_serial::Encodable;

use dwow_oracle_contract::client::register_oracle_v1::{
    RegisterOracleV1CallData, register_oracle_v1_proof,
};
use dwow_oracle_contract::model::RegisterOracleParamsV1;

/// Oracle Harness for isolated testing
pub struct OracleHarness {
    /// RegisterOracle_V1 ZkBinary
    register_oracle_zkbin: ZkBinary,
    /// RegisterOracle_V1 ProvingKey
    register_oracle_pk: ProvingKey,
    /// PushValueCommitment_V1 ZkBinary
    push_value_commitment_zkbin: ZkBinary,
    /// PushValueCommitment_V1 ProvingKey
    push_value_commitment_pk: ProvingKey,
}

impl OracleHarness {
    /// Spawn a new Oracle harness with pre-loaded circuits
    pub fn spawn() -> Self {
        let register_oracle_bin =
            include_bytes!("../../../oracle/proof/register_oracle_v1.zk.bin");
        let push_value_commitment_bin =
            include_bytes!("../../../oracle/proof/push_value_commitment_v1.zk.bin");

        let register_oracle_zkbin =
            ZkBinary::decode(register_oracle_bin, false).unwrap();
        let push_value_commitment_zkbin =
            ZkBinary::decode(push_value_commitment_bin, false).unwrap();

        let register_oracle_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&register_oracle_zkbin).unwrap(),
            &register_oracle_zkbin,
        );
        let push_value_commitment_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&push_value_commitment_zkbin).unwrap(),
            &push_value_commitment_zkbin,
        );

        let register_oracle_pk =
            ProvingKey::build(register_oracle_zkbin.k, &register_oracle_circuit).expect("ProvingKey::build failed");
        let push_value_commitment_pk =
            ProvingKey::build(push_value_commitment_zkbin.k, &push_value_commitment_circuit).expect("ProvingKey::build failed");

        Self { register_oracle_zkbin, register_oracle_pk, push_value_commitment_zkbin, push_value_commitment_pk }
    }

    /// Register an oracle
    pub fn register_oracle(
        &self,
        oracle_secret: pallas::Base,
        oracle_public: PublicKey,
        oracle_id: pallas::Base,
        name: String,
        data_type: String,
    ) -> Result<RegisterOracleResult, Box<dyn std::error::Error>> {
        let input = RegisterOracleV1CallData::new(oracle_secret, oracle_public);

        let (proof, public_inputs) = register_oracle_v1_proof(
            &self.register_oracle_zkbin,
            &self.register_oracle_pk,
            &input,
        )?;

        // Build RegisterOracleParamsV1 for call_data
        let params = RegisterOracleParamsV1 {
            proof: vec![],
            oracle_id,
            oracle_pub_x: public_inputs.oracle_pub_x,
            oracle_pub_y: public_inputs.oracle_pub_y,
            name,
            data_type,
        };

        let mut call_data = vec![];
        params.encode(&mut call_data)?;

        Ok(RegisterOracleResult {
            call_data,
            oracle_pub_x: public_inputs.oracle_pub_x,
            oracle_pub_y: public_inputs.oracle_pub_y,
            proof,
        })
    }
}

impl super::ContractHarness for OracleHarness {
    fn name(&self) -> &str {
        "oracle"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec!["RegisterOracleV1", "PushValueCommitment"]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "RegisterOracleV1" => Some(&self.register_oracle_zkbin),
            "PushValueCommitment" => Some(&self.push_value_commitment_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "RegisterOracleV1" => Some(&self.register_oracle_pk),
            "PushValueCommitment" => Some(&self.push_value_commitment_pk),
            _ => None,
        }
    }
}

/// Result of register_oracle
pub struct RegisterOracleResult {
    pub call_data: Vec<u8>,
    pub oracle_pub_x: pallas::Base,
    pub oracle_pub_y: pallas::Base,
    pub proof: dwow_core::zk::Proof,
}
