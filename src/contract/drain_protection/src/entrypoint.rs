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

//! WASM entrypoint for the DrainProtection contract
//!
//! ## Overview
//!
//! This contract provides governance-level protections for endowment/treasury
//! funds against malicious DAO actions or mass exit attacks.
//!
//! ## Key Protections
//!
//! | Action | Threshold | Notes |
//! |--------|-----------|-------|
//! | Fund transfers (within rate limit) | None | Base rate per block |
//! | Fund transfers (exceeds rate) | 2/3 total vote | Configurable rate limit |
//! | Lock endowment funds | 2/3 total vote | Max 7 days, renewable |
//! | Unlock funds | 2/3 total vote | + 24hr timelock |
//! | Change spend authority | 2/3 total vote | + 48hr timelock |
//! | Member exit | 1/3 haircut | Any time, block-height-weighted |
//!
//! ## Provisional Status
//!
//! This contract is EXPERIMENTAL. The protections are provisionally specified
//! and require full implementation and security audit.

use dwow_sdk::{
    crypto::{pasta_prelude::PrimeField, poseidon_hash},
    error::{ContractError, ContractResult},
    msg,
    pasta::pallas,
    wasm, ContractCall,
};
use dwow_serial::{deserialize, serialize, Encodable};

use crate::{
    error::DrainProtectionError,
    model::{
        ExitParamsV1, ExitUpdateV1, LockParamsV1, LockUpdateV1,
        ProposeParamsV1, ProposeUpdateV1, ProtectedFund, RateLimit, UnlockParamsV1,
        UnlockUpdateV1, VoteAction, VoteParamsV1, VoteProposal, VoteThresholds, VoteUpdateV1,
    },
    DrainProtectionFunction,
    DRAIN_PROTECTION_CONTRACT_EXITS_TREE, DRAIN_PROTECTION_CONTRACT_FUNDS_TREE,
    DRAIN_PROTECTION_CONTRACT_INFO_TREE, DRAIN_PROTECTION_CONTRACT_MEMBERS_TREE,
    DRAIN_PROTECTION_CONTRACT_PROPOSALS_TREE, DRAIN_PROTECTION_CONTRACT_TRANSFERS_TREE,
    DRAIN_PROTECTION_CONTRACT_VOTES_TREE,
};

dwow_sdk::define_contract!(
    init: init_contract,
    exec: process_instruction,
    apply: process_update,
    metadata: get_metadata
);

// ============================================================================
// INITIALIZATION
// ============================================================================

/// Initialize DrainProtection contract state
///
/// Sets up:
/// - Info tree (version, config)
/// - Funds tree (protected fund records)
/// - Proposals tree (pending votes)
/// - Members tree (weights for exit)
/// - Transfer history tree (rate limiting)
/// - Exits tree (processed exits)
/// - Vote history tree (prevent double-voting)
pub fn init_contract(cid: dwow_sdk::crypto::ContractId, _ix: &[u8]) -> ContractResult {
    msg!("[drain_protection::init_contract] Initializing DrainProtection contract");

    // Initialize info tree
    let info_db = wasm::db::db_init(cid, DRAIN_PROTECTION_CONTRACT_INFO_TREE)?;
    wasm::db::db_set(info_db, b"db_version", env!("CARGO_PKG_VERSION").as_bytes())?;

    // Initialize funds tree
    wasm::db::db_init(cid, DRAIN_PROTECTION_CONTRACT_FUNDS_TREE)?;

    // Initialize proposals tree
    wasm::db::db_init(cid, DRAIN_PROTECTION_CONTRACT_PROPOSALS_TREE)?;

    // Initialize members tree
    wasm::db::db_init(cid, DRAIN_PROTECTION_CONTRACT_MEMBERS_TREE)?;

    // Initialize transfer history tree
    wasm::db::db_init(cid, DRAIN_PROTECTION_CONTRACT_TRANSFERS_TREE)?;

    // Initialize exits tree
    wasm::db::db_init(cid, DRAIN_PROTECTION_CONTRACT_EXITS_TREE)?;

    // Initialize votes tree
    wasm::db::db_init(cid, DRAIN_PROTECTION_CONTRACT_VOTES_TREE)?;

    msg!("[drain_protection::init_contract] DrainProtection contract initialized");

    let exit_v1_bincode = include_bytes!("../proof/exit_v1.zk.bin");
    wasm::db::zkas_db_set(&exit_v1_bincode[..])?;

    Ok(())
}

