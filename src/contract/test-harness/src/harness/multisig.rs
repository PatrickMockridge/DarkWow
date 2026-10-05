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

//! MultiSig Test Harness

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    crypto::{pasta_prelude::PrimeField, poseidon_hash, Nullifier},
    pasta::pallas,
};
use dwow_serial::Encodable;
use rand::rngs::OsRng;
use rand::SeedableRng;

pub struct MultiSigHarness {
    create_group_zkbin: ZkBinary, create_group_pk: ProvingKey,
    finalize_zkbin: ZkBinary, finalize_pk: ProvingKey,
    sign_zkbin: ZkBinary, sign_pk: ProvingKey,
}

impl MultiSigHarness {
    pub fn spawn() -> Self {
        dwow_multisig_contract::enable_deterministic_zk();
        let cg = include_bytes!("../../../multisig/proof/create_group.zk.bin");
        let fi = include_bytes!("../../../multisig/proof/finalize.zk.bin");
        let si = include_bytes!("../../../multisig/proof/sign.zk.bin");
        let cg_zk = ZkBinary::decode(cg, false).unwrap();
        let fi_zk = ZkBinary::decode(fi, false).unwrap();
        let si_zk = ZkBinary::decode(si, false).unwrap();
        let cg_pk = ProvingKey::build(cg_zk.k, &ZkCircuit::new(dwow_core::zk::empty_witnesses(&cg_zk).unwrap(), &cg_zk)).expect("pk");
        let fi_pk = ProvingKey::build(fi_zk.k, &ZkCircuit::new(dwow_core::zk::empty_witnesses(&fi_zk).unwrap(), &fi_zk)).expect("pk");
        let si_pk = ProvingKey::build(si_zk.k, &ZkCircuit::new(dwow_core::zk::empty_witnesses(&si_zk).unwrap(), &si_zk)).expect("pk");
        Self { create_group_zkbin: cg_zk, create_group_pk: cg_pk, finalize_zkbin: fi_zk, finalize_pk: fi_pk, sign_zkbin: si_zk, sign_pk: si_pk }
    }

    /// The member commitment a secret opens: `H(DOMAIN_MEMBER_COMMITMENT, secret)` with
    /// `DOMAIN_MEMBER_COMMITMENT = witness_base(4) = 4`.
    pub fn member_commitment(member_secret: pallas::Base) -> pallas::Base {
        poseidon_hash([pallas::Base::from(4u64), member_secret])
    }

    /// The group id a set of member commitments derives, so a caller can address the group before
    /// creating it. Delegates to the contract's own derivation.
    pub fn group_id(threshold: u8, member_commitments: &[pallas::Base]) -> pallas::Base {
        dwow_multisig_contract::model::MultiSigGroup::derive_group_id(
            member_commitments[0], threshold, member_commitments.len() as u8,
        ).inner()
    }

    /// A member's signature nullifier for a message:
    /// `H(DOMAIN_NULLIFIER, secret, group_id, message_hash)` with `DOMAIN_NULLIFIER = witness_base(1) = 1`.
    pub fn signer_nullifier(
        member_secret: pallas::Base,
        group_id: pallas::Base,
        message_hash: pallas::Base,
    ) -> Nullifier {
        let nf = poseidon_hash([pallas::Base::from(1u64), member_secret, group_id, message_hash]);
        Nullifier::from_bytes(nf.to_repr()).expect("poseidon output is a valid non-zero nullifier")
    }

