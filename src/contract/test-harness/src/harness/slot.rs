/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * This program is free software; you can redistribute it and/or
 * it under the terms of the GNU General Public License as published by the
 * Free Software Foundation; either version 3 of the License, or at your
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

//! Slot Test Harness
//!
//! Provides isolated testing for Slot contract.

use darkfi::{
    zk::{ProvingKey, ZkCircuit},
    zkas::ZkBinary,
};

/// Slot Harness for isolated testing
pub struct SlotHarness {
    /// CommitBet_V1 ZkBinary
    commit_bet_zkbin: ZkBinary,
    /// CommitBet_V1 ProvingKey
    commit_bet_pk: ProvingKey,
    /// SettleBet_V1 ZkBinary
    settle_bet_zkbin: ZkBinary,
    /// SettleBet_V1 ProvingKey
    settle_bet_pk: ProvingKey,
}

impl SlotHarness {
    /// Spawn a new Slot harness with pre-loaded circuits
    pub fn spawn() -> Self {
        let commit_bet_bin = include_bytes!("../../../slot/proof/commit_bet_v1.zk.bin");
        let settle_bet_bin = include_bytes!("../../../slot/proof/settle_bet_v1.zk.bin");

        let commit_bet_zkbin = ZkBinary::decode(commit_bet_bin, false).unwrap();
        let settle_bet_zkbin = ZkBinary::decode(settle_bet_bin, false).unwrap();

        let commit_bet_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&commit_bet_zkbin).unwrap(),
            &commit_bet_zkbin,
        );
        let settle_bet_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&settle_bet_zkbin).unwrap(),
            &settle_bet_zkbin,
        );

        let commit_bet_pk = ProvingKey::build(commit_bet_zkbin.k, &commit_bet_circuit);
        let settle_bet_pk = ProvingKey::build(settle_bet_zkbin.k, &settle_bet_circuit);

        Self { commit_bet_zkbin, commit_bet_pk, settle_bet_zkbin, settle_bet_pk }
    }
}

impl super::ContractHarness for SlotHarness {
    fn name(&self) -> &str {
        "slot"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec!["CommitBetV1", "SettleBetV1"]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "CommitBetV1" => Some(&self.commit_bet_zkbin),
            "SettleBetV1" => Some(&self.settle_bet_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "CommitBetV1" => Some(&self.commit_bet_pk),
            "SettleBetV1" => Some(&self.settle_bet_pk),
            _ => None,
        }
    }
}