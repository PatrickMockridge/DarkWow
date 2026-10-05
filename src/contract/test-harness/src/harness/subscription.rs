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
    zk::{Proof, ProvingKey, ZkCircuit},
    zkas::ZkBinary,
};
use dwow_sdk::{
    crypto::{pasta_prelude::*, ContractId, MerkleNode, PublicKey},
    pasta::pallas,
};
use dwow_serial::Encodable;

use dwow_subscription_contract::client::{
    cancel::{create_cancel_proof, CancelCallData},
    renew::{create_renew_proof, RenewCallData},
    subscribe::{
        SubscribeCallData, SubscribePublicInputs, create_subscribe_proof,
    },
    update_usage::{
        UpdateUsageCallData, UpdateUsagePublicInputs, create_update_usage_proof,
    },
    verify_access::{
        VerifyAccessCallData, VerifyAccessPublicInputs, create_verify_access_proof,
    },
    tx_binding_of,
};
use dwow_subscription_contract::model::{
    access_capability, CancelParamsV1, RenewParamsV1, SubscribeParamsV1, SubscriptionId,
    UpdateUsageParamsV1, VerifyAccessParamsV1,
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
    /// CancelV1 ZkBinary
    cancel_zkbin: ZkBinary,
    /// CancelV1 ProvingKey
    cancel_pk: ProvingKey,
    /// RenewV1 ZkBinary
    renew_zkbin: ZkBinary,
    /// RenewV1 ProvingKey
    renew_pk: ProvingKey,
    /// The deployed id of the subscription contract this harness builds calls for — the commitment
    /// covers the call *including* the contract id, so a harness that proves must be told it
    /// (`OBL-C198`).
    contract_id: ContractId,
}

/// The commitment a call set's proofs must bind to (`OBL-C198`) — over the **whole ordered set**
/// the node will hash, children before parents, each call serialized with its contract id. One
/// helper because every builder needs the same derivation and a second copy is a second value
/// (`safety.md` RC5).
fn commitment_of(calls: &[dwow_sdk::tx::ContractCall]) -> pallas::Base {
    dwow_sdk::crypto::util::tx_commitment(calls.iter())
}

impl SubscriptionHarness {
    /// The access capability a `verify_access` call must present for a subscription with these
    /// fields (`OBL-C84`) — the **model's** derivation, called rather than re-implemented, so a
    /// fixture cannot invent a value and then read its own rejection as a contract defect.
    ///
    /// The host recomputes this from the *stored record*, which is why a fixture's call only
    /// verifies if the capability it passes was derived from the record the `subscribe` before it
    /// wrote — the same plan, id, expiry and subscriber key.
    #[expect(clippy::too_many_arguments, reason = "the derivation's inputs, one per argument")]
    pub fn access_capability(
        subscriber_pub_x: pallas::Base,
        subscriber_pub_y: pallas::Base,
        plan_id: u32,
        subscription_id: pallas::Base,
        lock_until_block: u64,
        nonce: pallas::Base,
    ) -> pallas::Base {
        access_capability(
            subscriber_pub_x,
            subscriber_pub_y,
            plan_id,
            SubscriptionId(subscription_id),
            lock_until_block,
            nonce,
        )
    }

