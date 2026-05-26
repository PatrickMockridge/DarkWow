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

//! Subscription Test Harness
//!
//! Provides isolated testing for Subscription contract.

use dwow_core::{
    zk::{ProvingKey, ZkCircuit},
    zkas::ZkBinary,
};
use dwow_sdk::{
    crypto::{pasta_prelude::*, MerkleNode, PublicKey},
    pasta::pallas,
};
use dwow_serial::Encodable;

use dwow_subscription_contract::client::{
    subscribe_v1::{
        SubscribeCallData, SubscribePublicInputs, create_subscribe_proof,
    },
    update_usage_v1::{
        UpdateUsageCallData, UpdateUsagePublicInputs, create_update_usage_proof,
    },
    verify_access_v1::{
        VerifyAccessCallData, VerifyAccessPublicInputs, create_verify_access_proof,
    },
};
use dwow_subscription_contract::model::{
    SubscribeParamsV1, UpdateUsageParamsV1, VerifyAccessParamsV1,
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
        let update_bin = include_bytes!("../../../subscription/proof/update_usage_v1.zk.bin");

        let subscribe_zkbin = ZkBinary::decode(subscribe_bin, false).unwrap();
        let verify_access_zkbin = ZkBinary::decode(verify_bin, false).unwrap();
        let update_usage_zkbin = ZkBinary::decode(update_bin, false).unwrap();

        let subscribe_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&subscribe_zkbin).unwrap(),
            &subscribe_zkbin,
        );
        let verify_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&verify_access_zkbin).unwrap(),
            &verify_access_zkbin,
        );
        let update_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&update_usage_zkbin).unwrap(),
            &update_usage_zkbin,
        );

        let subscribe_pk = ProvingKey::build(subscribe_zkbin.k, &subscribe_circuit);
        let verify_access_pk = ProvingKey::build(verify_access_zkbin.k, &verify_circuit);
        let update_usage_pk = ProvingKey::build(update_usage_zkbin.k, &update_circuit);

        Self {
            subscribe_zkbin,
            subscribe_pk,
            verify_access_zkbin,
            verify_access_pk,
            update_usage_zkbin,
            update_usage_pk,
        }
    }

    /// Subscribe to a plan (function code 0x01)
    #[allow(clippy::too_many_arguments)]
    pub fn subscribe(
        &self,
        subscriber_secret: pallas::Base,
        nonce: pallas::Base,
        plan_merkle_proof: Vec<MerkleNode>,
        value_blind: pallas::Scalar,
        dao_member_pub_x: pallas::Base,
        dao_member_pub_y: pallas::Base,
        dao_membership_expiry: u64,
        dao_membership_value: pallas::Base,
        dao_leaf_pos: u32,
        dao_path: Vec<MerkleNode>,
        plan_leaf_pos: u32,
        plan_path: Vec<MerkleNode>,
        subscription_id: pallas::Base,
        subscriber_public: PublicKey,
        plan_id: u32,
        deposit: u64,
        token_id: pallas::Base,
        lock_until_block: u64,
        plan_merkle_root: pallas::Base,
        current_block: u64,
        value_commit_x: pallas::Base,
        value_commit_y: pallas::Base,
        dao_escrow_bulla: pallas::Base,
        dao_membership_note: pallas::Base,
        dao_escrow_merkle_root: pallas::Base,
    ) -> Result<SubscribeResult, Box<dyn std::error::Error>> {
        let merkle_proof_values: Vec<pallas::Base> =
            plan_merkle_proof.iter().map(|n| n.inner()).collect();
        let dao_proof_values: Vec<pallas::Base> =
            dao_path.iter().map(|n| n.inner()).collect();

        let input = SubscribeCallData::new(
            subscriber_secret,
            nonce,
            plan_merkle_proof,
            value_blind,
            dao_member_pub_x,
            dao_member_pub_y,
            dao_membership_expiry,
            dao_membership_value,
            dao_leaf_pos,
            dao_path,
            plan_leaf_pos,
            plan_path,
            subscription_id,
            subscriber_public,
            plan_id,
            deposit,
            token_id,
            lock_until_block,
            plan_merkle_root,
            current_block,
            value_commit_x,
            value_commit_y,
            dao_escrow_bulla,
            dao_membership_note,
            dao_escrow_merkle_root,
        );

        let (proof, public_inputs) = create_subscribe_proof(
            &self.subscribe_zkbin,
            &self.subscribe_pk,
            &input,
        )?;

        let params = SubscribeParamsV1 {
            plan_id: public_inputs.plan_id,
            subscriber_pubkey: subscriber_public,
            commitment: public_inputs.subscription_id,
            value_commit: pallas::Point::identity(),
            merkle_proof: merkle_proof_values,
            merkle_root: public_inputs.plan_merkle_root,
            dao_escrow_bulla: Some(public_inputs.dao_escrow_bulla),
            dao_membership_note: Some(public_inputs.dao_membership_note),
            dao_escrow_merkle_root: Some(public_inputs.dao_escrow_merkle_root),
            dao_merkle_proof: Some(dao_proof_values),
            dao_leaf_pos: Some(dao_leaf_pos),
        };

        let mut call_data = vec![0x01];
        params.encode(&mut call_data)?;

        Ok(SubscribeResult { call_data, proof, public_inputs })
    }

    /// Verify access to a subscription (function code 0x04)
    #[allow(clippy::too_many_arguments)]
    pub fn verify_access(
        &self,
        subscriber_secret: pallas::Base,
        nonce: pallas::Base,
        permissions_claimed: u8,
        subscription_leaf_pos: u32,
        subscription_path: Vec<MerkleNode>,
        subscription_state: pallas::Base,
        subscription_spent_nullifier: pallas::Base,
        expected_capability: pallas::Base,
        subscription_id: pallas::Base,
        current_block: u64,
        subscriber_pub_x: pallas::Base,
        subscriber_pub_y: pallas::Base,
        plan_id: u32,
        lock_until_block: u64,
        uses_allowed: u64,
        rate_period: u64,
        period_uses: u64,
        last_access_block: u64,
        uses_remaining: u64,
        subscription_state_root: pallas::Base,
    ) -> Result<VerifyAccessResult, Box<dyn std::error::Error>> {
        let input = VerifyAccessCallData::new(
            subscriber_secret,
            nonce,
            permissions_claimed,
            subscription_leaf_pos,
            subscription_path,
            subscription_state,
            subscription_spent_nullifier,
            expected_capability,
            subscription_id,
            current_block,
            subscriber_pub_x,
            subscriber_pub_y,
            plan_id,
            lock_until_block,
            uses_allowed,
            rate_period,
            period_uses,
            last_access_block,
            uses_remaining,
            subscription_state_root,
        );

        let (proof, public_inputs) = create_verify_access_proof(
            &self.verify_access_zkbin,
            &self.verify_access_pk,
            &input,
        )?;

        let params = VerifyAccessParamsV1 {
            subscription_id: public_inputs.subscription_id,
            capability: public_inputs.expected_capability,
            nonce,
        };

        let mut call_data = vec![0x04];
        params.encode(&mut call_data)?;

        Ok(VerifyAccessResult { call_data, proof, public_inputs })
    }

    /// Update usage tracking for a subscription (function code 0x06)
    pub fn update_usage(
        &self,
        subscription_id: pallas::Base,
        subscriber_pub_x: pallas::Base,
        subscriber_pub_y: pallas::Base,
        usage_timestamp: pallas::Base,
        nonce: pallas::Base,
        subscriber_secret: pallas::Base,
        current_block: u64,
        spent_nullifier: pallas::Base,
        merkle_proof: Vec<pallas::Base>,
    ) -> Result<UpdateUsageResult, Box<dyn std::error::Error>> {
        let input = UpdateUsageCallData::new(
            subscription_id,
            subscriber_pub_x,
            subscriber_pub_y,
            usage_timestamp,
            nonce,
        );

        let (proof, public_inputs) = create_update_usage_proof(
            &self.update_usage_zkbin,
            &self.update_usage_pk,
            &input,
        )?;

        let params = UpdateUsageParamsV1 {
            subscription_id,
            subscriber_pub_x,
            subscriber_pub_y,
            subscriber_secret,
            current_block,
            nonce,
            spent_nullifier,
            merkle_proof,
        };

        let mut call_data = vec![0x06];
        params.encode(&mut call_data)?;

        Ok(UpdateUsageResult { call_data, proof, public_inputs })
    }
}

