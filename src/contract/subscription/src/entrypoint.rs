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

//! WASM entrypoint for the subscription contract
//!
//! ## Subscription Contract Overview
//!
//! Privacy-preserving member subscription service with:
//! - Block-based time locks (no oracle needed)
//! - DAO treasury for subscription fees
//! - Endowment fund for insurance/refunds
//! - Cross-chain atomic swap integration
//!
//! ## State Machine
//!
//! ```text
//! Active ──[Cancel]──> Cancelled ──[Expiry]──> Expired
//!    │                                          │
//!    └──[Renew]──> Active                       │
//! ```
//!
//! ## Trust Model
//!
//! - **Block-based locks**: Subscriptions expire at specific block heights
//! - **DAO governance**: Subscription terms, pricing, and endowment managed by DAO
//! - **Endowment insurance**: DAO can authorize refunds from endowment fund
//! - **Atomic swap**: Cross-chain payments via HTLC pattern

use dwow_sdk::{
    crypto::{pasta_prelude::PrimeField, ContractId},
    error::{ContractError, ContractResult},
    msg,
    pasta::pallas,
    wasm, ContractCall,
};
use dwow_serial::{deserialize, serialize};

use crate::{
    model::{
        CancelParamsV1, CancelUpdateV1, DaoControlAction, DaoControlParamsV1, DaoControlUpdateV1,
        Plan, RenewParamsV1, RenewUpdateV1, SubscribeParamsV1, SubscribeUpdateV1, Subscription,
        SubscriptionState, UpdateUsageParamsV1, UpdateUsageUpdateV1,
    },
    SubscriptionFunction, SUBSCRIPTION_CONTRACT_INFO_TREE,
    SUBSCRIPTION_CONTRACT_NULLIFIERS_TREE, SUBSCRIPTION_CONTRACT_PLANS_TREE,
    SUBSCRIPTION_CONTRACT_SUBSCRIPTIONS_TREE,
};

// ============================================================================
// DATABASE KEYS
// ============================================================================

const SUBSCRIPTION_DB_VERSION_KEY: &[u8] = b"db_version";

dwow_sdk::define_contract!(
    init: init_contract,
    exec: process_instruction,
    apply: process_update,
    metadata: get_metadata
);

// ============================================================================
// INITIALIZATION
// ============================================================================

/// Initialize subscription contract state
///
/// Sets up:
/// - Info tree (version, config)
/// - Subscriptions tree (subscription records)
/// - Nullifiers tree (spent nullifiers)
/// - Plans tree (subscription plan definitions)
pub fn init_contract(cid: ContractId, _ix: &[u8]) -> ContractResult {
    msg!("[subscription::init_contract] Initializing subscription contract");

    // Initialize info tree
    let info_db = wasm::db::db_init(cid, SUBSCRIPTION_CONTRACT_INFO_TREE)?;
    wasm::db::db_set(
        info_db,
        SUBSCRIPTION_DB_VERSION_KEY,
        &env!("CARGO_PKG_VERSION").as_bytes(),
    )?;

    // Initialize subscriptions tree
    wasm::db::db_init(cid, SUBSCRIPTION_CONTRACT_SUBSCRIPTIONS_TREE)?;

    // Initialize nullifiers tree
    wasm::db::db_init(cid, SUBSCRIPTION_CONTRACT_NULLIFIERS_TREE)?;

    // Initialize plans tree
    wasm::db::db_init(cid, SUBSCRIPTION_CONTRACT_PLANS_TREE)?;

    msg!("[subscription::init_contract] Subscription contract initialized successfully");
    Ok(())
}

// ============================================================================
// METADATA (ZK proof verification)
// ============================================================================

/// Fetch metadata for ZK proof verification
fn get_metadata(_cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<dwow_sdk::dark_tree::DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = SubscriptionFunction::try_from(self_.data[0])?;

    msg!("[subscription::get_metadata] Processing function: {:?}", func);

    // TODO: Implement metadata fetching for ZK proof verification
    // This would involve deserializing the call data and returning
    // public inputs needed for proof verification.

    wasm::util::set_return_data(&[])
}

// ============================================================================
// INSTRUCTION PROCESSING (state transition verification)
// ============================================================================

