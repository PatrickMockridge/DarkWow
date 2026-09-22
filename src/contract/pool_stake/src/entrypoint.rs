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

//! Pool Stake Contract Entrypoint

use dwow_sdk::{
    crypto::{pasta_prelude::PrimeField, poseidon_hash, ContractId, PURSE_CONTRACT_ID},
    dark_tree::DarkLeaf,
    error::{ContractError, ContractResult},
    msg, ContractCall,
    pasta::pallas,
    wasm,
};
use dwow_promissory_note_contract::validation::{
    validate_child_contract_id,
    validate_child_value_commit,
};
use dwow_serial::{deserialize, Encodable};

use crate::error::PoolStakeError;
use crate::model::*;
use crate::PoolStakeFunction;
use crate::{
    POOL_STAKE_ALLOCATIONS_TREE, POOL_STAKE_MEMBERS_TREE, POOL_STAKE_REGISTRY_TREE,
    POOL_STAKE_MIN_STAKE, POOL_STAKE_INFO_TREE, POOL_STAKE_PROMISSORY_NOTE_CONTRACT_ID,
    POOL_STAKE_PURSE_CONTRACT_ID,
    POOL_STAKE_ZKAS_CREATE_POOL_NS_V2, POOL_STAKE_ZKAS_JOIN_POOL_NS_V2,
    POOL_STAKE_ZKAS_ALLOCATE_COVERAGE_NS_V2, POOL_STAKE_ZKAS_SLASH_COVERAGE_NS_V2,
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

fn init_contract(cid: ContractId, _ix: &[u8]) -> ContractResult {
    // Initialize INFO_TREE with redeployment guard
    let info_db = match wasm::db::db_lookup(cid, POOL_STAKE_INFO_TREE) {
        Ok(v) => v,
        Err(_) => wasm::db::db_init(cid, POOL_STAKE_INFO_TREE)?,
    };
    wasm::db::db_set(info_db, POOL_STAKE_PROMISSORY_NOTE_CONTRACT_ID, &dwow_sdk::crypto::PROMISSORY_NOTE_CONTRACT_ID.to_bytes())?;
    wasm::db::db_set(info_db, POOL_STAKE_PURSE_CONTRACT_ID, &PURSE_CONTRACT_ID.to_bytes())?;

    // Initialize database trees with redeployment guards
    if wasm::db::db_lookup(cid, POOL_STAKE_REGISTRY_TREE).is_err() {
        wasm::db::db_init(cid, POOL_STAKE_REGISTRY_TREE)?;
    }
    if wasm::db::db_lookup(cid, POOL_STAKE_MEMBERS_TREE).is_err() {
        wasm::db::db_init(cid, POOL_STAKE_MEMBERS_TREE)?;
    }
    if wasm::db::db_lookup(cid, POOL_STAKE_ALLOCATIONS_TREE).is_err() {
        wasm::db::db_init(cid, POOL_STAKE_ALLOCATIONS_TREE)?;
    }

    // Register the V2 circuits (domain separation, HAZOP RC3).
    //
    // These four `include_bytes!` used to bind to `_`-prefixed variables and stop there: the
    // bytes were read into the artifact and never handed to `zkas_db_set`, so no circuit was
    // registered at all. The namespaces `get_metadata` pushes are the V2 ones (`CreatePoolV2`,
    // `JoinPoolV2`, `AllocateCoverageV2`, `SlashCoverageV2`) and each `.zk` carries that identity
    // string, so registering them is all that was missing — no rename, no metadata change.
    let allocate_coverage_v2_bincode = include_bytes!("../proof/allocate_coverage.zk.bin");
    wasm::db::zkas_db_set(&allocate_coverage_v2_bincode[..])?;
    let create_pool_v2_bincode = include_bytes!("../proof/create_pool.zk.bin");
    wasm::db::zkas_db_set(&create_pool_v2_bincode[..])?;
    let join_pool_v2_bincode = include_bytes!("../proof/join_pool.zk.bin");
    wasm::db::zkas_db_set(&join_pool_v2_bincode[..])?;
    let slash_coverage_v2_bincode = include_bytes!("../proof/slash_coverage.zk.bin");
    wasm::db::zkas_db_set(&slash_coverage_v2_bincode[..])?;

    Ok(())
}

fn get_metadata(_cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = PoolStakeFunction::try_from(self_.data[0])?;

    let metadata = match func {
        PoolStakeFunction::CreatePoolV1 => {
            let params= CreatePoolParamsV1::decode(&self_.data[1..])?;
            create_pool_get_metadata_v1(params)?
        }
        PoolStakeFunction::JoinPoolV1 => {
            let params= JoinPoolParamsV1::decode(&self_.data[1..])?;
            join_pool_get_metadata_v1(params)?
        }
        PoolStakeFunction::AllocateCoverageV1 => {
            let params= AllocateCoverageParamsV1::decode(&self_.data[1..])?;
            allocate_coverage_get_metadata_v1(params)?
        }
        PoolStakeFunction::SlashCoverageV1 => {
            let params= SlashCoverageParamsV1::decode(&self_.data[1..])?;
            slash_coverage_get_metadata_v1(params)?
        }
        // Functions without ZK proofs: an **encoded** empty `zk_public_inputs`, not a bare `vec![]`.
        // The host decodes the metadata as `Vec<(String, Vec<Base>)>` (`execution.rs:423`) and a
        // 0-byte buffer fails that decode, which it reports as "contract signalled EMPTY metadata,
        // the documented rejection signal" — so a raw empty Vec made every non-ZK endpoint
        // uncallable. This is exactly what the ZK helpers below return for an empty vector.
        PoolStakeFunction::LeavePoolV1
        | PoolStakeFunction::ReleaseCoverageV1
        | PoolStakeFunction::ClaimFeesV1
        | PoolStakeFunction::UpdatePoolConfigV1
        | PoolStakeFunction::RebalancePoolSharesV1 => {
            let zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
            let mut metadata = vec![];
            zk_public_inputs.encode(&mut metadata)?;
            metadata
        }
    };

    wasm::util::set_return_data(&metadata)
}

fn create_pool_get_metadata_v1(
    params: CreatePoolParamsV1,
) -> Result<Vec<u8>, dwow_sdk::error::ContractError> {
    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    // Circuit order: tx_binding, tx_nonce, derived_pool_id.
    //
    // `tx_binding` was a bare `Base::zero()` here, while `create_pool.zk` constrains its witness to
    // equal `poseidon_hash(3, tx_commitment, tx_nonce)`. Both cannot hold, so the proof was
    // unsatisfiable — the CreatePoolV2 rejection this contract's suite died on.
    //
    // The value below is the same one the client's `CreatePoolV1CallData::compute_tx_binding`
    // derives from its `tx_commitment`/`tx_nonce`, which default to zero — so this is a *constant
    // binding*, consistent on both sides but carrying no transaction identity. That state is
    // deliberate and recorded in `doc/src/arch/verification-hazop.md`: nothing in this repository
    // validates `tx_nonce`, and the client builds the proof before the transaction exists, so the
    // binding cannot reference it. Do not read this as a derivation.
    let tx_binding =
        poseidon_hash([pallas::Base::from(3u64), pallas::Base::zero(), pallas::Base::zero()]);
    zk_public_inputs.push((
        POOL_STAKE_ZKAS_CREATE_POOL_NS_V2.to_string(),
        vec![tx_binding, pallas::Base::zero(), params.derived_pool_id],
    ));
    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    Ok(metadata)
}

fn join_pool_get_metadata_v1(
    params: JoinPoolParamsV1,
) -> Result<Vec<u8>, dwow_sdk::error::ContractError> {
    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    // Circuit order: derived_member_id, vc_x, tx_binding, tx_nonce, vc_y — note `value_commit_y`
    // is last, after the tx pair, not beside its x.
    //
    // The tx pair was `[zero, zero]` against a circuit that constrains `tx_binding` to
    // `poseidon_hash(3, tx_commitment, tx_nonce)`; see `create_pool_get_metadata_v1` for the full
    // note and for why this value is a constant rather than a transaction reference.
    zk_public_inputs.push((
        POOL_STAKE_ZKAS_JOIN_POOL_NS_V2.to_string(),
        vec![
            params.derived_member_id,
            params.value_commit_x,
            poseidon_hash([pallas::Base::from(3u64), pallas::Base::zero(), pallas::Base::zero()]),
            pallas::Base::zero(),
            params.value_commit_y,
        ],
    ));
    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    Ok(metadata)
}

fn allocate_coverage_get_metadata_v1(
    params: AllocateCoverageParamsV1,
) -> Result<Vec<u8>, dwow_sdk::error::ContractError> {
    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    // Circuit order: tx_binding(0), tx_nonce(1), derived_allocation_id(2)
    zk_public_inputs.push((
        POOL_STAKE_ZKAS_ALLOCATE_COVERAGE_NS_V2.to_string(),
        // Circuit order: tx_binding, tx_nonce, derived_allocation_id. The binding was a bare zero
        // against a circuit that constrains it to `poseidon_hash(3, tx_commitment, tx_nonce)`; see
        // `create_pool_get_metadata_v1` for the full note and for what this constant does and does
        // not mean.
        vec![
            poseidon_hash([pallas::Base::from(3u64), pallas::Base::zero(), pallas::Base::zero()]),
            pallas::Base::zero(),
            params.derived_allocation_id,
        ],
    ));
    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    Ok(metadata)
}

fn slash_coverage_get_metadata_v1(
    params: SlashCoverageParamsV1,
) -> Result<Vec<u8>, dwow_sdk::error::ContractError> {
    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    // Circuit order: tx_binding(0), tx_nonce(1), derived_slash_id(2)
    zk_public_inputs.push((
        POOL_STAKE_ZKAS_SLASH_COVERAGE_NS_V2.to_string(),
        // Circuit order: tx_binding, tx_nonce, derived_slash_id — same correction as
        // `create_pool_get_metadata_v1`.
        vec![
            poseidon_hash([pallas::Base::from(3u64), pallas::Base::zero(), pallas::Base::zero()]),
            pallas::Base::zero(),
            params.derived_slash_id,
        ],
    ));
    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    Ok(metadata)
}

// ============================================================================
// INSTRUCTION PROCESSING
// ============================================================================

fn process_instruction(cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = PoolStakeFunction::try_from(self_.data[0])?;

    match func {
        PoolStakeFunction::CreatePoolV1 => {
            process_create_pool_instruction(cid, call_idx, calls)
        }
        PoolStakeFunction::JoinPoolV1 => process_join_pool_instruction(cid, call_idx, calls),
        PoolStakeFunction::LeavePoolV1 => process_leave_pool_instruction(cid, call_idx, calls),
        PoolStakeFunction::AllocateCoverageV1 => {
            process_allocate_coverage_instruction(cid, call_idx, calls)
        }
        PoolStakeFunction::ReleaseCoverageV1 => {
            process_release_coverage_instruction(cid, call_idx, calls)
        }
        PoolStakeFunction::SlashCoverageV1 => {
            process_slash_coverage_instruction(cid, call_idx, calls)
        }
        PoolStakeFunction::ClaimFeesV1 => process_claim_fees_instruction(cid, call_idx, calls),
        PoolStakeFunction::UpdatePoolConfigV1 => {
            process_update_pool_config_instruction(cid, call_idx, calls)
        }
        PoolStakeFunction::RebalancePoolSharesV1 => {
            process_rebalance_pool_shares_instruction(cid, call_idx, calls)
        }
    }
}

fn process_update(cid: ContractId, update_data: &[u8]) -> ContractResult {
    let func = PoolStakeFunction::try_from(update_data[0])?;

    match func {
        PoolStakeFunction::CreatePoolV1 => {
            let update = CreatePoolUpdateV1::decode(&update_data[1..])?;
            apply_create_pool_update(cid, update)
        }
        PoolStakeFunction::JoinPoolV1 => {
            let update = JoinPoolUpdateV1::decode(&update_data[1..])?;
            apply_join_pool_update(cid, update)
        }
        PoolStakeFunction::LeavePoolV1 => {
            let update = LeavePoolUpdateV1::decode(&update_data[1..])?;
            apply_leave_pool_update(cid, update)
        }
        PoolStakeFunction::AllocateCoverageV1 => {
            let update = AllocateCoverageUpdateV1::decode(&update_data[1..])?;
            apply_allocate_coverage_update(cid, update)
        }
        PoolStakeFunction::ReleaseCoverageV1 => {
            let update = ReleaseCoverageUpdateV1::decode(&update_data[1..])?;
            apply_release_coverage_update(cid, update)
        }
        PoolStakeFunction::SlashCoverageV1 => {
            let update = SlashCoverageUpdateV1::decode(&update_data[1..])?;
            apply_slash_coverage_update(cid, update)
        }
        PoolStakeFunction::ClaimFeesV1 => {
            let update = ClaimFeesUpdateV1::decode(&update_data[1..])?;
            apply_claim_fees_update(cid, update)
        }
        PoolStakeFunction::UpdatePoolConfigV1 => {
            let update = UpdatePoolConfigUpdateV1::decode(&update_data[1..])?;
            apply_update_pool_config_update(cid, update)
        }
        PoolStakeFunction::RebalancePoolSharesV1 => {
            let update = RebalancePoolSharesUpdateV1::decode(&update_data[1..])?;
            apply_rebalance_pool_shares_update(cid, update)
        }
    }
}

// ============================================================================
// CREATE POOL
// ============================================================================

fn process_create_pool_instruction(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params= CreatePoolParamsV1::decode(&self_.data[1..])?;

    msg!("[pool_stake::create_pool] Creating new pool");

    // Validate coverage ratio
    if params.max_coverage_ratio == 0 || params.max_coverage_ratio > 10000 {
        return Err(PoolStakeError::InvalidCoverageRatio.into());
    }

    // The pool's identity is the value its own proof binds.
    //
    // `derived_pool_id` is computed in-circuit from the creator's public key, the config hash and
    // the nonce (`create_pool.zk`), exposed as the third public instance, and published by
    // `create_pool_get_metadata_v1` — so it is the one identifier here that the proof actually
    // attests to. It used to be ignored in favour of `derive_pool_id(verifying_block_height)`,
    // which made three things true at once: the proof-bound id was decorative, no client could
    // predict the id of the pool it had just created (the height is not known when the proof is
    // built), and two pools created in the same block could not coexist — the second would hit the
    // duplicate guard below and be rejected as `PoolNotFound`.
    let pool_id = params.derived_pool_id;

    // Check pool doesn't already exist

    let registry_db = wasm::db::db_lookup(cid, POOL_STAKE_REGISTRY_TREE)?;
    if wasm::db::db_contains_key(registry_db, &pool_id.to_repr())? {
        return Err(PoolStakeError::PoolNotFound.into());
    }

    let update = CreatePoolUpdateV1 {
        instance_seed: params.instance_seed,
        pool_id,
        owner_pub: params.owner_pub,
        max_coverage_ratio: params.max_coverage_ratio,
        operator_fee_bp: params.operator_fee_bp,
        created_at: wasm::util::get_verifying_block_height()?.get(),
    };

    msg!("[pool_stake::create_pool] Pool {:?} created", pool_id);
    wasm::util::set_return_data(&[&[PoolStakeFunction::CreatePoolV1 as u8], &update.encode()[..]].concat())
}

fn apply_create_pool_update(cid: ContractId, update: CreatePoolUpdateV1) -> ContractResult {
    let registry_db = wasm::db::db_lookup(cid, POOL_STAKE_REGISTRY_TREE)?;

    let registry = PoolStakeRegistry {
        version: 1,
        pool_id: update.pool_id,
        owner_pub: update.owner_pub,
        total_stake: 0,
        available_coverage: 0,
        allocated_coverage: 0,
        member_count: 0,
        max_coverage_ratio: update.max_coverage_ratio,
        operator_fee_bp: update.operator_fee_bp,
        created_at: update.created_at,
        total_slashed: 0,
        pool_slash_count: 0,
        is_active: true,
    };

    wasm::db::db_set(registry_db, &update.pool_id.to_repr(), &registry.encode())?;
    msg!("[pool_stake::create_pool::update] Pool registry stored");

    Ok(())
}

// ============================================================================
// JOIN POOL
// ============================================================================

fn process_join_pool_instruction(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params= JoinPoolParamsV1::decode(&self_.data[1..])?;

    msg!("[pool_stake::join_pool] Joining pool {:?} with amount {}", params.pool_id, params.amount);

    // Validate promissory_note::transfer_v1 child call for stake deposit
    let this_call = &calls[call_idx];
    if this_call.children_indexes.len() != 1 {
        msg!("[JoinPoolV1] Expected 1 child call (promissory_note::transfer_v1)");
        return Err(PoolStakeError::InvalidChildrenIndexes.into())
    }
    let child_idx = this_call.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!("[JoinPoolV1] Child call is not promissory_note::transfer_v1 (0x04)");
        return Err(PoolStakeError::InvalidChildCall.into())
    }

    // Validate child call targets promissory_note (prevent cross-contract routing)
    let info_db = wasm::db::db_lookup(cid, POOL_STAKE_INFO_TREE)?;
    let promissory_note_bytes = wasm::db::db_get(info_db, POOL_STAKE_PROMISSORY_NOTE_CONTRACT_ID)?
        .ok_or(PoolStakeError::InvalidChildCall)?;
    let promissory_note_cid: ContractId = deserialize(&promissory_note_bytes)?;
    // HAZOP H-11: fail-closed — reject if promissory_note not configured
    if promissory_note_cid == ContractId::ZERO {
        return Err(ContractError::IoError("promissory_note contract ID not configured".into()));
    }
    validate_child_contract_id(&child_call.contract_id, &promissory_note_cid)?;

    let value_blind = poseidon_hash([
        pallas::Base::from(params.amount),
        params.pool_id,
    ]);
    validate_child_value_commit(&child_call.data, params.amount, value_blind)?;

    // Validate stake amount
    if params.amount < POOL_STAKE_MIN_STAKE {
        return Err(PoolStakeError::InsufficientStake(POOL_STAKE_MIN_STAKE).into());
    }

    // Get registry
    let registry_db = wasm::db::db_lookup(cid, POOL_STAKE_REGISTRY_TREE)?;
    let mut pool: PoolStakeRegistry =
        match wasm::db::db_get(registry_db, &params.pool_id.to_repr())? {
            Some(data) => PoolStakeRegistry::decode(&data)?,
            None => return Err(PoolStakeError::PoolNotFound.into()),
        };

    // The stake id is the **proof-bound** `derived_member_id` the circuit computes and get_metadata
    // publishes (entrypoint.rs:175). This used to be `derive_stake_id(pool_id, relayer_id, height)`,
    // which (a) disagreed with the published public input, so the proof bound an id nothing used,
    // and (b) depended on the block height, which the client cannot know when it builds the proof —
    // so no client could address the stake it had just created. Same defect as CreatePoolV1's
    // ignored `derived_pool_id`; `derive_stake_id` is now dead and removed.
    let current_block = wasm::util::get_verifying_block_height()?.get();
    let stake_id = params.derived_member_id;

    // Check stake doesn't already exist
    let stakes_db = wasm::db::db_lookup(cid, POOL_STAKE_MEMBERS_TREE)?;
    if wasm::db::db_contains_key(stakes_db, &stake_id.to_repr())? {
        return Err(PoolStakeError::AlreadyMember.into());
    }

    // Calculate coverage contribution: amount * coverage_ratio / 10000
    let coverage_contribution =
        (params.amount as u64 * pool.max_coverage_ratio as u64) / 10000_u64;

    // Calculate pool share in basis points
    let new_total = pool.total_stake + params.amount;
    let pool_share_bp = if new_total == 0 {
        0
    } else {
        ((params.amount as u128 * 10000) / new_total as u128) as u32
    };

    // Update pool
    pool.total_stake += params.amount;
    pool.available_coverage += coverage_contribution;
    pool.member_count += 1;

    let update = JoinPoolUpdateV1 {
        instance_seed: params.instance_seed,
        stake_id,
        member_pub: params.member_pub,
        relayer_id: params.relayer_id,
        amount: params.amount,
        coverage_contribution,
        pool_share_bp,
        created_at: current_block,
        pool,
    };

    msg!("[pool_stake::join_pool] Stake {:?} created", stake_id);
    wasm::util::set_return_data(&[&[PoolStakeFunction::JoinPoolV1 as u8], &update.encode()[..]].concat())
}