// ============================================================================
// METADATA (ZK proof verification)
// ============================================================================

/// Fetch metadata for ZK proof verification
fn get_metadata(_cid: dwow_sdk::crypto::ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<dwow_sdk::dark_tree::DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = DrainProtectionFunction::try_from(self_.data[0])?;

    let metadata = match func {
        DrainProtectionFunction::ExitV1 => {
            let params: ExitParamsV1 = deserialize(&self_.data[1..])?;
            drain_protection_exit_get_metadata_v1(params)?
        }
        // No ZK circuits for other functions yet
        _ => vec![],
    };

    wasm::util::set_return_data(&metadata)
}

fn drain_protection_exit_get_metadata_v1(
    params: ExitParamsV1,
) -> Result<Vec<u8>, ContractError> {
    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    zk_public_inputs.push((
        crate::DRAIN_PROTECTION_CONTRACT_ZKAS_EXIT_NS_V1.to_string(),
        vec![
            params.fund_id,
            params.member_pubkey.x(),
            params.member_pubkey.y(),
            params.dao_escrow_bulla,
            params.dao_membership_note,
            params.effective_weight,
        ],
    ));
    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    Ok(metadata)
}

// ============================================================================
// INSTRUCTION PROCESSING
// ============================================================================

/// Verify state transition and produce update if valid
fn process_instruction(cid: dwow_sdk::crypto::ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<dwow_sdk::dark_tree::DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx];
    let func = DrainProtectionFunction::try_from(self_.data.data[0])?;

    msg!("[drain_protection::process_instruction] Processing function: {:?}", func);

    match func {
        DrainProtectionFunction::InitializeV1 => {
            let params: crate::model::InitializeParamsV1 =
                deserialize(&self_.data.data[1..])?;
            let update = init_fund_process_instruction_v1(cid, params)?;
            let _ = wasm::util::set_return_data(&update);
        }
        DrainProtectionFunction::ProposeV1 => {
            let params: ProposeParamsV1 = deserialize(&self_.data.data[1..])?;
            let update = propose_process_instruction_v1(cid, params)?;
            let _ = wasm::util::set_return_data(&update);
        }
        DrainProtectionFunction::VoteV1 => {
            let params: VoteParamsV1 = deserialize(&self_.data.data[1..])?;
            let update = vote_process_instruction_v1(cid, params)?;
            let _ = wasm::util::set_return_data(&update);
        }
        DrainProtectionFunction::ExecuteV1 => {
            let params: crate::model::ExecuteParamsV1 =
                deserialize(&self_.data.data[1..])?;
            let update = execute_process_instruction_v1(cid, params)?;
            let _ = wasm::util::set_return_data(&update);
        }
        DrainProtectionFunction::ExitV1 => {
            // Validate children_indexes for token payout
            if self_.children_indexes.len() != 1 {
                msg!("[drain_protection::ExitV1] Error: Expected 1 child call (money_v3::transfer_v1), got {}", self_.children_indexes.len());
                return Err(DrainProtectionError::InvalidChildrenIndexes.into())
            }
            let child_idx = self_.children_indexes[0];
            if calls[child_idx].data.data[0] != 0x04 {
                msg!("[drain_protection::ExitV1] Error: Expected money_v3::transfer_v1 (0x04), got 0x{:02x}", calls[child_idx].data.data[0]);
                return Err(DrainProtectionError::InvalidChildCall.into())
            }

            let params: ExitParamsV1 = deserialize(&self_.data.data[1..])?;
            let update = exit_process_instruction_v1(cid, params)?;
            let _ = wasm::util::set_return_data(&update);
        }
        DrainProtectionFunction::TransferV1 => {
            // Validate children_indexes for token transfer
            if self_.children_indexes.len() != 1 {
                msg!("[drain_protection::TransferV1] Error: Expected 1 child call (money_v3::transfer_v1), got {}", self_.children_indexes.len());
                return Err(DrainProtectionError::InvalidChildrenIndexes.into())
            }
            let child_idx = self_.children_indexes[0];
            if calls[child_idx].data.data[0] != 0x04 {
                msg!("[drain_protection::TransferV1] Error: Expected money_v3::transfer_v1 (0x04), got 0x{:02x}", calls[child_idx].data.data[0]);
                return Err(DrainProtectionError::InvalidChildCall.into())
            }

            let params: crate::model::TransferParamsV1 =
                deserialize(&self_.data.data[1..])?;
            let update = transfer_process_instruction_v1(cid, params)?;
            let _ = wasm::util::set_return_data(&update);
        }
        DrainProtectionFunction::LockV1 => {
            let params: LockParamsV1 = deserialize(&self_.data.data[1..])?;
            let update = lock_process_instruction_v1(cid, params)?;
            let _ = wasm::util::set_return_data(&update);
        }
        DrainProtectionFunction::UnlockV1 => {
            let params: UnlockParamsV1 = deserialize(&self_.data.data[1..])?;
            let update = unlock_process_instruction_v1(cid, params)?;
            let _ = wasm::util::set_return_data(&update);
        }
        DrainProtectionFunction::UpdateConfigV1 => {
            let params: crate::model::UpdateConfigParamsV1 =
                deserialize(&self_.data.data[1..])?;
            let update = update_config_process_instruction_v1(cid, params)?;
            let _ = wasm::util::set_return_data(&update);
        }
    }

    Ok(())
}

