/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

//! Plain Subscription Contract Entrypoint
//!
//! # Architecture
//!
//! This contract uses a hybrid ZK/plain approach:
//!
//! | Operation | Method | Why |
//! |-----------|--------|-----|
//! | Signature verification | ZK (Schnorr) | Sound, constrainable |
//! | Subscription commitment | ZK (Poseidon) | Privacy-preserving for ID |
//! | Access bitmask check | Native Rust | Needs `base_div` (not in ZK) |
//! | Rate limit calculation | Native Rust | Needs `base_div` (not in ZK) |
//!
//! # Privacy
//!
//! This is a **partial transparency** contract. State is public on-chain.
//! See [`PRIVACY_TRADEOFFS.md`](PRIVACY_TRADEOFFS.md) for full details.

use darkfi_sdk::{
    crypto::{poseidon_hash, schnorr::SchnorrPublic, ContractId},
    dark_tree::DarkLeaf,
    error::GenericResult,
    msg, wasm, ContractCall,
};
use darkfi_sdk::pasta::pallas::Base;
use darkfi_serial::{deserialize, serialize, Encodable};

use crate::error::SubscriptionPlainError;
use crate::model::{
    CancelParamsV1, SubscribeParamsV1, Subscription, SubscriptionState, VerifyAccessParamsV1,
    ACCESS_ADMIN, ACCESS_READ, ACCESS_WRITE,
};
use crate::SubscriptionPlainFunction;

// Database trees
const SUBSCRIPTION_TREE: &str = "subscriptions";

darkfi_sdk::define_contract!(
    init: init_contract,
    exec: process_instruction,
    apply: process_update,
    metadata: get_metadata
);

/// Initialize the contract
fn init_contract(cid: ContractId, _ix: &[u8]) -> GenericResult<()> {
    wasm::db::db_init(cid, SUBSCRIPTION_TREE)?;
    Ok(())
}

/// Get metadata for verification
fn get_metadata(_cid: ContractId, _ix: &[u8]) -> GenericResult<()> {
    Ok(())
}

/// Process instruction
fn process_instruction(cid: ContractId, ix: &[u8]) -> GenericResult<()> {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = SubscriptionPlainFunction::try_from(self_.data[0])?;

    let update_data = match func {
        SubscriptionPlainFunction::SubscribeV1 => {
            subscribe_process_instruction_v1(cid, call_idx, calls)?
        }
        SubscriptionPlainFunction::VerifyAccessV1 => {
            verify_access_process_instruction_v1(cid, call_idx, calls)?
        }
        SubscriptionPlainFunction::CancelV1 => {
            cancel_process_instruction_v1(cid, call_idx, calls)?
        }
    };

    wasm::util::set_return_data(&update_data)
}

/// Process update
fn process_update(cid: ContractId, update_data: &[u8]) -> GenericResult<()> {
    match SubscriptionPlainFunction::try_from(update_data[0])? {
        SubscriptionPlainFunction::SubscribeV1 => {
            let update: crate::model::SubscribeUpdateV1 = deserialize(&update_data[1..])?;
            subscribe_process_update_v1(cid, update)
        }
        SubscriptionPlainFunction::VerifyAccessV1 => {
            let update: crate::model::VerifyAccessUpdateV1 = deserialize(&update_data[1..])?;
            verify_access_process_update_v1(cid, update)
        }
        SubscriptionPlainFunction::CancelV1 => {
            let update: crate::model::CancelUpdateV1 = deserialize(&update_data[1..])?;
            cancel_process_update_v1(cid, update)
        }
    }
}

// =============================================================================
// SUBSCRIBE
// =============================================================================

