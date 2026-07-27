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

//! Labor Market Contract Data Structures

use dwow_sdk::{
    crypto::pasta_prelude::PrimeField,
    error::ContractError,
    pasta::pallas,
};

/// Delivery type for job work
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DeliveryType {
    /// Generic deliverable: hash of a zip file
    Generic = 0,
    /// Git deliverable: commit hash
    Git = 1,
}

impl Default for DeliveryType {
    fn default() -> Self {
        Self::Generic
    }
}

// ============================================================================
// MILESTONE (For multi-stage jobs with time-weighted payments)
// ============================================================================

/// A milestone in a multi-stage job
/// Each milestone has its own deadline and payment amount
#[derive(Debug, Clone)]
pub struct Milestone {
    /// Milestone index (0-based)
    pub index: u32,
    /// Payment amount for this milestone
    pub payment_amount: u64,
    /// Deadline block for this milestone
    pub deadline_block: u64,
    /// Whether this milestone has been completed
    pub completed: bool,
    /// Block when completed
    pub completed_at_block: Option<u64>,
}

impl Default for Milestone {
    fn default() -> Self {
        Self { index: 0, payment_amount: 0, deadline_block: 0, completed: false, completed_at_block: None }
    }
}

impl Milestone {
    pub fn encode(&self) -> Vec<u8> {
        let cap = 22 + match &self.completed_at_block { Some(_) => 8, None => 0 };
        let mut b = Vec::with_capacity(cap);
        b.extend_from_slice(&self.index.to_le_bytes());
        b.extend_from_slice(&self.payment_amount.to_le_bytes());
        b.extend_from_slice(&self.deadline_block.to_le_bytes());
        b.push(self.completed as u8);
        match &self.completed_at_block {
            None => b.push(0u8),
            Some(v) => {
                b.push(1u8);
                b.extend_from_slice(&v.to_le_bytes());
            }
        }
        b
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 22 {
            return Err(ContractError::IoError(format!(
                "Milestone: expected at least 22 bytes, got {}",
                data.len()
            )));
        }
        let index = u32::from_le_bytes(data[0..4].try_into().unwrap());
        let payment_amount = u64::from_le_bytes(data[4..12].try_into().unwrap());
        let deadline_block = u64::from_le_bytes(data[12..20].try_into().unwrap());
        let completed = data[20] != 0;
        let (completed_at_block, advance): (Option<u64>, usize) = match data[21] {
            0 => (None, 1),
            1 => {
                if data.len() < 30 {
                    return Err(ContractError::IoError("Milestone: truncated completed_at_block".into()));
                }
                let v = Some(u64::from_le_bytes(data[22..30].try_into().unwrap()));
                (v, 9)
            }
            tag => return Err(ContractError::IoError(format!("Milestone: invalid completed_at_block tag {}", tag))),
        };
        let pos = 21 + advance;
        if pos != data.len() {
            return Err(ContractError::IoError(format!(
                "Milestone: expected {} bytes consumed, {} remaining",
                data.len(), data.len() - pos
            )));
        }
        Ok(Milestone { index, payment_amount, deadline_block, completed, completed_at_block })
    }
}

/// Job state in the state machine
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum JobState {
    /// Job created, awaiting worker acceptance
    Created = 0,
    /// Worker accepted, working on deliverable
    InProgress = 1,
    /// Work delivered, awaiting confirmation
    Delivered = 2,
    /// Employer confirmed, payment released
    Confirmed = 3,
    /// Escalated to DAO for dispute resolution
    Disputed = 4,
    /// Timeout, employer refunded
    Refunded = 5,
    /// Cancelled before acceptance
    Cancelled = 6,
}

impl Default for JobState {
    fn default() -> Self {
        Self::Created
    }
}

impl TryFrom<u8> for JobState {
    type Error = ContractError;

    fn try_from(b: u8) -> Result<Self, Self::Error> {
        match b {
            0 => Ok(JobState::Created),
            1 => Ok(JobState::InProgress),
            2 => Ok(JobState::Delivered),
            3 => Ok(JobState::Confirmed),
            4 => Ok(JobState::Disputed),
            5 => Ok(JobState::Refunded),
            6 => Ok(JobState::Cancelled),
            _ => Err(ContractError::IoError(format!("Invalid JobState: {}", b))),
        }
    }
}

/// A job posting in the labor market
#[derive(Debug, Clone)]
pub struct Job {
    /// Unique job identifier (Poseidon hash commitment)
    pub id: pallas::Base,
    /// Employer's public key
    pub employer_pubkey: [pallas::Base; 2],
    /// Worker's public key (set when accepted)
    pub worker_pubkey: Option<[pallas::Base; 2]>,
    /// Attestation ID for deliverable verification (references attestation contract)
    pub attestation_id: pallas::Base,
    /// Type of deliverable (generic or git)
    pub delivery_type: DeliveryType,
    /// Payment amount
    pub payment_amount: u64,
    /// Token being paid
    pub payment_token: pallas::Base,
    /// Payment commitment (Pedersen)
    pub payment_commit: [pallas::Base; 2],
    /// Block by which work must be delivered
    pub deadline_block: u64,
    /// Current job state
    pub state: JobState,
    /// DAO-Escrow bulla for dispute resolution
    pub dao_escrow_bulla: Option<pallas::Base>,
    /// Milestones for this job (empty if no milestones)
    pub milestones: Vec<Milestone>,
    /// Current milestone index (0-based)
    pub current_milestone: u32,
    /// Accumulated payment released so far
    pub released_payment: u64,
    /// Required capability ID for workers (None = any worker can accept)
    pub required_capability_id: Option<[u8; 32]>,
    /// Required DAG ID for multi-path qualification (None = no DAG requirement)
    pub required_dag_id: Option<[u8; 32]>,
}

