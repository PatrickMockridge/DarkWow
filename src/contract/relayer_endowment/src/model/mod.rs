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

//! Data structures for relayer_endowment contract calls

use dwow_sdk::{
    crypto::{pasta_prelude::PrimeField, PublicKey},
    error::ContractError,
    pasta::pallas,
};
use pasta_curves::group::GroupEncoding;

/// Relayer's endowment account - tracks total deployed capital and fee distribution
#[derive(Debug, Clone)]
pub struct RelayerEndowmentAccount {
    pub version: u8,
    /// Instance seed for per-capability key derivation
    pub instance_seed: [u8; 32],
    /// Relayer ID (public key)
    pub relayer_pub: PublicKey,
    /// Total deployed capital from all backers
    pub total_deployed: u64,
    /// Active deployments count
    pub active_deployments: u64,
    /// Accumulated fees to be distributed to backers
    pub accumulated_fees: u64,
    /// Default fee cut for backers (basis points)
    pub default_backer_cut_bp: u32,
    /// Block when account was created
    pub created_at: u64,
    /// Last block height when fees were settled (for force-settlement timeout)
    pub last_settlement_height: u64,
    /// Total fees collected since last settlement (for backer audit)
    pub total_collected_fees_log: u64,
    /// Whether account is active
    pub is_active: bool,
    /// Total amount slashed from this relayer (Phase 2d hardening)
    pub total_slashed: u64,
    /// Total successful withdrawals processed (Phase 2d hardening)
    pub total_successful: u64,
}

impl RelayerEndowmentAccount {
    pub const ENCODED_SIZE: usize = 134;

    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(134);
        b.push(self.version);
        b.extend_from_slice(&self.instance_seed);
        b.extend_from_slice(&self.relayer_pub.to_bytes());
        b.extend_from_slice(&self.total_deployed.to_le_bytes());
        b.extend_from_slice(&self.active_deployments.to_le_bytes());
        b.extend_from_slice(&self.accumulated_fees.to_le_bytes());
        b.extend_from_slice(&self.default_backer_cut_bp.to_le_bytes());
        b.extend_from_slice(&self.created_at.to_le_bytes());
        b.extend_from_slice(&self.last_settlement_height.to_le_bytes());
        b.extend_from_slice(&self.total_collected_fees_log.to_le_bytes());
        b.push(self.is_active as u8);
        b.extend_from_slice(&self.total_slashed.to_le_bytes());
        b.extend_from_slice(&self.total_successful.to_le_bytes());
        b
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 134 {
            return Err(ContractError::IoError(format!(
                "RelayerEndowmentAccount: expected 134 bytes, got {}",
                data.len()
            )));
        }
        Ok(RelayerEndowmentAccount {
            version: data[0],
            instance_seed: data[1..33].try_into().unwrap(),
            relayer_pub: PublicKey::from_bytes(data[33..65].try_into().unwrap())?,
            total_deployed: u64::from_le_bytes(data[65..73].try_into().unwrap()),
            active_deployments: u64::from_le_bytes(data[73..81].try_into().unwrap()),
            accumulated_fees: u64::from_le_bytes(data[81..89].try_into().unwrap()),
            default_backer_cut_bp: u32::from_le_bytes(data[89..93].try_into().unwrap()),
            created_at: u64::from_le_bytes(data[93..101].try_into().unwrap()),
            last_settlement_height: u64::from_le_bytes(data[101..109].try_into().unwrap()),
            total_collected_fees_log: u64::from_le_bytes(data[109..117].try_into().unwrap()),
            is_active: data[117] != 0,
            total_slashed: u64::from_le_bytes(data[118..126].try_into().unwrap()),
            total_successful: u64::from_le_bytes(data[126..134].try_into().unwrap()),
        })
    }
}

/// Individual deployment from a backer to a relayer
#[derive(Debug, Clone)]
pub struct EndowmentDeployment {
    pub version: u8,
    /// Unique deployment identifier
    pub deployment_id: pallas::Base,
    /// Relayer this deployment is for
    pub relayer_pub: PublicKey,
    /// Backer who deployed capital
    pub backer_pub: PublicKey,
    /// Amount deployed
    pub amount: u64,
    /// Backer's cut of relayer fees (basis points)
    pub backer_cut_bp: u32,
    /// Accumulated fees claimable by backer
    pub accumulated_fees: u64,
    /// Block when deployment was made
    pub deployed_at: u64,
    /// Block when withdrawal was requested (if requested)
    pub withdraw_requested_at: Option<u64>,
    /// Whether deployment has been withdrawn
    pub withdrawn: bool,
}

