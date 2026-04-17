/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * This program is free software; you can redistribute it and/or
 * it under the terms of the GNU General Public License as published by the
 * Free Software Foundation; either version 3 of the License, or at your
 * option) any later version.
 *
 * This program is distributed in the hope that it will be useful, WITHOUT
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
    Result,
};
use darkfi_sdk::{
    crypto::pasta_prelude::*,
    pasta::pallas,
};

use darkfi_dao_escrow_contract::client::{
    init_v1::{init_v1_proof, InitV1CallData, InitV1PublicInputs},
    pay_premium_v1::{pay_premium_v1_proof, PayPremiumV1CallData, PayPremiumV1PublicInputs},
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

    /// Initialize a new DAO-Escrow
    pub fn initialize(
        &self,
        nullifier_k: pallas::Scalar,
        dao_bulla: pallas::Base,
        owner_secret: pallas::Base,
        endowment_token_id: pallas::Base,
        bulla_blind: pallas::Base,
    ) -> Result<(InitV1PublicInputs, Vec<darkfi::zk::Proof>)> {
        let input = InitV1CallData::new(
            nullifier_k,
            dao_bulla,
            owner_secret,
            endowment_token_id,
            bulla_blind,
        );
        let (proof, public_inputs) = init_v1_proof(&self.init_zkbin, &self.init_pk, &input)?;
        Ok((public_inputs, vec![proof]))
    }

    /// Pay premium to join DAO-Escrow as member
    #[allow(clippy::too_many_arguments)]
    pub fn pay_premium(
        &self,
        nullifier_k: pallas::Scalar,
        dao_escrow_bulla: pallas::Base,
        current_block: u64,
        member_secret: pallas::Base,
        value: u64,
        token_id: pallas::Base,
        expiry: u64,
        membership_blind: pallas::Base,
        value_blind: pallas::Scalar,
        mpc_secret_1: pallas::Scalar,
        mpc_secret_2: pallas::Scalar,
        mpc_secret_3: pallas::Scalar,
        max_membership_blocks: u64,
        max_expiry: u64,
    ) -> Result<(PayPremiumV1PublicInputs, Vec<darkfi::zk::Proof>)> {
        let input = PayPremiumV1CallData::new(
            nullifier_k,
            dao_escrow_bulla,
            current_block,
            member_secret,
            value,
            token_id,
            expiry,
            membership_blind,
            value_blind,
            mpc_secret_1,
            mpc_secret_2,
            mpc_secret_3,
            max_membership_blocks,
            max_expiry,
        );
        let (proof, public_inputs) =
            pay_premium_v1_proof(&self.pay_premium_zkbin, &self.pay_premium_pk, &input)?;
        Ok((public_inputs, vec![proof]))
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