fn apply_join_pool_update(cid: ContractId, update: JoinPoolUpdateV1) -> ContractResult {
    let registry_db = wasm::db::db_lookup(cid, POOL_STAKE_REGISTRY_TREE)?;
    let stakes_db = wasm::db::db_lookup(cid, POOL_STAKE_MEMBERS_TREE)?;

    // Re-store the registry exactly as exec left it. Blind write: the read triad is denied in
    // `Update` (contract-wasm-type-system.md §B.2.2, register OBL-C72), so apply must not read
    // the record back. Note this also fixes a defect the old read-modify-write hid: exec has
    // always computed `available_coverage += coverage_contribution`, but the update carried only
    // `total_stake` and `member_count`, so the pool's available coverage was never written.
    wasm::db::db_set(registry_db, &update.pool.pool_id.to_repr(), &update.pool.encode())?;

    // Create stake
    let stake = PoolMemberStake {
        version: 1,
        instance_seed: update.instance_seed,
        stake_id: update.stake_id,
        pool_id: update.pool.pool_id,
        member_pub: update.member_pub,
        relayer_id: update.relayer_id,
        original_amount: update.amount,
        current_amount: update.amount,
        coverage_contribution: update.coverage_contribution,
        pool_share_bp: update.pool_share_bp,
        accumulated_fees: 0,
        created_at: update.created_at,
        leave_requested_at: None,
        slash_count: 0,
        is_active: true,
    };

    wasm::db::db_set(stakes_db, &update.stake_id.to_repr(), &stake.encode())?;
    msg!("[pool_stake::join_pool::update] Stake stored");

    Ok(())
}