impl EndowmentDeployment {
    /// Max encoded size (with Some for Option)
    pub const ENCODED_SIZE: usize = 135;

    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(135);
        b.push(self.version);
        b.extend_from_slice(&self.deployment_id.to_repr());
        b.extend_from_slice(&self.relayer_pub.to_bytes());
        b.extend_from_slice(&self.backer_pub.to_bytes());
        b.extend_from_slice(&self.amount.to_le_bytes());
        b.extend_from_slice(&self.backer_cut_bp.to_le_bytes());
        b.extend_from_slice(&self.accumulated_fees.to_le_bytes());
        b.extend_from_slice(&self.deployed_at.to_le_bytes());
        match &self.withdraw_requested_at {
            None => b.push(0u8),
            Some(v) => {
                b.push(1u8);
                b.extend_from_slice(&v.to_le_bytes());
            }
        }
        b.push(self.withdrawn as u8);
        b
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        let min_len = 127; // minimum without Option value bytes
        if data.len() < min_len {
            return Err(ContractError::IoError(format!(
                "EndowmentDeployment: expected at least {} bytes, got {}",
                min_len, data.len()
            )));
        }
        let tag = data[125];
        let expected_len = if tag == 0 { 127 } else { 135 };
        if data.len() != expected_len {
            return Err(ContractError::IoError(format!(
                "EndowmentDeployment: expected {} bytes, got {}",
                expected_len, data.len()
            )));
        }
        let withdraw_requested_at = match tag {
            0 => None,
            1 => Some(u64::from_le_bytes(data[126..134].try_into().unwrap())),
            _ => return Err(ContractError::IoError(
                "EndowmentDeployment: invalid withdraw_requested_at tag".into()
            )),
        };
        Ok(EndowmentDeployment {
            version: data[0],
            deployment_id: pallas::Base::from_repr(data[1..33].try_into().unwrap())
                .into_option()
                .ok_or_else(|| ContractError::IoError("EndowmentDeployment: invalid deployment_id".into()))?,
            relayer_pub: PublicKey::from_bytes(data[33..65].try_into().unwrap())?,
            backer_pub: PublicKey::from_bytes(data[65..97].try_into().unwrap())?,
            amount: u64::from_le_bytes(data[97..105].try_into().unwrap()),
            backer_cut_bp: u32::from_le_bytes(data[105..109].try_into().unwrap()),
            accumulated_fees: u64::from_le_bytes(data[109..117].try_into().unwrap()),
            deployed_at: u64::from_le_bytes(data[117..125].try_into().unwrap()),
            withdraw_requested_at,
            withdrawn: data[if tag == 0 { 126 } else { 134 }] != 0,
        })
    }
}

// ============================================================================
// PARAMETER STRUCTS
// ============================================================================

/// Parameters for initializing a relayer endowment account
#[derive(Debug, Clone)]
pub struct InitializeParamsV1 {
    /// Instance seed for per-capability key derivation
    pub instance_seed: [u8; 32],
    /// Default fee cut for backers (basis points)
    pub default_backer_cut_bp: u32,
    /// Public key of the relayer (from transaction signature)
    pub signature_public: PublicKey,
}

impl InitializeParamsV1 {
    pub const ENCODED_SIZE: usize = 68;

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 68 {
            return Err(ContractError::IoError(format!(
                "InitializeParamsV1: expected 68 bytes, got {}",
                data.len()
            )));
        }
        Ok(InitializeParamsV1 {
            instance_seed: data[0..32].try_into().unwrap(),
            default_backer_cut_bp: u32::from_le_bytes(data[32..36].try_into().unwrap()),
            signature_public: PublicKey::from_bytes(data[36..68].try_into().unwrap())?,
        })
    }
}

/// Update returned after initializing endowment
#[derive(Debug, Clone)]
pub struct InitializeUpdateV1 {
    /// Instance seed for per-capability key derivation
    pub instance_seed: [u8; 32],
    pub relayer_pub: PublicKey,
    pub default_backer_cut_bp: u32,
    pub created_at: u64,
}

