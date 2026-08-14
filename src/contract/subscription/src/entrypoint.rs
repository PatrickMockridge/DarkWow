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
    crypto::{poseidon_hash, pasta_prelude::PrimeField, ContractId},
    error::{ContractError, ContractResult},
    msg,
    pasta::pallas,
    wasm, ContractCall,
};
use dwow_promissory_note_contract::validation::{
    validate_child_contract_id, validate_child_value_commit,
};
use dwow_serial::deserialize;
use dwow_serial::Encodable;

use crate::{
    model::{
        CancelParamsV1, CancelUpdateV1, DaoControlAction, DaoControlParamsV1, DaoControlUpdateV1,
        Plan, RenewParamsV1, RenewUpdateV1, SubscribeParamsV1, SubscribeUpdateV1, Subscription,
        SubscriptionState, UpdateUsageParamsV1, UpdateUsageUpdateV1,
    },
    SubscriptionFunction, SUBSCRIPTION_CONTRACT_INFO_TREE,
    SUBSCRIPTION_CONTRACT_PROMISSORY_NOTE_CONTRACT_ID,
    SUBSCRIPTION_CONTRACT_NULLIFIERS_TREE, SUBSCRIPTION_CONTRACT_PLANS_TREE,
    SUBSCRIPTION_CONTRACT_SUBSCRIPTIONS_TREE,
    SUBSCRIPTION_CONTRACT_ZKAS_SUBSCRIBE_NS_V2, SUBSCRIPTION_CONTRACT_ZKAS_VERIFY_NS_V2,
    SUBSCRIPTION_CONTRACT_ZKAS_UPDATE_NS_V2, SUBSCRIPTION_CONTRACT_ZKAS_CANCEL_NS_V2,
    SUBSCRIPTION_CONTRACT_ZKAS_RENEW_NS_V2,
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
    wasm::db::db_set(info_db, SUBSCRIPTION_CONTRACT_PROMISSORY_NOTE_CONTRACT_ID, &dwow_sdk::crypto::PROMISSORY_NOTE_CONTRACT_ID.to_bytes())?;

    // Initialize subscriptions tree
    wasm::db::db_init(cid, SUBSCRIPTION_CONTRACT_SUBSCRIPTIONS_TREE)?;

    // Initialize nullifiers tree
    wasm::db::db_init(cid, SUBSCRIPTION_CONTRACT_NULLIFIERS_TREE)?;

    // Initialize plans tree
    wasm::db::db_init(cid, SUBSCRIPTION_CONTRACT_PLANS_TREE)?;

    msg!("[subscription::init_contract] Subscription contract initialized successfully");

    wasm::db::zkas_db_set(include_bytes!("../proof/subscribe.zk.bin"))?;
    wasm::db::zkas_db_set(include_bytes!("../proof/update_usage.zk.bin"))?;
    wasm::db::zkas_db_set(include_bytes!("../proof/verify_access.zk.bin"))?;
    wasm::db::zkas_db_set(include_bytes!("../proof/cancel.zk.bin"))?;
    wasm::db::zkas_db_set(include_bytes!("../proof/renew.zk.bin"))?;

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

    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];

    match func {
        SubscriptionFunction::SubscribeV1 => {
            // SubscribeV2 circuit: [tx_binding, tx_nonce]
            zk_public_inputs.push((
                SUBSCRIPTION_CONTRACT_ZKAS_SUBSCRIBE_NS_V2.to_string(),
                vec![pallas::Base::zero(), pallas::Base::zero()],
            ));
        }
        SubscriptionFunction::VerifyAccessV1 => {
            // VerifyAccessV2 circuit: [tx_binding, tx_nonce]
            zk_public_inputs.push((
                SUBSCRIPTION_CONTRACT_ZKAS_VERIFY_NS_V2.to_string(),
                vec![pallas::Base::zero(), pallas::Base::zero()],
            ));
        }
        SubscriptionFunction::UpdateUsageV1 => {
            let params= UpdateUsageParamsV1::decode(&self_.data[1..])?;
            // derived_id = poseidon_hash(DOMAIN_COIN_COMMIT, subscription_id, pub_x, pub_y, block, nonce)
            let derived_id = poseidon_hash([
                pallas::Base::from(4u64), // DOMAIN_COIN_COMMIT
                params.subscription_id.inner(),
                params.subscriber_pub_x,
                params.subscriber_pub_y,
                pallas::Base::from(params.current_block),
                params.nonce,
            ]);
            // Circuit constrain_instance order: [tx_binding, tx_nonce, derived_id]
            zk_public_inputs.push((
                SUBSCRIPTION_CONTRACT_ZKAS_UPDATE_NS_V2.to_string(),
                vec![pallas::Base::zero(), pallas::Base::zero(), derived_id],
            ));
        }
        SubscriptionFunction::CancelV1 => {
            // CancelV2 circuit: [subscription_id, spent_nullifier, tx_binding, tx_nonce]
            zk_public_inputs.push((
                SUBSCRIPTION_CONTRACT_ZKAS_CANCEL_NS_V2.to_string(),
                vec![pallas::Base::zero(), pallas::Base::zero(), pallas::Base::zero(), pallas::Base::zero()],
            ));
        }
        SubscriptionFunction::RenewV1 => {
            // RenewV2 circuit: [subscription_id, spent_nullifier, tx_binding, tx_nonce]
            zk_public_inputs.push((
                SUBSCRIPTION_CONTRACT_ZKAS_RENEW_NS_V2.to_string(),
                vec![pallas::Base::zero(), pallas::Base::zero(), pallas::Base::zero(), pallas::Base::zero()],
            ));
        }
        SubscriptionFunction::DaoControlV1 | SubscriptionFunction::InitializeV1 => {}
        _ => {}
    }

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    wasm::util::set_return_data(&metadata)
}

