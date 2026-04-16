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

//! Escrow Test Harness
//!
//! Provides isolated testing for Escrow contract.

use darkfi::{
    zk::{ProvingKey, ZkCircuit},
    zkas::ZkBinary,
};
use darkfi_sdk::pasta::pallas;

use darkfi_escrow_contract::client::{
    claim_v1::{ClaimEscrowCallData, create_claim_escrow_proof},
    create_escrow_v1::{CreateEscrowCallData, create_escrow_proof},
    refund_v1::{RefundEscrowCallData, create_refund_escrow_proof},
};

/// Escrow Harness for isolated testing
pub struct EscrowHarness {
    /// CreateEscrow_V1 ZkBinary
    create_escrow_zkbin: ZkBinary,
    /// CreateEscrow_V1 ProvingKey
    create_escrow_pk: ProvingKey,
    /// ClaimEscrow_V1 ZkBinary
    claim_escrow_zkbin: ZkBinary,
    /// ClaimEscrow_V1 ProvingKey
    claim_escrow_pk: ProvingKey,
    /// RefundEscrow_V1 ZkBinary
    refund_escrow_zkbin: ZkBinary,
    /// RefundEscrow_V1 ProvingKey
    refund_escrow_pk: ProvingKey,
}

impl EscrowHarness {
    /// Spawn a new Escrow harness with pre-loaded circuits
    pub fn spawn() -> Self {
        let create_bin = include_bytes!("../../../escrow/proof/create_escrow_v1.zk.bin");
        let claim_bin = include_bytes!("../../../escrow/proof/claim_v1.zk.bin");
        let refund_bin = include_bytes!("../../../escrow/proof/refund_v1.zk.bin");

        let create_escrow_zkbin = ZkBinary::decode(create_bin, false).unwrap();
        let claim_escrow_zkbin = ZkBinary::decode(claim_bin, false).unwrap();
        let refund_escrow_zkbin = ZkBinary::decode(refund_bin, false).unwrap();

        let create_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&create_escrow_zkbin).unwrap(),
            &create_escrow_zkbin,
        );
        let claim_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&claim_escrow_zkbin).unwrap(),
            &claim_escrow_zkbin,
        );
        let refund_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&refund_escrow_zkbin).unwrap(),
            &refund_escrow_zkbin,
        );

        let create_escrow_pk = ProvingKey::build(create_escrow_zkbin.k, &create_circuit);
        let claim_escrow_pk = ProvingKey::build(claim_escrow_zkbin.k, &claim_circuit);
        let refund_escrow_pk = ProvingKey::build(refund_escrow_zkbin.k, &refund_circuit);

        Self {
            create_escrow_zkbin,
            create_escrow_pk,
            claim_escrow_zkbin,
            claim_escrow_pk,
            refund_escrow_zkbin,
            refund_escrow_pk,
        }
    }
}

impl super::ContractHarness for EscrowHarness {
    fn name(&self) -> &str {
        "escrow"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec!["CreateEscrowV1", "ClaimEscrowV1", "RefundEscrowV1"]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "CreateEscrowV1" => Some(&self.create_escrow_zkbin),
            "ClaimEscrowV1" => Some(&self.claim_escrow_zkbin),
            "RefundEscrowV1" => Some(&self.refund_escrow_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "CreateEscrowV1" => Some(&self.create_escrow_pk),
            "ClaimEscrowV1" => Some(&self.claim_escrow_pk),
            "RefundEscrowV1" => Some(&self.refund_escrow_pk),
            _ => None,
        }
    }
}