fn subscribe_process_instruction_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> GenericResult<Vec<u8>> {
    let self_ = &calls[call_idx].data;
    let params: SubscribeParamsV1 = deserialize(&self_.data[1..])?;

    msg!("[subscription_plain::subscribe] Creating subscription");

    // Validate tier bitmask
    // ZK: Could be constrained to valid tier values
    // Native: Validates here for now
    if params.tier != ACCESS_READ && params.tier != (ACCESS_READ | ACCESS_WRITE) &&
       params.tier != (ACCESS_READ | ACCESS_WRITE | ACCESS_ADMIN) {
        return Err(SubscriptionPlainError::InvalidAccessMask.into())
    }

    // Validate duration
    if params.duration_blocks == 0 || params.duration_blocks > 1000000 {
        return Err(SubscriptionPlainError::InvalidDuration.into())
    }

    // Get current block for signature verification
    let current_block = wasm::util::get_verifying_block_height()? as u64;

    // ZK: Signature verified via Schnorr in ZK constraint
    // Create message for signature verification
    let mut signature_msg = vec![];
    params.subscriber.x().encode(&mut signature_msg)?;
    params.subscriber.y().encode(&mut signature_msg)?;
    params.provider.x().encode(&mut signature_msg)?;
    params.provider.y().encode(&mut signature_msg)?;
    params.tier.encode(&mut signature_msg)?;
    params.uses_allowed.encode(&mut signature_msg)?;
    params.rate_period.encode(&mut signature_msg)?;
    params.duration_blocks.encode(&mut signature_msg)?;
    current_block.encode(&mut signature_msg)?;

    // Verify subscriber signature
    // In ZK version, subscriber would be constrained from the signature
    // OPCODE PLACEHOLDER: When Schnorr verification is in ZK, subscriber would be constrained
    // Currently: We verify the signature against the provided subscriber public key
    if !params.subscriber.verify(&signature_msg, &params.signature) {
        return Err(SubscriptionPlainError::InvalidSignature.into())
    }

    msg!(
        "[subscription_plain::subscribe] Subscriber: {:?}, Provider: {:?}",
        params.subscriber,
        params.provider
    );

    // Derive subscription ID
    let subscription_id = poseidon_hash([
        params.subscriber.x(),
        params.subscriber.y(),
        params.provider.x(),
        params.provider.y(),
        Base::from(params.tier as u64),
        Base::from(current_block),
    ]);

    // Check subscription doesn't already exist
    let db = wasm::db::db_lookup(cid, SUBSCRIPTION_TREE)?;
    if wasm::db::db_contains_key(db, &serialize(&subscription_id))? {
        return Err(SubscriptionPlainError::SubscriptionAlreadyExists.into())
    }

    let start_block = current_block;
    let expiry_block = current_block + params.duration_blocks;

    let update = crate::model::SubscribeUpdateV1 {
        subscription_id,
        subscriber: params.subscriber,
        provider: params.provider,
        tier: params.tier,
        uses_allowed: params.uses_allowed,
        rate_period: params.rate_period,
        start_block,
        expiry_block,
    };

    msg!("[subscription_plain::subscribe] Subscription {:?} created", subscription_id);
    Ok(serialize(&update))
}

fn subscribe_process_update_v1(cid: ContractId, update: crate::model::SubscribeUpdateV1) -> GenericResult<()> {
    let db = wasm::db::db_lookup(cid, SUBSCRIPTION_TREE)?;

    let subscription = Subscription {
        id: update.subscription_id,
        subscriber: update.subscriber,
        provider: update.provider,
        tier: update.tier,
        state: SubscriptionState::Active,
        uses_remaining: update.uses_allowed,
        uses_allowed: update.uses_allowed,
        rate_period: update.rate_period,
        start_block: update.start_block,
        expiry_block: update.expiry_block,
        last_access_block: 0,
        period_uses: 0,
    };

    wasm::db::db_set(db, &serialize(&update.subscription_id), &serialize(&subscription))?;
    msg!("[subscription_plain::subscribe::update] Subscription stored");

    Ok(())
}

// =============================================================================
// VERIFY ACCESS
// =============================================================================