    /// Spawn a new Subscription harness with pre-loaded circuits, for the contract deployed at
    /// `contract_id` — see the field's note for why it cannot be defaulted.
    pub fn spawn(contract_id: ContractId) -> Self {
        let subscribe_bin = include_bytes!("../../../subscription/proof/subscribe.zk.bin");
        let verify_bin = include_bytes!("../../../subscription/proof/verify_access.zk.bin");
        let update_bin = include_bytes!("../../../subscription/proof/update_usage.zk.bin");

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

        let subscribe_pk = ProvingKey::build(subscribe_zkbin.k, &subscribe_circuit).expect("ProvingKey::build failed");
        let verify_access_pk = ProvingKey::build(verify_access_zkbin.k, &verify_circuit).expect("ProvingKey::build failed");
        let update_usage_pk = ProvingKey::build(update_usage_zkbin.k, &update_circuit).expect("ProvingKey::build failed");

        let cancel_bin = include_bytes!("../../../subscription/proof/cancel.zk.bin");
        let renew_bin = include_bytes!("../../../subscription/proof/renew.zk.bin");

        let cancel_zkbin = ZkBinary::decode(cancel_bin, false).unwrap();
        let renew_zkbin = ZkBinary::decode(renew_bin, false).unwrap();

        let cancel_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&cancel_zkbin).unwrap(),
            &cancel_zkbin,
        );
        let renew_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&renew_zkbin).unwrap(),
            &renew_zkbin,
        );

        let cancel_pk = ProvingKey::build(cancel_zkbin.k, &cancel_circuit).expect("ProvingKey::build failed");
        let renew_pk = ProvingKey::build(renew_zkbin.k, &renew_circuit).expect("ProvingKey::build failed");

        Self {
            subscribe_zkbin,
            subscribe_pk,
            verify_access_zkbin,
            verify_access_pk,
            update_usage_zkbin,
            update_usage_pk,
            cancel_zkbin,
            cancel_pk,
            renew_zkbin,
            renew_pk,
            contract_id,
        }
    }

    /// Subscribe to a plan (function code 0x01).
    ///
    /// `children` are the calls that precede this one in the transaction (DFS post-order) — this
    /// endpoint carries a promissory-note transfer in the spec — and the commitment the proof binds
    /// to is taken over them and this call together (`OBL-C198`).
    #[allow(clippy::too_many_arguments)]
    pub fn subscribe(
        &self,
        children: &[dwow_sdk::tx::ContractCall],
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
        asset_id: pallas::Base,
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

        let mut input = SubscribeCallData::new(
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
            asset_id,
            lock_until_block,
            plan_merkle_root,
            current_block,
            value_commit_x,
            value_commit_y,
            dao_escrow_bulla,
            dao_membership_note,
            dao_escrow_merkle_root,
        );

        // `OBL-C198`: the call data first, the commitment over the whole ordered set next, and the
        // proof last — the reverse order cannot bind to a real transaction. The public inputs the
        // params are built from are a pure function of the call data, so they need no proof.
        let public_inputs = input.compute_public_inputs();

        let params = SubscribeParamsV1 {
            plan_id: public_inputs.plan_id,
            subscriber_pubkey: subscriber_public,
            commitment: SubscriptionId(public_inputs.subscription_id),
            value_commit: pallas::Point::identity(),
            merkle_proof: merkle_proof_values,
            merkle_root: public_inputs.plan_merkle_root,
            dao_escrow_bulla: Some(public_inputs.dao_escrow_bulla),
            dao_membership_note: Some(public_inputs.dao_membership_note),
            dao_escrow_merkle_root: Some(public_inputs.dao_escrow_merkle_root),
            dao_merkle_proof: Some(dao_proof_values),
            dao_leaf_pos: Some(dao_leaf_pos),
            instance_seed: [0u8; 32],
            // `tx_binding` left the params in `OBL-C198`; the nonce stays, taken from the client's
            // own derivation rather than chosen here.
            tx_nonce: public_inputs.tx_nonce,
        };

        let mut call_data = vec![0x01];
        call_data.extend_from_slice(&params.encode().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?);

        // ONE commitment over the whole ordered call set — children first, this call last (DFS
        // post-order) — so this proof and its children's bind to the same value.
        let mut calls = children.to_vec();
        calls.push(dwow_sdk::tx::ContractCall { contract_id: self.contract_id, data: call_data.clone() });
        let commitment = commitment_of(&calls);
        input.tx_commitment = commitment;

        let (proof, _public_inputs) = create_subscribe_proof(
            &self.subscribe_zkbin,
            &self.subscribe_pk,
            &input,
        )?;

        Ok(SubscribeResult { call_data, proof, public_inputs, commitment })
    }

    /// Verify access to a subscription (function code 0x04) — `children` as `subscribe` above.
    #[allow(clippy::too_many_arguments)]
    pub fn verify_access(
        &self,
        children: &[dwow_sdk::tx::ContractCall],
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
        let mut input = VerifyAccessCallData::new(
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

        // `OBL-C198`: call data, then the commitment over the whole ordered set, then the proof.
        let public_inputs = input.compute_public_inputs();

        let params = VerifyAccessParamsV1 {
            subscription_id: SubscriptionId(public_inputs.subscription_id),
            capability: public_inputs.expected_capability,
            nonce,
            // `tx_binding` left the params in `OBL-C198`; the nonce stays.
            tx_nonce: public_inputs.tx_nonce,
        };

        let mut call_data = vec![0x04];
        call_data.extend_from_slice(&params.encode());

        let mut calls = children.to_vec();
        calls.push(dwow_sdk::tx::ContractCall { contract_id: self.contract_id, data: call_data.clone() });
        let commitment = commitment_of(&calls);
        input.tx_commitment = commitment;

        let (proof, _public_inputs) = create_verify_access_proof(
            &self.verify_access_zkbin,
            &self.verify_access_pk,
            &input,
        )?;

        Ok(VerifyAccessResult { call_data, proof, public_inputs, commitment })
    }

    /// Update usage tracking for a subscription (function code 0x06) — `children` as `subscribe`
    /// above.
    pub fn update_usage(
        &self,
        children: &[dwow_sdk::tx::ContractCall],
        subscription_id: pallas::Base,
        subscriber_pub_x: pallas::Base,
        subscriber_pub_y: pallas::Base,
        usage_timestamp: pallas::Base,
        nonce: pallas::Base,
        subscriber_secret: pallas::Base,
        current_block: u64,
        merkle_proof: Vec<pallas::Base>,
    ) -> Result<UpdateUsageResult, Box<dyn std::error::Error>> {
        let mut input = UpdateUsageCallData::new(
            subscription_id,
            subscriber_pub_x,
            subscriber_pub_y,
            usage_timestamp,
            nonce,
        );

        // `OBL-C198`: call data, then the commitment over the whole ordered set, then the proof.
        let public_inputs = input.compute_public_inputs();

        let params = UpdateUsageParamsV1 {
            subscription_id: SubscriptionId(subscription_id),
            subscriber_pub_x,
            subscriber_pub_y,
            subscriber_secret,
            current_block,
            nonce,
            // The derivation the host compares against — `model::nullifier_of`, called rather than
            // asserted by the caller. The fixture used to pass its own value and the two disagreed
            // silently until `OBL-C106` gave the derivation one home.
            spent_nullifier: dwow_subscription_contract::model::nullifier_of(
                SubscriptionId(subscription_id),
                subscriber_secret,
            ),
            merkle_proof,
            // `tx_binding` left the params in `OBL-C198`; the nonce stays.
            tx_nonce: public_inputs.tx_nonce,
        };

        let mut call_data = vec![0x06];
        call_data.extend_from_slice(&params.encode().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?);

        let mut calls = children.to_vec();
        calls.push(dwow_sdk::tx::ContractCall { contract_id: self.contract_id, data: call_data.clone() });
        let commitment = commitment_of(&calls);
        input.tx_commitment = commitment;

        let (proof, _public_inputs) = create_update_usage_proof(
            &self.update_usage_zkbin,
            &self.update_usage_pk,
            &input,
        )?;

        Ok(UpdateUsageResult { call_data, proof, public_inputs, commitment })
    }

    /// Cancel a subscription (function code 0x02)
    /// Cancel a subscription (function code 0x02).
    ///
    /// **The proof is real** as of 2026-09-24 (`OBL-C106`): it is built by the contract's own client
    /// (`create_cancel_proof`, which did not exist before) and the params carry the public inputs it
    /// was made with. The nullifier is no longer a parameter — the client derives it
    /// (`model::nullifier_of`), the circuit constrains it, and the host compares the published value
    /// against the same derivation, which is what makes the three agree. Before this the endpoint
    /// proved with `empty_witnesses`: a fabricated proof carrying no instances, refused by the L2
    /// verify for a reason the endpoint did not name.
    pub fn cancel(
        &self,
        children: &[dwow_sdk::tx::ContractCall],
        subscription_id: pallas::Base,
        subscriber_secret: pallas::Base,
        current_block: u64,
        recipient_pubkey: PublicKey,
    ) -> Result<CancelResult, Box<dyn std::error::Error>> {
        let mut input = CancelCallData::new(subscription_id, subscriber_secret, current_block, recipient_pubkey);
        // `OBL-C198`: the call data first, the commitment over the whole ordered set next, the proof
        // last. `public_inputs` is a pure function of the call data's own inputs.
        let public_inputs = input.compute_public_inputs();

        let params = CancelParamsV1 {
            subscription_id: SubscriptionId(subscription_id),
            subscriber_secret,
            spent_nullifier: public_inputs.spent_nullifier,
            current_block,
            recipient_pubkey,
            // `tx_binding` left the params in `OBL-C198`; the nonce stays.
            tx_nonce: public_inputs.tx_nonce,
        };

        let mut call_data = vec![0x02];
        call_data.extend_from_slice(&params.encode());

        let mut calls = children.to_vec();
        calls.push(dwow_sdk::tx::ContractCall { contract_id: self.contract_id, data: call_data.clone() });
        let commitment = commitment_of(&calls);
        input.tx_commitment = commitment;

        let (proof, _public_inputs) =
            create_cancel_proof(&self.cancel_zkbin, &self.cancel_pk, &input)?;

        Ok(CancelResult { call_data, proof, commitment })
    }

    /// Renew a subscription (function code 0x03)
    /// Renew a subscription (function code 0x03) — the same repair as `cancel` above (`OBL-C106`):
    /// a real proof from `create_renew_proof`, params carrying the instances it was made with, and a
    /// nullifier the client derives rather than the caller asserting.
    pub fn renew(
        &self,
        children: &[dwow_sdk::tx::ContractCall],
        subscription_id: pallas::Base,
        subscriber_secret: pallas::Base,
        new_lock_until_block: u64,
        value_commit: pallas::Point,
    ) -> Result<RenewResult, Box<dyn std::error::Error>> {
        let mut input = RenewCallData::new(subscription_id, subscriber_secret, new_lock_until_block, value_commit);
        input.merkle_proof = vec![];
        // `OBL-C198`: the call data first, the commitment over the whole ordered set next, the proof
        // last.
        let public_inputs = input.compute_public_inputs();

        let params = RenewParamsV1 {
            subscription_id: SubscriptionId(subscription_id),
            subscriber_secret,
            new_lock_until_block,
            spent_nullifier: public_inputs.spent_nullifier,
            value_commit,
            merkle_proof: input.merkle_proof.clone(),
            // `tx_binding` left the params in `OBL-C198`; the nonce stays.
            tx_nonce: public_inputs.tx_nonce,
        };

        let mut call_data = vec![0x03];
        call_data.extend_from_slice(&params.encode().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?);

        let mut calls = children.to_vec();
        calls.push(dwow_sdk::tx::ContractCall { contract_id: self.contract_id, data: call_data.clone() });
        let commitment = commitment_of(&calls);
        input.tx_commitment = commitment;

        let (proof, _public_inputs) =
            create_renew_proof(&self.renew_zkbin, &self.renew_pk, &input)?;

        Ok(RenewResult { call_data, proof, commitment })
    }
}

