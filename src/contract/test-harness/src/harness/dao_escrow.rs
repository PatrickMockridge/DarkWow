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
    resolve_dispute::{resolve_dispute_v1_proof, ResolveDisputeV1CallData, ResolveDisputeV1PublicInputs},
    verify_member_capability::{verify_member_capability_v1_proof, VerifyMemberCapabilityV1CallData, VerifyMemberCapabilityV1PublicInputs},
    vote_claim::{vote_claim_v1_proof, VoteClaimV1CallData, VoteClaimV1PublicInputs},
};
use dwow_dao_escrow_contract::model::{
    CancelClaimParamsV1, CapabilityProof, ClaimId, DaoEscrowBulla, ClaimType, ExecuteClaimParamsV1,
    InitializeParamsV1, PayPremiumParamsV1, ProposeClaimParamsV1,
    RegisterCapabilityRequirementParamsV1, ResolveDisputeParamsV1,
    VerifyMemberCapabilityParamsV1,
    VoteClaimParamsV1, WithdrawParamsV1, MembershipNote, ProposalId, EndowmentWithdrawParamsV1,
    TreasurySpendParamsV1, OracleAttestationRef, VoteType,
};
use dwow_dao_escrow_contract::model::Membership;

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
    /// VerifyMemberCapability_V1 ZkBinary
    verify_member_capability_zkbin: ZkBinary,
    /// VerifyMemberCapability_V1 ProvingKey
    verify_member_capability_pk: ProvingKey,
    /// ResolveDispute_V1 ZkBinary
    resolve_dispute_zkbin: ZkBinary,
    /// ResolveDispute_V1 ProvingKey
    resolve_dispute_pk: ProvingKey,
    /// SetGovernanceConfig_V1 ZkBinary
    set_governance_config_zkbin: ZkBinary,
    /// SetGovernanceConfig_V1 ProvingKey
    set_governance_config_pk: ProvingKey,
}