impl Job {
    pub fn encode(&self) -> Vec<u8> {
        // Fixed prefix: id(32)+employer(64)+worker_tag(1)+attestation(32)+delivery(1)
        // +payment_amount(8)+payment_token(32)+payment_commit(64)+deadline(8)+state(1)
        // +dao_tag(1)+milestones_count(1)+current_milestone(4)+released_payment(8)
        // +cap_tag(1)+dag_tag(1) = 259 + worker_pubkey_opt + dao_opt + milestones + cap_opt + dag_opt
        let cap = 259
            + match &self.worker_pubkey { Some(_) => 64, None => 0 }
            + match &self.dao_escrow_bulla { Some(_) => 32, None => 0 }
            + match &self.required_capability_id { Some(_) => 32, None => 0 }
            + match &self.required_dag_id { Some(_) => 32, None => 0 }
            + self.milestones.iter().map(|m| {
                22 + match &m.completed_at_block { Some(_) => 8, None => 0 }
            }).sum::<usize>();
        let mut b = Vec::with_capacity(cap);
        b.extend_from_slice(&self.id.to_repr());
        b.extend_from_slice(&self.employer_pubkey[0].to_repr());
        b.extend_from_slice(&self.employer_pubkey[1].to_repr());
        match &self.worker_pubkey {
            None => b.push(0u8),
            Some(pk) => {
                b.push(1u8);
                b.extend_from_slice(&pk[0].to_repr());
                b.extend_from_slice(&pk[1].to_repr());
            }
        }
        b.extend_from_slice(&self.attestation_id.to_repr());
        b.push(self.delivery_type as u8);
        b.extend_from_slice(&self.payment_amount.to_le_bytes());
        b.extend_from_slice(&self.payment_token.to_repr());
        b.extend_from_slice(&self.payment_commit[0].to_repr());
        b.extend_from_slice(&self.payment_commit[1].to_repr());
        b.extend_from_slice(&self.deadline_block.to_le_bytes());
        b.push(self.state as u8);
        match &self.dao_escrow_bulla {
            None => b.push(0u8),
            Some(v) => {
                b.push(1u8);
                b.extend_from_slice(&v.to_repr());
            }
        }
        b.push(self.milestones.len() as u8);
        for m in &self.milestones {
            b.extend_from_slice(&m.encode());
        }
        b.extend_from_slice(&self.current_milestone.to_le_bytes());
        b.extend_from_slice(&self.released_payment.to_le_bytes());
        match &self.required_capability_id {
            None => b.push(0u8),
            Some(v) => {
                b.push(1u8);
                b.extend_from_slice(v);
            }
        }
        match &self.required_dag_id {
            None => b.push(0u8),
            Some(v) => {
                b.push(1u8);
                b.extend_from_slice(v);
            }
        }
        b
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        // Minimum: fixed prefix 259 bytes (with all Options = None, no milestones)
        if data.len() < 259 {
            return Err(ContractError::IoError(format!(
                "Job: expected at least 259 bytes, got {}",
                data.len()
            )));
        }
        let id = pallas::Base::from_repr(data[0..32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("Job: invalid id".into()))?;
        let emp_x = pallas::Base::from_repr(data[32..64].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("Job: invalid employer_pubkey[0]".into()))?;
        let emp_y = pallas::Base::from_repr(data[64..96].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("Job: invalid employer_pubkey[1]".into()))?;
        let employer_pubkey = [emp_x, emp_y];
        let (worker_pubkey, advance1): (Option<[pallas::Base; 2]>, usize) = match data[96] {
            0 => (None, 1),
            1 => {
                if data.len() < 161 {
                    return Err(ContractError::IoError("Job: truncated worker_pubkey".into()));
                }
                let wx = pallas::Base::from_repr(data[97..129].try_into().unwrap())
                    .into_option()
                    .ok_or_else(|| ContractError::IoError("Job: invalid worker_pubkey[0]".into()))?;
                let wy = pallas::Base::from_repr(data[129..161].try_into().unwrap())
                    .into_option()
                    .ok_or_else(|| ContractError::IoError("Job: invalid worker_pubkey[1]".into()))?;
                (Some([wx, wy]), 65)
            }
            tag => return Err(ContractError::IoError(format!("Job: invalid worker_pubkey tag {}", tag))),
        };
        let pos = 96 + advance1;
        if pos + 32 > data.len() {
            return Err(ContractError::IoError("Job: truncated attestation_id".into()));
        }
        let attestation_id = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("Job: invalid attestation_id".into()))?;
        let pos = pos + 32;
        if pos + 1 > data.len() {
            return Err(ContractError::IoError("Job: truncated delivery_type".into()));
        }
        let delivery_type = match data[pos] {
            0 => DeliveryType::Generic,
            1 => DeliveryType::Git,
            _ => return Err(ContractError::IoError("Job: invalid delivery_type".into())),
        };
        let pos = pos + 1;
        if pos + 8 > data.len() {
            return Err(ContractError::IoError("Job: truncated payment_amount".into()));
        }
        let payment_amount = u64::from_le_bytes(data[pos..pos+8].try_into().unwrap());
        let pos = pos + 8;
        if pos + 32 > data.len() {
            return Err(ContractError::IoError("Job: truncated payment_token".into()));
        }
        let payment_token = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("Job: invalid payment_token".into()))?;
        let pos = pos + 32;
        if pos + 64 > data.len() {
            return Err(ContractError::IoError("Job: truncated payment_commit".into()));
        }
        let pc_x = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("Job: invalid payment_commit[0]".into()))?;
        let pc_y = pallas::Base::from_repr(data[pos+32..pos+64].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("Job: invalid payment_commit[1]".into()))?;
        let payment_commit = [pc_x, pc_y];
        let pos = pos + 64;
        if pos + 8 > data.len() {
            return Err(ContractError::IoError("Job: truncated deadline_block".into()));
        }
        let deadline_block = u64::from_le_bytes(data[pos..pos+8].try_into().unwrap());
        let pos = pos + 8;
        if pos + 1 > data.len() {
            return Err(ContractError::IoError("Job: truncated state".into()));
        }
        let state = JobState::try_from(data[pos])?;
        let pos = pos + 1;
        // dao_escrow_bulla: Option<pallas::Base>
        if pos + 1 > data.len() {
            return Err(ContractError::IoError("Job: truncated dao_escrow_bulla tag".into()));
        }
        let (dao_escrow_bulla, advance2): (Option<pallas::Base>, usize) = match data[pos] {
            0 => (None, 1),
            1 => {
                if pos + 1 + 32 > data.len() {
                    return Err(ContractError::IoError("Job: truncated dao_escrow_bulla".into()));
                }
                let v = Some(pallas::Base::from_repr(data[pos+1..pos+33].try_into().unwrap())
                    .into_option()
                    .ok_or_else(|| ContractError::IoError("Job: invalid dao_escrow_bulla".into()))?);
                (v, 33)
            }
            tag => return Err(ContractError::IoError(format!("Job: invalid dao_escrow_bulla tag {}", tag))),
        };
        let mut pos = pos + advance2;
        // milestones: Vec<Milestone>
        if pos + 1 > data.len() {
            return Err(ContractError::IoError("Job: truncated milestones count".into()));
        }
        let milestone_count = data[pos] as usize;
        pos += 1;
        let mut milestones = Vec::with_capacity(milestone_count);
        for _ in 0..milestone_count {
            // Each milestone starts with 21 bytes fixed + optional tag
            if pos + 22 > data.len() {
                return Err(ContractError::IoError("Job: truncated milestone".into()));
            }
            let m_opt_tag = data[pos + 21];
            let m_len = if m_opt_tag == 0 { 22 } else { 30 };
            if pos + m_len > data.len() {
                return Err(ContractError::IoError("Job: truncated milestone body".into()));
            }
            milestones.push(Milestone::decode(&data[pos..pos+m_len])?);
            pos += m_len;
        }
        // current_milestone + released_payment
        if pos + 4 > data.len() {
            return Err(ContractError::IoError("Job: truncated current_milestone".into()));
        }
        let current_milestone = u32::from_le_bytes(data[pos..pos+4].try_into().unwrap());
        pos += 4;
        if pos + 8 > data.len() {
            return Err(ContractError::IoError("Job: truncated released_payment".into()));
        }
        let released_payment = u64::from_le_bytes(data[pos..pos+8].try_into().unwrap());
        pos += 8;
        // required_capability_id: Option<[u8;32]>
        if pos + 1 > data.len() {
            return Err(ContractError::IoError("Job: truncated required_capability_id tag".into()));
        }
        let (required_capability_id, advance3): (Option<[u8;32]>, usize) = match data[pos] {
            0 => (None, 1),
            1 => {
                if pos + 1 + 32 > data.len() {
                    return Err(ContractError::IoError("Job: truncated required_capability_id".into()));
                }
                let mut arr = [0u8; 32];
                arr.copy_from_slice(&data[pos+1..pos+33]);
                (Some(arr), 33)
            }
            tag => return Err(ContractError::IoError(format!("Job: invalid required_capability_id tag {}", tag))),
        };
        pos += advance3;
        // required_dag_id: Option<[u8;32]>
        if pos + 1 > data.len() {
            return Err(ContractError::IoError("Job: truncated required_dag_id tag".into()));
        }
        let (required_dag_id, advance4): (Option<[u8;32]>, usize) = match data[pos] {
            0 => (None, 1),
            1 => {
                if pos + 1 + 32 > data.len() {
                    return Err(ContractError::IoError("Job: truncated required_dag_id".into()));
                }
                let mut arr = [0u8; 32];
                arr.copy_from_slice(&data[pos+1..pos+33]);
                (Some(arr), 33)
            }
            tag => return Err(ContractError::IoError(format!("Job: invalid required_dag_id tag {}", tag))),
        };
        pos += advance4;

        if pos != data.len() {
            return Err(ContractError::IoError(format!(
                "Job: expected {} bytes consumed, {} remaining",
                data.len(), data.len() - pos
            )));
        }

        Ok(Job {
            id,
            employer_pubkey,
            worker_pubkey,
            attestation_id,
            delivery_type,
            payment_amount,
            payment_token,
            payment_commit,
            deadline_block,
            state,
            dao_escrow_bulla,
            milestones,
            current_milestone,
            released_payment,
            required_capability_id,
            required_dag_id,
        })
    }
}