// ============================================================================
// LEAVE POOL
// ============================================================================

fn process_leave_pool_instruction(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params= LeavePoolParamsV1::decode(&self_.data[1..])?;

    msg!("[pool_stake::leave_pool] Leave request for stake {:?}", params.stake_id);

    // Validate promissory_note::transfer_v1 child call for stake withdrawal
    let this_call = &calls[call_idx];
    if this_call.children_indexes.len() != 1 {
        msg!("[LeavePoolV1] Expected 1 child call (promissory_note::transfer_v1)");
        return Err(PoolStakeError::InvalidChildrenIndexes.into())
    }
    let child_idx = this_call.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!("[LeavePoolV1] Child call is not promissory_note::transfer_v1 (0x04)");
        return Err(PoolStakeError::InvalidChildCall.into())
    }

    // Validate child call targets promissory_note (prevent cross-contract routing)
    let info_db = wasm::db::db_lookup(cid, POOL_STAKE_INFO_TREE)?;
    let promissory_note_bytes = wasm::db::db_get(info_db, POOL_STAKE_PROMISSORY_NOTE_CONTRACT_ID)?
        .ok_or(PoolStakeError::InvalidChildCall)?;
    let promissory_note_cid: ContractId = deserialize(&promissory_note_bytes)?;
    // HAZOP H-11: fail-closed — reject if promissory_note not configured
    if promissory_note_cid == ContractId::ZERO {
        return Err(ContractError::IoError("promissory_note contract ID not configured".into()));
    }
    validate_child_contract_id(&child_call.contract_id, &promissory_note_cid)?;

    // Get stake
    let stakes_db = wasm::db::db_lookup(cid, POOL_STAKE_MEMBERS_TREE)?;
    let mut stake: PoolMemberStake =
        match wasm::db::db_get(stakes_db, &params.stake_id.to_repr())? {
            Some(data) => PoolMemberStake::decode(&data)?,
            None => return Err(PoolStakeError::StakeNotFound.into()),
        };

    if !stake.is_active {
        return Err(PoolStakeError::StakeLocked.into());
    }

    // Enforce cooldown period before leaving
    let current_block = wasm::util::get_verifying_block_height()?.get();
    if let Some(requested_at) = stake.leave_requested_at {
        // Cooldown started — verify it's elapsed
        if current_block < requested_at + crate::POOL_STAKE_LEAVE_COOLDOWN_BLOCKS {
            let remaining = (requested_at + crate::POOL_STAKE_LEAVE_COOLDOWN_BLOCKS).saturating_sub(current_block);
            msg!("[pool_stake::leave_pool] Cooldown active: {} blocks remaining", remaining);
            return Err(PoolStakeError::StakeLocked.into())
        }
        // Cooldown elapsed — proceed with leave
    } else {
        // First call — start the cooldown. Starting it is a state write, and exec does not write
        // (OBL-C73): the updated stake travels in the update and apply stores it. The call
        // *succeeds*, so the caller learns the cooldown began from a successful call.
        stake.leave_requested_at = Some(current_block);
        let update = LeavePoolUpdateV1 { stake, payout_amount: 0, unstake_penalty: 0 };
        msg!("[pool_stake::leave_pool] Cooldown started: {} blocks", crate::POOL_STAKE_LEAVE_COOLDOWN_BLOCKS);
        return wasm::util::set_return_data(&[&[PoolStakeFunction::LeavePoolV1 as u8], &update.encode()[..]].concat())
    }

    // Calculate final payout (current_amount - proportional losses)
    let registry_db = wasm::db::db_lookup(cid, POOL_STAKE_REGISTRY_TREE)?;
    let _pool: PoolStakeRegistry =
        match wasm::db::db_get(registry_db, &stake.pool_id.to_repr())? {
            Some(data) => PoolStakeRegistry::decode(&data)?,
            None => return Err(PoolStakeError::PoolNotFound.into()),
        };

    // Calculate payout (simplified - no losses in this basic version)
    let payout_amount = stake.current_amount;
    let unstake_penalty = 0;

    let value_blind = poseidon_hash([
        pallas::Base::from(payout_amount),
        stake.pool_id,
    ]);
    validate_child_value_commit(&child_call.data, payout_amount, value_blind)?;

    stake.is_active = false;
    stake.current_amount = 0;

    let update = LeavePoolUpdateV1 { stake, payout_amount, unstake_penalty };

    msg!("[pool_stake::leave_pool] Payout: {}", payout_amount);
    wasm::util::set_return_data(&[&[PoolStakeFunction::LeavePoolV1 as u8], &update.encode()[..]].concat())
}

