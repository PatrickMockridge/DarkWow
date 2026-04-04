/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

use darkfi_sdk::{
    crypto::ContractId,
    dark_tree::DarkLeaf,
    error::ContractResult,
    msg, ContractCall,
    pasta::pallas,
    wasm,
};
use darkfi_serial::{deserialize, serialize};

use crate::error::PoolStakeError;
use crate::model::*;
use crate::PoolStakeFunction;
use crate::{
    POOL_STAKE_ALLOCATIONS_TREE, POOL_STAKE_MEMBERS_TREE, POOL_STAKE_REGISTRY_TREE,
    POOL_STAKE_MIN_STAKE,
};

darkfi_sdk::define_contract!(
    init: init_contract,
    exec: process_instruction,
    apply: process_update,
    metadata: get_metadata
);

// ============================================================================
// INITIALIZATION
// ============================================================================

fn init_contract(cid: ContractId, _ix: &[u8]) -> ContractResult {
    wasm::db::db_init(cid, POOL_STAKE_REGISTRY_TREE)?;
    wasm::db::db_init(cid, POOL_STAKE_MEMBERS_TREE)?;
    wasm::db::db_init(cid, POOL_STAKE_ALLOCATIONS_TREE)?;
    Ok(())
}

fn get_metadata(_cid: ContractId, _ix: &[u8]) -> ContractResult {
    Ok(())
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
        stake_id,
        pool_id: params.pool_id,
        member_pub: pool.owner_pub, // Placeholder
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

    // Get stake
    let stakes_db = wasm::db::db_lookup(cid, POOL_STAKE_MEMBERS_TREE)?;
    let stake: PoolMemberStake =
        match wasm::db::db_get(stakes_db, &serialize(&params.stake_id))? {
            Some(data) => deserialize(&data)?,
            None => return Err(PoolStakeError::StakeNotFound.into()),
        };

    if !stake.is_active {
        return Err(PoolStakeError::StakeLocked.into());
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
    use darkfi_sdk::pasta::pallas;

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

    // Find contributing members (simplified - proportional from all members)
    let _members_db = wasm::db::db_lookup(cid, POOL_STAKE_MEMBERS_TREE)?;
    let contributing_members = vec![pallas::Base::zero()]; // Placeholder

    let update = AllocateCoverageUpdateV1 {
        allocation_id,
        pool_id: params.pool_id,
        withdrawal_nullifier: params.withdrawal_nullifier,
        amount: params.amount,
        contributing_members,
        available_coverage: pool.available_coverage - params.amount,
        allocated_coverage: pool.allocated_coverage + params.amount,
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
        timeout_height: 0, // Would be set from params
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

    // Update pool
    let mut pool: PoolStakeRegistry =
        match wasm::db::db_get(registry_db, &serialize(&update.allocation_id))? {
            Some(data) => deserialize(&data)?,
            // Use pool_id from allocation instead
            None => return Err(PoolStakeError::PoolNotFound.into()),
        };

    pool.available_coverage = update.available_coverage;
    pool.allocated_coverage = update.allocated_coverage;

    wasm::db::db_set(registry_db, &serialize(&pool.pool_id), &serialize(&pool))?;

    // Update allocation
    let mut allocation: CoverageAllocation =
        match wasm::db::db_get(allocations_db, &serialize(&update.allocation_id))? {
            Some(data) => deserialize(&data)?,
            None => return Err(PoolStakeError::AllocationNotFound.into()),
        };

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

    let update = SlashCoverageUpdateV1 {
        allocation_id: params.allocation_id,
        slashed_amount: params.slash_amount,
        compensated_user: [0u8; 32], // Placeholder
        available_coverage: pool.available_coverage,
        allocated_coverage: pool.allocated_coverage - params.slash_amount,
    };

    wasm::util::set_return_data(&serialize(&update))
}

fn apply_slash_coverage_update(cid: ContractId, update: SlashCoverageUpdateV1) -> ContractResult {
    let allocations_db = wasm::db::db_lookup(cid, POOL_STAKE_ALLOCATIONS_TREE)?;

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
    msg!("[pool_stake::slash_coverage::update] Coverage slashed");

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

    let stakes_db = wasm::db::db_lookup(cid, POOL_STAKE_MEMBERS_TREE)?;
    let stake: PoolMemberStake =
        match wasm::db::db_get(stakes_db, &serialize(&params.stake_id))? {
            Some(data) => deserialize(&data)?,
            None => return Err(PoolStakeError::StakeNotFound.into()),
        };

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
    use darkfi_sdk::crypto::poseidon_hash;
    use darkfi_sdk::pasta::pallas;
    poseidon_hash([pallas::Base::from(nonce)])
}

fn derive_stake_id(pool_id: pallas::Base, relayer_id: &[u8; 32], nonce: u64) -> pallas::Base {
    use darkfi_sdk::crypto::poseidon_hash;
    use darkfi_sdk::pasta::pallas;
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
    use darkfi_sdk::crypto::poseidon_hash;
    use darkfi_sdk::pasta::pallas;
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