impl DaoEscrowHarness {
    /// Spawn a new DaoEscrow harness with pre-loaded circuits
    pub fn spawn() -> Self {
        let init_bin = include_bytes!("../../../dao_escrow/proof/init.zk.bin");
        let pay_premium_bin = include_bytes!("../../../dao_escrow/proof/pay_premium.zk.bin");
        let propose_claim_bin = include_bytes!("../../../dao_escrow/proof/propose_claim.zk.bin");
        let vote_claim_bin = include_bytes!("../../../dao_escrow/proof/vote_claim.zk.bin");
        let verify_member_cap_bin = include_bytes!("../../../dao_escrow/proof/verify_member_capability.zk.bin");
        let resolve_dispute_bin = include_bytes!("../../../dao_escrow/proof/resolve_dispute.zk.bin");
        let set_governance_config_bin = include_bytes!("../../../dao_escrow/proof/set_governance_config.zk.bin");

        let init_zkbin = ZkBinary::decode(init_bin, false).unwrap();
        let pay_premium_zkbin = ZkBinary::decode(pay_premium_bin, false).unwrap();
        let propose_claim_zkbin = ZkBinary::decode(propose_claim_bin, false).unwrap();
        let vote_claim_zkbin = ZkBinary::decode(vote_claim_bin, false).unwrap();
        let verify_member_capability_zkbin = ZkBinary::decode(verify_member_cap_bin, false).unwrap();
        let resolve_dispute_zkbin = ZkBinary::decode(resolve_dispute_bin, false).unwrap();
        let set_governance_config_zkbin = ZkBinary::decode(set_governance_config_bin, false).unwrap();

        let init_circuit =
            ZkCircuit::new(dwow_core::zk::empty_witnesses(&init_zkbin).unwrap(), &init_zkbin);
        let pay_premium_circuit =
            ZkCircuit::new(dwow_core::zk::empty_witnesses(&pay_premium_zkbin).unwrap(), &pay_premium_zkbin);
        let propose_claim_circuit =
            ZkCircuit::new(dwow_core::zk::empty_witnesses(&propose_claim_zkbin).unwrap(), &propose_claim_zkbin);
        let vote_claim_circuit =
            ZkCircuit::new(dwow_core::zk::empty_witnesses(&vote_claim_zkbin).unwrap(), &vote_claim_zkbin);
        let verify_member_capability_circuit =
            ZkCircuit::new(dwow_core::zk::empty_witnesses(&verify_member_capability_zkbin).unwrap(), &verify_member_capability_zkbin);
        let resolve_dispute_circuit =
            ZkCircuit::new(dwow_core::zk::empty_witnesses(&resolve_dispute_zkbin).unwrap(), &resolve_dispute_zkbin);
        let set_governance_config_circuit =
            ZkCircuit::new(dwow_core::zk::empty_witnesses(&set_governance_config_zkbin).unwrap(), &set_governance_config_zkbin);

        let init_pk = ProvingKey::build(init_zkbin.k, &init_circuit).expect("ProvingKey::build failed");
        let pay_premium_pk = ProvingKey::build(pay_premium_zkbin.k, &pay_premium_circuit).expect("ProvingKey::build failed");
        let propose_claim_pk = ProvingKey::build(propose_claim_zkbin.k, &propose_claim_circuit).expect("ProvingKey::build failed");
        let vote_claim_pk = ProvingKey::build(vote_claim_zkbin.k, &vote_claim_circuit).expect("ProvingKey::build failed");
        let verify_member_capability_pk = ProvingKey::build(verify_member_capability_zkbin.k, &verify_member_capability_circuit).expect("ProvingKey::build failed");
        let resolve_dispute_pk = ProvingKey::build(resolve_dispute_zkbin.k, &resolve_dispute_circuit).expect("ProvingKey::build failed");
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
            verify_member_capability_zkbin,
            verify_member_capability_pk,
            resolve_dispute_zkbin,
            resolve_dispute_pk,
            set_governance_config_zkbin,
            set_governance_config_pk,
        }
    }

    /// Initialize a new DAO-Escrow
    pub fn initialize(
        &self,
        nullifier_k: pallas::Scalar,
        dao_bulla: pallas::Base,
        owner_secret: pallas::Base,
        endowment_asset_id: pallas::Base,
        bulla_blind: pallas::Base,
    ) -> Result<InitializeResult> {
        let input = InitV1CallData::new(
            nullifier_k,
            dao_bulla,
            owner_secret,
            endowment_asset_id,
            bulla_blind,
        );
        let (proof, public_inputs) = init_v1_proof(&self.init_zkbin, &self.init_pk, &input)?;

        // Derive owner public key from secret
        let owner_pub = PublicKey::from_secret(SecretKey::from_bytes(owner_secret.to_repr()).unwrap());
        let (_owner_pub_x, _owner_pub_y) = owner_pub.xy().expect("pk not identity");

        // Build InitializeParamsV1 for call_data
        let params = InitializeParamsV1 {
            dao_bulla: DaoEscrowBulla(dao_bulla),
            owner_pubkey: owner_pub,
            endowment_asset_id: dwow_sdk::crypto::AssetId::from_base(endowment_asset_id),
            bulla_blind: dwow_sdk::crypto::Blind(bulla_blind),
            enable_drain_protection: false,
            instance_seed: [0u8; 32],
        };

        let mut call_data = vec![];
        call_data.extend_from_slice(&params.encode());

        Ok(InitializeResult { call_data, public_inputs, proof })
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
        asset_id: pallas::Base,
        expiry: u64,
        membership_blind: pallas::Base,
        value_blind: pallas::Scalar,
        mpc_secret_1: pallas::Base,
        mpc_secret_2: pallas::Base,
        mpc_secret_3: pallas::Base,
    ) -> Result<PayPremiumResult> {
        let input = PayPremiumV1CallData::new(
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
        let (proof, public_inputs) =
            pay_premium_v1_proof(&self.pay_premium_zkbin, &self.pay_premium_pk, &input)?;

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

        let mut call_data = vec![];
        call_data.extend_from_slice(&params.encode());

        Ok(PayPremiumResult { call_data, public_inputs, proof })
    }

    /// Build InitializeParamsV1 call data without ZK proof (for testing when proof fails)
    pub fn initialize_call_data(
        &self,
        dao_bulla: pallas::Base,
        owner_pubkey: PublicKey,
        endowment_asset_id: pallas::Base,
        bulla_blind: pallas::Base,
    ) -> Result<Vec<u8>> {
        let params = InitializeParamsV1 {
            dao_bulla: DaoEscrowBulla(dao_bulla),
            owner_pubkey,
            endowment_asset_id: dwow_sdk::crypto::AssetId::from_base(endowment_asset_id),
            bulla_blind: dwow_sdk::crypto::Blind(bulla_blind),
            enable_drain_protection: false,
            instance_seed: [0u8; 32],
        };
        let mut call_data = vec![];
        call_data.extend_from_slice(&params.encode());
        Ok(call_data)
    }

    /// Withdraw from endowment (WithdrawV1 - 0x03)
    pub fn withdraw(
        &self,
        dao_escrow_bulla: pallas::Base,
        recipient_pubkey: PublicKey,
        value: u64,
    ) -> Result<WithdrawResult> {
        let params = WithdrawParamsV1 {
            dao_escrow_bulla: DaoEscrowBulla(dao_escrow_bulla),
            value,
            recipient_pubkey,
            capability_proof: None,
        };
        let mut call_data = vec![];
        call_data.extend_from_slice(&params.encode());
        Ok(WithdrawResult { call_data })
    }

    /// Endowment withdraw (EndowmentWithdrawV1 - 0x04)
    pub fn endowment_withdraw(
        &self,
        dao_escrow_bulla: pallas::Base,
        claim_id: pallas::Base,
        recipient_pubkey: PublicKey,
        value: u64,
    ) -> Result<EndowmentWithdrawResult> {
        let params = EndowmentWithdrawParamsV1 {
            dao_escrow_bulla: DaoEscrowBulla(dao_escrow_bulla),
            claim_id: ClaimId(claim_id),
            recipient_pubkey,
            value,
            capability_proof: None,
            proposal_id: None,
        };
        let mut call_data = vec![];
        call_data.extend_from_slice(&params.encode());
        Ok(EndowmentWithdrawResult { call_data })
    }

    /// Treasury spend (TreasurySpendV1 - 0x05)
    pub fn treasury_spend(
        &self,
        dao_escrow_bulla: pallas::Base,
        proposal_id: pallas::Base,
        recipient_pubkey: PublicKey,
        value: u64,
    ) -> Result<TreasurySpendResult> {
        let params = TreasurySpendParamsV1 {
            dao_escrow_bulla: DaoEscrowBulla(dao_escrow_bulla),
            proposal_id: proposal_id,
            recipient_pubkey,
            value,
            capability_proof: None,
        };
        let mut call_data = vec![];
        call_data.extend_from_slice(&params.encode());
        Ok(TreasurySpendResult { call_data })
    }

    // ========================================================================
    // GOVERNANCE ZK PROOF METHODS
    // ========================================================================

    /// Propose a claim with ZK proof (ProposeClaimV1 - 0x07)
    #[allow(clippy::too_many_arguments)]
    pub fn propose_claim(
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
        proposer_pubkey: PublicKey,
        claim_type: ClaimType,
        proposal_blind: pallas::Base,
        capability_proof: CapabilityProof,
    ) -> Result<ProposeClaimResult> {
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
        let (proof, public_inputs) =
            propose_claim_v1_proof(&self.propose_claim_zkbin, &self.propose_claim_pk, &input)?;

        let params = ProposeClaimParamsV1 {
            dao_escrow_bulla: DaoEscrowBulla(dao_escrow_bulla),
            claim_id: ClaimId(claim_id),
            value,
            description_hash,
            recipient_pubkey,
            proposer_pubkey,
            claim_type,
            capability_proof,
        };

        let mut call_data = vec![];
        call_data.extend_from_slice(&params.encode());

        Ok(ProposeClaimResult { call_data, public_inputs, proof })
    }

    /// Vote on a claim with ZK proof (VoteClaimV1 - 0x08)
    pub fn vote_claim(
        &self,
        nullifier_k: pallas::Scalar,
        vote_commit_value: pallas::Point,
        vote_commit_random: pallas::Point,
        proposal_id: pallas::Base,
        capability_id: pallas::Base,
        capability_secret: pallas::Base,
        voter_secret: pallas::Base,
        vote_yes: bool,
        vote_blind: pallas::Scalar,
        dao_escrow_bulla: pallas::Base,
        claim_id: pallas::Base,
        voter_pubkey: PublicKey,
        capability_proof: CapabilityProof,
    ) -> Result<VoteClaimHarnessResult> {
        let input = VoteClaimV1CallData::new(
            nullifier_k,
            vote_commit_value,
            vote_commit_random,
            proposal_id,
            capability_id,
            capability_secret,
            voter_secret,
            vote_yes,
            vote_blind,
        );
        let (proof, public_inputs) =
            vote_claim_v1_proof(&self.vote_claim_zkbin, &self.vote_claim_pk, &input)?;

        let params = VoteClaimParamsV1 {
            dao_escrow_bulla: DaoEscrowBulla(dao_escrow_bulla),
            claim_id: ClaimId(claim_id),
            vote: if vote_yes { VoteType::Yes } else { VoteType::No },
            voter_pubkey,
            capability_proof,
        };

        let mut call_data = vec![];
        call_data.extend_from_slice(&params.encode());

        Ok(VoteClaimHarnessResult { call_data, public_inputs, proof })
    }

    /// Verify member capability with ZK proof (VerifyMemberCapabilityV1 - 0x0b)
    pub fn verify_member_capability(
        &self,
        nullifier_k: pallas::Scalar,
        capability_id: pallas::Base,
        dao_escrow_bulla: pallas::Base,
        capability_secret: pallas::Base,
        holder_secret: pallas::Base,
        holder_pubkey: PublicKey,
        capability_proof: CapabilityProof,
    ) -> Result<VerifyMemberCapabilityResult> {
        let input = VerifyMemberCapabilityV1CallData::new(
            nullifier_k,
            capability_id,
            dao_escrow_bulla,
            capability_secret,
            holder_secret,
        );
        let (proof, public_inputs) =
            verify_member_capability_v1_proof(&self.verify_member_capability_zkbin, &self.verify_member_capability_pk, &input)?;

        let params = VerifyMemberCapabilityParamsV1 {
            dao_escrow_bulla: DaoEscrowBulla(dao_escrow_bulla),
            capability_proof,
            holder_pubkey,
        };

        let mut call_data = vec![];
        call_data.extend_from_slice(&params.encode());

        Ok(VerifyMemberCapabilityResult { call_data, public_inputs, proof })
    }

    /// Resolve a dispute with ZK proof (ResolveDisputeV1 - 0x0c)
    #[allow(clippy::too_many_arguments)]
    pub fn resolve_dispute(
        &self,
        nullifier_k: pallas::Scalar,
        capability_id: pallas::Base,
        dao_escrow_bulla: pallas::Base,
        dispute_id: pallas::Base,
        capability_secret: pallas::Base,
        arbitrator_secret: pallas::Base,
        attestations: Vec<OracleAttestationRef>,
        attestation_root: pallas::Base,
        resolution_result: bool,
        payout_amount: u64,
        payout_recipient: PublicKey,
        proposal_id: pallas::Base,
        capability_proof: CapabilityProof,
    ) -> Result<ResolveDisputeHarnessResult> {
        let attestation_count = attestations.len() as u64;
        let threshold = attestation_count; // In tests, threshold = number of attestations provided

        let input = ResolveDisputeV1CallData::new(
            nullifier_k,
            capability_id,
            dao_escrow_bulla,
            dispute_id,
            capability_secret,
            arbitrator_secret,
            attestation_count,
            threshold,
            resolution_result,
            payout_amount,
            payout_recipient,
            attestation_root,
        );
        let (proof, public_inputs) =
            resolve_dispute_v1_proof(&self.resolve_dispute_zkbin, &self.resolve_dispute_pk, &input)?;

        let params = ResolveDisputeParamsV1 {
            dao_escrow_bulla: DaoEscrowBulla(dao_escrow_bulla),
            proposal_id: ProposalId(proposal_id),
            attestations,
            capability_proof,
            payout_amount,
            payout_recipient,
        };

        let mut call_data = vec![];
        call_data.extend_from_slice(&params.encode());

        Ok(ResolveDisputeHarnessResult { call_data, public_inputs, proof })
    }

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
        let mut call_data = vec![];
        call_data.extend_from_slice(&params.encode());
        Ok(ExecuteClaimResult { call_data })
    }

    /// Register a capability requirement (RegisterCapabilityRequirementV1 - 0x0a)
    pub fn register_capability_requirement(
        &self,
        dao_escrow_bulla: pallas::Base,
        role: Vec<u8>,
        capability_id: [u8; 32],
        identity_contract_bulla: pallas::Base,
    ) -> Result<RegisterCapabilityRequirementResult> {
        let params = RegisterCapabilityRequirementParamsV1 {
            dao_escrow_bulla: DaoEscrowBulla(dao_escrow_bulla),
            role,
            capability_id,
            identity_contract_bulla,
        };
        let mut call_data = vec![];
        call_data.extend_from_slice(&params.encode());
        Ok(RegisterCapabilityRequirementResult { call_data })
    }

    /// Cancel a pending claim (CancelClaimV1 - 0x0d)
    pub fn cancel_claim(
        &self,
        dao_escrow_bulla: pallas::Base,
        claim_id: pallas::Base,
        proposer_pubkey: PublicKey,
    ) -> Result<CancelClaimResult> {
        let params = CancelClaimParamsV1 {
            dao_escrow_bulla: DaoEscrowBulla(dao_escrow_bulla),
            claim_id: ClaimId(claim_id),
            proposer_pubkey,
        };
        let mut call_data = vec![];
        call_data.extend_from_slice(&params.encode());
        Ok(CancelClaimResult { call_data })
    }
}

