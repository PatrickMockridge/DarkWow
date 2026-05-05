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

//! AtomicSwap Test Harness
//!
//! Provides isolated testing for AtomicSwap contract.

use darkfi::{
    zk::{ProvingKey, ZkCircuit},
    zkas::ZkBinary,
};

/// AtomicSwap Harness for isolated testing
pub struct AtomicSwapHarness {
    /// CreateSwap_V1 ZkBinary
    create_swap_zkbin: ZkBinary,
    /// CreateSwap_V1 ProvingKey
    create_swap_pk: ProvingKey,
    /// ClaimSwap_V1 ZkBinary
    claim_swap_zkbin: ZkBinary,
    /// ClaimSwap_V1 ProvingKey
    claim_swap_pk: ProvingKey,
    /// RefundSwap_V1 ZkBinary
    refund_swap_zkbin: ZkBinary,
    /// RefundSwap_V1 ProvingKey
    refund_swap_pk: ProvingKey,
}

impl AtomicSwapHarness {
    /// Spawn a new AtomicSwap harness with pre-loaded circuits
    pub fn spawn() -> Self {
        let create_bin = include_bytes!("../../../atomic_swap/proof/create_swap_v1.zk.bin");
        let claim_bin = include_bytes!("../../../atomic_swap/proof/claim_v1.zk.bin");
        let refund_bin = include_bytes!("../../../atomic_swap/proof/refund_v1.zk.bin");

        let create_swap_zkbin = ZkBinary::decode(create_bin, false).unwrap();
        let claim_swap_zkbin = ZkBinary::decode(claim_bin, false).unwrap();
        let refund_swap_zkbin = ZkBinary::decode(refund_bin, false).unwrap();

        let create_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&create_swap_zkbin).unwrap(),
            &create_swap_zkbin,
        );
        let claim_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&claim_swap_zkbin).unwrap(),
            &claim_swap_zkbin,
        );
        let refund_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&refund_swap_zkbin).unwrap(),
            &refund_swap_zkbin,
        );

        let create_swap_pk = ProvingKey::build(create_swap_zkbin.k, &create_circuit);
        let claim_swap_pk = ProvingKey::build(claim_swap_zkbin.k, &claim_circuit);
        let refund_swap_pk = ProvingKey::build(refund_swap_zkbin.k, &refund_circuit);

        Self {
            create_swap_zkbin,
            create_swap_pk,
            claim_swap_zkbin,
            claim_swap_pk,
            refund_swap_zkbin,
            refund_swap_pk,
        }
    }
}

impl super::ContractHarness for AtomicSwapHarness {
    fn name(&self) -> &str {
        "atomic_swap"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec!["CreateSwap", "ClaimSwap", "RefundSwap"]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "CreateSwap" => Some(&self.create_swap_zkbin),
            "ClaimSwap" => Some(&self.claim_swap_zkbin),
            "RefundSwap" => Some(&self.refund_swap_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "CreateSwap" => Some(&self.create_swap_pk),
            "ClaimSwap" => Some(&self.claim_swap_pk),
            "RefundSwap" => Some(&self.refund_swap_pk),
            _ => None,
        }
    }
}