/// `process_instruction` for InitializeV1
fn init_fund_process_instruction_v1(
    cid: dwow_sdk::crypto::ContractId,
    params: crate::model::InitializeParamsV1,
) -> Result<Vec<u8>, ContractError> {
    msg!("[InitializeV1] Initializing protected fund");

    let funds_db = wasm::db::db_lookup(cid, DRAIN_PROTECTION_CONTRACT_FUNDS_TREE)?;

    // Check fund doesn't already exist
    if wasm::db::db_contains_key(funds_db, &serialize(&params.fund_id))? {
        return Err(DrainProtectionError::MemberAlreadyExists.into())
    }

    // Create the protected fund
    let fund = ProtectedFund {
        version: 1,
        instance_seed: params.instance_seed,
        id: params.fund_id,
        total_funds: 0,
        spend_authority: params.spend_authority,
        lock_state: crate::model::LockState::Unlocked,
        rate_limit: RateLimit::default(),
        thresholds: VoteThresholds::default(),
        drain_config: crate::model::DrainConfig::default(),
        members: vec![],
        lock_expires_at: 0,
        authority_change_timelock: 0,
        created_at: wasm::util::get_verifying_block_height()? as u64,
        exit_queue_state: vec![],
        circuit_breaker_state: None,
        guardian_pause_state: None,
        dead_mans_switch_state: None,
        no_loss_reserve_balance: 0,
        observation_pending: vec![],
    };

    // Store fund directly (InitializeUpdateV1 only has fund_id)
    let key = serialize(&fund.id);
    let value = serialize(&fund);
    wasm::db::db_set(funds_db, &key, &value)?;

    let update = crate::model::InitializeUpdateV1 { instance_seed: params.instance_seed, fund_id: fund.id };
    Ok(serialize(&update))
}

/// `process_instruction` for ProposeV1
fn propose_process_instruction_v1(
    cid: dwow_sdk::crypto::ContractId,
    params: ProposeParamsV1,
) -> Result<Vec<u8>, ContractError> {
    msg!("[ProposeV1] Creating vote proposal");

    let funds_db = wasm::db::db_lookup(cid, DRAIN_PROTECTION_CONTRACT_FUNDS_TREE)?;
    let proposals_db = wasm::db::db_lookup(cid, DRAIN_PROTECTION_CONTRACT_PROPOSALS_TREE)?;

    // Verify fund exists and is not locked
    let fund_data = wasm::db::db_get(funds_db, &serialize(&params.action.fund_id()))?
        .ok_or(DrainProtectionError::MemberNotFound)?;
    let fund: ProtectedFund = deserialize(&fund_data)?;

    if fund.lock_state == crate::model::LockState::Locked {
        if (wasm::util::get_verifying_block_height()? as u64) < fund.lock_expires_at {
            return Err(DrainProtectionError::FundsLocked.into())
        }
    }

    // Create proposal
    let proposal_id =
        dwow_sdk::crypto::poseidon_hash([fund.id, pallas::Base::from(wasm::util::get_verifying_block_height()? as u64)]);

    let proposal = VoteProposal {
        version: 1,
        id: proposal_id,
        action: params.action.clone(),
        started_at: wasm::util::get_verifying_block_height()? as u64,
        ends_at: wasm::util::get_verifying_block_height()? as u64 + params.vote_period_blocks,
        yes_votes: 0,
        no_votes: 0,
        concluded: false,
    };

    wasm::db::db_set(proposals_db, &serialize(&proposal_id), &serialize(&proposal))?;

    let update = ProposeUpdateV1 { proposal_id };
    Ok(serialize(&update))
}