/// Verify state transition and produce update if valid
fn process_instruction(cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<dwow_sdk::dark_tree::DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = SubscriptionFunction::try_from(self_.data[0])?;

    msg!("[subscription::process_instruction] Processing function: {:?}", func);

    match func {
        SubscriptionFunction::SubscribeV1 => {
            let params: SubscribeParamsV1 = deserialize(&self_.data[1..])?;
            subscribe_v1(cid, call_idx, calls, params)
        }
        SubscriptionFunction::CancelV1 => {
            let params: CancelParamsV1 = deserialize(&self_.data[1..])?;
            cancel_v1(cid, params)
        }
        SubscriptionFunction::RenewV1 => {
            let params: RenewParamsV1 = deserialize(&self_.data[1..])?;
            renew_v1(cid, call_idx, calls, params)
        }
        SubscriptionFunction::VerifyAccessV1 => {
            // No state update needed - just verification
            msg!("[subscription::process_instruction] VerifyAccessV1 has no state update");
            wasm::util::set_return_data(&[])
        }
        SubscriptionFunction::UpdateUsageV1 => {
            let params: UpdateUsageParamsV1 = deserialize(&self_.data[1..])?;
            update_usage_v1(cid, params)
        }
        SubscriptionFunction::DaoControlV1 => {
            let params: DaoControlParamsV1 = deserialize(&self_.data[1..])?;
            dao_control_v1(cid, call_idx, calls, params)
        }
        SubscriptionFunction::InitializeV1 => {
            msg!("[subscription::process_instruction] InitializeV1 has no update data");
            wasm::util::set_return_data(&[])
        }
    }
}

// ============================================================================
// STATE UPDATE (write new state)
// ============================================================================

/// Write state update after successful verification
fn process_update(cid: ContractId, update_data: &[u8]) -> ContractResult {
    let func = SubscriptionFunction::try_from(update_data[0])?;

    match func {
        SubscriptionFunction::SubscribeV1 => {
            let update: SubscribeUpdateV1 = deserialize(&update_data[1..])?;
            subscribe_apply_v1(cid, update)
        }
        SubscriptionFunction::CancelV1 => {
            let update: CancelUpdateV1 = deserialize(&update_data[1..])?;
            cancel_apply_v1(cid, update)
        }
        SubscriptionFunction::RenewV1 => {
            let update: RenewUpdateV1 = deserialize(&update_data[1..])?;
            renew_apply_v1(cid, update)
        }
        SubscriptionFunction::VerifyAccessV1 => {
            // No state update needed - just verification
            msg!("[subscription::process_update] VerifyAccessV1 has no state update");
            Ok(())
        }
        SubscriptionFunction::UpdateUsageV1 => {
            let update: UpdateUsageUpdateV1 = deserialize(&update_data[1..])?;
            update_usage_apply_v1(cid, update)
        }
        SubscriptionFunction::DaoControlV1 => {
            let update: DaoControlUpdateV1 = deserialize(&update_data[1..])?;
            dao_control_apply_v1(cid, update)
        }
        SubscriptionFunction::InitializeV1 => {
            msg!("[subscription::process_update] InitializeV1 has no update data");
            Ok(())
        }
    }
}

// ============================================================================
// INSTRUCTION HANDLERS
// ============================================================================

/// SubscribeV1 instruction - create a new subscription
fn subscribe_v1(cid: ContractId, call_idx: usize, calls: Vec<dwow_sdk::dark_tree::DarkLeaf<ContractCall>>, params: SubscribeParamsV1) -> ContractResult {
    msg!("[subscription::subscribe_v1] Creating subscription for plan {}", params.plan_id);

    // Validate children_indexes for payment
    let self_ = &calls[call_idx];
    if self_.children_indexes.len() != 1 {
        msg!("[subscribe_v1] Error: Expected 1 child call (money_v3::transfer_v1), got {}",
             self_.children_indexes.len());
        return Err(ContractError::Custom(30).into())
    }
    let child_idx = self_.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!("[subscribe_v1] Error: Expected money_v3::transfer_v1 (0x04), got 0x{:02x}",
             child_call.data[0]);
        return Err(ContractError::Custom(31).into())
    }

    // Look up the plan to get duration and settings
    let plans_db = wasm::db::db_lookup(cid, SUBSCRIPTION_CONTRACT_PLANS_TREE)?;
    let plan_bytes = wasm::db::db_get(plans_db, &params.plan_id.to_le_bytes())?;
    let plan: Plan = match plan_bytes {
        Some(data) => deserialize(&data)?,
        None => {
            msg!("[subscription::subscribe_v1] ERROR: Plan not found");
            return Err(ContractError::Custom(1).into())
        }
    };

    // Verify plan is active
    if !plan.active {
        msg!("[subscription::subscribe_v1] ERROR: Plan is not active");
        return Err(ContractError::Custom(1).into())
    }

    let current_block = wasm::util::get_verifying_block_height()?;

    // Create subscription
    let subscription = Subscription {
        id: params.commitment,
        subscriber_pubkey: params.subscriber_pubkey,
        plan_id: params.plan_id,
        lock_until_block: current_block as u64 + plan.duration_blocks as u64,
        deposit: plan.price,
        token_id: plan.token_id,
        value_commit: params.value_commit,
        state: SubscriptionState::Active,
        spent_nullifier: pallas::Base::zero(),
        created_at: current_block as u64,
        dao_escrow_bulla: params.dao_escrow_bulla,
        dao_membership_note: params.dao_membership_note,
        uses_allowed: 0,
        rate_period: 0,
        period_uses: 0,
        last_access_block: current_block as u64,
        uses_remaining: 0,
    };

    let update = SubscribeUpdateV1 { subscription };
    msg!("[subscription::subscribe_v1] Subscription created: {:?}", update.subscription.id);
    wasm::util::set_return_data(&serialize(&update))
}