fn apply_leave_pool_update(cid: ContractId, update: LeavePoolUpdateV1) -> ContractResult {
    let stakes_db = wasm::db::db_lookup(cid, POOL_STAKE_MEMBERS_TREE)?;

    // Re-store the stake exactly as exec left it — blind write, no read (OBL-C72). One path
    // covers both transitions: the cooldown start and the leave itself.
    wasm::db::db_set(stakes_db, &update.stake.stake_id.to_repr(), &update.stake.encode())?;
    msg!("[pool_stake::leave_pool::update] Stake updated");

    Ok(())
}

// ============================================================================
// ALLOCATE COVERAGE
// ============================================================================

fn process_allocate_coverage_instruction(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params= AllocateCoverageParamsV1::decode(&self_.data[1..])?;

    msg!(
        "[pool_stake::allocate_coverage] Allocating {} for withdrawal {:?}",
        params.amount,
        params.withdrawal_nullifier
    );

    // Get pool
    let registry_db = wasm::db::db_lookup(cid, POOL_STAKE_REGISTRY_TREE)?;
    let mut pool: PoolStakeRegistry =
        match wasm::db::db_get(registry_db, &params.pool_id.to_repr())? {
            Some(data) => PoolStakeRegistry::decode(&data)?,
            None => return Err(PoolStakeError::PoolNotFound.into()),
        };

    // Check available coverage
    if pool.available_coverage < params.amount {
        return Err(PoolStakeError::InsufficientCoverage.into());
    }

    // The proof-bound `derived_allocation_id` the circuit computes and get_metadata publishes
    // (entrypoint.rs:201), for the same reason as the stake id above — the old
    // `derive_allocation_id(.., height)` disagreed with the public input and was unknowable to the
    // client. `derive_allocation_id` is now dead and removed.
    let current_block = wasm::util::get_verifying_block_height()?.get();
    let allocation_id = params.derived_allocation_id;

    // NOTE: contributing_members requires iteration over POOL_STAKE_MEMBERS_TREE.
    // The wasm::db API currently lacks iteration support. Deferred to DB API upgrade
    // (per-pool member index or wasm::db iteration). Proportional payout is skipped.
    let contributing_members = vec![];

    // Move the coverage from available to allocated here, on the record exec carries through —
    // apply re-stores it rather than reading it back (OBL-C72).
    pool.available_coverage -= params.amount;
    pool.allocated_coverage += params.amount;

    let update = AllocateCoverageUpdateV1 {
        allocation_id,
        withdrawal_nullifier: params.withdrawal_nullifier,
        amount: params.amount,
        contributing_members,
        timeout_height: params.timeout_height,
        created_at: current_block,
        pool,
    };

    msg!("[pool_stake::allocate_coverage] Allocation {:?} created", allocation_id);
    wasm::util::set_return_data(&[&[PoolStakeFunction::AllocateCoverageV1 as u8], &update.encode()?[..]].concat())
}