/// Parameters for creating a new job
#[derive(Debug)]
pub struct CreateJobParamsV1 {
    /// ZK proof for job creation
    pub proof: Vec<u8>,
    /// Job ID (public input)
    pub job_id: pallas::Base,
    /// Employer's public key x coordinate
    pub employer_pub_x: pallas::Base,
    /// Employer's public key y coordinate
    pub employer_pub_y: pallas::Base,
    /// Attestation ID for deliverable verification (references attestation contract)
    pub attestation_id: pallas::Base,
    /// Type of deliverable (0 = Generic, 1 = Git)
    pub delivery_type: u8,
    /// Payment amount
    pub payment_amount: u64,
    /// Token being paid
    pub payment_token: pallas::Base,
    /// Payment commitment x coordinate
    pub payment_commit_x: pallas::Base,
    /// Payment commitment y coordinate
    pub payment_commit_y: pallas::Base,
}

impl CreateJobParamsV1 {
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 185 {
            return Err(ContractError::IoError(format!(
                "CreateJobParamsV1: expected at least 185 bytes, got {}",
                data.len()
            )));
        }
        let proof_len = data[0] as usize;
        let mut pos = 1usize;
        if pos + proof_len > data.len() {
            return Err(ContractError::IoError("CreateJobParamsV1: truncated proof".into()));
        }
        let proof = data[pos..pos+proof_len].to_vec();
        pos += proof_len;
        // job_id(32)+emp_x(32)+emp_y(32)+attestation_id(32)+delivery_type(1)+payment_amount(8)+payment_token(32)+pc_x(32)+pc_y(32) = 233
        if pos + 233 > data.len() {
            return Err(ContractError::IoError("CreateJobParamsV1: truncated fixed fields".into()));
        }
        let job_id = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("CreateJobParamsV1: invalid job_id".into()))?;
        pos += 32;
        let employer_pub_x = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("CreateJobParamsV1: invalid employer_pub_x".into()))?;
        pos += 32;
        let employer_pub_y = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("CreateJobParamsV1: invalid employer_pub_y".into()))?;
        pos += 32;
        let attestation_id = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("CreateJobParamsV1: invalid attestation_id".into()))?;
        pos += 32;
        let delivery_type = data[pos];
        pos += 1;
        let payment_amount = u64::from_le_bytes(data[pos..pos+8].try_into().unwrap());
        pos += 8;
        let payment_token = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("CreateJobParamsV1: invalid payment_token".into()))?;
        pos += 32;
        let payment_commit_x = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("CreateJobParamsV1: invalid payment_commit_x".into()))?;
        pos += 32;
        let payment_commit_y = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("CreateJobParamsV1: invalid payment_commit_y".into()))?;
        pos += 32;

        if pos != data.len() {
            return Err(ContractError::IoError(format!(
                "CreateJobParamsV1: expected {} bytes consumed, {} remaining",
                data.len(), data.len() - pos
            )));
        }

        Ok(CreateJobParamsV1 {
            proof,
            job_id,
            employer_pub_x,
            employer_pub_y,
            attestation_id,
            delivery_type,
            payment_amount,
            payment_token,
            payment_commit_x,
            payment_commit_y,
        })
    }
}

/// Parameters for accepting a job
#[derive(Debug)]
pub struct AcceptJobParamsV1 {
    /// ZK proof for job acceptance
    pub proof: Vec<u8>,
    /// Job ID being accepted
    pub job_id: pallas::Base,
    /// Worker's public key x coordinate
    pub worker_pub_x: pallas::Base,
    /// Worker's public key y coordinate
    pub worker_pub_y: pallas::Base,
}

impl AcceptJobParamsV1 {
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 97 {
            return Err(ContractError::IoError(format!(
                "AcceptJobParamsV1: expected at least 97 bytes, got {}",
                data.len()
            )));
        }
        let proof_len = data[0] as usize;
        let mut pos = 1usize;
        if pos + proof_len > data.len() {
            return Err(ContractError::IoError("AcceptJobParamsV1: truncated proof".into()));
        }
        let proof = data[pos..pos+proof_len].to_vec();
        pos += proof_len;
        if pos + 96 > data.len() {
            return Err(ContractError::IoError("AcceptJobParamsV1: truncated fixed fields".into()));
        }
        let job_id = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("AcceptJobParamsV1: invalid job_id".into()))?;
        pos += 32;
        let worker_pub_x = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("AcceptJobParamsV1: invalid worker_pub_x".into()))?;
        pos += 32;
        let worker_pub_y = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("AcceptJobParamsV1: invalid worker_pub_y".into()))?;
        pos += 32;
        if pos != data.len() {
            return Err(ContractError::IoError(format!(
                "AcceptJobParamsV1: expected {} bytes consumed, {} remaining",
                data.len(), data.len() - pos
            )));
        }
        Ok(AcceptJobParamsV1 { proof, job_id, worker_pub_x, worker_pub_y })
    }
}

/// Parameters for submitting a generic deliverable (zip hash)
#[derive(Debug)]
pub struct SubmitDeliverableParamsV1 {
    /// ZK proof for deliverable submission
    pub proof: Vec<u8>,
    /// Job ID being completed
    pub job_id: pallas::Base,
    /// Attestation claim ID (from attestation.create_claim)
    pub claim_id: pallas::Base,
    /// Worker's public key x coordinate
    pub worker_pub_x: pallas::Base,
    /// Worker's public key y coordinate
    pub worker_pub_y: pallas::Base,
    /// Nullifier for preventing double-submission
    pub spent_nullifier: pallas::Base,
}

impl SubmitDeliverableParamsV1 {
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 161 {
            return Err(ContractError::IoError(format!(
                "SubmitDeliverableParamsV1: expected at least 161 bytes, got {}",
                data.len()
            )));
        }
        let proof_len = data[0] as usize;
        let mut pos = 1usize;
        if pos + proof_len > data.len() {
            return Err(ContractError::IoError("SubmitDeliverableParamsV1: truncated proof".into()));
        }
        let proof = data[pos..pos+proof_len].to_vec();
        pos += proof_len;
        if pos + 160 > data.len() {
            return Err(ContractError::IoError("SubmitDeliverableParamsV1: truncated fixed fields".into()));
        }
        let job_id = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("SubmitDeliverableParamsV1: invalid job_id".into()))?;
        pos += 32;
        let claim_id = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("SubmitDeliverableParamsV1: invalid claim_id".into()))?;
        pos += 32;
        let worker_pub_x = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("SubmitDeliverableParamsV1: invalid worker_pub_x".into()))?;
        pos += 32;
        let worker_pub_y = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("SubmitDeliverableParamsV1: invalid worker_pub_y".into()))?;
        pos += 32;
        let spent_nullifier = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("SubmitDeliverableParamsV1: invalid spent_nullifier".into()))?;
        pos += 32;
        if pos != data.len() {
            return Err(ContractError::IoError(format!(
                "SubmitDeliverableParamsV1: expected {} bytes consumed, {} remaining",
                data.len(), data.len() - pos
            )));
        }
        Ok(SubmitDeliverableParamsV1 { proof, job_id, claim_id, worker_pub_x, worker_pub_y, spent_nullifier })
    }
}

