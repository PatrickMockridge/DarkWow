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

//! DarkbetExchange Test Harness
//!
//! Provides isolated testing for DarkbetExchange contract.

use darkfi::{
    zk::{ProvingKey, ZkCircuit},
    zkas::ZkBinary,
};

/// DarkbetExchange Harness for isolated testing
pub struct DarkbetExchangeHarness {
    /// CreateMarket_V1 ZkBinary
    create_market_zkbin: ZkBinary,
    /// CreateMarket_V1 ProvingKey
    create_market_pk: ProvingKey,
    /// BuyPosition_V1 ZkBinary
    buy_position_zkbin: ZkBinary,
    /// BuyPosition_V1 ProvingKey
    buy_position_pk: ProvingKey,
    /// ClaimWinnings_V1 ZkBinary
    claim_winnings_zkbin: ZkBinary,
    /// ClaimWinnings_V1 ProvingKey
    claim_winnings_pk: ProvingKey,
    /// AddLiquidity_V1 ZkBinary
    add_liquidity_zkbin: ZkBinary,
    /// AddLiquidity_V1 ProvingKey
    add_liquidity_pk: ProvingKey,
}

impl DarkbetExchangeHarness {
    /// Spawn a new DarkbetExchange harness with pre-loaded circuits
    pub fn spawn() -> Self {
        let create_market_bin =
            include_bytes!("../../../darkbet_exchange/proof/create_market_v1.zk.bin");
        let buy_position_bin =
            include_bytes!("../../../darkbet_exchange/proof/buy_position_v1.zk.bin");
        let claim_winnings_bin =
            include_bytes!("../../../darkbet_exchange/proof/claim_winnings_v1.zk.bin");
        let add_liquidity_bin =
            include_bytes!("../../../darkbet_exchange/proof/add_liquidity_v1.zk.bin");

        let create_market_zkbin = ZkBinary::decode(create_market_bin, false).unwrap();
        let buy_position_zkbin = ZkBinary::decode(buy_position_bin, false).unwrap();
        let claim_winnings_zkbin = ZkBinary::decode(claim_winnings_bin, false).unwrap();
        let add_liquidity_zkbin = ZkBinary::decode(add_liquidity_bin, false).unwrap();

        let create_market_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&create_market_zkbin).unwrap(),
            &create_market_zkbin,
        );
        let buy_position_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&buy_position_zkbin).unwrap(),
            &buy_position_zkbin,
        );
        let claim_winnings_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&claim_winnings_zkbin).unwrap(),
            &claim_winnings_zkbin,
        );
        let add_liquidity_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&add_liquidity_zkbin).unwrap(),
            &add_liquidity_zkbin,
        );

        let create_market_pk = ProvingKey::build(create_market_zkbin.k, &create_market_circuit);
        let buy_position_pk = ProvingKey::build(buy_position_zkbin.k, &buy_position_circuit);
        let claim_winnings_pk =
            ProvingKey::build(claim_winnings_zkbin.k, &claim_winnings_circuit);
        let add_liquidity_pk = ProvingKey::build(add_liquidity_zkbin.k, &add_liquidity_circuit);

        Self {
            create_market_zkbin,
            create_market_pk,
            buy_position_zkbin,
            buy_position_pk,
            claim_winnings_zkbin,
            claim_winnings_pk,
            add_liquidity_zkbin,
            add_liquidity_pk,
        }
    }
}

impl super::ContractHarness for DarkbetExchangeHarness {
    fn name(&self) -> &str {
        "darkbet_exchange"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec!["CreateMarket", "BuyPosition", "ClaimWinnings", "AddLiquidity"]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "CreateMarket" => Some(&self.create_market_zkbin),
            "BuyPosition" => Some(&self.buy_position_zkbin),
            "ClaimWinnings" => Some(&self.claim_winnings_zkbin),
            "AddLiquidity" => Some(&self.add_liquidity_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "CreateMarket" => Some(&self.create_market_pk),
            "BuyPosition" => Some(&self.buy_position_pk),
            "ClaimWinnings" => Some(&self.claim_winnings_pk),
            "AddLiquidity" => Some(&self.add_liquidity_pk),
            _ => None,
        }
    }
}