impl InitializeUpdateV1 {
    pub const ENCODED_SIZE: usize = 76;

    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(76);
        b.extend_from_slice(&self.instance_seed);
        b.extend_from_slice(&self.relayer_pub.to_bytes());
        b.extend_from_slice(&self.default_backer_cut_bp.to_le_bytes());
        b.extend_from_slice(&self.created_at.to_le_bytes());
        b
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 76 {
            return Err(ContractError::IoError(format!(
                "InitializeUpdateV1: expected 76 bytes, got {}",
                data.len()
            )));
        }
        Ok(InitializeUpdateV1 {
            instance_seed: data[0..32].try_into().unwrap(),
            relayer_pub: PublicKey::from_bytes(data[32..64].try_into().unwrap())?,
            default_backer_cut_bp: u32::from_le_bytes(data[64..68].try_into().unwrap()),
            created_at: u64::from_le_bytes(data[68..76].try_into().unwrap()),
        })
    }
}

/// Parameters for deploying capital to a relayer's endowment
#[derive(Debug, Clone)]
pub struct DeployCapitalParamsV1 {
    /// Instance seed for per-capability key derivation
    pub instance_seed: [u8; 32],
    /// Relayer to deploy capital to
    pub relayer_pub: PublicKey,
    /// Amount to deploy
    pub amount: u64,
    /// Backer's desired cut of relayer fees (basis points)
    pub backer_cut_bp: u32,
    /// Backer's public key (from transaction signature)
    pub signature_public: PublicKey,
    /// Value commitment point (public input for ZK proof)
    pub value_commit: pallas::Point,
    /// Optional minimum success rate threshold (basis points, e.g. 8000 = 80%)
    pub min_success_rate_bp: Option<u64>,
    /// Optional maximum slash count threshold
    pub max_slash_count: Option<u64>,
}

impl DeployCapitalParamsV1 {
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 140 {
            return Err(ContractError::IoError(format!(
                "DeployCapitalParamsV1: expected at least 140 bytes, got {}",
                data.len()
            )));
        }
        let tag1 = data[140];
        let mut pos = 141usize;
        let min_success_rate_bp = match tag1 {
            0 => None,
            1 => {
                if pos + 8 > data.len() {
                    return Err(ContractError::IoError("DeployCapitalParamsV1: truncated min_success_rate_bp".into()));
                }
                let v = Some(u64::from_le_bytes(data[pos..pos + 8].try_into().unwrap()));
                pos += 8;
                v
            }
            _ => return Err(ContractError::IoError("DeployCapitalParamsV1: invalid min_success_rate_bp tag".into())),
        };
        let tag2 = if pos < data.len() { data[pos] } else {
            return Err(ContractError::IoError("DeployCapitalParamsV1: missing max_slash_count tag".into()));
        };
        pos += 1;
        let max_slash_count = match tag2 {
            0 => None,
            1 => {
                if pos + 8 > data.len() {
                    return Err(ContractError::IoError("DeployCapitalParamsV1: truncated max_slash_count".into()));
                }
                let v = Some(u64::from_le_bytes(data[pos..pos + 8].try_into().unwrap()));
                pos += 8;
                v
            }
            _ => return Err(ContractError::IoError("DeployCapitalParamsV1: invalid max_slash_count tag".into())),
        };
        Ok(DeployCapitalParamsV1 {
            instance_seed: data[0..32].try_into().unwrap(),
            relayer_pub: PublicKey::from_bytes(data[32..64].try_into().unwrap())?,
            amount: u64::from_le_bytes(data[64..72].try_into().unwrap()),
            backer_cut_bp: u32::from_le_bytes(data[72..76].try_into().unwrap()),
            signature_public: PublicKey::from_bytes(data[76..108].try_into().unwrap())?,
            value_commit: {
                let ct = pallas::Point::from_bytes(&data[108..140].try_into().unwrap());
                if bool::from(ct.is_some()) { ct.unwrap() } else {
                    return Err(ContractError::IoError("DeployCapitalParamsV1: invalid value_commit".into()));
                }
            },
            min_success_rate_bp,
            max_slash_count,
        })
    }
}

