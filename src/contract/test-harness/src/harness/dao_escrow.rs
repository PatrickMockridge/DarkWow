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

//! DaoEscrow Test Harness
//!
//! Provides isolated testing for DaoEscrow contract.

use dwow_core::{
    zk::{ProvingKey, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    crypto::{pasta_prelude::*, PublicKey, SecretKey},
    pasta::pallas,
};
use dwow_serial::Encodable;

use dwow_dao_escrow_contract::client::{
    init::{init_v1_proof, InitV1CallData, InitV1PublicInputs},
    pay_premium::{pay_premium_v1_proof, PayPremiumV1CallData, PayPremiumV1PublicInputs},
    propose_claim::{propose_claim_v1_proof, ProposeClaimV1CallData, ProposeClaimV1PublicInputs},
    update::{update_v1_proof, UpdateV1CallData, UpdateV1PublicInputs},
    vote_claim::{vote_claim_v1_proof, VoteClaimV1CallData, VoteClaimV1PublicInputs},
};
use dwow_dao_escrow_contract::model::{
    CancelClaimParamsV1, CapabilityProof, ClaimId, DaoEscrowBulla, DaoEscrowMode,
    EndowmentWithdrawParamsV1, ExecuteClaimParamsV1, InitializeParamsV1, MembershipNote,
    PayPremiumParamsV1, ProposalId, ProposeClaimParamsV1, TreasurySpendParamsV1, UpdateParamsV1,
    VoteClaimParamsV1, WithdrawParamsV1, VoteType,
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
    /// ProposeClaim_V1 ZkBinary
    propose_claim_zkbin: ZkBinary,
    /// ProposeClaim_V1 ProvingKey
    propose_claim_pk: ProvingKey,
    /// VoteClaim_V1 ZkBinary
    vote_claim_zkbin: ZkBinary,
    /// VoteClaim_V1 ProvingKey
    vote_claim_pk: ProvingKey,
    /// SetGovernanceConfig_V1 ZkBinary
    set_governance_config_zkbin: ZkBinary,
    /// SetGovernanceConfig_V1 ProvingKey
    set_governance_config_pk: ProvingKey,
    /// The contract's deployed id (`OBL-C198`): the transaction commitment is derived over the
    /// call set, and a call carries this id, so the harness has to know it. Taken as a parameter
    /// rather than defaulted — a wrong id is a wrong commitment, and the proof is refused.
    contract_id: dwow_sdk::crypto::ContractId,
}

/// The transaction commitment over an ordered call set — the same derivation the node recomputes
/// (`dwow_sdk::crypto::util::tx_commitment`), so a proof and the transaction that carries it agree.
///
/// The order is the one `DarkForest::build_vec` emits (`TransactionBuilder::build`): DFS
/// post-order, children before parents.
fn commitment_of(calls: &[dwow_sdk::tx::ContractCall]) -> pallas::Base {
    dwow_sdk::crypto::util::tx_commitment(calls.iter())
}

impl DaoEscrowHarness {
    /// Spawn a new DaoEscrow harness with pre-loaded circuits
    pub fn spawn(contract_id: dwow_sdk::crypto::ContractId) -> Self {
        let init_bin = include_bytes!("../../../dao_escrow/proof/init.zk.bin");
        let pay_premium_bin = include_bytes!("../../../dao_escrow/proof/pay_premium.zk.bin");
        let propose_claim_bin = include_bytes!("../../../dao_escrow/proof/propose_claim.zk.bin");
        let vote_claim_bin = include_bytes!("../../../dao_escrow/proof/vote_claim.zk.bin");
        let set_governance_config_bin = include_bytes!("../../../dao_escrow/proof/set_governance_config.zk.bin");

        let init_zkbin = ZkBinary::decode(init_bin, false).unwrap();
        let pay_premium_zkbin = ZkBinary::decode(pay_premium_bin, false).unwrap();
        let propose_claim_zkbin = ZkBinary::decode(propose_claim_bin, false).unwrap();
        let vote_claim_zkbin = ZkBinary::decode(vote_claim_bin, false).unwrap();
        let set_governance_config_zkbin = ZkBinary::decode(set_governance_config_bin, false).unwrap();

        let init_circuit =
            ZkCircuit::new(dwow_core::zk::empty_witnesses(&init_zkbin).unwrap(), &init_zkbin);
        let pay_premium_circuit =
            ZkCircuit::new(dwow_core::zk::empty_witnesses(&pay_premium_zkbin).unwrap(), &pay_premium_zkbin);
        let propose_claim_circuit =
            ZkCircuit::new(dwow_core::zk::empty_witnesses(&propose_claim_zkbin).unwrap(), &propose_claim_zkbin);
        let vote_claim_circuit =
            ZkCircuit::new(dwow_core::zk::empty_witnesses(&vote_claim_zkbin).unwrap(), &vote_claim_zkbin);
        let set_governance_config_circuit =
            ZkCircuit::new(dwow_core::zk::empty_witnesses(&set_governance_config_zkbin).unwrap(), &set_governance_config_zkbin);

        let init_pk = ProvingKey::build(init_zkbin.k, &init_circuit).expect("ProvingKey::build failed");
        let pay_premium_pk = ProvingKey::build(pay_premium_zkbin.k, &pay_premium_circuit).expect("ProvingKey::build failed");
        let propose_claim_pk = ProvingKey::build(propose_claim_zkbin.k, &propose_claim_circuit).expect("ProvingKey::build failed");
        let vote_claim_pk = ProvingKey::build(vote_claim_zkbin.k, &vote_claim_circuit).expect("ProvingKey::build failed");
        let set_governance_config_pk = ProvingKey::build(set_governance_config_zkbin.k, &set_governance_config_circuit).expect("ProvingKey::build failed");

        Self {
            init_zkbin,
            init_pk,
            pay_premium_zkbin,
            pay_premium_pk,
            propose_claim_zkbin,
            propose_claim_pk,
            vote_claim_zkbin,
            vote_claim_pk,
            set_governance_config_zkbin,
            set_governance_config_pk,
            contract_id,
        }
    }

    /// The commitment over `children` followed by this call — the ordered set the node hashes.
    fn commitment_over(
        &self,
        children: &[dwow_sdk::tx::ContractCall],
        call_data: &[u8],
    ) -> pallas::Base {
        let mut calls = children.to_vec();
        calls.push(dwow_sdk::tx::ContractCall { contract_id: self.contract_id, data: call_data.to_vec() });
        commitment_of(&calls)
    }

    /// Initialize a new DAO-Escrow
    ///
    /// `mode` and `min_premium` are the caller's, because `InitializeParamsV1` carries them and
    /// `initialize_apply_v1` writes what it is given. They used to be a constant `Escrow` and a zero
    /// inside the contract, which made the other two `DaoEscrowMode` variants unreachable and
    /// `treasury_spend_v1`'s mode gate impossible to pass (`OBL-C154`); the fixture now states the
    /// mode it wants rather than inheriting one.
    #[allow(clippy::too_many_arguments)]
    pub fn initialize(
        &self,
        children: &[dwow_sdk::tx::ContractCall],
        nullifier_k: pallas::Scalar,
        dao_bulla: pallas::Base,
        owner_secret: pallas::Base,
        endowment_asset_id: pallas::Base,
        bulla_blind: pallas::Base,
        mode: DaoEscrowMode,
        min_premium: u64,
    ) -> Result<InitializeResult> {
        let mut input = InitV1CallData::new(
            nullifier_k,
            dao_bulla,
            owner_secret,
            endowment_asset_id,
            bulla_blind,
        );

        // Derive owner public key from secret
        let owner_pub = PublicKey::from_secret(SecretKey::from_bytes(owner_secret.to_repr()).unwrap());
        let (_owner_pub_x, _owner_pub_y) = owner_pub.xy().expect("pk not identity");

        // Build InitializeParamsV1 for call_data
        let params = InitializeParamsV1 {
            dao_bulla: DaoEscrowBulla(dao_bulla),
            owner_pubkey: owner_pub,
            endowment_asset_id: dwow_sdk::crypto::AssetId::from_base(endowment_asset_id),
            bulla_blind: dwow_sdk::crypto::Blind(bulla_blind),
            mode,
            min_premium,
        };

        // The selector IS part of the call data: the runner submits `call_data` verbatim
        // (`uniform_runner.rs:246-249`), and the contract's `initialize_get_metadata` decodes
        // `data[1..]`. Without the `0x00` this was 161 bytes of params and a 160-byte slice, so the
        // decode failed, the metadata arm returned its bare `vec![]`, and the host read that as the
        // documented rejection signal — which is how `test_heavyweight_dao_escrow` died at height 2
        // with "EMPTY metadata" while the register recorded only the symptom.
        let mut call_data = vec![0x00]; // InitializeV1
        call_data.extend_from_slice(&params.encode());

        // `OBL-C198`: the call data comes first because the commitment is a derivation over it, and
        // the set is `children` followed by this call — DFS post-order, children before parents.
        input.tx_commitment = self.commitment_over(children, &call_data);
        let commitment = input.tx_commitment;

        let (proof, public_inputs) = init_v1_proof(&self.init_zkbin, &self.init_pk, &input)?;

        Ok(InitializeResult { call_data, public_inputs, proof, commitment })
    }

    /// Pay premium to join DAO-Escrow as member
    #[allow(clippy::too_many_arguments)]
    pub fn pay_premium(
        &self,
        children: &[dwow_sdk::tx::ContractCall],
        nullifier_k: pallas::Scalar,
        dao_escrow_bulla: pallas::Base,
        current_block: u64,
        member_secret: pallas::Base,
        value: u64,
        asset_id: pallas::Base,
        expiry: u64,
        membership_blind: pallas::Base,
        value_blind: pallas::Scalar,
        mpc_secret_1: pallas::Base,
        mpc_secret_2: pallas::Base,
        mpc_secret_3: pallas::Base,
    ) -> Result<PayPremiumResult> {
        let mut input = PayPremiumV1CallData::new(
            nullifier_k,
            dao_escrow_bulla,
            current_block,
            member_secret,
            value,
            asset_id,
            expiry,
            membership_blind,
            value_blind,
            mpc_secret_1,
            mpc_secret_2,
            mpc_secret_3,
        );

        // Derive member public key from secret
        let member_pub =
            PublicKey::from_secret(SecretKey::from_bytes(member_secret.to_repr()).unwrap());
        let (mx, my) = member_pub.xy().expect("pk not identity");

        // Compute membership_note locally using same formula as circuit:
        // membership_note = poseidon_hash(DOMAIN_COIN_COMMIT, member_pub_x, member_pub_y,
        //                                  value, asset_id, expiry, membership_blind)
        let membership_note = dwow_sdk::crypto::poseidon_hash([
            pallas::Base::from(4u64), // DOMAIN_COIN_COMMIT
            mx,
            my,
            pallas::Base::from(value),
            pallas::Base::from(asset_id),
            pallas::Base::from(expiry),
            membership_blind,
        ]);

        // Build PayPremiumParamsV1 for call_data
        // Note: value_commit uses zero placeholders because EC operations cannot be replicated outside circuit
        let value_commit = pallas::Point::identity();

        let params = PayPremiumParamsV1 {
            dao_escrow_bulla: DaoEscrowBulla(dao_escrow_bulla),
            membership_note: MembershipNote(membership_note),
            value_commit,
            value,
            asset_id: dwow_sdk::crypto::AssetId::from_base(asset_id),
            expiry,
            membership_blind: dwow_sdk::crypto::Blind(membership_blind),
            value_blind: dwow_sdk::crypto::Blind(value_blind),
            member_pubkey: member_pub,
        };

        let mut call_data = vec![0x02]; // PayPremiumV1
        call_data.extend_from_slice(&params.encode());

        // `OBL-C198`: see `initialize` — call data first, then the commitment over the ordered set.
        input.tx_commitment = self.commitment_over(children, &call_data);
        let commitment = input.tx_commitment;

        let (proof, public_inputs) =
            pay_premium_v1_proof(&self.pay_premium_zkbin, &self.pay_premium_pk, &input)?;

        Ok(PayPremiumResult { call_data, public_inputs, proof, commitment })
    }

    /// Build InitializeParamsV1 call data without ZK proof (for testing when proof fails)
    pub fn initialize_call_data(
        &self,
        dao_bulla: pallas::Base,
        owner_pubkey: PublicKey,
        endowment_asset_id: pallas::Base,
        bulla_blind: pallas::Base,
        mode: DaoEscrowMode,
        min_premium: u64,
    ) -> Result<Vec<u8>> {
        let params = InitializeParamsV1 {
            dao_bulla: DaoEscrowBulla(dao_bulla),
            owner_pubkey,
            endowment_asset_id: dwow_sdk::crypto::AssetId::from_base(endowment_asset_id),
            bulla_blind: dwow_sdk::crypto::Blind(bulla_blind),
            mode,
            min_premium,
        };
        let mut call_data = vec![0x00]; // InitializeV1
        call_data.extend_from_slice(&params.encode());
        Ok(call_data)
    }

    /// Withdraw from endowment (WithdrawV1 - 0x03)
    /// The **owner's** withdrawal, carrying the `SetGovernanceConfigV2` ownership proof.
    ///
    /// `owner_secret` is the endowment's owner secret; the proof binds `recipient_pubkey`'s coordinates
    /// to knowledge of it, which is the endpoint's whole authorisation (`OBL-C152`'s last instance).
    /// The proof's `owner_nullifier` instance must ride in the params, so it is computed here with the
    /// same `UpdateV1CallData` the setter uses — one derivation, two callers, which is what keeps the
    /// contract's metadata arm and the prover's instance vector from disagreeing (`OBL-C156`'s class).
    pub fn withdraw(
        &self,
        children: &[dwow_sdk::tx::ContractCall],
        dao_escrow_bulla: pallas::Base,
        recipient_pubkey: PublicKey,
        value: u64,
        owner_secret: pallas::Base,
    ) -> Result<WithdrawResult> {
        let mut input = UpdateV1CallData::new(owner_secret, recipient_pubkey, dao_escrow_bulla);
        let owner_nullifier = input.compute_public_inputs().owner_nullifier;

        let params = WithdrawParamsV1 {
            dao_escrow_bulla: DaoEscrowBulla(dao_escrow_bulla),
            value,
            recipient_pubkey,
            owner_nullifier,
        };
        let mut call_data = vec![0x03]; // WithdrawV1
        // `WithdrawParamsV1::encode` is infallible — four fixed-size fields and no length prefix.
        call_data.extend_from_slice(&params.encode());

        // `OBL-C198`: the public inputs above are a pure function of the call data, so they come
        // before the proof — which is what lets the call data (and the commitment over it) exist
        // first.
        input.tx_commitment = self.commitment_over(children, &call_data);
        let commitment = input.tx_commitment;

        let (proof, _public_inputs) =
            update_v1_proof(&self.set_governance_config_zkbin, &self.set_governance_config_pk, &input)?;

        Ok(WithdrawResult { call_data, proof, commitment })
    }

    /// Endowment withdraw (EndowmentWithdrawV1 - 0x04)
    ///
    /// The endpoint's authority is the endowment's group and nothing else. Three *path selector* fields
    /// this took are gone from `EndowmentWithdrawParamsV1`, because none was a value the handler read:
    /// `capability_proof` carried a single `is_some()` bit, `proposal_id` named a second executor for
    /// the lifecycle `ExecuteClaimV1` already executes, and `claim_id` reached only the record it was
    /// copied into (`OBL-C166`).
    pub fn endowment_withdraw(
        &self,
        dao_escrow_bulla: pallas::Base,
        recipient_pubkey: PublicKey,
        value: u64,
    ) -> Result<EndowmentWithdrawResult> {
        let params = EndowmentWithdrawParamsV1 {
            dao_escrow_bulla: DaoEscrowBulla(dao_escrow_bulla),
            recipient_pubkey,
            value,
        };
        let mut call_data = vec![0x04]; // EndowmentWithdrawV1
        // `EndowmentWithdrawParamsV1::encode` is infallible now: three fixed-size fields, no `Option` and
        // no length prefix.
        call_data.extend_from_slice(&params.encode());
        Ok(EndowmentWithdrawResult { call_data })
    }

    /// Treasury spend (TreasurySpendV1 - 0x05)
    ///
    /// The same phantoms as `endowment_withdraw` lost their `proposal_id` and `capability_proof` params:
    /// the group's approval is the only authority, and it names the action rather than a proposal.
    pub fn treasury_spend(
        &self,
        dao_escrow_bulla: pallas::Base,
        recipient_pubkey: PublicKey,
        value: u64,
    ) -> Result<TreasurySpendResult> {
        let params = TreasurySpendParamsV1 {
            dao_escrow_bulla: DaoEscrowBulla(dao_escrow_bulla),
            recipient_pubkey,
            value,
        };
        let mut call_data = vec![0x05]; // TreasurySpendV1
        // `TreasurySpendParamsV1::encode` is infallible — three fixed-size fields.
        call_data.extend_from_slice(&params.encode());
        Ok(TreasurySpendResult { call_data })
    }

    // ========================================================================
    // GOVERNANCE ZK PROOF METHODS
    // ========================================================================

    /// Propose a claim with ZK proof (ProposeClaimV1 - 0x07)
    ///
    /// `capability_id` and `capability_secret` are **circuit witnesses**, not call parameters:
    /// `ProposeClaimParamsV1` carries neither, so they reach the proof and nothing else.
    /// `description_hash` is passed the same way — the circuit has no witness for it either, and the
    /// params struct no longer carries a field it could ride on.
    #[allow(clippy::too_many_arguments)]
    pub fn propose_claim(
        &self,
        children: &[dwow_sdk::tx::ContractCall],
        nullifier_k: pallas::Scalar,
        dao_escrow_bulla: pallas::Base,
        claim_id: pallas::Base,
        capability_id: pallas::Base,
        capability_secret: pallas::Base,
        proposer_secret: pallas::Base,
        value: u64,
        description_hash: pallas::Base,
        recipient_pubkey: PublicKey,
        proposal_blind: pallas::Base,
    ) -> Result<ProposeClaimResult> {
        let mut input = ProposeClaimV1CallData::new(
            nullifier_k,
            dao_escrow_bulla,
            claim_id,
            capability_id,
            capability_secret,
            proposer_secret,
            value,
            description_hash,
            recipient_pubkey,
            proposal_blind,
        );

        let params = ProposeClaimParamsV1 {
            dao_escrow_bulla: DaoEscrowBulla(dao_escrow_bulla),
            claim_id: ClaimId(claim_id),
            value,
            recipient_pubkey,
            // The same blind the proof was built with (`OBL-C153`): the contract hashes this into
            // `claim_commit` and the circuit's instances carry it, so the two sides agree by
            // construction rather than by coincidence.
            claim_blind: proposal_blind,
        };

        let mut call_data = vec![0x07]; // ProposeClaimV1
        // `ProposeClaimParamsV1::encode` is infallible now: five fixed-size fields, the `CapabilityProof`
        // and its length prefix having left the struct.
        call_data.extend_from_slice(&params.encode());

        // `OBL-C198`: see `initialize`.
        input.tx_commitment = self.commitment_over(children, &call_data);
        let commitment = input.tx_commitment;

        let (proof, public_inputs) =
            propose_claim_v1_proof(&self.propose_claim_zkbin, &self.propose_claim_pk, &input)?;

        Ok(ProposeClaimResult { call_data, public_inputs, proof, commitment })
    }

    /// Assemble a `propose_claim` call and **stop before the proof**, so a caller can bind it to a
    /// frame this call is not the last member of — see [`ProposeClaimPlan`] for the case that needs
    /// it. Everything here is `propose_claim`'s front half, verbatim: the same params, the same call
    /// data, the same `ProposeClaimV1CallData` — only `tx_commitment` is left for the caller.
    #[allow(clippy::too_many_arguments)]
    pub fn propose_claim_prepare(
        &self,
        nullifier_k: pallas::Scalar,
        dao_escrow_bulla: pallas::Base,
        claim_id: pallas::Base,
        capability_id: pallas::Base,
        capability_secret: pallas::Base,
        proposer_secret: pallas::Base,
        value: u64,
        description_hash: pallas::Base,
        recipient_pubkey: PublicKey,
        proposal_blind: pallas::Base,
    ) -> Result<ProposeClaimPlan> {
        let input = ProposeClaimV1CallData::new(
            nullifier_k,
            dao_escrow_bulla,
            claim_id,
            capability_id,
            capability_secret,
            proposer_secret,
            value,
            description_hash,
            recipient_pubkey,
            proposal_blind,
        );

        let params = ProposeClaimParamsV1 {
            dao_escrow_bulla: DaoEscrowBulla(dao_escrow_bulla),
            claim_id: ClaimId(claim_id),
            value,
            recipient_pubkey,
            // The same blind the proof is built with (`OBL-C153`) — see `propose_claim`.
            claim_blind: proposal_blind,
        };

        let mut call_data = vec![0x07]; // ProposeClaimV1
        call_data.extend_from_slice(&params.encode());

        Ok(ProposeClaimPlan {
            input,
            call_data,
            propose_claim_zkbin: self.propose_claim_zkbin.clone(),
            propose_claim_pk: self.propose_claim_pk.clone(),
        })
    }

    /// Vote on a claim with ZK proof (VoteClaimV1 - 0x08)
    pub fn vote_claim(
        &self,
        children: &[dwow_sdk::tx::ContractCall],
        nullifier_k: pallas::Scalar,
        vote_commit_value: pallas::Point,
        vote_commit_random: pallas::Point,
        proposal_id: pallas::Base,
        capability_id: pallas::Base,
        capability_secret: pallas::Base,
        voter_secret: pallas::Base,
        vote_yes: bool,
        vote_blind: pallas::Base,
        dao_escrow_bulla: pallas::Base,
        claim_id: pallas::Base,
        voter_pubkey: PublicKey,
        capability_proof: CapabilityProof,
    ) -> Result<VoteClaimHarnessResult> {
        let mut input = VoteClaimV1CallData::new(
            nullifier_k,
            vote_commit_value,
            vote_commit_random,
            proposal_id,
            capability_id,
            capability_secret,
            dao_escrow_bulla,
            voter_secret,
            vote_yes,
            vote_blind,
        );

        let params = VoteClaimParamsV1 {
            dao_escrow_bulla: DaoEscrowBulla(dao_escrow_bulla),
            claim_id: ClaimId(claim_id),
            vote: if vote_yes { VoteType::Yes } else { VoteType::No },
            voter_pubkey,
            capability_proof,
        };

        let mut call_data = vec![0x08]; // VoteClaimV1
        call_data.extend_from_slice(&params.encode().map_err(|e| dwow_core::Error::Custom(format!("{e}")))?);

        // `OBL-C198`: see `initialize`.
        input.tx_commitment = self.commitment_over(children, &call_data);
        let commitment = input.tx_commitment;

        let (proof, public_inputs) =
            vote_claim_v1_proof(&self.vote_claim_zkbin, &self.vote_claim_pk, &input)?;

        Ok(VoteClaimHarnessResult { call_data, public_inputs, proof, commitment })
    }

    // Two ZK builders were removed here — `verify_member_capability` (0x0b) and `resolve_dispute`
    // (0x0c) — with their endpoints, their circuits and their `.zk`/`.zk.bin` files. Both belonged to
    // the OCap/Identity model that MultiSig groups replaced; `CapabilityProof` survives only as a field
    // of `VoteClaimParamsV1`.

    // ========================================================================
    // NON-ZK CALL DATA METHODS
    // ========================================================================

    /// Execute an approved claim (ExecuteClaimV1 - 0x09)
    pub fn execute_claim(
        &self,
        dao_escrow_bulla: pallas::Base,
        proposal_id: pallas::Base,
        recipient_pubkey: PublicKey,
        value: u64,
    ) -> Result<ExecuteClaimResult> {
        let params = ExecuteClaimParamsV1 {
            dao_escrow_bulla: DaoEscrowBulla(dao_escrow_bulla),
            proposal_id: ProposalId(proposal_id),
            recipient_pubkey,
            value,
        };
        let mut call_data = vec![0x09]; // ExecuteClaimV1
        call_data.extend_from_slice(&params.encode());
        Ok(ExecuteClaimResult { call_data })
    }

    /// Three call-data builders were removed here — `register_capability_requirement` (0x0a),
    /// `enable_drain_protection` (0x06) and `deactivate_capability_requirement` (0x10). Their endpoints
    /// are retired: `lib.rs` leaves the selectors unassigned rather than mapping them to a no-op, so a
    /// caller sending one reaches `InvalidFunction`.

    /// Cancel a pending claim (CancelClaimV1 - 0x0d)
    ///
    /// `params.proposer_pubkey` is gone with the check it fed: the comparison it made was between two
    /// public values, so it admitted anyone who knew the proposer's key (`OBL-C152`). The endowment's
    /// group authorises the cancellation now, over the claim id.
    pub fn cancel_claim(
        &self,
        dao_escrow_bulla: pallas::Base,
        claim_id: pallas::Base,
    ) -> Result<CancelClaimResult> {
        let params = CancelClaimParamsV1 {
            dao_escrow_bulla: DaoEscrowBulla(dao_escrow_bulla),
            claim_id: ClaimId(claim_id),
        };
        let mut call_data = vec![0x0d]; // CancelClaimV1
        call_data.extend_from_slice(&params.encode());
        Ok(CancelClaimResult { call_data })
    }

    // ── Governance (`OBL-C151`) ──────────────────────────────────────────────────────────────────

    /// The governing group's **threshold**, and the secrets of the members who join it — three members,
    /// two of whom must approve.
    ///
    /// These are the fixture's, and they are the only place the group is defined: `governance_group`
    /// below derives its id with the multisig contract's own `derive_group_id`, so the id the endowment
    /// stores and the id the group signs under are one value computed once. The shape is
    /// `DrainProtectionHarness`'s (`OBL-C101`).
    pub const GOVERNANCE_THRESHOLD: u8 = 2;
    pub const GOVERNANCE_MEMBERS: [pallas::Base; 3] = [
        pallas::Base::from_raw([11, 0, 0, 0]),
        pallas::Base::from_raw([12, 0, 0, 0]),
        pallas::Base::from_raw([13, 0, 0, 0]),
    ];

    /// The member commitments of the governance group, in the order `create_group` is given them.
    pub fn governance_member_commitments() -> Vec<pallas::Base> {
        Self::GOVERNANCE_MEMBERS
            .iter()
            .map(|s| crate::harness::multisig::MultiSigHarness::member_commitment(*s))
            .collect()
    }

    /// The id of the governance group: the multisig contract's derivation, called rather than
    /// re-implemented, over the members above. The endowment stores this id and every approval has to
    /// name it, so the id has one definition.
    pub fn governance_group() -> pallas::Base {
        crate::harness::multisig::MultiSigHarness::group_id(
            Self::GOVERNANCE_THRESHOLD,
            &Self::governance_member_commitments(),
        )
    }

    /// `UpdateV1` (0x01) — the governance setter (`OBL-C151`).
    ///
    /// **ZK, because the owner must be proved rather than asserted.** The proof is the
    /// `SetGovernanceConfigV2` circuit's: it derives `owner_pub = ec_mul_base(owner_secret,
    /// NULLIFIER_K)` and constrains the exposed coordinates to it, so a caller who does not hold
    /// `owner_secret` cannot produce a proof the contract's owner check will accept. A harness that
    /// supplied `owner_pubkey` without the matching secret would produce a proof that cannot
    /// satisfy the circuit — a failure at proving time, not a forged authorisation.
    ///
    /// `owner_nullifier` is taken from the proof's own public inputs rather than accepted from the
    /// caller, because it is witness-derived and the contract records it to make the proof one-shot.
    pub fn update(
        &self,
        children: &[dwow_sdk::tx::ContractCall],
        dao_escrow_bulla: pallas::Base,
        owner_secret: pallas::Base,
        owner_pubkey: PublicKey,
        multisig_group_id: Option<pallas::Base>,
    ) -> Result<UpdateResult> {
        let mut input = UpdateV1CallData::new(owner_secret, owner_pubkey, dao_escrow_bulla);

        // `OBL-C198`: the public inputs are a pure function of the call data, so they come before
        // the proof — which is what lets the call data (and the commitment over it) exist first.
        let public_inputs = input.compute_public_inputs();
        let params = UpdateParamsV1 {
            bulla: DaoEscrowBulla(dao_escrow_bulla),
            multisig_group_id,
            owner_pubkey,
            owner_nullifier: public_inputs.owner_nullifier,
        };
        let mut call_data = vec![0x01]; // UpdateV1 — the selector is part of the payload
        call_data.extend_from_slice(&params.encode());

        input.tx_commitment = self.commitment_over(children, &call_data);
        let commitment = input.tx_commitment;

        let (proof, _public_inputs) = update_v1_proof(
            &self.set_governance_config_zkbin,
            &self.set_governance_config_pk,
            &input,
        )?;

        Ok(UpdateResult { call_data, proof, public_inputs, commitment })
    }
}

/// Result of the `UpdateV1` governance setter
pub struct UpdateResult {
    pub call_data: Vec<u8>,
    pub proof: dwow_core::zk::Proof,
    /// The five instances the proof was created over. A caller cannot choose them: the nullifier is
    /// witness-derived and the contract records it, so this is the only place the value is legible.
    pub public_inputs: UpdateV1PublicInputs,
    /// The commitment every proof in this transaction binds to (`OBL-C198`), so the caller can
    /// prove a child against the same value.
    pub commitment: pallas::Base,
}

/// Result of DAO-Escrow withdraw
pub struct WithdrawResult {
    pub call_data: Vec<u8>,
    /// The `SetGovernanceConfigV2` ownership proof, which the endpoint now requires.
    pub proof: dwow_core::zk::Proof,
    /// The commitment every proof in this transaction binds to (`OBL-C198`), so the caller can
    /// prove a child against the same value.
    pub commitment: pallas::Base,
}

/// Result of DAO-Escrow endowment withdraw
pub struct EndowmentWithdrawResult {
    pub call_data: Vec<u8>,
}

/// Result of DAO-Escrow treasury spend
pub struct TreasurySpendResult {
    pub call_data: Vec<u8>,
}

impl super::ContractHarness for DaoEscrowHarness {
    fn name(&self) -> &str {
        "dao_escrow"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec![
            "InitV2",
            "PayPremiumV2",
            "ProposeClaimV2",
            "VoteClaimV2",
            "SetGovernanceConfigV2",
        ]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "InitV2" => Some(&self.init_zkbin),
            "PayPremiumV2" => Some(&self.pay_premium_zkbin),
            "ProposeClaimV2" => Some(&self.propose_claim_zkbin),
            "VoteClaimV2" => Some(&self.vote_claim_zkbin),
            "SetGovernanceConfigV2" => Some(&self.set_governance_config_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "InitV2" => Some(&self.init_pk),
            "PayPremiumV2" => Some(&self.pay_premium_pk),
            "ProposeClaimV2" => Some(&self.propose_claim_pk),
            "VoteClaimV2" => Some(&self.vote_claim_pk),
            "SetGovernanceConfigV2" => Some(&self.set_governance_config_pk),
            _ => None,
        }
    }
}

// ============================================================================
/// Result structs for DAO Escrow harness
// ============================================================================

/// Result of initializing a DAO-Escrow
pub struct InitializeResult {
    pub call_data: Vec<u8>,
    pub public_inputs: InitV1PublicInputs,
    pub proof: dwow_core::zk::Proof,
    /// The commitment every proof in this transaction binds to (`OBL-C198`), so the caller can
    /// prove a child against the same value.
    pub commitment: pallas::Base,
}

/// Result of paying premium to join DAO-Escrow
pub struct PayPremiumResult {
    pub call_data: Vec<u8>,
    pub public_inputs: PayPremiumV1PublicInputs,
    pub proof: dwow_core::zk::Proof,
    /// The commitment every proof in this transaction binds to (`OBL-C198`), so the caller can
    /// prove a child against the same value.
    pub commitment: pallas::Base,
}

// ============================================================================
// Governance ZK proof result structs
// ============================================================================

/// Result of proposing a claim
pub struct ProposeClaimResult {
    pub call_data: Vec<u8>,
    pub public_inputs: ProposeClaimV1PublicInputs,
    pub proof: dwow_core::zk::Proof,
    /// The commitment every proof in this transaction binds to (`OBL-C198`), so the caller can
    /// prove a child against the same value.
    pub commitment: pallas::Base,
}

/// A prepared `propose_claim`, proved against a caller-supplied commitment — the shape
/// [`MultiSigHarness::finalize_prepare`] established, and needed here for the case that one cannot
/// serve: a call that is a **child with a parent after it**.
///
/// `propose_claim` takes its children and hashes `[…children, this call]`, which is the whole frame
/// only when this call is the last one in it. In `labor_market`'s `DisputeV1` row it is not: the
/// frame is `[multisig finalize, this call, the labor_market call]`, so the commitment it must bind
/// cannot be computed at all from what it is given — the root's bytes do not exist yet. A plan lets
/// the caller assemble the frame (preparing every call first, since a proof is not part of a call's
/// bytes) and then prove each one against it.
pub struct ProposeClaimPlan {
    input: ProposeClaimV1CallData,
    /// The call data the commitment must cover — selector `0x07` and the encoded params.
    pub call_data: Vec<u8>,
    propose_claim_zkbin: ZkBinary,
    propose_claim_pk: ProvingKey,
}

impl ProposeClaimPlan {
    /// Prove against `tx_commitment` — the commitment over the whole ordered call set the node will
    /// hash, not just this call.
    pub fn prove(mut self, tx_commitment: pallas::Base) -> Result<ProposeClaimResult> {
        self.input.tx_commitment = tx_commitment;
        let (proof, public_inputs) =
            propose_claim_v1_proof(&self.propose_claim_zkbin, &self.propose_claim_pk, &self.input)?;
        Ok(ProposeClaimResult {
            call_data: self.call_data,
            public_inputs,
            proof,
            commitment: tx_commitment,
        })
    }
}

/// Result of voting on a claim
pub struct VoteClaimHarnessResult {
    pub call_data: Vec<u8>,
    pub public_inputs: VoteClaimV1PublicInputs,
    pub proof: dwow_core::zk::Proof,
    /// The commitment every proof in this transaction binds to (`OBL-C198`), so the caller can
    /// prove a child against the same value.
    pub commitment: pallas::Base,
}

// ============================================================================
// Non-ZK call data result structs
// ============================================================================

/// Result of executing a claim
pub struct ExecuteClaimResult {
    pub call_data: Vec<u8>,
}

/// Result of cancelling a claim
pub struct CancelClaimResult {
    pub call_data: Vec<u8>,
}