impl super::ContractHarness for SubscriptionHarness {
    fn name(&self) -> &str {
        "subscription"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec!["SubscribeV1", "VerifyAccessV1", "UpdateUsageV1"]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "SubscribeV1" => Some(&self.subscribe_zkbin),
            "VerifyAccessV1" => Some(&self.verify_access_zkbin),
            "UpdateUsageV1" => Some(&self.update_usage_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "SubscribeV1" => Some(&self.subscribe_pk),
            "VerifyAccessV1" => Some(&self.verify_access_pk),
            "UpdateUsageV1" => Some(&self.update_usage_pk),
            _ => None,
        }
    }
}

/// Result of subscribe
pub struct SubscribeResult {
    pub call_data: Vec<u8>,
    pub proof: dwow_core::zk::Proof,
    pub public_inputs: SubscribePublicInputs,
}

/// Result of verify_access
pub struct VerifyAccessResult {
    pub call_data: Vec<u8>,
    pub proof: dwow_core::zk::Proof,
    pub public_inputs: VerifyAccessPublicInputs,
}

/// Result of update_usage
pub struct UpdateUsageResult {
    pub call_data: Vec<u8>,
    pub proof: dwow_core::zk::Proof,
    pub public_inputs: UpdateUsagePublicInputs,
}
