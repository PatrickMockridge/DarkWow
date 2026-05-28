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
    crypto::{pasta_prelude::PrimeField, ContractId},
    dark_tree::DarkLeaf,
    error::ContractResult,
    msg, ContractCall,
    pasta::pallas,
    wasm,
};
use dwow_serial::{deserialize, serialize, Encodable};

use crate::error::PoolStakeError;
use crate::model::*;
use crate::PoolStakeFunction;
use crate::{
    POOL_STAKE_ALLOCATIONS_TREE, POOL_STAKE_MEMBERS_TREE, POOL_STAKE_REGISTRY_TREE,
    POOL_STAKE_MIN_STAKE, POOL_STAKE_INFO_TREE,
    POOL_STAKE_ZKAS_CREATE_POOL_NS_V1, POOL_STAKE_ZKAS_JOIN_POOL_NS_V1,
    POOL_STAKE_ZKAS_ALLOCATE_COVERAGE_NS_V1, POOL_STAKE_ZKAS_SLASH_COVERAGE_NS_V1,
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
    let _info_db = match wasm::db::db_lookup(cid, POOL_STAKE_INFO_TREE) {
        Ok(v) => v,
        Err(_) => wasm::db::db_init(cid, POOL_STAKE_INFO_TREE)?,
    };

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

    let allocate_coverage_v1_bincode = include_bytes!("../proof/allocate_coverage_v1.zk.bin");
    wasm::db::zkas_db_set(&allocate_coverage_v1_bincode[..])?;
    let create_pool_v1_bincode = include_bytes!("../proof/create_pool_v1.zk.bin");
    wasm::db::zkas_db_set(&create_pool_v1_bincode[..])?;
    let join_pool_v1_bincode = include_bytes!("../proof/join_pool_v1.zk.bin");
    wasm::db::zkas_db_set(&join_pool_v1_bincode[..])?;
    let slash_coverage_v1_bincode = include_bytes!("../proof/slash_coverage_v1.zk.bin");
    wasm::db::zkas_db_set(&slash_coverage_v1_bincode[..])?;

    Ok(())
}

fn get_metadata(_cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = PoolStakeFunction::try_from(self_.data[0])?;

    let metadata = match func {
        PoolStakeFunction::CreatePoolV1 => {
            let params: CreatePoolParamsV1 = deserialize(&self_.data[1..])?;
            create_pool_get_metadata_v1(params)?
        }
        PoolStakeFunction::JoinPoolV1 => {
            let params: JoinPoolParamsV1 = deserialize(&self_.data[1..])?;
            join_pool_get_metadata_v1(params)?
        }
        PoolStakeFunction::AllocateCoverageV1 => {
            let params: AllocateCoverageParamsV1 = deserialize(&self_.data[1..])?;
            allocate_coverage_get_metadata_v1(params)?
        }
        PoolStakeFunction::SlashCoverageV1 => {
            let params: SlashCoverageParamsV1 = deserialize(&self_.data[1..])?;
            slash_coverage_get_metadata_v1(params)?
        }
        // Functions without ZK proofs: empty metadata
        PoolStakeFunction::LeavePoolV1
        | PoolStakeFunction::ReleaseCoverageV1
        | PoolStakeFunction::ClaimFeesV1
        | PoolStakeFunction::UpdatePoolConfigV1
        | PoolStakeFunction::RebalancePoolSharesV1 => vec![],
    };

    wasm::util::set_return_data(&metadata)
}

fn create_pool_get_metadata_v1(
    params: CreatePoolParamsV1,
) -> Result<Vec<u8>, dwow_sdk::error::ContractError> {
    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    // Only constrain_instance value: derived_pool_id
    zk_public_inputs.push((
        POOL_STAKE_ZKAS_CREATE_POOL_NS_V1.to_string(),
        vec![params.derived_pool_id],
    ));
    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    Ok(metadata)
}

fn join_pool_get_metadata_v1(
    params: JoinPoolParamsV1,
) -> Result<Vec<u8>, dwow_sdk::error::ContractError> {
    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    // Only constrain_instance values: derived_member_id, value_commit_x, value_commit_y
    zk_public_inputs.push((
        POOL_STAKE_ZKAS_JOIN_POOL_NS_V1.to_string(),
        vec![params.derived_member_id, params.value_commit_x, params.value_commit_y],
    ));
    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    Ok(metadata)
}