// ============================================================================
// INSTRUCTION PROCESSING (state transition verification)
// ============================================================================

/// Verify state transition and produce update if valid
fn process_instruction(cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<dwow_sdk::dark_tree::DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func_byte = self_.data[0];
    let func = SubscriptionFunction::try_from(func_byte)?;

    msg!("[subscription::process_instruction] Processing function: {:?}", func);

    let update_bytes = match func {
        SubscriptionFunction::SubscribeV1 => {
            let params= SubscribeParamsV1::decode(&self_.data[1..])?;
            subscribe_v1(cid, call_idx, calls, params)?
        }
        SubscriptionFunction::CancelV1 => {
            let params= CancelParamsV1::decode(&self_.data[1..])?;
            cancel_v1(cid, params)?
        }
        SubscriptionFunction::RenewV1 => {
            let params= RenewParamsV1::decode(&self_.data[1..])?;
            renew_v1(cid, call_idx, calls, params)?
        }
        SubscriptionFunction::VerifyAccessV1 => {
            // No state update needed - just verification
            msg!("[subscription::process_instruction] VerifyAccessV1 has no state update");
            vec![]
        }
        SubscriptionFunction::UpdateUsageV1 => {
            let params= UpdateUsageParamsV1::decode(&self_.data[1..])?;
            update_usage_v1(cid, params)?
        }
        SubscriptionFunction::DaoControlV1 => {
            let params = DaoControlParamsV1::decode(&self_.data[1..])?;
            dao_control_v1(cid, call_idx, calls, params)?
        }
        SubscriptionFunction::InitializeV1 => {
            msg!("[subscription::process_instruction] InitializeV1 has no update data");
            vec![]
        }
    };

    wasm::util::set_return_data(&[&[func_byte], &update_bytes[..]].concat())
}

// ============================================================================
// STATE UPDATE (write new state)
// ============================================================================

