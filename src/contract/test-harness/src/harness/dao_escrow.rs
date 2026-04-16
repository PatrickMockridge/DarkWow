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

//! DaoEscrow Test Harness
//!
//! Provides isolated testing for DaoEscrow contract.

use darkfi::{
    zk::{ProvingKey, ZkCircuit},
    zkas::ZkBinary,
};

/// DaoEscrow Harness for isolated testing
pub struct DaoEscrowHarness {
    /// Init_V1 ZkBinary
    init_zkbin: ZkBinary,
    /// Init_V1 ProvingKey
    init_pk: ProvingKey,
    /// PayPremium_V1 ZkBinary
    pay_premium_zkbin: ZkBinary,
    /// PayPremium_V1 ProvingKey
    pay_premium_pk: ProvingKey,
}

impl DaoEscrowHarness {
    /// Spawn a new DaoEscrow harness with pre-loaded circuits
    pub fn spawn() -> Self {
        let init_bin = include_bytes!("../../../dao_escrow/proof/init_v1.zk.bin");
        let pay_premium_bin = include_bytes!("../../../dao_escrow/proof/pay_premium_v1.zk.bin");

        let init_zkbin = ZkBinary::decode(init_bin, false).unwrap();
        let pay_premium_zkbin = ZkBinary::decode(pay_premium_bin, false).unwrap();

        let init_circuit =
            ZkCircuit::new(darkfi::zk::empty_witnesses(&init_zkbin).unwrap(), &init_zkbin);
        let pay_premium_circuit =
            ZkCircuit::new(darkfi::zk::empty_witnesses(&pay_premium_zkbin).unwrap(), &pay_premium_zkbin);

        let init_pk = ProvingKey::build(init_zkbin.k, &init_circuit);
        let pay_premium_pk = ProvingKey::build(pay_premium_zkbin.k, &pay_premium_circuit);

        Self { init_zkbin, init_pk, pay_premium_zkbin, pay_premium_pk }
    }
}

impl super::ContractHarness for DaoEscrowHarness {
    fn name(&self) -> &str {
        "dao_escrow"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec!["Init", "PayPremium"]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "Init" => Some(&self.init_zkbin),
            "PayPremium" => Some(&self.pay_premium_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "Init" => Some(&self.init_pk),
            "PayPremium" => Some(&self.pay_premium_pk),
            _ => None,
        }
    }
}