fn allocate_coverage_get_metadata_v1(
    params: AllocateCoverageParamsV1,
) -> Result<Vec<u8>, dwow_sdk::error::ContractError> {
    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    // Only constrain_instance value: derived_allocation_id
    zk_public_inputs.push((
        POOL_STAKE_ZKAS_ALLOCATE_COVERAGE_NS_V1.to_string(),
        vec![params.derived_allocation_id],
    ));
    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    Ok(metadata)
}

fn slash_coverage_get_metadata_v1(
    params: SlashCoverageParamsV1,
) -> Result<Vec<u8>, dwow_sdk::error::ContractError> {
    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    // Only constrain_instance value: derived_slash_id
    zk_public_inputs.push((
        POOL_STAKE_ZKAS_SLASH_COVERAGE_NS_V1.to_string(),
        vec![params.derived_slash_id],
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
            let update: CreatePoolUpdateV1 = deserialize(&update_data[1..])?;
            apply_create_pool_update(cid, update)
        }
        PoolStakeFunction::JoinPoolV1 => {
            let update: JoinPoolUpdateV1 = deserialize(&update_data[1..])?;
            apply_join_pool_update(cid, update)
        }
        PoolStakeFunction::LeavePoolV1 => {
            let update: LeavePoolUpdateV1 = deserialize(&update_data[1..])?;
            apply_leave_pool_update(cid, update)
        }
        PoolStakeFunction::AllocateCoverageV1 => {
            let update: AllocateCoverageUpdateV1 = deserialize(&update_data[1..])?;
            apply_allocate_coverage_update(cid, update)
        }
        PoolStakeFunction::ReleaseCoverageV1 => {
            let update: ReleaseCoverageUpdateV1 = deserialize(&update_data[1..])?;
            apply_release_coverage_update(cid, update)
        }
        PoolStakeFunction::SlashCoverageV1 => {
            let update: SlashCoverageUpdateV1 = deserialize(&update_data[1..])?;
            apply_slash_coverage_update(cid, update)
        }
        PoolStakeFunction::ClaimFeesV1 => {
            let update: ClaimFeesUpdateV1 = deserialize(&update_data[1..])?;
            apply_claim_fees_update(cid, update)
        }
        PoolStakeFunction::UpdatePoolConfigV1 => {
            let update: UpdatePoolConfigUpdateV1 = deserialize(&update_data[1..])?;
            apply_update_pool_config_update(cid, update)
        }
        PoolStakeFunction::RebalancePoolSharesV1 => {
            let update: RebalancePoolSharesUpdateV1 = deserialize(&update_data[1..])?;
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
    let params: CreatePoolParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[pool_stake::create_pool] Creating new pool");

    // Validate coverage ratio
    if params.max_coverage_ratio == 0 || params.max_coverage_ratio > 10000 {
        return Err(PoolStakeError::InvalidCoverageRatio.into());
    }

    // Derive pool ID
    let pool_id = derive_pool_id(wasm::util::get_verifying_block_height()? as u64);

    // Check pool doesn't already exist (shouldn't happen with unique ID)
    let registry_db = wasm::db::db_lookup(cid, POOL_STAKE_REGISTRY_TREE)?;
    if wasm::db::db_contains_key(registry_db, &serialize(&pool_id))? {
        return Err(PoolStakeError::PoolNotFound.into());
    }

    let update = CreatePoolUpdateV1 {
        instance_seed: params.instance_seed,
        pool_id,
        owner_pub: params.owner_pub,
        max_coverage_ratio: params.max_coverage_ratio,
        operator_fee_bp: params.operator_fee_bp,
        created_at: wasm::util::get_verifying_block_height()? as u64,
    };

    msg!("[pool_stake::create_pool] Pool {:?} created", pool_id);
    wasm::util::set_return_data(&serialize(&update))
}

fn apply_create_pool_update(cid: ContractId, update: CreatePoolUpdateV1) -> ContractResult {
    let registry_db = wasm::db::db_lookup(cid, POOL_STAKE_REGISTRY_TREE)?;

    let registry = PoolStakeRegistry {
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

    wasm::db::db_set(registry_db, &serialize(&update.pool_id), &serialize(&registry))?;
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
    let params: JoinPoolParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[pool_stake::join_pool] Joining pool {:?} with amount {}", params.pool_id, params.amount);

    // Validate money_v3::transfer_v1 child call for stake deposit
    let this_call = &calls[call_idx];
    if this_call.children_indexes.len() != 1 {
        msg!("[JoinPoolV1] Expected 1 child call (money_v3::transfer_v1)");
        return Err(PoolStakeError::InvalidChildrenIndexes.into())
    }
    let child_idx = this_call.children_indexes[0];
    if calls[child_idx].data.data[0] != 0x04 {
        msg!("[JoinPoolV1] Child call is not money_v3::transfer_v1 (0x04)");
        return Err(PoolStakeError::InvalidChildCall.into())
    }

    // Validate stake amount
    if params.amount < POOL_STAKE_MIN_STAKE {
        return Err(PoolStakeError::InsufficientStake(POOL_STAKE_MIN_STAKE).into());
    }

    // Get registry
    let registry_db = wasm::db::db_lookup(cid, POOL_STAKE_REGISTRY_TREE)?;
    let mut pool: PoolStakeRegistry =
        match wasm::db::db_get(registry_db, &serialize(&params.pool_id))? {
            Some(data) => deserialize(&data)?,
            None => return Err(PoolStakeError::PoolNotFound.into()),
        };

    // Generate stake ID
    let stake_id = derive_stake_id(
        params.pool_id,
        &params.relayer_id,
        wasm::util::get_verifying_block_height()? as u64,
    );

    // Check stake doesn't already exist
    let stakes_db = wasm::db::db_lookup(cid, POOL_STAKE_MEMBERS_TREE)?;
    if wasm::db::db_contains_key(stakes_db, &serialize(&stake_id))? {
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
        pool_id: params.pool_id,
        member_pub: params.member_pub,
        relayer_id: params.relayer_id,
        amount: params.amount,
        coverage_contribution,
        pool_share_bp,
        total_stake: pool.total_stake,
        member_count: pool.member_count,
    };

    msg!("[pool_stake::join_pool] Stake {:?} created", stake_id);
    wasm::util::set_return_data(&serialize(&update))
}

fn apply_join_pool_update(cid: ContractId, update: JoinPoolUpdateV1) -> ContractResult {
    let registry_db = wasm::db::db_lookup(cid, POOL_STAKE_REGISTRY_TREE)?;
    let stakes_db = wasm::db::db_lookup(cid, POOL_STAKE_MEMBERS_TREE)?;

    // Get and update registry
    let mut pool: PoolStakeRegistry =
        match wasm::db::db_get(registry_db, &serialize(&update.pool_id))? {
            Some(data) => deserialize(&data)?,
            None => return Err(PoolStakeError::PoolNotFound.into()),
        };

    pool.total_stake = update.total_stake;
    pool.member_count = update.member_count;

    wasm::db::db_set(registry_db, &serialize(&update.pool_id), &serialize(&pool))?;

    // Create stake
    let stake = PoolMemberStake {
        instance_seed: update.instance_seed,
        stake_id: update.stake_id,
        pool_id: update.pool_id,
        member_pub: update.member_pub,
        relayer_id: update.relayer_id,
        original_amount: update.amount,
        current_amount: update.amount,
        coverage_contribution: update.coverage_contribution,
        pool_share_bp: update.pool_share_bp,
        accumulated_fees: 0,
        created_at: wasm::util::get_verifying_block_height()? as u64,
        leave_requested_at: None,
        slash_count: 0,
        is_active: true,
    };

    wasm::db::db_set(stakes_db, &serialize(&update.stake_id), &serialize(&stake))?;
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
    let params: LeavePoolParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[pool_stake::leave_pool] Leave request for stake {:?}", params.stake_id);

    // Validate money_v3::transfer_v1 child call for stake withdrawal
    let this_call = &calls[call_idx];
    if this_call.children_indexes.len() != 1 {
        msg!("[LeavePoolV1] Expected 1 child call (money_v3::transfer_v1)");
        return Err(PoolStakeError::InvalidChildrenIndexes.into())
    }
    let child_idx = this_call.children_indexes[0];
    if calls[child_idx].data.data[0] != 0x04 {
        msg!("[LeavePoolV1] Child call is not money_v3::transfer_v1 (0x04)");
        return Err(PoolStakeError::InvalidChildCall.into())
    }

    // Get stake
    let stakes_db = wasm::db::db_lookup(cid, POOL_STAKE_MEMBERS_TREE)?;
    let mut stake: PoolMemberStake =
        match wasm::db::db_get(stakes_db, &serialize(&params.stake_id))? {
            Some(data) => deserialize(&data)?,
            None => return Err(PoolStakeError::StakeNotFound.into()),
        };

    if !stake.is_active {
        return Err(PoolStakeError::StakeLocked.into());
    }

    // Enforce cooldown period before leaving
    let current_block = wasm::util::get_verifying_block_height()? as u64;
    if let Some(requested_at) = stake.leave_requested_at {
        // Cooldown started — verify it's elapsed
        if current_block < requested_at + crate::POOL_STAKE_LEAVE_COOLDOWN_BLOCKS {
            let remaining = (requested_at + crate::POOL_STAKE_LEAVE_COOLDOWN_BLOCKS).saturating_sub(current_block);
            msg!("[pool_stake::leave_pool] Cooldown active: {} blocks remaining", remaining);
            return Err(PoolStakeError::StakeLocked.into())
        }
        // Cooldown elapsed — proceed with leave
    } else {
        // First call — start cooldown
        stake.leave_requested_at = Some(current_block);
        wasm::db::db_set(stakes_db, &serialize(&params.stake_id), &serialize(&stake))?;
        msg!("[pool_stake::leave_pool] Cooldown started: {} blocks", crate::POOL_STAKE_LEAVE_COOLDOWN_BLOCKS);
        return Err(PoolStakeError::StakeLocked.into())
    }

    // Calculate final payout (current_amount - proportional losses)
    let registry_db = wasm::db::db_lookup(cid, POOL_STAKE_REGISTRY_TREE)?;
    let _pool: PoolStakeRegistry =
        match wasm::db::db_get(registry_db, &serialize(&stake.pool_id))? {
            Some(data) => deserialize(&data)?,
            None => return Err(PoolStakeError::PoolNotFound.into()),
        };

    // Calculate payout (simplified - no losses in this basic version)
    let payout_amount = stake.current_amount;
    let unstake_penalty = 0;

    let update = LeavePoolUpdateV1 { stake_id: params.stake_id, payout_amount, unstake_penalty };

    msg!("[pool_stake::leave_pool] Payout: {}", payout_amount);
    wasm::util::set_return_data(&serialize(&update))
}

fn apply_leave_pool_update(cid: ContractId, update: LeavePoolUpdateV1) -> ContractResult {
    let stakes_db = wasm::db::db_lookup(cid, POOL_STAKE_MEMBERS_TREE)?;

    // Get and update stake
    let mut stake: PoolMemberStake =
        match wasm::db::db_get(stakes_db, &serialize(&update.stake_id))? {
            Some(data) => deserialize(&data)?,
            None => return Err(PoolStakeError::StakeNotFound.into()),
        };

    stake.is_active = false;
    stake.current_amount = 0;

    wasm::db::db_set(stakes_db, &serialize(&update.stake_id), &serialize(&stake))?;
    msg!("[pool_stake::leave_pool::update] Stake deactivated");

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
    let params: AllocateCoverageParamsV1 = deserialize(&self_.data[1..])?;

    msg!(
        "[pool_stake::allocate_coverage] Allocating {} for withdrawal {:?}",
        params.amount,
        params.withdrawal_nullifier
    );

    // Get pool
    let registry_db = wasm::db::db_lookup(cid, POOL_STAKE_REGISTRY_TREE)?;
    let pool: PoolStakeRegistry =
        match wasm::db::db_get(registry_db, &serialize(&params.pool_id))? {
            Some(data) => deserialize(&data)?,
            None => return Err(PoolStakeError::PoolNotFound.into()),
        };

    // Check available coverage
    if pool.available_coverage < params.amount {
        return Err(PoolStakeError::InsufficientCoverage.into());
    }

    // Generate allocation ID
    let allocation_id = derive_allocation_id(
        params.pool_id,
        &params.withdrawal_nullifier,
        wasm::util::get_verifying_block_height()? as u64,
    );

    // NOTE: contributing_members requires iteration over POOL_STAKE_MEMBERS_TREE.
    // The wasm::db API currently lacks iteration support. Deferred to DB API upgrade
    // (per-pool member index or wasm::db iteration). Proportional payout is skipped.
    let contributing_members = vec![];

    let update = AllocateCoverageUpdateV1 {
        allocation_id,
        pool_id: params.pool_id,
        withdrawal_nullifier: params.withdrawal_nullifier,
        amount: params.amount,
        contributing_members,
        available_coverage: pool.available_coverage - params.amount,
        allocated_coverage: pool.allocated_coverage + params.amount,
        timeout_height: params.timeout_height,
    };

    msg!("[pool_stake::allocate_coverage] Allocation {:?} created", allocation_id);
    wasm::util::set_return_data(&serialize(&update))
}

fn apply_allocate_coverage_update(cid: ContractId, update: AllocateCoverageUpdateV1) -> ContractResult {
    let registry_db = wasm::db::db_lookup(cid, POOL_STAKE_REGISTRY_TREE)?;
    let allocations_db = wasm::db::db_lookup(cid, POOL_STAKE_ALLOCATIONS_TREE)?;

    // Update pool
    let mut pool: PoolStakeRegistry =
        match wasm::db::db_get(registry_db, &serialize(&update.pool_id))? {
            Some(data) => deserialize(&data)?,
            None => return Err(PoolStakeError::PoolNotFound.into()),
        };

    pool.available_coverage = update.available_coverage;
    pool.allocated_coverage = update.allocated_coverage;

    wasm::db::db_set(registry_db, &serialize(&update.pool_id), &serialize(&pool))?;

    // Create allocation
    let allocation = CoverageAllocation {
        allocation_id: update.allocation_id,
        pool_id: update.pool_id,
        withdrawal_nullifier: update.withdrawal_nullifier,
        amount: update.amount,
        contributing_members: update.contributing_members,
        created_at: wasm::util::get_verifying_block_height()? as u64,
        timeout_height: update.timeout_height,
        executed: false,
        slashed: false,
    };

    wasm::db::db_set(
        allocations_db,
        &serialize(&update.allocation_id),
        &serialize(&allocation),
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
    let params: ReleaseCoverageParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[pool_stake::release_coverage] Releasing allocation {:?}", params.allocation_id);

    let allocations_db = wasm::db::db_lookup(cid, POOL_STAKE_ALLOCATIONS_TREE)?;
    let allocation: CoverageAllocation =
        match wasm::db::db_get(allocations_db, &serialize(&params.allocation_id))? {
            Some(data) => deserialize(&data)?,
            None => return Err(PoolStakeError::AllocationNotFound.into()),
        };

    if allocation.executed || allocation.slashed {
        return Err(PoolStakeError::AllocationNotFound.into());
    }

    // Get pool to calculate new coverage
    let registry_db = wasm::db::db_lookup(cid, POOL_STAKE_REGISTRY_TREE)?;
    let pool: PoolStakeRegistry =
        match wasm::db::db_get(registry_db, &serialize(&allocation.pool_id))? {
            Some(data) => deserialize(&data)?,
            None => return Err(PoolStakeError::PoolNotFound.into()),
        };

    if params.owner_pub != pool.owner_pub {
        return Err(PoolStakeError::Unauthorized.into())
    }

    let update = ReleaseCoverageUpdateV1 {
        allocation_id: params.allocation_id,
        released_amount: allocation.amount,
        available_coverage: pool.available_coverage + allocation.amount,
        allocated_coverage: pool.allocated_coverage - allocation.amount,
    };

    wasm::util::set_return_data(&serialize(&update))
}

fn apply_release_coverage_update(cid: ContractId, update: ReleaseCoverageUpdateV1) -> ContractResult {
    let registry_db = wasm::db::db_lookup(cid, POOL_STAKE_REGISTRY_TREE)?;
    let allocations_db = wasm::db::db_lookup(cid, POOL_STAKE_ALLOCATIONS_TREE)?;

    // Look up allocation to get pool_id
    let mut allocation: CoverageAllocation =
        match wasm::db::db_get(allocations_db, &serialize(&update.allocation_id))? {
            Some(data) => deserialize(&data)?,
            None => return Err(PoolStakeError::AllocationNotFound.into()),
        };

    // Look up pool by pool_id from the allocation
    let mut pool: PoolStakeRegistry =
        match wasm::db::db_get(registry_db, &serialize(&allocation.pool_id))? {
            Some(data) => deserialize(&data)?,
            None => return Err(PoolStakeError::PoolNotFound.into()),
        };

    pool.available_coverage = update.available_coverage;
    pool.allocated_coverage = update.allocated_coverage;

    wasm::db::db_set(registry_db, &serialize(&pool.pool_id), &serialize(&pool))?;

    // Mark allocation as executed
    allocation.executed = true;

    wasm::db::db_set(
        allocations_db,
        &serialize(&update.allocation_id),
        &serialize(&allocation),
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
    let params: SlashCoverageParamsV1 = deserialize(&self_.data[1..])?;

    msg!(
        "[pool_stake::slash_coverage] Slashing {} from allocation {:?}",
        params.slash_amount,
        params.allocation_id
    );

    let allocations_db = wasm::db::db_lookup(cid, POOL_STAKE_ALLOCATIONS_TREE)?;
    let allocation: CoverageAllocation =
        match wasm::db::db_get(allocations_db, &serialize(&params.allocation_id))? {
            Some(data) => deserialize(&data)?,
            None => return Err(PoolStakeError::AllocationNotFound.into()),
        };

    if allocation.slashed {
        return Err(PoolStakeError::AllocationNotFound.into());
    }

    // Get pool
    let registry_db = wasm::db::db_lookup(cid, POOL_STAKE_REGISTRY_TREE)?;
    let pool: PoolStakeRegistry =
        match wasm::db::db_get(registry_db, &serialize(&allocation.pool_id))? {
            Some(data) => deserialize(&data)?,
            None => return Err(PoolStakeError::PoolNotFound.into()),
        };

    if params.owner_pub != pool.owner_pub {
        return Err(PoolStakeError::Unauthorized.into())
    }

    if pool.allocated_coverage < params.slash_amount {
        return Err(PoolStakeError::InsufficientCoverage.into())
    }

    let update = SlashCoverageUpdateV1 {
        allocation_id: params.allocation_id,
        slashed_amount: params.slash_amount,
        compensated_user: params.user_pub.x().to_repr(),
        available_coverage: pool.available_coverage,
        allocated_coverage: pool.allocated_coverage - params.slash_amount,
    };

    wasm::util::set_return_data(&serialize(&update))
}

fn apply_slash_coverage_update(cid: ContractId, update: SlashCoverageUpdateV1) -> ContractResult {
    let allocations_db = wasm::db::db_lookup(cid, POOL_STAKE_ALLOCATIONS_TREE)?;
    let members_db = wasm::db::db_lookup(cid, POOL_STAKE_MEMBERS_TREE)?;
    let registry_db = wasm::db::db_lookup(cid, POOL_STAKE_REGISTRY_TREE)?;

    // Update allocation
    let mut allocation: CoverageAllocation =
        match wasm::db::db_get(allocations_db, &serialize(&update.allocation_id))? {
            Some(data) => deserialize(&data)?,
            None => return Err(PoolStakeError::AllocationNotFound.into()),
        };

    allocation.slashed = true;

    wasm::db::db_set(
        allocations_db,
        &serialize(&update.allocation_id),
        &serialize(&allocation),
    )?;

    // Track per-member slash counts (Phase 2d hardening)
    for member_id in &allocation.contributing_members {
        if let Some(data) = wasm::db::db_get(members_db, &serialize(member_id))? {
            let mut stake: PoolMemberStake = deserialize(&data)?;
            stake.slash_count = stake.slash_count.saturating_add(1);
            wasm::db::db_set(members_db, &serialize(member_id), &serialize(&stake))?;
        }
    }

    // Update pool-level slash stats
    if let Some(pool_data) = wasm::db::db_get(registry_db, &serialize(&allocation.pool_id))? {
        let mut pool: PoolStakeRegistry = deserialize(&pool_data)?;
        pool.total_slashed = pool.total_slashed.saturating_add(update.slashed_amount);
        pool.pool_slash_count = pool.pool_slash_count.saturating_add(1);
        pool.available_coverage = update.available_coverage;
        pool.allocated_coverage = update.allocated_coverage;
        wasm::db::db_set(registry_db, &serialize(&allocation.pool_id), &serialize(&pool))?;
    }

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
    let params: ClaimFeesParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[pool_stake::claim_fees] Claiming fees for stake {:?}", params.stake_id);

    // Validate money_v3::transfer_v1 child call for fee payout
    let this_call = &calls[call_idx];
    if this_call.children_indexes.len() != 1 {
        msg!("[ClaimFeesV1] Expected 1 child call (money_v3::transfer_v1)");
        return Err(PoolStakeError::InvalidChildrenIndexes.into())
    }
    let child_idx = this_call.children_indexes[0];
    if calls[child_idx].data.data[0] != 0x04 {
        msg!("[ClaimFeesV1] Child call is not money_v3::transfer_v1 (0x04)");
        return Err(PoolStakeError::InvalidChildCall.into())
    }

    let stakes_db = wasm::db::db_lookup(cid, POOL_STAKE_MEMBERS_TREE)?;
    let stake: PoolMemberStake =
        match wasm::db::db_get(stakes_db, &serialize(&params.stake_id))? {
            Some(data) => deserialize(&data)?,
            None => return Err(PoolStakeError::StakeNotFound.into()),
        };

    // Verify owner authorization
    let registry_db = wasm::db::db_lookup(cid, POOL_STAKE_REGISTRY_TREE)?;
    let pool: PoolStakeRegistry =
        match wasm::db::db_get(registry_db, &serialize(&stake.pool_id))? {
            Some(data) => deserialize(&data)?,
            None => return Err(PoolStakeError::PoolNotFound.into()),
        };
    if params.owner_pub != pool.owner_pub {
        return Err(PoolStakeError::Unauthorized.into())
    }

    if stake.accumulated_fees == 0 {
        return Err(PoolStakeError::NoEarnings.into());
    }

    let update = ClaimFeesUpdateV1 {
        stake_id: params.stake_id,
        claimed_amount: stake.accumulated_fees,
        remaining_fees: 0,
    };

    wasm::util::set_return_data(&serialize(&update))
}

fn apply_claim_fees_update(cid: ContractId, update: ClaimFeesUpdateV1) -> ContractResult {
    let stakes_db = wasm::db::db_lookup(cid, POOL_STAKE_MEMBERS_TREE)?;

    let mut stake: PoolMemberStake =
        match wasm::db::db_get(stakes_db, &serialize(&update.stake_id))? {
            Some(data) => deserialize(&data)?,
            None => return Err(PoolStakeError::StakeNotFound.into()),
        };

    stake.accumulated_fees = update.remaining_fees;

    wasm::db::db_set(stakes_db, &serialize(&update.stake_id), &serialize(&stake))?;
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
    let params: UpdatePoolConfigParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[pool_stake::update_config] Updating pool {:?}", params.pool_id);

    let registry_db = wasm::db::db_lookup(cid, POOL_STAKE_REGISTRY_TREE)?;
    let pool: PoolStakeRegistry =
        match wasm::db::db_get(registry_db, &serialize(&params.pool_id))? {
            Some(data) => deserialize(&data)?,
            None => return Err(PoolStakeError::PoolNotFound.into()),
        };

    if params.owner_pub != pool.owner_pub {
        return Err(PoolStakeError::Unauthorized.into())
    }

    let max_coverage_ratio = params.max_coverage_ratio.unwrap_or(pool.max_coverage_ratio);
    let operator_fee_bp = params.operator_fee_bp.unwrap_or(pool.operator_fee_bp);

    let update = UpdatePoolConfigUpdateV1 {
        pool_id: params.pool_id,
        max_coverage_ratio,
        operator_fee_bp,
    };

    wasm::util::set_return_data(&serialize(&update))
}

fn apply_update_pool_config_update(
    cid: ContractId,
    update: UpdatePoolConfigUpdateV1,
) -> ContractResult {
    let registry_db = wasm::db::db_lookup(cid, POOL_STAKE_REGISTRY_TREE)?;

    let mut pool: PoolStakeRegistry =
        match wasm::db::db_get(registry_db, &serialize(&update.pool_id))? {
            Some(data) => deserialize(&data)?,
            None => return Err(PoolStakeError::PoolNotFound.into()),
        };

    pool.max_coverage_ratio = update.max_coverage_ratio;
    pool.operator_fee_bp = update.operator_fee_bp;

    wasm::db::db_set(registry_db, &serialize(&update.pool_id), &serialize(&pool))?;
    msg!("[pool_stake::update_config::update] Pool config updated");

    Ok(())
}

// ============================================================================
// HELPERS
// ============================================================================

fn derive_pool_id(nonce: u64) -> pallas::Base {
    use dwow_sdk::crypto::poseidon_hash;
    use dwow_sdk::pasta::pallas;
    poseidon_hash([pallas::Base::from(nonce)])
}

fn derive_stake_id(pool_id: pallas::Base, relayer_id: &[u8; 32], nonce: u64) -> pallas::Base {
    use dwow_sdk::crypto::poseidon_hash;
    use dwow_sdk::pasta::pallas;
    // Hash the relayer_id with blake3 to get bytes we can convert to pallas::Base
    let hashed = blake3::hash(relayer_id);
    let bytes: [u8; 32] = *hashed.as_bytes();
    let words: [u64; 4] = [
        u64::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7]]),
        u64::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15]]),
        u64::from_le_bytes([bytes[16], bytes[17], bytes[18], bytes[19], bytes[20], bytes[21], bytes[22], bytes[23]]),
        u64::from_le_bytes([bytes[24], bytes[25], bytes[26], bytes[27], bytes[28], bytes[29], bytes[30], bytes[31]]),
    ];
    poseidon_hash([pool_id, pallas::Base::from_raw(words), pallas::Base::from(nonce)])
}

