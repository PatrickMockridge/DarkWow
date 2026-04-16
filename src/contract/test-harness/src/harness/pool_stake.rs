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

//! PoolStake Test Harness
//!
//! Provides isolated testing for PoolStake contract.

use darkfi::{
    zk::{ProvingKey, ZkCircuit},
    zkas::ZkBinary,
};

/// PoolStake Harness for isolated testing
pub struct PoolStakeHarness {
    /// CreatePool_V1 ZkBinary
    create_pool_zkbin: ZkBinary,
    /// CreatePool_V1 ProvingKey
    create_pool_pk: ProvingKey,
    /// JoinPool_V1 ZkBinary
    join_pool_zkbin: ZkBinary,
    /// JoinPool_V1 ProvingKey
    join_pool_pk: ProvingKey,
    /// AllocateCoverage_V1 ZkBinary
    allocate_coverage_zkbin: ZkBinary,
    /// AllocateCoverage_V1 ProvingKey
    allocate_coverage_pk: ProvingKey,
    /// SlashCoverage_V1 ZkBinary
    slash_coverage_zkbin: ZkBinary,
    /// SlashCoverage_V1 ProvingKey
    slash_coverage_pk: ProvingKey,
}

impl PoolStakeHarness {
    /// Spawn a new PoolStake harness with pre-loaded circuits
    pub fn spawn() -> Self {
        let create_pool_bin =
            include_bytes!("../../../pool_stake/proof/create_pool_v1.zk.bin");
        let join_pool_bin =
            include_bytes!("../../../pool_stake/proof/join_pool_v1.zk.bin");
        let allocate_coverage_bin =
            include_bytes!("../../../pool_stake/proof/allocate_coverage_v1.zk.bin");
        let slash_coverage_bin =
            include_bytes!("../../../pool_stake/proof/slash_coverage_v1.zk.bin");

        let create_pool_zkbin = ZkBinary::decode(create_pool_bin, false).unwrap();
        let join_pool_zkbin = ZkBinary::decode(join_pool_bin, false).unwrap();
        let allocate_coverage_zkbin = ZkBinary::decode(allocate_coverage_bin, false).unwrap();
        let slash_coverage_zkbin = ZkBinary::decode(slash_coverage_bin, false).unwrap();

        let create_pool_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&create_pool_zkbin).unwrap(),
            &create_pool_zkbin,
        );
        let join_pool_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&join_pool_zkbin).unwrap(),
            &join_pool_zkbin,
        );
        let allocate_coverage_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&allocate_coverage_zkbin).unwrap(),
            &allocate_coverage_zkbin,
        );
        let slash_coverage_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&slash_coverage_zkbin).unwrap(),
            &slash_coverage_zkbin,
        );

        let create_pool_pk = ProvingKey::build(create_pool_zkbin.k, &create_pool_circuit);
        let join_pool_pk = ProvingKey::build(join_pool_zkbin.k, &join_pool_circuit);
        let allocate_coverage_pk =
            ProvingKey::build(allocate_coverage_zkbin.k, &allocate_coverage_circuit);
        let slash_coverage_pk =
            ProvingKey::build(slash_coverage_zkbin.k, &slash_coverage_circuit);

        Self {
            create_pool_zkbin,
            create_pool_pk,
            join_pool_zkbin,
            join_pool_pk,
            allocate_coverage_zkbin,
            allocate_coverage_pk,
            slash_coverage_zkbin,
            slash_coverage_pk,
        }
    }
}

impl super::ContractHarness for PoolStakeHarness {
    fn name(&self) -> &str {
        "pool_stake"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec!["CreatePool", "JoinPool", "AllocateCoverage", "SlashCoverage"]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "CreatePool" => Some(&self.create_pool_zkbin),
            "JoinPool" => Some(&self.join_pool_zkbin),
            "AllocateCoverage" => Some(&self.allocate_coverage_zkbin),
            "SlashCoverage" => Some(&self.slash_coverage_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "CreatePool" => Some(&self.create_pool_pk),
            "JoinPool" => Some(&self.join_pool_pk),
            "AllocateCoverage" => Some(&self.allocate_coverage_pk),
            "SlashCoverage" => Some(&self.slash_coverage_pk),
            _ => None,
        }
    }
}