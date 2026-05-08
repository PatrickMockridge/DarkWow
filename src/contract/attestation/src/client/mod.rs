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

//! Attestation contract client API
//!
//! This module provides builder structs for constructing attestation contract calls.
//! Also includes ZK proof generation modules for circuit verification.

//! ZK proof client modules
pub mod create_attestation_v1;
pub mod create_claim_v1;
pub mod verify_claim_v1;
pub mod consume_claim_v1;
pub mod check_not_revoked_v1;
pub mod delegate_attestation_v1;
pub mod verify_chain_v1;
pub mod update_delegation_v1;

use dwow_sdk::{
    crypto::{PublicKey},
    pasta::pallas,
};

use crate::model::{
    AttestationId, ClaimId, ConsumeClaimParamsV1, CreateAttestationParamsV1, CreateClaimParamsV1,
    ExpireAttestationParamsV1, Predicate, RevokeAttestationParamsV1, ValidateClaimParamsV1,
    VerifyClaimParamsV1,
};

/// Builder for CreateAttestationV1 params
#[derive(Default)]
pub struct CreateAttestationBuilder {
    attestation_id: Option<AttestationId>,
    attestor_pubkey: Option<PublicKey>,
    claim_type: Option<Predicate>,
    claim_data: Option<Vec<pallas::Base>>,
    metadata: Option<Vec<u8>>,
    expires_at: Option<u64>,
}

impl CreateAttestationBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn attestation_id(mut self, id: AttestationId) -> Self {
        self.attestation_id = Some(id);
        self
    }

    pub fn attestor_pubkey(mut self, pubkey: PublicKey) -> Self {
        self.attestor_pubkey = Some(pubkey);
        self
    }

    pub fn claim_type(mut self, claim_type: Predicate) -> Self {
        self.claim_type = Some(claim_type);
        self
    }

    pub fn claim_data(mut self, data: Vec<pallas::Base>) -> Self {
        self.claim_data = Some(data);
        self
    }

    pub fn metadata(mut self, metadata: Vec<u8>) -> Self {
        self.metadata = Some(metadata);
        self
    }

    pub fn expires_at(mut self, block: u64) -> Self {
        self.expires_at = Some(block);
        self
    }

    pub fn build(self) -> Result<CreateAttestationParamsV1, &'static str> {
        let (ax, ay) = self.attestor_pubkey.ok_or("attestor_pubkey not set")?.xy();
        Ok(CreateAttestationParamsV1 {
            proof: vec![],
            attestation_id: self.attestation_id.ok_or("attestation_id not set")?,
            attestor_pub_x: ax,
            attestor_pub_y: ay,
            claim_type: self.claim_type.ok_or("claim_type not set")?,
            claim_data: self.claim_data.ok_or("claim_data not set")?,
            metadata: self.metadata.unwrap_or_default(),
            expires_at: self.expires_at,
        })
    }
}

/// Builder for CreateClaimV1 params
#[derive(Default)]
pub struct CreateClaimBuilder {
    claim_id: Option<ClaimId>,
    attestation_id: Option<AttestationId>,
    claimant_pubkey: Option<PublicKey>,
    predicate: Option<Predicate>,
    evidence_commitment: Option<Vec<u8>>,
    revealed_result: Option<Vec<u8>>,
}

impl CreateClaimBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn claim_id(mut self, id: ClaimId) -> Self {
        self.claim_id = Some(id);
        self
    }

    pub fn attestation_id(mut self, id: AttestationId) -> Self {
        self.attestation_id = Some(id);
        self
    }

    pub fn claimant_pubkey(mut self, pubkey: PublicKey) -> Self {
        self.claimant_pubkey = Some(pubkey);
        self
    }

    pub fn predicate(mut self, predicate: Predicate) -> Self {
        self.predicate = Some(predicate);
        self
    }

    pub fn evidence_commitment(mut self, commitment: Vec<u8>) -> Self {
        self.evidence_commitment = Some(commitment);
        self
    }

    pub fn revealed_result(mut self, result: Vec<u8>) -> Self {
        self.revealed_result = Some(result);
        self
    }

    pub fn build(self) -> Result<CreateClaimParamsV1, &'static str> {
        let (cx, cy) = self.claimant_pubkey.ok_or("claimant_pubkey not set")?.xy();
        Ok(CreateClaimParamsV1 {
            proof: vec![],
            claim_id: self.claim_id.ok_or("claim_id not set")?,
            attestation_id: self.attestation_id.ok_or("attestation_id not set")?,
            claimant_pub_x: cx,
            claimant_pub_y: cy,
            predicate: self.predicate.ok_or("predicate not set")?,
            evidence_commitment: self.evidence_commitment.ok_or("evidence_commitment not set")?,
            revealed_result: self.revealed_result.unwrap_or_default(),
        })
    }
}

/// Builder for VerifyClaimV1 params
#[derive(Default)]
pub struct VerifyClaimBuilder {
    claim_id: Option<ClaimId>,
    attestation_id: Option<AttestationId>,
    evidence_commitment: Option<pallas::Base>,
    revealed_result: Option<pallas::Base>,
}

