/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by the
 * Free Software Foundation, either version 3 of the License, or (at your
 * option) any later version.
 *
 * This program is distributed in the hope that it will be useful, but WITHOUT
 * ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
 * FOR A PARTICULAR PURPOSE.  See the GNU General Public License for more
 * details.
 *
 * You should have received a copy of the GNU General Public License along with
 * this program; if not, see <https://www.gnu.org/licenses/>.
 */

//! Verification keys for native contracts.
//!
//! This module provides cached proving keys and verifying keys for the native
//! contracts (NativeToken, Deployooor) that are deployed at genesis.
//!
//! The .zk.bin files contain ZkBinary encoded circuits. These are loaded at
//! compile time via include_bytes!() and used to build ProvingKey and VerifyingKey
//! structures at runtime.

use darkfi_sdk::crypto::NATIVE_TOKEN_CONTRACT_ID;

use crate::{
    blockchain::BlockchainOverlayPtr,
    error::Result,
    zk::{empty_witnesses, ProvingKey, VerifyingKey, ZkCircuit},
    zkas::ZkBinary,
};
use darkfi_serial::serialize;

/// Represents a VK entry for injection into the blockchain overlay.
/// Format: (zkbin_bytes, namespace, vk_bytes)
pub type VkEntry = (Vec<u8>, String, Vec<u8>);

/// Helper to process a zkbin file and create PK/VK entry
fn process_zkbin(zkbin_bytes: Vec<u8>) -> Result<(ProvingKey, VkEntry)> {
    let zkbin = ZkBinary::decode(&zkbin_bytes, false)?;
    let circuit = ZkCircuit::new(empty_witnesses(&zkbin)?, &zkbin);
    let pk = ProvingKey::build(zkbin.k, &circuit);
    let vk = VerifyingKey::build(zkbin.k, &circuit);

    let mut vk_buf = Vec::new();
    vk.write(&mut vk_buf)?;

    let entry = (zkbin_bytes, zkbin.namespace.clone(), vk_buf);

    Ok((pk, entry))
}

/// Load cached proving keys and verifying keys for native contracts.
///
/// Returns:
/// - ProvingKeys for proof generation in tests
/// - VK entries (zkbin_bytes, namespace, vk_bytes) for injection
pub fn get_cached_pks_and_vks() -> Result<(Vec<ProvingKey>, Vec<VkEntry>)> {
    let mut pks = Vec::new();
    let mut vks = Vec::new();

    // NativeToken contract circuits (Mint_V1, Burn_V1, Fee_V1)
    let (pk, entry) =
        process_zkbin(include_bytes!("../../src/contract/native_token/proof/mint_v1.zk.bin").to_vec())?;
    pks.push(pk);
    vks.push(entry);

    let (pk, entry) =
        process_zkbin(include_bytes!("../../src/contract/native_token/proof/fee_v1.zk.bin").to_vec())?;
    pks.push(pk);
    vks.push(entry);

    let (pk, entry) =
        process_zkbin(include_bytes!("../../src/contract/native_token/proof/burn_v1.zk.bin").to_vec())?;
    pks.push(pk);
    vks.push(entry);

    // Deployooor has no ZK circuits - it's a pure WASM contract
    // So we don't add anything for it here

    Ok((pks, vks))
}

/// Inject verifying keys into the blockchain overlay.
///
/// This stores (zkbin_bytes, vk_bytes) in the sled tree for each VK entry.
/// The keys are used by the WASM runtime to verify proofs.
pub fn inject(overlay: &BlockchainOverlayPtr, vks: &[VkEntry]) -> Result<()> {
    // Grab a lock over the blockchain overlay
    let lock = overlay.lock().unwrap();
    let mut overlay = lock.overlay.lock().unwrap();

    // Derive the database name for NativeToken contract's zkas tree
    let native_token_db_name = NATIVE_TOKEN_CONTRACT_ID.hash_state_id("_zkas");

    // Ensure the tree is open in the overlay
    overlay.open_tree(&native_token_db_name, false)?;

    for (zkbin_bytes, namespace, vk_bytes) in vks.iter() {
        let key = serialize(namespace);
        let value = serialize(&(zkbin_bytes.clone(), vk_bytes.clone()));
        overlay.insert(&native_token_db_name, &key, &value)?;
    }

    Ok(())
}