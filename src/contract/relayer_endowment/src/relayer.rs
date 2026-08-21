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

//! Optional relayer-registry submodule.
//!
//! Moved from the bridge contract's relayer coordination surface (Phase 2). Keeps
//! only the registry functions — `RegisterRelayerV1`, `VerifyRelayerReputationV1`,
//! `RegisterFeeScheduleV1`. The old withdrawal-coordination functions
//! (`AcceptWithdrawalV1`, `ReassignWithdrawalV1`, `CancelWithdrawV1`) are dropped:
//! bridge-core's `withdrawals` record is the external-release signal, and the
//! relayer node watches it directly.

use dwow_sdk::{
    crypto::{ContractId, PublicKey},
    dark_tree::DarkLeaf,
    error::{ContractError, ContractResult},
    msg, ContractCall,
    wasm,
};

use crate::entrypoint::compute_relayer_key;
use crate::error::RelayerEndowmentError;

/// Relayer registry tree — maps a hashed relayer pubkey to `RelayerInfo`.
pub const RELAYER_ENDOWMENT_RELAYERS_TREE: &str = "relayers";

/// Parameters for registering a relayer.
#[derive(Debug, Clone)]
pub struct RegisterRelayerParams {
    pub relayer_pub: PublicKey,
}
impl RegisterRelayerParams {
    pub const ENCODED_SIZE: usize = 32;
    pub fn encode(&self) -> Vec<u8> { self.relayer_pub.to_bytes().to_vec() }
    #[expect(clippy::unwrap_used, reason = "slice length checked above")]
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!("RegisterRelayerParams: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len())));
        }
        Ok(RegisterRelayerParams {
            relayer_pub: PublicKey::from_bytes(data[0..32].try_into().unwrap())
                .map_err(|e| ContractError::IoError(format!("RegisterRelayerParams: invalid relayer_pub: {}", e)))?,
        })
    }
}

/// Stored relayer info.
#[derive(Debug, Clone)]
pub struct RelayerInfo {
    pub version: u8,
    pub pubkey: PublicKey,
    pub registered_at: u64,
    pub total_slashed: u64,
    pub total_withdrawals: u64,
    pub total_successful: u64,
    pub is_active: bool,
    pub fee_schedule_id: Option<[u8; 32]>,
}

impl RelayerInfo {
    pub fn encode(&self) -> Vec<u8> {
        let cap = 66 + if self.fee_schedule_id.is_some() { 32 } else { 0 };
        let mut b = Vec::with_capacity(cap);
        b.push(self.version);
        b.extend_from_slice(&self.pubkey.to_bytes());
        b.extend_from_slice(&self.registered_at.to_le_bytes());
        b.extend_from_slice(&self.total_slashed.to_le_bytes());
        b.extend_from_slice(&self.total_withdrawals.to_le_bytes());
        b.extend_from_slice(&self.total_successful.to_le_bytes());
        b.push(self.is_active as u8);
        b.push(self.fee_schedule_id.is_some() as u8);
        if let Some(ref id) = self.fee_schedule_id { b.extend_from_slice(id); }
        b
    }
    #[expect(clippy::unwrap_used, reason = "slice length checked above")]
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 66 {
            return Err(ContractError::IoError(format!("RelayerInfo: expected at least 66 bytes, got {}", data.len())));
        }
        Ok(RelayerInfo {
            version: data[0],
            pubkey: PublicKey::from_bytes(data[1..33].try_into().unwrap())
                .map_err(|e| ContractError::IoError(format!("RelayerInfo: invalid pubkey: {}", e)))?,
            registered_at: u64::from_le_bytes(data[33..41].try_into().unwrap()),
            total_slashed: u64::from_le_bytes(data[41..49].try_into().unwrap()),
            total_withdrawals: u64::from_le_bytes(data[49..57].try_into().unwrap()),
            total_successful: u64::from_le_bytes(data[57..65].try_into().unwrap()),
            is_active: data[65] != 0,
            fee_schedule_id: if data[66] != 0 { Some(data[67..99].try_into().unwrap()) } else { None },
        })
    }
}

/// Register relayer update.
#[derive(Debug, Clone)]
pub struct RegisterRelayerUpdateV1 {
    pub relayer_pub: PublicKey,
    pub registered_at: u64,
}

impl RegisterRelayerUpdateV1 {
    pub const ENCODED_SIZE: usize = 40;
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(Self::ENCODED_SIZE);
        b.extend_from_slice(&self.relayer_pub.to_bytes());
        b.extend_from_slice(&self.registered_at.to_le_bytes());
        b
    }
    #[expect(clippy::unwrap_used, reason = "slice length checked above")]
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!("RegisterRelayerUpdateV1: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len())));
        }
        Ok(RegisterRelayerUpdateV1 {
            relayer_pub: PublicKey::from_bytes(data[0..32].try_into().unwrap())
                .map_err(|e| ContractError::IoError(format!("RegisterRelayerUpdateV1: invalid relayer_pub: {}", e)))?,
            registered_at: u64::from_le_bytes(data[32..40].try_into().unwrap()),
        })
    }
}