/// `process_instruction` for VoteV1
fn vote_process_instruction_v1(
    cid: dwow_sdk::crypto::ContractId,
    params: VoteParamsV1,
) -> Result<Vec<u8>, ContractError> {
    msg!("[VoteV1] Casting vote on proposal");

    let proposals_db = wasm::db::db_lookup(cid, DRAIN_PROTECTION_CONTRACT_PROPOSALS_TREE)?;
    let votes_db = wasm::db::db_lookup(cid, DRAIN_PROTECTION_CONTRACT_VOTES_TREE)?;

    // Fetch proposal
    let proposal_data = wasm::db::db_get(proposals_db, &serialize(&params.proposal_id))?
        .ok_or(DrainProtectionError::ConfigurationError("Proposal not found".to_string()))?;
    let mut proposal: VoteProposal = deserialize(&proposal_data)?;

    // Check voting period hasn't ended
    let current_block = wasm::util::get_verifying_block_height()? as u64;
    if current_block > proposal.ends_at {
        return Err(DrainProtectionError::ConfigurationError("Voting period ended".to_string()).into())
    }

    // Check not already voted (key hashed so pubkey is not exposed as raw DB key)
    let vote_key = poseidon_hash([params.proposal_id, params.voter_pubkey.x(), params.voter_pubkey.y()]).to_repr().to_vec();
    if wasm::db::db_contains_key(votes_db, &vote_key)? {
        return Err(DrainProtectionError::ConfigurationError("Already voted".to_string()).into())
    }

    // Record vote
    if params.vote {
        proposal.yes_votes += 1;
    } else {
        proposal.no_votes += 1;
    }

    wasm::db::db_set(votes_db, &vote_key, &[1])?;
    wasm::db::db_set(proposals_db, &serialize(&proposal.id), &serialize(&proposal))?;

    let update = VoteUpdateV1 { proposal_id: proposal.id, yes_votes: proposal.yes_votes, no_votes: proposal.no_votes };
    Ok(serialize(&update))
}