fn apply_allocate_coverage_update(cid: ContractId, update: AllocateCoverageUpdateV1) -> ContractResult {
    let registry_db = wasm::db::db_lookup(cid, POOL_STAKE_REGISTRY_TREE)?;
    let allocations_db = wasm::db::db_lookup(cid, POOL_STAKE_ALLOCATIONS_TREE)?;

    // Re-store the registry exactly as exec left it — blind write, no read (OBL-C72).
    wasm::db::db_set(registry_db, &update.pool.pool_id.to_repr(), &update.pool.encode())?;

    // Create allocation
    let allocation = CoverageAllocation {
        version: 1,
        allocation_id: update.allocation_id,
        pool_id: update.pool.pool_id,
        withdrawal_nullifier: update.withdrawal_nullifier,
        amount: update.amount,
        contributing_members: update.contributing_members,
        created_at: update.created_at,
        timeout_height: update.timeout_height,
        executed: false,
        slashed: false,
    };

    wasm::db::db_set(
        allocations_db,
        &update.allocation_id.to_repr(),
        &allocation.encode()?,
    )?;
    msg!("[pool_stake::allocate_coverage::update] Allocation stored");

    Ok(())
}

// ============================================================================
// RELEASE COVERAGE
// ============================================================================

fn process_release_coverage_instruction(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params= ReleaseCoverageParamsV1::decode(&self_.data[1..])?;

    msg!("[pool_stake::release_coverage] Releasing allocation {:?}", params.allocation_id);

    let allocations_db = wasm::db::db_lookup(cid, POOL_STAKE_ALLOCATIONS_TREE)?;
    let mut allocation: CoverageAllocation =
        match wasm::db::db_get(allocations_db, &params.allocation_id.to_repr())? {
            Some(data) => CoverageAllocation::decode(&data)?,
            None => return Err(PoolStakeError::AllocationNotFound.into()),
        };

    if allocation.executed || allocation.slashed {
        return Err(PoolStakeError::AllocationNotFound.into());
    }

    // Get pool to calculate new coverage
    let registry_db = wasm::db::db_lookup(cid, POOL_STAKE_REGISTRY_TREE)?;
    let mut pool: PoolStakeRegistry =
        match wasm::db::db_get(registry_db, &allocation.pool_id.to_repr())? {
            Some(data) => PoolStakeRegistry::decode(&data)?,
            None => return Err(PoolStakeError::PoolNotFound.into()),
        };

    if params.owner_pub != pool.owner_pub {
        return Err(PoolStakeError::Unauthorized.into())
    }

    // Apply both record changes here, on the values exec carries through: apply re-stores them
    // rather than reading them back (OBL-C72). The allocation is encoded here because it has a
    // variable-length member list — apply stores the bytes rather than re-encoding the record.
    let released_amount = allocation.amount;
    pool.available_coverage += released_amount;
    pool.allocated_coverage -= released_amount;
    allocation.executed = true;
    let allocation_bytes = allocation.encode()?;

    let update = ReleaseCoverageUpdateV1 {
        allocation_id: params.allocation_id,
        released_amount,
        allocation_bytes,
        pool,
    };

    wasm::util::set_return_data(&[&[PoolStakeFunction::ReleaseCoverageV1 as u8], &update.encode()?[..]].concat())
}

