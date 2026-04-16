/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * This program is free software; you can redistribute it and/or
 * it under the terms of the GNU General Public License as published by the
 * Free Software Foundation; either version 3 of the License, or (at your
 * option) any later version.
 *
 * This program is distributed in the hope that it will be useful, but WITHOUT
 * ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
 * FOR A PARTICULAR PURPOSE. See the GNU General Public License for more
 * details.
 *
 * You should have received a copy of the GNU General Public License along
 * with this program; if not, see <https://www.gnu.org/licenses/>.
 */

//! Oracle Test Harness
//!
//! Provides isolated testing for Oracle contract.

use darkfi::{
    zk::{ProvingKey, ZkCircuit},
    zkas::ZkBinary,
};
use darkfi_sdk::{crypto::PublicKey, pasta::pallas};

use darkfi_oracle_contract::client::register_oracle_v1::{
    RegisterOracleV1CallData, register_oracle_v1_proof,
};

/// Oracle Harness for isolated testing
pub struct OracleHarness {
    /// RegisterOracle_V1 ZkBinary
    register_oracle_zkbin: ZkBinary,
    /// RegisterOracle_V1 ProvingKey
    register_oracle_pk: ProvingKey,
}

impl OracleHarness {
    /// Spawn a new Oracle harness with pre-loaded circuits
    pub fn spawn() -> Self {
        let register_oracle_bin =
            include_bytes!("../../../oracle/proof/register_oracle_v1.zk.bin");

        let register_oracle_zkbin =
            ZkBinary::decode(register_oracle_bin, false).unwrap();

        let register_oracle_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&register_oracle_zkbin).unwrap(),
            &register_oracle_zkbin,
        );

        let register_oracle_pk =
            ProvingKey::build(register_oracle_zkbin.k, &register_oracle_circuit);

        Self { register_oracle_zkbin, register_oracle_pk }
    }

    /// Register an oracle
    pub fn register_oracle(
        &self,
        oracle_secret: pallas::Base,
        oracle_public: PublicKey,
    ) -> Result<RegisterOracleResult, Box<dyn std::error::Error>> {
        let input = RegisterOracleV1CallData::new(oracle_secret, oracle_public);

        let (proof, public_inputs) = register_oracle_v1_proof(
            &self.register_oracle_zkbin,
            &self.register_oracle_pk,
            &input,
        )?;

        Ok(RegisterOracleResult {
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
        vec!["RegisterOracleV1"]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "RegisterOracleV1" => Some(&self.register_oracle_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "RegisterOracleV1" => Some(&self.register_oracle_pk),
            _ => None,
        }
    }
}

/// Result of register_oracle
pub struct RegisterOracleResult {
    pub oracle_pub_x: pallas::Base,
    pub oracle_pub_y: pallas::Base,
    pub proof: darkfi::zk::Proof,
}