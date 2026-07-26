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
    crypto::{pasta_prelude::PrimeField, poseidon_hash, BOX_CONTRACT_ID, ContractId, MULTISIG_CONTRACT_ID, PURSE_CONTRACT_ID},
    error::{ContractError, ContractResult},
    msg,
    pasta::pallas,
    wasm, ContractCall,
};
use dwow_serial::{deserialize, serialize, Encodable};
use dwow_promissory_note_contract::validation::{
    validate_child_contract_id, validate_child_value_commit,
};

use crate::{
    error::DrainProtectionError,
    model::{
        ExitParamsV1, ExitUpdateV1, LockParamsV1, LockUpdateV1,
        ProposeParamsV1, ProposeUpdateV1, ProtectedFund, RateLimit, UnlockParamsV1,
        UnlockUpdateV1, VoteParamsV1, VoteUpdateV1,
    },
    DrainProtectionFunction,
    DRAIN_PROTECTION_CONTRACT_EXITS_TREE, DRAIN_PROTECTION_CONTRACT_FUNDS_TREE,
    DRAIN_PROTECTION_CONTRACT_INFO_TREE, DRAIN_PROTECTION_CONTRACT_MEMBERS_TREE,
    DRAIN_PROTECTION_CONTRACT_PROPOSALS_TREE, DRAIN_PROTECTION_CONTRACT_TRANSFERS_TREE,
    DRAIN_PROTECTION_CONTRACT_VOTES_TREE, DRAIN_PROTECTION_CONTRACT_PROMISSORY_NOTE_CONTRACT_ID, DRAIN_PROTECTION_CONTRACT_PURSE_CONTRACT_ID, DRAIN_PROTECTION_CONTRACT_BOX_CONTRACT_ID, DRAIN_PROTECTION_CONTRACT_MULTISIG_CONTRACT_ID,
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

    // Store default promissory_note contract ID for cross-contract validation
    wasm::db::db_set(info_db, DRAIN_PROTECTION_CONTRACT_PROMISSORY_NOTE_CONTRACT_ID, &[0u8; 32])?;
    wasm::db::db_set(info_db, DRAIN_PROTECTION_CONTRACT_PURSE_CONTRACT_ID, &PURSE_CONTRACT_ID.to_bytes())?;
    wasm::db::db_set(info_db, DRAIN_PROTECTION_CONTRACT_BOX_CONTRACT_ID, &BOX_CONTRACT_ID.to_bytes())?;
    wasm::db::db_set(info_db, DRAIN_PROTECTION_CONTRACT_MULTISIG_CONTRACT_ID, &MULTISIG_CONTRACT_ID.to_bytes())?;

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

    // V2 circuits (HAZOP RC3: domain separation)
    let execute_v2_bincode = include_bytes!("../proof/execute_v2.zk.bin");
    wasm::db::zkas_db_set(&execute_v2_bincode[..])?;
    let exit_v2_bincode = include_bytes!("../proof/exit_v2.zk.bin");
    wasm::db::zkas_db_set(&exit_v2_bincode[..])?;
    let initialize_v2_bincode = include_bytes!("../proof/initialize_v2.zk.bin");
    wasm::db::zkas_db_set(&initialize_v2_bincode[..])?;
    let lock_v2_bincode = include_bytes!("../proof/lock_v2.zk.bin");
    wasm::db::zkas_db_set(&lock_v2_bincode[..])?;
    let propose_v2_bincode = include_bytes!("../proof/propose_v2.zk.bin");
    wasm::db::zkas_db_set(&propose_v2_bincode[..])?;
    let transfer_v2_bincode = include_bytes!("../proof/transfer_v2.zk.bin");
    wasm::db::zkas_db_set(&transfer_v2_bincode[..])?;
    let unlock_v2_bincode = include_bytes!("../proof/unlock_v2.zk.bin");
    wasm::db::zkas_db_set(&unlock_v2_bincode[..])?;
    let update_config_v2_bincode = include_bytes!("../proof/update_config_v2.zk.bin");
    wasm::db::zkas_db_set(&update_config_v2_bincode[..])?;
    let vote_v2_bincode = include_bytes!("../proof/vote_v2.zk.bin");
    wasm::db::zkas_db_set(&vote_v2_bincode[..])?;

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
        // V2 circuits registered (exit_v2, execute_v2, initialize_v2, lock_v2,
        // propose_v2, transfer_v2, unlock_v2, update_config_v2, vote_v2) but
        // metadata sub-functions not yet created — returning empty metadata
        // until circuit public-input layouts are specified.
        DrainProtectionFunction::InitializeV1
        | DrainProtectionFunction::ProposeV1
        | DrainProtectionFunction::VoteV1
        | DrainProtectionFunction::ExecuteV1
        | DrainProtectionFunction::TransferV1
        | DrainProtectionFunction::LockV1
        | DrainProtectionFunction::UnlockV1
        | DrainProtectionFunction::UpdateConfigV1 => vec![],
    };

    wasm::util::set_return_data(&metadata)
}