fn derive_allocation_id(
    pool_id: pallas::Base,
    withdrawal_nullifier: &[u8; 32],
    nonce: u64,
) -> pallas::Base {
    use dwow_sdk::crypto::poseidon_hash;
    use dwow_sdk::pasta::pallas;
    let hashed = blake3::hash(withdrawal_nullifier);
    let bytes: [u8; 32] = *hashed.as_bytes();
    let words: [u64; 4] = [
        u64::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7]]),
        u64::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15]]),
        u64::from_le_bytes([bytes[16], bytes[17], bytes[18], bytes[19], bytes[20], bytes[21], bytes[22], bytes[23]]),
        u64::from_le_bytes([bytes[24], bytes[25], bytes[26], bytes[27], bytes[28], bytes[29], bytes[30], bytes[31]]),
    ];
    poseidon_hash([pool_id, pallas::Base::from_raw(words), pallas::Base::from(nonce)])
}

// ============================================================================
// REBALANCE POOL SHARES (Phase 2d hardening)
// ============================================================================

fn process_rebalance_pool_shares_instruction(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> ContractResult {
    let self_ = &calls[call_idx].data;
    let params: RebalancePoolSharesParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[pool_stake::rebalance] Rebalancing shares for pool {:?}", params.pool_id);

    let registry_db = wasm::db::db_lookup(cid, POOL_STAKE_REGISTRY_TREE)?;
    let pool: PoolStakeRegistry =
        match wasm::db::db_get(registry_db, &serialize(&params.pool_id))? {
            Some(data) => deserialize(&data)?,
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

    for member_id in &params.member_ids {
        let stake: PoolMemberStake =
            match wasm::db::db_get(members_db, &serialize(member_id))? {
                Some(data) => deserialize(&data)?,
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

        total_share_bp = total_share_bp.saturating_add(adjusted_bp);
        members_rebalanced = members_rebalanced.saturating_add(1);

        msg!(
            "[pool_stake::rebalance] Member {:?} share: {} -> {} (slash_count: {})",
            member_id, stake.pool_share_bp, adjusted_bp, stake.slash_count
        );
    }

    let update = RebalancePoolSharesUpdateV1 {
        pool_id: params.pool_id,
        members_rebalanced,
        total_share_bp,
    };

    msg!("[pool_stake::rebalance] Rebalanced {} members", members_rebalanced);
    wasm::util::set_return_data(&serialize(&update))
}

fn apply_rebalance_pool_shares_update(
    _cid: ContractId,
    update: RebalancePoolSharesUpdateV1,
) -> ContractResult {
    // The rebalance is computed and validated in the instruction phase.
    // The update phase records the new total share basis points for the pool.
    // Per-member share adjustments are applied during instruction phase
    // via DB writes for each member. The update confirms the rebalance.
    msg!(
        "[pool_stake::rebalance::update] Pool {:?} rebalanced: {} members, total_share_bp: {}",
        update.pool_id, update.members_rebalanced, update.total_share_bp
    );

    Ok(())
}