    pub fn create_group(
        &self,
        threshold: u8,
        member_commitments: Vec<pallas::Base>,
    ) -> Result<CreateGroupResult> {
        let t = pallas::Base::from(threshold as u64);
        let n = pallas::Base::from(member_commitments.len() as u64);
        // The same derivation the contract stores, called rather than re-implemented so the two
        // cannot drift.
        let group_id = dwow_multisig_contract::model::MultiSigGroup::derive_group_id(
            member_commitments[0], threshold, member_commitments.len() as u8,
        ).inner();
        let tx_nonce = pallas::Base::from(300u64);

        // `OBL-C198`, and the order is the whole point. The params are built and encoded FIRST,
        // with an **empty** proof: the proof travels in the transaction's proof vector, which
        // the commitment excludes, and a copy inside the call data would be computed from a
        // value that covers it. Then the commitment is derived over the finished call, and only
        // then is the proof made over witnesses bound to that commitment.
        let params = dwow_multisig_contract::model::CreateGroupParamsV1 {
            member_commitments, threshold,
            proof: vec![], tx_nonce,
        };
        let mut call_data = vec![0x01u8];
        call_data.extend_from_slice(&params.encode()?);
        let call = dwow_sdk::tx::ContractCall { contract_id: *dwow_sdk::crypto::MULTISIG_CONTRACT_ID, data: call_data.clone() };
        let tx_commitment: pallas::Base = dwow_sdk::crypto::util::tx_commitment([&call]);
        let tx_binding = poseidon_hash([pallas::Base::from(3u64), tx_commitment, tx_nonce]);

        let witnesses = vec![
            Witness::Base(Value::known(group_id)), Witness::Base(Value::known(t)),
            Witness::Base(Value::known(n)), Witness::Base(Value::known(tx_commitment)),
            Witness::Base(Value::known(tx_nonce)),
        ];
        // constrain_instance order: group_id, threshold, total_keys, tx_binding, tx_nonce —
        // the pair is last, which is the convention the node reads.
        let public_inputs = vec![group_id, t, n, tx_binding, tx_nonce];

        let proof = if dwow_multisig_contract::deterministic_zk_enabled() {
            Proof::create(&self.create_group_pk, &[ZkCircuit::new(witnesses, &self.create_group_zkbin)], &public_inputs, rand::rngs::StdRng::seed_from_u64(0))
        } else {
            Proof::create(&self.create_group_pk, &[ZkCircuit::new(witnesses, &self.create_group_zkbin)], &public_inputs, OsRng)
        }.map_err(|e| dwow_core::Error::Custom(format!("Proof::create: {:?}", e)))?;

        Ok(CreateGroupResult { call_data, proof, group_id })
    }

    pub fn sign(&self, group_id: pallas::Base, message_hash: pallas::Base, signer_secret: pallas::Base) -> Result<SignResult> {
        let member_commitment = Self::member_commitment(signer_secret);
        let nullifier = Self::signer_nullifier(signer_secret, group_id, message_hash);
        let tx_nonce = pallas::Base::from(300u64);

        // Call first, commitment second, proof third — see `create_group` in this file for why
        // the reverse order cannot bind to a real transaction (`OBL-C198`).
        let params = dwow_multisig_contract::model::SignParamsV1 {
            group_id: dwow_multisig_contract::model::GroupId(group_id),
            message_hash,
            member_commitment,
            nullifier: nullifier.inner(),
            proof: vec![], tx_nonce,
        };
        let mut call_data = vec![0x02u8];
        call_data.extend_from_slice(&params.encode()?);
        let call = dwow_sdk::tx::ContractCall { contract_id: *dwow_sdk::crypto::MULTISIG_CONTRACT_ID, data: call_data.clone() };
        let tx_commitment: pallas::Base = dwow_sdk::crypto::util::tx_commitment([&call]);
        let tx_binding = poseidon_hash([pallas::Base::from(3u64), tx_commitment, tx_nonce]);

        let witnesses = vec![
            Witness::Base(Value::known(group_id)), Witness::Base(Value::known(message_hash)),
            Witness::Base(Value::known(signer_secret)),
            Witness::Base(Value::known(tx_commitment)),
            Witness::Base(Value::known(tx_nonce)),
        ];
        // constrain_instance order: group_id, message_hash, member_commitment, nullifier,
        // tx_binding, tx_nonce
        let public_inputs = vec![
            group_id, message_hash, member_commitment, nullifier.inner(),
            tx_binding, tx_nonce,
        ];

        let proof = if dwow_multisig_contract::deterministic_zk_enabled() {
            Proof::create(&self.sign_pk, &[ZkCircuit::new(witnesses, &self.sign_zkbin)], &public_inputs, rand::rngs::StdRng::seed_from_u64(0))
        } else {
            Proof::create(&self.sign_pk, &[ZkCircuit::new(witnesses, &self.sign_zkbin)], &public_inputs, OsRng)
        }.map_err(|e| dwow_core::Error::Custom(format!("Proof::create: {:?}", e)))?;

        Ok(SignResult { call_data, proof, nullifier })
    }