/// SubscribeV1 apply - store subscription
fn subscribe_apply_v1(cid: ContractId, update: SubscribeUpdateV1) -> ContractResult {
    let subs_db = wasm::db::db_lookup(cid, SUBSCRIPTION_CONTRACT_SUBSCRIPTIONS_TREE)?;
    let nullifiers_db = wasm::db::db_lookup(cid, SUBSCRIPTION_CONTRACT_NULLIFIERS_TREE)?;

    // Write subscription to subscriptions tree
    wasm::db::db_set(
        subs_db,
        &update.subscription.id.to_repr(),
        &serialize(&update.subscription),
    )?;

    // Record nullifier placeholder (not spent yet - tracks subscription existence)
    wasm::db::db_set(nullifiers_db, &update.subscription.id.to_repr(), &[])?;

    msg!("[subscription::subscribe_apply_v1] Subscription stored: {:?}", update.subscription.id);
    Ok(())
}

/// CancelV1 instruction - cancel an existing subscription
fn cancel_v1(cid: ContractId, params: CancelParamsV1) -> ContractResult {
    msg!("[subscription::cancel_v1] Cancelling subscription {:?}", params.subscription_id);

    // Look up the subscription
    let subs_db = wasm::db::db_lookup(cid, SUBSCRIPTION_CONTRACT_SUBSCRIPTIONS_TREE)?;
    let sub_bytes = wasm::db::db_get(subs_db, &params.subscription_id.to_repr())?;
    let mut subscription: Subscription = match sub_bytes {
        Some(data) => deserialize(&data)?,
        None => {
            msg!("[subscription::cancel_v1] ERROR: Subscription not found");
            return Err(ContractError::Custom(1).into())
        }
    };

    // Verify subscription is active
    if subscription.state != SubscriptionState::Active {
        msg!("[subscription::cancel_v1] ERROR: Subscription not active");
        return Err(ContractError::Custom(3).into())
    }

    // Compute the nullifier to verify ownership
    let expected_nullifier = subscription.compute_nullifier(params.subscriber_secret);
    if expected_nullifier != params.spent_nullifier {
        msg!("[subscription::cancel_v1] ERROR: Invalid nullifier");
        return Err(ContractError::Custom(4).into())
    }

    // Update subscription state to Cancelled
    subscription.state = SubscriptionState::Cancelled;

    let update = CancelUpdateV1 {
        subscription_id: params.subscription_id,
        spent_nullifier: params.spent_nullifier,
        updated_subscription: subscription,
    };

    msg!("[subscription::cancel_v1] Cancellation prepared for: {:?}", params.subscription_id);
    wasm::util::set_return_data(&serialize(&update))
}