fn apply_release_coverage_update(cid: ContractId, update: ReleaseCoverageUpdateV1) -> ContractResult {
    let registry_db = wasm::db::db_lookup(cid, POOL_STAKE_REGISTRY_TREE)?;
    let allocations_db = wasm::db::db_lookup(cid, POOL_STAKE_ALLOCATIONS_TREE)?;

    // Both are blind writes — no read, no decode (OBL-C72).
    wasm::db::db_set(registry_db, &update.pool.pool_id.to_repr(), &update.pool.encode())?;
    wasm::db::db_set(
        allocations_db,
        &update.allocation_id.to_repr(),
        &update.allocation_bytes,
    )?;
    msg!("[pool_stake::release_coverage::update] Coverage released");

    Ok(())
}

// ============================================================================
// SLASH COVERAGE
// ============================================================================

fn process_slash_coverage_instruction(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params= SlashCoverageParamsV1::decode(&self_.data[1..])?;

    msg!(
        "[pool_stake::slash_coverage] Slashing {} from allocation {:?}",
        params.slash_amount,
        params.allocation_id
    );

    let allocations_db = wasm::db::db_lookup(cid, POOL_STAKE_ALLOCATIONS_TREE)?;
    let mut allocation: CoverageAllocation =
        match wasm::db::db_get(allocations_db, &params.allocation_id.to_repr())? {
            Some(data) => CoverageAllocation::decode(&data)?,
            None => return Err(PoolStakeError::AllocationNotFound.into()),
        };

    if allocation.slashed {
        return Err(PoolStakeError::AllocationNotFound.into());
    }

    // Get pool
    let registry_db = wasm::db::db_lookup(cid, POOL_STAKE_REGISTRY_TREE)?;
    let mut pool: PoolStakeRegistry =
        match wasm::db::db_get(registry_db, &allocation.pool_id.to_repr())? {
            Some(data) => PoolStakeRegistry::decode(&data)?,
            None => return Err(PoolStakeError::PoolNotFound.into()),
        };

    if params.owner_pub != pool.owner_pub {
        return Err(PoolStakeError::Unauthorized.into())
    }

    if pool.allocated_coverage < params.slash_amount {
        return Err(PoolStakeError::InsufficientCoverage.into())
    }

    // Apply all three record changes here, on the values exec carries through: the registry, the
    // allocation, and each contributing member's slash count. apply re-stores them without reading
    // (OBL-C72) — the previous apply read all three back. The allocation is encoded here because
    // its member list is variable-length.
    pool.total_slashed = pool.total_slashed.saturating_add(params.slash_amount);
    pool.pool_slash_count = pool.pool_slash_count.saturating_add(1);
    pool.allocated_coverage -= params.slash_amount;
    allocation.slashed = true;
    let allocation_bytes = allocation.encode()?;

    // Track per-member slash counts (Phase 2d hardening)
    let members_db = wasm::db::db_lookup(cid, POOL_STAKE_MEMBERS_TREE)?;
    let mut member_stakes = Vec::with_capacity(allocation.contributing_members.len());
    for member_id in &allocation.contributing_members {
        if let Some(data) = wasm::db::db_get(members_db, &member_id.to_repr())? {
            let mut stake: PoolMemberStake = PoolMemberStake::decode(&data)?;
            stake.slash_count = stake.slash_count.saturating_add(1);
            member_stakes.push(stake);
        }
    }

    #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy()/x()/y() is always Some")]
    let update = SlashCoverageUpdateV1 {
        allocation_id: params.allocation_id,
        slashed_amount: params.slash_amount,
        compensated_user: params.user_pub.x().expect("pk not identity").to_repr(),
        allocation_bytes,
        member_stakes,
        pool,
    };

    wasm::util::set_return_data(&[&[PoolStakeFunction::SlashCoverageV1 as u8], &update.encode()?[..]].concat())
}