/// Update returned after deploying capital
#[derive(Debug, Clone)]
pub struct DeployCapitalUpdateV1 {
    /// Instance seed for per-capability key derivation
    pub instance_seed: [u8; 32],
    pub deployment_id: pallas::Base,
    pub relayer_pub: PublicKey,
    pub backer_pub: PublicKey,
    pub amount: u64,
    pub backer_cut_bp: u32,
    pub total_deployed: u64,
    pub active_deployments: u64,
}

impl DeployCapitalUpdateV1 {
    pub const ENCODED_SIZE: usize = 156;

    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(156);
        b.extend_from_slice(&self.instance_seed);
        b.extend_from_slice(&self.deployment_id.to_repr());
        b.extend_from_slice(&self.relayer_pub.to_bytes());
        b.extend_from_slice(&self.backer_pub.to_bytes());
        b.extend_from_slice(&self.amount.to_le_bytes());
        b.extend_from_slice(&self.backer_cut_bp.to_le_bytes());
        b.extend_from_slice(&self.total_deployed.to_le_bytes());
        b.extend_from_slice(&self.active_deployments.to_le_bytes());
        b
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 156 {
            return Err(ContractError::IoError(format!(
                "DeployCapitalUpdateV1: expected 156 bytes, got {}",
                data.len()
            )));
        }
        Ok(DeployCapitalUpdateV1 {
            instance_seed: data[0..32].try_into().unwrap(),
            deployment_id: pallas::Base::from_repr(data[32..64].try_into().unwrap())
                .into_option()
                .ok_or_else(|| ContractError::IoError("DeployCapitalUpdateV1: invalid deployment_id".into()))?,
            relayer_pub: PublicKey::from_bytes(data[64..96].try_into().unwrap())?,
            backer_pub: PublicKey::from_bytes(data[96..128].try_into().unwrap())?,
            amount: u64::from_le_bytes(data[128..136].try_into().unwrap()),
            backer_cut_bp: u32::from_le_bytes(data[136..140].try_into().unwrap()),
            total_deployed: u64::from_le_bytes(data[140..148].try_into().unwrap()),
            active_deployments: u64::from_le_bytes(data[148..156].try_into().unwrap()),
        })
    }
}

/// Parameters for withdrawing a deployment
#[derive(Debug, Clone)]
pub struct WithdrawDeploymentParamsV1 {
    /// Deployment ID to withdraw
    pub deployment_id: pallas::Base,
}

impl WithdrawDeploymentParamsV1 {
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 32 {
            return Err(ContractError::IoError(format!(
                "WithdrawDeploymentParamsV1: expected 32 bytes, got {}",
                data.len()
            )));
        }
        Ok(WithdrawDeploymentParamsV1 {
            deployment_id: pallas::Base::from_repr(data[0..32].try_into().unwrap())
                .into_option()
                .ok_or_else(|| ContractError::IoError("WithdrawDeploymentParamsV1: invalid deployment_id".into()))?,
        })
    }
}

/// Update returned after withdrawing deployment
#[derive(Debug, Clone)]
pub struct WithdrawDeploymentUpdateV1 {
    pub deployment_id: pallas::Base,
    pub payout_amount: u64,
    pub fees_claimed: u64,
}

impl WithdrawDeploymentUpdateV1 {
    pub const ENCODED_SIZE: usize = 48;

    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(48);
        b.extend_from_slice(&self.deployment_id.to_repr());
        b.extend_from_slice(&self.payout_amount.to_le_bytes());
        b.extend_from_slice(&self.fees_claimed.to_le_bytes());
        b
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 48 {
            return Err(ContractError::IoError(format!(
                "WithdrawDeploymentUpdateV1: expected 48 bytes, got {}",
                data.len()
            )));
        }
        Ok(WithdrawDeploymentUpdateV1 {
            deployment_id: pallas::Base::from_repr(data[0..32].try_into().unwrap())
                .into_option()
                .ok_or_else(|| ContractError::IoError("WithdrawDeploymentUpdateV1: invalid deployment_id".into()))?,
            payout_amount: u64::from_le_bytes(data[32..40].try_into().unwrap()),
            fees_claimed: u64::from_le_bytes(data[40..48].try_into().unwrap()),
        })
    }
}