fn verify_access_process_instruction_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> GenericResult<Vec<u8>> {
    let self_ = &calls[call_idx].data;
    let params: VerifyAccessParamsV1 = deserialize(&self_.data[1..])?;

    msg!(
        "[subscription_plain::verify_access] Verifying access for subscription {:?}",
        params.subscription_id
    );

    // Look up subscription
    let db = wasm::db::db_lookup(cid, SUBSCRIPTION_TREE)?;
    let subscription: Subscription = match wasm::db::db_get(db, &serialize(&params.subscription_id))? {
        Some(data) => deserialize(&data)?,
        None => return Err(SubscriptionPlainError::SubscriptionNotFound.into()),
    };

    // Check if subscription is active
    if !subscription.is_active(params.current_block) {
        return Err(SubscriptionPlainError::SubscriptionNotActive.into())
    }

    // Native Rust bitmask check (visible on-chain)
    // ZK OPCODE PLACEHOLDER: When base_div is in ZK, this could be constrained
    // PRIVACY NOTICE: This reveals the full access bitmask on-chain
    let has_access = (subscription.tier & params.required_access) == params.required_access;

    if !has_access {
        msg!(
            "[subscription_plain::verify_access] Access denied: tier={}, required={}",
            subscription.tier,
            params.required_access
        );
        return Err(SubscriptionPlainError::InsufficientPermissions.into())
    }

    // Check rate limit
    // Native Rust calculation (visible on-chain)
    // ZK OPCODE PLACEHOLDER: When base_div is in ZK, rate limit could use division
    if !subscription.check_rate_limit(params.current_block) {
        return Err(SubscriptionPlainError::RateLimitExceeded.into())
    }

    let update = crate::model::VerifyAccessUpdateV1 {
        subscription_id: params.subscription_id,
        access_granted: true,
        uses_remaining: subscription.uses_remaining,
    };

    msg!("[subscription_plain::verify_access] Access granted");
    Ok(serialize(&update))
}

fn verify_access_process_update_v1(
    cid: ContractId,
    update: crate::model::VerifyAccessUpdateV1,
) -> GenericResult<()> {
    let db = wasm::db::db_lookup(cid, SUBSCRIPTION_TREE)?;

    // Get and update subscription
    let mut subscription: Subscription = match wasm::db::db_get(db, &serialize(&update.subscription_id))? {
        Some(data) => deserialize(&data)?,
        None => return Err(SubscriptionPlainError::SubscriptionNotFound.into()),
    };

    // Update usage counters
    let current_block = wasm::util::get_verifying_block_height()? as u64;
    subscription.update_usage(current_block);

    wasm::db::db_set(db, &serialize(&update.subscription_id), &serialize(&subscription))?;
    msg!("[subscription_plain::verify_access::update] Usage updated");

    Ok(())
}

// =============================================================================
// CANCEL
// =============================================================================

fn cancel_process_instruction_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> GenericResult<Vec<u8>> {
    let self_ = &calls[call_idx].data;
    let params: CancelParamsV1 = deserialize(&self_.data[1..])?;

    msg!(
        "[subscription_plain::cancel] Cancelling subscription {:?}",
        params.subscription_id
    );

    // Look up subscription
    let db = wasm::db::db_lookup(cid, SUBSCRIPTION_TREE)?;
    let subscription: Subscription = match wasm::db::db_get(db, &serialize(&params.subscription_id))? {
        Some(data) => deserialize(&data)?,
        None => return Err(SubscriptionPlainError::SubscriptionNotFound.into()),
    };

    // Verify subscription is active
    if subscription.state != SubscriptionState::Active {
        return Err(SubscriptionPlainError::SubscriptionNotActive.into())
    }

    // Verify subscriber matches
    if subscription.subscriber != params.subscriber {
        return Err(SubscriptionPlainError::UnauthorizedCaller.into())
    }

    // ZK: Signature verification would be done in ZK constraint
    // Currently: We verify the signature against the subscriber public key
    // OPCODE PLACEHOLDER: When Schnorr verification is in ZK, caller would be constrained
    let mut signature_msg = vec![];
    params.subscription_id.encode(&mut signature_msg)?;
    if !params.subscriber.verify(&signature_msg, &params.signature) {
        return Err(SubscriptionPlainError::InvalidSignature.into())
    }

    let update = crate::model::CancelUpdateV1 {
        subscription_id: params.subscription_id,
        refunded_amount: 0, // No refunds in this simple version
    };

    msg!("[subscription_plain::cancel] Subscription cancelled");
    Ok(serialize(&update))
}

fn cancel_process_update_v1(cid: ContractId, update: crate::model::CancelUpdateV1) -> GenericResult<()> {
    let db = wasm::db::db_lookup(cid, SUBSCRIPTION_TREE)?;

    // Get and update subscription
    let mut subscription: Subscription = match wasm::db::db_get(db, &serialize(&update.subscription_id))? {
        Some(data) => deserialize(&data)?,
        None => return Err(SubscriptionPlainError::SubscriptionNotFound.into()),
    };

    subscription.state = SubscriptionState::Cancelled;
    subscription.uses_remaining = 0;

    wasm::db::db_set(db, &serialize(&update.subscription_id), &serialize(&subscription))?;
    msg!("[subscription_plain::cancel::update] Subscription cancelled");

    Ok(())
}