/// Parameters for submitting a git deliverable (commit hash)
#[derive(Debug)]
pub struct SubmitGitDeliverableParamsV1 {
    /// ZK proof for git deliverable submission
    pub proof: Vec<u8>,
    /// Job ID being completed
    pub job_id: pallas::Base,
    /// Attestation claim ID (from attestation.create_claim)
    pub claim_id: pallas::Base,
    /// Worker's public key x coordinate
    pub worker_pub_x: pallas::Base,
    /// Worker's public key y coordinate
    pub worker_pub_y: pallas::Base,
    /// Nullifier for preventing double-submission
    pub spent_nullifier: pallas::Base,
}

impl SubmitGitDeliverableParamsV1 {
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 161 {
            return Err(ContractError::IoError(format!(
                "SubmitGitDeliverableParamsV1: expected at least 161 bytes, got {}",
                data.len()
            )));
        }
        let proof_len = data[0] as usize;
        let mut pos = 1usize;
        if pos + proof_len > data.len() {
            return Err(ContractError::IoError("SubmitGitDeliverableParamsV1: truncated proof".into()));
        }
        let proof = data[pos..pos+proof_len].to_vec();
        pos += proof_len;
        if pos + 160 > data.len() {
            return Err(ContractError::IoError("SubmitGitDeliverableParamsV1: truncated fixed fields".into()));
        }
        let job_id = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("SubmitGitDeliverableParamsV1: invalid job_id".into()))?;
        pos += 32;
        let claim_id = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("SubmitGitDeliverableParamsV1: invalid claim_id".into()))?;
        pos += 32;
        let worker_pub_x = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("SubmitGitDeliverableParamsV1: invalid worker_pub_x".into()))?;
        pos += 32;
        let worker_pub_y = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("SubmitGitDeliverableParamsV1: invalid worker_pub_y".into()))?;
        pos += 32;
        let spent_nullifier = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("SubmitGitDeliverableParamsV1: invalid spent_nullifier".into()))?;
        pos += 32;
        if pos != data.len() {
            return Err(ContractError::IoError(format!(
                "SubmitGitDeliverableParamsV1: expected {} bytes consumed, {} remaining",
                data.len(), data.len() - pos
            )));
        }
        Ok(SubmitGitDeliverableParamsV1 { proof, job_id, claim_id, worker_pub_x, worker_pub_y, spent_nullifier })
    }
}

/// Parameters for confirming delivery and releasing payment
#[derive(Debug)]
pub struct ConfirmDeliveryParamsV1 {
    /// ZK proof for confirmation
    pub proof: Vec<u8>,
    /// Job ID being confirmed
    pub job_id: pallas::Base,
    /// Employer's public key x coordinate
    pub employer_pub_x: pallas::Base,
    /// Employer's public key y coordinate
    pub employer_pub_y: pallas::Base,
    /// Nullifier for release authorization
    pub spent_nullifier: pallas::Base,
}

impl ConfirmDeliveryParamsV1 {
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 129 {
            return Err(ContractError::IoError(format!(
                "ConfirmDeliveryParamsV1: expected at least 129 bytes, got {}",
                data.len()
            )));
        }
        let proof_len = data[0] as usize;
        let mut pos = 1usize;
        if pos + proof_len > data.len() {
            return Err(ContractError::IoError("ConfirmDeliveryParamsV1: truncated proof".into()));
        }
        let proof = data[pos..pos+proof_len].to_vec();
        pos += proof_len;
        if pos + 128 > data.len() {
            return Err(ContractError::IoError("ConfirmDeliveryParamsV1: truncated fixed fields".into()));
        }
        let job_id = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("ConfirmDeliveryParamsV1: invalid job_id".into()))?;
        pos += 32;
        let employer_pub_x = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("ConfirmDeliveryParamsV1: invalid employer_pub_x".into()))?;
        pos += 32;
        let employer_pub_y = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("ConfirmDeliveryParamsV1: invalid employer_pub_y".into()))?;
        pos += 32;
        let spent_nullifier = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("ConfirmDeliveryParamsV1: invalid spent_nullifier".into()))?;
        pos += 32;
        if pos != data.len() {
            return Err(ContractError::IoError(format!(
                "ConfirmDeliveryParamsV1: expected {} bytes consumed, {} remaining",
                data.len(), data.len() - pos
            )));
        }
        Ok(ConfirmDeliveryParamsV1 { proof, job_id, employer_pub_x, employer_pub_y, spent_nullifier })
    }
}

/// Parameters for escalating to DAO dispute resolution
#[derive(Debug)]
pub struct DisputeParamsV1 {
    /// ZK proof for dispute
    pub proof: Vec<u8>,
    /// Job ID being disputed
    pub job_id: pallas::Base,
    /// Disputer's public key x coordinate
    pub disputer_pub_x: pallas::Base,
    /// Disputer's public key y coordinate
    pub disputer_pub_y: pallas::Base,
    /// DAO-Escrow handling the dispute
    pub dao_escrow_bulla: pallas::Base,
    /// Nullifier for dispute
    pub spent_nullifier: pallas::Base,
}

impl DisputeParamsV1 {
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 161 {
            return Err(ContractError::IoError(format!(
                "DisputeParamsV1: expected at least 161 bytes, got {}",
                data.len()
            )));
        }
        let proof_len = data[0] as usize;
        let mut pos = 1usize;
        if pos + proof_len > data.len() {
            return Err(ContractError::IoError("DisputeParamsV1: truncated proof".into()));
        }
        let proof = data[pos..pos+proof_len].to_vec();
        pos += proof_len;
        if pos + 160 > data.len() {
            return Err(ContractError::IoError("DisputeParamsV1: truncated fixed fields".into()));
        }
        let job_id = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("DisputeParamsV1: invalid job_id".into()))?;
        pos += 32;
        let disputer_pub_x = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("DisputeParamsV1: invalid disputer_pub_x".into()))?;
        pos += 32;
        let disputer_pub_y = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("DisputeParamsV1: invalid disputer_pub_y".into()))?;
        pos += 32;
        let dao_escrow_bulla = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("DisputeParamsV1: invalid dao_escrow_bulla".into()))?;
        pos += 32;
        let spent_nullifier = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("DisputeParamsV1: invalid spent_nullifier".into()))?;
        pos += 32;
        if pos != data.len() {
            return Err(ContractError::IoError(format!(
                "DisputeParamsV1: expected {} bytes consumed, {} remaining",
                data.len(), data.len() - pos
            )));
        }
        Ok(DisputeParamsV1 { proof, job_id, disputer_pub_x, disputer_pub_y, dao_escrow_bulla, spent_nullifier })
    }
}

/// Parameters for timeout refund
#[derive(Debug)]
pub struct RefundParamsV1 {
    /// ZK proof for refund
    pub proof: Vec<u8>,
    /// Job ID being refunded
    pub job_id: pallas::Base,
    /// Employer's public key x coordinate
    pub employer_pub_x: pallas::Base,
    /// Employer's public key y coordinate
    pub employer_pub_y: pallas::Base,
    /// Milestone count (for partial refund calculation)
    pub milestone_count: u64,
    /// Completed payment amount so far
    pub completed_payment: u64,
    /// Amount to be refunded
    pub refund_amount: u64,
    /// Nullifier for refund authorization
    pub spent_nullifier: pallas::Base,
}

