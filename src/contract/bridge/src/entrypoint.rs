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

//! WASM entrypoint for the bridge contract
//!
//! Security Model: Object Capability Security
//!
//! Unlike VSS-based bridges that require threshold signatures for withdrawals,
//! this bridge uses deterministic address derivation. Users control their own
//! funds via secrets - the bridge never holds key shards.

use darkfi_sdk::{
    bridge::{BridgeCall, BridgeParameter},
    contract::ContractResult,
    error::ContractError,
    runtime::Runtime,
};

use crate::{error::BridgeError, model::*, BridgeFunction};

/// Initialize bridge contract state
pub fn bridge_init(_rt: &mut Runtime, _params: BridgeParameter) -> ContractResult<()> {
    // TODO: Initialize bridge state
    // - Set initial deposit tree (Merkle tree for deposits)
    // - Set nullifier tree (for spent nullifiers)
    // - Configure initial fee parameters
    // - Set minimum confirmation requirements
    Ok(())
}

/// Main contract entrypoint
pub fn bridge_exec(rt: &mut Runtime, params: BridgeParameter) -> ContractResult<()> {
    let call = BridgeCall::decode(params)?;

    let function = BridgeFunction::try_from(call.function)?;

    match function {
        BridgeFunction::InitializeV1 => bridge_init(rt, params),
        BridgeFunction::DepositV1 => bridge_deposit(rt, call),
        BridgeFunction::WithdrawV1 => bridge_withdraw(rt, call),
        BridgeFunction::UpdateConfigV1 => bridge_update_config(rt, call),
    }
}

/// Process a deposit from an external chain
///
/// Security: No VSS required. Deposit creates a commitment that
/// only the depositor can later claim via their secret.
///
/// Flow:
/// 1. Verify Merkle proof of deposit on external chain
/// 2. Verify deposit hasn't already been registered
/// 3. Compute bridge address from recipient identity + nonce
/// 4. Store deposit record with commitment
/// 5. Emit deposit event
fn bridge_deposit(_rt: &mut Runtime, _call: BridgeCall) -> ContractResult<()> {
    // TODO: Implement deposit logic
    //
    // 1. Parse DepositParams:
    //    - commitment: H(secret, amount, bridge_address)
    //    - external_block_hash: Block containing deposit
    //    - merkle_proof: Proof of deposit inclusion
    //    - recipient_pub: For address derivation
    //    - bridge_nonce: For fresh address
    //
    // 2. Verify Merkle proof against external chain state root
    //    - This ensures deposit actually exists on external chain
    //
    // 3. Derive bridge_address:
    //    - bridge_secret = poseidon_hash(recipient_pub_x, recipient_pub_y, nonce)
    //    - bridge_pub = bridge_secret * G
    //    - bridge_address = poseidon_hash(bridge_pub.x, bridge_pub.y)
    //
    // 4. Verify commitment matches:
    //    - commitment == poseidon_hash(secret, amount, bridge_address)
    //    (Note: This verification happens in ZK proof, not here)
    //
    // 5. Store deposit record:
    //    - Insert commitment into deposit Merkle tree
    //    - Record deposit info (amount, external_chain, etc.)
    //
    // 6. Emit Deposit event for indexing/oracles

    Err(ContractError::NotYetImplemented.into())
}

/// Process a withdrawal to an external chain
///
/// Security: No VSS/threshold required. User signs withdrawal
/// with their own secret. Bridge verifies ZK proof.
///
/// Flow:
/// 1. Verify ZK proof of withdrawal authorization
/// 2. Verify nullifier hasn't been spent
/// 3. Mark nullifier as spent
/// 4. Store withdrawal record
/// 5. Emit withdrawal event for relayer/external chain
fn bridge_withdraw(_rt: &mut Runtime, _call: BridgeCall) -> ContractResult<()> {
    // TODO: Implement withdrawal logic
    //
    // 1. Parse WithdrawParams:
    //    - nullifier: H(secret) - proves deposit ownership
    //    - proof: ZK proof of withdrawal authorization
    //    - recipient: Address on external chain
    //    - amount: Withdrawal amount
    //    - fee: Bridge fee
    //
    // 2. Verify ZK proof:
    //    - Proof demonstrates:
    //      a) Knowledge of secret corresponding to deposit
    //      b) Deposit exists in bridge's Merkle tree
    //      c) Amount is valid
    //      d) Recipient hash matches
    //
    // 3. Check nullifier not spent:
    //    - Query nullifier tree
    //    - If exists, reject (double-spend)
    //
    // 4. Mark nullifier spent:
    //    - Insert nullifier into spent nullifiers tree
    //
    // 5. Store withdrawal:
    //    - Record withdrawal info
    //    - Emit Withdraw event
    //
    // Note: No threshold signing needed!
    //       User alone authorizes via their secret.

    Err(ContractError::NotYetImplemented.into())
}

/// Update bridge configuration
///
/// Security: Only authorized callers (DAO) can update config.
/// This doesn't affect VSS - there is no VSS in this design.
fn bridge_update_config(_rt: &mut Runtime, _call: BridgeCall) -> ContractResult<()> {
    // TODO: Implement config update logic
    //
    // Authorized callers:
    // - DAO governance (via proposal/vote)
    // - Emergency multisig (for upgrades)
    //
    // Updateable parameters:
    // - Fee structure (deposit fee, withdrawal fee)
    // - Minimum confirmations
    // - Relayer configuration
    //
    // NOT updateable (immutable):
    // - Deposit verification logic
    // - Withdrawal authorization logic
    // - Object capability model

    Err(ContractError::NotYetImplemented.into())
}

// ================================================================
// SECURITY NOTES
// ================================================================
//
// Object Capability vs VSS:
//
// VSS-Based Bridge:
//   - Deposit: User → VSS nodes (secret shared)
//   - Withdraw: VSS threshold signing required
//   - Risk: Compromised VSS node = stolen funds
//
// OCap Bridge (this design):
//   - Deposit: User → commitment (no sharing)
//   - Withdraw: User self-signs (no VSS)
//   - Risk: None from node compromise (no shared secrets)
//
// The key insight: VSS is unnecessary because the bridge
// doesn't hold funds - it only verifies proofs. User's
// secret controls the deposit, not the bridge nodes.
//
// ================================================================
