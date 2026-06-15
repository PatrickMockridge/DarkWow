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

//! OTC Swap Test Harness

use dwow_core::{
    zk::{empty_witnesses, ProvingKey, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::pasta::pallas;

/// OTC Swap Harness for isolated testing
pub struct OtcSwapHarness {
    create_zkbin: ZkBinary,
    create_pk: ProvingKey,
    fund_zkbin: ZkBinary,
    fund_pk: ProvingKey,
    execute_zkbin: ZkBinary,
    execute_pk: ProvingKey,
    cancel_zkbin: ZkBinary,
    cancel_pk: ProvingKey,
}

impl OtcSwapHarness {
    pub fn spawn() -> Self {
        let create_bin = include_bytes!("../../../otc_swap/proof/create_swap_v1.zk.bin");
        let fund_bin = include_bytes!("../../../otc_swap/proof/fund_swap_v1.zk.bin");
        let execute_bin = include_bytes!("../../../otc_swap/proof/execute_swap_v1.zk.bin");
        let cancel_bin = include_bytes!("../../../otc_swap/proof/cancel_swap_v1.zk.bin");

        let create_zkbin = ZkBinary::decode(create_bin, false).unwrap();
        let create_circuit = ZkCircuit::new(empty_witnesses(&create_zkbin).unwrap(), &create_zkbin);
        let create_pk = ProvingKey::build(create_zkbin.k, &create_circuit);
        let fund_zkbin = ZkBinary::decode(fund_bin, false).unwrap();
        let fund_circuit = ZkCircuit::new(empty_witnesses(&fund_zkbin).unwrap(), &fund_zkbin);
        let fund_pk = ProvingKey::build(fund_zkbin.k, &fund_circuit);
        let execute_zkbin = ZkBinary::decode(execute_bin, false).unwrap();
        let execute_circuit = ZkCircuit::new(empty_witnesses(&execute_zkbin).unwrap(), &execute_zkbin);
        let execute_pk = ProvingKey::build(execute_zkbin.k, &execute_circuit);
        let cancel_zkbin = ZkBinary::decode(cancel_bin, false).unwrap();
        let cancel_circuit = ZkCircuit::new(empty_witnesses(&cancel_zkbin).unwrap(), &cancel_zkbin);
        let cancel_pk = ProvingKey::build(cancel_zkbin.k, &cancel_circuit);

        Self { create_zkbin, create_pk, fund_zkbin, fund_pk, execute_zkbin, execute_pk, cancel_zkbin, cancel_pk }
    }
}

impl super::ContractHarness for OtcSwapHarness {
    fn name(&self) -> &str {
        "otc_swap"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec!["CreateSwap", "FundSwap", "ExecuteSwap", "CancelSwap"]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "CreateSwap" => Some(&self.create_zkbin),
            "FundSwap" => Some(&self.fund_zkbin),
            "ExecuteSwap" => Some(&self.execute_zkbin),
            "CancelSwap" => Some(&self.cancel_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "CreateSwap" => Some(&self.create_pk),
            "FundSwap" => Some(&self.fund_pk),
            "ExecuteSwap" => Some(&self.execute_pk),
            "CancelSwap" => Some(&self.cancel_pk),
            _ => None,
        }
    }
}