/// CancelV1 apply - mark subscription cancelled and record nullifier
fn cancel_apply_v1(cid: ContractId, update: CancelUpdateV1) -> ContractResult {
    let subs_db = wasm::db::db_lookup(cid, SUBSCRIPTION_CONTRACT_SUBSCRIPTIONS_TREE)?;
    let nullifiers_db = wasm::db::db_lookup(cid, SUBSCRIPTION_CONTRACT_NULLIFIERS_TREE)?;

    // Write updated subscription with Cancelled state
    wasm::db::db_set(
        subs_db,
        &update.subscription_id.to_repr(),
        &serialize(&update.updated_subscription),
    )?;

    // Record the nullifier as spent
    wasm::db::db_set(nullifiers_db, &update.spent_nullifier.to_repr(), &[])?;

    msg!("[subscription::cancel_apply_v1] Subscription cancelled: {:?}", update.subscription_id);
    Ok(())
}

/// RenewV1 instruction - renew an existing subscription
fn renew_v1(cid: ContractId, call_idx: usize, calls: Vec<dwow_sdk::dark_tree::DarkLeaf<ContractCall>>, params: RenewParamsV1) -> ContractResult {
    msg!("[subscription::renew_v1] Renewing subscription {:?}", params.subscription_id);

    // Validate children_indexes for payment
    let self_ = &calls[call_idx];
    if self_.children_indexes.len() != 1 {
        msg!("[renew_v1] Error: Expected 1 child call (money_v3::transfer_v1), got {}",
             self_.children_indexes.len());
        return Err(ContractError::Custom(30).into())
    }
    let child_idx = self_.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!("[renew_v1] Error: Expected money_v3::transfer_v1 (0x04), got 0x{:02x}",
             child_call.data[0]);
        return Err(ContractError::Custom(31).into())
    }

    // Look up the existing subscription
    let subs_db = wasm::db::db_lookup(cid, SUBSCRIPTION_CONTRACT_SUBSCRIPTIONS_TREE)?;
    let sub_bytes = wasm::db::db_get(subs_db, &params.subscription_id.to_repr())?;
    let old_subscription: Subscription = match sub_bytes {
        Some(data) => deserialize(&data)?,
        None => {
            msg!("[subscription::renew_v1] ERROR: Subscription not found");
            return Err(ContractError::Custom(1).into())
        }
    };

    // Verify subscription is active
    if old_subscription.state != SubscriptionState::Active {
        msg!("[subscription::renew_v1] ERROR: Subscription not active");
        return Err(ContractError::Custom(3).into())
    }

    // Look up the plan to get duration
    let plans_db = wasm::db::db_lookup(cid, SUBSCRIPTION_CONTRACT_PLANS_TREE)?;
    let plan_bytes = wasm::db::db_get(plans_db, &old_subscription.plan_id.to_le_bytes())?;
    let _plan: Plan = match plan_bytes {
        Some(data) => deserialize(&data)?,
        None => {
            msg!("[subscription::renew_v1] ERROR: Plan not found");
            return Err(ContractError::Custom(1).into())
        }
    };

    let current_block = wasm::util::get_verifying_block_height()?;

    // Create new subscription with renewed lock_until_block
    let new_subscription = Subscription {
        id: params.subscription_id, // Same ID for continuity
        subscriber_pubkey: old_subscription.subscriber_pubkey,
        plan_id: old_subscription.plan_id,
        lock_until_block: params.new_lock_until_block,
        deposit: old_subscription.deposit,
        token_id: old_subscription.token_id,
        value_commit: params.value_commit,
        state: SubscriptionState::Active,
        spent_nullifier: pallas::Base::zero(),
        created_at: current_block as u64,
        dao_escrow_bulla: old_subscription.dao_escrow_bulla,
        dao_membership_note: old_subscription.dao_membership_note,
        uses_allowed: old_subscription.uses_allowed,
        rate_period: old_subscription.rate_period,
        period_uses: old_subscription.period_uses,
        last_access_block: current_block as u64,
        uses_remaining: old_subscription.uses_remaining,
    };

    let update = RenewUpdateV1 {
        subscription_id: params.subscription_id,
        spent_nullifier: params.spent_nullifier,
        new_subscription,
    };

    msg!("[subscription::renew_v1] Renewal prepared for: {:?}", params.subscription_id);
    wasm::util::set_return_data(&serialize(&update))
}

/// RenewV1 apply - nullify old, create new subscription
fn renew_apply_v1(cid: ContractId, update: RenewUpdateV1) -> ContractResult {
    let subs_db = wasm::db::db_lookup(cid, SUBSCRIPTION_CONTRACT_SUBSCRIPTIONS_TREE)?;
    let nullifiers_db = wasm::db::db_lookup(cid, SUBSCRIPTION_CONTRACT_NULLIFIERS_TREE)?;

    // Write new subscription
    wasm::db::db_set(
        subs_db,
        &update.subscription_id.to_repr(),
        &serialize(&update.new_subscription),
    )?;

    // Record old nullifier as spent
    wasm::db::db_set(nullifiers_db, &update.spent_nullifier.to_repr(), &[])?;

    msg!("[subscription::renew_apply_v1] Subscription renewed: {:?}", update.subscription_id);
    Ok(())
}