fn apply_slash_coverage_update(cid: ContractId, update: SlashCoverageUpdateV1) -> ContractResult {
    let allocations_db = wasm::db::db_lookup(cid, POOL_STAKE_ALLOCATIONS_TREE)?;
    let members_db = wasm::db::db_lookup(cid, POOL_STAKE_MEMBERS_TREE)?;
    let registry_db = wasm::db::db_lookup(cid, POOL_STAKE_REGISTRY_TREE)?;

    // All blind writes — no reads (OBL-C72).
    wasm::db::db_set(
        allocations_db,
        &update.allocation_id.to_repr(),
        &update.allocation_bytes,
    )?;

    // Track per-member slash counts (Phase 2d hardening)
    for stake in &update.member_stakes {
        wasm::db::db_set(
            members_db,
            &stake.stake_id.to_repr(),
            &stake.encode(),
        )?;
    }

    // Update pool-level slash stats
    wasm::db::db_set(
        registry_db,
        &update.pool.pool_id.to_repr(),
        &update.pool.encode(),
    )?;

    msg!("[pool_stake::slash_coverage::update] Coverage slashed (per-member tracking)");

    Ok(())
}

// ============================================================================
// CLAIM FEES
// ============================================================================

fn process_claim_fees_instruction(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params= ClaimFeesParamsV1::decode(&self_.data[1..])?;

    msg!("[pool_stake::claim_fees] Claiming fees for stake {:?}", params.stake_id);

    // Validate promissory_note::transfer_v1 child call for fee payout
    let this_call = &calls[call_idx];
    if this_call.children_indexes.len() != 1 {
        msg!("[ClaimFeesV1] Expected 1 child call (promissory_note::transfer_v1)");
        return Err(PoolStakeError::InvalidChildrenIndexes.into())
    }
    let child_idx = this_call.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!("[ClaimFeesV1] Child call is not promissory_note::transfer_v1 (0x04)");
        return Err(PoolStakeError::InvalidChildCall.into())
    }

    // Validate child call targets promissory_note (prevent cross-contract routing)
    let info_db = wasm::db::db_lookup(cid, POOL_STAKE_INFO_TREE)?;
    let promissory_note_bytes = wasm::db::db_get(info_db, POOL_STAKE_PROMISSORY_NOTE_CONTRACT_ID)?
        .ok_or(PoolStakeError::InvalidChildCall)?;
    let promissory_note_cid: ContractId = deserialize(&promissory_note_bytes)?;
    // HAZOP H-11: fail-closed — reject if promissory_note not configured
    if promissory_note_cid == ContractId::ZERO {
        return Err(ContractError::IoError("promissory_note contract ID not configured".into()));
    }
    validate_child_contract_id(&child_call.contract_id, &promissory_note_cid)?;

    let stakes_db = wasm::db::db_lookup(cid, POOL_STAKE_MEMBERS_TREE)?;
    let mut stake: PoolMemberStake =
        match wasm::db::db_get(stakes_db, &params.stake_id.to_repr())? {
            Some(data) => PoolMemberStake::decode(&data)?,
            None => return Err(PoolStakeError::StakeNotFound.into()),
        };

    // Verify owner authorization
    let registry_db = wasm::db::db_lookup(cid, POOL_STAKE_REGISTRY_TREE)?;
    let pool: PoolStakeRegistry =
        match wasm::db::db_get(registry_db, &stake.pool_id.to_repr())? {
            Some(data) => PoolStakeRegistry::decode(&data)?,
            None => return Err(PoolStakeError::PoolNotFound.into()),
        };
    if params.owner_pub != pool.owner_pub {
        return Err(PoolStakeError::Unauthorized.into())
    }

    if stake.accumulated_fees == 0 {
        return Err(PoolStakeError::NoEarnings.into());
    }

    let claimed_amount = stake.accumulated_fees;
    let value_blind = poseidon_hash([
        pallas::Base::from(claimed_amount),
        stake.pool_id,
    ]);
    validate_child_value_commit(&child_call.data, claimed_amount, value_blind)?;

    // Zero the fees here, on the record exec carries through — apply re-stores it (OBL-C72).
    stake.accumulated_fees = 0;

    let update = ClaimFeesUpdateV1 { stake, claimed_amount };

    wasm::util::set_return_data(&[&[PoolStakeFunction::ClaimFeesV1 as u8], &update.encode()[..]].concat())
}

fn apply_claim_fees_update(cid: ContractId, update: ClaimFeesUpdateV1) -> ContractResult {
    let stakes_db = wasm::db::db_lookup(cid, POOL_STAKE_MEMBERS_TREE)?;

    // Blind write (OBL-C72).
    wasm::db::db_set(stakes_db, &update.stake.stake_id.to_repr(), &update.stake.encode())?;
    msg!("[pool_stake::claim_fees::update] Fees claimed");

    Ok(())
}

// ============================================================================
// UPDATE POOL CONFIG
// ============================================================================