/// `process_instruction` for ExecuteV1
fn execute_process_instruction_v1(
    cid: dwow_sdk::crypto::ContractId,
    params: crate::model::ExecuteParamsV1,
) -> Result<Vec<u8>, ContractError> {
    msg!("[ExecuteV1] Executing proposal");

    let proposals_db = wasm::db::db_lookup(cid, DRAIN_PROTECTION_CONTRACT_PROPOSALS_TREE)?;
    let funds_db = wasm::db::db_lookup(cid, DRAIN_PROTECTION_CONTRACT_FUNDS_TREE)?;

    // Fetch proposal
    let proposal_data = wasm::db::db_get(proposals_db, &serialize(&params.proposal_id))?
        .ok_or(DrainProtectionError::ConfigurationError("Proposal not found".to_string()))?;
    let mut proposal: VoteProposal = deserialize(&proposal_data)?;

    // Check voting period has ended
    let current_block: u64 = wasm::util::get_verifying_block_height()? as u64;
    if current_block < proposal.ends_at {
        return Err(DrainProtectionError::ConfigurationError("Voting period not ended".to_string()).into())
    }

    // Check not already concluded
    if proposal.concluded {
        return Err(DrainProtectionError::ConfigurationError("Proposal already concluded".to_string()).into())
    }

    // Verify vote thresholds
    let fund_data = wasm::db::db_get(funds_db, &serialize(&proposal.action.fund_id()))?
        .ok_or(DrainProtectionError::MemberNotFound)?;
    let fund: ProtectedFund = deserialize(&fund_data)?;

    let total_votes = proposal.yes_votes + proposal.no_votes;
    let total_members = fund.members.len() as u64;

    // Check quorum
    let quorum_bps = (total_votes * 10_000 / total_members.max(1)) as u64;
    if quorum_bps < fund.thresholds.quorum_min_bps {
        return Err(DrainProtectionError::QuorumNotReached {
            required: fund.thresholds.quorum_min_bps,
            actual: quorum_bps,
        }.into())
    }

    // Check threshold
    let yes_bps = (proposal.yes_votes * 10_000 / total_votes.max(1)) as u64;
    let required_thresh = match &proposal.action {
        VoteAction::LargeWithdrawal { .. } => fund.thresholds.large_withdrawal_thresh,
        VoteAction::LockFunds => fund.thresholds.lock_unlock_thresh,
        VoteAction::UnlockFunds => fund.thresholds.lock_unlock_thresh,
        VoteAction::ChangeSpendAuthority { .. } => fund.thresholds.authority_change_thresh,
        VoteAction::RenewLock => fund.thresholds.lock_unlock_thresh,
    };

    if yes_bps < required_thresh {
        return Err(DrainProtectionError::InsufficientVoteThreshold {
            required: required_thresh,
            actual: yes_bps,
        }.into())
    }

    proposal.concluded = true;
    wasm::db::db_set(proposals_db, &serialize(&proposal.id), &serialize(&proposal))?;

    let update = crate::model::ExecuteUpdateV1 { proposal_id: proposal.id, action_executed: proposal.action };
    Ok(serialize(&update))
}

/// `process_instruction` for ExitV1
fn exit_process_instruction_v1(
    cid: dwow_sdk::crypto::ContractId,
    params: ExitParamsV1,
) -> Result<Vec<u8>, ContractError> {
    msg!("[ExitV1] Processing member exit");

    let funds_db = wasm::db::db_lookup(cid, DRAIN_PROTECTION_CONTRACT_FUNDS_TREE)?;
    let exits_db = wasm::db::db_lookup(cid, DRAIN_PROTECTION_CONTRACT_EXITS_TREE)?;

    // Find the fund by its ID
    let fund_data = wasm::db::db_get(funds_db, &serialize(&params.fund_id))?
        .ok_or(DrainProtectionError::MemberNotFound)?;
    let fund: ProtectedFund = deserialize(&fund_data)?;

    // Calculate exit value with haircut
    // exit_value = (weight / total_weight) × total_funds × 0.666
    let total_weight: u64 = fund.members.iter().map(|m| m.effective_weight(params.current_block)).sum();
    let member_weight = params.contribution_weight;

    if member_weight == 0 {
        return Err(DrainProtectionError::ZeroContributionWeight.into())
    }

    let haircut_bps = 3333; // 33.33%
    let exit_value = (member_weight * fund.total_funds / total_weight.max(1)) * (10_000 - haircut_bps) / 10_000;

    let exit_id = dwow_sdk::crypto::poseidon_hash([
        fund.id,
        pallas::Base::from(params.current_block),
    ]);

    wasm::db::db_set(exits_db, &serialize(&exit_id), &[1])?;

    let update = ExitUpdateV1 {
        exit_id,
        member_pubkey: params.member_pubkey,
        payout_value: exit_value,
        haircut_collected: (member_weight * fund.total_funds / total_weight.max(1)) * haircut_bps / 10_000,
    };
    Ok(serialize(&update))
}

