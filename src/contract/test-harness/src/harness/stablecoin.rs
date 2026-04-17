/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
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
 * with this program. If not, see <https://www.gnu.org/licenses/>.
 */

//! Stablecoin Test Harness
//!
//! Provides isolated testing for Stablecoin contract.

use darkfi::{
    zk::{ProvingKey, ZkCircuit},
    zkas::ZkBinary,
};
use darkfi_sdk::pasta::pallas;

use darkfi_stablecoin_contract::client::{
    open_position_v1::{OpenPositionCallData, create_open_position_proof, OpenPositionPublicInputs},
};

/// Stablecoin Harness for isolated testing
pub struct StablecoinHarness {
    /// OpenPosition_V1 ZkBinary
    open_position_zkbin: ZkBinary,
    /// OpenPosition_V1 ProvingKey
    open_position_pk: ProvingKey,
}

impl StablecoinHarness {
    /// Spawn a new Stablecoin harness with pre-loaded circuits
    pub fn spawn() -> Self {
        let open_bin = include_bytes!("../../../stablecoin/proof/open_position_v1.zk.bin");
        let open_position_zkbin = ZkBinary::decode(open_bin, false).unwrap();
        let open_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&open_position_zkbin).unwrap(),
            &open_position_zkbin,
        );
        let open_position_pk = ProvingKey::build(open_position_zkbin.k, &open_circuit);

        Self {
            open_position_zkbin,
            open_position_pk,
        }
    }

    /// Create an open position proof and return all data needed for execution
    pub fn open_position(
        &self,
        owner_secret: pallas::Base,
        collateral_amount: u64,
        debt_amount: u64,
        collateral_type: pallas::Base,
    ) -> Result<OpenPositionResult, Box<dyn std::error::Error>> {
        let input = OpenPositionCallData::new(
            owner_secret,
            collateral_amount,
            debt_amount,
            collateral_type,
        );

        let (proof, public_inputs) = create_open_position_proof(
            &self.open_position_zkbin,
            &self.open_position_pk,
            &input,
        )?;

        Ok(OpenPositionResult {
            position_commitment: public_inputs.position_commitment,
            position_nullifier: public_inputs.position_nullifier,
            owner_public_key: input.owner_public_key(),
            collateral_commitment: input.collateral_commitment(),
            debt_commitment: input.debt_commitment(),
            proof,
        })
    }
}

impl super::ContractHarness for StablecoinHarness {
    fn name(&self) -> &str {
        "stablecoin"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec!["OpenPositionV1"]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "OpenPositionV1" => Some(&self.open_position_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "OpenPositionV1" => Some(&self.open_position_pk),
            _ => None,
        }
    }
}

/// Result of open_position
pub struct OpenPositionResult {
    pub position_commitment: pallas::Base,
    pub position_nullifier: pallas::Base,
    pub owner_public_key: pallas::Base,
    pub collateral_commitment: pallas::Base,
    pub debt_commitment: pallas::Base,
    pub proof: darkfi::zk::Proof,
}