    /// Build a `FinalizeV1` call over the approvals the caller names. The contract checks each one
    /// against the signature records it can see; see `entrypoint/mod.rs`.
    pub fn finalize(
        &self,
        group_id: pallas::Base,
        message_hash: pallas::Base,
        approvals: Vec<Nullifier>,
    ) -> Result<FinalizeResult> {
        // Circuit: DOMAIN_COMMITMENT = witness_base(4) = 4
        let approval_commit = poseidon_hash([pallas::Base::from(4u64), group_id, message_hash]);
        let tx_nonce = pallas::Base::from(300u64);

        // Call first, commitment second, proof third — see `create_group` in this file
        // (`OBL-C198`).
        let params = dwow_multisig_contract::model::FinalizeParamsV1 {
            group_id: dwow_multisig_contract::model::GroupId(group_id),
            message_hash, approval_commit, approvals,
            proof: vec![], tx_nonce,
        };
        let mut call_data = vec![0x03u8];
        call_data.extend_from_slice(&params.encode()?);
        let call = dwow_sdk::tx::ContractCall { contract_id: *dwow_sdk::crypto::MULTISIG_CONTRACT_ID, data: call_data.clone() };
        let tx_commitment: pallas::Base = dwow_sdk::crypto::util::tx_commitment([&call]);
        let tx_binding = poseidon_hash([pallas::Base::from(3u64), tx_commitment, tx_nonce]);

        let witnesses = vec![
            Witness::Base(Value::known(group_id)), Witness::Base(Value::known(message_hash)),
            Witness::Base(Value::known(approval_commit)), Witness::Base(Value::known(tx_commitment)),
            Witness::Base(Value::known(tx_nonce)),
        ];
        // constrain_instance order: group_id, message_hash, approval_commit, tx_binding, tx_nonce
        let public_inputs = vec![group_id, message_hash, approval_commit, tx_binding, tx_nonce];

        let proof = if dwow_multisig_contract::deterministic_zk_enabled() {
            Proof::create(&self.finalize_pk, &[ZkCircuit::new(witnesses, &self.finalize_zkbin)], &public_inputs, rand::rngs::StdRng::seed_from_u64(0))
        } else {
            Proof::create(&self.finalize_pk, &[ZkCircuit::new(witnesses, &self.finalize_zkbin)], &public_inputs, OsRng)
        }.map_err(|e| dwow_core::Error::Custom(format!("Proof::create: {:?}", e)))?;

        Ok(FinalizeResult { call_data, proof })
    }

    /// A `finalize` call built but not yet proven (`OBL-C198`).
    ///
    /// [`Self::finalize`] binds its proof to `tx_commitment([&call])` — the call **alone** — which
    /// is correct when the multisig call is the whole transaction, as it is in this contract's own
    /// spec. When it is a **child** of another call the node hashes the whole ordered set instead,
    /// so the child's `tx_binding` has to be derived over a call that does not exist yet when the
    /// child is built: the parent's. `finalize_prepare` therefore stops before the proof and hands
    /// the caller the call data; the caller assembles the set, takes the commitment and calls
    /// [`FinalizePlan::prove`].
    ///
    /// This is additive on purpose. Rewriting `finalize` to take a child set would change every
    /// caller in `multisig_spec.rs`, where the bind-over-self form is already right.
    pub fn finalize_prepare(
        &self,
        group_id: pallas::Base,
        message_hash: pallas::Base,
        approvals: Vec<Nullifier>,
    ) -> Result<FinalizePlan> {
        let approval_commit = poseidon_hash([pallas::Base::from(4u64), group_id, message_hash]);
        let tx_nonce = pallas::Base::from(300u64);

        let params = dwow_multisig_contract::model::FinalizeParamsV1 {
            group_id: dwow_multisig_contract::model::GroupId(group_id),
            message_hash, approval_commit, approvals,
            proof: vec![], tx_nonce,
        };
        let mut call_data = vec![0x03u8];
        call_data.extend_from_slice(&params.encode()?);

        Ok(FinalizePlan {
            witnesses_head: vec![
                Witness::Base(Value::known(group_id)),
                Witness::Base(Value::known(message_hash)),
                Witness::Base(Value::known(approval_commit)),
            ],
            public_head: vec![group_id, message_hash, approval_commit],
            tx_nonce,
            call_data,
            finalize_zkbin: self.finalize_zkbin.clone(),
            finalize_pk: self.finalize_pk.clone(),
        })
    }
}