/// `process_instruction` for TransferV1
fn transfer_process_instruction_v1(
    cid: dwow_sdk::crypto::ContractId,
    params: crate::model::TransferParamsV1,
) -> Result<Vec<u8>, ContractError> {
    msg!("[TransferV1] Processing transfer");

    let funds_db = wasm::db::db_lookup(cid, DRAIN_PROTECTION_CONTRACT_FUNDS_TREE)?;
    let transfers_db = wasm::db::db_lookup(cid, DRAIN_PROTECTION_CONTRACT_TRANSFERS_TREE)?;

    let fund_data = wasm::db::db_get(funds_db, &serialize(&params.fund_id))?
        .ok_or(DrainProtectionError::MemberNotFound)?;
    let fund: ProtectedFund = deserialize(&fund_data)?;

    // Check if locked
    if fund.lock_state == crate::model::LockState::Locked {
        let current_block: u64 = wasm::util::get_verifying_block_height()? as u64;
        if current_block < fund.lock_expires_at {
            return Err(DrainProtectionError::FundsLocked.into())
        }
    }

    // Check rate limit
    let current_block: u64 = wasm::util::get_verifying_block_height()? as u64;
    let rate_limited = check_rate_limit(&fund, transfers_db, params.amount, current_block)?;

    if rate_limited && !params.exceeds_rate_limit {
        return Err(DrainProtectionError::WithdrawalExceedsRateLimit.into())
    }

    if params.exceeds_rate_limit {
        // Verify vote proposal was approved
        if let Some(proposal_id) = params.vote_proposal_id {
            let proposals_db = wasm::db::db_lookup(cid, DRAIN_PROTECTION_CONTRACT_PROPOSALS_TREE)?;
            let proposal_data = wasm::db::db_get(proposals_db, &serialize(&proposal_id))?
                .ok_or(DrainProtectionError::ConfigurationError("Proposal not found".to_string()))?;
            let proposal: VoteProposal = deserialize(&proposal_data)?;

            if !proposal.concluded {
                return Err(DrainProtectionError::ConfigurationError("Proposal not concluded".to_string()).into())
            }
        }
    }

    // Record transfer for rate limiting
    let record = crate::model::TransferRecord { version: 1, block: current_block, amount: params.amount };
    let transfer_key = dwow_sdk::crypto::poseidon_hash([current_block.into()]);
    wasm::db::db_set(transfers_db, &serialize(&transfer_key), &serialize(&record))?;

    let update = crate::model::TransferUpdateV1 {
        amount: params.amount,
        recipient: params.recipient,
        rate_limited,
    };
    Ok(serialize(&update))
}

/// `process_instruction` for LockV1
fn lock_process_instruction_v1(
    cid: dwow_sdk::crypto::ContractId,
    params: LockParamsV1,
) -> Result<Vec<u8>, ContractError> {
    msg!("[LockV1] Locking funds");

    let funds_db = wasm::db::db_lookup(cid, DRAIN_PROTECTION_CONTRACT_FUNDS_TREE)?;

    let fund_data = wasm::db::db_get(funds_db, &serialize(&params.fund_id))?
        .ok_or(DrainProtectionError::MemberNotFound)?;
    let mut fund: ProtectedFund = deserialize(&fund_data)?;

    let current_block: u64 = wasm::util::get_verifying_block_height()? as u64;

    // Max lock duration is 7 days worth of blocks (~30240 blocks/day at 5min blocks)
    let max_lock_blocks = 7 * 30240;
    if params.duration_blocks > max_lock_blocks {
        return Err(DrainProtectionError::ConfigurationError("Lock duration too long".to_string()).into())
    }

    fund.lock_state = crate::model::LockState::Locked;
    fund.lock_expires_at = current_block + params.duration_blocks;

    wasm::db::db_set(funds_db, &serialize(&fund.id), &serialize(&fund))?;

    let update = LockUpdateV1 { locked_until: fund.lock_expires_at };
    Ok(serialize(&update))
}

/// `process_instruction` for UnlockV1
fn unlock_process_instruction_v1(
    cid: dwow_sdk::crypto::ContractId,
    params: UnlockParamsV1,
) -> Result<Vec<u8>, ContractError> {
    msg!("[UnlockV1] Unlocking funds");

    let funds_db = wasm::db::db_lookup(cid, DRAIN_PROTECTION_CONTRACT_FUNDS_TREE)?;

    let fund_data = wasm::db::db_get(funds_db, &serialize(&params.fund_id))?
        .ok_or(DrainProtectionError::MemberNotFound)?;
    let mut fund: ProtectedFund = deserialize(&fund_data)?;

    // Check timelock (24hr after lock expires)
    let current_block: u64 = wasm::util::get_verifying_block_height()? as u64;
    if fund.lock_state == crate::model::LockState::Locked {
        if current_block < fund.lock_expires_at + 1440 {
            // 24hr timelock
            return Err(DrainProtectionError::UnlockTimelockNotExpired {
                needed: fund.lock_expires_at + 1440 - current_block,
            }.into())
        }
    }

    fund.lock_state = crate::model::LockState::Unlocked;

    wasm::db::db_set(funds_db, &serialize(&fund.id), &serialize(&fund))?;

    let update = UnlockUpdateV1 { unlocked_at: current_block };
    Ok(serialize(&update))
}