/// Parameters for verifying a relayer's reputation.
#[derive(Debug, Clone)]
pub struct VerifyRelayerReputationParams {
    pub relayer_pub: PublicKey,
}
impl VerifyRelayerReputationParams {
    pub const ENCODED_SIZE: usize = 32;
    pub fn encode(&self) -> Vec<u8> { self.relayer_pub.to_bytes().to_vec() }
    #[expect(clippy::unwrap_used, reason = "slice length checked above")]
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!("VerifyRelayerReputationParams: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len())));
        }
        Ok(VerifyRelayerReputationParams {
            relayer_pub: PublicKey::from_bytes(data[0..32].try_into().unwrap())
                .map_err(|e| ContractError::IoError(format!("VerifyRelayerReputationParams: invalid relayer_pub: {}", e)))?,
        })
    }
}

/// Reputation info returned to the caller.
#[derive(Debug, Clone)]
pub struct ReputationInfo {
    pub slash_count: u64,
    pub success_count: u64,
    pub total_volume: u64,
    pub settlement_frequency: u64,
    pub is_registered: bool,
}
impl ReputationInfo {
    pub const ENCODED_SIZE: usize = 33;
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(Self::ENCODED_SIZE);
        b.extend_from_slice(&self.slash_count.to_le_bytes());
        b.extend_from_slice(&self.success_count.to_le_bytes());
        b.extend_from_slice(&self.total_volume.to_le_bytes());
        b.extend_from_slice(&self.settlement_frequency.to_le_bytes());
        b.push(self.is_registered as u8);
        b
    }
    #[expect(clippy::unwrap_used, reason = "slice length checked above")]
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!("ReputationInfo: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len())));
        }
        Ok(ReputationInfo {
            slash_count: u64::from_le_bytes(data[0..8].try_into().unwrap()),
            success_count: u64::from_le_bytes(data[8..16].try_into().unwrap()),
            total_volume: u64::from_le_bytes(data[16..24].try_into().unwrap()),
            settlement_frequency: u64::from_le_bytes(data[24..32].try_into().unwrap()),
            is_registered: data[32] != 0,
        })
    }
}

impl dwow_serial::Encodable for ReputationInfo {
    fn encode<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<usize> {
        let b = self.encode();
        w.write_all(&b)?;
        Ok(b.len())
    }
}
impl dwow_serial::Decodable for ReputationInfo {
    fn decode<D: std::io::Read>(d: &mut D) -> std::io::Result<Self> {
        let mut b = vec![];
        d.read_to_end(&mut b)?;
        Self::decode(&b).map_err(|e| std::io::Error::other(format!("{e}")))
    }
}

/// Parameters for registering a fee schedule.
#[derive(Debug, Clone)]
pub struct RegisterFeeScheduleParams {
    pub relayer_pub: PublicKey,
    pub fee_schedule_id: [u8; 32],
}
impl RegisterFeeScheduleParams {
    pub const ENCODED_SIZE: usize = 64;
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(Self::ENCODED_SIZE);
        b.extend_from_slice(&self.relayer_pub.to_bytes());
        b.extend_from_slice(&self.fee_schedule_id);
        b
    }
    #[expect(clippy::unwrap_used, reason = "slice length checked above")]
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!("RegisterFeeScheduleParams: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len())));
        }
        Ok(RegisterFeeScheduleParams {
            relayer_pub: PublicKey::from_bytes(data[0..32].try_into().unwrap())
                .map_err(|e| ContractError::IoError(format!("RegisterFeeScheduleParams: invalid relayer_pub: {}", e)))?,
            fee_schedule_id: data[32..64].try_into().unwrap(),
        })
    }
}

/// Register fee schedule update.
#[derive(Debug, Clone)]
pub struct RegisterFeeScheduleUpdateV1 {
    pub relayer_pub: PublicKey,
    pub fee_schedule_id: [u8; 32],
}

impl RegisterFeeScheduleUpdateV1 {
    pub const ENCODED_SIZE: usize = 64;
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(Self::ENCODED_SIZE);
        b.extend_from_slice(&self.relayer_pub.to_bytes());
        b.extend_from_slice(&self.fee_schedule_id);
        b
    }
    #[expect(clippy::unwrap_used, reason = "slice length checked above")]
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!("RegisterFeeScheduleUpdateV1: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len())));
        }
        Ok(RegisterFeeScheduleUpdateV1 {
            relayer_pub: PublicKey::from_bytes(data[0..32].try_into().unwrap())
                .map_err(|e| ContractError::IoError(format!("RegisterFeeScheduleUpdateV1: invalid relayer_pub: {}", e)))?,
            fee_schedule_id: data[32..64].try_into().unwrap(),
        })
    }
}