impl super::ContractHarness for SubscriptionHarness {
    fn name(&self) -> &str {
        "subscription"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec!["SubscribeV2", "VerifyAccessV2", "UpdateUsageV2", "CancelV2", "RenewV2"]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "SubscribeV2" => Some(&self.subscribe_zkbin),
            "VerifyAccessV2" => Some(&self.verify_access_zkbin),
            "UpdateUsageV2" => Some(&self.update_usage_zkbin),
            "CancelV2" => Some(&self.cancel_zkbin),
            "RenewV2" => Some(&self.renew_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "SubscribeV2" => Some(&self.subscribe_pk),
            "VerifyAccessV2" => Some(&self.verify_access_pk),
            "UpdateUsageV2" => Some(&self.update_usage_pk),
            "CancelV2" => Some(&self.cancel_pk),
            "RenewV2" => Some(&self.renew_pk),
            _ => None,
        }
    }
}

/// Result of subscribe
pub struct SubscribeResult {
    pub call_data: Vec<u8>,
    pub proof: dwow_core::zk::Proof,
    pub public_inputs: SubscribePublicInputs,
    /// The commitment every proof in this transaction binds to (`OBL-C198`), so the caller can
    /// prove a child against the same value.
    pub commitment: pallas::Base,
}

/// Result of verify_access
pub struct VerifyAccessResult {
    pub call_data: Vec<u8>,
    pub proof: dwow_core::zk::Proof,
    pub public_inputs: VerifyAccessPublicInputs,
    /// The commitment every proof in this transaction binds to (`OBL-C198`).
    pub commitment: pallas::Base,
}

/// Result of update_usage
pub struct UpdateUsageResult {
    pub call_data: Vec<u8>,
    pub proof: dwow_core::zk::Proof,
    pub public_inputs: UpdateUsagePublicInputs,
    /// The commitment every proof in this transaction binds to (`OBL-C198`).
    pub commitment: pallas::Base,
}

/// Result of cancel
pub struct CancelResult {
    pub call_data: Vec<u8>,
    pub proof: dwow_core::zk::Proof,
    /// The commitment every proof in this transaction binds to (`OBL-C198`).
    pub commitment: pallas::Base,
}

/// Result of renew
pub struct RenewResult {
    pub call_data: Vec<u8>,
    pub proof: dwow_core::zk::Proof,
    /// The commitment every proof in this transaction binds to (`OBL-C198`), so the caller can
    /// prove a child against the same value.
    pub commitment: pallas::Base,
}
