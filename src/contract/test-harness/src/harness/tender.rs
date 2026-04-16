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

//! Tender Test Harness
//!
//! Provides isolated testing for Tender contract.

use darkfi::{
    zk::{ProvingKey, ZkCircuit},
    zkas::ZkBinary,
};
use darkfi_sdk::crypto::PublicKey;

use darkfi_tender_contract::client::{
    create_tender_v1::{CreateTenderV1CallData, create_tender_v1_proof},
    reveal_bid_v1::{RevealBidV1CallData, reveal_bid_v1_proof},
    select_winner_v1::{SelectWinnerV1CallData, select_winner_v1_proof},
    submit_bid_v1::{SubmitBidV1CallData, submit_bid_v1_proof},
};

/// Tender Harness for isolated testing
pub struct TenderHarness {
    /// CreateTender_V1 ZkBinary
    create_tender_zkbin: ZkBinary,
    /// CreateTender_V1 ProvingKey
    create_tender_pk: ProvingKey,
    /// SubmitBid_V1 ZkBinary
    submit_bid_zkbin: ZkBinary,
    /// SubmitBid_V1 ProvingKey
    submit_bid_pk: ProvingKey,
    /// RevealBid_V1 ZkBinary
    reveal_bid_zkbin: ZkBinary,
    /// RevealBid_V1 ProvingKey
    reveal_bid_pk: ProvingKey,
    /// SelectWinner_V1 ZkBinary
    select_winner_zkbin: ZkBinary,
    /// SelectWinner_V1 ProvingKey
    select_winner_pk: ProvingKey,
}

impl TenderHarness {
    /// Spawn a new Tender harness with pre-loaded circuits
    pub fn spawn() -> Self {
        let create_bin = include_bytes!("../../../tender/proof/create_tender_v1.zk.bin");
        let submit_bin = include_bytes!("../../../tender/proof/submit_bid_v1.zk.bin");
        let reveal_bin = include_bytes!("../../../tender/proof/reveal_bid_v1.zk.bin");
        let select_bin = include_bytes!("../../../tender/proof/select_winner_v1.zk.bin");

        let create_tender_zkbin = ZkBinary::decode(create_bin, false).unwrap();
        let submit_bid_zkbin = ZkBinary::decode(submit_bin, false).unwrap();
        let reveal_bid_zkbin = ZkBinary::decode(reveal_bin, false).unwrap();
        let select_winner_zkbin = ZkBinary::decode(select_bin, false).unwrap();

        let create_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&create_tender_zkbin).unwrap(),
            &create_tender_zkbin,
        );
        let submit_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&submit_bid_zkbin).unwrap(),
            &submit_bid_zkbin,
        );
        let reveal_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&reveal_bid_zkbin).unwrap(),
            &reveal_bid_zkbin,
        );
        let select_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&select_winner_zkbin).unwrap(),
            &select_winner_zkbin,
        );

        let create_tender_pk = ProvingKey::build(create_tender_zkbin.k, &create_circuit);
        let submit_bid_pk = ProvingKey::build(submit_bid_zkbin.k, &submit_circuit);
        let reveal_bid_pk = ProvingKey::build(reveal_bid_zkbin.k, &reveal_circuit);
        let select_winner_pk = ProvingKey::build(select_winner_zkbin.k, &select_circuit);

        Self {
            create_tender_zkbin,
            create_tender_pk,
            submit_bid_zkbin,
            submit_bid_pk,
            reveal_bid_zkbin,
            reveal_bid_pk,
            select_winner_zkbin,
            select_winner_pk,
        }
    }
}

impl super::ContractHarness for TenderHarness {
    fn name(&self) -> &str {
        "tender"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec![
            "CreateTenderV1",
            "SubmitBidV1",
            "RevealBidV1",
            "SelectWinnerV1",
        ]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "CreateTenderV1" => Some(&self.create_tender_zkbin),
            "SubmitBidV1" => Some(&self.submit_bid_zkbin),
            "RevealBidV1" => Some(&self.reveal_bid_zkbin),
            "SelectWinnerV1" => Some(&self.select_winner_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "CreateTenderV1" => Some(&self.create_tender_pk),
            "SubmitBidV1" => Some(&self.submit_bid_pk),
            "RevealBidV1" => Some(&self.reveal_bid_pk),
            "SelectWinnerV1" => Some(&self.select_winner_pk),
            _ => None,
        }
    }
}