/// `process_instruction` for UpdateConfigV1
fn update_config_process_instruction_v1(
    cid: dwow_sdk::crypto::ContractId,
    params: crate::model::UpdateConfigParamsV1,
) -> Result<Vec<u8>, ContractError> {
    msg!("[UpdateConfigV1] Updating configuration");

    let funds_db = wasm::db::db_lookup(cid, DRAIN_PROTECTION_CONTRACT_FUNDS_TREE)?;

    let fund_data = wasm::db::db_get(funds_db, &serialize(&params.fund_id))?
        .ok_or(DrainProtectionError::MemberNotFound)?;
    let mut fund: ProtectedFund = deserialize(&fund_data)?;

    let current_block: u64 = wasm::util::get_verifying_block_height()? as u64;

    // Update rate limit if provided
    if let Some(rate_limit) = params.rate_limit {
        fund.rate_limit = rate_limit;
    }

    // Update thresholds if provided
    if let Some(thresholds) = params.thresholds {
        fund.thresholds = thresholds;
    }

    // Update spend authority if provided (subject to 48hr timelock)
    if let Some(new_authority) = params.new_spend_authority {
        if current_block < fund.authority_change_timelock {
            return Err(DrainProtectionError::AuthorityChangeTimelock.into())
        }
        fund.authority_change_timelock = current_block + (48 * 60); // 48hr in minutes
        fund.spend_authority = new_authority;
    }

    wasm::db::db_set(funds_db, &serialize(&fund.id), &serialize(&fund))?;

    let update = crate::model::UpdateConfigUpdateV1 {
        authority_change_timelock: if params.new_spend_authority.is_some() {
            Some(fund.authority_change_timelock)
        } else {
            None
        },
    };
    Ok(serialize(&update))
}

// ============================================================================
// STATE UPDATE
// ============================================================================

/// Write state update after successful verification.
/// State is written directly in process_instruction to keep the DB write
/// co-located with validation logic. This function is a no-op that confirms
/// the update was accepted by consensus.
fn process_update(_cid: dwow_sdk::crypto::ContractId, _update_data: &[u8]) -> ContractResult {
    msg!("[drain_protection::process_update] Update applied");
    Ok(())
}

// ============================================================================
// HELPERS
// ============================================================================

/// Check if a transfer exceeds the rate limit
fn check_rate_limit(
    fund: &ProtectedFund,
    _transfers_db: u32,
    amount: u64,
    current_block: u64,
) -> Result<bool, ContractError> {
    // Calculate total transferred in averaging window
    let _window_start = current_block.saturating_sub(fund.rate_limit.averaging_window_blocks);
    let _total_recent = 0u64;

    // This is a simplified check - in production, iterate over transfer history
    let rate_threshold = fund.total_funds * fund.rate_limit.base_rate_bps / 10_000;

    if amount > rate_threshold {
        return Ok(true)
    }

    Ok(false)
}

// ============================================================================
// VoteAction Extension
// ============================================================================

impl VoteAction {
    /// Get the fund ID this action applies to.
    ///
    /// NOTE: Current implementation assumes single-fund (returns zero).
    /// When multi-fund support is added, each VoteAction variant should
    /// carry its own fund_id field rather than using this default.
    fn fund_id(&self) -> pallas::Base {
        match self {
            VoteAction::LargeWithdrawal { .. } => pallas::Base::zero(),
            VoteAction::LockFunds => pallas::Base::zero(),
            VoteAction::UnlockFunds => pallas::Base::zero(),
            VoteAction::ChangeSpendAuthority { .. } => pallas::Base::zero(),
            VoteAction::RenewLock => pallas::Base::zero(),
        }
    }
}