impl VerifyClaimBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn claim_id(mut self, id: ClaimId) -> Self {
        self.claim_id = Some(id);
        self
    }

    pub fn attestation_id(mut self, id: AttestationId) -> Self {
        self.attestation_id = Some(id);
        self
    }

    pub fn evidence_commitment(mut self, commitment: pallas::Base) -> Self {
        self.evidence_commitment = Some(commitment);
        self
    }

    pub fn revealed_result(mut self, result: pallas::Base) -> Self {
        self.revealed_result = Some(result);
        self
    }

    pub fn build(self) -> Result<VerifyClaimParamsV1, &'static str> {
        Ok(VerifyClaimParamsV1 {
            claim_id: self.claim_id.ok_or("claim_id not set")?,
            attestation_id: self.attestation_id.ok_or("attestation_id not set")?,
            evidence_commitment: self.evidence_commitment.ok_or("evidence_commitment not set")?,
            revealed_result: self.revealed_result.ok_or("revealed_result not set")?,
        })
    }
}

/// Builder for ConsumeClaimV1 params
#[derive(Default)]
pub struct ConsumeClaimBuilder {
    claim_id: Option<ClaimId>,
    attestation_id: Option<AttestationId>,
    claimant_pubkey: Option<PublicKey>,
    nullifier: Option<pallas::Base>,
}

impl ConsumeClaimBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn claim_id(mut self, id: ClaimId) -> Self {
        self.claim_id = Some(id);
        self
    }

    pub fn attestation_id(mut self, id: AttestationId) -> Self {
        self.attestation_id = Some(id);
        self
    }

    pub fn claimant_pubkey(mut self, pubkey: PublicKey) -> Self {
        self.claimant_pubkey = Some(pubkey);
        self
    }

    pub fn nullifier(mut self, nullifier: pallas::Base) -> Self {
        self.nullifier = Some(nullifier);
        self
    }

    pub fn build(self) -> Result<ConsumeClaimParamsV1, &'static str> {
        let (cx, cy) = self.claimant_pubkey.ok_or("claimant_pubkey not set")?.xy();
        Ok(ConsumeClaimParamsV1 {
            claim_id: self.claim_id.ok_or("claim_id not set")?,
            attestation_id: self.attestation_id.ok_or("attestation_id not set")?,
            claimant_pub_x: cx,
            claimant_pub_y: cy,
            nullifier: self.nullifier.ok_or("nullifier not set")?,
        })
    }
}

/// Builder for ValidateClaimV1 params
#[derive(Default)]
pub struct ValidateClaimBuilder {
    claim_id: Option<ClaimId>,
    attestation_id: Option<AttestationId>,
    evidence: Option<Vec<pallas::Base>>,
}

impl ValidateClaimBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn claim_id(mut self, id: ClaimId) -> Self {
        self.claim_id = Some(id);
        self
    }

    pub fn attestation_id(mut self, id: AttestationId) -> Self {
        self.attestation_id = Some(id);
        self
    }

    pub fn evidence(mut self, evidence: Vec<pallas::Base>) -> Self {
        self.evidence = Some(evidence);
        self
    }

    pub fn build(self) -> Result<ValidateClaimParamsV1, &'static str> {
        Ok(ValidateClaimParamsV1 {
            claim_id: self.claim_id.ok_or("claim_id not set")?,
            attestation_id: self.attestation_id.ok_or("attestation_id not set")?,
            evidence: self.evidence.ok_or("evidence not set")?,
        })
    }
}

/// Builder for RevokeAttestationV1 params
#[derive(Default)]
pub struct RevokeAttestationBuilder {
    attestation_id: Option<AttestationId>,
    attestor_pubkey: Option<PublicKey>,
}

impl RevokeAttestationBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn attestation_id(mut self, id: AttestationId) -> Self {
        self.attestation_id = Some(id);
        self
    }

    pub fn attestor_pubkey(mut self, pubkey: PublicKey) -> Self {
        self.attestor_pubkey = Some(pubkey);
        self
    }

    pub fn build(self) -> Result<RevokeAttestationParamsV1, &'static str> {
        let (ax, ay) = self.attestor_pubkey.ok_or("attestor_pubkey not set")?.xy();
        Ok(RevokeAttestationParamsV1 {
            attestation_id: self.attestation_id.ok_or("attestation_id not set")?,
            attestor_pub_x: ax,
            attestor_pub_y: ay,
        })
    }
}

/// Builder for ExpireAttestationV1 params
#[derive(Default)]
pub struct ExpireAttestationBuilder {
    attestation_id: Option<AttestationId>,
}

impl ExpireAttestationBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn attestation_id(mut self, id: AttestationId) -> Self {
        self.attestation_id = Some(id);
        self
    }

    pub fn build(self) -> Result<ExpireAttestationParamsV1, &'static str> {
        Ok(ExpireAttestationParamsV1 {
            attestation_id: self.attestation_id.ok_or("attestation_id not set")?,
        })
    }
}