/// Result of DAO-Escrow withdraw
pub struct WithdrawResult {
    pub call_data: Vec<u8>,
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
            "VerifyMemberCapabilityV2",
            "ResolveDisputeV2",
            "SetGovernanceConfigV2",
        ]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "InitV2" => Some(&self.init_zkbin),
            "PayPremiumV2" => Some(&self.pay_premium_zkbin),
            "ProposeClaimV2" => Some(&self.propose_claim_zkbin),
            "VoteClaimV2" => Some(&self.vote_claim_zkbin),
            "VerifyMemberCapabilityV2" => Some(&self.verify_member_capability_zkbin),
            "ResolveDisputeV2" => Some(&self.resolve_dispute_zkbin),
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
            "VerifyMemberCapabilityV2" => Some(&self.verify_member_capability_pk),
            "ResolveDisputeV2" => Some(&self.resolve_dispute_pk),
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
}

/// Result of paying premium to join DAO-Escrow
pub struct PayPremiumResult {
    pub call_data: Vec<u8>,
    pub public_inputs: PayPremiumV1PublicInputs,
    pub proof: dwow_core::zk::Proof,
}

// ============================================================================
// Governance ZK proof result structs
// ============================================================================

/// Result of proposing a claim
pub struct ProposeClaimResult {
    pub call_data: Vec<u8>,
    pub public_inputs: ProposeClaimV1PublicInputs,
    pub proof: dwow_core::zk::Proof,
}

/// Result of voting on a claim
pub struct VoteClaimHarnessResult {
    pub call_data: Vec<u8>,
    pub public_inputs: VoteClaimV1PublicInputs,
    pub proof: dwow_core::zk::Proof,
}

/// Result of verifying member capability
pub struct VerifyMemberCapabilityResult {
    pub call_data: Vec<u8>,
    pub public_inputs: VerifyMemberCapabilityV1PublicInputs,
    pub proof: dwow_core::zk::Proof,
}

/// Result of resolving a dispute
pub struct ResolveDisputeHarnessResult {
    pub call_data: Vec<u8>,
    pub public_inputs: ResolveDisputeV1PublicInputs,
    pub proof: dwow_core::zk::Proof,
}

// ============================================================================
// Non-ZK call data result structs
// ============================================================================

/// Result of executing a claim
pub struct ExecuteClaimResult {
    pub call_data: Vec<u8>,
}

/// Result of registering a capability requirement
pub struct RegisterCapabilityRequirementResult {
    pub call_data: Vec<u8>,
}

/// Result of cancelling a claim
pub struct CancelClaimResult {
    pub call_data: Vec<u8>,
}

