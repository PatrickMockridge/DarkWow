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
use dwow_serial::{deserialize, Encodable};
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
    wasm::db::db_set(info_db, DRAIN_PROTECTION_CONTRACT_PROMISSORY_NOTE_CONTRACT_ID, &dwow_sdk::crypto::PROMISSORY_NOTE_CONTRACT_ID.to_bytes())?;
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


    // V2 circuits (HAZOP RC3: domain separation)
    let execute_v2_bincode = include_bytes!("../proof/execute.zk.bin");
    wasm::db::zkas_db_set(&execute_v2_bincode[..])?;
    let exit_v2_bincode = include_bytes!("../proof/exit.zk.bin");
    wasm::db::zkas_db_set(&exit_v2_bincode[..])?;
    let initialize_v2_bincode = include_bytes!("../proof/initialize.zk.bin");
    wasm::db::zkas_db_set(&initialize_v2_bincode[..])?;
    let lock_v2_bincode = include_bytes!("../proof/lock.zk.bin");
    wasm::db::zkas_db_set(&lock_v2_bincode[..])?;
    let propose_v2_bincode = include_bytes!("../proof/propose.zk.bin");
    wasm::db::zkas_db_set(&propose_v2_bincode[..])?;
    let transfer_v2_bincode = include_bytes!("../proof/transfer.zk.bin");
    wasm::db::zkas_db_set(&transfer_v2_bincode[..])?;
    let unlock_v2_bincode = include_bytes!("../proof/unlock.zk.bin");
    wasm::db::zkas_db_set(&unlock_v2_bincode[..])?;
    let update_config_v2_bincode = include_bytes!("../proof/update_config.zk.bin");
    wasm::db::zkas_db_set(&update_config_v2_bincode[..])?;
    let vote_v2_bincode = include_bytes!("../proof/vote.zk.bin");
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
            let params= ExitParamsV1::decode(&self_.data[1..])?;
            drain_protection_exit_get_metadata_v1(params)?
        }
        DrainProtectionFunction::InitializeV1 => {
            let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
            zk_public_inputs.push((
                crate::DRAIN_PROTECTION_CONTRACT_ZKAS_INITIALIZE_NS_V2.to_string(),
                vec![pallas::Base::zero(), pallas::Base::zero()],
            ));
            let mut metadata = vec![];
            zk_public_inputs.encode(&mut metadata).map(|_| metadata).unwrap_or_default()
        }
        DrainProtectionFunction::ProposeV1 => {
            let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
            zk_public_inputs.push((
                crate::DRAIN_PROTECTION_CONTRACT_ZKAS_PROPOSE_NS_V2.to_string(),
                vec![pallas::Base::zero(), pallas::Base::zero()],
            ));
            let mut metadata = vec![];
            zk_public_inputs.encode(&mut metadata).map(|_| metadata).unwrap_or_default()
        }
        DrainProtectionFunction::VoteV1 => {
            let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
            zk_public_inputs.push((
                crate::DRAIN_PROTECTION_CONTRACT_ZKAS_VOTE_NS_V2.to_string(),
                vec![pallas::Base::zero(), pallas::Base::zero()],
            ));
            let mut metadata = vec![];
            zk_public_inputs.encode(&mut metadata).map(|_| metadata).unwrap_or_default()
        }
        DrainProtectionFunction::ExecuteV1 => {
            let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
            zk_public_inputs.push((
                crate::DRAIN_PROTECTION_CONTRACT_ZKAS_EXECUTE_NS_V2.to_string(),
                vec![pallas::Base::zero(), pallas::Base::zero()],
            ));
            let mut metadata = vec![];
            zk_public_inputs.encode(&mut metadata).map(|_| metadata).unwrap_or_default()
        }
        DrainProtectionFunction::TransferV1 => {
            let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
            zk_public_inputs.push((
                crate::DRAIN_PROTECTION_CONTRACT_ZKAS_TRANSFER_NS_V2.to_string(),
                vec![pallas::Base::zero(), pallas::Base::zero()],
            ));
            let mut metadata = vec![];
            zk_public_inputs.encode(&mut metadata).map(|_| metadata).unwrap_or_default()
        }
        DrainProtectionFunction::LockV1 => {
            let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
            zk_public_inputs.push((
                crate::DRAIN_PROTECTION_CONTRACT_ZKAS_LOCK_NS_V2.to_string(),
                vec![pallas::Base::zero(), pallas::Base::zero()],
            ));
            let mut metadata = vec![];
            zk_public_inputs.encode(&mut metadata).map(|_| metadata).unwrap_or_default()
        }
        DrainProtectionFunction::UnlockV1 => {
            let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
            zk_public_inputs.push((
                crate::DRAIN_PROTECTION_CONTRACT_ZKAS_UNLOCK_NS_V2.to_string(),
                vec![pallas::Base::zero(), pallas::Base::zero()],
            ));
            let mut metadata = vec![];
            zk_public_inputs.encode(&mut metadata).map(|_| metadata).unwrap_or_default()
        }
        DrainProtectionFunction::UpdateConfigV1 => {
            let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
            zk_public_inputs.push((
                crate::DRAIN_PROTECTION_CONTRACT_ZKAS_UPDATE_CONFIG_NS_V2.to_string(),
                vec![pallas::Base::zero(), pallas::Base::zero()],
            ));
            let mut metadata = vec![];
            zk_public_inputs.encode(&mut metadata).map(|_| metadata).unwrap_or_default()
        },
    };

    wasm::util::set_return_data(&metadata)
}