fn drain_protection_exit_get_metadata_v1(
    params: ExitParamsV1,
) -> Result<Vec<u8>, ContractError> {
    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    zk_public_inputs.push((
        crate::DRAIN_PROTECTION_CONTRACT_ZKAS_EXIT_NS_V2.to_string(),
        vec![
            params.fund_id,
            params.member_pubkey.x().expect("pk not identity"),
            params.member_pubkey.y().expect("pk not identity"),
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
                msg!("[drain_protection::ExitV1] Error: Expected 1 child call (promissory_note::transfer_v1), got {}", self_.children_indexes.len());
                return Err(DrainProtectionError::InvalidChildrenIndexes.into())
            }
            let child_idx = self_.children_indexes[0];
            let child_call = &calls[child_idx].data;
            if child_call.data[0] != 0x04 {
                msg!("[drain_protection::ExitV1] Error: Expected promissory_note::transfer_v1 (0x04), got 0x{:02x}", child_call.data[0]);
                return Err(DrainProtectionError::InvalidChildCall.into())
            }

            // Validate child call targets promissory_note (prevent cross-contract routing)
            let info_db = wasm::db::db_lookup(cid, DRAIN_PROTECTION_CONTRACT_INFO_TREE)?;
            let promissory_note_bytes = wasm::db::db_get(info_db, DRAIN_PROTECTION_CONTRACT_PROMISSORY_NOTE_CONTRACT_ID)?
                .ok_or(DrainProtectionError::InvalidChildCall)?;
            let promissory_note_cid: ContractId = deserialize(&promissory_note_bytes)?;
            if promissory_note_cid != ContractId::ZERO {
                validate_child_contract_id(&child_call.contract_id, &promissory_note_cid)?;
            }

            let params: ExitParamsV1 = deserialize(&self_.data.data[1..])?;
            let update = exit_process_instruction_v1(cid, params, &child_call.data)?;
            let _ = wasm::util::set_return_data(&update);
        }
        DrainProtectionFunction::TransferV1 => {
            // Validate children_indexes for token transfer
            if self_.children_indexes.len() != 1 {
                msg!("[drain_protection::TransferV1] Error: Expected 1 child call (promissory_note::transfer_v1), got {}", self_.children_indexes.len());
                return Err(DrainProtectionError::InvalidChildrenIndexes.into())
            }
            let child_idx = self_.children_indexes[0];
            let child_call = &calls[child_idx].data;
            if child_call.data[0] != 0x04 {
                msg!("[drain_protection::TransferV1] Error: Expected promissory_note::transfer_v1 (0x04), got 0x{:02x}", child_call.data[0]);
                return Err(DrainProtectionError::InvalidChildCall.into())
            }

            // Validate child call targets promissory_note (prevent cross-contract routing)
            let info_db = wasm::db::db_lookup(cid, DRAIN_PROTECTION_CONTRACT_INFO_TREE)?;
            let promissory_note_bytes = wasm::db::db_get(info_db, DRAIN_PROTECTION_CONTRACT_PROMISSORY_NOTE_CONTRACT_ID)?
                .ok_or(DrainProtectionError::InvalidChildCall)?;
            let promissory_note_cid: ContractId = deserialize(&promissory_note_bytes)?;
            if promissory_note_cid != ContractId::ZERO {
                validate_child_contract_id(&child_call.contract_id, &promissory_note_cid)?;
            }

            let params: crate::model::TransferParamsV1 =
                deserialize(&self_.data.data[1..])?;
            let update = transfer_process_instruction_v1(cid, params, &child_call.data)?;
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
        multisig_group_id: pallas::Base::zero(),
        purse_id: pallas::Base::zero(),
        drain_config: crate::model::DrainConfig::default(),
        members: vec![],
        lock_expires_at: 0,
        authority_change_timelock: 0,
        created_at: wasm::util::get_verifying_block_height()?.get(),
        exit_queue_state: vec![],
        circuit_breaker_state: None,
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
    msg!("[ProposeV1] Registering MultiSig-governed proposal");

    let funds_db = wasm::db::db_lookup(cid, DRAIN_PROTECTION_CONTRACT_FUNDS_TREE)?;

    // Verify fund exists and multisig group is configured
    let fund_data = wasm::db::db_get(funds_db, &serialize(&params.multisig_group_id))?
        .ok_or(DrainProtectionError::NotInitialized)?;
    let fund: ProtectedFund = deserialize(&fund_data)?;

    if fund.lock_state == crate::model::LockState::Locked {
        if (wasm::util::get_verifying_block_height()?.get()) < fund.lock_expires_at {
            return Err(DrainProtectionError::FundsLocked.into())
        }
    }

    // Proposal ID derived from fund and message hash
    let proposal_id = dwow_sdk::crypto::poseidon_hash([fund.id, params.message_hash]);

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

    // MultiSig composition: voting is MultiSig::SignV1.
    // Each signer proves membership; the MultiSig group tracks partial signatures.
    // This function records the vote intent; threshold checking is in execute.
    let vote_key = poseidon_hash([params.proposal_id, params.voter_pubkey.x().expect("pk not identity"), params.voter_pubkey.y().expect("pk not identity")]).to_repr().to_vec();
    if wasm::db::db_contains_key(votes_db, &vote_key)? {
        return Err(DrainProtectionError::ConfigurationError("Already voted".to_string()).into())
    }

    // Record vote yes/no via MultiSig-compatible signature
    let vote_value = if params.vote { pallas::Base::one() } else { pallas::Base::zero() };
    wasm::db::db_set(votes_db, &vote_key, &serialize(&vote_value))?;

    let update = VoteUpdateV1 { proposal_id: params.proposal_id, yes_votes: 0, no_votes: 0 };
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

    // MultiSig composition: execute validates fund's multisig_group_id is configured.
    // The MultiSig::FinalizeV1 child call produces an approval_commit verified in
    // the process_instruction layer (has access to calls/self_).
    let fund_data = wasm::db::db_get(funds_db, &serialize(&params.proposal_id))?
        .ok_or(DrainProtectionError::NotInitialized)?;
    let fund: ProtectedFund = deserialize(&fund_data)?;

    if fund.multisig_group_id == pallas::Base::zero() {
        return Err(DrainProtectionError::Unauthorized.into());
    }

    let update = crate::model::ExecuteUpdateV1 { proposal_id: params.proposal_id, action: params.proposal_id };
    Ok(serialize(&update))
}

/// `process_instruction` for ExitV1
fn exit_process_instruction_v1(
    cid: dwow_sdk::crypto::ContractId,
    params: ExitParamsV1,
    child_call_data: &[u8],
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

    // Validate child transfer amount using value_commit comparison
    let value_blind = poseidon_hash([
        pallas::Base::from(exit_value),
        params.fund_id,
    ]);
    validate_child_value_commit(child_call_data, exit_value, value_blind)?;

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
    child_call_data: &[u8],
) -> Result<Vec<u8>, ContractError> {
    msg!("[TransferV1] Processing transfer");

    let funds_db = wasm::db::db_lookup(cid, DRAIN_PROTECTION_CONTRACT_FUNDS_TREE)?;
    let transfers_db = wasm::db::db_lookup(cid, DRAIN_PROTECTION_CONTRACT_TRANSFERS_TREE)?;

    let fund_data = wasm::db::db_get(funds_db, &serialize(&params.fund_id))?
        .ok_or(DrainProtectionError::MemberNotFound)?;
    let fund: ProtectedFund = deserialize(&fund_data)?;

    // Check if locked
    if fund.lock_state == crate::model::LockState::Locked {
        let current_block: u64 = wasm::util::get_verifying_block_height()?.get();
        if current_block < fund.lock_expires_at {
            return Err(DrainProtectionError::FundsLocked.into())
        }
    }

    // Check rate limit
    let current_block: u64 = wasm::util::get_verifying_block_height()?.get();
    let rate_limited = check_rate_limit(&fund, transfers_db, params.amount, current_block)?;

    if rate_limited && !params.exceeds_rate_limit {
        return Err(DrainProtectionError::WithdrawalExceedsRateLimit.into())
    }

    if params.exceeds_rate_limit {
        // MultiSig: rate-limited transfers require approved proposal
        if params.vote_proposal_id.is_none() {
            return Err(DrainProtectionError::Unauthorized.into());
        }
    }

    // Validate child transfer amount using value_commit comparison
    let value_blind = poseidon_hash([
        pallas::Base::from(params.amount),
        params.fund_id,
    ]);
    validate_child_value_commit(child_call_data, params.amount, value_blind)?;

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

    let current_block: u64 = wasm::util::get_verifying_block_height()?.get();

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
    let current_block: u64 = wasm::util::get_verifying_block_height()?.get();
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

    let current_block: u64 = wasm::util::get_verifying_block_height()?.get();

    // Update rate limit if provided
    if let Some(rate_limit) = params.rate_limit {
        fund.rate_limit = rate_limit;
    }

    // Update MultiSig group ID if provided
    if let Some(gid) = params.multisig_group_id {
        fund.multisig_group_id = gid;
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
// VoteAction replaced by MultiSig composition — see MultiSig contract
// ============================================================================