/// Parameters for claiming accumulated fees
#[derive(Debug, Clone)]
pub struct ClaimFeesParamsV1 {
    /// Deployment ID to claim fees for
    pub deployment_id: pallas::Base,
    /// Backer's public key X coordinate
    pub backer_pub_x: [u8; 32],
    /// Backer's public key Y coordinate
    pub backer_pub_y: [u8; 32],
    /// Fee share allocated to this backer
    pub fee_share: u64,
}

impl ClaimFeesParamsV1 {
    pub const ENCODED_SIZE: usize = 104;

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 104 {
            return Err(ContractError::IoError(format!(
                "ClaimFeesParamsV1: expected 104 bytes, got {}",
                data.len()
            )));
        }
        Ok(ClaimFeesParamsV1 {
            deployment_id: pallas::Base::from_repr(data[0..32].try_into().unwrap())
                .into_option()
                .ok_or_else(|| ContractError::IoError("ClaimFeesParamsV1: invalid deployment_id".into()))?,
            backer_pub_x: data[32..64].try_into().unwrap(),
            backer_pub_y: data[64..96].try_into().unwrap(),
            fee_share: u64::from_le_bytes(data[96..104].try_into().unwrap()),
        })
    }
}

/// Update returned after claiming fees
#[derive(Debug, Clone)]
pub struct ClaimFeesUpdateV1 {
    pub deployment_id: pallas::Base,
    pub claimed_amount: u64,
    pub remaining_fees: u64,
}

impl ClaimFeesUpdateV1 {
    pub const ENCODED_SIZE: usize = 48;

    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(48);
        b.extend_from_slice(&self.deployment_id.to_repr());
        b.extend_from_slice(&self.claimed_amount.to_le_bytes());
        b.extend_from_slice(&self.remaining_fees.to_le_bytes());
        b
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 48 {
            return Err(ContractError::IoError(format!(
                "ClaimFeesUpdateV1: expected 48 bytes, got {}",
                data.len()
            )));
        }
        Ok(ClaimFeesUpdateV1 {
            deployment_id: pallas::Base::from_repr(data[0..32].try_into().unwrap())
                .into_option()
                .ok_or_else(|| ContractError::IoError("ClaimFeesUpdateV1: invalid deployment_id".into()))?,
            claimed_amount: u64::from_le_bytes(data[32..40].try_into().unwrap()),
            remaining_fees: u64::from_le_bytes(data[40..48].try_into().unwrap()),
        })
    }
}

/// Per-deployment fee allocation for SettleFees
#[derive(Debug, Clone)]
pub struct FeeAllocation {
    /// Deployment receiving fees
    pub deployment_id: pallas::Base,
    /// Fee amount allocated to this deployment
    pub fee_amount: u64,
}

impl FeeAllocation {
    pub const ENCODED_SIZE: usize = 40;

    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(40);
        b.extend_from_slice(&self.deployment_id.to_repr());
        b.extend_from_slice(&self.fee_amount.to_le_bytes());
        b
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 40 {
            return Err(ContractError::IoError(format!(
                "FeeAllocation: expected 40 bytes, got {}",
                data.len()
            )));
        }
        Ok(FeeAllocation {
            deployment_id: pallas::Base::from_repr(data[0..32].try_into().unwrap())
                .into_option()
                .ok_or_else(|| ContractError::IoError("FeeAllocation: invalid deployment_id".into()))?,
            fee_amount: u64::from_le_bytes(data[32..40].try_into().unwrap()),
        })
    }
}

/// Parameters for settling fees to deployments
#[derive(Debug, Clone)]
pub struct SettleFeesParamsV1 {
    /// Relayer to settle fees for
    pub relayer_pub: PublicKey,
    /// Total fees to distribute
    pub total_fees: u64,
    /// Per-deployment fee allocations
    pub allocations: Vec<FeeAllocation>,
    /// Public key of the relayer (from transaction signature)
    pub signature_public: PublicKey,
}