// ============================================================================
// PROCESS / APPLY
// ============================================================================

/// Process RegisterRelayer instruction.
pub fn process_register_relayer(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params = RegisterRelayerParams::decode(&self_.data[1..])?;

    msg!("[relayer_endowment::RegisterRelayerV1] Registering relayer: {:?}", &params.relayer_pub);

    let relayers_db = wasm::db::db_lookup(cid, RELAYER_ENDOWMENT_RELAYERS_TREE)?;
    let relayer_key = compute_relayer_key(&params.relayer_pub);
    if wasm::db::db_contains_key(relayers_db, &relayer_key)? {
        msg!("[relayer_endowment::RegisterRelayerV1] ERROR: Relayer already registered");
        return Err(RelayerEndowmentError::RelayerAlreadyRegistered.into());
    }

    let update = RegisterRelayerUpdateV1 {
        relayer_pub: params.relayer_pub,
        registered_at: wasm::util::get_verifying_block_height()?.get(),
    };
    Ok(update.encode())
}

/// Apply RegisterRelayer update.
pub fn apply_register_relayer(cid: ContractId, update: RegisterRelayerUpdateV1) -> ContractResult {
    let relayers_db = wasm::db::db_lookup(cid, RELAYER_ENDOWMENT_RELAYERS_TREE)?;

    let info = RelayerInfo {
        version: 1,
        pubkey: update.relayer_pub,
        registered_at: update.registered_at,
        total_slashed: 0,
        total_withdrawals: 0,
        total_successful: 0,
        is_active: true,
        fee_schedule_id: None,
    };

    wasm::db::db_set(relayers_db, &compute_relayer_key(&update.relayer_pub), &info.encode())?;

    msg!("[relayer_endowment::apply] Relayer registered: {:?}", update.relayer_pub);
    Ok(())
}

/// Process VerifyRelayerReputation instruction (read-only).
pub fn process_verify_reputation(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params = VerifyRelayerReputationParams::decode(&self_.data[1..])?;

    let relayers_db = wasm::db::db_lookup(cid, RELAYER_ENDOWMENT_RELAYERS_TREE)?;
    let relayer_key = compute_relayer_key(&params.relayer_pub);

    let reputation = match wasm::db::db_get(relayers_db, &relayer_key)? {
        Some(data) => {
            let info = RelayerInfo::decode(&data)?;
            ReputationInfo {
                slash_count: info.total_slashed,
                success_count: info.total_successful,
                total_volume: 0,
                settlement_frequency: 0,
                is_registered: info.is_active,
            }
        }
        None => ReputationInfo {
            slash_count: 0,
            success_count: 0,
            total_volume: 0,
            settlement_frequency: 0,
            is_registered: false,
        },
    };

    msg!("[relayer_endowment::VerifyRelayerReputationV1] Reputation: registered={}, slashes={}, successful={}",
        reputation.is_registered, reputation.slash_count, reputation.success_count);

    Ok(reputation.encode())
}

/// Process RegisterFeeSchedule instruction.
pub fn process_register_fee_schedule(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params = RegisterFeeScheduleParams::decode(&self_.data[1..])?;

    let relayers_db = wasm::db::db_lookup(cid, RELAYER_ENDOWMENT_RELAYERS_TREE)?;
    let relayer_key = compute_relayer_key(&params.relayer_pub);
    if wasm::db::db_get(relayers_db, &relayer_key)?.is_none() {
        msg!("[relayer_endowment::RegisterFeeScheduleV1] ERROR: Relayer not registered");
        return Err(RelayerEndowmentError::RelayerNotRegistered.into());
    }

    let update = RegisterFeeScheduleUpdateV1 {
        relayer_pub: params.relayer_pub,
        fee_schedule_id: params.fee_schedule_id,
    };
    Ok(update.encode())
}

/// Apply RegisterFeeSchedule update.
pub fn apply_register_fee_schedule(cid: ContractId, update: RegisterFeeScheduleUpdateV1) -> ContractResult {
    let relayers_db = wasm::db::db_lookup(cid, RELAYER_ENDOWMENT_RELAYERS_TREE)?;
    let relayer_key = compute_relayer_key(&update.relayer_pub);

    let Some(relayer_data) = wasm::db::db_get(relayers_db, &relayer_key)? else {
        return Err(RelayerEndowmentError::RelayerNotRegistered.into());
    };

    let mut info = RelayerInfo::decode(&relayer_data)?;
    info.fee_schedule_id = Some(update.fee_schedule_id);
    wasm::db::db_set(relayers_db, &relayer_key, &info.encode())?;

    msg!("[relayer_endowment::apply] Fee schedule registered for {:?}", update.relayer_pub);
    Ok(())
}