impl RefundParamsV1 {
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 153 {
            return Err(ContractError::IoError(format!(
                "RefundParamsV1: expected at least 153 bytes, got {}",
                data.len()
            )));
        }
        let proof_len = data[0] as usize;
        let mut pos = 1usize;
        if pos + proof_len > data.len() {
            return Err(ContractError::IoError("RefundParamsV1: truncated proof".into()));
        }
        let proof = data[pos..pos+proof_len].to_vec();
        pos += proof_len;
        // job_id(32)+emp_x(32)+emp_y(32)+milestone_count(8)+completed_payment(8)+refund_amount(8)+spent_nullifier(32)=152
        if pos + 152 > data.len() {
            return Err(ContractError::IoError("RefundParamsV1: truncated fixed fields".into()));
        }
        let job_id = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("RefundParamsV1: invalid job_id".into()))?;
        pos += 32;
        let employer_pub_x = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("RefundParamsV1: invalid employer_pub_x".into()))?;
        pos += 32;
        let employer_pub_y = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("RefundParamsV1: invalid employer_pub_y".into()))?;
        pos += 32;
        let milestone_count = u64::from_le_bytes(data[pos..pos+8].try_into().unwrap());
        pos += 8;
        let completed_payment = u64::from_le_bytes(data[pos..pos+8].try_into().unwrap());
        pos += 8;
        let refund_amount = u64::from_le_bytes(data[pos..pos+8].try_into().unwrap());
        pos += 8;
        let spent_nullifier = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("RefundParamsV1: invalid spent_nullifier".into()))?;
        pos += 32;
        if pos != data.len() {
            return Err(ContractError::IoError(format!(
                "RefundParamsV1: expected {} bytes consumed, {} remaining",
                data.len(), data.len() - pos
            )));
        }
        Ok(RefundParamsV1 { proof, job_id, employer_pub_x, employer_pub_y, milestone_count, completed_payment, refund_amount, spent_nullifier })
    }
}

/// Parameters for cancelling a job before acceptance
#[derive(Debug)]
pub struct CancelJobParamsV1 {
    /// ZK proof for cancellation
    pub proof: Vec<u8>,
    /// Job ID being cancelled
    pub job_id: pallas::Base,
    /// Employer's public key x coordinate
    pub employer_pub_x: pallas::Base,
    /// Employer's public key y coordinate
    pub employer_pub_y: pallas::Base,
}

impl CancelJobParamsV1 {
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 97 {
            return Err(ContractError::IoError(format!(
                "CancelJobParamsV1: expected at least 97 bytes, got {}",
                data.len()
            )));
        }
        let proof_len = data[0] as usize;
        let mut pos = 1usize;
        if pos + proof_len > data.len() {
            return Err(ContractError::IoError("CancelJobParamsV1: truncated proof".into()));
        }
        let proof = data[pos..pos+proof_len].to_vec();
        pos += proof_len;
        if pos + 96 > data.len() {
            return Err(ContractError::IoError("CancelJobParamsV1: truncated fixed fields".into()));
        }
        let job_id = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("CancelJobParamsV1: invalid job_id".into()))?;
        pos += 32;
        let employer_pub_x = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("CancelJobParamsV1: invalid employer_pub_x".into()))?;
        pos += 32;
        let employer_pub_y = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("CancelJobParamsV1: invalid employer_pub_y".into()))?;
        pos += 32;
        if pos != data.len() {
            return Err(ContractError::IoError(format!(
                "CancelJobParamsV1: expected {} bytes consumed, {} remaining",
                data.len(), data.len() - pos
            )));
        }
        Ok(CancelJobParamsV1 { proof, job_id, employer_pub_x, employer_pub_y })
    }
}

// ============================================================================
// MILESTONE-ENHANCED PARAMETERS (For jobs with milestones)
// ============================================================================

/// Parameters for creating a job with milestones
#[derive(Debug)]
pub struct CreateJobWithMilestonesParamsV1 {
    /// ZK proof for job creation
    pub proof: Vec<u8>,
    /// Job ID (public input)
    pub job_id: pallas::Base,
    /// Employer's public key x coordinate
    pub employer_pub_x: pallas::Base,
    /// Employer's public key y coordinate
    pub employer_pub_y: pallas::Base,
    /// Attestation ID for deliverable verification
    pub attestation_id: pallas::Base,
    /// Type of deliverable (0 = Generic, 1 = Git)
    pub delivery_type: u8,
    /// Total payment amount (sum of all milestones)
    pub payment_amount: u64,
    /// Token being paid
    pub payment_token: pallas::Base,
    /// Payment commitment x coordinate
    pub payment_commit_x: pallas::Base,
    /// Payment commitment y coordinate
    pub payment_commit_y: pallas::Base,
    /// Overall deadline block
    pub deadline_block: u64,
    /// Number of milestones
    pub milestone_count: u32,
    /// Milestone definitions (payment amounts, deadlines)
    pub milestones: Vec<Milestone>,
}

impl CreateJobWithMilestonesParamsV1 {
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 247 {
            return Err(ContractError::IoError(format!(
                "CreateJobWithMilestonesParamsV1: expected at least 247 bytes, got {}",
                data.len()
            )));
        }
        let proof_len = data[0] as usize;
        let mut pos = 1usize;
        if pos + proof_len > data.len() {
            return Err(ContractError::IoError("CreateJobWithMilestonesParamsV1: truncated proof".into()));
        }
        let proof = data[pos..pos+proof_len].to_vec();
        pos += proof_len;
        // job_id(32)+emp_x(32)+emp_y(32)+attestation_id(32)+delivery_type(1)+payment_amount(8)+payment_token(32)+pc_x(32)+pc_y(32)+deadline_block(8)+milestone_count(4)=277
        // But we said min 247... let me recalculate. Fixed (after proof): 32+32+32+32+1+8+32+32+32+8+4 = 245
        if pos + 245 > data.len() {
            return Err(ContractError::IoError("CreateJobWithMilestonesParamsV1: truncated fixed fields".into()));
        }
        let job_id = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("CreateJobWithMilestonesParamsV1: invalid job_id".into()))?;
        pos += 32;
        let employer_pub_x = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("CreateJobWithMilestonesParamsV1: invalid employer_pub_x".into()))?;
        pos += 32;
        let employer_pub_y = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("CreateJobWithMilestonesParamsV1: invalid employer_pub_y".into()))?;
        pos += 32;
        let attestation_id = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("CreateJobWithMilestonesParamsV1: invalid attestation_id".into()))?;
        pos += 32;
        let delivery_type = data[pos];
        pos += 1;
        let payment_amount = u64::from_le_bytes(data[pos..pos+8].try_into().unwrap());
        pos += 8;
        let payment_token = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("CreateJobWithMilestonesParamsV1: invalid payment_token".into()))?;
        pos += 32;
        let payment_commit_x = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("CreateJobWithMilestonesParamsV1: invalid payment_commit_x".into()))?;
        pos += 32;
        let payment_commit_y = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("CreateJobWithMilestonesParamsV1: invalid payment_commit_y".into()))?;
        pos += 32;
        let deadline_block = u64::from_le_bytes(data[pos..pos+8].try_into().unwrap());
        pos += 8;
        let milestone_count = u32::from_le_bytes(data[pos..pos+4].try_into().unwrap()) as usize;
        pos += 4;
        let mut milestones = Vec::with_capacity(milestone_count);
        for _ in 0..milestone_count {
            if pos + 22 > data.len() {
                return Err(ContractError::IoError("CreateJobWithMilestonesParamsV1: truncated milestone".into()));
            }
            let m_opt_tag = data[pos + 21];
            let m_len = if m_opt_tag == 0 { 22 } else { 30 };
            if pos + m_len > data.len() {
                return Err(ContractError::IoError("CreateJobWithMilestonesParamsV1: truncated milestone body".into()));
            }
            milestones.push(Milestone::decode(&data[pos..pos+m_len])?);
            pos += m_len;
        }
        if pos != data.len() {
            return Err(ContractError::IoError(format!(
                "CreateJobWithMilestonesParamsV1: expected {} bytes consumed, {} remaining",
                data.len(), data.len() - pos
            )));
        }
        Ok(CreateJobWithMilestonesParamsV1 {
            proof,
            job_id,
            employer_pub_x,
            employer_pub_y,
            attestation_id,
            delivery_type,
            payment_amount,
            payment_token,
            payment_commit_x,
            payment_commit_y,
            deadline_block,
            milestone_count: milestone_count as u32,
            milestones,
        })
    }
}