impl SettleFeesParamsV1 {
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 105 {
            return Err(ContractError::IoError(format!(
                "SettleFeesParamsV1: expected at least 105 bytes, got {}",
                data.len()
            )));
        }
        let relayer_pub = PublicKey::from_bytes(data[0..32].try_into().unwrap())?;
        let total_fees = u64::from_le_bytes(data[32..40].try_into().unwrap());
        let alloc_count = data[40] as usize;
        let header = 73usize; // 32 + 8 + 1 + 32
        let expected = header + alloc_count * FeeAllocation::ENCODED_SIZE;
        if data.len() != expected {
            return Err(ContractError::IoError(format!(
                "SettleFeesParamsV1: expected {} bytes, got {}",
                expected, data.len()
            )));
        }
        let signature_public = PublicKey::from_bytes(data[41..73].try_into().unwrap())?;
        let mut allocations = Vec::with_capacity(alloc_count);
        for i in 0..alloc_count {
            let start = header + i * FeeAllocation::ENCODED_SIZE;
            allocations.push(FeeAllocation::decode(&data[start..start + FeeAllocation::ENCODED_SIZE])?);
        }
        Ok(SettleFeesParamsV1 { relayer_pub, total_fees, allocations, signature_public })
    }
}

/// Update returned after settling fees
#[derive(Debug, Clone)]
pub struct SettleFeesUpdateV1 {
    pub relayer_pub: PublicKey,
    pub total_fees_settled: u64,
    pub deployments_updated: u64,
    /// Per-deployment fee allocations applied
    pub allocations: Vec<FeeAllocation>,
}

impl SettleFeesUpdateV1 {
    pub fn encode(&self) -> Vec<u8> {
        let cap = 49 + self.allocations.len() * FeeAllocation::ENCODED_SIZE;
        let mut b = Vec::with_capacity(cap);
        b.extend_from_slice(&self.relayer_pub.to_bytes());
        b.extend_from_slice(&self.total_fees_settled.to_le_bytes());
        b.extend_from_slice(&self.deployments_updated.to_le_bytes());
        b.push(self.allocations.len() as u8);
        for alloc in &self.allocations {
            b.extend_from_slice(&alloc.encode());
        }
        b
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 49 {
            return Err(ContractError::IoError(format!(
                "SettleFeesUpdateV1: expected at least 49 bytes, got {}",
                data.len()
            )));
        }
        let alloc_count = data[48] as usize;
        let expected = 49 + alloc_count * FeeAllocation::ENCODED_SIZE;
        if data.len() != expected {
            return Err(ContractError::IoError(format!(
                "SettleFeesUpdateV1: expected {} bytes, got {}",
                expected, data.len()
            )));
        }
        let mut allocations = Vec::with_capacity(alloc_count);
        for i in 0..alloc_count {
            let start = 49 + i * FeeAllocation::ENCODED_SIZE;
            allocations.push(FeeAllocation::decode(&data[start..start + FeeAllocation::ENCODED_SIZE])?);
        }
        Ok(SettleFeesUpdateV1 {
            relayer_pub: PublicKey::from_bytes(data[0..32].try_into().unwrap())?,
            total_fees_settled: u64::from_le_bytes(data[32..40].try_into().unwrap()),
            deployments_updated: u64::from_le_bytes(data[40..48].try_into().unwrap()),
            allocations,
        })
    }
}

/// Parameters for updating fee configuration
#[derive(Debug, Clone)]
pub struct UpdateConfigParamsV1 {
    /// Relayer to update config for
    pub relayer_pub: PublicKey,
    /// New default backer cut (basis points)
    pub default_backer_cut_bp: u32,
}

impl UpdateConfigParamsV1 {
    pub const ENCODED_SIZE: usize = 36;

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 36 {
            return Err(ContractError::IoError(format!(
                "UpdateConfigParamsV1: expected 36 bytes, got {}",
                data.len()
            )));
        }
        Ok(UpdateConfigParamsV1 {
            relayer_pub: PublicKey::from_bytes(data[0..32].try_into().unwrap())?,
            default_backer_cut_bp: u32::from_le_bytes(data[32..36].try_into().unwrap()),
        })
    }
}

/// Update returned after updating config
#[derive(Debug, Clone)]
pub struct UpdateConfigUpdateV1 {
    pub relayer_pub: PublicKey,
    pub default_backer_cut_bp: u32,
}

impl UpdateConfigUpdateV1 {
    pub const ENCODED_SIZE: usize = 36;

    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(36);
        b.extend_from_slice(&self.relayer_pub.to_bytes());
        b.extend_from_slice(&self.default_backer_cut_bp.to_le_bytes());
        b
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 36 {
            return Err(ContractError::IoError(format!(
                "UpdateConfigUpdateV1: expected 36 bytes, got {}",
                data.len()
            )));
        }
        Ok(UpdateConfigUpdateV1 {
            relayer_pub: PublicKey::from_bytes(data[0..32].try_into().unwrap())?,
            default_backer_cut_bp: u32::from_le_bytes(data[32..36].try_into().unwrap()),
        })
    }
}