fn process_update_pool_config_instruction(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params= UpdatePoolConfigParamsV1::decode(&self_.data[1..])?;

    msg!("[pool_stake::update_config] Updating pool {:?}", params.pool_id);

    let registry_db = wasm::db::db_lookup(cid, POOL_STAKE_REGISTRY_TREE)?;
    let mut pool: PoolStakeRegistry =
        match wasm::db::db_get(registry_db, &params.pool_id.to_repr())? {
            Some(data) => PoolStakeRegistry::decode(&data)?,
            None => return Err(PoolStakeError::PoolNotFound.into()),
        };

    if params.owner_pub != pool.owner_pub {
        return Err(PoolStakeError::Unauthorized.into())
    }

    pool.max_coverage_ratio = params.max_coverage_ratio.unwrap_or(pool.max_coverage_ratio);
    pool.operator_fee_bp = params.operator_fee_bp.unwrap_or(pool.operator_fee_bp);

    let update = UpdatePoolConfigUpdateV1 { pool };

    wasm::util::set_return_data(&[&[PoolStakeFunction::UpdatePoolConfigV1 as u8], &update.encode()[..]].concat())
}

fn apply_update_pool_config_update(
    cid: ContractId,
    update: UpdatePoolConfigUpdateV1,
) -> ContractResult {
    let registry_db = wasm::db::db_lookup(cid, POOL_STAKE_REGISTRY_TREE)?;

    // Blind write (OBL-C72).
    wasm::db::db_set(registry_db, &update.pool.pool_id.to_repr(), &update.pool.encode())?;
    msg!("[pool_stake::update_config::update] Pool config updated");

    Ok(())
}

// ============================================================================
// HELPERS
// ============================================================================
// `derive_stake_id` and `derive_allocation_id` were removed: both computed an id from the
// verifying block height while the circuit derived a different one from its public inputs, so
// the published public input was decorative and no client could predict the id it would get.
// The ids now come from the proof-bound params fields. Safety.md RC5 — one fact, one source.

// ============================================================================
// REBALANCE POOL SHARES (Phase 2d hardening)
// ============================================================================

fn process_rebalance_pool_shares_instruction(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params= RebalancePoolSharesParamsV1::decode(&self_.data[1..])?;

    msg!("[pool_stake::rebalance] Rebalancing shares for pool {:?}", params.pool_id);

    let registry_db = wasm::db::db_lookup(cid, POOL_STAKE_REGISTRY_TREE)?;
    let pool: PoolStakeRegistry =
        match wasm::db::db_get(registry_db, &params.pool_id.to_repr())? {
            Some(data) => PoolStakeRegistry::decode(&data)?,
            None => return Err(PoolStakeError::PoolNotFound.into()),
        };

    if params.owner_pub != pool.owner_pub {
        return Err(PoolStakeError::Unauthorized.into())
    }

    if !pool.is_active {
        return Err(PoolStakeError::PoolNotFound.into());
    }

    let members_db = wasm::db::db_lookup(cid, POOL_STAKE_MEMBERS_TREE)?;
    let mut total_share_bp: u32 = 0;
    let mut members_rebalanced: u64 = 0;
    let mut updated_stakes: Vec<PoolMemberStake> = vec![];

	    if params.member_ids.len() > crate::POOL_STAKE_MAX_REBALANCE_MEMBERS {
	        msg!("[pool_stake::rebalance] Too many members: {} (max {})",
	            params.member_ids.len(), crate::POOL_STAKE_MAX_REBALANCE_MEMBERS);
	        return Err(PoolStakeError::InvalidParams("too many members".into()).into());
	    }
	    for member_id in &params.member_ids {
	        let mut stake: PoolMemberStake =
	            match wasm::db::db_get(members_db, &member_id.to_repr())? {
	                Some(data) => PoolMemberStake::decode(&data)?,
	                None => continue,
	            };

	        if stake.pool_id != params.pool_id || !stake.is_active {
	            continue;
	        }

	        // Reputation-adjusted share: good relayers (low slash) gain weight
	        // new_weight = base_share * (1 / (1 + slash_count))
	        let slash_penalty = 1u32.saturating_add(stake.slash_count as u32);
	        let adjusted_bp = (stake.pool_share_bp as u64)
	            .saturating_div(slash_penalty as u64)
	            .min(u32::MAX as u64) as u32;

	        // Adjust the share here and carry the record. exec does not write (OBL-C73) — the old
	        // code wrote each member straight to the members tree from this loop, and apply was a
	        // no-op that only logged. apply now re-stores what exec collected.
	        stake.pool_share_bp = adjusted_bp;
	        total_share_bp = total_share_bp.saturating_add(adjusted_bp);
	        members_rebalanced = members_rebalanced.saturating_add(1);

        msg!(
            "[pool_stake::rebalance] Member {:?} share: {} -> {} (slash_count: {})",
            member_id, stake.pool_share_bp, adjusted_bp, stake.slash_count
        );
        updated_stakes.push(stake);
    }

    let update = RebalancePoolSharesUpdateV1 {
        pool_id: params.pool_id,
        members_rebalanced,
        total_share_bp,
        updated_stakes,
    };

    msg!("[pool_stake::rebalance] Rebalanced {} members", members_rebalanced);
    wasm::util::set_return_data(&[&[PoolStakeFunction::RebalancePoolSharesV1 as u8], &update.encode()?[..]].concat())
}

fn apply_rebalance_pool_shares_update(
    cid: ContractId,
    update: RebalancePoolSharesUpdateV1,
) -> ContractResult {
    let members_db = wasm::db::db_lookup(cid, POOL_STAKE_MEMBERS_TREE)?;

    // Blind writes — the adjusted shares were computed in exec and carried here (OBL-C72/C73).
    for stake in &update.updated_stakes {
        wasm::db::db_set(members_db, &stake.stake_id.to_repr(), &stake.encode())?;
    }

    msg!(
        "[pool_stake::rebalance::update] Pool {:?} rebalanced: {} members, total_share_bp: {}",
        update.pool_id, update.members_rebalanced, update.total_share_bp
    );

    Ok(())
}
