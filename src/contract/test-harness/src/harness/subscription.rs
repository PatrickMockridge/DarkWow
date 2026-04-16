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

//! Subscription Test Harness
//!
//! Provides isolated testing for Subscription contract.

use darkfi::{
    zk::{ProvingKey, ZkCircuit},
    zkas::ZkBinary,
};
use darkfi_sdk::pasta::pallas;

use subscription_contract::client::{
    rate_limit_v1::{RateLimitCallData, create_rate_limit_proof},
    subscribe_v1::{SubscribeCallData, create_subscribe_proof},
    verify_access_v1::{VerifyAccessCallData, create_verify_access_proof},
};

/// Subscription Harness for isolated testing
pub struct SubscriptionHarness {
    /// Subscribe_V1 ZkBinary
    subscribe_zkbin: ZkBinary,
    /// Subscribe_V1 ProvingKey
    subscribe_pk: ProvingKey,
    /// VerifyAccess_V1 ZkBinary
    verify_access_zkbin: ZkBinary,
    /// VerifyAccess_V1 ProvingKey
    verify_access_pk: ProvingKey,
    /// RateLimit_V1 ZkBinary
    rate_limit_zkbin: ZkBinary,
    /// RateLimit_V1 ProvingKey
    rate_limit_pk: ProvingKey,
    /// UpdateUsage_V1 ZkBinary
    update_usage_zkbin: ZkBinary,
    /// UpdateUsage_V1 ProvingKey
    update_usage_pk: ProvingKey,
}

impl SubscriptionHarness {
    /// Spawn a new Subscription harness with pre-loaded circuits
    pub fn spawn() -> Self {
        let subscribe_bin = include_bytes!("../../../subscription/proof/subscribe_v1.zk.bin");
        let verify_bin = include_bytes!("../../../subscription/proof/verify_access_v1.zk.bin");
        let rate_bin = include_bytes!("../../../subscription/proof/rate_limit_v1.zk.bin");
        let update_bin = include_bytes!("../../../subscription/proof/update_usage_v1.zk.bin");

        let subscribe_zkbin = ZkBinary::decode(subscribe_bin, false).unwrap();
        let verify_access_zkbin = ZkBinary::decode(verify_bin, false).unwrap();
        let rate_limit_zkbin = ZkBinary::decode(rate_bin, false).unwrap();
        let update_usage_zkbin = ZkBinary::decode(update_bin, false).unwrap();

        let subscribe_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&subscribe_zkbin).unwrap(),
            &subscribe_zkbin,
        );
        let verify_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&verify_access_zkbin).unwrap(),
            &verify_access_zkbin,
        );
        let rate_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&rate_limit_zkbin).unwrap(),
            &rate_limit_zkbin,
        );
        let update_circuit = ZkCircuit::new(
            darkfi::zk::empty_witnesses(&update_usage_zkbin).unwrap(),
            &update_usage_zkbin,
        );

        let subscribe_pk = ProvingKey::build(subscribe_zkbin.k, &subscribe_circuit);
        let verify_access_pk = ProvingKey::build(verify_access_zkbin.k, &verify_circuit);
        let rate_limit_pk = ProvingKey::build(rate_limit_zkbin.k, &rate_circuit);
        let update_usage_pk = ProvingKey::build(update_usage_zkbin.k, &update_circuit);

        Self {
            subscribe_zkbin,
            subscribe_pk,
            verify_access_zkbin,
            verify_access_pk,
            rate_limit_zkbin,
            rate_limit_pk,
            update_usage_zkbin,
            update_usage_pk,
        }
    }
}

impl super::ContractHarness for SubscriptionHarness {
    fn name(&self) -> &str {
        "subscription"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec!["SubscribeV1", "VerifyAccessV1", "RateLimitV1", "UpdateUsageV1"]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "SubscribeV1" => Some(&self.subscribe_zkbin),
            "VerifyAccessV1" => Some(&self.verify_access_zkbin),
            "RateLimitV1" => Some(&self.rate_limit_zkbin),
            "UpdateUsageV1" => Some(&self.update_usage_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "SubscribeV1" => Some(&self.subscribe_pk),
            "VerifyAccessV1" => Some(&self.verify_access_pk),
            "RateLimitV1" => Some(&self.rate_limit_pk),
            "UpdateUsageV1" => Some(&self.update_usage_pk),
            _ => None,
        }
    }
}