/// Parameters for submitting a deliverable for a specific milestone
#[derive(Debug)]
pub struct SubmitMilestoneDeliverableParamsV1 {
    /// ZK proof for milestone deliverable submission
    pub proof: Vec<u8>,
    /// Job ID being completed
    pub job_id: pallas::Base,
    /// Milestone index being submitted
    pub milestone_index: u32,
    /// Attestation claim ID
    pub claim_id: pallas::Base,
    /// Worker's public key x coordinate
    pub worker_pub_x: pallas::Base,
    /// Worker's public key y coordinate
    pub worker_pub_y: pallas::Base,
    /// Nullifier for preventing double-submission
    pub spent_nullifier: pallas::Base,
}

impl SubmitMilestoneDeliverableParamsV1 {
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 165 {
            return Err(ContractError::IoError(format!(
                "SubmitMilestoneDeliverableParamsV1: expected at least 165 bytes, got {}",
                data.len()
            )));
        }
        let proof_len = data[0] as usize;
        let mut pos = 1usize;
        if pos + proof_len > data.len() {
            return Err(ContractError::IoError("SubmitMilestoneDeliverableParamsV1: truncated proof".into()));
        }
        let proof = data[pos..pos+proof_len].to_vec();
        pos += proof_len;
        // job_id(32)+milestone_index(4)+claim_id(32)+worker_pub_x(32)+worker_pub_y(32)+spent_nullifier(32)=164
        if pos + 164 > data.len() {
            return Err(ContractError::IoError("SubmitMilestoneDeliverableParamsV1: truncated fixed fields".into()));
        }
        let job_id = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("SubmitMilestoneDeliverableParamsV1: invalid job_id".into()))?;
        pos += 32;
        let milestone_index = u32::from_le_bytes(data[pos..pos+4].try_into().unwrap());
        pos += 4;
        let claim_id = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("SubmitMilestoneDeliverableParamsV1: invalid claim_id".into()))?;
        pos += 32;
        let worker_pub_x = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("SubmitMilestoneDeliverableParamsV1: invalid worker_pub_x".into()))?;
        pos += 32;
        let worker_pub_y = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("SubmitMilestoneDeliverableParamsV1: invalid worker_pub_y".into()))?;
        pos += 32;
        let spent_nullifier = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("SubmitMilestoneDeliverableParamsV1: invalid spent_nullifier".into()))?;
        pos += 32;
        if pos != data.len() {
            return Err(ContractError::IoError(format!(
                "SubmitMilestoneDeliverableParamsV1: expected {} bytes consumed, {} remaining",
                data.len(), data.len() - pos
            )));
        }
        Ok(SubmitMilestoneDeliverableParamsV1 { proof, job_id, milestone_index, claim_id, worker_pub_x, worker_pub_y, spent_nullifier })
    }
}

/// Parameters for confirming a milestone and releasing payment
#[derive(Debug)]
pub struct ConfirmMilestoneParamsV1 {
    /// ZK proof for milestone confirmation
    pub proof: Vec<u8>,
    /// Job ID being confirmed
    pub job_id: pallas::Base,
    /// Milestone index being confirmed
    pub milestone_index: u32,
    /// Employer's public key x coordinate
    pub employer_pub_x: pallas::Base,
    /// Employer's public key y coordinate
    pub employer_pub_y: pallas::Base,
    /// Payment release amount for this milestone
    pub payment_release: u64,
    /// Nullifier for release authorization
    pub spent_nullifier: pallas::Base,
}

impl ConfirmMilestoneParamsV1 {
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 141 {
            return Err(ContractError::IoError(format!(
                "ConfirmMilestoneParamsV1: expected at least 141 bytes, got {}",
                data.len()
            )));
        }
        let proof_len = data[0] as usize;
        let mut pos = 1usize;
        if pos + proof_len > data.len() {
            return Err(ContractError::IoError("ConfirmMilestoneParamsV1: truncated proof".into()));
        }
        let proof = data[pos..pos+proof_len].to_vec();
        pos += proof_len;
        // job_id(32)+milestone_index(4)+emp_x(32)+emp_y(32)+payment_release(8)+spent_nullifier(32)=140
        if pos + 140 > data.len() {
            return Err(ContractError::IoError("ConfirmMilestoneParamsV1: truncated fixed fields".into()));
        }
        let job_id = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("ConfirmMilestoneParamsV1: invalid job_id".into()))?;
        pos += 32;
        let milestone_index = u32::from_le_bytes(data[pos..pos+4].try_into().unwrap());
        pos += 4;
        let employer_pub_x = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("ConfirmMilestoneParamsV1: invalid employer_pub_x".into()))?;
        pos += 32;
        let employer_pub_y = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("ConfirmMilestoneParamsV1: invalid employer_pub_y".into()))?;
        pos += 32;
        let payment_release = u64::from_le_bytes(data[pos..pos+8].try_into().unwrap());
        pos += 8;
        let spent_nullifier = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("ConfirmMilestoneParamsV1: invalid spent_nullifier".into()))?;
        pos += 32;
        if pos != data.len() {
            return Err(ContractError::IoError(format!(
                "ConfirmMilestoneParamsV1: expected {} bytes consumed, {} remaining",
                data.len(), data.len() - pos
            )));
        }
        Ok(ConfirmMilestoneParamsV1 { proof, job_id, milestone_index, employer_pub_x, employer_pub_y, payment_release, spent_nullifier })
    }
}

/// Parameters for raising a dispute for a specific milestone
#[derive(Debug)]
pub struct InitiateDisputeParamsV1 {
    /// ZK proof for dispute
    pub proof: Vec<u8>,
    /// Job ID being disputed
    pub job_id: pallas::Base,
    /// Milestone index being disputed
    pub milestone_index: u32,
    /// Disputer's public key x coordinate
    pub disputer_pub_x: pallas::Base,
    /// Disputer's public key y coordinate
    pub disputer_pub_y: pallas::Base,
    /// DAO-Escrow handling the dispute
    pub dao_escrow_bulla: pallas::Base,
    /// Nullifier for dispute
    pub spent_nullifier: pallas::Base,
}