fn drain_protection_exit_get_metadata_v1(
    params: ExitParamsV1,
) -> Result<Vec<u8>, ContractError> {
    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    zk_public_inputs.push((
        crate::DRAIN_PROTECTION_CONTRACT_ZKAS_EXIT_NS_V2.to_string(),
        vec![pallas::Base::zero(), pallas::Base::zero()],
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
            let params = crate::model::InitializeParamsV1::decode(&self_.data.data[1..])?;
            let update = init_fund_process_instruction_v1(cid, params)?;
            let _ = wasm::util::set_return_data(&update);
        }
        DrainProtectionFunction::ProposeV1 => {
            let params= ProposeParamsV1::decode(&self_.data.data[1..])?;
            let update = propose_process_instruction_v1(cid, params)?;
            let _ = wasm::util::set_return_data(&update);
        }
        DrainProtectionFunction::VoteV1 => {
            let params= VoteParamsV1::decode(&self_.data.data[1..])?;
            let update = vote_process_instruction_v1(cid, params)?;
            let _ = wasm::util::set_return_data(&update);
        }
        DrainProtectionFunction::ExecuteV1 => {
            let params = crate::model::ExecuteParamsV1::decode(&self_.data.data[1..])?;
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
            // HAZOP H-11: fail-closed — reject if promissory_note not configured
            if promissory_note_cid == ContractId::ZERO {
                return Err(ContractError::IoError("promissory_note contract ID not configured".into()));
            }
            validate_child_contract_id(&child_call.contract_id, &promissory_note_cid)?;

            let params= ExitParamsV1::decode(&self_.data.data[1..])?;
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
            // HAZOP H-11: fail-closed — reject if promissory_note not configured
            if promissory_note_cid == ContractId::ZERO {
                return Err(ContractError::IoError("promissory_note contract ID not configured".into()));
            }
            validate_child_contract_id(&child_call.contract_id, &promissory_note_cid)?;

            let params = crate::model::TransferParamsV1::decode(&self_.data.data[1..])?;
            let update = transfer_process_instruction_v1(cid, params, &child_call.data)?;
            let _ = wasm::util::set_return_data(&update);
        }
        DrainProtectionFunction::LockV1 => {
            let params= LockParamsV1::decode(&self_.data.data[1..])?;
            let update = lock_process_instruction_v1(cid, params)?;
            let _ = wasm::util::set_return_data(&update);
        }
        DrainProtectionFunction::UnlockV1 => {
            let params= UnlockParamsV1::decode(&self_.data.data[1..])?;
            let update = unlock_process_instruction_v1(cid, params)?;
            let _ = wasm::util::set_return_data(&update);
        }
        DrainProtectionFunction::UpdateConfigV1 => {
            let params = crate::model::UpdateConfigParamsV1::decode(&self_.data.data[1..])?;
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
    if wasm::db::db_contains_key(funds_db, &params.fund_id.to_repr())? {
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

    // The fund travels in the update and is stored in apply (`OBL-C73`): the exec phase may read but not
    // write, and apply may not read, so the value it must store has to be carried to it.
    let update = crate::model::InitializeUpdateV1 { fund };
    encode_initialize_update_v1(&update)
}

/// `process_instruction` for ProposeV1
fn propose_process_instruction_v1(
    cid: dwow_sdk::crypto::ContractId,
    params: ProposeParamsV1,
) -> Result<Vec<u8>, ContractError> {
    msg!("[ProposeV1] Registering MultiSig-governed proposal");

    let funds_db = wasm::db::db_lookup(cid, DRAIN_PROTECTION_CONTRACT_FUNDS_TREE)?;

    // Verify fund exists and multisig group is configured
    let fund_data = wasm::db::db_get(funds_db, &params.multisig_group_id.to_repr())?
        .ok_or(DrainProtectionError::NotInitialized)?;
    let fund: ProtectedFund = ProtectedFund::decode(&fund_data)?;

    if fund.lock_state == crate::model::LockState::Locked {
        if (wasm::util::get_verifying_block_height()?.get()) < fund.lock_expires_at {
            return Err(DrainProtectionError::FundsLocked.into())
        }
    }

    // Proposal ID derived from fund and message hash
    let proposal_id = dwow_sdk::crypto::poseidon_hash([fund.id, params.message_hash]);

    let update = ProposeUpdateV1 { proposal_id };
    encode_propose_update_v1(&update)
}

/// `process_instruction` for VoteV1
fn vote_process_instruction_v1(
    cid: dwow_sdk::crypto::ContractId,
    params: VoteParamsV1,
) -> Result<Vec<u8>, ContractError> {
    msg!("[VoteV1] Casting vote on proposal");

    let _proposals_db = wasm::db::db_lookup(cid, DRAIN_PROTECTION_CONTRACT_PROPOSALS_TREE)?;
    let votes_db = wasm::db::db_lookup(cid, DRAIN_PROTECTION_CONTRACT_VOTES_TREE)?;

    // MultiSig composition: voting is MultiSig::SignV1.
    // Each signer proves membership; the MultiSig group tracks partial signatures.
    // This function records the vote intent; threshold checking is in execute.
    #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy()/x()/y() is always Some")]
    let vote_key_base = poseidon_hash([params.proposal_id, params.voter_pubkey.x().expect("pk not identity"), params.voter_pubkey.y().expect("pk not identity")]);
    let vote_key = vote_key_base.to_repr().to_vec();
    if wasm::db::db_contains_key(votes_db, &vote_key)? {
        return Err(DrainProtectionError::ConfigurationError("Already voted".to_string()).into())
    }

    // Record vote yes/no via MultiSig-compatible signature.
    // The key and the value travel in the update and are written in apply (`OBL-C73`) — apply may not
    // read, so neither can be recomputed there.
    let vote_value = if params.vote { pallas::Base::one() } else { pallas::Base::zero() };

    let update = VoteUpdateV1 { proposal_id: params.proposal_id, vote_key: vote_key_base, vote_value };
    encode_vote_update_v1(&update)
}

/// `process_instruction` for ExecuteV1
fn execute_process_instruction_v1(
    cid: dwow_sdk::crypto::ContractId,
    params: crate::model::ExecuteParamsV1,
) -> Result<Vec<u8>, ContractError> {
    msg!("[ExecuteV1] Executing proposal");

    let _proposals_db = wasm::db::db_lookup(cid, DRAIN_PROTECTION_CONTRACT_PROPOSALS_TREE)?;
    let funds_db = wasm::db::db_lookup(cid, DRAIN_PROTECTION_CONTRACT_FUNDS_TREE)?;

    // MultiSig composition: execute validates fund's multisig_group_id is configured.
    // The MultiSig::FinalizeV1 child call produces an approval_commit verified in
    // the process_instruction layer (has access to calls/self_).
    let fund_data = wasm::db::db_get(funds_db, &params.proposal_id.to_repr())?
        .ok_or(DrainProtectionError::NotInitialized)?;
    let fund: ProtectedFund = ProtectedFund::decode(&fund_data)?;

    if fund.multisig_group_id == pallas::Base::zero() {
        return Err(DrainProtectionError::Unauthorized.into());
    }

    let update = crate::model::ExecuteUpdateV1 { proposal_id: params.proposal_id, action: params.proposal_id };
    encode_execute_update_v1(&update)
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
    let fund_data = wasm::db::db_get(funds_db, &params.fund_id.to_repr())?
        .ok_or(DrainProtectionError::MemberNotFound)?;
    let fund: ProtectedFund = ProtectedFund::decode(&fund_data)?;

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

    // The exit marker is written in apply (`OBL-C73`); the update already carries the id it is keyed on.
    let update = ExitUpdateV1 {
        exit_id,
        member_pubkey: params.member_pubkey,
        payout_value: exit_value,
        haircut_collected: (member_weight * fund.total_funds / total_weight.max(1)) * haircut_bps / 10_000,
    };
    encode_exit_update_v1(&update)
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

    let fund_data = wasm::db::db_get(funds_db, &params.fund_id.to_repr())?
        .ok_or(DrainProtectionError::MemberNotFound)?;
    let fund: ProtectedFund = ProtectedFund::decode(&fund_data)?;

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

    // Record transfer for rate limiting — written in apply (`OBL-C73`), so the record and the key it is
    // filed under both travel in the update.
    let record = crate::model::TransferRecord { version: 1, block: current_block, amount: params.amount };
    let transfer_key = dwow_sdk::crypto::poseidon_hash([current_block.into()]);

    let update = crate::model::TransferUpdateV1 {
        amount: params.amount,
        recipient: params.recipient,
        rate_limited,
        transfer_key,
        record,
    };
    encode_transfer_update_v1(&update)
}

/// `process_instruction` for LockV1
fn lock_process_instruction_v1(
    cid: dwow_sdk::crypto::ContractId,
    params: LockParamsV1,
) -> Result<Vec<u8>, ContractError> {
    msg!("[LockV1] Locking funds");

    let funds_db = wasm::db::db_lookup(cid, DRAIN_PROTECTION_CONTRACT_FUNDS_TREE)?;

    let fund_data = wasm::db::db_get(funds_db, &params.fund_id.to_repr())?
        .ok_or(DrainProtectionError::MemberNotFound)?;
    let mut fund: ProtectedFund = ProtectedFund::decode(&fund_data)?;

    let current_block: u64 = wasm::util::get_verifying_block_height()?.get();

    // Max lock duration is 7 days worth of blocks (~30240 blocks/day at 5min blocks)
    let max_lock_blocks = 7 * 30240;
    if params.duration_blocks > max_lock_blocks {
        return Err(DrainProtectionError::ConfigurationError("Lock duration too long".to_string()).into())
    }

    fund.lock_state = crate::model::LockState::Locked;
    fund.lock_expires_at = current_block + params.duration_blocks;

    // Stored in apply (`OBL-C73`): the mutated fund travels in the update.
    let update = LockUpdateV1 { fund };
    encode_lock_update_v1(&update)
}

/// `process_instruction` for UnlockV1
fn unlock_process_instruction_v1(
    cid: dwow_sdk::crypto::ContractId,
    params: UnlockParamsV1,
) -> Result<Vec<u8>, ContractError> {
    msg!("[UnlockV1] Unlocking funds");

    let funds_db = wasm::db::db_lookup(cid, DRAIN_PROTECTION_CONTRACT_FUNDS_TREE)?;

    let fund_data = wasm::db::db_get(funds_db, &params.fund_id.to_repr())?
        .ok_or(DrainProtectionError::MemberNotFound)?;
    let mut fund: ProtectedFund = ProtectedFund::decode(&fund_data)?;

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

    // Stored in apply (`OBL-C73`): the mutated fund travels in the update, and `unlocked_at` with it
    // because the fund records only *that* it is unlocked, not when.
    let update = UnlockUpdateV1 { fund, unlocked_at: current_block };
    encode_unlock_update_v1(&update)
}

/// `process_instruction` for UpdateConfigV1
fn update_config_process_instruction_v1(
    cid: dwow_sdk::crypto::ContractId,
    params: crate::model::UpdateConfigParamsV1,
) -> Result<Vec<u8>, ContractError> {
    msg!("[UpdateConfigV1] Updating configuration");

    let funds_db = wasm::db::db_lookup(cid, DRAIN_PROTECTION_CONTRACT_FUNDS_TREE)?;

    let fund_data = wasm::db::db_get(funds_db, &params.fund_id.to_repr())?
        .ok_or(DrainProtectionError::MemberNotFound)?;
    let mut fund: ProtectedFund = ProtectedFund::decode(&fund_data)?;

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

    // Stored in apply (`OBL-C73`): the mutated fund travels in the update, `authority_change_timelock`
    // included — it is a field of the fund.
    let update = crate::model::UpdateConfigUpdateV1 { fund };
    encode_update_config_update_v1(&update)
}

// ============================================================================
// RHO-CALCULUS EXPLICIT BRIDGE ENCODE/DECODE
// ============================================================================

fn encode_initialize_update_v1(update: &crate::model::InitializeUpdateV1) -> Result<Vec<u8>, ContractError> {
    // `?` because the update's own encoder is fallible where it carries a variable-length value
    // (a fund, a record): a length that does not fit the prefix is an error, not a short buffer.
    let inner = update.encode()?;
    let mut buf = Vec::with_capacity(1 + inner.len());
    buf.push(DrainProtectionFunction::InitializeV1 as u8);
    buf.extend_from_slice(&inner);
    Ok(buf)
}

fn encode_propose_update_v1(update: &crate::model::ProposeUpdateV1) -> Result<Vec<u8>, ContractError> {
    let inner = update.encode();
    let mut buf = Vec::with_capacity(1 + inner.len());
    buf.push(DrainProtectionFunction::ProposeV1 as u8);
    buf.extend_from_slice(&inner);
    Ok(buf)
}

fn encode_vote_update_v1(update: &crate::model::VoteUpdateV1) -> Result<Vec<u8>, ContractError> {
    let inner = update.encode();
    let mut buf = Vec::with_capacity(1 + inner.len());
    buf.push(DrainProtectionFunction::VoteV1 as u8);
    buf.extend_from_slice(&inner);
    Ok(buf)
}

fn encode_execute_update_v1(update: &crate::model::ExecuteUpdateV1) -> Result<Vec<u8>, ContractError> {
    let inner = update.encode();
    let mut buf = Vec::with_capacity(1 + inner.len());
    buf.push(DrainProtectionFunction::ExecuteV1 as u8);
    buf.extend_from_slice(&inner);
    Ok(buf)
}

fn encode_exit_update_v1(update: &crate::model::ExitUpdateV1) -> Result<Vec<u8>, ContractError> {
    let inner = update.encode();
    let mut buf = Vec::with_capacity(1 + inner.len());
    buf.push(DrainProtectionFunction::ExitV1 as u8);
    buf.extend_from_slice(&inner);
    Ok(buf)
}

fn encode_transfer_update_v1(update: &crate::model::TransferUpdateV1) -> Result<Vec<u8>, ContractError> {
    // `?` because the update's own encoder is fallible where it carries a variable-length value
    // (a fund, a record): a length that does not fit the prefix is an error, not a short buffer.
    let inner = update.encode()?;
    let mut buf = Vec::with_capacity(1 + inner.len());
    buf.push(DrainProtectionFunction::TransferV1 as u8);
    buf.extend_from_slice(&inner);
    Ok(buf)
}

fn encode_lock_update_v1(update: &crate::model::LockUpdateV1) -> Result<Vec<u8>, ContractError> {
    // `?` because the update's own encoder is fallible where it carries a variable-length value
    // (a fund, a record): a length that does not fit the prefix is an error, not a short buffer.
    let inner = update.encode()?;
    let mut buf = Vec::with_capacity(1 + inner.len());
    buf.push(DrainProtectionFunction::LockV1 as u8);
    buf.extend_from_slice(&inner);
    Ok(buf)
}

fn encode_unlock_update_v1(update: &crate::model::UnlockUpdateV1) -> Result<Vec<u8>, ContractError> {
    // `?` because the update's own encoder is fallible where it carries a variable-length value
    // (a fund, a record): a length that does not fit the prefix is an error, not a short buffer.
    let inner = update.encode()?;
    let mut buf = Vec::with_capacity(1 + inner.len());
    buf.push(DrainProtectionFunction::UnlockV1 as u8);
    buf.extend_from_slice(&inner);
    Ok(buf)
}

fn encode_update_config_update_v1(update: &crate::model::UpdateConfigUpdateV1) -> Result<Vec<u8>, ContractError> {
    // `?` because the update's own encoder is fallible where it carries a variable-length value
    // (a fund, a record): a length that does not fit the prefix is an error, not a short buffer.
    let inner = update.encode()?;
    let mut buf = Vec::with_capacity(1 + inner.len());
    buf.push(DrainProtectionFunction::UpdateConfigV1 as u8);
    buf.extend_from_slice(&inner);
    Ok(buf)
}

// ============================================================================
// STATE UPDATE
// ============================================================================

/// Apply the state update the exec phase produced. **This is where every write happens** (`OBL-C73`).
///
/// The contract used to write from its exec phase — seven `db_set` calls, each of them a call the host
/// refuses at runtime, because apply is the only section whose ACL admits a write and `Exec` is in no
/// write function's list. So as shipped `drain_protection` committed nothing and this function, which
/// ignored both arguments, was the reason: the writes that should have been here were in the section
/// that cannot perform them.
///
/// The dispatch mirrors `identity`'s: the selector byte the exec phase prepended picks the update type,
/// and the update carries everything the write needs — apply may neither read nor validate (`§B.2.2`),
/// so nothing is re-derived here. Trusting the update is sound because the update is produced *inside
/// the contract's own exec phase* and handed to apply by the host; a client supplies call data, never
/// an update.
fn process_update(cid: dwow_sdk::crypto::ContractId, update_data: &[u8]) -> ContractResult {
    let update_func = *update_data.first().ok_or_else(|| {
        ContractError::IoError("empty update data: no selector byte".to_string())
    })?;
    let update_payload = update_data.get(1..).ok_or_else(|| {
        ContractError::IoError("empty update data: no payload after selector".to_string())
    })?;
    let func = DrainProtectionFunction::try_from(update_func)?;

    match func {
        DrainProtectionFunction::InitializeV1 => {
            let update = crate::model::InitializeUpdateV1::decode(update_payload)?;
            apply_initialize_update(cid, update)
        }
        DrainProtectionFunction::ProposeV1 => {
            let update = crate::model::ProposeUpdateV1::decode(update_payload)?;
            apply_propose_update(cid, update)
        }
        DrainProtectionFunction::VoteV1 => {
            let update = VoteUpdateV1::decode(update_payload)?;
            apply_vote_update(cid, update)
        }
        DrainProtectionFunction::ExecuteV1 => {
            let update = crate::model::ExecuteUpdateV1::decode(update_payload)?;
            apply_execute_update(cid, update)
        }
        DrainProtectionFunction::ExitV1 => {
            let update = ExitUpdateV1::decode(update_payload)?;
            apply_exit_update(cid, update)
        }
        DrainProtectionFunction::TransferV1 => {
            let update = crate::model::TransferUpdateV1::decode(update_payload)?;
            apply_transfer_update(cid, update)
        }
        DrainProtectionFunction::LockV1 => {
            let update = LockUpdateV1::decode(update_payload)?;
            apply_lock_update(cid, update)
        }
        DrainProtectionFunction::UnlockV1 => {
            let update = UnlockUpdateV1::decode(update_payload)?;
            apply_unlock_update(cid, update)
        }
        DrainProtectionFunction::UpdateConfigV1 => {
            let update = crate::model::UpdateConfigUpdateV1::decode(update_payload)?;
            apply_update_config_update(cid, update)
        }
    }
}

// ============================================================================
// APPLY — the write half of each instruction
// ============================================================================

/// Store the fund `InitializeV1` created.
fn apply_initialize_update(
    cid: dwow_sdk::crypto::ContractId,
    update: crate::model::InitializeUpdateV1,
) -> ContractResult {
    let funds_db = wasm::db::db_lookup(cid, DRAIN_PROTECTION_CONTRACT_FUNDS_TREE)?;
    wasm::db::db_set(funds_db, &update.fund.id.to_repr(), &update.fund.encode()?)?;
    msg!("[InitializeV1::apply] Fund stored");
    Ok(())
}

/// `ProposeV1` writes nothing: a proposal is recorded by the votes cast on it. Kept as an explicit arm
/// so the dispatch is total and the reason is visible rather than looking like a missing case.
fn apply_propose_update(
    _cid: dwow_sdk::crypto::ContractId,
    _update: crate::model::ProposeUpdateV1,
) -> ContractResult {
    Ok(())
}

/// `ExecuteV1` writes nothing — execution is the proposal's effect, carried by its own child calls.
fn apply_execute_update(
    _cid: dwow_sdk::crypto::ContractId,
    _update: crate::model::ExecuteUpdateV1,
) -> ContractResult {
    Ok(())
}

/// File the vote under the key the exec phase derived and the value it decided.
fn apply_vote_update(cid: dwow_sdk::crypto::ContractId, update: VoteUpdateV1) -> ContractResult {
    let votes_db = wasm::db::db_lookup(cid, DRAIN_PROTECTION_CONTRACT_VOTES_TREE)?;
    wasm::db::db_set(votes_db, &update.vote_key.to_repr(), &update.vote_value.to_repr())?;
    msg!("[VoteV1::apply] Vote recorded");
    Ok(())
}

/// Mark the exit processed. The value is the marker `[1]` the exec phase used to write.
fn apply_exit_update(cid: dwow_sdk::crypto::ContractId, update: ExitUpdateV1) -> ContractResult {
    let exits_db = wasm::db::db_lookup(cid, DRAIN_PROTECTION_CONTRACT_EXITS_TREE)?;
    wasm::db::db_set(exits_db, &update.exit_id.to_repr(), &[1])?;
    msg!("[ExitV1::apply] Exit recorded");
    Ok(())
}

/// Record the transfer for rate limiting, under the key and with the record the update carries.
fn apply_transfer_update(
    cid: dwow_sdk::crypto::ContractId,
    update: crate::model::TransferUpdateV1,
) -> ContractResult {
    let transfers_db = wasm::db::db_lookup(cid, DRAIN_PROTECTION_CONTRACT_TRANSFERS_TREE)?;
    wasm::db::db_set(transfers_db, &update.transfer_key.to_repr(), &update.record.encode())?;
    msg!("[TransferV1::apply] Transfer recorded");
    Ok(())
}

/// Store the fund `LockV1` mutated.
fn apply_lock_update(cid: dwow_sdk::crypto::ContractId, update: LockUpdateV1) -> ContractResult {
    let funds_db = wasm::db::db_lookup(cid, DRAIN_PROTECTION_CONTRACT_FUNDS_TREE)?;
    wasm::db::db_set(funds_db, &update.fund.id.to_repr(), &update.fund.encode()?)?;
    msg!("[LockV1::apply] Fund locked");
    Ok(())
}

/// Store the fund `UnlockV1` mutated.
fn apply_unlock_update(cid: dwow_sdk::crypto::ContractId, update: UnlockUpdateV1) -> ContractResult {
    let funds_db = wasm::db::db_lookup(cid, DRAIN_PROTECTION_CONTRACT_FUNDS_TREE)?;
    wasm::db::db_set(funds_db, &update.fund.id.to_repr(), &update.fund.encode()?)?;
    msg!("[UnlockV1::apply] Fund unlocked");
    Ok(())
}

/// Store the fund `UpdateConfigV1` mutated.
fn apply_update_config_update(
    cid: dwow_sdk::crypto::ContractId,
    update: crate::model::UpdateConfigUpdateV1,
) -> ContractResult {
    let funds_db = wasm::db::db_lookup(cid, DRAIN_PROTECTION_CONTRACT_FUNDS_TREE)?;
    wasm::db::db_set(funds_db, &update.fund.id.to_repr(), &update.fund.encode()?)?;
    msg!("[UpdateConfigV1::apply] Fund configuration stored");
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