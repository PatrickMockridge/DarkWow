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

//! Block Height Prediction Test Harness
//!
//! Provides isolated testing for Block Height Prediction contract.

use darkfi::{
    zk::{ProvingKey, ZkCircuit},
    zkas::ZkBinary,
};

/// BlockHeightPrediction Harness for isolated testing
pub struct BlockHeightPredictionHarness {
    /// CreateMarket_V1 ZkBinary
    create_market_zkbin: ZkBinary,
    /// CreateMarket_V1 ProvingKey
    create_market_pk: ProvingKey,
    /// CreatePosition_V1 ZkBinary
    create_position_zkbin: ZkBinary,
    /// CreatePosition_V1 ProvingKey
    create_position_pk: ProvingKey,
}

impl BlockHeightPredictionHarness {
    /// Spawn a new BlockHeightPrediction harness with pre-loaded circuits
    pub fn spawn() -> Self {
        let create_market_bin = include_bytes!("../../../block_height_prediction/proof/create_market_v1.zk.bin");
        let create_position_bin = include_bytes!("../../../block_height_prediction/proof/create_position_v1.zk.bin");

        let create_market_zkbin = ZkBinary::decode(create_market_bin, false).unwrap();
        let create_position_zkbin = ZkBinary::decode(create_position_bin, false).unwrap();

        let create_market_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&create_market_zkbin).unwrap(),
            &create_market_zkbin,
        );
        let create_position_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&create_position_zkbin).unwrap(),
            &create_position_zkbin,
        );

        let create_market_pk = ProvingKey::build(create_market_zkbin.k, &create_market_circuit);
        let create_position_pk = ProvingKey::build(create_position_zkbin.k, &create_position_circuit);

        Self {
            create_market_zkbin,
            create_market_pk,
            create_position_zkbin,
            create_position_pk,
        }
    }
}

impl super::ContractHarness for BlockHeightPredictionHarness {
    fn name(&self) -> &str {
        "block_height_prediction"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec!["CreateMarketV1", "CreatePositionV1"]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "CreateMarketV1" => Some(&self.create_market_zkbin),
            "CreatePositionV1" => Some(&self.create_position_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "CreateMarketV1" => Some(&self.create_market_pk),
            "CreatePositionV1" => Some(&self.create_position_pk),
            _ => None,
        }
    }
}