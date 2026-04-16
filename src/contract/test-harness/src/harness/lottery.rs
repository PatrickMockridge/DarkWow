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

//! Lottery Test Harness
//!
//! Provides isolated testing for Lottery contract.

use darkfi::{
    zk::{ProvingKey, ZkCircuit},
    zkas::ZkBinary,
};

/// Lottery Harness for isolated testing
pub struct LotteryHarness {
    /// CommitTicket_V1 ZkBinary
    commit_ticket_zkbin: ZkBinary,
    /// CommitTicket_V1 ProvingKey
    commit_ticket_pk: ProvingKey,
    /// RevealTicket_V1 ZkBinary
    reveal_ticket_zkbin: ZkBinary,
    /// RevealTicket_V1 ProvingKey
    reveal_ticket_pk: ProvingKey,
}

impl LotteryHarness {
    /// Spawn a new Lottery harness with pre-loaded circuits
    pub fn spawn() -> Self {
        let commit_ticket_bin = include_bytes!("../../../lottery/proof/commit_ticket_v1.zk.bin");
        let reveal_ticket_bin = include_bytes!("../../../lottery/proof/reveal_ticket_v1.zk.bin");

        let commit_ticket_zkbin = ZkBinary::decode(commit_ticket_bin, false).unwrap();
        let reveal_ticket_zkbin = ZkBinary::decode(reveal_ticket_bin, false).unwrap();

        let commit_ticket_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&commit_ticket_zkbin).unwrap(),
            &commit_ticket_zkbin,
        );
        let reveal_ticket_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&reveal_ticket_zkbin).unwrap(),
            &reveal_ticket_zkbin,
        );

        let commit_ticket_pk = ProvingKey::build(commit_ticket_zkbin.k, &commit_ticket_circuit);
        let reveal_ticket_pk = ProvingKey::build(reveal_ticket_zkbin.k, &reveal_ticket_circuit);

        Self { commit_ticket_zkbin, commit_ticket_pk, reveal_ticket_zkbin, reveal_ticket_pk }
    }
}

impl super::ContractHarness for LotteryHarness {
    fn name(&self) -> &str {
        "lottery"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec!["CommitTicketV1", "RevealTicketV1"]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "CommitTicketV1" => Some(&self.commit_ticket_zkbin),
            "RevealTicketV1" => Some(&self.reveal_ticket_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "CommitTicketV1" => Some(&self.commit_ticket_pk),
            "RevealTicketV1" => Some(&self.reveal_ticket_pk),
            _ => None,
        }
    }
}