/// Write state update after successful verification
fn process_update(cid: ContractId, update_data: &[u8]) -> ContractResult {
    let func = SubscriptionFunction::try_from(update_data[0])?;

    match func {
        SubscriptionFunction::SubscribeV1 => {
            let update: SubscribeUpdateV1 = SubscribeUpdateV1::decode(&update_data[1..])?;
            subscribe_apply_v1(cid, update)
        }
        SubscriptionFunction::CancelV1 => {
            let update: CancelUpdateV1 = CancelUpdateV1::decode(&update_data[1..])?;
            cancel_apply_v1(cid, update)
        }
        SubscriptionFunction::RenewV1 => {
            let update: RenewUpdateV1 = RenewUpdateV1::decode(&update_data[1..])?;
            renew_apply_v1(cid, update)
        }
        SubscriptionFunction::VerifyAccessV1 => {
            // No state update needed - just verification
            msg!("[subscription::process_update] VerifyAccessV1 has no state update");
            Ok(())
        }
        SubscriptionFunction::UpdateUsageV1 => {
            let update: UpdateUsageUpdateV1 = UpdateUsageUpdateV1::decode(&update_data[1..])?;
            update_usage_apply_v1(cid, update)
        }
        SubscriptionFunction::DaoControlV1 => {
            let update: DaoControlUpdateV1 = DaoControlUpdateV1::decode(&update_data[1..])?;
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
fn subscribe_v1(cid: ContractId, call_idx: usize, calls: Vec<dwow_sdk::dark_tree::DarkLeaf<ContractCall>>, params: SubscribeParamsV1) -> Result<Vec<u8>, ContractError> {
    msg!("[subscription::subscribe_v1] Creating subscription for plan {}", params.plan_id);

    // Validate children_indexes for payment
    let self_ = &calls[call_idx];
    if self_.children_indexes.len() != 1 {
        msg!("[subscribe_v1] Error: Expected 1 child call (promissory_note::transfer_v1), got {}",
             self_.children_indexes.len());
        return Err(ContractError::Custom(30).into())
    }
    let child_idx = self_.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!("[subscribe_v1] Error: Expected promissory_note::transfer_v1 (0x04), got 0x{:02x}",
             child_call.data[0]);
        return Err(ContractError::Custom(31).into())
    }

    // Validate child call targets promissory_note (prevent cross-contract routing)
    let info_db = wasm::db::db_lookup(cid, SUBSCRIPTION_CONTRACT_INFO_TREE)?;
    let promissory_note_bytes = wasm::db::db_get(info_db, SUBSCRIPTION_CONTRACT_PROMISSORY_NOTE_CONTRACT_ID)?
        .ok_or(ContractError::Custom(31))?;
    let promissory_note_cid: ContractId = ContractId::from_bytes(
        promissory_note_bytes.try_into().map_err(|_| {
            ContractError::IoError("subscribe_v1: invalid ContractId bytes".into())
        })?,
    )?;
    // HAZOP H-11: fail-closed — reject if promissory_note not configured
    if promissory_note_cid == ContractId::ZERO {
        return Err(ContractError::IoError("promissory_note contract ID not configured".into()));
    }
    validate_child_contract_id(&child_call.contract_id, &promissory_note_cid)?;

    // Look up the plan to get duration and settings
    let plans_db = wasm::db::db_lookup(cid, SUBSCRIPTION_CONTRACT_PLANS_TREE)?;
    let plan_bytes = wasm::db::db_get(plans_db, &params.plan_id.to_le_bytes())?;
    let plan: Plan = match plan_bytes {
        Some(data) => Plan::decode(&data)?,
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

    // Validate child transfer amount using value_commit comparison
    let value_blind = poseidon_hash([
        pallas::Base::from(plan.price),
        params.commitment.inner(),
    ]);
    validate_child_value_commit(&child_call.data, plan.price, value_blind)?;

    let current_block = wasm::util::get_verifying_block_height()?.get();

    // Create subscription
    let subscription = Subscription {
        version: 1,
        id: params.commitment,
        subscriber_pubkey: params.subscriber_pubkey,
        plan_id: params.plan_id,
        lock_until_block: current_block + plan.duration_blocks as u64,
        deposit: plan.price,
        token_id: plan.token_id,
        value_commit: params.value_commit,
        state: SubscriptionState::Active,
        spent_nullifier: pallas::Base::zero(),
        created_at: current_block,
        dao_escrow_bulla: params.dao_escrow_bulla,
        dao_membership_note: params.dao_membership_note,
        uses_allowed: 0,
        rate_period: 0,
        period_uses: 0,
        last_access_block: current_block,
        uses_remaining: 0,
        instance_seed: params.instance_seed,
    };

    let update = SubscribeUpdateV1 { subscription };
    msg!("[subscription::subscribe_v1] Subscription created: {:?}", update.subscription.id);
    Ok(update.encode())
}

/// SubscribeV1 apply - store subscription
fn subscribe_apply_v1(cid: ContractId, update: SubscribeUpdateV1) -> ContractResult {
    let subs_db = wasm::db::db_lookup(cid, SUBSCRIPTION_CONTRACT_SUBSCRIPTIONS_TREE)?;
    let nullifiers_db = wasm::db::db_lookup(cid, SUBSCRIPTION_CONTRACT_NULLIFIERS_TREE)?;

    // Write subscription to subscriptions tree
    let sub_data = update.subscription.encode();
    wasm::db::db_set(
        subs_db,
        &update.subscription.id.to_bytes(),
        &sub_data,
    )?;

    // Record nullifier placeholder (not spent yet - tracks subscription existence)
    wasm::db::db_mark_spent(nullifiers_db, &update.subscription.id.to_bytes())?;

    msg!("[subscription::subscribe_apply_v1] Subscription stored: {:?}", update.subscription.id);
    Ok(())
}

/// CancelV1 instruction - cancel an existing subscription
fn cancel_v1(cid: ContractId, params: CancelParamsV1) -> Result<Vec<u8>, ContractError> {
    msg!("[subscription::cancel_v1] Cancelling subscription {:?}", params.subscription_id);

    // Look up the subscription
    let subs_db = wasm::db::db_lookup(cid, SUBSCRIPTION_CONTRACT_SUBSCRIPTIONS_TREE)?;
    let sub_bytes = wasm::db::db_get(subs_db, &params.subscription_id.to_bytes())?;
    let mut subscription: Subscription = match sub_bytes {
        Some(data) => Subscription::decode(&data)?,
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

    // Check nullifier hasn't been used (replay protection)
    let nullifiers_db = wasm::db::db_lookup(cid, SUBSCRIPTION_CONTRACT_NULLIFIERS_TREE)?;
    if wasm::db::db_contains_key(nullifiers_db, &params.spent_nullifier.to_repr())? {
        msg!("[subscription::cancel_v1] ERROR: Nullifier already spent");
        return Err(ContractError::Custom(7).into())
    }

    // Update subscription state to Cancelled
    subscription.state = SubscriptionState::Cancelled;

    let update = CancelUpdateV1 {
        subscription_id: params.subscription_id,
        spent_nullifier: params.spent_nullifier,
        updated_subscription: subscription,
    };

    msg!("[subscription::cancel_v1] Cancellation prepared for: {:?}", params.subscription_id);
    Ok(update.encode())
}

/// CancelV1 apply - mark subscription cancelled and record nullifier
fn cancel_apply_v1(cid: ContractId, update: CancelUpdateV1) -> ContractResult {
    let subs_db = wasm::db::db_lookup(cid, SUBSCRIPTION_CONTRACT_SUBSCRIPTIONS_TREE)?;
    let nullifiers_db = wasm::db::db_lookup(cid, SUBSCRIPTION_CONTRACT_NULLIFIERS_TREE)?;

    // Write updated subscription with Cancelled state
    let sub_data = update.updated_subscription.encode();
    wasm::db::db_set(
        subs_db,
        &update.subscription_id.to_bytes(),
        &sub_data,
    )?;

    // Record the nullifier as spent
    wasm::db::db_mark_spent(nullifiers_db, &update.spent_nullifier.to_repr())?;

    msg!("[subscription::cancel_apply_v1] Subscription cancelled: {:?}", update.subscription_id);
    Ok(())
}

/// RenewV1 instruction - renew an existing subscription
fn renew_v1(cid: ContractId, call_idx: usize, calls: Vec<dwow_sdk::dark_tree::DarkLeaf<ContractCall>>, params: RenewParamsV1) -> Result<Vec<u8>, ContractError> {
    msg!("[subscription::renew_v1] Renewing subscription {:?}", params.subscription_id);

    // Validate children_indexes for payment
    let self_ = &calls[call_idx];
    if self_.children_indexes.len() != 1 {
        msg!("[renew_v1] Error: Expected 1 child call (promissory_note::transfer_v1), got {}",
             self_.children_indexes.len());
        return Err(ContractError::Custom(30).into())
    }
    let child_idx = self_.children_indexes[0];
    let child_call = &calls[child_idx].data;
    if child_call.data[0] != 0x04 {
        msg!("[renew_v1] Error: Expected promissory_note::transfer_v1 (0x04), got 0x{:02x}",
             child_call.data[0]);
        return Err(ContractError::Custom(31).into())
    }

    // Validate child call targets promissory_note (prevent cross-contract routing)
    let info_db = wasm::db::db_lookup(cid, SUBSCRIPTION_CONTRACT_INFO_TREE)?;
    let promissory_note_bytes = wasm::db::db_get(info_db, SUBSCRIPTION_CONTRACT_PROMISSORY_NOTE_CONTRACT_ID)?
        .ok_or(ContractError::Custom(31))?;
    let promissory_note_cid: ContractId = ContractId::from_bytes(
        promissory_note_bytes.try_into().map_err(|_| {
            ContractError::IoError("renew_v1: invalid ContractId bytes".into())
        })?,
    )?;
    // HAZOP H-11: fail-closed — reject if promissory_note not configured
    if promissory_note_cid == ContractId::ZERO {
        return Err(ContractError::IoError("promissory_note contract ID not configured".into()));
    }
    validate_child_contract_id(&child_call.contract_id, &promissory_note_cid)?;

    // Look up the existing subscription
    let subs_db = wasm::db::db_lookup(cid, SUBSCRIPTION_CONTRACT_SUBSCRIPTIONS_TREE)?;
    let sub_bytes = wasm::db::db_get(subs_db, &params.subscription_id.to_bytes())?;
    let old_subscription: Subscription = match sub_bytes {
        Some(data) => Subscription::decode(&data)?,
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

    // Check nullifier hasn't been used (replay protection)
    let nullifiers_db = wasm::db::db_lookup(cid, SUBSCRIPTION_CONTRACT_NULLIFIERS_TREE)?;
    if wasm::db::db_contains_key(nullifiers_db, &params.spent_nullifier.to_repr())? {
        msg!("[subscription::renew_v1] ERROR: Nullifier already spent");
        return Err(ContractError::Custom(7).into())
    }

    // Look up the plan to get duration
    let plans_db = wasm::db::db_lookup(cid, SUBSCRIPTION_CONTRACT_PLANS_TREE)?;
    let plan_bytes = wasm::db::db_get(plans_db, &old_subscription.plan_id.to_le_bytes())?;
    let plan: Plan = match plan_bytes {
        Some(data) => Plan::decode(&data)?,
        None => {
            msg!("[subscription::renew_v1] ERROR: Plan not found");
            return Err(ContractError::Custom(1).into())
        }
    };

    // Validate child transfer amount using value_commit comparison
    let value_blind = poseidon_hash([
        pallas::Base::from(plan.price),
        old_subscription.id.inner(),
    ]);
    validate_child_value_commit(&child_call.data, plan.price, value_blind)?;

    let current_block = wasm::util::get_verifying_block_height()?.get();

    // Create new subscription with renewed lock_until_block
    let new_subscription = Subscription {
        version: 1,
        id: params.subscription_id, // Same ID for continuity
        subscriber_pubkey: old_subscription.subscriber_pubkey,
        plan_id: old_subscription.plan_id,
        lock_until_block: params.new_lock_until_block,
        deposit: old_subscription.deposit,
        token_id: old_subscription.token_id,
        value_commit: params.value_commit,
        state: SubscriptionState::Active,
        spent_nullifier: pallas::Base::zero(),
        created_at: current_block,
        dao_escrow_bulla: old_subscription.dao_escrow_bulla,
        dao_membership_note: old_subscription.dao_membership_note,
        uses_allowed: old_subscription.uses_allowed,
        rate_period: old_subscription.rate_period,
        period_uses: old_subscription.period_uses,
        last_access_block: current_block,
        uses_remaining: old_subscription.uses_remaining,
        instance_seed: old_subscription.instance_seed,
    };

    let update = RenewUpdateV1 {
        subscription_id: params.subscription_id,
        spent_nullifier: params.spent_nullifier,
        new_subscription,
    };

    msg!("[subscription::renew_v1] Renewal prepared for: {:?}", params.subscription_id);
    Ok(update.encode())
}

/// RenewV1 apply - nullify old, create new subscription
fn renew_apply_v1(cid: ContractId, update: RenewUpdateV1) -> ContractResult {
    let subs_db = wasm::db::db_lookup(cid, SUBSCRIPTION_CONTRACT_SUBSCRIPTIONS_TREE)?;
    let nullifiers_db = wasm::db::db_lookup(cid, SUBSCRIPTION_CONTRACT_NULLIFIERS_TREE)?;

    // Write new subscription
    let sub_data = update.new_subscription.encode();
    wasm::db::db_set(
        subs_db,
        &update.subscription_id.to_bytes(),
        &sub_data,
    )?;

    // Record old nullifier as spent
    wasm::db::db_mark_spent(nullifiers_db, &update.spent_nullifier.to_repr())?;

    msg!("[subscription::renew_apply_v1] Subscription renewed: {:?}", update.subscription_id);
    Ok(())
}

/// UpdateUsageV1 instruction - record usage of a subscription
fn update_usage_v1(cid: ContractId, params: UpdateUsageParamsV1) -> Result<Vec<u8>, ContractError> {
    msg!("[subscription::update_usage_v1] Updating usage for: {:?}", params.subscription_id);

    // Look up the subscription
    let subs_db = wasm::db::db_lookup(cid, SUBSCRIPTION_CONTRACT_SUBSCRIPTIONS_TREE)?;
    let sub_bytes = wasm::db::db_get(subs_db, &params.subscription_id.to_bytes())?;
    let subscription: Subscription = match sub_bytes {
        Some(data) => Subscription::decode(&data)?,
        None => {
            msg!("[subscription::update_usage_v1] ERROR: Subscription not found");
            return Err(ContractError::Custom(1).into())
        }
    };

    // Verify subscription hasn't expired (lock_until_block check)
    let current_block = wasm::util::get_verifying_block_height()?.get();
    if current_block >= subscription.lock_until_block {
        msg!("[subscription::update_usage_v1] ERROR: Subscription expired at block {}", subscription.lock_until_block);
        return Err(ContractError::Custom(6).into())
    }

    // Compute the nullifier to verify ownership
    let expected_nullifier = subscription.compute_nullifier(params.subscriber_secret);
    if expected_nullifier != params.spent_nullifier {
        msg!("[subscription::update_usage_v1] ERROR: Invalid nullifier");
        return Err(ContractError::Custom(4).into())
    }

    // Check if this is a new period
    let blocks_since_last = params.current_block.saturating_sub(subscription.last_access_block);
    let is_new_period = blocks_since_last >= subscription.rate_period;

    // Reject if no uses remaining (must renew to continue)
    if !is_new_period && subscription.uses_remaining == 0 {
        msg!("[subscription::update_usage_v1] ERROR: No uses remaining");
        return Err(ContractError::Custom(5).into())
    }

    // Calculate new usage values
    let (period_uses, uses_remaining, last_access_block) = if is_new_period {
        // Reset period counters
        (1u64, subscription.uses_allowed.saturating_sub(1), params.current_block)
    } else {
        // Increment within period
        (
            subscription.period_uses.saturating_add(1),
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
    Ok(update.encode())
}

/// UpdateUsageV1 apply - write updated usage to subscription
fn update_usage_apply_v1(cid: ContractId, update: UpdateUsageUpdateV1) -> ContractResult {
    let subs_db = wasm::db::db_lookup(cid, SUBSCRIPTION_CONTRACT_SUBSCRIPTIONS_TREE)?;

    // Get current subscription
    let sub_bytes = wasm::db::db_get(subs_db, &update.subscription_id.to_bytes())?;
    let mut subscription: Subscription = match sub_bytes {
        Some(data) => Subscription::decode(&data)?,
        None => {
            msg!("[subscription::update_usage_apply_v1] ERROR: Subscription not found");
            return Err(ContractError::Custom(1).into())
        }
    };

    // Update usage fields
    subscription.period_uses = update.period_uses;
    subscription.last_access_block = update.last_access_block;
    subscription.uses_remaining = update.uses_remaining;

    let sub_data = subscription.encode();
    wasm::db::db_set(
        subs_db,
        &update.subscription_id.to_bytes(),
        &sub_data,
    )?;

    msg!("[subscription::update_usage_apply_v1] Usage updated for: {:?}", update.subscription_id);
    Ok(())
}

/// DaoControlV1 instruction - execute DAO governance action
///
/// Money Integration: When executing `EndowmentWithdraw`, this function REQUIRES
/// a promissory_note::transfer_v1 child call to be bundled to transfer the endowment funds.
fn dao_control_v1(cid: ContractId, call_idx: usize, calls: Vec<dwow_sdk::dark_tree::DarkLeaf<ContractCall>>, params: DaoControlParamsV1) -> Result<Vec<u8>, ContractError> {
    msg!("[subscription::dao_control_v1] Executing DAO control action");

    // Validate children_indexes for EndowmentWithdraw
    if let DaoControlParamsV1::EndowmentWithdraw { amount, recipient } = params {
        // Validate children_indexes to ensure promissory_note::transfer_v1 is bundled
        let self_ = &calls[call_idx];
        if self_.children_indexes.len() != 1 {
            msg!(
                "[DaoControlV1] Error: EndowmentWithdraw requires 1 child call (promissory_note::transfer_v1), got {}",
                self_.children_indexes.len()
            );
            return Err(ContractError::Custom(1).into())
        }

        // Verify child call is promissory_note::transfer_v1 (function code 0x04)
        let child_idx = self_.children_indexes[0];
        let child_call = &calls[child_idx].data;
        if child_call.data[0] != 0x04 {
            msg!(
                "[DaoControlV1] Error: Expected promissory_note::transfer_v1 (0x04), got 0x{:02x}",
                child_call.data[0]
            );
            return Err(ContractError::Custom(2).into())
        }

        // Validate child call targets promissory_note (prevent cross-contract routing)
        let info_db = wasm::db::db_lookup(cid, SUBSCRIPTION_CONTRACT_INFO_TREE)?;
        let promissory_note_bytes = wasm::db::db_get(info_db, SUBSCRIPTION_CONTRACT_PROMISSORY_NOTE_CONTRACT_ID)?
            .ok_or(ContractError::Custom(2))?;
        let promissory_note_cid: ContractId = ContractId::from_bytes(
            promissory_note_bytes.try_into().map_err(|_| {
                ContractError::IoError("dao_control_v1: invalid ContractId bytes".into())
            })?,
        )?;
        // HAZOP H-11: fail-closed — reject if promissory_note not configured
        if promissory_note_cid == ContractId::ZERO {
            return Err(ContractError::IoError("promissory_note contract ID not configured".into()));
        }
        validate_child_contract_id(&child_call.contract_id, &promissory_note_cid)?;
        let value_blind = poseidon_hash([
                pallas::Base::from(amount),
                pallas::Base::from(amount),
            ]);
            validate_child_value_commit(&child_call.data, amount, value_blind)?;

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
    Ok(update.encode())
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
                Some(data) => Plan::decode(&data)?,
                None => {
                    msg!("[subscription::dao_control_apply_v1] ERROR: Plan not found");
                    return Err(ContractError::Custom(1).into())
                }
            };
            plan.active = active;
            let mut plan_data = vec![];
            dwow_serial::Encodable::encode(&plan, &mut plan_data).map_err(|e| ContractError::IoError(e.to_string()))?;
            wasm::db::db_set(plans_db, &plan_id.to_le_bytes(), &plan_data)?;
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
            let sub_bytes = wasm::db::db_get(subs_db, &subscription_id.to_bytes())?;
            let mut subscription: Subscription = match sub_bytes {
                Some(data) => Subscription::decode(&data)?,
                None => {
                    msg!("[subscription::dao_control_apply_v1] ERROR: Subscription not found");
                    return Err(ContractError::Custom(1).into())
                }
            };
            subscription.state = SubscriptionState::Cancelled;
            let sub_data = subscription.encode();
            wasm::db::db_set(subs_db, &subscription_id.to_bytes(), &sub_data)?;
            msg!("[subscription::dao_control_apply_v1] Subscription slashed: {:?}", subscription_id);
        }
    }

    Ok(())
}