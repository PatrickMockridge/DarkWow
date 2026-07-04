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

//! Purse Test Harness
//!
//! Provides isolated testing for Purse contract (balance/deposit/withdraw).

use dwow_core::{
    zk::{ProvingKey, ZkCircuit},
    zkas::ZkBinary,
};

/// Purse Harness for isolated testing
pub struct PurseHarness {
    /// BalanceV1 ZkBinary
    balance_zkbin: ZkBinary,
    /// BalanceV1 ProvingKey
    balance_pk: ProvingKey,
    /// DepositV1 ZkBinary
    deposit_zkbin: ZkBinary,
    /// DepositV1 ProvingKey
    deposit_pk: ProvingKey,
    /// WithdrawV1 ZkBinary
    withdraw_zkbin: ZkBinary,
    /// WithdrawV1 ProvingKey
    withdraw_pk: ProvingKey,
}

impl PurseHarness {
    /// Spawn a new Purse harness with pre-loaded circuits
    pub fn spawn() -> Self {
        // Load circuit binaries
        let balance_bin = include_bytes!("../../../purse/proof/balance_v1.zk.bin");
        let deposit_bin = include_bytes!("../../../purse/proof/deposit_v1.zk.bin");
        let withdraw_bin = include_bytes!("../../../purse/proof/withdraw_v1.zk.bin");

        let balance_zkbin = ZkBinary::decode(balance_bin, false).unwrap();
        let deposit_zkbin = ZkBinary::decode(deposit_bin, false).unwrap();
        let withdraw_zkbin = ZkBinary::decode(withdraw_bin, false).unwrap();

        // Build proving keys
        let balance_circuit =
            ZkCircuit::new(dwow_core::zk::empty_witnesses(&balance_zkbin).unwrap(), &balance_zkbin);
        let deposit_circuit =
            ZkCircuit::new(dwow_core::zk::empty_witnesses(&deposit_zkbin).unwrap(), &deposit_zkbin);
        let withdraw_circuit =
            ZkCircuit::new(dwow_core::zk::empty_witnesses(&withdraw_zkbin).unwrap(), &withdraw_zkbin);

        let balance_pk = ProvingKey::build(balance_zkbin.k, &balance_circuit).expect("ProvingKey::build failed");
        let deposit_pk = ProvingKey::build(deposit_zkbin.k, &deposit_circuit).expect("ProvingKey::build failed");
        let withdraw_pk = ProvingKey::build(withdraw_zkbin.k, &withdraw_circuit).expect("ProvingKey::build failed");

        Self {
            balance_zkbin,
            balance_pk,
            deposit_zkbin,
            deposit_pk,
            withdraw_zkbin,
            withdraw_pk,
        }
    }
}

impl super::ContractHarness for PurseHarness {
    fn name(&self) -> &str {
        "purse"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec!["BalanceV1", "DepositV1", "WithdrawV1"]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "BalanceV1" => Some(&self.balance_zkbin),
            "DepositV1" => Some(&self.deposit_zkbin),
            "WithdrawV1" => Some(&self.withdraw_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "BalanceV1" => Some(&self.balance_pk),
            "DepositV1" => Some(&self.deposit_pk),
            "WithdrawV1" => Some(&self.withdraw_pk),
            _ => None,
        }
    }
}