impl InitiateDisputeParamsV1 {
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 165 {
            return Err(ContractError::IoError(format!(
                "InitiateDisputeParamsV1: expected at least 165 bytes, got {}",
                data.len()
            )));
        }
        let proof_len = data[0] as usize;
        let mut pos = 1usize;
        if pos + proof_len > data.len() {
            return Err(ContractError::IoError("InitiateDisputeParamsV1: truncated proof".into()));
        }
        let proof = data[pos..pos+proof_len].to_vec();
        pos += proof_len;
        // job_id(32)+milestone_index(4)+disp_x(32)+disp_y(32)+dao(32)+spent_nullifier(32)=164
        if pos + 164 > data.len() {
            return Err(ContractError::IoError("InitiateDisputeParamsV1: truncated fixed fields".into()));
        }
        let job_id = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("InitiateDisputeParamsV1: invalid job_id".into()))?;
        pos += 32;
        let milestone_index = u32::from_le_bytes(data[pos..pos+4].try_into().unwrap());
        pos += 4;
        let disputer_pub_x = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("InitiateDisputeParamsV1: invalid disputer_pub_x".into()))?;
        pos += 32;
        let disputer_pub_y = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("InitiateDisputeParamsV1: invalid disputer_pub_y".into()))?;
        pos += 32;
        let dao_escrow_bulla = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("InitiateDisputeParamsV1: invalid dao_escrow_bulla".into()))?;
        pos += 32;
        let spent_nullifier = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("InitiateDisputeParamsV1: invalid spent_nullifier".into()))?;
        pos += 32;
        if pos != data.len() {
            return Err(ContractError::IoError(format!(
                "InitiateDisputeParamsV1: expected {} bytes consumed, {} remaining",
                data.len(), data.len() - pos
            )));
        }
        Ok(InitiateDisputeParamsV1 { proof, job_id, milestone_index, disputer_pub_x, disputer_pub_y, dao_escrow_bulla, spent_nullifier })
    }
}

// ============================================================================
// O-CAP ENABLED PARAMETERS (For capability-aware jobs)
// ============================================================================

/// Parameters for creating a job that requires workers to have a capability
#[derive(Debug)]
pub struct CreateJobWithCapabilityParamsV1 {
    /// ZK proof for job creation
    pub proof: Vec<u8>,
    /// Job ID (public input)
    pub job_id: pallas::Base,
    /// Employer's public key x coordinate
    pub employer_pub_x: pallas::Base,
    /// Employer's public key y coordinate
    pub employer_pub_y: pallas::Base,
    /// Attestation ID for deliverable verification
    pub attestation_id: pallas::Base,
    /// Type of deliverable (0 = Generic, 1 = Git)
    pub delivery_type: u8,
    /// Payment amount
    pub payment_amount: u64,
    /// Token being paid
    pub payment_token: pallas::Base,
    /// Payment commitment x coordinate
    pub payment_commit_x: pallas::Base,
    /// Payment commitment y coordinate
    pub payment_commit_y: pallas::Base,
    /// Required capability ID for workers
    pub required_capability_id: [u8; 32],
    /// Required DAG ID (None if just capability required)
    pub required_dag_id: Option<[u8; 32]>,
}

impl CreateJobWithCapabilityParamsV1 {
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        // Fixed: proof_len(1)+job_id(32)+emp_x(32)+emp_y(32)+attestation_id(32)+delivery(1)+amount(8)+token(32)+pc_x(32)+pc_y(32)+cap_id(32)+dag_tag(1) = 267
        if data.len() < 267 {
            return Err(ContractError::IoError(format!(
                "CreateJobWithCapabilityParamsV1: expected at least 267 bytes, got {}",
                data.len()
            )));
        }
        let proof_len = data[0] as usize;
        let mut pos = 1usize;
        if pos + proof_len > data.len() {
            return Err(ContractError::IoError("CreateJobWithCapabilityParamsV1: truncated proof".into()));
        }
        let proof = data[pos..pos+proof_len].to_vec();
        pos += proof_len;
        if pos + 233 > data.len() {
            return Err(ContractError::IoError("CreateJobWithCapabilityParamsV1: truncated fixed fields 1".into()));
        }
        let job_id = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("CreateJobWithCapabilityParamsV1: invalid job_id".into()))?;
        pos += 32;
        let employer_pub_x = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("CreateJobWithCapabilityParamsV1: invalid employer_pub_x".into()))?;
        pos += 32;
        let employer_pub_y = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("CreateJobWithCapabilityParamsV1: invalid employer_pub_y".into()))?;
        pos += 32;
        let attestation_id = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("CreateJobWithCapabilityParamsV1: invalid attestation_id".into()))?;
        pos += 32;
        let delivery_type = data[pos];
        pos += 1;
        let payment_amount = u64::from_le_bytes(data[pos..pos+8].try_into().unwrap());
        pos += 8;
        let payment_token = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("CreateJobWithCapabilityParamsV1: invalid payment_token".into()))?;
        pos += 32;
        let payment_commit_x = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("CreateJobWithCapabilityParamsV1: invalid payment_commit_x".into()))?;
        pos += 32;
        let payment_commit_y = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("CreateJobWithCapabilityParamsV1: invalid payment_commit_y".into()))?;
        pos += 32;
        let mut required_capability_id = [0u8; 32];
        required_capability_id.copy_from_slice(&data[pos..pos+32]);
        pos += 32;
        // required_dag_id: Option<[u8;32]>
        if pos + 1 > data.len() {
            return Err(ContractError::IoError("CreateJobWithCapabilityParamsV1: truncated required_dag_id tag".into()));
        }
        let (required_dag_id, advance): (Option<[u8;32]>, usize) = match data[pos] {
            0 => (None, 1),
            1 => {
                if pos + 1 + 32 > data.len() {
                    return Err(ContractError::IoError("CreateJobWithCapabilityParamsV1: truncated required_dag_id".into()));
                }
                let mut arr = [0u8; 32];
                arr.copy_from_slice(&data[pos+1..pos+33]);
                (Some(arr), 33)
            }
            tag => return Err(ContractError::IoError(format!("CreateJobWithCapabilityParamsV1: invalid required_dag_id tag {}", tag))),
        };
        pos += advance;
        if pos != data.len() {
            return Err(ContractError::IoError(format!(
                "CreateJobWithCapabilityParamsV1: expected {} bytes consumed, {} remaining",
                data.len(), data.len() - pos
            )));
        }
        Ok(CreateJobWithCapabilityParamsV1 {
            proof,
            job_id,
            employer_pub_x,
            employer_pub_y,
            attestation_id,
            delivery_type,
            payment_amount,
            payment_token,
            payment_commit_x,
            payment_commit_y,
            required_capability_id,
            required_dag_id,
        })
    }
}

/// Parameters for accepting a job with capability proof
#[derive(Debug)]
pub struct AcceptJobWithCapabilityParamsV1 {
    /// ZK proof for job acceptance
    pub proof: Vec<u8>,
    /// Job ID being accepted
    pub job_id: pallas::Base,
    /// Worker's public key x coordinate
    pub worker_pub_x: pallas::Base,
    /// Worker's public key y coordinate
    pub worker_pub_y: pallas::Base,
    /// Required capability ID (must match job's capability requirement)
    pub required_capability_id: pallas::Base,
    /// Capability proof from Identity contract
    pub capability_proof: Vec<u8>,
    /// Capability secret (proves ownership)
    pub capability_secret: [u8; 32],
}