/// Parameters for backer-initiated force settlement
///
/// If a relayer hasn't settled fees within `FORCE_SETTLEMENT_TIMEOUT` blocks,
/// any backer with active deployment can force a pro-rata settlement.
#[derive(Debug, Clone)]
pub struct ForceSettleParamsV1 {
    /// Relayer whose fees are being force-settled
    pub relayer_pub: PublicKey,
    /// Deployment ID to force-settle for
    pub deployment_id: pallas::Base,
    /// Current block height for timeout verification
    pub current_block: u64,
    /// Backer's public key (from transaction signature)
    pub signature_public: PublicKey,
}

impl ForceSettleParamsV1 {
    pub const ENCODED_SIZE: usize = 104;

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 104 {
            return Err(ContractError::IoError(format!(
                "ForceSettleParamsV1: expected 104 bytes, got {}",
                data.len()
            )));
        }
        Ok(ForceSettleParamsV1 {
            relayer_pub: PublicKey::from_bytes(data[0..32].try_into().unwrap())?,
            deployment_id: pallas::Base::from_repr(data[32..64].try_into().unwrap())
                .into_option()
                .ok_or_else(|| ContractError::IoError("ForceSettleParamsV1: invalid deployment_id".into()))?,
            current_block: u64::from_le_bytes(data[64..72].try_into().unwrap()),
            signature_public: PublicKey::from_bytes(data[72..104].try_into().unwrap())?,
        })
    }
}

/// Update returned after force settlement
#[derive(Debug, Clone)]
pub struct ForceSettleUpdateV1 {
    pub deployment_id: pallas::Base,
    pub relayer_pub: PublicKey,
    pub force_settled_amount: u64,
}

impl ForceSettleUpdateV1 {
    pub const ENCODED_SIZE: usize = 72;

    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(72);
        b.extend_from_slice(&self.deployment_id.to_repr());
        b.extend_from_slice(&self.relayer_pub.to_bytes());
        b.extend_from_slice(&self.force_settled_amount.to_le_bytes());
        b
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 72 {
            return Err(ContractError::IoError(format!(
                "ForceSettleUpdateV1: expected 72 bytes, got {}",
                data.len()
            )));
        }
        Ok(ForceSettleUpdateV1 {
            deployment_id: pallas::Base::from_repr(data[0..32].try_into().unwrap())
                .into_option()
                .ok_or_else(|| ContractError::IoError("ForceSettleUpdateV1: invalid deployment_id".into()))?,
            relayer_pub: PublicKey::from_bytes(data[32..64].try_into().unwrap())?,
            force_settled_amount: u64::from_le_bytes(data[64..72].try_into().unwrap()),
        })
    }
}

/// Parameters for deactivating an endowment account
#[derive(Debug, Clone)]
pub struct DeactivateEndowmentParamsV1 {
    pub relayer_pub: PublicKey,
}

impl DeactivateEndowmentParamsV1 {
    pub const ENCODED_SIZE: usize = 32;

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 32 {
            return Err(ContractError::IoError(format!(
                "DeactivateEndowmentParamsV1: expected 32 bytes, got {}",
                data.len()
            )));
        }
        Ok(DeactivateEndowmentParamsV1 {
            relayer_pub: PublicKey::from_bytes(data[0..32].try_into().unwrap())?,
        })
    }
}

/// Update returned after deactivating an endowment
#[derive(Debug, Clone)]
pub struct DeactivateEndowmentUpdateV1 {
    pub relayer_pub: PublicKey,
}

impl DeactivateEndowmentUpdateV1 {
    pub const ENCODED_SIZE: usize = 32;

    pub fn encode(&self) -> Vec<u8> {
        self.relayer_pub.to_bytes().to_vec()
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 32 {
            return Err(ContractError::IoError(format!(
                "DeactivateEndowmentUpdateV1: expected 32 bytes, got {}",
                data.len()
            )));
        }
        Ok(DeactivateEndowmentUpdateV1 {
            relayer_pub: PublicKey::from_bytes(data[0..32].try_into().unwrap())?,
        })
    }
}