/// A prepared multisig `finalize`, proved against a caller-supplied commitment — see
/// [`MultiSigHarness::finalize_prepare`] for why a child needs this and a whole transaction does
/// not.
pub struct FinalizePlan {
    witnesses_head: Vec<Witness>,
    public_head: Vec<pallas::Base>,
    tx_nonce: pallas::Base,
    /// The call data the commitment must cover. The caller needs it to build the `ContractCall`
    /// this plan signs for.
    pub call_data: Vec<u8>,
    finalize_zkbin: ZkBinary,
    finalize_pk: ProvingKey,
}

impl FinalizePlan {
    /// Prove against `tx_commitment` — the commitment over the whole ordered call set the node
    /// will hash, not just this call.
    pub fn prove(self, tx_commitment: pallas::Base) -> Result<FinalizeResult> {
        let tx_binding = poseidon_hash([pallas::Base::from(3u64), tx_commitment, self.tx_nonce]);

        let mut witnesses = self.witnesses_head;
        witnesses.push(Witness::Base(Value::known(tx_commitment)));
        witnesses.push(Witness::Base(Value::known(self.tx_nonce)));

        // constrain_instance order: group_id, message_hash, approval_commit, tx_binding, tx_nonce
        let mut public_inputs = self.public_head;
        public_inputs.push(tx_binding);
        public_inputs.push(self.tx_nonce);

        let proof = if dwow_multisig_contract::deterministic_zk_enabled() {
            Proof::create(&self.finalize_pk, &[ZkCircuit::new(witnesses, &self.finalize_zkbin)], &public_inputs, rand::rngs::StdRng::seed_from_u64(0))
        } else {
            Proof::create(&self.finalize_pk, &[ZkCircuit::new(witnesses, &self.finalize_zkbin)], &public_inputs, OsRng)
        }.map_err(|e| dwow_core::Error::Custom(format!("Proof::create: {:?}", e)))?;

        Ok(FinalizeResult { call_data: self.call_data, proof })
    }
}

impl super::ContractHarness for MultiSigHarness {
    fn name(&self) -> &str { "multisig" }
    fn circuits(&self) -> Vec<&'static str> { vec!["CreateGroupV2", "FinalizeV2", "SignV2"] }
    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns { "CreateGroupV2" => Some(&self.create_group_zkbin), "FinalizeV2" => Some(&self.finalize_zkbin), "SignV2" => Some(&self.sign_zkbin), _ => None }
    }
    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns { "CreateGroupV2" => Some(&self.create_group_pk), "FinalizeV2" => Some(&self.finalize_pk), "SignV2" => Some(&self.sign_pk), _ => None }
    }
}

pub struct CreateGroupResult { pub call_data: Vec<u8>, pub proof: dwow_core::zk::Proof, pub group_id: pallas::Base }
pub struct SignResult { pub call_data: Vec<u8>, pub proof: dwow_core::zk::Proof, pub nullifier: Nullifier }
pub struct FinalizeResult { pub call_data: Vec<u8>, pub proof: dwow_core::zk::Proof }