impl AcceptJobWithCapabilityParamsV1 {
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 170 {
            return Err(ContractError::IoError(format!(
                "AcceptJobWithCapabilityParamsV1: expected at least 170 bytes, got {}",
                data.len()
            )));
        }
        let proof_len = data[0] as usize;
        let mut pos = 1usize;
        if pos + proof_len > data.len() {
            return Err(ContractError::IoError("AcceptJobWithCapabilityParamsV1: truncated proof".into()));
        }
        let proof = data[pos..pos+proof_len].to_vec();
        pos += proof_len;
        // job_id(32)+worker_x(32)+worker_y(32)+required_cap_id(32) = 128
        if pos + 128 > data.len() {
            return Err(ContractError::IoError("AcceptJobWithCapabilityParamsV1: truncated fixed fields 1".into()));
        }
        let job_id = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("AcceptJobWithCapabilityParamsV1: invalid job_id".into()))?;
        pos += 32;
        let worker_pub_x = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("AcceptJobWithCapabilityParamsV1: invalid worker_pub_x".into()))?;
        pos += 32;
        let worker_pub_y = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("AcceptJobWithCapabilityParamsV1: invalid worker_pub_y".into()))?;
        pos += 32;
        let required_capability_id = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("AcceptJobWithCapabilityParamsV1: invalid required_capability_id".into()))?;
        pos += 32;
        // capability_proof: u8 len + bytes
        if pos + 1 > data.len() {
            return Err(ContractError::IoError("AcceptJobWithCapabilityParamsV1: truncated cap_proof_len".into()));
        }
        let cap_proof_len = data[pos] as usize;
        pos += 1;
        if pos + cap_proof_len > data.len() {
            return Err(ContractError::IoError("AcceptJobWithCapabilityParamsV1: truncated capability_proof".into()));
        }
        let capability_proof = data[pos..pos+cap_proof_len].to_vec();
        pos += cap_proof_len;
        // capability_secret: [u8;32]
        if pos + 32 > data.len() {
            return Err(ContractError::IoError("AcceptJobWithCapabilityParamsV1: truncated capability_secret".into()));
        }
        let mut capability_secret = [0u8; 32];
        capability_secret.copy_from_slice(&data[pos..pos+32]);
        pos += 32;
        if pos != data.len() {
            return Err(ContractError::IoError(format!(
                "AcceptJobWithCapabilityParamsV1: expected {} bytes consumed, {} remaining",
                data.len(), data.len() - pos
            )));
        }
        Ok(AcceptJobWithCapabilityParamsV1 { proof, job_id, worker_pub_x, worker_pub_y, required_capability_id, capability_proof, capability_secret })
    }
}

/// Parameters for creating a milestone job with capability requirement
#[derive(Debug)]
pub struct CreateJobWithMilestonesAndCapabilityParamsV1 {
    /// ZK proof for job creation
    pub proof: Vec<u8>,
    /// Job ID (public input)
    pub job_id: pallas::Base,
    /// Employer's public key x coordinate
    pub employer_pub_x: pallas::Base,
    /// Employer's public key y coordinate
    pub employer_pub_y: pallas::Base,
    /// Attestation ID for deliverable verification
    pub attestation_id: pallas::Base,
    /// Type of deliverable (0 = Generic, 1 = Git)
    pub delivery_type: u8,
    /// Total payment amount
    pub payment_amount: u64,
    /// Token being paid
    pub payment_token: pallas::Base,
    /// Payment commitment x coordinate
    pub payment_commit_x: pallas::Base,
    /// Payment commitment y coordinate
    pub payment_commit_y: pallas::Base,
    /// Overall deadline block
    pub deadline_block: u64,
    /// Number of milestones
    pub milestone_count: u32,
    /// Milestone definitions (payment amounts, deadlines)
    pub milestones: Vec<Milestone>,
    /// Required capability ID for workers
    pub required_capability_id: [u8; 32],
    /// Required DAG ID (None if just capability required)
    pub required_dag_id: Option<[u8; 32]>,
}

impl CreateJobWithMilestonesAndCapabilityParamsV1 {
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 280 {
            return Err(ContractError::IoError(format!(
                "CreateJobWithMilestonesAndCapabilityParamsV1: expected at least 280 bytes, got {}",
                data.len()
            )));
        }
        let proof_len = data[0] as usize;
        let mut pos = 1usize;
        if pos + proof_len > data.len() {
            return Err(ContractError::IoError("CreateJobWithMilestonesAndCapabilityParamsV1: truncated proof".into()));
        }
        let proof = data[pos..pos+proof_len].to_vec();
        pos += proof_len;
        // job_id(32)+emp_x(32)+emp_y(32)+attestation_id(32)+delivery(1)+amount(8)+token(32)+pc_x(32)+pc_y(32)+deadline(8)+count(4)+cap_id(32)+dag_tag(1) = 278
        if pos + 278 > data.len() {
            return Err(ContractError::IoError("CreateJobWithMilestonesAndCapabilityParamsV1: truncated fixed fields".into()));
        }
        let job_id = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("CreateJobWithMilestonesAndCapabilityParamsV1: invalid job_id".into()))?;
        pos += 32;
        let employer_pub_x = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("CreateJobWithMilestonesAndCapabilityParamsV1: invalid employer_pub_x".into()))?;
        pos += 32;
        let employer_pub_y = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("CreateJobWithMilestonesAndCapabilityParamsV1: invalid employer_pub_y".into()))?;
        pos += 32;
        let attestation_id = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("CreateJobWithMilestonesAndCapabilityParamsV1: invalid attestation_id".into()))?;
        pos += 32;
        let delivery_type = data[pos];
        pos += 1;
        let payment_amount = u64::from_le_bytes(data[pos..pos+8].try_into().unwrap());
        pos += 8;
        let payment_token = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("CreateJobWithMilestonesAndCapabilityParamsV1: invalid payment_token".into()))?;
        pos += 32;
        let payment_commit_x = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("CreateJobWithMilestonesAndCapabilityParamsV1: invalid payment_commit_x".into()))?;
        pos += 32;
        let payment_commit_y = pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())
            .into_option()
            .ok_or_else(|| ContractError::IoError("CreateJobWithMilestonesAndCapabilityParamsV1: invalid payment_commit_y".into()))?;
        pos += 32;
        let deadline_block = u64::from_le_bytes(data[pos..pos+8].try_into().unwrap());
        pos += 8;
        let milestone_count = u32::from_le_bytes(data[pos..pos+4].try_into().unwrap()) as usize;
        pos += 4;
        // required_capability_id
        let mut required_capability_id = [0u8; 32];
        required_capability_id.copy_from_slice(&data[pos..pos+32]);
        pos += 32;
        // required_dag_id: Option<[u8;32]>
        if pos + 1 > data.len() {
            return Err(ContractError::IoError("CreateJobWithMilestonesAndCapabilityParamsV1: truncated required_dag_id tag".into()));
        }
        let (required_dag_id, advance): (Option<[u8;32]>, usize) = match data[pos] {
            0 => (None, 1),
            1 => {
                if pos + 1 + 32 > data.len() {
                    return Err(ContractError::IoError("CreateJobWithMilestonesAndCapabilityParamsV1: truncated required_dag_id".into()));
                }
                let mut arr = [0u8; 32];
                arr.copy_from_slice(&data[pos+1..pos+33]);
                (Some(arr), 33)
            }
            tag => return Err(ContractError::IoError(format!("CreateJobWithMilestonesAndCapabilityParamsV1: invalid required_dag_id tag {}", tag))),
        };
        pos += advance;
        // milestones
        let mut milestones = Vec::with_capacity(milestone_count);
        for _ in 0..milestone_count {
            if pos + 22 > data.len() {
                return Err(ContractError::IoError("CreateJobWithMilestonesAndCapabilityParamsV1: truncated milestone".into()));
            }
            let m_opt_tag = data[pos + 21];
            let m_len = if m_opt_tag == 0 { 22 } else { 30 };
            if pos + m_len > data.len() {
                return Err(ContractError::IoError("CreateJobWithMilestonesAndCapabilityParamsV1: truncated milestone body".into()));
            }
            milestones.push(Milestone::decode(&data[pos..pos+m_len])?);
            pos += m_len;
        }
        if pos != data.len() {
            return Err(ContractError::IoError(format!(
                "CreateJobWithMilestonesAndCapabilityParamsV1: expected {} bytes consumed, {} remaining",
                data.len(), data.len() - pos
            )));
        }
        Ok(CreateJobWithMilestonesAndCapabilityParamsV1 {
            proof,
            job_id,
            employer_pub_x,
            employer_pub_y,
            attestation_id,
            delivery_type,
            payment_amount,
            payment_token,
            payment_commit_x,
            payment_commit_y,
            deadline_block,
            milestone_count: milestone_count as u32,
            milestones,
            required_capability_id,
            required_dag_id,
        })
    }
}