/// UpdateUsageV1 instruction - record usage of a subscription
fn update_usage_v1(cid: ContractId, params: UpdateUsageParamsV1) -> ContractResult {
    msg!("[subscription::update_usage_v1] Updating usage for: {:?}", params.subscription_id);

    // Look up the subscription
    let subs_db = wasm::db::db_lookup(cid, SUBSCRIPTION_CONTRACT_SUBSCRIPTIONS_TREE)?;
    let sub_bytes = wasm::db::db_get(subs_db, &params.subscription_id.to_repr())?;
    let subscription: Subscription = match sub_bytes {
        Some(data) => deserialize(&data)?,
        None => {
            msg!("[subscription::update_usage_v1] ERROR: Subscription not found");
            return Err(ContractError::Custom(1).into())
        }
    };

    // Compute the nullifier to verify ownership
    let expected_nullifier = subscription.compute_nullifier(params.subscriber_secret);
    if expected_nullifier != params.spent_nullifier {
        msg!("[subscription::update_usage_v1] ERROR: Invalid nullifier");
        return Err(ContractError::Custom(4).into())
    }

    // Check if this is a new period
    let blocks_since_last = params.current_block.saturating_sub(subscription.last_access_block);
    let is_new_period = blocks_since_last >= subscription.rate_period;

    // Calculate new usage values
    let (period_uses, uses_remaining, last_access_block) = if is_new_period {
        // Reset period counters
        (1u64, subscription.uses_allowed.saturating_sub(1), params.current_block)
    } else {
        // Increment within period
        (
            subscription.period_uses + 1,
            subscription.uses_remaining.saturating_sub(1),
            params.current_block,
        )
    };

    let update = UpdateUsageUpdateV1 {
        subscription_id: params.subscription_id,
        period_uses,
        last_access_block,
        uses_remaining,
        is_new_period,
    };

    msg!("[subscription::update_usage_v1] Usage update prepared for: {:?}", params.subscription_id);
    wasm::util::set_return_data(&serialize(&update))
}

/// UpdateUsageV1 apply - write updated usage to subscription
fn update_usage_apply_v1(cid: ContractId, update: UpdateUsageUpdateV1) -> ContractResult {
    let subs_db = wasm::db::db_lookup(cid, SUBSCRIPTION_CONTRACT_SUBSCRIPTIONS_TREE)?;

    // Get current subscription
    let sub_bytes = wasm::db::db_get(subs_db, &update.subscription_id.to_repr())?;
    let mut subscription: Subscription = match sub_bytes {
        Some(data) => deserialize(&data)?,
        None => {
            msg!("[subscription::update_usage_apply_v1] ERROR: Subscription not found");
            return Err(ContractError::Custom(1).into())
        }
    };

    // Update usage fields
    subscription.period_uses = update.period_uses;
    subscription.last_access_block = update.last_access_block;
    subscription.uses_remaining = update.uses_remaining;

    wasm::db::db_set(
        subs_db,
        &update.subscription_id.to_repr(),
        &serialize(&subscription),
    )?;

    msg!("[subscription::update_usage_apply_v1] Usage updated for: {:?}", update.subscription_id);
    Ok(())
}

