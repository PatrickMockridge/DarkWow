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

//! AcceptSwapV1 entrypoint functions
//!
//! ## Security Model
//!
//! The prover MUST compute the nullifier externally before calling this function:
//! - nullifier = poseidon_hash([secret, lock_commitment])
//!
//! This nullifier is passed in AcceptSwapParams and is used for:
//! 1. ZK proof verification (public input)
//! 2. Double-spend prevention (stored in participants_db)

use dwow_sdk::{
    crypto::pasta_prelude::PrimeField,
    dark_tree::DarkLeaf,
    error::{ContractError, ContractResult},
    msg, ContractCall,
    pasta::pallas,
    wasm,
};
use dwow_serial::{Decodable, Encodable};

use crate::{
    error::DexError,
    model::{AcceptSwapParams, AcceptSwapUpdateV1, Swap, SwapState},
    DEX_CONTRACT_INFO_TREE, DEX_CONTRACT_PARTICIPANTS_TREE,
    DEX_CONTRACT_SWAPS_TREE,
    DEX_CONTRACT_ZKAS_ACCEPT_SWAP_NS_V2,
};

/// `get_metadata` function for `Dex::AcceptSwapV1`
///
/// Returns public inputs for ZK proof verification:
/// - computed_lock: from params (acceptor_lock_commitment)
/// - acceptor_nullifier: from params (prover-computed)
/// - signature_public_x: X coordinate of acceptor's signature public key
/// - signature_public_y: Y coordinate of acceptor's signature public key
///
/// The host uses these to verify the ZK proof.
pub(crate) fn dex_accept_swap_get_metadata_v1(
    _cid: dwow_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params= AcceptSwapParams::decode(&self_.data[1..])?;

    // Public inputs for the ZK proof verification
    // The order must match the `constrain_instance` calls in accept_swap_v1.zk:
    // 1. computed_lock (acceptor_lock_commitment)
    // 2. acceptor_nullifier
    // 3. signature_public_x
    // 4. signature_public_y
    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];

    // The prover computed the nullifier externally and passed it in params.
    // We use these directly as public inputs for ZK verification.
    let lock_commitment = params.lock_commitment.inner();
    let nullifier = params.nullifier.inner();

    // Extract signature public key coordinates
    let (sig_x, sig_y) = params.signature_public.xy().expect("pk not identity");

    zk_public_inputs.push((
        DEX_CONTRACT_ZKAS_ACCEPT_SWAP_NS_V2.to_string(),
        vec![lock_commitment, nullifier, sig_x, sig_y, params.tx_binding, params.tx_nonce],
    ));

    // Serialize metadata for ZK verification
    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;

    Ok(metadata)
}

/// `process_instruction` function for `Dex::AcceptSwapV1`
///
/// Verifies:
/// 1. Swap exists and is in Created state
/// 2. Swap has not expired
/// 3. Returns update with nullifier for storage
pub(crate) fn dex_accept_swap_process_instruction_v1(
    cid: dwow_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx];
    let params= AcceptSwapParams::decode(&self_.data.data[1..])?;

    msg!("[AcceptSwapV1] Accepting swap: id={:?}", &params.swap_id);

    // Load the swap
    let swaps_db = wasm::db::db_lookup(cid, DEX_CONTRACT_SWAPS_TREE)?;
    let swap_data = wasm::db::db_get(swaps_db, &params.swap_id)?;
    let swap: Swap = match swap_data {
        Some(data) => Swap::decode(&data)?,
        None => {
            msg!("[AcceptSwapV1] Error: Swap not found");
            return Err(DexError::SwapNotFound.into())
        }
    };

    // Verify swap is in Created state
    match swap.state {
        SwapState::Created => {}
        _ => {
            msg!("[AcceptSwapV1] Error: Swap not in Created state");
            return Err(DexError::InvalidSwapState.into())
        }
    }

    // Verify not expired
    let info_db = wasm::db::db_lookup(cid, DEX_CONTRACT_INFO_TREE)?;
    let current_time = get_current_timestamp(info_db)?;
    if current_time > swap.expires_at {
        msg!("[AcceptSwapV1] Error: Swap expired");
        return Err(DexError::SwapExpired.into())
    }

    // Extract acceptor's public key
    let (acceptor_pub_x, acceptor_pub_y) = params.signature_public.xy().expect("pk not identity");

    // Create the update struct with nullifier from params
    let update = AcceptSwapUpdateV1 {
        swap_id: params.swap_id,
        acceptor_pub_x: acceptor_pub_x.to_repr(),
        acceptor_pub_y: acceptor_pub_y.to_repr(),
        acceptor_lock: params.lock_commitment,
        acceptor_nullifier: params.nullifier,
    };

    Ok(update.encode())
}

/// `process_update` function for `Dex::AcceptSwapV1`
pub(crate) fn dex_accept_swap_process_update_v1(
    cid: dwow_sdk::crypto::ContractId,
    update: AcceptSwapUpdateV1,
) -> ContractResult {
    let swaps_db = wasm::db::db_lookup(cid, DEX_CONTRACT_SWAPS_TREE)?;
    let participants_db = wasm::db::db_lookup(cid, DEX_CONTRACT_PARTICIPANTS_TREE)?;

    // Load existing swap
    let swap_data = wasm::db::db_get(swaps_db, &update.swap_id)?;
    let mut swap: Swap = match swap_data {
        Some(data) => Swap::decode(&data)?,
        None => {
            msg!("[AcceptSwapV1] Error: Swap not found during update");
            return Err(DexError::SwapNotFound.into())
        }
    };

    // Update swap with acceptor's info and nullifier
    swap.acceptor_pub_x = update.acceptor_pub_x;
    swap.acceptor_pub_y = update.acceptor_pub_y;
    swap.acceptor_lock = Some(update.acceptor_lock);
    swap.acceptor_nullifier = Some(update.acceptor_nullifier);
    swap.state = SwapState::Accepted;

    // Store updated swap
    wasm::db::db_set(swaps_db, &update.swap_id, &swap.encode())?;

    // Store acceptor's nullifier using nullifier as key (not lock_commitment)
    wasm::db::db_mark_spent(participants_db, &update.acceptor_nullifier.to_bytes())?;

    msg!("[AcceptSwapV1] Swap accepted: id={:?}", &update.swap_id);

    Ok(())
}

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

/// Get current block timestamp from info_db
fn get_current_timestamp(info_db: u32) -> Result<u64, ContractError> {
    let data = wasm::db::db_get(info_db, b"current_timestamp")?;
    match data {
        Some(d) => {
            let mut cursor = std::io::Cursor::new(&d);
            u64::decode(&mut cursor).map_err(|_| ContractError::IoError("decode error".to_string()))
        }
        None => Ok(0),
    }
}