/// DaoControlV1 instruction - execute DAO governance action
///
/// Money Integration: When executing `EndowmentWithdraw`, this function REQUIRES
/// a money_v3::transfer_v1 child call to be bundled to transfer the endowment funds.
fn dao_control_v1(
    _cid: ContractId,
    call_idx: usize,
    calls: Vec<dwow_sdk::dark_tree::DarkLeaf<ContractCall>>,
    params: DaoControlParamsV1,
) -> ContractResult {
    msg!("[subscription::dao_control_v1] Executing DAO control action");

    // Validate children_indexes for EndowmentWithdraw
    if let DaoControlParamsV1::EndowmentWithdraw { amount, recipient } = params {
        // Validate children_indexes to ensure money_v3::transfer_v1 is bundled
        let self_ = &calls[call_idx];
        if self_.children_indexes.len() != 1 {
            msg!(
                "[DaoControlV1] Error: EndowmentWithdraw requires 1 child call (money_v3::transfer_v1), got {}",
                self_.children_indexes.len()
            );
            return Err(ContractError::Custom(1).into())
        }

        // Verify child call is money_v3::transfer_v1 (function code 0x04)
        let child_idx = self_.children_indexes[0];
        let child_call = &calls[child_idx].data;
        if child_call.data[0] != 0x04 {
            msg!(
                "[DaoControlV1] Error: Expected money_v3::transfer_v1 (0x04), got 0x{:02x}",
                child_call.data[0]
            );
            return Err(ContractError::Custom(2).into())
        }

        msg!(
            "[DaoControlV1] EndowmentWithdraw validated: {} to {:?}",
            amount,
            recipient
        );
    }

    // Convert params to action for update
    let action = match params {
        DaoControlParamsV1::UpdatePlan(plan) => DaoControlAction::PlanUpdated(plan.id),
        DaoControlParamsV1::SetPlanActive { plan_id, active } => {
            DaoControlAction::PlanStatusChanged { plan_id, active }
        }
        DaoControlParamsV1::EmergencyPause { pause, reason: _ } => {
            DaoControlAction::EmergencyPauseToggled(pause)
        }
        DaoControlParamsV1::EndowmentWithdraw { amount, recipient } => {
            DaoControlAction::EndowmentWithdrawn { amount, recipient }
        }
        DaoControlParamsV1::Slash { subscription_id, reason: _ } => {
            DaoControlAction::SubscriptionSlashed(subscription_id)
        }
    };

    let update = DaoControlUpdateV1 { action };

    msg!("[subscription::dao_control_v1] DAO control action prepared");
    wasm::util::set_return_data(&serialize(&update))
}

/// DaoControlV1 apply - execute DAO governance action
fn dao_control_apply_v1(cid: ContractId, update: DaoControlUpdateV1) -> ContractResult {
    match update.action {
        DaoControlAction::PlanUpdated(plan_id) => {
            // The plan was already updated during instruction, nothing to do here
            msg!("[subscription::dao_control_apply_v1] Plan updated: {}", plan_id);
        }
        DaoControlAction::PlanStatusChanged { plan_id, active } => {
            let plans_db = wasm::db::db_lookup(cid, SUBSCRIPTION_CONTRACT_PLANS_TREE)?;
            let plan_bytes = wasm::db::db_get(plans_db, &plan_id.to_le_bytes())?;
            let mut plan: Plan = match plan_bytes {
                Some(data) => deserialize(&data)?,
                None => {
                    msg!("[subscription::dao_control_apply_v1] ERROR: Plan not found");
                    return Err(ContractError::Custom(1).into())
                }
            };
            plan.active = active;
            wasm::db::db_set(plans_db, &plan_id.to_le_bytes(), &serialize(&plan))?;
            msg!("[subscription::dao_control_apply_v1] Plan {} active status: {}", plan_id, active);
        }
        DaoControlAction::EmergencyPauseToggled(pause) => {
            // In a full implementation, this would set a global pause flag
            msg!("[subscription::dao_control_apply_v1] Emergency pause: {}", pause);
        }
        DaoControlAction::EndowmentWithdrawn { amount: _, recipient: _ } => {
            // In a full implementation, this would transfer funds from endowment
            msg!("[subscription::dao_control_apply_v1] Endowment withdraw executed");
        }
        DaoControlAction::SubscriptionSlashed(subscription_id) => {
            // Mark subscription as slashed/cancelled
            let subs_db = wasm::db::db_lookup(cid, SUBSCRIPTION_CONTRACT_SUBSCRIPTIONS_TREE)?;
            let sub_bytes = wasm::db::db_get(subs_db, &subscription_id.to_repr())?;
            let mut subscription: Subscription = match sub_bytes {
                Some(data) => deserialize(&data)?,
                None => {
                    msg!("[subscription::dao_control_apply_v1] ERROR: Subscription not found");
                    return Err(ContractError::Custom(1).into())
                }
            };
            subscription.state = SubscriptionState::Cancelled;
            wasm::db::db_set(subs_db, &subscription_id.to_repr(), &serialize(&subscription))?;
            msg!("[subscription::dao_control_apply_v1] Subscription slashed: {:?}", subscription_id);
        }
    }

    Ok(())
}